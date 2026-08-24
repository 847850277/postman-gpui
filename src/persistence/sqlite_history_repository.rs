use super::{
    HistoryLoadResult, HistoryLoadWarning, HistoryRepository, HistoryRepositoryError,
    HistoryRepositoryOperation, HistorySnapshotError, VersionedHistorySnapshot,
};
use chrono::DateTime;
use directories::BaseDirs;
use rusqlite::{
    params, Connection, Error as SqliteError, ErrorCode, OpenFlags, OptionalExtension,
    TransactionBehavior,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub const CURRENT_HISTORY_SCHEMA_VERSION: u32 = 2;
pub const DEFAULT_HISTORY_RETENTION_LIMIT: usize = 50;

const APPLICATION_DATA_DIRECTORY: &str = "postman-gpui";
const HISTORY_DATABASE_FILE: &str = "request-history.sqlite3";
const DEFAULT_BUSY_TIMEOUT: Duration = Duration::from_secs(1);
const MIN_BUSY_TIMEOUT: Duration = Duration::from_millis(1);
const MAX_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

const MIGRATION_0_TO_1: &str = r#"
CREATE TABLE history_entries (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    entry_id TEXT NOT NULL UNIQUE,
    created_at_ms INTEGER NOT NULL,
    snapshot_json BLOB NOT NULL
) STRICT;

CREATE INDEX history_entries_recent_idx
    ON history_entries(created_at_ms DESC, sequence DESC);
"#;

const MIGRATION_1_TO_2: &str = r#"
ALTER TABLE history_entries
    ADD COLUMN snapshot_version INTEGER NOT NULL DEFAULT 1;
"#;

const VERIFY_CURRENT_SCHEMA: &str = r#"
SELECT sequence, entry_id, created_at_ms, snapshot_version, snapshot_json
FROM history_entries
WHERE 0
"#;

const LOAD_RECENT: &str = r#"
SELECT entry_id, created_at_ms, snapshot_version, snapshot_json
FROM history_entries
ORDER BY created_at_ms DESC, sequence DESC
"#;

const INSERT_SNAPSHOT: &str = r#"
INSERT INTO history_entries (
    entry_id,
    created_at_ms,
    snapshot_json,
    snapshot_version
) VALUES (?1, ?2, ?3, ?4)
ON CONFLICT(entry_id) DO NOTHING
"#;

/// SQLite adapter containing every SQL statement used by durable History.
///
/// The adapter stores only `VersionedHistorySnapshot`, opens a fresh connection for each method,
/// and is intended to be owned by `HistoryRepositoryWorker`.
#[derive(Debug, Clone)]
pub struct SqliteHistoryRepository {
    path: PathBuf,
    busy_timeout: Duration,
    open_flags: OpenFlags,
}

impl SqliteHistoryRepository {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, HistoryRepositoryError> {
        let path = path.into();
        validate_database_path(&path)?;
        Ok(Self {
            path,
            busy_timeout: DEFAULT_BUSY_TIMEOUT,
            open_flags: OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
        })
    }

    pub fn production() -> Result<Self, HistoryRepositoryError> {
        Self::new(production_history_database_path()?)
    }

    pub fn with_busy_timeout(mut self, timeout: Duration) -> Self {
        self.busy_timeout = timeout.clamp(MIN_BUSY_TIMEOUT, MAX_BUSY_TIMEOUT);
        self
    }

    pub fn database_path(&self) -> &Path {
        &self.path
    }

    pub fn busy_timeout(&self) -> Duration {
        self.busy_timeout
    }

    fn ensure_parent_directory(&self) -> Result<(), HistoryRepositoryError> {
        let parent =
            self.path
                .parent()
                .ok_or_else(|| HistoryRepositoryError::InvalidDatabasePath {
                    path: self.path.clone(),
                    reason: "must have a parent directory",
                })?;
        fs::create_dir_all(parent).map_err(|error| HistoryRepositoryError::Io {
            operation: HistoryRepositoryOperation::Initialize,
            path: parent.to_path_buf(),
            message: error.to_string(),
        })
    }

    fn open_connection(
        &self,
        operation: HistoryRepositoryOperation,
    ) -> Result<Connection, HistoryRepositoryError> {
        let connection = Connection::open_with_flags(&self.path, self.open_flags)
            .map_err(|error| map_sqlite_error(operation, &self.path, error))?;
        connection
            .busy_timeout(self.busy_timeout)
            .map_err(|error| map_sqlite_error(operation, &self.path, error))?;
        connection
            .pragma_update(None, "foreign_keys", true)
            .map_err(|error| map_sqlite_error(operation, &self.path, error))?;
        Ok(connection)
    }

    fn open_current_schema(
        &self,
        operation: HistoryRepositoryOperation,
    ) -> Result<Connection, HistoryRepositoryError> {
        let connection = self.open_connection(operation)?;
        let version = schema_version(&connection, operation, &self.path)?;
        if version > CURRENT_HISTORY_SCHEMA_VERSION {
            return Err(HistoryRepositoryError::UnsupportedSchemaVersion {
                found: version,
                supported: CURRENT_HISTORY_SCHEMA_VERSION,
            });
        }
        if version != CURRENT_HISTORY_SCHEMA_VERSION {
            return Err(HistoryRepositoryError::NotInitialized {
                found: version,
                expected: CURRENT_HISTORY_SCHEMA_VERSION,
            });
        }
        Ok(connection)
    }

    fn enable_wal(&self, connection: &Connection) -> Result<(), HistoryRepositoryError> {
        let journal_mode: String = connection
            .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
            .map_err(|error| {
                map_sqlite_error(HistoryRepositoryOperation::Initialize, &self.path, error)
            })?;
        if !journal_mode.eq_ignore_ascii_case("wal") {
            return Err(HistoryRepositoryError::Database {
                operation: HistoryRepositoryOperation::Initialize,
                message: format!("SQLite refused WAL journal mode and returned {journal_mode}"),
            });
        }
        Ok(())
    }

    fn migrate(
        &self,
        connection: &mut Connection,
        starting_version: u32,
    ) -> Result<(), HistoryRepositoryError> {
        if starting_version == CURRENT_HISTORY_SCHEMA_VERSION {
            return Ok(());
        }
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| {
                map_migration_error(
                    &self.path,
                    starting_version,
                    starting_version.saturating_add(1),
                    error,
                )
            })?;
        let mut version = starting_version;

        if version < 1 {
            transaction
                .execute_batch(MIGRATION_0_TO_1)
                .map_err(|error| map_migration_error(&self.path, 0, 1, error))?;
            transaction
                .pragma_update(None, "user_version", 1_u32)
                .map_err(|error| map_migration_error(&self.path, 0, 1, error))?;
            version = 1;
        }
        if version < 2 {
            transaction
                .execute_batch(MIGRATION_1_TO_2)
                .map_err(|error| map_migration_error(&self.path, 1, 2, error))?;
            transaction
                .pragma_update(None, "user_version", 2_u32)
                .map_err(|error| map_migration_error(&self.path, 1, 2, error))?;
        }

        transaction.commit().map_err(|error| {
            map_migration_error(
                &self.path,
                starting_version,
                CURRENT_HISTORY_SCHEMA_VERSION,
                error,
            )
        })
    }

    fn verify_current_schema(&self, connection: &Connection) -> Result<(), HistoryRepositoryError> {
        let quick_check: String = connection
            .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
            .map_err(|error| {
                map_sqlite_error(HistoryRepositoryOperation::Initialize, &self.path, error)
            })?;
        if quick_check != "ok" {
            return Err(HistoryRepositoryError::CorruptDatabase {
                operation: HistoryRepositoryOperation::Initialize,
                message: quick_check,
            });
        }
        connection.prepare(VERIFY_CURRENT_SCHEMA).map_err(|error| {
            map_migration_error(
                &self.path,
                CURRENT_HISTORY_SCHEMA_VERSION,
                CURRENT_HISTORY_SCHEMA_VERSION,
                error,
            )
        })?;
        Ok(())
    }

    #[cfg(test)]
    fn read_only_for_test(&self) -> Self {
        Self {
            path: self.path.clone(),
            busy_timeout: self.busy_timeout,
            open_flags: OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
        }
    }
}

impl HistoryRepository for SqliteHistoryRepository {
    fn initialize(&mut self) -> Result<(), HistoryRepositoryError> {
        self.ensure_parent_directory()?;
        let mut connection = self.open_connection(HistoryRepositoryOperation::Initialize)?;
        let version = schema_version(
            &connection,
            HistoryRepositoryOperation::Initialize,
            &self.path,
        )?;
        if version > CURRENT_HISTORY_SCHEMA_VERSION {
            return Err(HistoryRepositoryError::UnsupportedSchemaVersion {
                found: version,
                supported: CURRENT_HISTORY_SCHEMA_VERSION,
            });
        }
        self.enable_wal(&connection)?;
        self.migrate(&mut connection, version)?;
        self.verify_current_schema(&connection)
    }

    fn load_recent(&mut self, limit: usize) -> Result<HistoryLoadResult, HistoryRepositoryError> {
        let connection = self.open_current_schema(HistoryRepositoryOperation::Load)?;
        if limit == 0 {
            return Ok(HistoryLoadResult::default());
        }

        let mut statement = connection.prepare(LOAD_RECENT).map_err(|error| {
            map_sqlite_error(HistoryRepositoryOperation::Load, &self.path, error)
        })?;
        let mut rows = statement.query([]).map_err(|error| {
            map_sqlite_error(HistoryRepositoryOperation::Load, &self.path, error)
        })?;
        let mut snapshots = Vec::with_capacity(limit.min(DEFAULT_HISTORY_RETENTION_LIMIT));
        let mut warnings = Vec::new();

        while snapshots.len() < limit {
            let Some(row) = rows.next().map_err(|error| {
                map_sqlite_error(HistoryRepositoryOperation::Load, &self.path, error)
            })?
            else {
                break;
            };
            let entry_id: String = row.get(0).map_err(|error| {
                map_sqlite_error(HistoryRepositoryOperation::Load, &self.path, error)
            })?;
            let created_at_ms: i64 = row.get(1).map_err(|error| {
                map_sqlite_error(HistoryRepositoryOperation::Load, &self.path, error)
            })?;
            let stored_version: i64 = row.get(2).map_err(|error| {
                map_sqlite_error(HistoryRepositoryOperation::Load, &self.path, error)
            })?;
            let payload: Vec<u8> = row.get(3).map_err(|error| {
                map_sqlite_error(HistoryRepositoryOperation::Load, &self.path, error)
            })?;

            let snapshot = match VersionedHistorySnapshot::from_json_bytes(&payload) {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    warnings.push(HistoryLoadWarning::snapshot_decode(entry_id, error));
                    continue;
                }
            };
            if snapshot.as_v1().entry_id() != entry_id {
                let stored_entry_id = entry_id.clone();
                warnings.push(HistoryLoadWarning::metadata_mismatch(
                    entry_id,
                    "entry_id",
                    snapshot.as_v1().entry_id(),
                    stored_entry_id,
                ));
                continue;
            }
            let Ok(stored_version) = u64::try_from(stored_version) else {
                warnings.push(HistoryLoadWarning::metadata_mismatch(
                    entry_id,
                    "snapshot_version",
                    snapshot.version().to_string(),
                    "negative value",
                ));
                continue;
            };
            if snapshot.version() != stored_version {
                warnings.push(HistoryLoadWarning::metadata_mismatch(
                    entry_id,
                    "snapshot_version",
                    snapshot.version().to_string(),
                    stored_version.to_string(),
                ));
                continue;
            }
            let snapshot_created_at_ms = snapshot_created_at_ms(&snapshot).map_err(|source| {
                HistoryRepositoryError::Snapshot {
                    operation: HistoryRepositoryOperation::Load,
                    source,
                }
            })?;
            if snapshot_created_at_ms != created_at_ms {
                warnings.push(HistoryLoadWarning::metadata_mismatch(
                    entry_id,
                    "created_at_ms",
                    snapshot_created_at_ms.to_string(),
                    created_at_ms.to_string(),
                ));
                continue;
            }
            snapshots.push(snapshot);
        }

        Ok(HistoryLoadResult::new(snapshots, warnings))
    }

    fn append_and_trim(
        &mut self,
        snapshot: &VersionedHistorySnapshot,
        limit: usize,
    ) -> Result<(), HistoryRepositoryError> {
        let limit =
            i64::try_from(limit).map_err(|_| HistoryRepositoryError::LimitTooLarge { limit })?;
        let payload =
            snapshot
                .to_json_bytes()
                .map_err(|source| HistoryRepositoryError::Snapshot {
                    operation: HistoryRepositoryOperation::Append,
                    source,
                })?;
        let snapshot_version =
            i64::try_from(snapshot.version()).map_err(|_| HistoryRepositoryError::Snapshot {
                operation: HistoryRepositoryOperation::Append,
                source: HistorySnapshotError::NumericOverflow {
                    field: "snapshot.version",
                },
            })?;
        let created_at_ms = snapshot_created_at_ms(snapshot).map_err(|source| {
            HistoryRepositoryError::Snapshot {
                operation: HistoryRepositoryOperation::Append,
                source,
            }
        })?;
        let entry_id = snapshot.as_v1().entry_id();

        let mut connection = self.open_current_schema(HistoryRepositoryOperation::Append)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| {
                map_sqlite_error(HistoryRepositoryOperation::Append, &self.path, error)
            })?;
        let inserted = transaction
            .execute(
                INSERT_SNAPSHOT,
                params![entry_id, created_at_ms, &payload, snapshot_version],
            )
            .map_err(|error| {
                map_sqlite_error(HistoryRepositoryOperation::Append, &self.path, error)
            })?;
        if inserted == 0 {
            let existing: Option<Vec<u8>> = transaction
                .query_row(
                    "SELECT snapshot_json FROM history_entries WHERE entry_id = ?1",
                    [entry_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| {
                    map_sqlite_error(HistoryRepositoryOperation::Append, &self.path, error)
                })?;
            if existing.as_deref() != Some(payload.as_slice()) {
                return Err(HistoryRepositoryError::EntryIdConflict {
                    entry_id: entry_id.to_string(),
                });
            }
        }

        if limit == 0 {
            transaction
                .execute("DELETE FROM history_entries", [])
                .map_err(|error| {
                    map_sqlite_error(HistoryRepositoryOperation::Append, &self.path, error)
                })?;
        } else {
            transaction
                .execute(
                    r#"
DELETE FROM history_entries
WHERE sequence NOT IN (
    SELECT sequence
    FROM history_entries
    ORDER BY created_at_ms DESC, sequence DESC
    LIMIT ?1
)
"#,
                    [limit],
                )
                .map_err(|error| {
                    map_sqlite_error(HistoryRepositoryOperation::Append, &self.path, error)
                })?;
        }
        transaction.commit().map_err(|error| {
            map_sqlite_error(HistoryRepositoryOperation::Append, &self.path, error)
        })
    }

    fn clear(&mut self) -> Result<(), HistoryRepositoryError> {
        let mut connection = self.open_current_schema(HistoryRepositoryOperation::Clear)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| {
                map_sqlite_error(HistoryRepositoryOperation::Clear, &self.path, error)
            })?;
        transaction
            .execute("DELETE FROM history_entries", [])
            .map_err(|error| {
                map_sqlite_error(HistoryRepositoryOperation::Clear, &self.path, error)
            })?;
        transaction
            .commit()
            .map_err(|error| map_sqlite_error(HistoryRepositoryOperation::Clear, &self.path, error))
    }
}

pub fn production_history_database_path() -> Result<PathBuf, HistoryRepositoryError> {
    let base_directories =
        BaseDirs::new().ok_or(HistoryRepositoryError::ApplicationDataDirectoryUnavailable)?;
    Ok(base_directories
        .data_local_dir()
        .join(APPLICATION_DATA_DIRECTORY)
        .join(HISTORY_DATABASE_FILE))
}

fn validate_database_path(path: &Path) -> Result<(), HistoryRepositoryError> {
    if !path.is_absolute() {
        return Err(HistoryRepositoryError::InvalidDatabasePath {
            path: path.to_path_buf(),
            reason: "must be absolute so storage never falls back to the current directory",
        });
    }
    if path.file_name().is_none() {
        return Err(HistoryRepositoryError::InvalidDatabasePath {
            path: path.to_path_buf(),
            reason: "must identify a database file",
        });
    }
    Ok(())
}

fn schema_version(
    connection: &Connection,
    operation: HistoryRepositoryOperation,
    path: &Path,
) -> Result<u32, HistoryRepositoryError> {
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| map_sqlite_error(operation, path, error))?;
    u32::try_from(version).map_err(|_| HistoryRepositoryError::Database {
        operation,
        message: format!("invalid negative schema version {version}"),
    })
}

fn snapshot_created_at_ms(
    snapshot: &VersionedHistorySnapshot,
) -> Result<i64, HistorySnapshotError> {
    DateTime::parse_from_rfc3339(snapshot.as_v1().timestamp())
        .map(|timestamp| timestamp.timestamp_millis())
        .map_err(|_| HistorySnapshotError::InvalidField {
            field: "timestamp",
            reason: "must be RFC 3339",
        })
}

fn map_migration_error(
    path: &Path,
    from: u32,
    to: u32,
    error: SqliteError,
) -> HistoryRepositoryError {
    match map_sqlite_error(HistoryRepositoryOperation::Initialize, path, error) {
        error @ (HistoryRepositoryError::Busy { .. }
        | HistoryRepositoryError::CorruptDatabase { .. }
        | HistoryRepositoryError::Io { .. }) => error,
        other => HistoryRepositoryError::Migration {
            from,
            to,
            message: other.to_string(),
        },
    }
}

fn map_sqlite_error(
    operation: HistoryRepositoryOperation,
    path: &Path,
    error: SqliteError,
) -> HistoryRepositoryError {
    match error.sqlite_error_code() {
        Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => {
            HistoryRepositoryError::Busy { operation }
        }
        Some(ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase) => {
            HistoryRepositoryError::CorruptDatabase {
                operation,
                message: error.to_string(),
            }
        }
        Some(
            ErrorCode::PermissionDenied
            | ErrorCode::ReadOnly
            | ErrorCode::SystemIoFailure
            | ErrorCode::DiskFull
            | ErrorCode::CannotOpen
            | ErrorCode::FileLockingProtocolFailed,
        ) => HistoryRepositoryError::Io {
            operation,
            path: path.to_path_buf(),
            message: error.to_string(),
        },
        _ => HistoryRepositoryError::Database {
            operation,
            message: error.to_string(),
        },
    }
}

#[cfg(test)]
mod tests;

use super::*;
use crate::models::{HistoryEntry, HttpMethod, Request, RequestBody};
use crate::persistence::HistoryLoadWarningKind;
use chrono::{TimeZone, Utc};
use tempfile::TempDir;

const BASE_TIMESTAMP_MS: i64 = 1_787_529_600_000;

fn isolated_repository() -> (TempDir, SqliteHistoryRepository) {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary
        .path()
        .join("nested")
        .join("history")
        .join("history.sqlite3");
    let repository = SqliteHistoryRepository::new(path).unwrap();
    (temporary, repository)
}

fn snapshot(index: u64, timestamp_ms: i64) -> VersionedHistorySnapshot {
    let mut entry = HistoryEntry::completed(
        Request::new(
            HttpMethod::GET,
            format!("https://example.com/items/{index}"),
        ),
        format!("entry-{index}"),
        200,
        u128::from(index),
        usize::try_from(index).unwrap(),
    );
    entry.id = format!("00000000-0000-4000-8000-{index:012x}");
    entry.timestamp = Utc.timestamp_millis_opt(timestamp_ms).single().unwrap();
    VersionedHistorySnapshot::try_from(&entry).unwrap()
}

fn row_count(path: &Path) -> i64 {
    Connection::open(path)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM history_entries", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn first_run_creates_parent_schema_pragmas_and_is_idempotent() {
    let (_temporary, mut repository) = isolated_repository();
    let parent = repository.database_path().parent().unwrap().to_path_buf();
    assert!(!parent.exists());

    repository.initialize().unwrap();
    repository.initialize().unwrap();

    assert!(parent.is_dir());
    assert!(repository.database_path().is_file());
    let connection = repository
        .open_connection(HistoryRepositoryOperation::Initialize)
        .unwrap();
    let version = schema_version(
        &connection,
        HistoryRepositoryOperation::Initialize,
        repository.database_path(),
    )
    .unwrap();
    let journal_mode: String = connection
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    let foreign_keys: i64 = connection
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .unwrap();
    let busy_timeout: i64 = connection
        .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
        .unwrap();
    let recent_index: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'history_entries_recent_idx'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(version, CURRENT_HISTORY_SCHEMA_VERSION);
    assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
    assert_eq!(foreign_keys, 1);
    assert_eq!(busy_timeout, 1_000);
    assert_eq!(recent_index, 1);
}

#[test]
fn production_path_is_absolute_and_outside_the_current_working_directory_contract() {
    let path = production_history_database_path().unwrap();
    assert!(path.is_absolute());
    assert_eq!(path.file_name().unwrap(), HISTORY_DATABASE_FILE);
    assert_eq!(
        path.parent().unwrap().file_name().unwrap(),
        APPLICATION_DATA_DIRECTORY
    );
    assert!(matches!(
        SqliteHistoryRepository::new("relative-history.sqlite3").unwrap_err(),
        HistoryRepositoryError::InvalidDatabasePath { .. }
    ));
}

#[test]
fn append_load_order_limit_idempotence_and_clear_are_deterministic() {
    let (_temporary, mut repository) = isolated_repository();
    repository.initialize().unwrap();
    let first = snapshot(1, BASE_TIMESTAMP_MS);
    let second = snapshot(2, BASE_TIMESTAMP_MS);
    let newest = snapshot(3, BASE_TIMESTAMP_MS + 1);

    repository.append_and_trim(&first, 50).unwrap();
    repository.append_and_trim(&second, 50).unwrap();
    repository.append_and_trim(&newest, 50).unwrap();
    repository.append_and_trim(&newest, 50).unwrap();

    let loaded = repository.load_recent(2).unwrap();
    assert!(loaded.warnings().is_empty());
    assert_eq!(loaded.snapshots().len(), 2);
    assert_eq!(loaded.snapshots()[0].as_v1().name(), "entry-3");
    assert_eq!(loaded.snapshots()[1].as_v1().name(), "entry-2");
    assert_eq!(row_count(repository.database_path()), 3);

    repository.clear().unwrap();
    assert!(repository.load_recent(50).unwrap().snapshots().is_empty());
    assert_eq!(row_count(repository.database_path()), 0);
}

#[test]
fn duplicate_entry_id_with_different_payload_is_rejected() {
    let (_temporary, mut repository) = isolated_repository();
    repository.initialize().unwrap();
    let original = snapshot(10, BASE_TIMESTAMP_MS);
    let mut conflicting_entry =
        HistoryEntry::try_from(snapshot(11, BASE_TIMESTAMP_MS + 1)).unwrap();
    conflicting_entry.id = original.as_v1().entry_id().to_string();
    let conflicting = VersionedHistorySnapshot::try_from(&conflicting_entry).unwrap();

    repository.append_and_trim(&original, 50).unwrap();
    assert_eq!(
        repository.append_and_trim(&conflicting, 50).unwrap_err(),
        HistoryRepositoryError::EntryIdConflict {
            entry_id: original.as_v1().entry_id().to_string(),
        }
    );
    assert_eq!(row_count(repository.database_path()), 1);
}

#[test]
fn append_and_retention_trim_are_one_atomic_transaction() {
    let (_temporary, mut repository) = isolated_repository();
    repository.initialize().unwrap();
    for index in 1..=3 {
        repository
            .append_and_trim(&snapshot(index, BASE_TIMESTAMP_MS + index as i64), 50)
            .unwrap();
    }
    let connection = Connection::open(repository.database_path()).unwrap();
    connection
        .execute_batch(
            r#"
CREATE TRIGGER reject_history_trim
BEFORE DELETE ON history_entries
BEGIN
    SELECT RAISE(ABORT, 'trim blocked for atomicity test');
END;
"#,
        )
        .unwrap();
    drop(connection);

    let new_snapshot = snapshot(4, BASE_TIMESTAMP_MS + 4);
    assert!(repository.append_and_trim(&new_snapshot, 2).is_err());
    assert_eq!(row_count(repository.database_path()), 3);
    let connection = Connection::open(repository.database_path()).unwrap();
    let inserted: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM history_entries WHERE entry_id = ?1",
            [new_snapshot.as_v1().entry_id()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(inserted, 0);
}

#[test]
fn retention_keeps_exactly_the_newest_fifty_rows() {
    let (_temporary, mut repository) = isolated_repository();
    repository.initialize().unwrap();
    for index in 0..55 {
        repository
            .append_and_trim(
                &snapshot(index, BASE_TIMESTAMP_MS + index as i64),
                DEFAULT_HISTORY_RETENTION_LIMIT,
            )
            .unwrap();
    }

    let loaded = repository.load_recent(100).unwrap();
    assert_eq!(loaded.snapshots().len(), DEFAULT_HISTORY_RETENTION_LIMIT);
    assert_eq!(loaded.snapshots()[0].as_v1().name(), "entry-54");
    assert_eq!(loaded.snapshots()[49].as_v1().name(), "entry-5");
    assert_eq!(row_count(repository.database_path()), 50);
}

#[test]
fn reopening_the_same_database_recovers_identical_sanitized_snapshots() {
    let (_temporary, mut repository) = isolated_repository();
    repository.initialize().unwrap();
    let expected = snapshot(20, BASE_TIMESTAMP_MS);
    repository.append_and_trim(&expected, 50).unwrap();
    let path = repository.database_path().to_path_buf();
    drop(repository);

    let mut reopened = SqliteHistoryRepository::new(path).unwrap();
    reopened.initialize().unwrap();
    let loaded = reopened.load_recent(50).unwrap();
    assert_eq!(loaded.snapshots(), &[expected]);
    assert!(loaded.warnings().is_empty());
}

#[test]
fn checked_in_v1_fixture_migrates_transactionally_and_preserves_rows() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("legacy.sqlite3");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(include_str!(
            "../../../tests/fixtures/history-schema-v1.sql"
        ))
        .unwrap();
    drop(connection);

    let mut repository = SqliteHistoryRepository::new(path.clone()).unwrap();
    repository.initialize().unwrap();
    let connection = Connection::open(&path).unwrap();
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    let snapshot_version: i64 = connection
        .query_row("SELECT snapshot_version FROM history_entries", [], |row| {
            row.get(0)
        })
        .unwrap();
    drop(connection);

    assert_eq!(version, CURRENT_HISTORY_SCHEMA_VERSION);
    assert_eq!(
        u64::try_from(snapshot_version).unwrap(),
        crate::persistence::HISTORY_SNAPSHOT_VERSION_V1
    );
    let loaded = repository.load_recent(50).unwrap();
    assert!(loaded.warnings().is_empty());
    assert_eq!(loaded.snapshots().len(), 1);
    assert_eq!(loaded.snapshots()[0].as_v1().name(), "Migrated V1 row");
    assert_eq!(
        loaded.snapshots()[0].as_v1().entry_id(),
        "00000000-0000-4000-8000-000000000129"
    );
}

#[test]
fn unsupported_future_schema_is_rejected_without_rewriting_it() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("future.sqlite3");
    let connection = Connection::open(&path).unwrap();
    connection
        .pragma_update(None, "user_version", 999_u32)
        .unwrap();
    drop(connection);

    let mut repository = SqliteHistoryRepository::new(path.clone()).unwrap();
    assert_eq!(
        repository.initialize().unwrap_err(),
        HistoryRepositoryError::UnsupportedSchemaVersion {
            found: 999,
            supported: CURRENT_HISTORY_SCHEMA_VERSION,
        }
    );
    let connection = Connection::open(&path).unwrap();
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    let table_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'history_entries'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, 999);
    assert_eq!(table_count, 0);
}

#[test]
fn malformed_individual_row_is_skipped_while_older_valid_rows_still_load() {
    let (_temporary, mut repository) = isolated_repository();
    repository.initialize().unwrap();
    let valid = snapshot(30, BASE_TIMESTAMP_MS);
    repository.append_and_trim(&valid, 50).unwrap();
    let corrupt_id = "00000000-0000-4000-8000-000000000031";
    let connection = Connection::open(repository.database_path()).unwrap();
    connection
        .execute(
            r#"
INSERT INTO history_entries (entry_id, created_at_ms, snapshot_json, snapshot_version)
VALUES (?1, ?2, ?3, ?4)
"#,
            params![
                corrupt_id,
                BASE_TIMESTAMP_MS + 1,
                b"not-json".as_slice(),
                1_i64
            ],
        )
        .unwrap();
    drop(connection);

    let loaded = repository.load_recent(1).unwrap();
    assert_eq!(loaded.snapshots(), &[valid]);
    assert_eq!(loaded.warnings().len(), 1);
    assert_eq!(loaded.warnings()[0].entry_id(), corrupt_id);
    assert!(matches!(
        loaded.warnings()[0].kind(),
        HistoryLoadWarningKind::SnapshotDecode(HistorySnapshotError::MalformedPayload { .. })
    ));
}

#[test]
fn mismatched_row_metadata_is_skipped_with_actionable_diagnostics() {
    let (_temporary, mut repository) = isolated_repository();
    repository.initialize().unwrap();
    let older_valid = snapshot(32, BASE_TIMESTAMP_MS);
    repository.append_and_trim(&older_valid, 50).unwrap();

    let mismatched = snapshot(33, BASE_TIMESTAMP_MS + 1);
    let payload = mismatched.to_json_bytes().unwrap();
    let stored_entry_id = "00000000-0000-4000-8000-000000000034";
    let connection = Connection::open(repository.database_path()).unwrap();
    connection
        .execute(
            r#"
INSERT INTO history_entries (entry_id, created_at_ms, snapshot_json, snapshot_version)
VALUES (?1, ?2, ?3, ?4)
"#,
            params![
                stored_entry_id,
                BASE_TIMESTAMP_MS + 1,
                payload,
                i64::try_from(mismatched.version()).unwrap()
            ],
        )
        .unwrap();
    drop(connection);

    let loaded = repository.load_recent(1).unwrap();
    assert_eq!(loaded.snapshots(), &[older_valid]);
    assert_eq!(loaded.warnings().len(), 1);
    assert_eq!(loaded.warnings()[0].entry_id(), stored_entry_id);
    assert_eq!(
        loaded.warnings()[0].kind(),
        &HistoryLoadWarningKind::MetadataMismatch {
            field: "entry_id",
            expected: mismatched.as_v1().entry_id().to_string(),
            found: stored_entry_id.to_string(),
        }
    );
}

#[test]
fn busy_lock_is_reported_as_a_typed_nonfatal_error() {
    let (_temporary, repository) = isolated_repository();
    let mut repository = repository.with_busy_timeout(Duration::from_millis(10));
    repository.initialize().unwrap();
    let blocker = Connection::open(repository.database_path()).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();

    assert_eq!(
        repository
            .append_and_trim(&snapshot(40, BASE_TIMESTAMP_MS), 50)
            .unwrap_err(),
        HistoryRepositoryError::Busy {
            operation: HistoryRepositoryOperation::Append,
        }
    );
    blocker.execute_batch("ROLLBACK").unwrap();
}

#[test]
fn read_only_database_loads_but_write_and_clear_fail_as_io() {
    let (_temporary, mut repository) = isolated_repository();
    repository.initialize().unwrap();
    let persisted = snapshot(50, BASE_TIMESTAMP_MS);
    repository.append_and_trim(&persisted, 50).unwrap();
    let mut read_only = repository.read_only_for_test();

    assert_eq!(read_only.load_recent(50).unwrap().snapshots(), &[persisted]);
    assert!(matches!(
        read_only
            .append_and_trim(&snapshot(51, BASE_TIMESTAMP_MS + 1), 50)
            .unwrap_err(),
        HistoryRepositoryError::Io {
            operation: HistoryRepositoryOperation::Append,
            ..
        }
    ));
    assert!(matches!(
        read_only.clear().unwrap_err(),
        HistoryRepositoryError::Io {
            operation: HistoryRepositoryOperation::Clear,
            ..
        }
    ));
}

#[test]
fn corrupt_database_file_is_distinct_from_a_corrupt_individual_row() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("corrupt.sqlite3");
    fs::write(&path, b"this is not a SQLite database").unwrap();
    let mut repository = SqliteHistoryRepository::new(path).unwrap();

    assert!(matches!(
        repository.initialize().unwrap_err(),
        HistoryRepositoryError::CorruptDatabase {
            operation: HistoryRepositoryOperation::Initialize,
            ..
        }
    ));
}

#[test]
fn direct_database_inspection_finds_no_denied_credentials() {
    let (_temporary, mut repository) = isolated_repository();
    repository.initialize().unwrap();
    let mut request = Request::new(
        HttpMethod::POST,
        "https://user-secret:password-secret@example.com/post?tag=rust&api_key=query-secret",
    );
    request.headers = vec![
        (
            "Authorization".to_string(),
            "Bearer authorization-secret".to_string(),
        ),
        ("Cookie".to_string(), "session=cookie-secret".to_string()),
        ("X-Trace".to_string(), "safe-value".to_string()),
    ];
    request.body = RequestBody::Json(
        r#"{"message":"request bodies are documented user-authored data"}"#.to_string(),
    );
    let entry = HistoryEntry::completed(request, "sensitive request".to_string(), 200, 3, 64);
    let snapshot = VersionedHistorySnapshot::try_from(&entry).unwrap();
    repository.append_and_trim(&snapshot, 50).unwrap();

    let connection = Connection::open(repository.database_path()).unwrap();
    let payload: Vec<u8> = connection
        .query_row("SELECT snapshot_json FROM history_entries", [], |row| {
            row.get(0)
        })
        .unwrap();
    let payload = String::from_utf8(payload).unwrap();
    for denied in [
        "user-secret",
        "password-secret",
        "query-secret",
        "authorization-secret",
        "cookie-secret",
    ] {
        assert!(!payload.contains(denied), "SQLite leaked {denied}");
    }
    assert!(payload.contains("safe-value"));
    assert!(payload.contains("documented user-authored data"));
}

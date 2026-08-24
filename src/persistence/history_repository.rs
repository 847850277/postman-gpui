use super::{HistorySnapshotError, VersionedHistorySnapshot};
use std::{fmt, path::PathBuf};

/// Repository operations are carried into typed errors so #130 can degrade and diagnose the
/// failing stage without parsing display strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryRepositoryOperation {
    Initialize,
    Load,
    Append,
    Clear,
}

impl fmt::Display for HistoryRepositoryOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Initialize => formatter.write_str("initialize"),
            Self::Load => formatter.write_str("load"),
            Self::Append => formatter.write_str("append"),
            Self::Clear => formatter.write_str("clear"),
        }
    }
}

/// Failures that make a repository operation unusable. A malformed individual row is represented
/// by `HistoryLoadWarning` instead, allowing other valid rows to load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryRepositoryError {
    ApplicationDataDirectoryUnavailable,
    InvalidDatabasePath {
        path: PathBuf,
        reason: &'static str,
    },
    Io {
        operation: HistoryRepositoryOperation,
        path: PathBuf,
        message: String,
    },
    Busy {
        operation: HistoryRepositoryOperation,
    },
    CorruptDatabase {
        operation: HistoryRepositoryOperation,
        message: String,
    },
    Database {
        operation: HistoryRepositoryOperation,
        message: String,
    },
    Migration {
        from: u32,
        to: u32,
        message: String,
    },
    UnsupportedSchemaVersion {
        found: u32,
        supported: u32,
    },
    NotInitialized {
        found: u32,
        expected: u32,
    },
    Snapshot {
        operation: HistoryRepositoryOperation,
        source: HistorySnapshotError,
    },
    EntryIdConflict {
        entry_id: String,
    },
    LimitTooLarge {
        limit: usize,
    },
    WorkerStart {
        message: String,
    },
    WorkerUnavailable,
}

impl fmt::Display for HistoryRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ApplicationDataDirectoryUnavailable => {
                formatter.write_str("platform application-data directory is unavailable")
            }
            Self::InvalidDatabasePath { path, reason } => {
                write!(
                    formatter,
                    "invalid History database path {}: {reason}",
                    path.display()
                )
            }
            Self::Io {
                operation,
                path,
                message,
            } => write!(
                formatter,
                "History repository {operation} I/O failure at {}: {message}",
                path.display()
            ),
            Self::Busy { operation } => {
                write!(formatter, "History repository is busy during {operation}")
            }
            Self::CorruptDatabase { operation, message } => {
                write!(
                    formatter,
                    "History database is corrupt during {operation}: {message}"
                )
            }
            Self::Database { operation, message } => {
                write!(formatter, "History database {operation} failure: {message}")
            }
            Self::Migration { from, to, message } => {
                write!(
                    formatter,
                    "History schema migration {from} -> {to} failed: {message}"
                )
            }
            Self::UnsupportedSchemaVersion { found, supported } => write!(
                formatter,
                "History schema version {found} is newer than supported version {supported}"
            ),
            Self::NotInitialized { found, expected } => write!(
                formatter,
                "History schema version {found} is not initialized at required version {expected}"
            ),
            Self::Snapshot { operation, source } => {
                write!(
                    formatter,
                    "History snapshot failed during {operation}: {source}"
                )
            }
            Self::EntryIdConflict { entry_id } => write!(
                formatter,
                "History entry ID {entry_id} already belongs to a different snapshot"
            ),
            Self::LimitTooLarge { limit } => {
                write!(
                    formatter,
                    "History retention limit {limit} exceeds SQLite integer range"
                )
            }
            Self::WorkerStart { message } => {
                write!(
                    formatter,
                    "failed to start History storage worker: {message}"
                )
            }
            Self::WorkerUnavailable => formatter.write_str("History storage worker is unavailable"),
        }
    }
}

impl std::error::Error for HistoryRepositoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Snapshot { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Why one durable row was skipped while other rows continued loading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryLoadWarningKind {
    SnapshotDecode(HistorySnapshotError),
    MetadataMismatch {
        field: &'static str,
        expected: String,
        found: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryLoadWarning {
    entry_id: String,
    kind: HistoryLoadWarningKind,
}

impl HistoryLoadWarning {
    pub(crate) fn snapshot_decode(entry_id: String, error: HistorySnapshotError) -> Self {
        Self {
            entry_id,
            kind: HistoryLoadWarningKind::SnapshotDecode(error),
        }
    }

    pub(crate) fn metadata_mismatch(
        entry_id: String,
        field: &'static str,
        expected: impl Into<String>,
        found: impl Into<String>,
    ) -> Self {
        Self {
            entry_id,
            kind: HistoryLoadWarningKind::MetadataMismatch {
                field,
                expected: expected.into(),
                found: found.into(),
            },
        }
    }

    pub fn entry_id(&self) -> &str {
        &self.entry_id
    }

    pub fn kind(&self) -> &HistoryLoadWarningKind {
        &self.kind
    }
}

/// Valid rows and nonfatal row-level diagnostics returned by `load_recent`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistoryLoadResult {
    snapshots: Vec<VersionedHistorySnapshot>,
    warnings: Vec<HistoryLoadWarning>,
}

impl HistoryLoadResult {
    pub(crate) fn new(
        snapshots: Vec<VersionedHistorySnapshot>,
        warnings: Vec<HistoryLoadWarning>,
    ) -> Self {
        Self {
            snapshots,
            warnings,
        }
    }

    pub fn snapshots(&self) -> &[VersionedHistorySnapshot] {
        &self.snapshots
    }

    pub fn warnings(&self) -> &[HistoryLoadWarning] {
        &self.warnings
    }

    pub fn into_parts(self) -> (Vec<VersionedHistorySnapshot>, Vec<HistoryLoadWarning>) {
        (self.snapshots, self.warnings)
    }
}

/// Application-facing History persistence abstraction. Implementations are synchronous because
/// they run inside `HistoryRepositoryWorker`, never on the GPUI thread.
pub trait HistoryRepository: Send {
    fn initialize(&mut self) -> Result<(), HistoryRepositoryError>;

    fn load_recent(&mut self, limit: usize) -> Result<HistoryLoadResult, HistoryRepositoryError>;

    fn append_and_trim(
        &mut self,
        snapshot: &VersionedHistorySnapshot,
        limit: usize,
    ) -> Result<(), HistoryRepositoryError>;

    fn clear(&mut self) -> Result<(), HistoryRepositoryError>;
}

//! Durable application boundaries.
//!
//! GPUI and ViewModels depend only on the application-facing repository/worker boundary. SQL and
//! SQLite connection details remain private to the adapter in this module.

mod history_repository;
mod history_repository_worker;
mod history_snapshot;
mod sqlite_history_repository;

pub use history_repository::{
    HistoryLoadResult, HistoryLoadWarning, HistoryLoadWarningKind, HistoryRepository,
    HistoryRepositoryError, HistoryRepositoryOperation,
};
pub use history_repository_worker::{HistoryRepositoryTask, HistoryRepositoryWorker};

pub use history_snapshot::{
    HeaderSnapshotV1, HistorySensitiveDataPolicy, HistorySnapshotError, HistorySnapshotV1,
    KeyValueSnapshotV1, MultipartEditorPartSnapshotV1, MultipartPartSnapshotV1,
    MultipartValueSnapshotV1, RedirectPolicySnapshotV1, RequestBodySnapshotV1,
    RequestEditorIntentSnapshotV1, RequestOptionsSnapshotV1, RequestSnapshotV1,
    VersionedHistorySnapshot, HISTORY_SNAPSHOT_VERSION_V1,
};
pub use sqlite_history_repository::{
    production_history_database_path, SqliteHistoryRepository, CURRENT_HISTORY_SCHEMA_VERSION,
    DEFAULT_HISTORY_RETENTION_LIMIT,
};

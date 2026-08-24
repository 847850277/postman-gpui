//! Durable application boundaries.
//!
//! This module intentionally contains no GPUI or SQLite types. Storage adapters consume only the
//! sanitized, versioned snapshots exported here.

mod history_snapshot;

pub use history_snapshot::{
    HeaderSnapshotV1, HistorySensitiveDataPolicy, HistorySnapshotError, HistorySnapshotV1,
    KeyValueSnapshotV1, MultipartEditorPartSnapshotV1, MultipartPartSnapshotV1,
    MultipartValueSnapshotV1, RedirectPolicySnapshotV1, RequestBodySnapshotV1,
    RequestEditorIntentSnapshotV1, RequestOptionsSnapshotV1, RequestSnapshotV1,
    VersionedHistorySnapshot, HISTORY_SNAPSHOT_VERSION_V1,
};

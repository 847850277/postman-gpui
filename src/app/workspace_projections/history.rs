use crate::{
    app::request_lifecycle::PendingRequest,
    models::{HistoricalResponse, HistoryEntry, Request, RequestHistory},
};
use std::{
    collections::{HashMap, HashSet},
    fmt,
};

const MAX_HISTORY_URL_LENGTH: usize = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryStorageStage {
    Initialize,
    Load,
    Append,
    Clear,
}

impl fmt::Display for HistoryStorageStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Initialize => "initialization",
            Self::Load => "load",
            Self::Append => "append",
            Self::Clear => "clear",
        })
    }
}

/// Observable state of the SQLite-backed History feature. Entries remain only the latest
/// successful database query result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryStorageStatus {
    Loading {
        stage: HistoryStorageStage,
    },
    Ready {
        skipped_rows: usize,
    },
    Error {
        stage: HistoryStorageStage,
        message: String,
    },
}

/// Immutable SQLite query result consumed by the History read model.
pub(crate) struct HistoryQueryRestoreInput {
    pub(crate) entries: Vec<HistoryEntry>,
    pub(crate) skipped_rows: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HistoryRestoreTransition {
    Restored { entry_count: usize },
    Reset,
}

pub(crate) struct HistoryQueryRestoreOutput {
    transition: HistoryRestoreTransition,
    retained_entry_ids: HashSet<String>,
}

impl HistoryQueryRestoreOutput {
    pub(crate) fn transition(&self) -> HistoryRestoreTransition {
        self.transition
    }

    pub(crate) fn retains(&self, entry_id: &str) -> bool {
        self.retained_entry_ids.contains(entry_id)
    }
}

/// Explicit persistence failure input. Applying it changes status but never fabricates or clears
/// the last successful SQLite query projection.
pub(crate) struct HistoryPersistenceFailure {
    pub(crate) stage: HistoryStorageStage,
    pub(crate) message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryReplaySource {
    RuntimeSecretOverlay,
    PersistedSnapshot,
}

/// Complete request input for restoring one History row into a specific request tab. Runtime
/// overlay inputs may contain secrets and must never cross the persistence boundary.
#[derive(Clone)]
pub struct HistoryReplayInput {
    entry_id: String,
    request: Request,
    source: HistoryReplaySource,
}

impl HistoryReplayInput {
    pub fn entry_id(&self) -> &str {
        &self.entry_id
    }

    pub fn request(&self) -> &Request {
        &self.request
    }

    pub fn source(&self) -> HistoryReplaySource {
        self.source
    }
}

/// Accepted request-lifecycle output consumed when constructing a persistence candidate.
pub(crate) struct CompletedSendHistoryInput<'a> {
    pub(crate) pending: &'a PendingRequest,
    pub(crate) response: HistoricalResponse,
}

/// SQLite-backed History read model plus the current-process secret replay overlay.
///
/// It is deliberately not a repository and never appends to visible History directly.
pub(crate) struct HistoryProjection {
    history: RequestHistory,
    runtime_replay_requests: HashMap<String, Request>,
    storage_status: HistoryStorageStatus,
}

impl HistoryProjection {
    pub(crate) fn new() -> Self {
        Self {
            history: RequestHistory::new(),
            runtime_replay_requests: HashMap::new(),
            storage_status: HistoryStorageStatus::Loading {
                stage: HistoryStorageStage::Initialize,
            },
        }
    }

    pub(crate) fn entries(&self) -> &[HistoryEntry] {
        self.history.entries()
    }

    pub(crate) fn len(&self) -> usize {
        self.history.len()
    }

    pub(crate) fn storage_status(&self) -> &HistoryStorageStatus {
        &self.storage_status
    }

    pub(crate) fn set_loading(&mut self, stage: HistoryStorageStage) {
        self.storage_status = HistoryStorageStatus::Loading { stage };
    }

    pub(crate) fn restore_query(
        &mut self,
        input: HistoryQueryRestoreInput,
    ) -> HistoryQueryRestoreOutput {
        self.history.replace(input.entries);
        let retained_entry_ids = self
            .history
            .entries()
            .iter()
            .map(|entry| entry.id.clone())
            .collect::<HashSet<_>>();
        self.runtime_replay_requests
            .retain(|entry_id, _| retained_entry_ids.contains(entry_id));
        self.storage_status = HistoryStorageStatus::Ready {
            skipped_rows: input.skipped_rows,
        };
        let transition = if retained_entry_ids.is_empty() {
            HistoryRestoreTransition::Reset
        } else {
            HistoryRestoreTransition::Restored {
                entry_count: retained_entry_ids.len(),
            }
        };
        HistoryQueryRestoreOutput {
            transition,
            retained_entry_ids,
        }
    }

    pub(crate) fn confirm_runtime_replay(&mut self, entry_id: String, request: Request) -> bool {
        if self
            .history
            .entries()
            .iter()
            .any(|entry| entry.id == entry_id)
        {
            self.runtime_replay_requests.insert(entry_id, request);
            true
        } else {
            false
        }
    }

    pub(crate) fn fail(&mut self, input: HistoryPersistenceFailure) {
        self.storage_status = HistoryStorageStatus::Error {
            stage: input.stage,
            message: input.message,
        };
    }

    pub(crate) fn replay_input(&self, entry: &HistoryEntry) -> HistoryReplayInput {
        let (request, source) = self.runtime_replay_requests.get(&entry.id).map_or_else(
            || {
                (
                    entry.request.clone(),
                    HistoryReplaySource::PersistedSnapshot,
                )
            },
            |request| (request.clone(), HistoryReplaySource::RuntimeSecretOverlay),
        );
        HistoryReplayInput {
            entry_id: entry.id.clone(),
            request,
            source,
        }
    }

    pub(crate) fn candidate_from_completion(
        &self,
        input: CompletedSendHistoryInput<'_>,
    ) -> HistoryEntry {
        HistoryEntry::completed_with_intent_and_options(
            input.pending.request().clone(),
            history_label(&input.pending.request().url),
            input.response.status,
            input.response.elapsed_ms,
            input.response.original_size,
            input.pending.editor_intent().cloned(),
            input.pending.request_options(),
        )
        .with_historical_response(input.response)
    }
}

fn history_label(url: &str) -> String {
    if url.chars().count() > MAX_HISTORY_URL_LENGTH {
        format!(
            "{}…",
            url.chars().take(MAX_HISTORY_URL_LENGTH).collect::<String>()
        )
    } else {
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::{RequestTabId, SendId, SendStart},
        models::{HttpMethod, RequestOptions},
    };
    use std::sync::{atomic::AtomicBool, Arc};

    #[test]
    fn restore_reset_and_failure_keep_the_sqlite_projection_contract() {
        let mut projection = HistoryProjection::new();
        let persisted_request = Request::new(HttpMethod::GET, "https://example.com/sanitized");
        let entry = HistoryEntry::new(persisted_request.clone(), "persisted".into());

        let restored = projection.restore_query(HistoryQueryRestoreInput {
            entries: vec![entry.clone()],
            skipped_rows: 2,
        });
        assert_eq!(
            restored.transition(),
            HistoryRestoreTransition::Restored { entry_count: 1 }
        );
        assert!(restored.retains(&entry.id));
        assert_eq!(
            projection.storage_status(),
            &HistoryStorageStatus::Ready { skipped_rows: 2 }
        );

        let runtime_request = Request::new(
            HttpMethod::GET,
            "https://example.com/raw?api_key=runtime-secret",
        );
        assert!(projection.confirm_runtime_replay(entry.id.clone(), runtime_request.clone()));
        let replay = projection.replay_input(&entry);
        assert_eq!(replay.request(), &runtime_request);
        assert_eq!(replay.source(), HistoryReplaySource::RuntimeSecretOverlay);

        projection.fail(HistoryPersistenceFailure {
            stage: HistoryStorageStage::Load,
            message: "database locked".into(),
        });
        assert_eq!(projection.len(), 1, "failure keeps the last good query");
        assert!(matches!(
            projection.storage_status(),
            HistoryStorageStatus::Error {
                stage: HistoryStorageStage::Load,
                message,
            } if message == "database locked"
        ));

        let reset = projection.restore_query(HistoryQueryRestoreInput {
            entries: Vec::new(),
            skipped_rows: 0,
        });
        assert_eq!(reset.transition(), HistoryRestoreTransition::Reset);
        assert!(!reset.retains(&entry.id));
        let replay_after_reset = projection.replay_input(&entry);
        assert_eq!(replay_after_reset.request(), &persisted_request);
        assert_eq!(
            replay_after_reset.source(),
            HistoryReplaySource::PersistedSnapshot
        );
    }

    #[test]
    fn runtime_replay_overlay_requires_a_restored_sqlite_identity() {
        let mut projection = HistoryProjection::new();
        let request = Request::new(HttpMethod::GET, "https://example.com/raw");

        assert!(!projection.confirm_runtime_replay("missing".into(), request));
        assert_eq!(projection.len(), 0);
    }

    #[test]
    fn accepted_completion_is_the_only_input_needed_for_a_history_candidate() {
        let request = Request::new(
            HttpMethod::POST,
            "https://example.com/a-very-long-request-path-that-needs-a-short-label",
        );
        let pending = PendingRequest::new(
            RequestTabId(1),
            SendId(1),
            SendStart::Begin,
            request.clone(),
            None,
            RequestOptions::default(),
            Arc::new(AtomicBool::new(false)),
        );
        let response = HistoricalResponse::completed(
            201,
            vec![("content-type".into(), "application/json".into())],
            "{}".into(),
            9,
        );

        let candidate =
            HistoryProjection::new().candidate_from_completion(CompletedSendHistoryInput {
                pending: &pending,
                response,
            });

        assert_eq!(candidate.request, request);
        assert_eq!(candidate.status, Some(201));
        assert_eq!(candidate.elapsed_ms, Some(9));
        assert!(candidate.name.ends_with('…'));
        assert!(candidate.historical_response.is_some());
    }
}

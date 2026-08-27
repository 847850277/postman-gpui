mod cookies;
mod history;
mod request_tags;
mod search;

pub use cookies::CookieJarEntry;
pub use history::{
    HistoryReplayInput, HistoryReplaySource, HistoryStorageStage, HistoryStorageStatus,
};
pub use request_tags::RequestTagProjection;
pub use search::{GlobalSearchHistoryResult, GlobalSearchRequestResult, GlobalSearchResults};

pub(crate) use cookies::{CookieProjection, CookieProjectionEvent};
pub(crate) use history::{
    CompletedSendHistoryInput, HistoryPersistenceFailure, HistoryProjection,
    HistoryQueryRestoreInput,
};
pub(crate) use request_tags::{request_tag_title, RequestTagInput, RequestTagProjector};
pub(crate) use search::{GlobalSearchInput, GlobalSearchProjection};

/// Cohesive application read models kept behind the workspace coordination facade.
pub(crate) struct WorkspaceProjections {
    pub(crate) history: HistoryProjection,
    pub(crate) search: GlobalSearchProjection,
    pub(crate) cookies: CookieProjection,
    pub(crate) request_tags: RequestTagProjector,
}

impl WorkspaceProjections {
    pub(crate) fn new() -> Self {
        Self {
            history: HistoryProjection::new(),
            search: GlobalSearchProjection,
            cookies: CookieProjection::default(),
            request_tags: RequestTagProjector,
        }
    }
}

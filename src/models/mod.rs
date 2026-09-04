// This file serves as a module for data models used in the application.

pub mod history;
pub mod request_draft;

// Re-export commonly used types
pub use history::{HistoricalResponse, HistoricalResponseBody, HistoryEntry, RequestHistory};
pub use postman_http::request::{
    HttpMethod, MultipartPart, MultipartValue, RedirectPolicy, Request, RequestBody,
    RequestOptions, DEFAULT_MAX_REDIRECT_HOPS, MAX_REDIRECT_HOPS,
};
pub use postman_http::response::RedirectHop;
pub use request_draft::{
    AuthorizationKind, BodyKind, EffectiveHeader, EffectiveHeaderSource, KeyValueRow,
    MultipartDraftPart, MultipartDraftValue, MultipartEditorPart, RequestBodyDraft,
    RequestConstruction, RequestDraft, RequestDraftError, RequestEditorIntent,
};

// This file serves as a module for data models used in the application.

pub mod history;
pub mod request;
pub mod response;

// Re-export commonly used types
pub use history::{
    HistoricalResponse, HistoricalResponseBody, HistoryEntry, MultipartEditorPart,
    RequestEditorIntent, RequestHistory,
};
pub use request::{
    HttpMethod, MultipartPart, MultipartValue, RedirectPolicy, Request, RequestBody,
    RequestOptions, DEFAULT_MAX_REDIRECT_HOPS, MAX_REDIRECT_HOPS,
};
pub use response::RedirectHop;

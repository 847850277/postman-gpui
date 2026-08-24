// This file serves as a module for data models used in the application.

pub mod history;
pub mod request;

// Re-export commonly used types
pub use history::{HistoryEntry, MultipartEditorPart, RequestEditorIntent, RequestHistory};
pub use request::{
    HttpMethod, MultipartPart, MultipartValue, RedirectPolicy, Request, RequestBody,
    RequestOptions, DEFAULT_MAX_REDIRECT_HOPS, MAX_REDIRECT_HOPS,
};

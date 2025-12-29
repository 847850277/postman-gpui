// This file serves as a module for data models used in the application.

pub mod collection;
pub mod history;
pub mod request;
pub mod workspace;

// Re-export commonly used types
pub use collection::Collection;
pub use history::{HistoryEntry, RequestHistory};
pub use request::{HttpMethod, Request};

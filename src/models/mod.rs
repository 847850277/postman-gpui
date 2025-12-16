// This file serves as a module for data models used in the application.

pub mod collection;
pub mod request;
pub mod workspace;

// Re-export commonly used types
pub use collection::Collection;
pub use request::Request;

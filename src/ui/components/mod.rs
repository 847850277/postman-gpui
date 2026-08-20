// src/ui/components/mod.rs
pub mod common;
pub mod display;
pub mod input;

// Re-export commonly used types for backward compatibility
pub use common::dropdown;
pub use display::{method_selector, response_viewer, ResponseState};
pub use input::{body_input, header_input, url_input};

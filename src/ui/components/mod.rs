// src/ui/components/mod.rs
pub mod input;
pub mod common;
pub mod display;

// Re-export commonly used types for backward compatibility
pub use input::{body_input, header_input, url_input};
pub use common::dropdown;
pub use display::{history_list, method_selector, response_viewer};

//! Field-local definition rules shared by editors and compilation. No values are resolved.
use crate::json_path::JsonPath;

/// Validate a definition name without trimming or rewriting its identity.
pub fn validate_definition_name(value: &str) -> Result<(), &'static str> {
    if value.trim().is_empty() {
        Err("name cannot be empty")
    } else {
        Ok(())
    }
}

/// Validate the status-code range accepted by native response checks.
pub fn validate_response_status(value: u16) -> Result<(), &'static str> {
    if (100..=599).contains(&value) {
        Ok(())
    } else {
        Err("HTTP status must be between 100 and 599")
    }
}

/// Validate the engine's JSONPath subset without inspecting a response.
pub fn validate_json_path(value: &str) -> Result<(), String> {
    JsonPath::compile(value).map(|_| ())
}

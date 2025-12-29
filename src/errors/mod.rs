// src/errors/mod.rs
//! Unified error handling module for the Postman GPUI application.
//!
//! This module provides a centralized error type that can be used throughout
//! the application for consistent error handling and reporting.

use std::fmt;

/// Unified application error type
#[derive(Debug, Clone)]
pub enum AppError {
    /// HTTP request error (wraps reqwest errors)
    HttpError(String),
    /// Validation error (e.g., invalid input)
    ValidationError(String),
    /// Parse error (e.g., JSON parsing failed)
    ParseError(String),
    /// URL is empty or missing
    UrlEmpty,
    /// Network connection error
    NetworkError(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::HttpError(msg) => write!(f, "HTTP Error: {}", msg),
            AppError::ValidationError(msg) => write!(f, "Validation Error: {}", msg),
            AppError::ParseError(msg) => write!(f, "Parse Error: {}", msg),
            AppError::UrlEmpty => write!(f, "Error: URL cannot be empty"),
            AppError::NetworkError(msg) => write!(f, "Network Error: {}", msg),
        }
    }
}

impl std::error::Error for AppError {}

// Implement From trait for reqwest::Error
impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            AppError::NetworkError(format!("Request timeout: {}", err))
        } else if err.is_connect() {
            AppError::NetworkError(format!("Connection failed: {}", err))
        } else if err.is_status() {
            AppError::HttpError(format!("HTTP status error: {}", err))
        } else {
            AppError::HttpError(err.to_string())
        }
    }
}

// Implement From trait for String (for backward compatibility)
impl From<String> for AppError {
    fn from(msg: String) -> Self {
        AppError::ValidationError(msg)
    }
}

// Implement From trait for &str (for convenience)
impl From<&str> for AppError {
    fn from(msg: &str) -> Self {
        AppError::ValidationError(msg.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_error_display() {
        let err = AppError::UrlEmpty;
        assert_eq!(err.to_string(), "Error: URL cannot be empty");

        let err = AppError::ValidationError("Invalid input".to_string());
        assert_eq!(err.to_string(), "Validation Error: Invalid input");

        let err = AppError::HttpError("404 Not Found".to_string());
        assert_eq!(err.to_string(), "HTTP Error: 404 Not Found");

        let err = AppError::ParseError("Invalid JSON".to_string());
        assert_eq!(err.to_string(), "Parse Error: Invalid JSON");

        let err = AppError::NetworkError("Connection timeout".to_string());
        assert_eq!(err.to_string(), "Network Error: Connection timeout");
    }

    #[test]
    fn test_from_string() {
        let err: AppError = "test error".into();
        assert_eq!(err.to_string(), "Validation Error: test error");

        let err: AppError = String::from("another error").into();
        assert_eq!(err.to_string(), "Validation Error: another error");
    }

    #[test]
    fn test_error_is_send_and_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<AppError>();
        assert_sync::<AppError>();
    }

    #[test]
    fn test_error_is_clone() {
        let err = AppError::UrlEmpty;
        let cloned = err.clone();
        assert_eq!(err.to_string(), cloned.to_string());
    }
}

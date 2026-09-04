// src/errors/mod.rs
//! Application-level error composition.
//!
//! HTTP failures are defined by `postman-http` and wrapped here. This module owns only errors
//! introduced by the application itself, so transport behavior is not duplicated in the GUI
//! crate.

use std::fmt;

use postman_http::{response::RedirectHop, HttpError};

/// Error exposed by the application boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    /// A transport-independent HTTP failure supplied by `postman-http`.
    Http(HttpError),
    /// Invalid application input or state.
    ValidationError(String),
    /// Application-owned parsing failure.
    ParseError(String),
    /// Failure while creating or driving application runtime infrastructure.
    RuntimeError(String),
    /// UI rendering failure.
    RenderError(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(HttpError::EmptyUrl) => formatter.write_str("Error: URL cannot be empty"),
            Self::Http(HttpError::InvalidRequest(message)) => {
                write!(formatter, "Validation Error: {message}")
            }
            Self::Http(HttpError::InvalidResponse(message)) => {
                write!(formatter, "HTTP Error: {message}")
            }
            Self::Http(HttpError::Network(message)) => {
                write!(formatter, "Network Error: {message}")
            }
            Self::Http(HttpError::Timeout { timeout_ms }) => {
                write!(
                    formatter,
                    "Request timed out after {} ms",
                    format_number(*timeout_ms)
                )
            }
            Self::Http(HttpError::ResponseTooLarge {
                limit_bytes,
                size_bytes,
            }) => {
                write!(
                    formatter,
                    "Response body is too large: {} bytes exceeds the {}-byte limit",
                    format_number(*size_bytes),
                    format_number(*limit_bytes)
                )
            }
            Self::Http(HttpError::Cancelled) => formatter.write_str("Request cancelled"),
            Self::Http(HttpError::RedirectLimitExceeded { max_hops, .. }) => {
                write!(formatter, "Redirect limit exceeded after {max_hops} hops.")
            }
            Self::ValidationError(message) => {
                write!(formatter, "Validation Error: {message}")
            }
            Self::ParseError(message) => write!(formatter, "Parse Error: {message}"),
            Self::RuntimeError(message) => write!(formatter, "Runtime Error: {message}"),
            Self::RenderError(message) => write!(formatter, "Render Error: {message}"),
        }
    }
}

fn format_number(value: u64) -> String {
    let digits = value.to_string();
    let mut formatted = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            formatted.push(',');
        }
        formatted.push(character);
    }
    formatted
}

impl AppError {
    /// Returns redirect evidence when the wrapped HTTP failure exceeded its redirect limit.
    pub fn redirect_chain(&self) -> &[RedirectHop] {
        match self {
            Self::Http(error) => error.redirect_chain(),
            _ => &[],
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::ValidationError(_)
            | Self::ParseError(_)
            | Self::RuntimeError(_)
            | Self::RenderError(_) => None,
        }
    }
}

impl From<HttpError> for AppError {
    fn from(error: HttpError) -> Self {
        Self::Http(error)
    }
}

impl From<String> for AppError {
    fn from(message: String) -> Self {
        Self::ValidationError(message)
    }
}

impl From<&str> for AppError {
    fn from(message: &str) -> Self {
        Self::ValidationError(message.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_http_errors_for_the_application() {
        let error = AppError::from(HttpError::Timeout { timeout_ms: 1_000 });
        assert_eq!(error.to_string(), "Request timed out after 1,000 ms");

        let error = AppError::from(HttpError::ResponseTooLarge {
            limit_bytes: 33_554_432,
            size_bytes: 33_554_433,
        });
        assert_eq!(
            error.to_string(),
            "Response body is too large: 33,554,433 bytes exceeds the 33,554,432-byte limit"
        );
    }

    #[test]
    fn displays_application_errors() {
        let cases = [
            (
                AppError::ValidationError("invalid input".to_owned()),
                "Validation Error: invalid input",
            ),
            (
                AppError::ParseError("invalid JSON".to_owned()),
                "Parse Error: invalid JSON",
            ),
            (
                AppError::RuntimeError("executor unavailable".to_owned()),
                "Runtime Error: executor unavailable",
            ),
            (
                AppError::RenderError("window closed".to_owned()),
                "Render Error: window closed",
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(error.to_string(), expected);
        }
    }

    #[test]
    fn converts_http_and_validation_errors() {
        assert_eq!(
            AppError::from(HttpError::EmptyUrl),
            AppError::Http(HttpError::EmptyUrl)
        );
        assert_eq!(
            AppError::from("invalid input"),
            AppError::ValidationError("invalid input".to_owned())
        );
    }

    #[test]
    fn exposes_redirect_chain_from_wrapped_http_error() {
        let chain = vec![RedirectHop::new(
            302,
            "https://example.com/start",
            Some("/next"),
        )];
        let error = AppError::from(HttpError::RedirectLimitExceeded {
            max_hops: 1,
            chain: chain.clone(),
        });

        assert_eq!(error.redirect_chain(), chain);
        assert!(AppError::ParseError("invalid".to_owned())
            .redirect_chain()
            .is_empty());
    }

    #[test]
    fn error_is_send_sync_and_clone() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AppError>();

        let error = AppError::from(HttpError::network("offline"));
        assert_eq!(error.clone(), error);
    }
}

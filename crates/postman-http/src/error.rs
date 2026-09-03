use std::fmt;

use crate::response::RedirectHop;

/// Transport-independent failures produced while preparing or executing an HTTP request.
///
/// Concrete adapters such as `postman-reqwest` translate their implementation errors into this
/// type instead of exposing reqwest or runtime-specific types to callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpError {
    EmptyUrl,
    InvalidRequest(String),
    InvalidResponse(String),
    Network(String),
    Timeout {
        timeout_ms: u64,
    },
    Cancelled,
    RedirectLimitExceeded {
        max_hops: u32,
        chain: Vec<RedirectHop>,
    },
}

impl HttpError {
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::InvalidRequest(message.into())
    }

    pub fn invalid_response(message: impl Into<String>) -> Self {
        Self::InvalidResponse(message.into())
    }

    pub fn network(message: impl Into<String>) -> Self {
        Self::Network(message.into())
    }

    /// Returns the redirect evidence retained by a redirect-limit failure.
    pub fn redirect_chain(&self) -> &[RedirectHop] {
        match self {
            Self::RedirectLimitExceeded { chain, .. } => chain,
            _ => &[],
        }
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyUrl => formatter.write_str("request URL cannot be empty"),
            Self::InvalidRequest(message) => write!(formatter, "invalid HTTP request: {message}"),
            Self::InvalidResponse(message) => {
                write!(formatter, "invalid HTTP response: {message}")
            }
            Self::Network(message) => write!(formatter, "HTTP network error: {message}"),
            Self::Timeout { timeout_ms } => {
                write!(formatter, "HTTP request timed out after {timeout_ms} ms")
            }
            Self::Cancelled => formatter.write_str("HTTP request was cancelled"),
            Self::RedirectLimitExceeded { max_hops, .. } => {
                write!(
                    formatter,
                    "HTTP redirect limit exceeded after {max_hops} hop(s)"
                )
            }
        }
    }
}

impl std::error::Error for HttpError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_messages_are_stable_and_transport_independent() {
        let cases = [
            (HttpError::EmptyUrl, "request URL cannot be empty"),
            (
                HttpError::invalid_request("header name is empty"),
                "invalid HTTP request: header name is empty",
            ),
            (
                HttpError::invalid_response("missing status line"),
                "invalid HTTP response: missing status line",
            ),
            (
                HttpError::network("connection refused"),
                "HTTP network error: connection refused",
            ),
            (
                HttpError::Timeout { timeout_ms: 1_500 },
                "HTTP request timed out after 1500 ms",
            ),
            (HttpError::Cancelled, "HTTP request was cancelled"),
            (
                HttpError::RedirectLimitExceeded {
                    max_hops: 3,
                    chain: Vec::new(),
                },
                "HTTP redirect limit exceeded after 3 hop(s)",
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(error.to_string(), expected);
        }
    }

    #[test]
    fn redirect_chain_is_available_only_for_redirect_limit_errors() {
        let chain = vec![RedirectHop::new(
            302,
            "https://example.com/start",
            Some("/next"),
        )];
        let error = HttpError::RedirectLimitExceeded {
            max_hops: 1,
            chain: chain.clone(),
        };

        assert_eq!(error.redirect_chain(), chain);
        assert!(HttpError::Cancelled.redirect_chain().is_empty());
    }

    #[test]
    fn error_is_send_sync_and_clone() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<HttpError>();

        let error = HttpError::network("offline");
        assert_eq!(error.clone(), error);
    }
}

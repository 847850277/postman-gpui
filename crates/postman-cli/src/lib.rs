//! UI-independent `.http` parsing and sequential execution.
//!
//! This crate is the first non-GPUI host for `postman-http` and `postman-request`. The parser is
//! intentionally a small, explicit compatibility subset; the execution boundary remains generic
//! over [`postman_http::HttpTransport`] so deterministic tests do not need network access.

mod http_file;
mod runner;

pub use http_file::{
    parse_http_file, Assertion, Capture, ExpectedError, HttpFile, HttpFileRequest, ParseError,
    RequestOptionOverrides,
};
pub use runner::{AssertionReport, HeadlessRunner, RequestReport, RunReport};

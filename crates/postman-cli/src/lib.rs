//! UI-independent `.http` parsing and Flow-backed sequential execution.
//!
//! `.http` files compile to [`postman_flow::FlowDefinition`]. `.http.yml` documents are parsed
//! directly. Both run through the same Flow plan and [`postman_http::HttpTransport`].

mod flow_runner;
mod http_file;
mod runner;

pub use flow_runner::{check_flow, run_flow, FlowCheckReport};
pub use http_file::{
    parse_http_file, Assertion, Capture, ExpectedError, HttpFile, HttpFileRequest, ParseError,
    RequestOptionOverrides,
};
pub use runner::{compile_http_file, AssertionReport, HeadlessRunner, RequestReport, RunReport};

use std::collections::BTreeMap;

use postman_flow::{compile_flow, ApiCatalog, CompileEnvironment, FlowInputs, FlowPlan};
use postman_http::{request::RequestOptions, HttpTransport};
use serde::Serialize;

use crate::{flow_runner::run_flow_plan, HttpFile};

pub fn compile_http_file(file: &HttpFile) -> Result<FlowPlan, String> {
    let definition = file
        .to_flow_definition()
        .map_err(|error| error.to_string())?;
    compile_flow(
        &definition,
        &ApiCatalog::new(),
        &CompileEnvironment::default(),
    )
    .map_err(|diagnostics| {
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunReport {
    pub success: bool,
    pub requests: Vec<RequestReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RequestReport {
    pub name: String,
    pub success: bool,
    pub status: Option<u16>,
    pub elapsed_ms: Option<u128>,
    pub assertions: Vec<AssertionReport>,
    pub captures: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssertionReport {
    pub expression: String,
    pub success: bool,
    pub message: Option<String>,
}

pub struct HeadlessRunner<T> {
    transport: T,
    options: RequestOptions,
}

impl<T> HeadlessRunner<T>
where
    T: HttpTransport + Clone,
{
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            options: RequestOptions::default(),
        }
    }

    pub fn with_options(mut self, options: RequestOptions) -> Self {
        self.options = options;
        self
    }

    pub async fn run(
        &self,
        file: &HttpFile,
        overrides: &BTreeMap<String, String>,
    ) -> Result<RunReport, String> {
        let plan = compile_http_file(file)?;
        let mut merged = file.variables.clone();
        merged.extend(overrides.clone());
        let mut inputs = FlowInputs::new();
        for (name, value) in merged {
            inputs.insert(name, value);
        }
        run_flow_plan(self.transport.clone(), plan, inputs, self.options).await
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    use postman_http::{
        request::{Request, RequestBody},
        HttpError, HttpResponse,
    };

    use super::*;
    use crate::parse_http_file;

    #[derive(Clone)]
    struct FakeTransport {
        state: Arc<Mutex<FakeState>>,
    }

    struct FakeState {
        responses: VecDeque<Result<HttpResponse, HttpError>>,
        requests: Vec<Request>,
        options: Vec<RequestOptions>,
    }

    impl FakeTransport {
        fn new(responses: impl IntoIterator<Item = Result<HttpResponse, HttpError>>) -> Self {
            Self {
                state: Arc::new(Mutex::new(FakeState {
                    responses: responses.into_iter().collect(),
                    requests: Vec::new(),
                    options: Vec::new(),
                })),
            }
        }

        fn requests(&self) -> Vec<Request> {
            self.state
                .lock()
                .expect("fake transport state should not be poisoned")
                .requests
                .clone()
        }

        fn options(&self) -> Vec<RequestOptions> {
            self.state
                .lock()
                .expect("fake transport state should not be poisoned")
                .options
                .clone()
        }
    }

    impl HttpTransport for FakeTransport {
        async fn execute(
            &self,
            request: Request,
            options: RequestOptions,
        ) -> Result<HttpResponse, HttpError> {
            let mut state = self
                .state
                .lock()
                .expect("fake transport state should not be poisoned");
            state.requests.push(request);
            state.options.push(options);
            state
                .responses
                .pop_front()
                .expect("the fake transport must have one response per request")
        }
    }

    #[tokio::test]
    async fn captured_json_value_is_bound_into_the_next_typed_request() {
        let file = parse_http_file(
            r#"
@host = https://example.com
@secret = do-not-print-this

### Seed
# @name seed
# @assert status == 200
# @capture correlation_id = jsonpath "$.uuid"
GET {{host}}/uuid

### Echo
# @name echo
# @assert status == 200
# @assert jsonpath "$.json.correlation_id" == "{{correlation_id}}"
POST {{host}}/anything/{{correlation_id}}
Content-Type: application/json
Authorization: Bearer {{secret}}

{"correlation_id":"{{correlation_id}}"}
"#,
        )
        .expect("the flow fixture should parse");
        let transport = FakeTransport::new([
            Ok(HttpResponse::new(
                200,
                Vec::new(),
                r#"{"uuid":"flow-123"}"#.to_owned(),
            )),
            Ok(HttpResponse::new(
                200,
                Vec::new(),
                r#"{"json":{"correlation_id":"flow-123"}}"#.to_owned(),
            )),
        ]);

        let report = HeadlessRunner::new(transport.clone())
            .run(&file, &BTreeMap::new())
            .await
            .expect("the compiled .http file should run");

        assert!(report.success, "{report:#?}");
        assert_eq!(report.requests.len(), 2);
        assert_eq!(report.requests[0].captures, ["correlation_id"]);
        let requests = transport.requests();
        assert_eq!(requests[1].url, "https://example.com/anything/flow-123");
        assert_eq!(
            requests[1].body,
            RequestBody::Json(r#"{"correlation_id":"flow-123"}"#.to_owned())
        );
        let serialized = serde_json::to_string(&report).expect("the report should serialize");
        assert!(!serialized.contains("do-not-print-this"));
        assert!(!serialized.contains("flow-123"));
    }

    #[tokio::test]
    async fn a_failed_assertion_stops_the_remaining_flow() {
        let file = parse_http_file(
            "### first\n# @assert status == 201\nGET https://example.com/first\n\n### second\nGET https://example.com/second\n",
        )
        .expect("the failure fixture should parse");
        let transport = FakeTransport::new([
            Ok(HttpResponse::new(200, Vec::new(), String::new())),
            Ok(HttpResponse::new(200, Vec::new(), String::new())),
        ]);

        let report = HeadlessRunner::new(transport.clone())
            .run(&file, &BTreeMap::new())
            .await
            .expect("assertion failures are a run report, not a compile error");

        assert!(!report.success);
        assert_eq!(report.requests.len(), 1);
        assert_eq!(transport.requests().len(), 1);
        assert!(!report.requests[0].assertions[0].success);
    }

    #[tokio::test]
    async fn nested_file_variables_are_expanded_against_overrides() {
        let file = parse_http_file(
            "@base = https://example.com\n@url = {{base}}/anything\n# @assert status == 200\nGET {{url}}\n",
        )
        .expect("the nested variable fixture should parse");
        let transport = FakeTransport::new([Ok(HttpResponse::new(200, Vec::new(), String::new()))]);
        let overrides = BTreeMap::from([("base".to_owned(), "https://other.test".to_owned())]);

        let report = HeadlessRunner::new(transport.clone())
            .run(&file, &overrides)
            .await
            .expect("nested file variables should compile");

        assert!(report.success, "{report:#?}");
        assert_eq!(transport.requests()[0].url, "https://other.test/anything");
    }

    #[tokio::test]
    async fn cyclic_file_variables_are_compile_errors() {
        let file = parse_http_file("@cycle = {{cycle}}\nGET {{cycle}}\n")
            .expect("cyclic source should parse");
        let transport = FakeTransport::new([]);
        let error = HeadlessRunner::new(transport)
            .run(&file, &BTreeMap::new())
            .await
            .expect_err("cycles must fail before any request");
        assert!(error.contains("cycle"), "{error}");
    }

    #[tokio::test]
    async fn expected_timeout_is_a_passing_step_and_request_options_are_local() {
        let file = parse_http_file(
            "### timeout\n# @timeout-ms 25\n# @redirect no-follow\n# @max-redirects 2\n# @assert error == timeout\nGET https://example.com/slow\n\n### next\n# @assert status == 200\nGET https://example.com/next\n",
        )
        .expect("expected-error flow should parse");
        let transport = FakeTransport::new([
            Err(HttpError::Timeout { timeout_ms: 25 }),
            Ok(HttpResponse::new(200, Vec::new(), String::new())),
        ]);

        let report = HeadlessRunner::new(transport.clone())
            .run(&file, &BTreeMap::new())
            .await
            .expect("expected-error flow should compile");

        assert!(report.success, "{report:#?}");
        assert_eq!(report.requests.len(), 2);
        assert!(report.requests[0].error.is_none());
        assert_eq!(transport.options()[0].timeout_ms, Some(25));
        assert_eq!(
            transport.options()[0].redirect_policy,
            postman_http::request::RedirectPolicy::DoNotFollow
        );
        assert_eq!(transport.options()[0].max_redirect_hops, 2);
        assert_eq!(transport.options()[1], RequestOptions::default());
    }

    #[tokio::test]
    async fn unexpected_error_kind_fails_and_stops_the_flow() {
        let file = parse_http_file("# @assert error == timeout\nGET https://example.com/failure\n")
            .expect("expected-error flow should parse");
        let transport = FakeTransport::new([Err(HttpError::network("offline"))]);

        let report = HeadlessRunner::new(transport)
            .run(&file, &BTreeMap::new())
            .await
            .expect("unexpected transport errors are a run report");

        assert!(!report.success);
        assert!(!report.requests[0].assertions[0].success);
        assert!(report.requests[0]
            .error
            .as_deref()
            .unwrap()
            .contains("network"));
    }

    #[tokio::test]
    async fn redirect_assertion_counts_only_redirect_responses() {
        use postman_http::response::RedirectHop;

        let file = parse_http_file(
            "# @assert status == 200\n# @assert redirects == 2\nGET https://example.com/start\n",
        )
        .expect("redirect assertion should parse");
        let response = HttpResponse::new(200, Vec::new(), String::new()).with_redirect_chain(vec![
            RedirectHop::new(302, "https://example.com/start", Some("/middle")),
            RedirectHop::new(307, "https://example.com/middle", Some("/end")),
            RedirectHop::terminal(200, "https://example.com/end"),
        ]);
        let transport = FakeTransport::new([Ok(response)]);

        let report = HeadlessRunner::new(transport)
            .run(&file, &BTreeMap::new())
            .await
            .expect("redirect assertion should compile");

        assert!(report.success, "{report:#?}");
    }

    #[tokio::test]
    async fn header_existence_assertion_is_case_insensitive() {
        let file =
            parse_http_file("# @assert header \"etag\" exists\nGET https://example.com/resource\n")
                .expect("header assertion should parse");
        let transport = FakeTransport::new([Ok(HttpResponse::new(
            200,
            vec![("ETag".to_owned(), "dynamic-value".to_owned())],
            String::new(),
        ))]);

        let report = HeadlessRunner::new(transport)
            .run(&file, &BTreeMap::new())
            .await
            .expect("header assertion should compile");

        assert!(report.success, "{report:#?}");
    }
}

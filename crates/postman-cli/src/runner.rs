use std::collections::BTreeMap;

use postman_http::{
    request::{Request, RequestBody, RequestOptions},
    HttpError, HttpResponse, HttpTransport,
};
use serde::Serialize;
use serde_json::Value;

use crate::{Assertion, Capture, ExpectedError, HttpFile, HttpFileRequest, RequestOptionOverrides};

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
    T: HttpTransport,
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

    pub async fn run(&self, file: &HttpFile, overrides: &BTreeMap<String, String>) -> RunReport {
        let mut variables = file.variables.clone();
        variables.extend(overrides.clone());
        let mut requests = Vec::with_capacity(file.requests.len());

        for request_spec in &file.requests {
            let request = match prepare_request(request_spec, &variables) {
                Ok(request) => request,
                Err(error) => {
                    requests.push(failed_request_report(&request_spec.name, None, None, error));
                    break;
                }
            };

            let options = merge_request_options(self.options, request_spec.options);
            let response = match self.transport.execute(request, options).await {
                Ok(response) => response,
                Err(error) => {
                    let assertion_reports = request_spec
                        .assertions
                        .iter()
                        .map(|assertion| evaluate_error_assertion(assertion, &error))
                        .collect::<Vec<_>>();
                    let has_expected_error = request_spec
                        .assertions
                        .iter()
                        .any(|assertion| matches!(assertion, Assertion::ErrorEquals(_)));
                    let success = has_expected_error
                        && request_spec.captures.is_empty()
                        && assertion_reports.iter().all(|assertion| assertion.success);
                    requests.push(RequestReport {
                        name: request_spec.name.clone(),
                        success,
                        status: None,
                        elapsed_ms: None,
                        assertions: assertion_reports,
                        captures: Vec::new(),
                        error: (!success).then(|| redact(&error.to_string(), &variables)),
                    });
                    if success {
                        continue;
                    }
                    break;
                }
            };

            let assertion_reports = request_spec
                .assertions
                .iter()
                .map(|assertion| evaluate_assertion(assertion, &response, &variables))
                .collect::<Vec<_>>();
            let assertion_failure = assertion_reports.iter().any(|assertion| !assertion.success);

            let mut captured_values = Vec::with_capacity(request_spec.captures.len());
            let mut capture_names = Vec::with_capacity(request_spec.captures.len());
            let mut capture_error = None;
            for capture in &request_spec.captures {
                match extract_capture(capture, &response) {
                    Ok(value) => {
                        capture_names.push(capture.name.clone());
                        captured_values.push((capture.name.clone(), value));
                    }
                    Err(error) => {
                        capture_error = Some(error);
                        break;
                    }
                }
            }

            let success = !assertion_failure && capture_error.is_none();
            let report = RequestReport {
                name: request_spec.name.clone(),
                success,
                status: Some(response.status),
                elapsed_ms: Some(response.elapsed_ms),
                assertions: assertion_reports,
                captures: capture_names,
                error: capture_error,
            };
            requests.push(report);

            if !success {
                break;
            }
            variables.extend(captured_values);
        }

        RunReport {
            success: requests.len() == file.requests.len()
                && requests.iter().all(|request| request.success),
            requests,
        }
    }
}

fn merge_request_options(
    defaults: RequestOptions,
    overrides: RequestOptionOverrides,
) -> RequestOptions {
    RequestOptions {
        timeout_ms: overrides.timeout_ms.or(defaults.timeout_ms),
        redirect_policy: overrides
            .redirect_policy
            .unwrap_or(defaults.redirect_policy),
        max_redirect_hops: overrides
            .max_redirect_hops
            .unwrap_or(defaults.max_redirect_hops),
    }
}

fn prepare_request(
    request_spec: &HttpFileRequest,
    variables: &BTreeMap<String, String>,
) -> Result<Request, String> {
    let mut request = Request::new(
        request_spec.method,
        render_template(&request_spec.url, variables)?,
    );
    request.headers = request_spec
        .headers
        .iter()
        .map(|(name, value)| {
            Ok((
                render_template(name, variables)?,
                render_template(value, variables)?,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    request.body = match &request_spec.body {
        RequestBody::None => RequestBody::None,
        RequestBody::Json(body) => RequestBody::Json(render_template(body, variables)?),
        RequestBody::Raw(body) => RequestBody::Raw(render_template(body, variables)?),
        RequestBody::UrlEncoded(body) => RequestBody::UrlEncoded(render_template(body, variables)?),
        RequestBody::Multipart(_) => {
            return Err(
                "parsed multipart parts are not supported by the first headless slice".into(),
            )
        }
    };
    Ok(request)
}

fn evaluate_assertion(
    assertion: &Assertion,
    response: &HttpResponse,
    variables: &BTreeMap<String, String>,
) -> AssertionReport {
    match assertion {
        Assertion::StatusEquals(expected) => assertion_report(
            format!("status == {expected}"),
            response.status == *expected,
            "response status did not equal the expected status",
        ),
        Assertion::RedirectsEquals(expected) => assertion_report(
            format!("redirects == {expected}"),
            redirect_count(response) == *expected,
            "response redirect count did not equal the expected count",
        ),
        Assertion::ErrorEquals(expected) => failed_assertion(
            format!("error == {}", expected_error_name(*expected)),
            "request completed without the expected transport error".to_owned(),
        ),
        Assertion::HeaderExists { name } => assertion_report(
            format!("header \"{name}\" exists"),
            response
                .headers
                .iter()
                .any(|(actual_name, _)| actual_name.eq_ignore_ascii_case(name)),
            "response header was not present",
        ),
        Assertion::HeaderContains { name, expected } => {
            let expression = format!("header \"{name}\" contains expected value");
            let expected = match render_expected_string(expected, variables) {
                Ok(expected) => expected,
                Err(error) => return failed_assertion(expression, error),
            };
            let matches = response.headers.iter().any(|(actual_name, value)| {
                actual_name.eq_ignore_ascii_case(name) && value.contains(&expected)
            });
            assertion_report(
                expression,
                matches,
                "response header did not contain the expected value",
            )
        }
        Assertion::BodyContains { expected } => {
            let expression = "body contains expected value".to_owned();
            let expected = match render_expected_string(expected, variables) {
                Ok(expected) => expected,
                Err(error) => return failed_assertion(expression, error),
            };
            assertion_report(
                expression,
                response.body.contains(&expected),
                "response body did not contain the expected value",
            )
        }
        Assertion::JsonPathEquals { path, expected } => {
            let expression = format!("jsonpath \"{path}\" equals expected value");
            let expected = match render_template(expected, variables) {
                Ok(expected) => parse_expected_json(&expected),
                Err(error) => return failed_assertion(expression, error),
            };
            let body = match serde_json::from_str::<Value>(&response.body) {
                Ok(body) => body,
                Err(_) => {
                    return failed_assertion(
                        expression,
                        "response body is not valid JSON".to_owned(),
                    )
                }
            };
            let actual = match json_path(&body, path) {
                Ok(actual) => actual,
                Err(error) => return failed_assertion(expression, error),
            };
            assertion_report(
                expression,
                actual == &expected,
                "JSONPath value did not equal the expected value",
            )
        }
    }
}

fn evaluate_error_assertion(assertion: &Assertion, error: &HttpError) -> AssertionReport {
    match assertion {
        Assertion::ErrorEquals(expected) => assertion_report(
            format!("error == {}", expected_error_name(*expected)),
            expected_error_matches(*expected, error),
            "transport error kind did not equal the expected error kind",
        ),
        assertion => failed_assertion(
            assertion_expression(assertion),
            "request failed before this response assertion could be evaluated".to_owned(),
        ),
    }
}

fn assertion_expression(assertion: &Assertion) -> String {
    match assertion {
        Assertion::StatusEquals(expected) => format!("status == {expected}"),
        Assertion::RedirectsEquals(expected) => format!("redirects == {expected}"),
        Assertion::ErrorEquals(expected) => format!("error == {}", expected_error_name(*expected)),
        Assertion::HeaderExists { name } => format!("header \"{name}\" exists"),
        Assertion::HeaderContains { name, .. } => {
            format!("header \"{name}\" contains expected value")
        }
        Assertion::BodyContains { .. } => "body contains expected value".to_owned(),
        Assertion::JsonPathEquals { path, .. } => {
            format!("jsonpath \"{path}\" equals expected value")
        }
    }
}

fn expected_error_matches(expected: ExpectedError, actual: &HttpError) -> bool {
    matches!(
        (expected, actual),
        (ExpectedError::Timeout, HttpError::Timeout { .. })
            | (
                ExpectedError::RedirectLimit,
                HttpError::RedirectLimitExceeded { .. }
            )
            | (ExpectedError::Network, HttpError::Network(_))
            | (
                ExpectedError::InvalidRequest,
                HttpError::EmptyUrl | HttpError::InvalidRequest(_)
            )
            | (
                ExpectedError::InvalidResponse,
                HttpError::InvalidResponse(_)
            )
            | (
                ExpectedError::ResponseTooLarge,
                HttpError::ResponseTooLarge { .. }
            )
            | (ExpectedError::Cancelled, HttpError::Cancelled)
    )
}

fn expected_error_name(error: ExpectedError) -> &'static str {
    match error {
        ExpectedError::Timeout => "timeout",
        ExpectedError::RedirectLimit => "redirect-limit",
        ExpectedError::Network => "network",
        ExpectedError::InvalidRequest => "invalid-request",
        ExpectedError::InvalidResponse => "invalid-response",
        ExpectedError::ResponseTooLarge => "response-too-large",
        ExpectedError::Cancelled => "cancelled",
    }
}

fn redirect_count(response: &HttpResponse) -> usize {
    response
        .redirect_chain
        .iter()
        .filter(|hop| (300..400).contains(&hop.status) && hop.location.is_some())
        .count()
}

fn extract_capture(capture: &Capture, response: &HttpResponse) -> Result<String, String> {
    let body = serde_json::from_str::<Value>(&response.body)
        .map_err(|_| format!("capture `{}` requires a JSON response body", capture.name))?;
    let value = json_path(&body, &capture.path).map_err(|error| {
        format!(
            "capture `{}` could not resolve {}: {error}",
            capture.name, capture.path
        )
    })?;
    Ok(match value {
        Value::String(value) => value.clone(),
        value => value.to_string(),
    })
}

fn render_template(template: &str, variables: &BTreeMap<String, String>) -> Result<String, String> {
    let mut rendered = template.to_owned();
    for _ in 0..16 {
        let (next, replaced) = render_template_pass(&rendered, variables)?;
        if !replaced {
            return Ok(rendered);
        }
        if next == rendered {
            return Err("variable interpolation contains a cycle".to_owned());
        }
        rendered = next;
    }
    Err("variable interpolation exceeded 16 expansions (possible cycle)".to_owned())
}

fn render_template_pass(
    template: &str,
    variables: &BTreeMap<String, String>,
) -> Result<(String, bool), String> {
    let mut rendered = String::with_capacity(template.len());
    let mut remaining = template;
    let mut replaced = false;

    while let Some(open) = remaining.find("{{") {
        rendered.push_str(&remaining[..open]);
        let expression = &remaining[open + 2..];
        let close = expression
            .find("}}")
            .ok_or_else(|| "unclosed variable expression".to_owned())?;
        let name = expression[..close].trim();
        if name.is_empty() {
            return Err("empty variable expression".to_owned());
        }
        let value = variables
            .get(name)
            .ok_or_else(|| format!("missing variable `{name}`"))?;
        rendered.push_str(value);
        remaining = &expression[close + 2..];
        replaced = true;
    }
    rendered.push_str(remaining);

    Ok((rendered, replaced))
}

fn render_expected_string(
    expected: &str,
    variables: &BTreeMap<String, String>,
) -> Result<String, String> {
    let rendered = render_template(expected, variables)?;
    Ok(match serde_json::from_str::<Value>(&rendered) {
        Ok(Value::String(value)) => value,
        Ok(value) => value.to_string(),
        Err(_) => rendered,
    })
}

fn parse_expected_json(expected: &str) -> Value {
    serde_json::from_str(expected).unwrap_or_else(|_| Value::String(expected.to_owned()))
}

fn json_path<'value>(root: &'value Value, path: &str) -> Result<&'value Value, String> {
    let mut remaining = path
        .strip_prefix('$')
        .ok_or_else(|| format!("JSONPath `{path}` must start with `$`"))?;
    let mut current = root;

    while !remaining.is_empty() {
        if let Some(after_dot) = remaining.strip_prefix('.') {
            let end = after_dot.find(['.', '[']).unwrap_or(after_dot.len());
            let key = &after_dot[..end];
            if key.is_empty() {
                return Err(format!("JSONPath `{path}` contains an empty object key"));
            }
            current = current
                .get(key)
                .ok_or_else(|| format!("JSONPath `{path}` did not find object key `{key}`"))?;
            remaining = &after_dot[end..];
            continue;
        }

        if let Some(after_bracket) = remaining.strip_prefix('[') {
            let end = after_bracket
                .find(']')
                .ok_or_else(|| format!("JSONPath `{path}` contains an unterminated array index"))?;
            let index = after_bracket[..end]
                .parse::<usize>()
                .map_err(|_| format!("JSONPath `{path}` contains a non-numeric array index"))?;
            current = current
                .get(index)
                .ok_or_else(|| format!("JSONPath `{path}` did not find array index {index}"))?;
            remaining = &after_bracket[end + 1..];
            continue;
        }

        return Err(format!("unsupported JSONPath syntax in `{path}`"));
    }

    Ok(current)
}

fn assertion_report(expression: String, success: bool, failure_message: &str) -> AssertionReport {
    AssertionReport {
        expression,
        success,
        message: (!success).then(|| failure_message.to_owned()),
    }
}

fn failed_assertion(expression: String, message: String) -> AssertionReport {
    AssertionReport {
        expression,
        success: false,
        message: Some(message),
    }
}

fn failed_request_report(
    name: &str,
    status: Option<u16>,
    elapsed_ms: Option<u128>,
    error: String,
) -> RequestReport {
    RequestReport {
        name: name.to_owned(),
        success: false,
        status,
        elapsed_ms,
        assertions: Vec::new(),
        captures: Vec::new(),
        error: Some(error),
    }
}

fn redact(message: &str, variables: &BTreeMap<String, String>) -> String {
    variables
        .values()
        .filter(|value| value.len() >= 3)
        .fold(message.to_owned(), |redacted, value| {
            redacted.replace(value, "[REDACTED]")
        })
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    use postman_http::HttpError;

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
            .await;

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
            .await;

        assert!(!report.success);
        assert_eq!(report.requests.len(), 1);
        assert_eq!(transport.requests().len(), 1);
        assert!(!report.requests[0].assertions[0].success);
    }

    #[tokio::test]
    async fn transport_errors_are_redacted_with_known_variable_values() {
        let file = parse_http_file(
            "@token = top-secret-token\n### request\nGET https://example.com/{{token}}\n",
        )
        .expect("the redaction fixture should parse");
        let transport = FakeTransport::new([Err(HttpError::network(
            "failed to request https://example.com/top-secret-token",
        ))]);

        let report = HeadlessRunner::new(transport)
            .run(&file, &BTreeMap::new())
            .await;
        let serialized = serde_json::to_string(&report).expect("the report should serialize");

        assert!(!report.success);
        assert!(!serialized.contains("top-secret-token"));
        assert!(serialized.contains("[REDACTED]"));
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
            .await;

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
            .await;

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
            .await;

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
            .await;

        assert!(report.success, "{report:#?}");
    }

    #[test]
    fn one_interpolation_pass_replaces_more_than_sixteen_occurrences() {
        let variables = BTreeMap::from([("value".to_owned(), "x".to_owned())]);
        let template = "{{value}}".repeat(32);

        let rendered = render_template(&template, &variables)
            .expect("the expansion limit should measure nesting, not occurrence count");

        assert_eq!(rendered, "x".repeat(32));
    }

    #[test]
    fn cyclic_variable_expansion_returns_an_actionable_error() {
        let variables = BTreeMap::from([("cycle".to_owned(), "{{cycle}}".to_owned())]);

        let error = render_template("{{cycle}}", &variables)
            .expect_err("a self-referential variable must not loop forever");

        assert!(error.contains("cycle"));
    }
}

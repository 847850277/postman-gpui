use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    fmt,
    pin::Pin,
};

use futures::{stream, Stream};
use postman_http::{
    request::{Request, RequestBody, RequestOptions},
    response::HttpResponse,
    HttpTransport,
};
use serde_json::Value;

use crate::model::{
    BodyTemplate, FlowEvent, FlowInputs, FlowPlan, ResponseCheck, ResponseExport, StepOutcome,
    TemplatePart, TextTemplate,
};

/// A pull-based stream of observable Flow progress.
pub type FlowEventStream = Pin<Box<dyn Stream<Item = Result<FlowEvent, FlowError>> + Send>>;

/// Stable dependencies and policies shared by every step in one run.
pub struct FlowSessionEnvironment<T> {
    pub transport: T,
    pub request_options: RequestOptions,
}

impl<T> FlowSessionEnvironment<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            request_options: RequestOptions::default(),
        }
    }

    pub fn with_request_options(mut self, request_options: RequestOptions) -> Self {
        self.request_options = request_options;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowError {
    InvalidPlan(String),
    InvalidInputs(String),
}

impl fmt::Display for FlowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPlan(message) => write!(formatter, "invalid Flow plan: {message}"),
            Self::InvalidInputs(message) => write!(formatter, "invalid Flow inputs: {message}"),
        }
    }
}

impl std::error::Error for FlowError {}

/// Validates and binds the plan before returning a lazy event stream.
///
/// Transport failures and failed response checks are normal run outcomes, so they are represented
/// by [`FlowEvent::StepFinished`] and [`FlowEvent::FlowFinished`] rather than stream errors.
pub fn execute_flow<T>(
    plan: FlowPlan,
    inputs: FlowInputs,
    environment: FlowSessionEnvironment<T>,
) -> Result<FlowEventStream, FlowError>
where
    T: HttpTransport + 'static,
{
    let context = validate_and_bind(&plan, &inputs)?;
    let machine = RunMachine::new(plan, environment, context);

    Ok(Box::pin(stream::unfold(
        machine,
        |mut machine| async move {
            let event = machine.next_event().await?;
            Some((event, machine))
        },
    )))
}

struct RunMachine<T> {
    plan: FlowPlan,
    environment: FlowSessionEnvironment<T>,
    context: RunContext,
    step_index: usize,
    phase: RunPhase,
    pending: VecDeque<FlowEvent>,
    success: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunPhase {
    FlowStart,
    StepStart,
    ExecuteStep,
    FlowFinish,
    Done,
}

impl<T: HttpTransport> RunMachine<T> {
    fn new(plan: FlowPlan, environment: FlowSessionEnvironment<T>, context: RunContext) -> Self {
        Self {
            plan,
            environment,
            context,
            step_index: 0,
            phase: RunPhase::FlowStart,
            pending: VecDeque::new(),
            success: true,
        }
    }

    async fn next_event(&mut self) -> Option<Result<FlowEvent, FlowError>> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                return Some(Ok(event));
            }

            match self.phase {
                RunPhase::FlowStart => {
                    self.phase = RunPhase::StepStart;
                    return Some(Ok(FlowEvent::FlowStarted {
                        name: self.plan.name.clone(),
                        total_steps: self.plan.steps.len(),
                    }));
                }
                RunPhase::StepStart => {
                    let step = &self.plan.steps[self.step_index];
                    self.phase = RunPhase::ExecuteStep;
                    return Some(Ok(FlowEvent::StepStarted {
                        step_id: step.id.clone(),
                        name: step.name.clone(),
                    }));
                }
                RunPhase::ExecuteStep => self.execute_current_step().await,
                RunPhase::FlowFinish => {
                    self.phase = RunPhase::Done;
                    return Some(Ok(FlowEvent::FlowFinished {
                        success: self.success,
                    }));
                }
                RunPhase::Done => return None,
            }
        }
    }

    async fn execute_current_step(&mut self) {
        let step = self.plan.steps[self.step_index].clone();
        let request = match prepare_request(&step, &self.context) {
            Ok(request) => request,
            Err(message) => {
                self.fail_step(&step.id, message);
                return;
            }
        };

        let response = match self
            .environment
            .transport
            .execute(request, self.environment.request_options)
            .await
        {
            Ok(response) => response,
            Err(error) => {
                self.fail_step(&step.id, error.to_string());
                return;
            }
        };

        self.pending.push_back(FlowEvent::ResponseReceived {
            step_id: step.id.clone(),
            status: response.status,
            elapsed_ms: response.elapsed_ms,
        });

        let mut checks_succeeded = true;
        for check in &step.checks {
            let result = evaluate_check(check, &response, &self.context);
            checks_succeeded &= result.success;
            self.pending.push_back(FlowEvent::CheckFinished {
                step_id: step.id.clone(),
                check: result.description,
                success: result.success,
                message: result.message.map(|message| self.context.redact(&message)),
            });
        }

        if !checks_succeeded {
            self.fail_step(&step.id, "one or more response checks failed".to_owned());
            return;
        }

        let mut staged_outputs = Vec::with_capacity(step.exports.len());
        for export in &step.exports {
            match extract_output(export, &response) {
                Ok(value) => staged_outputs.push((export, value)),
                Err(message) => {
                    self.fail_step(&step.id, message);
                    return;
                }
            }
        }

        for (export, value) in staged_outputs {
            self.context.outputs.insert(
                (step.id.clone(), export.name.clone()),
                BoundValue {
                    value,
                    sensitive: export.sensitive,
                },
            );
            self.pending.push_back(FlowEvent::OutputExported {
                step_id: step.id.clone(),
                name: export.name.clone(),
            });
        }

        self.pending.push_back(FlowEvent::StepFinished {
            step_id: step.id,
            outcome: StepOutcome::Succeeded,
        });
        self.step_index += 1;
        self.phase = if self.step_index < self.plan.steps.len() {
            RunPhase::StepStart
        } else {
            RunPhase::FlowFinish
        };
    }

    fn fail_step(&mut self, step_id: &str, message: String) {
        self.success = false;
        self.phase = RunPhase::FlowFinish;
        self.pending.push_back(FlowEvent::StepFinished {
            step_id: step_id.to_owned(),
            outcome: StepOutcome::Failed {
                message: self.context.redact(&message),
            },
        });
    }
}

fn prepare_request(
    step: &crate::model::HttpStepPlan,
    context: &RunContext,
) -> Result<Request, String> {
    let url = context.render(&step.url)?;
    if url.trim().is_empty() {
        return Err("rendered request URL is empty".to_owned());
    }

    let mut request = Request::new(step.method, url);
    for (name, value) in &step.headers {
        request.add_header(context.render(name)?, context.render(value)?);
    }
    request.body = step.body.render(|template| context.render(template))?;

    if let RequestBody::Json(body) = &request.body {
        serde_json::from_str::<Value>(body)
            .map_err(|error| format!("rendered JSON request body is invalid: {error}"))?;
    }

    Ok(request)
}

struct CheckResult {
    description: String,
    success: bool,
    message: Option<String>,
}

fn evaluate_check(
    check: &ResponseCheck,
    response: &HttpResponse,
    context: &RunContext,
) -> CheckResult {
    match check {
        ResponseCheck::StatusEquals(expected) => {
            let success = response.status == *expected;
            CheckResult {
                description: format!("status == {expected}"),
                success,
                message: (!success).then(|| {
                    format!(
                        "response status was {}, expected {expected}",
                        response.status
                    )
                }),
            }
        }
        ResponseCheck::JsonPathEquals { path, expected } => {
            let description = format!("jsonpath \"{path}\" == expected value");
            let expected = match context.resolve_expected(expected) {
                Ok(expected) => expected,
                Err(message) => {
                    return CheckResult {
                        description,
                        success: false,
                        message: Some(message),
                    };
                }
            };
            let body = match serde_json::from_str::<Value>(&response.body) {
                Ok(body) => body,
                Err(_) => {
                    return CheckResult {
                        description,
                        success: false,
                        message: Some("response body is not valid JSON".to_owned()),
                    };
                }
            };
            let actual = match resolve_json_path(&body, path) {
                Ok(actual) => actual,
                Err(message) => {
                    return CheckResult {
                        description,
                        success: false,
                        message: Some(message),
                    };
                }
            };
            let success = actual == &expected;
            CheckResult {
                description,
                success,
                message: (!success)
                    .then(|| "JSONPath value did not equal the expected value".to_owned()),
            }
        }
    }
}

fn extract_output(export: &ResponseExport, response: &HttpResponse) -> Result<Value, String> {
    let body = serde_json::from_str::<Value>(&response.body)
        .map_err(|_| format!("output `{}` requires a JSON response body", export.name))?;
    resolve_json_path(&body, &export.json_path)
        .cloned()
        .map_err(|message| format!("output `{}` could not be exported: {message}", export.name))
}

#[derive(Debug, Clone)]
struct BoundValue {
    value: Value,
    sensitive: bool,
}

#[derive(Debug, Clone)]
struct RunContext {
    inputs: BTreeMap<String, BoundValue>,
    outputs: BTreeMap<(String, String), BoundValue>,
}

impl RunContext {
    fn render(&self, template: &TextTemplate) -> Result<String, String> {
        let mut rendered = String::new();
        for part in &template.parts {
            match part {
                TemplatePart::Literal(value) => rendered.push_str(value),
                TemplatePart::Input(name) => {
                    let value = self
                        .inputs
                        .get(name)
                        .ok_or_else(|| format!("input `{name}` is not bound"))?;
                    rendered.push_str(&value_as_text(&value.value));
                }
                TemplatePart::StepOutput { step_id, name } => {
                    let value = self
                        .outputs
                        .get(&(step_id.clone(), name.clone()))
                        .ok_or_else(|| {
                            format!("output `{step_id}.{name}` is not available at runtime")
                        })?;
                    rendered.push_str(&value_as_text(&value.value));
                }
            }
        }
        Ok(rendered)
    }

    fn resolve_expected(&self, template: &TextTemplate) -> Result<Value, String> {
        match template.parts.as_slice() {
            [TemplatePart::Input(name)] => self
                .inputs
                .get(name)
                .map(|value| value.value.clone())
                .ok_or_else(|| format!("input `{name}` is not bound")),
            [TemplatePart::StepOutput { step_id, name }] => self
                .outputs
                .get(&(step_id.clone(), name.clone()))
                .map(|value| value.value.clone())
                .ok_or_else(|| format!("output `{step_id}.{name}` is not available at runtime")),
            _ => {
                let rendered = self.render(template)?;
                Ok(serde_json::from_str(&rendered)
                    .unwrap_or_else(|_| Value::String(rendered.to_owned())))
            }
        }
    }

    fn redact(&self, message: &str) -> String {
        let mut secrets = self
            .inputs
            .values()
            .chain(self.outputs.values())
            .filter(|value| value.sensitive)
            .map(|value| value_as_text(&value.value))
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        secrets.sort_by_key(|value| std::cmp::Reverse(value.len()));
        secrets.dedup();

        secrets
            .into_iter()
            .fold(message.to_owned(), |redacted, secret| {
                redacted.replace(&secret, "[REDACTED]")
            })
    }
}

fn value_as_text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        value => value.to_string(),
    }
}

fn validate_and_bind(plan: &FlowPlan, provided: &FlowInputs) -> Result<RunContext, FlowError> {
    if plan.name.trim().is_empty() {
        return Err(FlowError::InvalidPlan(
            "flow name cannot be empty".to_owned(),
        ));
    }
    if plan.steps.is_empty() {
        return Err(FlowError::InvalidPlan(
            "flow must contain at least one HTTP step".to_owned(),
        ));
    }

    let mut input_names = HashSet::new();
    for input in &plan.inputs {
        if input.name.trim().is_empty() {
            return Err(FlowError::InvalidPlan(
                "input name cannot be empty".to_owned(),
            ));
        }
        if !input_names.insert(input.name.clone()) {
            return Err(FlowError::InvalidPlan(format!(
                "input `{}` is declared more than once",
                input.name
            )));
        }
    }

    for name in provided.values.keys() {
        if !input_names.contains(name) {
            return Err(FlowError::InvalidInputs(format!(
                "input `{name}` is not declared by the plan"
            )));
        }
    }

    let mut inputs = BTreeMap::new();
    for input in &plan.inputs {
        let value = provided
            .values
            .get(&input.name)
            .cloned()
            .or_else(|| input.default.clone())
            .ok_or_else(|| {
                FlowError::InvalidInputs(format!("required input `{}` is missing", input.name))
            })?;
        inputs.insert(
            input.name.clone(),
            BoundValue {
                value,
                sensitive: input.sensitive,
            },
        );
    }

    let mut step_ids = HashSet::new();
    for step in &plan.steps {
        if step.id.trim().is_empty() {
            return Err(FlowError::InvalidPlan("step id cannot be empty".to_owned()));
        }
        if step.name.trim().is_empty() {
            return Err(FlowError::InvalidPlan(format!(
                "step `{}` has an empty display name",
                step.id
            )));
        }
        if !step_ids.insert(step.id.clone()) {
            return Err(FlowError::InvalidPlan(format!(
                "step id `{}` is used more than once",
                step.id
            )));
        }
    }

    let mut available_outputs = HashSet::new();
    for step in &plan.steps {
        validate_template(&step.url, &input_names, &available_outputs)?;
        for (name, value) in &step.headers {
            validate_template(name, &input_names, &available_outputs)?;
            validate_template(value, &input_names, &available_outputs)?;
        }
        match &step.body {
            BodyTemplate::None => {}
            BodyTemplate::Json(template)
            | BodyTemplate::Raw(template)
            | BodyTemplate::UrlEncoded(template) => {
                validate_template(template, &input_names, &available_outputs)?;
            }
        }
        for check in &step.checks {
            if let ResponseCheck::JsonPathEquals { path, expected } = check {
                parse_json_path(path).map_err(FlowError::InvalidPlan)?;
                validate_template(expected, &input_names, &available_outputs)?;
            }
        }

        let mut output_names = HashSet::new();
        for export in &step.exports {
            if export.name.trim().is_empty() {
                return Err(FlowError::InvalidPlan(format!(
                    "step `{}` has an empty output name",
                    step.id
                )));
            }
            if !output_names.insert(export.name.clone()) {
                return Err(FlowError::InvalidPlan(format!(
                    "step `{}` exports `{}` more than once",
                    step.id, export.name
                )));
            }
            parse_json_path(&export.json_path).map_err(FlowError::InvalidPlan)?;
        }
        for export in &step.exports {
            available_outputs.insert((step.id.clone(), export.name.clone()));
        }
    }

    Ok(RunContext {
        inputs,
        outputs: BTreeMap::new(),
    })
}

fn validate_template(
    template: &TextTemplate,
    input_names: &HashSet<String>,
    available_outputs: &HashSet<(String, String)>,
) -> Result<(), FlowError> {
    for part in &template.parts {
        match part {
            TemplatePart::Literal(_) => {}
            TemplatePart::Input(name) if input_names.contains(name) => {}
            TemplatePart::Input(name) => {
                return Err(FlowError::InvalidPlan(format!(
                    "template references undeclared input `{name}`"
                )));
            }
            TemplatePart::StepOutput { step_id, name }
                if available_outputs.contains(&(step_id.clone(), name.clone())) => {}
            TemplatePart::StepOutput { step_id, name } => {
                return Err(FlowError::InvalidPlan(format!(
                    "template output reference `{step_id}.{name}` must point to an earlier step"
                )));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum JsonPathSegment {
    Key(String),
    Index(usize),
}

fn parse_json_path(path: &str) -> Result<Vec<JsonPathSegment>, String> {
    let mut remaining = path
        .strip_prefix('$')
        .ok_or_else(|| format!("JSONPath `{path}` must start with `$`"))?;
    let mut segments = Vec::new();

    while !remaining.is_empty() {
        if let Some(after_dot) = remaining.strip_prefix('.') {
            let end = after_dot.find(['.', '[']).unwrap_or(after_dot.len());
            let key = &after_dot[..end];
            if key.is_empty() {
                return Err(format!("JSONPath `{path}` contains an empty object key"));
            }
            segments.push(JsonPathSegment::Key(key.to_owned()));
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
            segments.push(JsonPathSegment::Index(index));
            remaining = &after_bracket[end + 1..];
            continue;
        }

        return Err(format!("unsupported JSONPath syntax in `{path}`"));
    }

    Ok(segments)
}

fn resolve_json_path<'value>(root: &'value Value, path: &str) -> Result<&'value Value, String> {
    let mut current = root;
    for segment in parse_json_path(path)? {
        current = match segment {
            JsonPathSegment::Key(key) => current
                .get(key.as_str())
                .ok_or_else(|| format!("JSONPath `{path}` did not find object key `{key}`"))?,
            JsonPathSegment::Index(index) => current
                .get(index)
                .ok_or_else(|| format!("JSONPath `{path}` did not find array index {index}"))?,
        };
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use futures::StreamExt;
    use postman_http::{
        request::{HttpMethod, Request, RequestBody, RequestOptions},
        HttpError, HttpResponse,
    };
    use postman_request::RequestClient;

    use super::*;
    use crate::model::{FlowInputSpec, HttpStepPlan};

    #[derive(Clone)]
    struct FakeTransport {
        state: Arc<Mutex<FakeState>>,
    }

    struct FakeState {
        responses: VecDeque<Result<HttpResponse, HttpError>>,
        requests: Vec<Request>,
    }

    impl FakeTransport {
        fn new(responses: impl IntoIterator<Item = Result<HttpResponse, HttpError>>) -> Self {
            Self {
                state: Arc::new(Mutex::new(FakeState {
                    responses: responses.into_iter().collect(),
                    requests: Vec::new(),
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
    }

    impl HttpTransport for FakeTransport {
        async fn execute(
            &self,
            request: Request,
            _options: RequestOptions,
        ) -> Result<HttpResponse, HttpError> {
            let mut state = self
                .state
                .lock()
                .expect("fake transport state should not be poisoned");
            state.requests.push(request);
            state
                .responses
                .pop_front()
                .expect("fake transport must have one response per request")
        }
    }

    #[tokio::test]
    async fn an_exported_value_is_typed_and_available_to_the_next_step() {
        let transport = FakeTransport::new([
            Ok(HttpResponse::new(
                200,
                Vec::new(),
                r#"{"uuid":"flow-123"}"#.to_owned(),
            )),
            Ok(HttpResponse::new(
                200,
                Vec::new(),
                r#"{"method":"POST","json":{"client":"postman-flow","correlation_id":"flow-123"}}"#
                    .to_owned(),
            )),
        ]);
        let mut stream = execute_flow(
            two_step_plan(),
            FlowInputs::new(),
            FlowSessionEnvironment::new(transport.clone()),
        )
        .expect("the plan should validate");

        let events = stream
            .by_ref()
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .expect("the event stream should not fail");

        assert!(events.contains(&FlowEvent::OutputExported {
            step_id: "seed".to_owned(),
            name: "correlation_id".to_owned(),
        }));
        assert_eq!(
            events.last(),
            Some(&FlowEvent::FlowFinished { success: true })
        );

        let requests = transport.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[1].url,
            "https://example.com/anything/headless-e2e/flow-123"
        );
        assert_eq!(
            requests[1].body,
            RequestBody::Json(
                r#"{"client":"postman-flow","correlation_id":"flow-123"}"#.to_owned()
            )
        );
    }

    /// Opt-in live contract test. Keeping it ignored prevents ordinary unit tests from depending
    /// on DNS, TLS, or the availability of a public service.
    #[tokio::test]
    //#[ignore = "requires live network access to https://httpbingo.org"]
    async fn live_httpbingo_flow_reuses_first_response_in_second_request() {
        let transport = RequestClient::try_new("postman-flow-live-test/0.1.0")
            .expect("the live test HTTP client should initialize");
        let environment =
            FlowSessionEnvironment::new(transport).with_request_options(RequestOptions {
                timeout_ms: Some(15_000),
                ..RequestOptions::default()
            });
        let inputs = FlowInputs::new()
            .with("host", "https://httpbingo.org")
            .with("client", "postman-flow-live-test");
        let stream = execute_flow(two_step_plan(), inputs, environment)
            .expect("the live HTTPBingo plan should validate");

        let events = stream
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .expect("the event stream should not fail");
        for event in &events {
            println!("{event:?}");
        }

        assert!(events.contains(&FlowEvent::OutputExported {
            step_id: "seed".to_owned(),
            name: "correlation_id".to_owned(),
        }));
        assert!(events
            .iter()
            .all(|event| !matches!(event, FlowEvent::CheckFinished { success: false, .. })));
        assert_eq!(
            events.last(),
            Some(&FlowEvent::FlowFinished { success: true })
        );
    }

    #[tokio::test]
    async fn a_failed_check_stops_the_sequence() {
        let transport = FakeTransport::new([
            Ok(HttpResponse::new(
                500,
                Vec::new(),
                r#"{"uuid":"flow-123"}"#.to_owned(),
            )),
            Ok(HttpResponse::new(200, Vec::new(), String::new())),
        ]);
        let stream = execute_flow(
            two_step_plan(),
            FlowInputs::new(),
            FlowSessionEnvironment::new(transport.clone()),
        )
        .expect("the plan should validate");

        let events = stream
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .expect("the event stream should not fail");

        assert_eq!(transport.requests().len(), 1);
        assert_eq!(
            events.last(),
            Some(&FlowEvent::FlowFinished { success: false })
        );
        assert!(events.iter().any(|event| matches!(
            event,
            FlowEvent::StepFinished {
                step_id,
                outcome: StepOutcome::Failed { .. }
            } if step_id == "seed"
        )));
    }

    #[test]
    fn a_required_input_is_rejected_before_the_stream_starts() {
        let mut plan = two_step_plan();
        plan.inputs.push(FlowInputSpec::required("token"));
        let result = execute_flow(
            plan,
            FlowInputs::new(),
            FlowSessionEnvironment::new(FakeTransport::new([])),
        );

        let error = match result {
            Ok(_) => panic!("missing input must reject the run"),
            Err(error) => error,
        };
        assert_eq!(
            error,
            FlowError::InvalidInputs("required input `token` is missing".to_owned())
        );
    }

    #[test]
    fn a_forward_output_reference_is_rejected_before_the_stream_starts() {
        let plan = FlowPlan {
            name: "invalid-forward-reference".to_owned(),
            inputs: Vec::new(),
            steps: vec![
                HttpStepPlan::new(
                    "first",
                    "First",
                    HttpMethod::GET,
                    TextTemplate::parts([TemplatePart::step_output("second", "id")]),
                ),
                HttpStepPlan::new(
                    "second",
                    "Second",
                    HttpMethod::GET,
                    TextTemplate::literal("https://example.com"),
                )
                .export(ResponseExport::json("id", "$.id")),
            ],
        };
        let result = execute_flow(
            plan,
            FlowInputs::new(),
            FlowSessionEnvironment::new(FakeTransport::new([])),
        );

        let error = match result {
            Ok(_) => panic!("a forward reference must reject the plan"),
            Err(error) => error,
        };
        assert!(
            matches!(error, FlowError::InvalidPlan(message) if message.contains("earlier step"))
        );
    }

    #[tokio::test]
    async fn sensitive_values_are_redacted_from_transport_failures() {
        let transport = FakeTransport::new([Err(HttpError::network(
            "failed to reach https://example.com/top-secret-token",
        ))]);
        let plan = FlowPlan {
            name: "redaction".to_owned(),
            inputs: vec![FlowInputSpec::with_default("token", "top-secret-token").sensitive()],
            steps: vec![HttpStepPlan::new(
                "request",
                "Request",
                HttpMethod::GET,
                TextTemplate::parts([
                    TemplatePart::literal("https://example.com/"),
                    TemplatePart::input("token"),
                ]),
            )],
        };
        let stream = execute_flow(
            plan,
            FlowInputs::new(),
            FlowSessionEnvironment::new(transport),
        )
        .expect("the plan should validate");

        let events = stream
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .expect("the event stream should not fail");
        let debug = format!("{events:?}");

        assert!(!debug.contains("top-secret-token"));
        assert!(debug.contains("[REDACTED]"));
    }

    fn two_step_plan() -> FlowPlan {
        FlowPlan {
            name: "httpbingo-minimal".to_owned(),
            inputs: vec![
                FlowInputSpec::with_default("host", "https://example.com"),
                FlowInputSpec::with_default("client", "postman-flow"),
            ],
            steps: vec![
                HttpStepPlan::new(
                    "seed",
                    "Generate a correlation id",
                    HttpMethod::GET,
                    TextTemplate::parts([
                        TemplatePart::input("host"),
                        TemplatePart::literal("/uuid"),
                    ]),
                )
                .header(
                    TextTemplate::literal("Accept"),
                    TextTemplate::literal("application/json"),
                )
                .check(ResponseCheck::StatusEquals(200))
                .export(ResponseExport::json("correlation_id", "$.uuid")),
                HttpStepPlan::new(
                    "echo",
                    "Reuse the correlation id",
                    HttpMethod::POST,
                    TextTemplate::parts([
                        TemplatePart::input("host"),
                        TemplatePart::literal("/anything/headless-e2e/"),
                        TemplatePart::step_output("seed", "correlation_id"),
                    ]),
                )
                .header(
                    TextTemplate::literal("Accept"),
                    TextTemplate::literal("application/json"),
                )
                .header(
                    TextTemplate::literal("Content-Type"),
                    TextTemplate::literal("application/json"),
                )
                .json_body(TextTemplate::parts([
                    TemplatePart::literal("{\"client\":\""),
                    TemplatePart::input("client"),
                    TemplatePart::literal("\",\"correlation_id\":\""),
                    TemplatePart::step_output("seed", "correlation_id"),
                    TemplatePart::literal("\"}"),
                ]))
                .check(ResponseCheck::StatusEquals(200))
                .check(ResponseCheck::JsonPathEquals {
                    path: "$.json.client".to_owned(),
                    expected: TextTemplate::parts([TemplatePart::input("client")]),
                })
                .check(ResponseCheck::JsonPathEquals {
                    path: "$.json.correlation_id".to_owned(),
                    expected: TextTemplate::parts([TemplatePart::step_output(
                        "seed",
                        "correlation_id",
                    )]),
                }),
            ],
        }
    }
}

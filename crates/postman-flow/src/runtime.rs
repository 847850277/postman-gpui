use std::{
    collections::{BTreeMap, VecDeque},
    fmt,
};

use futures::{stream, Stream};
use postman_http::{
    request::{Request, RequestBody, RequestOptions},
    response::HttpResponse,
    HttpTransport,
};
use serde_json::Value;

use crate::{
    plan::{CompiledCheck, CompiledExport},
    FlowEvent, FlowInputs, FlowOutputs, FlowPlan, FlowValue, HttpRequestTemplate, JsonTemplate,
    StepOutcome, TemplatePart, TextTemplate, ValueReference,
};

/// Stable dependencies and policies shared by every step in one run.
#[derive(Clone, Default)]
pub struct FlowSessionEnvironment {
    pub inputs: FlowInputs,
    pub request_options: RequestOptions,
}

impl FlowSessionEnvironment {
    pub fn new(inputs: FlowInputs) -> Self {
        Self {
            inputs,
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
    InvariantViolation(String),
    InvalidInputs(String),
}

impl fmt::Display for FlowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvariantViolation(message) => {
                write!(formatter, "Flow invariant violation: {message}")
            }
            Self::InvalidInputs(message) => write!(formatter, "invalid Flow inputs: {message}"),
        }
    }
}

impl std::error::Error for FlowError {}

/// Bind session inputs and return a lazy, statically dispatched event stream.
/// No request runs until the stream is polled. Dropping it prevents subsequent steps.
pub fn execute_flow<T: HttpTransport>(
    plan: FlowPlan,
    http_transport: T,
    session_environment: FlowSessionEnvironment,
) -> Result<impl Stream<Item = Result<FlowEvent, FlowError>> + Send, FlowError> {
    let context = bind_inputs(&plan, &session_environment.inputs)?;
    let machine = RunMachine::new(plan, http_transport, session_environment, context);
    Ok(stream::unfold(machine, |mut machine| async move {
        let event = machine.next_event().await?;
        Some((event, machine))
    }))
}

struct RunMachine<T> {
    plan: FlowPlan,
    transport: T,
    environment: FlowSessionEnvironment,
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
    fn new(
        plan: FlowPlan,
        transport: T,
        environment: FlowSessionEnvironment,
        context: RunContext,
    ) -> Self {
        Self {
            plan,
            transport,
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
                    let outputs = if self.success {
                        match self.context.flow_outputs(&self.plan) {
                            Ok(outputs) => outputs,
                            Err(message) => {
                                return Some(Err(FlowError::InvariantViolation(message)))
                            }
                        }
                    } else {
                        FlowOutputs::new()
                    };
                    return Some(Ok(FlowEvent::FlowFinished {
                        success: self.success,
                        outputs,
                    }));
                }
                RunPhase::Done => return None,
            }
        }
    }

    async fn execute_current_step(&mut self) {
        let step = self.plan.steps[self.step_index].clone();
        let request = match prepare_request(&step.request, &self.context) {
            Ok(request) => request,
            Err(message) => {
                self.fail_step(&step.id, message);
                return;
            }
        };

        tracing::info!(
            step_id = %step.id,
            step_name = %step.name,
            method = %request.method,
            url = %self.context.redact(&request.url),
            "executing HTTP step"
        );
        for (name, value) in &request.headers {
            tracing::debug!(
                step_id = %step.id,
                header = %name,
                value = %self.context.redact(value),
                "request header"
            );
        }
        if let Some(body) = request.body.as_text() {
            if !body.is_empty() {
                tracing::debug!(
                    step_id = %step.id,
                    body = %self.context.redact(body),
                    "request body"
                );
            }
        }

        let response = match self
            .transport
            .execute(request, self.environment.request_options)
            .await
        {
            Ok(response) => {
                tracing::info!(
                    step_id = %step.id,
                    status = response.status,
                    elapsed_ms = response.elapsed_ms,
                    "HTTP response received"
                );
                for (name, value) in &response.headers {
                    tracing::debug!(
                        step_id = %step.id,
                        header = %name,
                        value = %self.context.redact(value),
                        "response header"
                    );
                }
                if !response.body.is_empty() {
                    tracing::debug!(
                        step_id = %step.id,
                        body = %self.context.redact(&response.body),
                        "response body"
                    );
                }
                response
            }
            Err(error) => {
                tracing::error!(
                    step_id = %step.id,
                    error = %self.context.redact(&error.to_string()),
                    "HTTP step execution error"
                );
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

fn prepare_request(step: &HttpRequestTemplate, context: &RunContext) -> Result<Request, String> {
    let url = context.render(&step.url)?;
    if url.trim().is_empty() {
        return Err("rendered request URL is empty".to_owned());
    }

    let mut request = Request::new(step.method, url);
    for (name, value) in &step.headers {
        request.add_header(context.render(name)?, context.render(value)?);
    }
    request.body = step.body.render(
        |template| context.render(template),
        |template| context.resolve_json(template),
    )?;

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
    check: &CompiledCheck,
    response: &HttpResponse,
    context: &RunContext,
) -> CheckResult {
    let (path, expected) = match check {
        CompiledCheck::Status(expected) => {
            let success = response.status == *expected;
            return CheckResult {
                description: format!("status == {expected}"),
                success,
                message: (!success).then(|| {
                    format!(
                        "response status was {}, expected {expected}",
                        response.status
                    )
                }),
            };
        }
        CompiledCheck::JsonText { path, expected } => (path, context.resolve_expected(expected)),
        CompiledCheck::JsonValue { path, expected } => (path, context.resolve_json(expected)),
    };
    let description = format!("jsonpath {:?} == expected value", path.source());
    let outcome = (|| {
        let expected = expected?;
        let body = serde_json::from_str::<Value>(&response.body)
            .map_err(|_| "response body is not valid JSON".to_owned())?;
        let actual = path.resolve(&body)?;
        if actual == &expected {
            Ok(())
        } else {
            Err("JSONPath value did not equal the expected value".to_owned())
        }
    })();
    CheckResult {
        description,
        success: outcome.is_ok(),
        message: outcome.err(),
    }
}

fn extract_output(export: &CompiledExport, response: &HttpResponse) -> Result<Value, String> {
    let body = serde_json::from_str::<Value>(&response.body)
        .map_err(|_| format!("output '{}' requires a JSON response body", export.name))?;
    export
        .path
        .resolve(&body)
        .cloned()
        .map_err(|message| format!("output '{}' could not be exported: {message}", export.name))
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
    fn flow_outputs(&self, plan: &FlowPlan) -> Result<FlowOutputs, String> {
        plan.outputs
            .iter()
            .map(|output| {
                let value = match &output.value {
                    ValueReference::Input(name) => self.inputs.get(name),
                    ValueReference::StepOutput { step_id, name } => {
                        self.outputs.get(&(step_id.clone(), name.clone()))
                    }
                }
                .ok_or_else(|| format!("compiled flow output '{}' is unavailable", output.name))?;
                Ok((
                    output.name.clone(),
                    FlowValue {
                        value: value.value.clone(),
                        sensitive: value.sensitive,
                    },
                ))
            })
            .collect()
    }

    fn resolve_json(&self, template: &JsonTemplate) -> Result<Value, String> {
        match template {
            JsonTemplate::Literal(value) => Ok(value.clone()),
            JsonTemplate::String(value) => self.render(value).map(Value::String),
            JsonTemplate::Input(name) => self
                .inputs
                .get(name)
                .map(|value| value.value.clone())
                .ok_or_else(|| format!("input `{name}` is not bound")),
            JsonTemplate::StepOutput { step_id, name } => self
                .outputs
                .get(&(step_id.clone(), name.clone()))
                .map(|value| value.value.clone())
                .ok_or_else(|| format!("output `{step_id}.{name}` is not available at runtime")),
            JsonTemplate::Object(fields) => fields
                .iter()
                .map(|(name, value)| Ok((name.clone(), self.resolve_json(value)?)))
                .collect::<Result<_, String>>()
                .map(Value::Object),
            JsonTemplate::Array(items) => items
                .iter()
                .map(|value| self.resolve_json(value))
                .collect::<Result<_, _>>()
                .map(Value::Array),
        }
    }

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

fn bind_inputs(plan: &FlowPlan, provided: &FlowInputs) -> Result<RunContext, FlowError> {
    let input_names = plan
        .inputs
        .iter()
        .map(|input| input.name.as_str())
        .collect::<std::collections::HashSet<_>>();
    for name in provided.values.keys() {
        if !input_names.contains(name.as_str()) {
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

    Ok(RunContext {
        inputs,
        outputs: BTreeMap::new(),
    })
}

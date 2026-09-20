pub fn is_builtin_variable(name: &str) -> bool {
    matches!(
        name,
        "$timestamp" | "$timestamp_ms" | "$uuid" | "$guid" | "$randomInt"
    )
}

pub fn eval_builtin_variable(name: &str) -> Option<Value> {
    use std::time::{SystemTime, UNIX_EPOCH};
    match name {
        "$timestamp" => {
            let secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            Some(Value::Number(secs.into()))
        }
        "$timestamp_ms" => {
            let ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            if let Ok(ms_u64) = u64::try_from(ms) {
                Some(Value::Number(ms_u64.into()))
            } else {
                Some(Value::String(ms.to_string()))
            }
        }
        "$uuid" | "$guid" => Some(Value::String(uuid::Uuid::new_v4().to_string())),
        "$randomInt" => {
            use std::collections::hash_map::RandomState;
            use std::hash::{BuildHasher, Hasher};
            let val = (RandomState::new().build_hasher().finish() % 1000) as u64;
            Some(Value::Number(val.into()))
        }
        _ => None,
    }
}

fn compute_hmac_sha256(secret: &str, data: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(data.as_bytes());
    let result = mac.finalize();
    hex::encode(result.into_bytes())
}

fn evaluate_calc_in_context(expr: &str, context: &RunContext) -> Result<f64, String> {
    crate::calc::evaluate_calc(expr, |name| {
        if let Some((step, out)) = name.split_once(".") {
            if let Some(val) = context.outputs.get(&(step.to_owned(), out.to_owned())) {
                return value_to_f64(&val.value);
            }
        }
        if let Some(val) = context.inputs.get(name) {
            return value_to_f64(&val.value);
        }
        if let Some(val) = eval_builtin_variable(name) {
            return value_to_f64(&val);
        }
        Err(format!("variable '{name}' not found for calc expression"))
    }).map_err(|e| e.to_string())
}

fn value_to_f64(val: &Value) -> Result<f64, String> {
    match val {
        Value::Number(n) => n.as_f64().ok_or_else(|| "number cannot convert to f64".to_owned()),
        Value::String(s) => s.trim().parse::<f64>().map_err(|e| format!("cannot parse string '{s}' as number: {e}")),
        _ => Err(format!("cannot use non-numeric value '{val:?}' in calculation")),
    }
}

use std::{
    collections::{BTreeMap, VecDeque},
    fmt,
};

use futures::{stream, Stream};
use postman_http::{
    request::{Request, RequestBody, RequestOptions},
    response::HttpResponse,
    HttpError, HttpTransport,
};
use serde_json::Value;

use crate::{
    plan::{CompiledCheck, CompiledCondition, CompiledExport},
    ExpectedError, FlowEvent, FlowInputs, FlowOutputs, FlowPlan, FlowValue, HttpRequestTemplate,
    JsonTemplate, StepOutcome, TemplatePart, TextTemplate, ValueReference,
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
                    if let Some(when) = &step.when {
                        match evaluate_condition(when, &self.context) {
                            Ok(false) => {
                                let event = FlowEvent::StepSkipped {
                                    step_id: step.id.clone(),
                                    name: step.name.clone(),
                                    reason: "condition evaluated to false".to_owned(),
                                };
                                self.step_index += 1;
                                self.phase = if self.step_index < self.plan.steps.len() {
                                    RunPhase::StepStart
                                } else {
                                    RunPhase::FlowFinish
                                };
                                return Some(Ok(event));
                            }
                            Ok(true) => {}
                            Err(error) => {
                                let step_id = step.id.clone();
                                let name = step.name.clone();
                                self.fail_step(
                                    &step_id,
                                    format!("failed to evaluate condition: {}", error),
                                );
                                return Some(Ok(FlowEvent::StepStarted { step_id, name }));
                            }
                        }
                    }
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
            url = %self.context.redact_url(&request.url),
            "executing HTTP step"
        );
        for (name, value) in &request.headers {
            tracing::debug!(
                step_id = %step.id,
                header = %name,
                value = %self.context.redact_header(name, value),
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

        let mut options = self.environment.request_options;
        if let Some(timeout_ms) = step.request.options.timeout_ms {
            options.timeout_ms = Some(timeout_ms);
        }
        if let Some(policy) = step.request.options.redirect_policy {
            options.redirect_policy = policy;
        }
        if let Some(max_hops) = step.request.options.max_redirect_hops {
            options.max_redirect_hops = max_hops;
        }

        let response = match self.transport.execute(request, options).await {
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
                        value = %self.context.redact_header(name, value),
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
                let has_expected_error = step
                    .checks
                    .iter()
                    .any(|check| matches!(check, CompiledCheck::Error(_)));
                if has_expected_error {
                    let mut checks_succeeded = true;
                    for check in &step.checks {
                        let result = evaluate_error_check(check, &error);
                        checks_succeeded &= result.success;
                        self.pending.push_back(FlowEvent::CheckFinished {
                            step_id: step.id.clone(),
                            check: result.description,
                            success: result.success,
                            message: result.message.map(|message| self.context.redact(&message)),
                        });
                    }
                    if checks_succeeded && step.exports.is_empty() {
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
                        return;
                    }
                }
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
    let mut url = context.render(&step.url)?;
    if url.trim().is_empty() {
        return Err("rendered request URL is empty".to_owned());
    }

    if let Some(crate::model::AuthTemplate::HmacSha256 { secret, param }) = &step.auth {
        let secret_val = context.render(secret)?;
        let query_str = url.split_once('?').map(|(_, q)| q).unwrap_or("");
        let sig = compute_hmac_sha256(&secret_val, query_str);
        if url.contains('?') {
            url.push('&');
        } else {
            url.push('?');
        }
        url.push_str(param);
        url.push('=');
        url.push_str(&sig);
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
    match check {
        CompiledCheck::Status(expected) => {
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
        CompiledCheck::Redirects(expected) => {
            let count = response
                .redirect_chain
                .iter()
                .filter(|hop| (300..400).contains(&hop.status) && hop.location.is_some())
                .count();
            let success = count == *expected;
            CheckResult {
                description: format!("redirects == {expected}"),
                success,
                message: (!success)
                    .then(|| format!("response redirect count was {count}, expected {expected}")),
            }
        }
        CompiledCheck::HeaderExists { name } => {
            let success = response
                .headers
                .iter()
                .any(|(actual_name, _)| actual_name.eq_ignore_ascii_case(name));
            CheckResult {
                description: format!("header \"{name}\" exists"),
                success,
                message: (!success).then(|| format!("response header \"{name}\" was not present")),
            }
        }
        CompiledCheck::HeaderContains { name, expected } => {
            let description = format!("header \"{name}\" contains expected value");
            let outcome = (|| {
                let expected = context.render(expected)?;
                let matches = response.headers.iter().any(|(actual_name, value)| {
                    actual_name.eq_ignore_ascii_case(name) && value.contains(&expected)
                });
                if matches {
                    Ok(())
                } else {
                    Err("response header did not contain the expected value".to_owned())
                }
            })();
            CheckResult {
                description,
                success: outcome.is_ok(),
                message: outcome.err(),
            }
        }
        CompiledCheck::BodyContains { expected } => {
            let description = "body contains expected value".to_owned();
            let outcome = (|| {
                let expected = context.render(expected)?;
                if response.body.contains(&expected) {
                    Ok(())
                } else {
                    Err("response body did not contain the expected value".to_owned())
                }
            })();
            CheckResult {
                description,
                success: outcome.is_ok(),
                message: outcome.err(),
            }
        }
        CompiledCheck::Error(expected) => CheckResult {
            description: format!("error == {}", expected_error_name(*expected)),
            success: false,
            message: Some("request completed without the expected transport error".to_owned()),
        },
        CompiledCheck::JsonValue { path, expected } => {
            let description = format!("jsonpath {:?} == expected value", path.source());
            let outcome = (|| {
                let expected = context.resolve_json(expected)?;
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
    }
}

fn evaluate_error_check(check: &CompiledCheck, error: &HttpError) -> CheckResult {
    match check {
        CompiledCheck::Error(expected) => {
            let success = expected_error_matches(*expected, error);
            CheckResult {
                description: format!("error == {}", expected_error_name(*expected)),
                success,
                message: (!success).then(|| {
                    "transport error kind did not equal the expected error kind".to_owned()
                }),
            }
        }
        check => CheckResult {
            description: check_description(check),
            success: false,
            message: Some(
                "request failed before this response check could be evaluated".to_owned(),
            ),
        },
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

fn check_description(check: &CompiledCheck) -> String {
    match check {
        CompiledCheck::Status(expected) => format!("status == {expected}"),
        CompiledCheck::Redirects(expected) => format!("redirects == {expected}"),
        CompiledCheck::HeaderExists { name } => format!("header \"{name}\" exists"),
        CompiledCheck::HeaderContains { name, .. } => {
            format!("header \"{name}\" contains expected value")
        }
        CompiledCheck::BodyContains { .. } => "body contains expected value".to_owned(),
        CompiledCheck::JsonValue { path, .. } => {
            format!("jsonpath {:?} == expected value", path.source())
        }
        CompiledCheck::Error(expected) => format!("error == {}", expected_error_name(*expected)),
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
    fn resolve_value_ref(&self, reference: &ValueReference) -> Option<BoundValue> {
        match reference {
            ValueReference::Input(name) => {
                if let Some(val) = self.inputs.get(name).cloned() {
                    Some(val)
                } else if let Some(builtin) = eval_builtin_variable(name) {
                    Some(BoundValue { value: builtin, sensitive: false })
                } else {
                    None
                }
            },
            ValueReference::StepOutput { step_id, name } => {
                self.outputs.get(&(step_id.clone(), name.clone())).cloned()
            }
            ValueReference::Literal(value) => Some(BoundValue {
                value: value.clone(),
                sensitive: false,
            }),
            ValueReference::Coalesce(candidates) => {
                for candidate in candidates {
                    if let Some(val) = self.resolve_value_ref(candidate) {
                        return Some(val);
                    }
                }
                None
            }
        }
    }

    fn flow_outputs(&self, plan: &FlowPlan) -> Result<FlowOutputs, String> {
        plan.outputs
            .iter()
            .map(|output| {
                let value = self.resolve_value_ref(&output.value).ok_or_else(|| {
                    format!("compiled flow output '{}' is unavailable", output.name)
                })?;
                Ok((
                    output.name.clone(),
                    FlowValue {
                        value: value.value,
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
            JsonTemplate::Input(name) => {
                if let Some(value) = self.inputs.get(name) {
                    Ok(value.value.clone())
                } else if let Some(builtin) = eval_builtin_variable(name) {
                    Ok(builtin)
                } else {
                    Err(format!("input `{name}` is not bound"))
                }
            }
            JsonTemplate::Calc(expr) => {
                let val = evaluate_calc_in_context(expr, self)?;
                if val.fract() == 0.0 && val.abs() < 1e15 {
                    Ok(Value::Number((val as i64).into()))
                } else if let Some(num) = serde_json::Number::from_f64(val) {
                    Ok(Value::Number(num))
                } else {
                    Ok(Value::String(val.to_string()))
                }
            }
            JsonTemplate::StepOutput { step_id, name } => self
                .outputs
                .get(&(step_id.clone(), name.clone()))
                .map(|value| value.value.clone())
                .ok_or_else(|| format!("output `{step_id}.{name}` is not available at runtime")),
            JsonTemplate::Coalesce(candidates) => {
                for candidate in candidates {
                    if let Ok(val) = self.resolve_json(candidate) {
                        return Ok(val);
                    }
                }
                Err("none of the coalesce candidates were available at runtime".to_owned())
            }
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

    fn render_part(&self, part: &TemplatePart) -> Result<String, String> {
        match part {
            TemplatePart::Literal(value) => Ok(value.clone()),
            TemplatePart::Input(name) => {
                if let Some(value) = self.inputs.get(name) {
                    Ok(value_as_text(&value.value))
                } else if let Some(builtin) = eval_builtin_variable(name) {
                    Ok(value_as_text(&builtin))
                } else {
                    Err(format!("input `{name}` is not bound"))
                }
            }
            TemplatePart::Calc(expr) => {
                let val = evaluate_calc_in_context(expr, self)?;
                if val.fract() == 0.0 && val.abs() < 1e15 {
                    Ok((val as i64).to_string())
                } else {
                    Ok(val.to_string())
                }
            }
            TemplatePart::StepOutput { step_id, name } => {
                let value = self
                    .outputs
                    .get(&(step_id.clone(), name.clone()))
                    .ok_or_else(|| {
                        format!("output `{step_id}.{name}` is not available at runtime")
                    })?;
                Ok(value_as_text(&value.value))
            }
            TemplatePart::Coalesce(candidates) => {
                for candidate in candidates {
                    if let Ok(s) = self.render(candidate) {
                        return Ok(s);
                    }
                }
                Err("none of the coalesce candidates were available at runtime".to_owned())
            }
        }
    }

    fn render(&self, template: &TextTemplate) -> Result<String, String> {
        let mut rendered = String::new();
        for part in &template.parts {
            rendered.push_str(&self.render_part(part)?);
        }
        Ok(rendered)
    }

    fn redact(&self, message: &str) -> String {
        let mut secrets = self
            .inputs
            .values()
            .chain(self.outputs.values())
            .filter(|value| value.sensitive)
            .flat_map(|value| {
                let raw = value_as_text(&value.value);
                let encoded = if raw.is_empty() {
                    String::new()
                } else {
                    serde_json::to_string(&value.value).unwrap_or_default()
                };
                let unquoted =
                    if encoded.len() >= 2 && encoded.starts_with('"') && encoded.ends_with('"') {
                        encoded[1..encoded.len() - 1].to_string()
                    } else {
                        String::new()
                    };
                [raw, encoded, unquoted]
            })
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        secrets.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        secrets.dedup();

        secrets
            .into_iter()
            .fold(message.to_owned(), |redacted, secret| {
                redacted.replace(&secret, "[REDACTED]")
            })
    }

    fn redact_header(&self, name: &str, value: &str) -> String {
        if is_sensitive_name(name) {
            "[REDACTED]".to_string()
        } else {
            self.redact(value)
        }
    }

    fn redact_url(&self, url: &str) -> String {
        let sanitized = sanitize_url_for_log(url);
        self.redact(&sanitized)
    }
}

fn is_sensitive_name(name: &str) -> bool {
    let compact_name: String = name
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    matches!(
        compact_name.as_str(),
        "authorization"
            | "proxyauthorization"
            | "cookie"
            | "cookies"
            | "setcookie"
            | "apikey"
            | "session"
            | "sessionid"
    ) || compact_name.contains("token")
        || compact_name.contains("secret")
        || compact_name.contains("password")
        || compact_name.contains("credential")
        || compact_name.contains("apikey")
}

fn sanitize_url_for_log(value: &str) -> String {
    if let Ok(url) = url::Url::parse(value) {
        if let Some(host) = url.host_str() {
            let mut output = format!("{}://{host}", url.scheme());
            if let Some(port) = url.port() {
                output.push_str(&format!(":{port}"));
            }
            output.push_str(url.path());
            sanitize_query_and_fragment(&url, &mut output);
            return output;
        }
    } else if let Ok(base) = url::Url::parse("http://dummy.invalid") {
        if let Ok(url) = base.join(value) {
            let mut output = url.path().to_string();
            sanitize_query_and_fragment(&url, &mut output);
            return output;
        }
    }
    value.to_string()
}

fn sanitize_query_and_fragment(url: &url::Url, output: &mut String) {
    let query = url
        .query_pairs()
        .map(|(name, value)| {
            let value = if is_sensitive_name(&name) {
                "[REDACTED]".to_string()
            } else {
                value.into_owned()
            };
            format!("{name}={value}")
        })
        .collect::<Vec<_>>();
    if !query.is_empty() {
        output.push('?');
        output.push_str(&query.join("&"));
    }
    if url.fragment().is_some() {
        output.push_str("#[REDACTED]");
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

fn evaluate_condition(condition: &CompiledCondition, context: &RunContext) -> Result<bool, String> {
    match condition {
        CompiledCondition::Eq(left, right) => {
            let l = context.resolve_json(left)?;
            let r = context.resolve_json(right)?;
            Ok(values_equal(&l, &r))
        }
        CompiledCondition::Ne(left, right) => {
            let l = context.resolve_json(left)?;
            let r = context.resolve_json(right)?;
            Ok(!values_equal(&l, &r))
        }
        CompiledCondition::Gt(left, right) => {
            let l = context.resolve_json(left)?;
            let r = context.resolve_json(right)?;
            compare_values(&l, &r).map(|cmp| cmp == std::cmp::Ordering::Greater)
        }
        CompiledCondition::Gte(left, right) => {
            let l = context.resolve_json(left)?;
            let r = context.resolve_json(right)?;
            compare_values(&l, &r).map(|cmp| cmp != std::cmp::Ordering::Less)
        }
        CompiledCondition::Lt(left, right) => {
            let l = context.resolve_json(left)?;
            let r = context.resolve_json(right)?;
            compare_values(&l, &r).map(|cmp| cmp == std::cmp::Ordering::Less)
        }
        CompiledCondition::Lte(left, right) => {
            let l = context.resolve_json(left)?;
            let r = context.resolve_json(right)?;
            compare_values(&l, &r).map(|cmp| cmp != std::cmp::Ordering::Greater)
        }
        CompiledCondition::In(item, collection) => {
            let it = context.resolve_json(item)?;
            let coll = context.resolve_json(collection)?;
            match coll {
                Value::Array(arr) => Ok(arr.iter().any(|elem| values_equal(&it, elem))),
                Value::String(s) => {
                    if let Value::String(sub) = it {
                        Ok(s.contains(&sub))
                    } else {
                        Ok(s.contains(&value_as_text(&it)))
                    }
                }
                _ => Err(
                    "`in` condition expects an array or string on the right-hand side".to_owned(),
                ),
            }
        }
        CompiledCondition::And(conditions) => {
            for c in conditions {
                if !evaluate_condition(c, context)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        CompiledCondition::Or(conditions) => {
            for c in conditions {
                if evaluate_condition(c, context)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        CompiledCondition::Not(inner) => {
            let res = evaluate_condition(inner, context)?;
            Ok(!res)
        }
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    if a == b {
        return true;
    }
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => compare_numbers(a, b).is_eq(),
        (Value::Number(n), Value::String(s)) | (Value::String(s), Value::Number(n)) => {
            parse_condition_number(s).is_some_and(|parsed| compare_numbers(n, &parsed).is_eq())
        }
        (Value::Bool(b), Value::String(s)) | (Value::String(s), Value::Bool(b)) => {
            if let Ok(parsed) = s.parse::<bool>() {
                return parsed == *b;
            }
            false
        }
        _ => false,
    }
}

fn compare_values(a: &Value, b: &Value) -> Result<std::cmp::Ordering, String> {
    match (a, b) {
        (Value::Number(n1), Value::Number(n2)) => Ok(compare_numbers(n1, n2)),
        (Value::String(s1), Value::String(s2)) => {
            if let (Some(n1), Some(n2)) = (parse_condition_number(s1), parse_condition_number(s2)) {
                Ok(compare_numbers(&n1, &n2))
            } else {
                Ok(s1.cmp(s2))
            }
        }
        (Value::Number(n), Value::String(s)) => {
            let parsed =
                parse_condition_number(s).ok_or("cannot compare non-numeric string to number")?;
            Ok(compare_numbers(n, &parsed))
        }
        (Value::String(s), Value::Number(n)) => {
            let parsed =
                parse_condition_number(s).ok_or("cannot compare non-numeric string to number")?;
            Ok(compare_numbers(&parsed, n))
        }
        _ => Err(format!(
            "cannot compare values of types {:?} and {:?}",
            a, b
        )),
    }
}

fn parse_condition_number(text: &str) -> Option<serde_json::Number> {
    if let Ok(value) = text.parse::<i64>() {
        return Some(value.into());
    }
    if let Ok(value) = text.parse::<u64>() {
        return Some(value.into());
    }
    serde_json::Number::from_f64(text.parse().ok()?)
}

fn compare_numbers(a: &serde_json::Number, b: &serde_json::Number) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    fn integer(n: &serde_json::Number) -> Option<i128> {
        n.as_i64()
            .map(i128::from)
            .or_else(|| n.as_u64().map(i128::from))
    }
    // Compare the integer to the float's integral and fractional parts without
    // rounding the integer to f64 (which loses bits above 2^53).
    fn integer_float(i: i128, f: f64) -> Ordering {
        if f >= 2_f64.powi(127) {
            return Ordering::Less;
        }
        if f < -2_f64.powi(127) {
            return Ordering::Greater;
        }
        i.cmp(&(f as i128))
            .then_with(|| 0.0_f64.partial_cmp(&f.fract()).expect("finite JSON number"))
    }
    match (integer(a), integer(b)) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(a), None) => integer_float(a, b.as_f64().unwrap()),
        (None, Some(b)) => integer_float(b, a.as_f64().unwrap()).reverse(),
        (None, None) => a
            .as_f64()
            .unwrap()
            .partial_cmp(&b.as_f64().unwrap())
            .unwrap(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn condition_numbers_preserve_integer_precision() {
        use serde_json::json;
        use std::cmp::Ordering::{Equal, Greater, Less};
        let cases = [
            (json!(u64::MAX), json!(u64::MAX - 1), Greater),
            (json!(u64::MAX), json!(i64::MAX), Greater),
            (json!(-1), json!(u64::MAX), Less),
            (json!(u64::MAX), json!((u64::MAX - 1).to_string()), Greater),
            (json!((u64::MAX - 1).to_string()), json!(u64::MAX), Less),
            (
                json!(9007199254740993_u64),
                json!(9007199254740992.0),
                Greater,
            ),
            (json!(u64::MAX), json!(18446744073709551616.0), Less),
            (json!(1), json!(1.5), Less),
            (json!(-1), json!(-1.5), Greater),
            (json!(1), json!(1.0), Equal),
            (json!(0), json!("0.00000000000000001"), Less),
        ];
        for (a, b, expected) in cases {
            assert_eq!(compare_values(&a, &b).unwrap(), expected, "{a} vs {b}");
            assert_eq!(compare_values(&b, &a).unwrap(), expected.reverse());
            assert_eq!(values_equal(&a, &b), expected == Equal, "{a} vs {b}");
        }
        assert!(compare_values(&json!(1), &json!("NaN")).is_err());
        assert!(compare_values(&json!(1), &json!("inf")).is_err());
    }

    #[test]
    fn test_sanitize_url_for_log_userinfo_and_query() {
        let url = "https://user:secretpass@example.com/api/v1/users?token=super_secret&page=2&api_key=key123&name=john";
        let sanitized = sanitize_url_for_log(url);
        assert_eq!(
            sanitized,
            "https://example.com/api/v1/users?token=[REDACTED]&page=2&api_key=[REDACTED]&name=john"
        );
        assert!(!sanitized.contains("secretpass"));
        assert!(!sanitized.contains("user:"));
        assert!(!sanitized.contains("super_secret"));
        assert!(!sanitized.contains("key123"));
    }

    #[test]
    fn test_sanitize_url_for_log_fragments_and_relative() {
        let url_with_frag = "https://example.com/docs#sensitive_anchor";
        assert_eq!(
            sanitize_url_for_log(url_with_frag),
            "https://example.com/docs#[REDACTED]"
        );

        let relative = "/api/v1/search?secret_token=topsecret&q=rust";
        assert_eq!(
            sanitize_url_for_log(relative),
            "/api/v1/search?secret_token=[REDACTED]&q=rust"
        );
    }

    #[test]
    fn test_redact_json_escaped_secret() {
        let mut inputs = BTreeMap::new();
        inputs.insert(
            "password".to_string(),
            BoundValue {
                value: Value::String("p\"a\\s:s".to_string()),
                sensitive: true,
            },
        );
        let context = RunContext {
            inputs,
            outputs: BTreeMap::new(),
        };

        // Raw occurrence
        let raw_text = "The password is p\"a\\s:s";
        assert_eq!(context.redact(raw_text), "The password is [REDACTED]");

        // Serialized as full JSON field
        let json_body = r#"{"user":"alice","password":"p\"a\\s:s"}"#;
        assert_eq!(
            context.redact(json_body),
            r#"{"user":"alice","password":[REDACTED]}"#
        );

        // Substring inside a JSON string
        let nested_json = r#"{"note":"Your secret was p\"a\\s:s, please remember"}"#;
        assert_eq!(
            context.redact(nested_json),
            r#"{"note":"Your secret was [REDACTED], please remember"}"#
        );
    }

    #[test]
    fn test_redact_url_with_sensitive_context_values() {
        let mut inputs = BTreeMap::new();
        inputs.insert(
            "tenant".to_string(),
            BoundValue {
                value: Value::String("tenant_private_123".to_string()),
                sensitive: true,
            },
        );
        let context = RunContext {
            inputs,
            outputs: BTreeMap::new(),
        };

        let url =
            "https://admin:pass@example.com/api/tenant_private_123/list?token=tok789&limit=10";
        let redacted = context.redact_url(url);
        assert_eq!(
            redacted,
            "https://example.com/api/[REDACTED]/list?token=[REDACTED]&limit=10"
        );
    }

    #[test]
    fn test_redact_header() {
        let context = RunContext {
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
        };
        assert_eq!(
            context.redact_header("Authorization", "Bearer secret123"),
            "[REDACTED]"
        );
        assert_eq!(
            context.redact_header("x-api-key", "key-value"),
            "[REDACTED]"
        );
        assert_eq!(context.redact_header("Cookie", "session=xyz"), "[REDACTED]");
        assert_eq!(
            context.redact_header("Content-Type", "application/json"),
            "application/json"
        );
    }
}

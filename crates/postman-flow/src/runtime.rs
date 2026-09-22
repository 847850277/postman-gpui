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
            let val = RandomState::new().build_hasher().finish() % 1000;
            Some(Value::Number(val.into()))
        }
        _ => None,
    }
}

fn compute_hmac_sha256(secret: &str, data: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(data.as_bytes());
    let result = mac.finalize();
    hex::encode(result.into_bytes())
}

fn evaluate_calc_in_context(
    expr: &str,
    context: &(impl TemplateContext + ?Sized),
) -> Result<f64, String> {
    crate::calc::evaluate_calc(expr, |name| match context.input(name) {
        Ok(value) => value_to_f64(&value),
        Err(error) => {
            if let Some((step, output)) = name.split_once('.') {
                value_to_f64(&context.output(step, output)?)
            } else {
                Err(error)
            }
        }
    })
    .map_err(|e| e.to_string())
}

fn value_to_f64(val: &Value) -> Result<f64, String> {
    match val {
        Value::Number(n) => n
            .as_f64()
            .ok_or_else(|| "number cannot convert to f64".to_owned()),
        Value::String(s) => s
            .trim()
            .parse::<f64>()
            .map_err(|e| format!("cannot parse string '{s}' as number: {e}")),
        _ => Err(format!(
            "cannot use non-numeric value '{val:?}' in calculation"
        )),
    }
}

use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    fmt,
    time::{Duration, Instant},
};

use bigdecimal::BigDecimal;
use futures::{stream, Stream};
use postman_http::{
    request::{Request, RequestBody, RequestOptions},
    response::HttpResponse,
    HttpError, HttpTransport,
};
use serde_json::Value;

use crate::{
    plan::{
        CompiledCheck, CompiledCondition, CompiledExport, CompiledRequest, CompiledStepAction,
        ForEachPlan, HttpStepPlan, RepeatUntilPlan,
    },
    ExpectedError, FlowEvent, FlowInputs, FlowOutputs, FlowPlan, FlowValue, HttpRequestTemplate,
    JsonTemplate, LoopErrorPolicy, LoopFinishReason, LoopKind, StepOutcome, TemplatePart,
    TextTemplate, ValueReference,
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
    tasks: Vec<Task>,
    pending: VecDeque<FlowEvent>,
    skipped_steps: HashSet<String>,
    started: bool,
    finished: bool,
    last_result: bool,
    last_loop_reason: Option<LoopFinishReason>,
    success: bool,
}

enum Task {
    RunSequence {
        steps: Vec<HttpStepPlan>,
        index: usize,
        on_error: LoopErrorPolicy,
        had_failure: bool,
        deadline: Option<Instant>,
    },
    ContinueSequence {
        steps: Vec<HttpStepPlan>,
        index: usize,
        on_error: LoopErrorPolicy,
        had_failure: bool,
        deadline: Option<Instant>,
    },
    StartStep {
        step: HttpStepPlan,
        deadline: Option<Instant>,
    },
    ExecuteHttp {
        step_id: String,
        request: CompiledRequest,
        checks: Vec<CompiledCheck>,
        exports: Vec<CompiledExport>,
        deadline: Option<Instant>,
    },
    StartForEach {
        step_id: String,
        plan: ForEachPlan,
        deadline: Option<Instant>,
    },
    ForEachNext(ForEachState),
    ForEachAfter(ForEachState),
    StartRepeatUntil {
        step_id: String,
        plan: RepeatUntilPlan,
        parent_deadline: Option<Instant>,
    },
    RepeatNext(RepeatState),
    RepeatAfter(RepeatState),
    DelayRepeat(RepeatState),
}

struct ForEachState {
    step_id: String,
    plan: ForEachPlan,
    items: Vec<Value>,
    item_sensitive: bool,
    index: usize,
    collected: BTreeMap<String, Vec<BoundValue>>,
    saved_item: Option<BoundValue>,
    saved_index: Option<BoundValue>,
    deadline: Option<Instant>,
}

struct RepeatState {
    step_id: String,
    plan: RepeatUntilPlan,
    index: usize,
    deadline: Instant,
    saved_carry: Vec<(String, Option<BoundValue>)>,
}

impl<T: HttpTransport> RunMachine<T> {
    fn new(
        plan: FlowPlan,
        transport: T,
        environment: FlowSessionEnvironment,
        context: RunContext,
    ) -> Self {
        Self {
            tasks: vec![Task::RunSequence {
                steps: plan.steps.clone(),
                index: 0,
                on_error: LoopErrorPolicy::FailFast,
                had_failure: false,
                deadline: None,
            }],
            plan,
            transport,
            environment,
            context,
            pending: VecDeque::new(),
            skipped_steps: HashSet::new(),
            started: false,
            finished: false,
            last_result: true,
            last_loop_reason: None,
            success: true,
        }
    }

    async fn next_event(&mut self) -> Option<Result<FlowEvent, FlowError>> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                return Some(Ok(event));
            }

            if !self.started {
                self.started = true;
                return Some(Ok(FlowEvent::FlowStarted {
                    name: self.plan.name.clone(),
                    total_steps: self.plan.steps.len(),
                }));
            }
            if self.finished {
                return None;
            }
            let Some(task) = self.tasks.pop() else {
                self.finished = true;
                self.success &= self.last_result;
                let outputs = if self.success {
                    match self.context.flow_outputs(&self.plan) {
                        Ok(outputs) => outputs,
                        Err(message) => return Some(Err(FlowError::InvariantViolation(message))),
                    }
                } else {
                    FlowOutputs::new()
                };
                return Some(Ok(FlowEvent::FlowFinished {
                    success: self.success,
                    outputs,
                }));
            };
            match task {
                Task::RunSequence {
                    steps,
                    index,
                    on_error,
                    had_failure,
                    deadline,
                } => {
                    if index >= steps.len() {
                        self.last_result = !had_failure;
                    } else {
                        let step = steps[index].clone();
                        self.tasks.push(Task::ContinueSequence {
                            steps,
                            index: index + 1,
                            on_error,
                            had_failure,
                            deadline,
                        });
                        self.tasks.push(Task::StartStep { step, deadline });
                    }
                }
                Task::ContinueSequence {
                    steps,
                    index,
                    on_error,
                    had_failure,
                    deadline,
                } => {
                    if self.last_result || on_error == LoopErrorPolicy::Continue {
                        self.tasks.push(Task::RunSequence {
                            steps,
                            index,
                            on_error,
                            had_failure: had_failure || !self.last_result,
                            deadline,
                        });
                    }
                }
                Task::StartStep { step, deadline } => {
                    self.last_loop_reason = None;
                    self.skipped_steps.remove(&step.id);
                    if let Some(when) = &step.when {
                        match evaluate_condition(when, &self.context) {
                            Ok(false) => {
                                self.last_result = true;
                                self.skipped_steps.insert(step.id.clone());
                                return Some(Ok(FlowEvent::StepSkipped {
                                    step_id: step.id,
                                    name: step.name,
                                    reason: "condition evaluated to false".to_owned(),
                                }));
                            }
                            Ok(true) => {}
                            Err(error) => {
                                self.last_result = false;
                                self.last_loop_reason = Some(LoopFinishReason::StepFailed);
                                self.pending.push_back(FlowEvent::StepFinished {
                                    step_id: step.id.clone(),
                                    outcome: StepOutcome::Failed {
                                        message: self.context.redact(&format!(
                                            "failed to evaluate condition: {error}"
                                        )),
                                    },
                                });
                                return Some(Ok(FlowEvent::StepStarted {
                                    step_id: step.id,
                                    name: step.name,
                                }));
                            }
                        }
                    }
                    let step_id = step.id.clone();
                    match step.action {
                        CompiledStepAction::Http {
                            request,
                            checks,
                            exports,
                        } => self.tasks.push(Task::ExecuteHttp {
                            step_id,
                            request,
                            checks,
                            exports,
                            deadline,
                        }),
                        CompiledStepAction::ForEach(plan) => self.tasks.push(Task::StartForEach {
                            step_id,
                            plan,
                            deadline,
                        }),
                        CompiledStepAction::RepeatUntil(plan) => {
                            self.tasks.push(Task::StartRepeatUntil {
                                step_id,
                                plan,
                                parent_deadline: deadline,
                            })
                        }
                    }
                    return Some(Ok(FlowEvent::StepStarted {
                        step_id: step.id,
                        name: step.name,
                    }));
                }
                Task::ExecuteHttp {
                    step_id,
                    request,
                    checks,
                    exports,
                    deadline,
                } => {
                    self.execute_http_step(step_id, request, checks, exports, deadline)
                        .await;
                }
                Task::StartForEach {
                    step_id,
                    plan,
                    deadline,
                } => {
                    let item_sensitive = self.context.json_sensitive(&plan.items);
                    match self.context.resolve_json(&plan.items) {
                        Ok(Value::Array(items)) => {
                            self.pending.push_back(FlowEvent::LoopStarted {
                                step_id: step_id.clone(),
                                kind: LoopKind::ForEach,
                                limit: plan.max_iterations,
                            });
                            let collected = plan
                                .collect
                                .iter()
                                .map(|item| (item.name.clone(), Vec::new()))
                                .collect();
                            self.tasks.push(Task::ForEachNext(ForEachState {
                                step_id,
                                plan,
                                items,
                                item_sensitive,
                                index: 0,
                                collected,
                                saved_item: None,
                                saved_index: None,
                                deadline,
                            }));
                        }
                        Ok(_) => self.fail_loop(
                            step_id,
                            0,
                            LoopFinishReason::StepFailed,
                            "for_each items must resolve to a JSON array",
                        ),
                        Err(error) => self.fail_loop(
                            step_id,
                            0,
                            LoopFinishReason::StepFailed,
                            &format!("failed to resolve for_each items: {error}"),
                        ),
                    }
                }
                Task::ForEachNext(mut state) => {
                    if state.index >= state.items.len() {
                        self.finish_for_each(state);
                    } else if state.index >= state.plan.max_iterations {
                        self.fail_loop(
                            state.step_id,
                            state.index,
                            LoopFinishReason::MaxIterations,
                            "for_each reached max_iterations before consuming all items",
                        );
                    } else {
                        self.clear_step_state(&state.plan.steps);
                        state.saved_item = self.context.inputs.insert(
                            state.plan.item_name.clone(),
                            BoundValue {
                                value: state.items[state.index].clone(),
                                sensitive: state.item_sensitive,
                            },
                        );
                        state.saved_index = state.plan.index_name.as_ref().and_then(|name| {
                            self.context.inputs.insert(
                                name.clone(),
                                BoundValue {
                                    value: Value::from(state.index),
                                    sensitive: false,
                                },
                            )
                        });
                        let steps = state.plan.steps.clone();
                        let step_id = state.step_id.clone();
                        let index = state.index;
                        let deadline = state.deadline;
                        self.tasks.push(Task::ForEachAfter(state));
                        self.tasks.push(Task::RunSequence {
                            steps,
                            index: 0,
                            on_error: LoopErrorPolicy::FailFast,
                            had_failure: false,
                            deadline,
                        });
                        return Some(Ok(FlowEvent::IterationStarted { step_id, index }));
                    }
                }
                Task::ForEachAfter(mut state) => {
                    let succeeded = self.last_result;
                    if succeeded {
                        let mut iteration_values = Vec::new();
                        for collection in &state.plan.collect {
                            if self.skipped_steps.contains(&collection.step_id) {
                                continue;
                            }
                            if let Some(value) = self
                                .context
                                .outputs
                                .get(&(collection.step_id.clone(), collection.output.clone()))
                                .cloned()
                            {
                                iteration_values.push((&collection.name, value));
                            } else {
                                self.last_result = false;
                                break;
                            }
                        }
                        if self.last_result {
                            for (name, value) in iteration_values {
                                state
                                    .collected
                                    .get_mut(name)
                                    .expect("compiled collection exists")
                                    .push(value);
                            }
                        }
                    }
                    self.restore_loop_locals(&mut state);
                    self.clear_step_state(&state.plan.steps);
                    let outcome = if self.last_result {
                        StepOutcome::Succeeded
                    } else {
                        StepOutcome::Failed {
                            message: "iteration failed".to_owned(),
                        }
                    };
                    let index = state.index;
                    let step_id = state.step_id.clone();
                    let failure_reason = self
                        .last_loop_reason
                        .take()
                        .unwrap_or(LoopFinishReason::StepFailed);
                    // Cancellation must unwind enclosing loops even when ordinary failures
                    // are skipped by the iteration's error policy.
                    let should_continue = self.last_result
                        || (state.plan.on_error == LoopErrorPolicy::Continue
                            && failure_reason != LoopFinishReason::Cancelled);
                    state.index += 1;
                    if should_continue {
                        self.tasks.push(Task::ForEachNext(state));
                    } else {
                        self.fail_loop(
                            step_id.clone(),
                            state.index,
                            failure_reason,
                            "for_each iteration failed",
                        );
                    }
                    return Some(Ok(FlowEvent::IterationFinished {
                        step_id,
                        index,
                        outcome,
                    }));
                }
                Task::StartRepeatUntil {
                    step_id,
                    plan,
                    parent_deadline,
                } => {
                    self.pending.push_back(FlowEvent::LoopStarted {
                        step_id: step_id.clone(),
                        kind: LoopKind::RepeatUntil,
                        limit: plan.max_iterations,
                    });
                    let mut saved_carry = Vec::new();
                    let mut initialization_error = None;
                    for carry in &plan.carry {
                        match self.context.resolve_json(&carry.initial) {
                            Ok(value) => {
                                let sensitive = self.context.json_sensitive(&carry.initial);
                                let previous = self
                                    .context
                                    .inputs
                                    .insert(carry.name.clone(), BoundValue { value, sensitive });
                                saved_carry.push((carry.name.clone(), previous));
                            }
                            Err(error) => {
                                initialization_error = Some(format!(
                                    "failed to initialize carry '{}': {error}",
                                    carry.name
                                ));
                                break;
                            }
                        }
                    }
                    let own_deadline = Instant::now() + Duration::from_millis(plan.timeout_ms);
                    let deadline = parent_deadline
                        .map(|parent| parent.min(own_deadline))
                        .unwrap_or(own_deadline);
                    let mut state = RepeatState {
                        step_id,
                        plan,
                        index: 0,
                        deadline,
                        saved_carry,
                    };
                    if let Some(error) = initialization_error {
                        let step_id = state.step_id.clone();
                        self.restore_repeat_locals(&mut state);
                        self.fail_loop(step_id, 0, LoopFinishReason::StepFailed, &error);
                    } else {
                        self.tasks.push(Task::RepeatNext(state));
                    }
                }
                Task::RepeatNext(mut state) => {
                    if Instant::now() >= state.deadline {
                        self.restore_repeat_locals(&mut state);
                        self.fail_loop(
                            state.step_id,
                            state.index,
                            LoopFinishReason::Timeout,
                            "repeat_until timed out",
                        );
                    } else if state.index >= state.plan.max_iterations {
                        self.restore_repeat_locals(&mut state);
                        self.fail_loop(
                            state.step_id,
                            state.index,
                            LoopFinishReason::MaxIterations,
                            "repeat_until reached max_iterations",
                        );
                    } else {
                        self.clear_step_state(&state.plan.steps);
                        let steps = state.plan.steps.clone();
                        let step_id = state.step_id.clone();
                        let index = state.index;
                        let deadline = state.deadline;
                        self.tasks.push(Task::RepeatAfter(state));
                        self.tasks.push(Task::RunSequence {
                            steps,
                            index: 0,
                            on_error: LoopErrorPolicy::FailFast,
                            had_failure: false,
                            deadline: Some(deadline),
                        });
                        return Some(Ok(FlowEvent::IterationStarted { step_id, index }));
                    }
                }
                Task::RepeatAfter(mut state) => {
                    let step_id = state.step_id.clone();
                    let index = state.index;
                    if !self.last_result {
                        let reason = self.last_loop_reason.take().unwrap_or_else(|| {
                            if Instant::now() >= state.deadline {
                                LoopFinishReason::Timeout
                            } else {
                                LoopFinishReason::StepFailed
                            }
                        });
                        self.restore_repeat_locals(&mut state);
                        self.fail_loop(
                            step_id.clone(),
                            index + 1,
                            reason,
                            "repeat_until body failed",
                        );
                        return Some(Ok(FlowEvent::IterationFinished {
                            step_id,
                            index,
                            outcome: StepOutcome::Failed {
                                message: "iteration failed".to_owned(),
                            },
                        }));
                    }
                    let failure = match state.plan.fail_when.as_ref() {
                        Some(condition) => match evaluate_condition(condition, &self.context) {
                            Ok(value) => value,
                            Err(error) => {
                                self.restore_repeat_locals(&mut state);
                                self.fail_loop(
                                    step_id.clone(),
                                    index + 1,
                                    LoopFinishReason::StepFailed,
                                    &format!(
                                        "failed to evaluate repeat_until failure condition: {error}"
                                    ),
                                );
                                return Some(Ok(FlowEvent::IterationFinished {
                                    step_id,
                                    index,
                                    outcome: StepOutcome::Failed {
                                        message: "condition evaluation failed".to_owned(),
                                    },
                                }));
                            }
                        },
                        None => false,
                    };
                    let completed = if failure {
                        false
                    } else {
                        match evaluate_condition(&state.plan.until, &self.context) {
                            Ok(value) => value,
                            Err(error) => {
                                self.restore_repeat_locals(&mut state);
                                self.fail_loop(
                                    step_id.clone(),
                                    index + 1,
                                    LoopFinishReason::StepFailed,
                                    &format!("failed to evaluate repeat_until condition: {error}"),
                                );
                                return Some(Ok(FlowEvent::IterationFinished {
                                    step_id,
                                    index,
                                    outcome: StepOutcome::Failed {
                                        message: "condition evaluation failed".to_owned(),
                                    },
                                }));
                            }
                        }
                    };
                    self.pending.push_back(FlowEvent::IterationFinished {
                        step_id: step_id.clone(),
                        index,
                        outcome: StepOutcome::Succeeded,
                    });
                    state.index += 1;
                    if failure {
                        self.restore_repeat_locals(&mut state);
                        self.fail_loop(
                            step_id,
                            state.index,
                            LoopFinishReason::FailureCondition,
                            "repeat_until failure condition matched",
                        );
                    } else if completed {
                        self.restore_repeat_locals(&mut state);
                        self.last_result = true;
                        self.pending.push_back(FlowEvent::LoopFinished {
                            step_id: step_id.clone(),
                            total_executed: state.index,
                            reason: LoopFinishReason::ConditionMet,
                            success: true,
                        });
                        self.pending.push_back(FlowEvent::StepFinished {
                            step_id,
                            outcome: StepOutcome::Succeeded,
                        });
                    } else if state.index >= state.plan.max_iterations {
                        self.restore_repeat_locals(&mut state);
                        self.fail_loop(
                            step_id,
                            state.index,
                            LoopFinishReason::MaxIterations,
                            "repeat_until reached max_iterations",
                        );
                    } else if Instant::now() >= state.deadline {
                        self.restore_repeat_locals(&mut state);
                        self.fail_loop(
                            step_id,
                            state.index,
                            LoopFinishReason::Timeout,
                            "repeat_until timed out",
                        );
                    } else {
                        let mut carry_error = None;
                        for carry in &state.plan.carry {
                            let key = (carry.step_id.clone(), carry.output.clone());
                            match self.context.outputs.get(&key).cloned() {
                                Some(value) => {
                                    self.context.inputs.insert(carry.name.clone(), value);
                                }
                                None => {
                                    carry_error = Some(format!(
                                        "carry '{}' source '{}.{}' is unavailable",
                                        carry.name, carry.step_id, carry.output
                                    ));
                                    break;
                                }
                            }
                        }
                        if let Some(error) = carry_error {
                            self.restore_repeat_locals(&mut state);
                            self.fail_loop(
                                step_id,
                                state.index,
                                LoopFinishReason::StepFailed,
                                &error,
                            );
                            continue;
                        }
                        self.pending.push_back(FlowEvent::LoopWaiting {
                            step_id,
                            index,
                            interval_ms: state.plan.interval_ms,
                        });
                        self.tasks.push(Task::DelayRepeat(state));
                    }
                }
                Task::DelayRepeat(state) => {
                    let remaining = state.deadline.saturating_duration_since(Instant::now());
                    let delay = Duration::from_millis(state.plan.interval_ms).min(remaining);
                    futures_timer::Delay::new(delay).await;
                    self.tasks.push(Task::RepeatNext(state));
                }
            }
        }
    }

    async fn execute_http_step(
        &mut self,
        step_id: String,
        compiled_request: CompiledRequest,
        checks: Vec<CompiledCheck>,
        exports: Vec<CompiledExport>,
        deadline: Option<Instant>,
    ) {
        let request = match prepare_request(&compiled_request, &self.context) {
            Ok(request) => request,
            Err(message) => {
                self.fail_step(&step_id, message);
                return;
            }
        };

        tracing::info!(
            step_id = %step_id,
            method = %request.method,
            url = %self.context.redact_url(&request.url),
            "executing HTTP step"
        );
        for (name, value) in &request.headers {
            tracing::debug!(
                step_id = %step_id,
                header = %name,
                value = %self.context.redact_header(name, value),
                "request header"
            );
        }
        if let Some(body) = request.body.as_text() {
            if !body.is_empty() {
                tracing::debug!(
                    step_id = %step_id,
                    body = %self.context.redact(body),
                    "request body"
                );
            }
        }

        let mut options = self.environment.request_options;
        if let Some(timeout_ms) = compiled_request.template.options.timeout_ms {
            options.timeout_ms = Some(timeout_ms);
        }
        if let Some(policy) = compiled_request.template.options.redirect_policy {
            options.redirect_policy = policy;
        }
        if let Some(max_hops) = compiled_request.template.options.max_redirect_hops {
            options.max_redirect_hops = max_hops;
        }
        if let Some(deadline) = deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.fail_step(&step_id, "enclosing loop timed out".to_owned());
                return;
            }
            let remaining_ms = u64::try_from(remaining.as_millis())
                .unwrap_or(u64::MAX)
                .max(1);
            options.timeout_ms = Some(
                options
                    .timeout_ms
                    .map(|configured| configured.min(remaining_ms))
                    .unwrap_or(remaining_ms),
            );
        }

        let response = match self.transport.execute(request, options).await {
            Ok(response) => {
                tracing::info!(
                    step_id = %step_id,
                    status = response.status,
                    elapsed_ms = response.elapsed_ms,
                    "HTTP response received"
                );
                for (name, value) in &response.headers {
                    tracing::debug!(
                        step_id = %step_id,
                        header = %name,
                        value = %self.context.redact_header(name, value),
                        "response header"
                    );
                }
                if !response.body.is_empty() {
                    tracing::debug!(
                        step_id = %step_id,
                        body = %self.context.redact(&response.body),
                        "response body"
                    );
                }
                response
            }
            Err(error) => {
                let has_expected_error = checks
                    .iter()
                    .any(|check| matches!(check, CompiledCheck::Error(_)));
                if has_expected_error {
                    let mut checks_succeeded = true;
                    for check in &checks {
                        let result = evaluate_error_check(check, &error);
                        checks_succeeded &= result.success;
                        self.pending.push_back(FlowEvent::CheckFinished {
                            step_id: step_id.clone(),
                            check: result.description,
                            success: result.success,
                            message: result.message.map(|message| self.context.redact(&message)),
                        });
                    }
                    if checks_succeeded && exports.is_empty() {
                        self.pending.push_back(FlowEvent::StepFinished {
                            step_id,
                            outcome: StepOutcome::Succeeded,
                        });
                        self.last_result = true;
                        return;
                    }
                }
                tracing::error!(
                    step_id = %step_id,
                    error = %self.context.redact(&error.to_string()),
                    "HTTP step execution error"
                );
                self.last_loop_reason = Some(match &error {
                    HttpError::Cancelled => LoopFinishReason::Cancelled,
                    HttpError::Timeout { .. }
                        if deadline.is_some_and(|limit| Instant::now() >= limit) =>
                    {
                        LoopFinishReason::Timeout
                    }
                    _ => LoopFinishReason::StepFailed,
                });
                self.fail_step(&step_id, error.to_string());
                return;
            }
        };

        self.pending.push_back(FlowEvent::ResponseReceived {
            step_id: step_id.clone(),
            status: response.status,
            elapsed_ms: response.elapsed_ms,
        });

        let mut checks_succeeded = true;
        for check in &checks {
            let result = evaluate_check(check, &response, &self.context);
            checks_succeeded &= result.success;
            self.pending.push_back(FlowEvent::CheckFinished {
                step_id: step_id.clone(),
                check: result.description,
                success: result.success,
                message: result.message.map(|message| self.context.redact(&message)),
            });
        }

        if !checks_succeeded {
            self.fail_step(&step_id, "one or more response checks failed".to_owned());
            return;
        }

        let mut staged_outputs = Vec::with_capacity(exports.len());
        for export in &exports {
            match extract_output(export, &response) {
                Ok(value) => staged_outputs.push((export, value)),
                Err(message) => {
                    self.fail_step(&step_id, message);
                    return;
                }
            }
        }

        for (export, value) in staged_outputs {
            self.context.outputs.insert(
                (step_id.clone(), export.name.clone()),
                BoundValue {
                    value,
                    sensitive: export.sensitive,
                },
            );
            self.pending.push_back(FlowEvent::OutputExported {
                step_id: step_id.clone(),
                name: export.name.clone(),
            });
        }

        self.pending.push_back(FlowEvent::StepFinished {
            step_id,
            outcome: StepOutcome::Succeeded,
        });
        self.last_result = true;
    }

    fn fail_step(&mut self, step_id: &str, message: String) {
        self.last_result = false;
        if self.last_loop_reason.is_none() {
            self.last_loop_reason = Some(LoopFinishReason::StepFailed);
        }
        self.pending.push_back(FlowEvent::StepFinished {
            step_id: step_id.to_owned(),
            outcome: StepOutcome::Failed {
                message: self.context.redact(&message),
            },
        });
    }

    fn fail_loop(
        &mut self,
        step_id: String,
        total_executed: usize,
        reason: LoopFinishReason,
        message: &str,
    ) {
        self.last_result = false;
        self.last_loop_reason = Some(reason);
        self.pending.push_back(FlowEvent::LoopFinished {
            step_id: step_id.clone(),
            total_executed,
            reason,
            success: false,
        });
        self.pending.push_back(FlowEvent::StepFinished {
            step_id,
            outcome: StepOutcome::Failed {
                message: self.context.redact(message),
            },
        });
    }

    fn finish_for_each(&mut self, state: ForEachState) {
        for collection in &state.plan.collect {
            let values = state
                .collected
                .get(&collection.name)
                .expect("compiled collection exists");
            let sensitive = values.iter().any(|value| value.sensitive);
            self.context.outputs.insert(
                (state.step_id.clone(), collection.name.clone()),
                BoundValue {
                    value: Value::Array(values.iter().map(|value| value.value.clone()).collect()),
                    sensitive,
                },
            );
            self.pending.push_back(FlowEvent::OutputExported {
                step_id: state.step_id.clone(),
                name: collection.name.clone(),
            });
        }
        self.last_result = true;
        self.last_loop_reason = None;
        self.pending.push_back(FlowEvent::LoopFinished {
            step_id: state.step_id.clone(),
            total_executed: state.index,
            reason: LoopFinishReason::Completed,
            success: true,
        });
        self.pending.push_back(FlowEvent::StepFinished {
            step_id: state.step_id,
            outcome: StepOutcome::Succeeded,
        });
    }

    fn restore_loop_locals(&mut self, state: &mut ForEachState) {
        match state.saved_item.take() {
            Some(value) => {
                self.context
                    .inputs
                    .insert(state.plan.item_name.clone(), value);
            }
            None => {
                self.context.inputs.remove(&state.plan.item_name);
            }
        }
        if let Some(name) = &state.plan.index_name {
            match state.saved_index.take() {
                Some(value) => {
                    self.context.inputs.insert(name.clone(), value);
                }
                None => {
                    self.context.inputs.remove(name);
                }
            }
        }
    }

    fn restore_repeat_locals(&mut self, state: &mut RepeatState) {
        for (name, previous) in state.saved_carry.drain(..).rev() {
            match previous {
                Some(value) => {
                    self.context.inputs.insert(name, value);
                }
                None => {
                    self.context.inputs.remove(&name);
                }
            }
        }
    }

    fn clear_step_state(&mut self, steps: &[HttpStepPlan]) {
        let mut ids = Vec::new();
        collect_step_ids(steps, &mut ids);
        self.context
            .outputs
            .retain(|(step_id, _), _| !ids.contains(step_id));
        self.skipped_steps.retain(|step_id| !ids.contains(step_id));
    }
}

fn collect_step_ids(steps: &[HttpStepPlan], ids: &mut Vec<String>) {
    for step in steps {
        ids.push(step.id.clone());
        match &step.action {
            CompiledStepAction::ForEach(plan) => collect_step_ids(&plan.steps, ids),
            CompiledStepAction::RepeatUntil(plan) => collect_step_ids(&plan.steps, ids),
            CompiledStepAction::Http { .. } => {}
        }
    }
}

fn prepare_request(step: &CompiledRequest, context: &RunContext) -> Result<Request, String> {
    match &step.bindings {
        Some(bindings) => render_request(
            &step.template,
            &ApiContext {
                parent: context,
                bindings,
            },
        ),
        None => render_request(&step.template, context),
    }
}

fn render_request(
    step: &HttpRequestTemplate,
    context: &impl TemplateContext,
) -> Result<Request, String> {
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

    if let Some(crate::model::AuthTemplate::HmacSha256 { secret, param }) = &step.auth {
        sign_request(&mut request, &context.render(secret)?, param)?;
    }

    Ok(request)
}

fn sign_request(request: &mut Request, secret: &str, param: &str) -> Result<(), String> {
    crate::model::validate_signature_param(param)?;
    let mut url =
        url::Url::parse(&request.url).map_err(|error| format!("invalid signing URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("signing requires an absolute HTTP(S) URL".into());
    }
    // The transport uses the same URL serialization. Do not decode/re-encode query pairs,
    // reorder parameters or include a fragment, which is never sent over HTTP.
    url.set_fragment(None);
    let body = match &request.body {
        RequestBody::None => "",
        RequestBody::UrlEncoded(body) => body.as_str(),
        _ => return Err("hmac_sha256 requires an empty or url_encoded body".into()),
    };
    if url.query_pairs().any(|(name, _)| name == param)
        || url::form_urlencoded::parse(body.as_bytes()).any(|(name, _)| name == param)
    {
        return Err("request already contains the configured signature parameter".into());
    }
    // Binance-style totalParams is query + form body, with no added separator.
    let payload = format!("{}{body}", url.query().unwrap_or(""));
    let signature = compute_hmac_sha256(secret, &payload);
    if let RequestBody::UrlEncoded(body) = &mut request.body {
        if !body.is_empty() {
            body.push('&');
        }
        body.push_str(&format!("{param}={signature}"));
        if !request
            .headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        {
            request.add_header("Content-Type", "application/x-www-form-urlencoded");
        }
    } else {
        let query = match url.query() {
            Some(query) if !query.is_empty() => format!("{query}&{param}={signature}"),
            _ => format!("{param}={signature}"),
        };
        url.set_query(Some(&query));
    }
    request.url = url.to_string();
    Ok(())
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
    fn json_sensitive(&self, template: &JsonTemplate) -> bool {
        match template {
            JsonTemplate::Literal(_) => false,
            JsonTemplate::String(value) => self.text_sensitive(value),
            JsonTemplate::Input(name) => self.inputs.get(name).is_some_and(|value| value.sensitive),
            JsonTemplate::StepOutput { step_id, name } => self
                .outputs
                .get(&(step_id.clone(), name.clone()))
                .is_some_and(|value| value.sensitive),
            JsonTemplate::Coalesce(items) | JsonTemplate::Array(items) => {
                items.iter().any(|item| self.json_sensitive(item))
            }
            JsonTemplate::Object(fields) => fields.values().any(|value| self.json_sensitive(value)),
            JsonTemplate::Calc(expression) => self.calc_sensitive(expression),
        }
    }

    fn text_sensitive(&self, template: &TextTemplate) -> bool {
        template.parts.iter().any(|part| match part {
            TemplatePart::Literal(_) => false,
            TemplatePart::Input(name) => self.inputs.get(name).is_some_and(|value| value.sensitive),
            TemplatePart::StepOutput { step_id, name } => self
                .outputs
                .get(&(step_id.clone(), name.clone()))
                .is_some_and(|value| value.sensitive),
            TemplatePart::Coalesce(items) => items.iter().any(|item| self.text_sensitive(item)),
            TemplatePart::Calc(expression) => self.calc_sensitive(expression),
        })
    }

    fn calc_sensitive(&self, source: &str) -> bool {
        crate::calc::Expression::parse(source).is_ok_and(|expression| {
            expression.variables().any(|name| {
                self.inputs
                    .get(name)
                    .or_else(|| {
                        name.split_once('.').and_then(|(step, output)| {
                            self.outputs.get(&(step.to_owned(), output.to_owned()))
                        })
                    })
                    .is_some_and(|value| value.sensitive)
            })
        })
    }

    fn resolve_value_ref(&self, reference: &ValueReference) -> Option<BoundValue> {
        match reference {
            ValueReference::Input(name) => {
                if let Some(val) = self.inputs.get(name).cloned() {
                    Some(val)
                } else {
                    eval_builtin_variable(name).map(|value| BoundValue {
                        value,
                        sensitive: false,
                    })
                }
            }
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
}

trait TemplateContext {
    fn input(&self, name: &str) -> Result<Value, String>;
    fn output(&self, step: &str, name: &str) -> Result<Value, String>;

    fn resolve_json(&self, template: &JsonTemplate) -> Result<Value, String> {
        match template {
            JsonTemplate::Literal(value) => Ok(value.clone()),
            JsonTemplate::String(value) => self.render(value).map(Value::String),
            JsonTemplate::Input(name) => self.input(name),
            JsonTemplate::Calc(expr) => {
                let val = evaluate_calc_in_context(expr, self)?;
                if val.fract() == 0.0 && val.abs() < 1e15 {
                    Ok(Value::Number((val as i64).into()))
                } else {
                    serde_json::Number::from_f64(val)
                        .map(Value::Number)
                        .ok_or_else(|| "calc result must be finite".into())
                }
            }
            JsonTemplate::StepOutput { step_id, name } => self.output(step_id, name),
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
            TemplatePart::Input(name) => self.input(name).map(|value| value_as_text(&value)),
            TemplatePart::Calc(expr) => {
                let val = evaluate_calc_in_context(expr, self)?;
                if val.fract() == 0.0 && val.abs() < 1e15 {
                    Ok((val as i64).to_string())
                } else {
                    Ok(val.to_string())
                }
            }
            TemplatePart::StepOutput { step_id, name } => self
                .output(step_id, name)
                .map(|value| value_as_text(&value)),
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
}

impl TemplateContext for RunContext {
    fn input(&self, name: &str) -> Result<Value, String> {
        self.inputs
            .get(name)
            .map(|value| value.value.clone())
            .or_else(|| eval_builtin_variable(name))
            .ok_or_else(|| format!("input `{name}` is not bound"))
    }

    fn output(&self, step: &str, name: &str) -> Result<Value, String> {
        self.outputs
            .get(&(step.to_owned(), name.to_owned()))
            .map(|value| value.value.clone())
            .ok_or_else(|| format!("output `{step}.{name}` is not available at runtime"))
    }
}

struct ApiContext<'a> {
    parent: &'a RunContext,
    bindings: &'a BTreeMap<String, TextTemplate>,
}

impl TemplateContext for ApiContext<'_> {
    fn input(&self, name: &str) -> Result<Value, String> {
        match self.bindings.get(name) {
            Some(binding) => match binding.parts.as_slice() {
                // Preserve the existing typed JSON argument semantics. Resolve lazily so
                // a missing export can still be handled by a template's coalesce branch.
                [TemplatePart::Input(name)] => self.parent.input(name),
                [TemplatePart::StepOutput { step_id, name }] => self.parent.output(step_id, name),
                _ => self.parent.render(binding).map(Value::String),
            },
            None if is_builtin_variable(name) => self.parent.input(name),
            None => Err(format!("API parameter `{name}` is not bound")),
        }
    }

    fn output(&self, step: &str, name: &str) -> Result<Value, String> {
        Err(format!(
            "output `{step}.{name}` must be supplied through an API binding"
        ))
    }
}

impl RunContext {
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
            parse_condition_number(s).is_some_and(|parsed| condition_decimal(n) == parsed)
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
                Ok(n1.cmp(&n2))
            } else {
                Ok(s1.cmp(s2))
            }
        }
        (Value::Number(n), Value::String(s)) => {
            let parsed =
                parse_condition_number(s).ok_or("cannot compare non-numeric string to number")?;
            Ok(condition_decimal(n).cmp(&parsed))
        }
        (Value::String(s), Value::Number(n)) => {
            let parsed =
                parse_condition_number(s).ok_or("cannot compare non-numeric string to number")?;
            Ok(parsed.cmp(&condition_decimal(n)))
        }
        _ => Err(format!(
            "cannot compare values of types {} and {}",
            json_type_name(a),
            json_type_name(b)
        )),
    }
}

fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn parse_condition_number(text: &str) -> Option<BigDecimal> {
    // Keep decimal/scientific notation syntax; BigDecimal also accepts underscores,
    // which would unexpectedly turn ordinary string ordering into numeric ordering.
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    let mantissa = if let Some((mantissa, exponent)) = unsigned.split_once(['e', 'E']) {
        let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        if exponent.is_empty() || !exponent.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        mantissa
    } else {
        unsigned
    };
    let mut digits = 0;
    let mut dots = 0;
    for byte in mantissa.bytes() {
        if byte.is_ascii_digit() {
            digits += 1;
        } else if byte == b'.' {
            dots += 1;
        } else {
            return None;
        }
    }
    if digits == 0 || dots > 1 {
        return None;
    }
    text.parse().ok()
}

fn condition_decimal(number: &serde_json::Number) -> BigDecimal {
    // JSON integers remain exact. For existing floats, use their serialized decimal
    // representation instead of introducing the binary expansion of an f64.
    number.to_string().parse().expect("finite JSON number")
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

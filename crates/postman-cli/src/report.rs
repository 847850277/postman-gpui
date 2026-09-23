use std::{collections::BTreeMap, fmt, time::Instant};

use postman_flow::{
    FlowEvent, HttpRequestSource, HttpStepDefinition, LoopFinishReason, LoopKind, StepOutcome,
};
use serde::{Serialize, Serializer};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RunReport {
    pub success: bool,
    pub requests: Vec<RequestReport>,
    /// Each loop invocation, including repeated invocations inside an outer loop.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loops: Vec<LoopReport>,
    /// Declared flow returns. Sensitive values are replaced before entering the report.
    pub outputs: BTreeMap<String, serde_json::Value>,
    pub redacted_outputs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RequestReport {
    pub name: String,
    pub success: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub skipped: bool,
    pub status: Option<u16>,
    pub elapsed_ms: Option<u128>,
    pub assertions: Vec<AssertionReport>,
    pub captures: Vec<String>,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loop_path: Vec<LoopIteration>,
}

impl RequestReport {
    pub fn display_name(&self) -> String {
        display_name(&self.loop_path, &self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssertionReport {
    pub expression: String,
    pub success: bool,
    pub message: Option<String>,
}

/// A one-based iteration number identifying an enclosing loop invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoopIteration {
    pub step_id: String,
    pub iteration: usize,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IterationReport {
    pub iteration: usize,
    pub success: bool,
    /// Body execution time, excluding the following polling wait.
    pub elapsed_ms: u128,
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait_interval_ms: Option<u64>,
    /// Actual wait observed by the CLI, including a wait cut short by the loop deadline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait_elapsed_ms: Option<u128>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoopReport {
    pub step_id: String,
    pub name: String,
    #[serde(serialize_with = "serialize_loop_kind")]
    pub kind: LoopKind,
    pub limit: usize,
    pub loop_path: Vec<LoopIteration>,
    pub total_executed: usize,
    pub elapsed_ms: u128,
    pub success: bool,
    pub skipped: bool,
    #[serde(serialize_with = "serialize_loop_reason")]
    pub reason: Option<LoopFinishReason>,
    pub iterations: Vec<IterationReport>,
    pub captures: Vec<String>,
    pub error: Option<String>,
}

impl LoopReport {
    pub fn display_name(&self) -> String {
        display_name(&self.loop_path, &self.name)
    }

    pub fn summary(&self) -> String {
        let marker = if self.skipped {
            "SKIPPED"
        } else if self.success {
            "PASS"
        } else {
            "FAIL"
        };
        let reason = self
            .reason
            .map(loop_reason_name)
            .unwrap_or(if self.skipped {
                "skipped"
            } else {
                "unfinished"
            });
        format!(
            "{marker} LOOP {} [{}/{}] — {reason} ({} ms)",
            self.display_name(),
            self.total_executed,
            self.limit,
            self.elapsed_ms
        )
    }
}

/// Live loop progress. Callbacks must return promptly because they run on the event consumer.
#[derive(Debug)]
pub enum LoopProgress<'a> {
    IterationStarted {
        report: &'a LoopReport,
        iteration: usize,
    },
    Waiting {
        report: &'a LoopReport,
        iteration: usize,
        interval_ms: u64,
    },
    Finished(&'a LoopReport),
}

impl fmt::Display for LoopProgress<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IterationStarted { report, iteration } => write!(
                f,
                "LOOP {} [{iteration}/{}] — running",
                report.display_name(),
                report.limit
            ),
            Self::Waiting {
                report,
                iteration,
                interval_ms,
            } => write!(
                f,
                "WAIT {} [{iteration}/{}] — interval {interval_ms} ms",
                report.display_name(),
                report.limit
            ),
            Self::Finished(report) => {
                write!(f, "{}", report.summary())?;
                if let Some(error) = &report.error {
                    write!(f, "\n  {error}")?;
                }
                Ok(())
            }
        }
    }
}

fn display_name(path: &[LoopIteration], name: &str) -> String {
    let mut display = String::new();
    for item in path {
        use fmt::Write;
        let _ = write!(
            display,
            "{}[{}/{}] > ",
            item.step_id, item.iteration, item.limit
        );
    }
    display.push_str(name);
    display
}

fn step_name(id: &str, name: &str) -> String {
    if name.is_empty() || name == id {
        id.to_owned()
    } else {
        format!("{id} ({name})")
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn serialize_loop_kind<S: Serializer>(kind: &LoopKind, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(match kind {
        LoopKind::ForEach => "for_each",
        LoopKind::RepeatUntil => "repeat_until",
    })
}

fn loop_reason_name(reason: LoopFinishReason) -> &'static str {
    match reason {
        LoopFinishReason::Completed => "completed",
        LoopFinishReason::ConditionMet => "condition_met",
        LoopFinishReason::FailureCondition => "failure_condition",
        LoopFinishReason::MaxIterations => "max_iterations",
        LoopFinishReason::Timeout => "timeout",
        LoopFinishReason::StepFailed => "step_failed",
        LoopFinishReason::Cancelled => "cancelled",
    }
}

fn serialize_loop_reason<S: Serializer>(
    reason: &Option<LoopFinishReason>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    reason.map(loop_reason_name).serialize(serializer)
}

#[derive(Default)]
pub(crate) struct ReportBuilder {
    report: RunReport,
    loop_specs: BTreeMap<String, (LoopKind, usize)>,
    requests: BTreeMap<String, RequestReport>,
    active_loops: Vec<ActiveLoop>,
}

struct ActiveLoop {
    report_index: usize,
    started_at: Instant,
    iteration_started_at: Option<Instant>,
    waiting_since: Option<Instant>,
}

impl ReportBuilder {
    pub(crate) fn new(steps: &[HttpStepDefinition]) -> Self {
        let mut builder = Self::default();
        builder.register_loops(steps);
        builder
    }

    fn register_loops(&mut self, steps: &[HttpStepDefinition]) {
        for step in steps {
            let (kind, limit, children) = match &step.request {
                HttpRequestSource::ForEach(plan) => {
                    (LoopKind::ForEach, plan.max_iterations, &plan.steps)
                }
                HttpRequestSource::RepeatUntil(plan) => {
                    (LoopKind::RepeatUntil, plan.max_iterations, &plan.steps)
                }
                HttpRequestSource::Inline(_) | HttpRequestSource::Api(_) => continue,
            };
            self.loop_specs.insert(step.id.clone(), (kind, limit));
            self.register_loops(children);
        }
    }

    fn path(&self) -> Vec<LoopIteration> {
        self.active_loops
            .iter()
            .filter_map(|active| {
                let report = &self.report.loops[active.report_index];
                report.iterations.last().map(|iteration| LoopIteration {
                    step_id: report.step_id.clone(),
                    iteration: iteration.iteration,
                    limit: report.limit,
                })
            })
            .collect()
    }

    fn active_loop(&self, step_id: &str) -> Option<usize> {
        self.active_loops
            .iter()
            .position(|active| self.report.loops[active.report_index].step_id == step_id)
    }

    fn finish_wait(&mut self, active_index: usize) {
        let active = &mut self.active_loops[active_index];
        if let Some(started) = active.waiting_since.take() {
            if let Some(iteration) = self.report.loops[active.report_index].iterations.last_mut() {
                iteration.wait_elapsed_ms = Some(started.elapsed().as_millis());
            }
        }
    }

    fn new_loop(&mut self, id: String, name: String, kind: LoopKind, limit: usize) -> usize {
        let report = LoopReport {
            name: step_name(&id, &name),
            step_id: id,
            kind,
            limit,
            loop_path: self.path(),
            total_executed: 0,
            elapsed_ms: 0,
            success: false,
            skipped: false,
            reason: None,
            iterations: Vec::new(),
            captures: Vec::new(),
            error: None,
        };
        let index = self.report.loops.len();
        self.report.loops.push(report);
        index
    }

    fn new_request(&self, id: &str, name: &str) -> RequestReport {
        RequestReport {
            name: step_name(id, name),
            success: false,
            skipped: false,
            status: None,
            elapsed_ms: None,
            assertions: Vec::new(),
            captures: Vec::new(),
            error: None,
            loop_path: self.path(),
        }
    }

    pub(crate) fn record(&mut self, event: FlowEvent, progress: &mut impl FnMut(LoopProgress<'_>)) {
        match event {
            FlowEvent::FlowStarted { .. } => {}
            FlowEvent::StepStarted { step_id, name } => {
                if let Some(&(kind, limit)) = self.loop_specs.get(&step_id) {
                    let report_index = self.new_loop(step_id, name, kind, limit);
                    self.active_loops.push(ActiveLoop {
                        report_index,
                        started_at: Instant::now(),
                        iteration_started_at: None,
                        waiting_since: None,
                    });
                } else {
                    let request = self.new_request(&step_id, &name);
                    self.requests.insert(step_id, request);
                }
            }
            FlowEvent::StepSkipped {
                step_id,
                name,
                reason,
            } => {
                if let Some(&(kind, limit)) = self.loop_specs.get(&step_id) {
                    let index = self.new_loop(step_id, name, kind, limit);
                    let report = &mut self.report.loops[index];
                    report.skipped = true;
                    report.success = true;
                    report.error = Some(format!("Skipped: {reason}"));
                    progress(LoopProgress::Finished(report));
                } else {
                    let mut request = self.new_request(&step_id, &name);
                    request.success = true;
                    request.skipped = true;
                    request.error = Some(format!("Skipped: {reason}"));
                    self.report.requests.push(request);
                }
            }
            FlowEvent::ResponseReceived {
                step_id,
                status,
                elapsed_ms,
            } => {
                if let Some(request) = self.requests.get_mut(&step_id) {
                    request.status = Some(status);
                    request.elapsed_ms = Some(elapsed_ms);
                }
            }
            FlowEvent::CheckFinished {
                step_id,
                check,
                success,
                message,
            } => {
                if let Some(request) = self.requests.get_mut(&step_id) {
                    request.assertions.push(AssertionReport {
                        expression: check,
                        success,
                        message,
                    });
                }
            }
            FlowEvent::OutputExported { step_id, name } => {
                if let Some(active_index) = self.active_loop(&step_id) {
                    self.report.loops[self.active_loops[active_index].report_index]
                        .captures
                        .push(name);
                } else if let Some(request) = self.requests.get_mut(&step_id) {
                    request.captures.push(name);
                }
            }
            FlowEvent::StepFinished { step_id, outcome } => {
                let (success, error) = match outcome {
                    StepOutcome::Succeeded => (true, None),
                    StepOutcome::Failed { message } => (false, Some(message)),
                };
                if let Some(active_index) = self.active_loop(&step_id) {
                    self.finish_wait(active_index);
                    let active = self.active_loops.remove(active_index);
                    let report = &mut self.report.loops[active.report_index];
                    report.elapsed_ms = active.started_at.elapsed().as_millis();
                    report.success = success;
                    report.error = error;
                    // Guard or item-resolution failures can precede LoopStarted/LoopFinished.
                    if report.reason.is_none() && !success {
                        report.reason = Some(LoopFinishReason::StepFailed);
                    }
                    progress(LoopProgress::Finished(report));
                } else if let Some(mut request) = self.requests.remove(&step_id) {
                    request.success = success;
                    request.error = error;
                    self.report.requests.push(request);
                }
            }
            FlowEvent::LoopStarted {
                step_id,
                kind,
                limit,
            } => {
                if let Some(index) = self.active_loop(&step_id) {
                    let report = &mut self.report.loops[self.active_loops[index].report_index];
                    report.kind = kind;
                    report.limit = limit;
                }
            }
            FlowEvent::IterationStarted { step_id, index } => {
                if let Some(active_index) = self.active_loop(&step_id) {
                    self.finish_wait(active_index);
                    let active = &mut self.active_loops[active_index];
                    active.iteration_started_at = Some(Instant::now());
                    let report = &mut self.report.loops[active.report_index];
                    report.iterations.push(IterationReport {
                        iteration: index + 1,
                        success: false,
                        elapsed_ms: 0,
                        error: None,
                        wait_interval_ms: None,
                        wait_elapsed_ms: None,
                    });
                    progress(LoopProgress::IterationStarted {
                        report,
                        iteration: index + 1,
                    });
                }
            }
            FlowEvent::IterationFinished {
                step_id,
                index,
                outcome,
            } => {
                if let Some(active_index) = self.active_loop(&step_id) {
                    let active = &mut self.active_loops[active_index];
                    let elapsed = active
                        .iteration_started_at
                        .take()
                        .map(|start| start.elapsed().as_millis())
                        .unwrap_or(0);
                    if let Some(iteration) = self.report.loops[active.report_index]
                        .iterations
                        .last_mut()
                        .filter(|iteration| iteration.iteration == index + 1)
                    {
                        iteration.elapsed_ms = elapsed;
                        match outcome {
                            StepOutcome::Succeeded => iteration.success = true,
                            StepOutcome::Failed { message } => iteration.error = Some(message),
                        }
                    }
                }
            }
            FlowEvent::LoopWaiting {
                step_id,
                index,
                interval_ms,
            } => {
                if let Some(active_index) = self.active_loop(&step_id) {
                    let active = &mut self.active_loops[active_index];
                    active.waiting_since = Some(Instant::now());
                    let report = &mut self.report.loops[active.report_index];
                    if let Some(iteration) = report.iterations.last_mut() {
                        iteration.wait_interval_ms = Some(interval_ms);
                    }
                    progress(LoopProgress::Waiting {
                        report,
                        iteration: index + 1,
                        interval_ms,
                    });
                }
            }
            FlowEvent::LoopFinished {
                step_id,
                total_executed,
                reason,
                success,
            } => {
                if let Some(active_index) = self.active_loop(&step_id) {
                    self.finish_wait(active_index);
                    let report =
                        &mut self.report.loops[self.active_loops[active_index].report_index];
                    report.total_executed = total_executed;
                    report.reason = Some(reason);
                    report.success = success;
                }
            }
            FlowEvent::FlowFinished { success, outputs } => {
                self.report.success = success;
                if success {
                    for (name, output) in outputs {
                        let value = if output.is_sensitive() {
                            self.report.redacted_outputs.push(name.clone());
                            serde_json::Value::String("[REDACTED]".into())
                        } else {
                            output.value().clone()
                        };
                        self.report.outputs.insert(name, value);
                    }
                }
            }
        }
    }

    pub(crate) fn finish(self) -> RunReport {
        self.report
    }
}

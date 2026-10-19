//! Bounded projection of engine events. Only safe inspection values enter UI/copy state.
use postman_flow::{FlowDocument, FlowEvent, LoopFinishReason, StepOutcome};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum Status {
    NotExecuted,
    Running,
    Waiting,
    Stopping,
    Succeeded,
    Failed,
    Skipped,
    Cancelled,
}
impl Status {
    pub(in super::super) fn label(self) -> &'static str {
        match self {
            Self::NotExecuted => "Not executed",
            Self::Running => "Running",
            Self::Waiting => "Waiting",
            Self::Stopping => "Stopping…",
            Self::Succeeded => "Succeeded",
            Self::Failed => "Failed",
            Self::Skipped => "Skipped",
            Self::Cancelled => "Cancelled",
        }
    }
    pub(in super::super) fn active(self) -> bool {
        matches!(self, Self::Running | Self::Waiting | Self::Stopping)
    }
}

#[derive(Clone)]
pub(in super::super) struct Step {
    pub(in super::super) id: String,
    pub(in super::super) status: Status,
}
#[derive(Clone)]
pub(in super::super) struct Run {
    pub(in super::super) id: u64,
    pub(in super::super) revision: u64,
    pub(in super::super) status: Status,
    pub(in super::super) steps: Vec<Step>,
    pub(in super::super) elapsed: Duration,
    pub(in super::super) output_count: usize,
    pub(in super::super) results: super::results::Results,
    started: Instant,
    loops: Vec<String>,
}
pub(in super::super) struct Feedback {
    pub(in super::super) status: Status,
    pub(in super::super) message: String,
    pub(in super::super) step: Option<String>,
}
impl Run {
    pub(super) fn new(id: u64, revision: u64, document: &FlowDocument) -> Self {
        Self {
            id,
            revision,
            results: super::results::Results::new(document),
            status: Status::Running,
            elapsed: Duration::ZERO,
            started: Instant::now(),
            loops: Vec::new(),
            output_count: 0,
            steps: document
                .flow
                .steps
                .iter()
                .map(|step| Step {
                    id: step.id.clone(),
                    status: Status::NotExecuted,
                })
                .collect(),
        }
    }
    pub(in super::super) fn label(&self, revision: u64) -> String {
        format!(
            "{} · {:.1}s{}",
            self.status.label(),
            self.elapsed.as_secs_f32(),
            if revision == self.revision {
                ""
            } else {
                " · Earlier revision"
            }
        )
    }
    pub(super) fn tick(&mut self) {
        if self.status.active() {
            self.elapsed = self.started.elapsed();
        }
    }
    pub(super) fn finish(&mut self, status: Status) {
        self.tick();
        self.status = if self.status == Status::Stopping {
            Status::Cancelled
        } else {
            status
        };
        self.results.finish(self.status);
        for step in &mut self.steps {
            if step.status.active() {
                step.status = if self.status == Status::Cancelled {
                    Status::Cancelled
                } else {
                    Status::Failed
                };
            }
        }
        if self.status != Status::Succeeded {
            self.output_count = 0;
        }
    }
    pub(super) fn event(&mut self, event: FlowEvent, observed: Instant) -> Option<Feedback> {
        self.tick();
        self.results.event(&event, observed);
        let (id, status, message, terminal_step) = match event {
            FlowEvent::FlowStarted { total_steps, .. } => {
                return Some(Feedback {
                    status: Status::Running,
                    message: format!("Run started · {total_steps} top-level steps."),
                    step: None,
                })
            }
            FlowEvent::StepStarted { step_id, .. } => {
                (step_id, Status::Running, "Step started.".into(), false)
            }
            FlowEvent::StepSkipped { step_id, .. } => (
                step_id,
                Status::Skipped,
                "Skipped by run condition.".into(),
                true,
            ),
            FlowEvent::ResponseReceived {
                step_id,
                status,
                elapsed_ms,
            } => (
                step_id,
                Status::Running,
                format!("HTTP {status} · {elapsed_ms} ms"),
                false,
            ),
            FlowEvent::CheckFinished {
                step_id, success, ..
            } => (
                step_id,
                if success {
                    Status::Succeeded
                } else {
                    Status::Failed
                },
                if success {
                    "Response check passed."
                } else {
                    "Response check failed."
                }
                .into(),
                false,
            ),
            FlowEvent::OutputExported { step_id, .. } => (
                step_id,
                Status::Running,
                "Output extracted · values omitted from logs.".into(),
                false,
            ),
            FlowEvent::StepFinished { step_id, outcome } => match outcome {
                StepOutcome::Succeeded => {
                    (step_id, Status::Succeeded, "Step succeeded.".into(), true)
                }
                StepOutcome::Failed { message } => (
                    step_id,
                    Status::Failed,
                    failure_summary(&message).into(),
                    true,
                ),
            },
            FlowEvent::LoopStarted {
                step_id,
                kind,
                limit,
            } => {
                self.loops.push(step_id.clone());
                (
                    step_id,
                    Status::Running,
                    format!(
                        "{} started · limit {limit}.",
                        match kind {
                            postman_flow::LoopKind::ForEach => "For each",
                            postman_flow::LoopKind::RepeatUntil => "Repeat until",
                        }
                    ),
                    false,
                )
            }
            FlowEvent::IterationStarted { step_id, index } => (
                step_id,
                Status::Running,
                format!("Iteration {} started.", index + 1),
                false,
            ),
            FlowEvent::IterationFinished {
                step_id,
                index,
                outcome,
            } => (
                step_id,
                if matches!(outcome, StepOutcome::Succeeded) {
                    Status::Succeeded
                } else {
                    Status::Failed
                },
                format!("Iteration {} finished.", index + 1),
                false,
            ),
            FlowEvent::LoopWaiting {
                step_id,
                index,
                interval_ms,
            } => (
                step_id,
                Status::Waiting,
                format!("Waiting {interval_ms} ms after iteration {}.", index + 1),
                false,
            ),
            FlowEvent::LoopFinished {
                step_id,
                total_executed,
                reason,
                success,
            } => {
                let owner = self
                    .loops
                    .first()
                    .cloned()
                    .unwrap_or_else(|| step_id.clone());
                if self.loops.last() == Some(&step_id) {
                    self.loops.pop();
                }
                let status = if success {
                    Status::Succeeded
                } else if reason == LoopFinishReason::Cancelled {
                    Status::Cancelled
                } else {
                    Status::Failed
                };
                // The subsequent StepFinished event owns the terminal top-level step state.
                return Some(Feedback {
                    status,
                    step: Some(owner),
                    message: format!(
                        "Loop finished · {total_executed} iterations · {}.",
                        loop_reason(reason)
                    ),
                });
            }
            FlowEvent::FlowFinished { success, outputs } => {
                self.output_count = if success { outputs.len() } else { 0 };
                // Completion is committed only after the worker has dropped the stream.
                return None;
            }
        };
        let owner = self.loops.first().cloned().unwrap_or_else(|| id.clone());
        if let Some(step) = self.steps.iter_mut().find(|s| s.id == owner) {
            if terminal_step && self.loops.is_empty()
                || matches!(status, Status::Running | Status::Waiting)
            {
                step.status = status;
            }
        }
        Some(Feedback {
            status,
            message,
            step: Some(owner),
        })
    }
}

pub(super) fn failure_summary(message: &str) -> &'static str {
    if message.starts_with("HTTP request timed out") {
        "Request timed out."
    } else if message.starts_with("HTTP network error:") {
        "Network request failed. Check connectivity and the request URL."
    } else if message.starts_with("output '") && message.ends_with("requires a JSON response body")
    {
        "Extraction failed: response body is not valid JSON."
    } else if message.starts_with("output '") && message.contains("could not be exported:") {
        "Extraction failed: JSON path could not be resolved."
    } else if message.starts_with("failed to evaluate condition:") {
        "Run condition could not be evaluated; no request sent."
    } else if message == "one or more response checks failed" {
        "Response checks failed."
    } else if message.starts_with("invalid HTTP request:") {
        "Invalid HTTP request. Review URL, headers and body/file references."
    } else if message.starts_with("HTTP redirect limit exceeded") {
        "HTTP redirect limit exceeded."
    } else if message.starts_with("HTTP response body size") {
        "HTTP response exceeded the size limit."
    } else if message == "HTTP request was cancelled" {
        "HTTP transport cancelled the request."
    } else {
        "Step failed. Review conditions, request configuration and output expressions."
    }
}

pub(super) fn loop_reason(reason: LoopFinishReason) -> &'static str {
    match reason {
        LoopFinishReason::Completed => "Completed",
        LoopFinishReason::ConditionMet => "Condition met",
        LoopFinishReason::FailureCondition => "Failure condition met",
        LoopFinishReason::MaxIterations => "Iteration limit reached",
        LoopFinishReason::Timeout => "Timed out",
        LoopFinishReason::StepFailed => "Step failed",
        LoopFinishReason::Cancelled => "Cancelled",
    }
}

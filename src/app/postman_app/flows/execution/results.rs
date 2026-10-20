//! Session-only inspection. IDs are row indices within a run, never unscoped DSL IDs.
use super::model::{failure_summary, loop_reason, Status};
use postman_flow::{
    FlowDocument, FlowEvent, HttpRequestSource, HttpStepDefinition, InspectionValue, StepOutcome,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub(in super::super) const MAX_ROWS: usize = 2000;
const MAX_VALUES: usize = 128;
const MAX_CHECKS: usize = 128;
const MAX_VALUE_BYTES: usize = 2 * 1024 * 1024;
const MAX_HTTP_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum HttpPart {
    Request,
    Response,
}
impl HttpPart {
    pub fn label(self) -> &'static str {
        match self {
            Self::Request => "Request",
            Self::Response => "Response",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct HttpTarget {
    pub run: u64,
    pub node: usize,
    pub part: HttpPart,
}

#[derive(Clone)]
struct Template {
    id: String,
    name: String,
    children: Vec<Arc<Template>>,
    checks: usize,
    exports: usize,
    limit: Option<usize>,
    interval: Option<u64>,
}
impl Template {
    fn from_step(step: &HttpStepDefinition) -> Arc<Self> {
        Arc::new(Self {
            id: step.id.clone(),
            name: step.name.chars().take(256).collect(),
            children: match &step.request {
                HttpRequestSource::ForEach(v) => v.steps.iter().map(Self::from_step).collect(),
                HttpRequestSource::RepeatUntil(v) => v.steps.iter().map(Self::from_step).collect(),
                _ => Vec::new(),
            },
            checks: step.checks.len(),
            exports: match &step.request {
                HttpRequestSource::ForEach(v) => v.collect.len(),
                _ => step.exports.len(),
            },
            limit: match &step.request {
                HttpRequestSource::ForEach(v) => Some(v.max_iterations),
                HttpRequestSource::RepeatUntil(v) => Some(v.max_iterations),
                _ => None,
            },
            interval: match &step.request {
                HttpRequestSource::RepeatUntil(v) => Some(v.interval_ms),
                _ => None,
            },
        })
    }
}
#[derive(Clone)]
pub(in super::super) struct NamedValue {
    pub name: String,
    pub value: InspectionValue,
}
#[derive(Clone)]
pub(in super::super) struct Check {
    pub success: bool,
    pub detail: String,
}
#[derive(Clone)]
pub(in super::super) struct Node {
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub owner: String,
    pub step: String,
    pub path: String,
    pub name: String,
    pub status: Status,
    pub detail: String,
    pub response: Option<(u16, u128)>,
    pub http: [Option<NamedValue>; 2],
    pub http_omitted: bool,
    pub checks: Vec<Check>,
    pub check_count: usize,
    pub expected_checks: usize,
    pub values: Vec<NamedValue>,
    pub value_count: usize,
    pub expected_values: usize,
    pub elapsed: Option<Duration>,
    pub wait: Duration,
    pub interval: Option<u64>,
    waited: bool,
    pub limit: Option<usize>,
    pub executed: Option<usize>,
    pub iteration: Option<usize>,
    started: Option<Instant>,
    waiting: Option<Instant>,
}
impl Node {
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if let Some(index) = self.iteration {
            parts.push(format!("Iteration {}", index + 1));
        }
        if let Some((status, elapsed)) = self.response {
            parts.push(format!("HTTP {status} · {elapsed} ms"));
            if !(200..300).contains(&status) && self.expected_checks == 0 {
                parts.push(
                    "No response checks configured; HTTP status does not fail this step".into(),
                );
            }
        }
        if let Some(limit) = self.limit {
            parts.push(format!(
                "{} executed · Limit {limit}",
                self.executed.map_or("—".into(), |v| v.to_string())
            ));
        }
        if let Some(elapsed) = self.elapsed {
            parts.push(format!(
                "Observed body {:.0} ms",
                elapsed.as_secs_f64() * 1000.
            ));
        }
        if let Some(interval) = self.interval {
            parts.push(format!(
                "Interval {interval} ms · Observed wait {}",
                if self.waited {
                    format!("{:.0} ms", self.wait.as_secs_f64() * 1000.)
                } else {
                    "—".into()
                }
            ));
        }
        if !self.detail.is_empty() {
            parts.push(self.detail.clone());
        }
        parts.join(" · ")
    }
    fn settle_wait(&mut self, observed: Instant) {
        if let Some(started) = self.waiting.take() {
            self.wait += observed.saturating_duration_since(started);
        }
    }
    fn finish(&mut self, status: Status, observed: Instant) {
        self.settle_wait(observed);
        self.status = status;
        self.elapsed = self.started.take().map(|t| {
            observed
                .saturating_duration_since(t)
                .saturating_sub(self.wait)
        });
    }
}
#[derive(Clone, Default)]
pub(in super::super) struct Results {
    pub nodes: Vec<Arc<Node>>,
    pub roots: Vec<usize>,
    pub total_rows: usize,
    pub outputs: Vec<NamedValue>,
    pub output_count: usize,
    pub complete: bool,
    value_bytes: usize,
    http_bytes: usize,
    outputs_copy: Option<String>,
    templates: Vec<Arc<Template>>,
    active_templates: Vec<Option<Arc<Template>>>,
    active: Vec<Option<usize>>,
    loops: Vec<Frame>,
}
#[derive(Clone)]
struct Frame {
    template: Option<Arc<Template>>,
    node: Option<usize>,
    iteration: Option<usize>,
}
impl Results {
    pub fn new(document: &FlowDocument) -> Self {
        let mut result = Self::default();
        for step in &document.flow.steps {
            let template = Template::from_step(step);
            result.templates.push(template.clone());
            result.insert(None, template, None);
        }
        result
    }
    fn insert(
        &mut self,
        parent: Option<usize>,
        template: Arc<Template>,
        iteration: Option<usize>,
    ) -> Option<usize> {
        self.total_rows = self.total_rows.saturating_add(1);
        if self.nodes.len() >= MAX_ROWS {
            return None;
        }
        let id = self.nodes.len();
        let owner = parent.map_or_else(|| template.id.clone(), |p| self.nodes[p].owner.clone());
        self.nodes.push(Arc::new(Node {
            parent,
            owner,
            path: format!(
                "{}{}",
                parent.map_or(String::new(), |p| format!("{} / ", self.nodes[p].path)),
                iteration.map_or_else(|| template.id.clone(), |i| format!("Iteration {}", i + 1))
            ),
            step: template.id.clone(),
            name: iteration.map_or_else(
                || {
                    if template.name.is_empty() {
                        template.id.clone()
                    } else {
                        template.name.clone()
                    }
                },
                |i| format!("Iteration {}", i + 1),
            ),
            children: Vec::new(),
            status: Status::NotExecuted,
            detail: String::new(),
            response: None,
            http: [None, None],
            http_omitted: false,
            checks: Vec::new(),
            check_count: 0,
            expected_checks: if iteration.is_none() {
                template.checks
            } else {
                0
            },
            values: Vec::new(),
            value_count: 0,
            expected_values: if iteration.is_none() {
                template.exports
            } else {
                0
            },
            elapsed: None,
            wait: Duration::ZERO,
            interval: if iteration.is_none() {
                template.interval
            } else {
                None
            },
            waited: false,
            limit: if iteration.is_none() {
                template.limit
            } else {
                None
            },
            executed: None,
            iteration,
            started: None,
            waiting: None,
        }));
        if let Some(parent) = parent {
            Arc::make_mut(&mut self.nodes[parent]).children.push(id);
        } else {
            self.roots.push(id);
        }
        Some(id)
    }
    fn step(&self, step_id: &str) -> Option<usize> {
        let children = if let Some(frame) = self.loops.last() {
            &self.nodes.get(frame.iteration?)?.children
        } else {
            &self.roots
        };
        children
            .iter()
            .copied()
            .find(|i| self.nodes[*i].step == step_id)
    }
    fn node(&mut self, id: Option<usize>) -> Option<&mut Node> {
        id.map(|id| Arc::make_mut(&mut self.nodes[id]))
    }
    pub fn http_target(&self, event: &FlowEvent, run: u64) -> Option<HttpTarget> {
        let part = match event {
            FlowEvent::RequestPrepared { .. } => HttpPart::Request,
            FlowEvent::ResponseReceived { .. } => HttpPart::Response,
            _ => return None,
        };
        let node = self.active.last().copied().flatten()?;
        self.nodes[node].http[part as usize].as_ref()?;
        Some(HttpTarget { run, node, part })
    }
    fn retain_http(&mut self, active: Option<usize>, part: HttpPart, value: &InspectionValue) {
        let retain = self.http_bytes.saturating_add(value.retained_byte_len()) <= MAX_HTTP_BYTES;
        if let Some(n) = self.node(active) {
            if retain {
                n.http[part as usize] = Some(NamedValue {
                    name: part.label().into(),
                    value: value.clone(),
                });
                self.http_bytes += value.retained_byte_len();
            } else {
                n.http_omitted = true;
            }
        }
    }
    pub fn event(&mut self, event: &FlowEvent, observed: Instant) {
        let active = self.active.last().copied().flatten();
        match event {
            FlowEvent::StepStarted { step_id, .. } => {
                let id = self.step(step_id);
                if let Some(n) = self.node(id) {
                    n.status = Status::Running;
                    n.started = Some(observed);
                }
                let template = if let Some(frame) = self.loops.last() {
                    frame
                        .template
                        .as_ref()
                        .and_then(|t| t.children.iter().find(|t| &t.id == step_id))
                        .cloned()
                } else {
                    self.templates.iter().find(|t| &t.id == step_id).cloned()
                };
                self.active_templates.push(template);
                self.active.push(id);
            }
            FlowEvent::StepSkipped { step_id, .. } => {
                let id = self.step(step_id);
                if let Some(n) = self.node(id) {
                    n.status = Status::Skipped;
                    n.detail = "Run condition was false; no request executed.".into();
                }
            }
            FlowEvent::RequestPrepared { inspection, .. } => {
                self.retain_http(active, HttpPart::Request, inspection);
            }
            FlowEvent::ResponseReceived {
                status,
                elapsed_ms,
                inspection,
                ..
            } => {
                if let Some(n) = self.node(active) {
                    n.response = Some((*status, *elapsed_ms));
                }
                self.retain_http(active, HttpPart::Response, inspection);
            }
            FlowEvent::CheckFinished {
                success,
                inspection,
                ..
            } => {
                if let Some(n) = self.node(active) {
                    n.check_count += 1;
                    if n.checks.len() < MAX_CHECKS {
                        n.checks.push(Check {
                            success: *success,
                            detail: inspection.clone(),
                        });
                    }
                }
            }
            FlowEvent::OutputExported { name, value, .. } => {
                let retain = self.value_bytes.saturating_add(value.byte_len()) <= MAX_VALUE_BYTES;
                if let Some(n) = self.node(active) {
                    n.value_count += 1;
                    if retain && n.values.len() < MAX_VALUES {
                        n.values.push(NamedValue {
                            name: name.clone(),
                            value: value.clone(),
                        });
                        self.value_bytes += value.byte_len();
                    }
                }
            }
            FlowEvent::StepFinished { outcome, .. } => {
                if let Some(n) = self.node(active) {
                    n.finish(outcome_status(outcome), observed);
                    if let StepOutcome::Failed { message } = outcome {
                        if n.detail.is_empty() {
                            n.detail = failure_summary(message).into();
                        }
                    }
                }
                self.active.pop();
                self.active_templates.pop();
            }
            FlowEvent::LoopStarted { limit, kind, .. } => {
                if let Some(n) = self.node(active) {
                    n.limit = Some(*limit);
                    n.executed = Some(0);
                    n.detail = format!("{kind:?}");
                }
                self.loops.push(Frame {
                    template: self.active_templates.last().cloned().flatten(),
                    node: active,
                    iteration: None,
                });
            }
            FlowEvent::IterationStarted { index, .. } => {
                let parent = self.loops.last().and_then(|f| f.node);
                let template = self.loops.last().and_then(|f| f.template.clone());
                let id = if let Some((parent, template)) = parent.zip(template.clone()) {
                    self.node(Some(parent)).unwrap().executed = Some(index + 1);
                    self.node(Some(parent)).unwrap().settle_wait(observed);
                    self.node(Some(parent)).unwrap().status = Status::Running;
                    let id = self.insert(Some(parent), template.clone(), Some(*index));
                    if let Some(id) = id {
                        let n = self.node(Some(id)).unwrap();
                        n.started = Some(observed);
                        n.status = Status::Running;
                        for child in &template.children {
                            self.insert(Some(id), child.clone(), None);
                        }
                    } else {
                        self.total_rows = self.total_rows.saturating_add(template.children.len());
                    }
                    id
                } else {
                    self.total_rows = self
                        .total_rows
                        .saturating_add(1 + template.map_or(0, |t| t.children.len()));
                    None
                };
                if let Some(frame) = self.loops.last_mut() {
                    frame.iteration = id;
                }
            }
            FlowEvent::IterationFinished { outcome, .. } => {
                let id = self.loops.last().and_then(|f| f.iteration);
                if let Some(n) = self.node(id) {
                    n.finish(outcome_status(outcome), observed);
                }
                if let Some(frame) = self.loops.last_mut() {
                    frame.iteration = None;
                }
            }
            FlowEvent::LoopWaiting { interval_ms, .. } => {
                let id = self.loops.last().and_then(|f| f.node);
                if let Some(n) = self.node(id) {
                    n.interval = Some(*interval_ms);
                    n.waited = true;
                    n.waiting = Some(observed);
                    n.status = Status::Waiting;
                }
            }
            FlowEvent::LoopFinished {
                total_executed,
                reason,
                ..
            } => {
                if let Some(frame) = self.loops.pop() {
                    if let Some(n) = self.node(frame.node) {
                        n.settle_wait(observed);
                        n.executed = Some(*total_executed);
                        n.detail = loop_reason(*reason).into();
                    }
                }
            }
            FlowEvent::FlowFinished { success, outputs } => {
                // Only worker completion commits these as final returns.
                self.complete = false;
                self.output_count = if *success { outputs.len() } else { 0 };
                self.outputs = if *success {
                    outputs
                        .iter()
                        .take(MAX_VALUES)
                        .map(|(name, value)| NamedValue {
                            name: name.clone(),
                            value: value.inspection().clone(),
                        })
                        .collect()
                } else {
                    Vec::new()
                };
            }
            FlowEvent::FlowStarted { .. } => {}
        }
    }
    pub fn finish(&mut self, status: Status) {
        for node in &mut self.nodes {
            if node.status.active() {
                let n = Arc::make_mut(node);
                n.finish(
                    if status == Status::Cancelled {
                        Status::Cancelled
                    } else {
                        Status::Failed
                    },
                    Instant::now(),
                );
                if status == Status::Cancelled {
                    n.detail = "Cancelled; partial evidence retained.".into();
                }
            }
        }
        self.active.clear();
        self.active_templates.clear();
        self.loops.clear();
        self.complete = status == Status::Succeeded;
        if status != Status::Succeeded {
            self.complete = false;
            self.outputs.clear();
            self.output_count = 0;
        }
        self.outputs_copy = self.build_outputs_json();
    }
    pub fn can_copy_outputs(&self) -> bool {
        self.outputs_copy.is_some()
    }
    pub fn outputs_json(&self) -> Option<String> {
        self.outputs_copy.clone()
    }
    fn build_outputs_json(&self) -> Option<String> {
        if !self.complete || self.outputs.len() != self.output_count {
            return None;
        }
        let values = self
            .outputs
            .iter()
            .map(|v| Some((v.name.clone(), v.value.value()?.clone())))
            .collect::<Option<serde_json::Map<_, _>>>()?;
        serde_json::to_string_pretty(&values).ok()
    }
}
fn outcome_status(outcome: &StepOutcome) -> Status {
    if matches!(outcome, StepOutcome::Succeeded) {
        Status::Succeeded
    } else {
        Status::Failed
    }
}

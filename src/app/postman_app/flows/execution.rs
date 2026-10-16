//! Run lifetimes and immutable engine commands. The bounded mailbox applies backpressure;
//! only value-free projections reach rendering, and each batch is checked against its run ID.
mod model;
pub(super) use model::{Feedback, Run, Status};
#[cfg(test)]
mod tests;

use super::{FlowId, FlowsEvent, FlowsView};
use crate::http::executor::RequestExecutor;
use futures::StreamExt;
use gpui::{Context, Window};
use gpui_kit::component::WindowExt;
use postman_flow::{execute_flow, FlowEvent, FlowPlan, FlowSessionEnvironment};
use postman_http::HttpTransport;
use std::{collections::HashMap, time::Duration};
use tokio::sync::{mpsc, oneshot};

pub(super) struct Execution {
    executor: RequestExecutor,
    next_id: u64,
    pub(super) runs: HashMap<FlowId, Run>,
    tasks: HashMap<FlowId, RunningTask>,
}
impl Execution {
    pub(super) fn new(executor: RequestExecutor) -> Self {
        Self {
            executor,
            next_id: 0,
            runs: HashMap::new(),
            tasks: HashMap::new(),
        }
    }
}
struct RunningTask {
    abort: tokio::task::AbortHandle,
    cancel: Option<oneshot::Sender<()>>,
}
impl Drop for RunningTask {
    fn drop(&mut self) {
        self.abort.abort();
    }
}
enum Message {
    Event(FlowEvent),
    Tick,
    Finished(Status),
}

async fn drive<T: HttpTransport>(
    plan: FlowPlan,
    transport: T,
    environment: FlowSessionEnvironment,
    mut cancel: oneshot::Receiver<()>,
    sender: mpsc::Sender<Message>,
) {
    let status = match execute_flow(plan, transport, environment) {
        Err(_) => Status::Failed,
        Ok(stream) => {
            let mut stream = std::pin::pin!(stream);
            let mut tick = tokio::time::interval(Duration::from_millis(250));
            let mut outcome = Status::Failed;
            loop {
                let message = tokio::select! {
                    biased;
                    _ = &mut cancel => { outcome = Status::Cancelled; break; }
                    event = stream.next() => match event {
                        Some(Ok(event)) => {
                            if let FlowEvent::FlowFinished { success, .. } = &event {
                                outcome = if *success { Status::Succeeded } else { Status::Failed };
                            }
                            Message::Event(event)
                        }
                        Some(Err(_)) | None => break,
                    },
                    _ = tick.tick() => Message::Tick,
                };
                tokio::select! {
                    biased;
                    _ = &mut cancel => { outcome = Status::Cancelled; break; }
                    result = sender.send(message) => if result.is_err() { return; },
                }
            }
            outcome
        }
    }; // Drop the execution stream (and any request/wait) before acknowledging completion.
    let _ = sender.send(Message::Finished(status)).await;
}

impl FlowsView {
    pub(super) fn running(&self, flow: FlowId) -> bool {
        self.execution.tasks.contains_key(&flow)
    }
    pub(super) fn run_label(&self) -> String {
        self.library
            .active()
            .and_then(|d| {
                self.execution
                    .runs
                    .get(&d.id())
                    .map(|r| r.label(d.revision()))
            })
            .unwrap_or_else(|| "Not run".into())
    }
    pub(super) fn run_or_cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        if self.library.active().is_some_and(|d| self.running(d.id())) {
            self.stop_active_flow(cx);
        } else {
            self.run_flow(window, cx);
        }
    }
    pub(in crate::app::postman_app) fn execution_shortcut(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Text/IME and Console retain their own Enter semantics. The explicit toolbar remains
        // available while editing; this shortcut belongs to the canvas/library navigation surface.
        if self.canvas_focus.is_focused(window)
            || self.library_focus.is_focused(window)
            || self.focus.is_focused(window)
        {
            self.run_or_cancel(window, cx);
        }
    }
    pub(super) fn run_flow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) || self.file_operation_pending() {
            return;
        }
        let Some(draft) = self.library.active() else {
            return;
        };
        let (flow, revision) = (draft.id(), draft.revision());
        if self.running(flow) {
            self.console.update(cx, |v, cx| {
                v.runtime_notice(
                    flow,
                    "A run is already active. Use Cancel or stop first.",
                    cx,
                )
            });
            return;
        }
        let plan = match self.prepare_plan(flow, cx) {
            Ok(plan) => plan,
            Err(issues) => {
                self.console
                    .update(cx, |v, cx| v.report(flow, revision, issues, window, cx));
                self.console.update(cx, |v, cx| {
                    v.runtime_notice(
                        flow,
                        "Run blocked. No requests sent; fix the Check diagnostics.",
                        cx,
                    )
                });
                return;
            }
        };
        let inputs = match self
            .inputs
            .get(&flow)
            .expect("Inputs retained for an open Flow")
            .read(cx)
            .prepare(cx)
        {
            Ok(values) => values,
            Err(errors) => {
                self.console
                    .update(cx, |v, cx| v.reject_run(flow, revision, errors, window, cx));
                return;
            }
        };
        self.execution.next_id += 1;
        let id = self.execution.next_id;
        let run = Run::new(id, revision, draft.document());
        self.execution.runs.insert(flow, run.clone());
        self.console
            .update(cx, |v, cx| v.begin_run(flow, run, window, cx));
        let (transport, runtime) = self.execution.executor.flow_context();
        let (sender, mut receiver) = mpsc::channel(64);
        let (cancel, cancelled) = oneshot::channel();
        // Flow-level options use product defaults; step overrides belong to the captured plan.
        // Never inherit the unrelated active HTTP tab's options.
        let worker = runtime.spawn(drive(
            plan,
            transport,
            FlowSessionEnvironment::new(inputs),
            cancelled,
            sender,
        ));
        self.execution.tasks.insert(
            flow,
            RunningTask {
                abort: worker.abort_handle(),
                cancel: Some(cancel),
            },
        );
        cx.spawn_in(window, async move |this, cx| loop {
            let runtime = runtime.clone();
            let batch = cx
                .background_executor()
                .spawn(async move {
                    let mut messages = Vec::new();
                    if let Some(message) = runtime.block_on(receiver.recv()) {
                        messages.push(message);
                        while messages.len() < 32 {
                            match receiver.try_recv() {
                                Ok(message) => messages.push(message),
                                Err(_) => break,
                            }
                        }
                    }
                    (receiver, messages)
                })
                .await;
            receiver = batch.0;
            let done =
                batch.1.is_empty() || batch.1.iter().any(|m| matches!(m, Message::Finished(_)));
            let applied = this
                .update_in(cx, |this, window, cx| {
                    this.apply_run_batch(flow, id, batch.1, window, cx)
                })
                .unwrap_or(false);
            if done || !applied {
                break;
            }
        })
        .detach();
        cx.notify();
    }
    fn apply_run_batch(
        &mut self,
        flow: FlowId,
        id: u64,
        messages: Vec<Message>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(run) = self
            .execution
            .runs
            .get_mut(&flow)
            .filter(|r| r.id == id && r.status.active())
        else {
            return false;
        };
        let mut feedback = Vec::new();
        let mut finished = messages.is_empty();
        if finished {
            run.finish(Status::Failed);
        }
        for message in messages {
            match message {
                Message::Tick => run.tick(),
                Message::Event(event) => {
                    if let Some(event) = run.event(event) {
                        feedback.push(event);
                    }
                }
                Message::Finished(status) => {
                    run.finish(status);
                    finished = true;
                }
            }
        }
        if finished {
            feedback.push(Feedback {
                status: run.status,
                message: format!(
                    "Run {} · {:.1}s.",
                    run.status.label().to_lowercase(),
                    run.elapsed.as_secs_f32()
                ),
                step: None,
            });
            self.execution.tasks.remove(&flow);
        }
        let projection = run.clone();
        self.console
            .update(cx, |v, cx| v.runtime_update(flow, projection, feedback, cx));
        cx.emit(FlowsEvent::CookiesChanged(
            self.execution.executor.cookie_snapshot(),
        ));
        if finished {
            self.advance_file_close(window, cx);
        }
        cx.notify();
        true
    }
    pub(super) fn stop_active_flow(&mut self, cx: &mut Context<Self>) {
        if let Some(flow) = self.library.active().map(|d| d.id()) {
            self.stop_flow(flow, cx);
        }
    }
    pub(super) fn stop_flow(&mut self, flow: FlowId, cx: &mut Context<Self>) {
        let Some(task) = self.execution.tasks.get_mut(&flow) else {
            self.console.update(cx, |v, cx| {
                v.runtime_notice(flow, "No Flow run is active.", cx)
            });
            return;
        };
        let Some(cancel) = task.cancel.take() else {
            return;
        };
        if let Some(run) = self.execution.runs.get_mut(&flow) {
            run.status = Status::Stopping;
            let projection = run.clone();
            self.console.update(cx, |v, cx| {
                v.runtime_update(
                    flow,
                    projection,
                    vec![Feedback {
                        status: Status::Stopping,
                        message: "Cancellation requested. Waiting for execution to stop.".into(),
                        step: None,
                    }],
                    cx,
                )
            });
        }
        let _ = cancel.send(());
        cx.notify();
    }
    pub(super) fn forget_run(&mut self, flow: FlowId) {
        self.execution.tasks.remove(&flow);
        self.execution.runs.remove(&flow);
    }
}

/// Shared semantic status treatment for canvas badges and the basic Steps view.
pub(super) fn status_color(status: Status) -> crate::ui::theme::ColorToken {
    use crate::ui::theme::{ERROR, INFO, MUTED, OK, WARNING};
    match status {
        Status::Succeeded => OK,
        Status::Failed => ERROR,
        Status::Running | Status::Waiting => INFO,
        Status::Cancelled | Status::Stopping => WARNING,
        _ => MUTED,
    }
}

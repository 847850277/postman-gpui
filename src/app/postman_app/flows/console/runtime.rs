use super::super::{
    execution::{Feedback, Run, Status},
    validation::{DiagnosticTarget, Target},
};
use super::*;

impl FlowConsole {
    pub(in super::super) fn runtime_notice(
        &mut self,
        flow: FlowId,
        message: &str,
        cx: &mut Context<Self>,
    ) {
        self.sessions
            .entry(flow)
            .or_default()
            .push(Level::Warning, message, None);
        if self.active == Some(flow) {
            self.refresh_logs(false);
        }
        cx.notify();
    }
    pub(in super::super) fn reject_run(
        &mut self,
        flow: FlowId,
        revision: u64,
        errors: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let session = self.sessions.entry(flow).or_default();
        session.view = View::Logs;
        session.filter = None;
        session.push(
            Level::Error,
            "Run blocked. No requests sent; review Run inputs.",
            None,
        );
        for error in errors {
            session.push(Level::Error, error, None);
            session.logs.back_mut().unwrap().diagnostic = Some(DiagnosticTarget {
                flow,
                revision,
                target: Target::RuntimeInputs,
            });
        }
        if self.mode == ConsoleMode::Collapsed {
            self.set_mode(ConsoleMode::Normal, cx);
        }
        self.sync_filter(window, cx);
        self.refresh_logs(false);
        cx.notify();
    }
    pub(in super::super) fn begin_run(
        &mut self,
        flow: FlowId,
        run: Run,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let session = self.sessions.entry(flow).or_default();
        session.run = Some(run);
        session
            .step_scroll
            .scroll_to_item(0, gpui::ScrollStrategy::Top);
        session.view = View::Logs;
        session.filter = None;
        session.follow = true;
        session.push(
            Level::Info,
            "Starting a new run from the current document and runtime inputs.",
            None,
        );
        if self.mode == ConsoleMode::Collapsed {
            self.set_mode(ConsoleMode::Normal, cx);
        }
        self.sync_filter(window, cx);
        self.refresh_logs(false);
        cx.notify();
    }
    pub(in super::super) fn runtime_update(
        &mut self,
        flow: FlowId,
        run: Run,
        events: Vec<Feedback>,
        cx: &mut Context<Self>,
    ) {
        let session = self.sessions.entry(flow).or_default();
        for event in events {
            let level = match event.status {
                Status::Failed => Level::Error,
                Status::Succeeded => Level::Success,
                Status::Stopping | Status::Cancelled => Level::Warning,
                _ => Level::Info,
            };
            session.push(level, event.message, event.step);
            session.logs.back_mut().unwrap().run_revision = Some(run.revision);
        }
        session.run = Some(run);
        if self.active == Some(flow) {
            self.refresh_logs(false);
        }
        cx.notify();
    }
}

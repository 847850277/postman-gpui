use super::*;

impl FlowsView {
    pub(in crate::app::postman_app) fn file_operation_pending(&self) -> bool {
        self.files.busy || self.files.conflict.is_some() || self.files.close.is_some()
    }
    pub(in crate::app::postman_app) fn close_flow(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.files_available(window, cx) {
            return;
        }
        if let Some(draft) = self.library.active() {
            self.files.close = Some(CloseTarget::Flow(draft.id()));
            self.files.discarded.clear();
            self.files.stop_approved.clear();
            self.advance_file_close(window, cx);
        }
    }
    pub(in crate::app::postman_app) fn confirm_flow_quit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.files_available(window, cx) {
            return;
        }
        self.files.close = Some(CloseTarget::Quit);
        self.files.discarded.clear();
        self.files.stop_approved.clear();
        self.advance_file_close(window, cx);
    }
    pub(in crate::app::postman_app::flows) fn advance_file_close(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.files.busy || self.files.dialog.is_some() || self.files.conflict.is_some() {
            return;
        }
        let Some(target) = self.files.close else {
            return;
        };
        let next = self
            .library
            .drafts()
            .iter()
            .find(|d| {
                (target == CloseTarget::Quit || target == CloseTarget::Flow(d.id()))
                    && self.is_dirty(d.id(), cx)
                    && self.files.discarded.get(&d.id()) != Some(&d.revision())
            })
            .map(|d| d.id());
        if let Some(flow) = next {
            self.open_close_dialog(flow, window, cx);
        } else {
            let running: Vec<_> = self
                .library
                .drafts()
                .iter()
                .filter(|d| {
                    (target == CloseTarget::Quit || target == CloseTarget::Flow(d.id()))
                        && self.running(d.id())
                })
                .map(|d| (d.id(), self.execution.runs[&d.id()].id))
                .collect();
            if let Some((flow, _)) = running
                .iter()
                .find(|(flow, run)| self.files.stop_approved.get(flow) != Some(run))
            {
                self.open_stop_dialog(*flow, window, cx);
                return;
            }
            if !running.is_empty() {
                for (flow, _) in running {
                    self.stop_flow(flow, cx);
                }
                return; // Worker acknowledgements re-enter this coordinator after stream drop.
            }
            self.files.close = None;
            self.files.discarded.clear();
            self.files.stop_approved.clear();
            self.files.return_focus = None;
            match target {
                CloseTarget::Quit => cx.emit(super::super::FlowsEvent::QuitReady),
                CloseTarget::Flow(flow) => {
                    self.forget_run(flow);
                    self.library.close(flow);
                    self.inputs.remove(&flow);
                    self.input_subscriptions.remove(&flow);
                    self.previews.remove(&flow);
                    self.inspectors.retain(|(id, _), _| *id != flow);
                    self.step_subscriptions.retain(|(id, _), _| *id != flow);
                    self.canvas_scroll.remove(&flow);
                    self.console
                        .update(cx, |console, cx| console.forget(flow, cx));
                    self.sync_fields(window, cx);
                    self.sync_picker(window, cx);
                    self.focus.focus(window, cx);
                }
            }
            cx.notify();
        }
    }
    pub(super) fn dismiss_file_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.files.dialog.take().is_some() {
            window.close_dialog(cx);
        }
    }
    pub(super) fn cancel_file_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.files.busy {
            return;
        }
        self.dismiss_file_dialog(window, cx);
        self.file_dialog_closed(window, cx);
    }
    // Kit has already closed the modal before calling on_close (Escape / close button).
    // Closing it again would disturb the restored input/focus or an underlying dialog.
    pub(super) fn file_dialog_closed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.files.dialog = None;
        self.files.close = None;
        self.files.discarded.clear();
        self.files.stop_approved.clear();
        self.files.conflict = None;
        if let Some(focus) = self.files.return_focus.take() {
            focus.focus(window, cx);
        }
        cx.notify();
    }
    pub(super) fn discard_flow(
        &mut self,
        flow: FlowId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.files.busy {
            return;
        }
        if let Some(draft) = self.library.get(flow) {
            self.files.discarded.insert(flow, draft.revision());
        }
        self.dismiss_file_dialog(window, cx);
        cx.defer_in(window, |this, window, cx| {
            this.advance_file_close(window, cx)
        });
    }
}

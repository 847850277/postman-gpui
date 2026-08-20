//! Request editing composition and projection ownership.
//!
//! `RequestWorkspace` owns request tabs and the response surface. `RequestComposer` owns the
//! method/URL/send controls and one long-lived entity for every request pane. Each pane entity owns
//! only its GPUI controls, subscriptions, focus/selection state, and scrolling; all request values,
//! ordering, and enabled flags live in the shared `Entity<WorkspaceViewModel>`.
//!
//! Projection is command-driven: changing the active request projects every control once, changing
//! the selected pane projects that pane, and ordinary input events write directly from the owning
//! pane to the ViewModel. Panes deliberately do not observe the ViewModel to re-project themselves,
//! so a keystroke cannot rebuild every inactive editor. Send builds its immutable command from the
//! ViewModel before emitting it to the runner.

use crate::{
    app::{PendingRequest, SendId, WorkspaceViewModel},
    models::HistoryEntry,
    ui::theme::BG,
};
use gpui::{
    div, rgb, AppContext, Context, Entity, EventEmitter, InteractiveElement, IntoElement,
    ParentElement, Render, Styled, Subscription, Window,
};

mod chrome;
mod composer;
mod composer_chrome;
mod layout;
mod panes;
mod response_panel;

use composer::{RequestComposer, RequestComposerEvent};
use response_panel::{setup_response_viewer_key_bindings, ResponseViewer};

#[derive(Clone, Debug)]
pub(super) enum RequestWorkspaceEvent {
    Execute(PendingRequest),
    Abort(SendId),
}

/// Workspace composition for request tabs, the active composer, and its response.
///
/// Request panes share one business ViewModel but retain their own GPUI control lifetimes inside
/// the composer. HTTP execution remains owned by RequestRunner above this entity.
pub(super) struct RequestWorkspace {
    view_model: Entity<WorkspaceViewModel>,
    composer: Entity<RequestComposer>,
    response_viewer: Entity<ResponseViewer>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<RequestWorkspaceEvent> for RequestWorkspace {}

impl RequestWorkspace {
    pub(super) fn new(view_model: Entity<WorkspaceViewModel>, cx: &mut Context<Self>) -> Self {
        cx.bind_keys(setup_response_viewer_key_bindings());

        let composer = cx.new(|cx| RequestComposer::new(view_model.clone(), cx));
        let response_viewer = cx.new(|cx| ResponseViewer::new(view_model.clone(), cx));
        let subscriptions = vec![
            cx.subscribe(&composer, Self::on_composer_event),
            cx.observe(&view_model, |_, _, cx| cx.notify()),
        ];

        Self {
            view_model,
            composer,
            response_viewer,
            _subscriptions: subscriptions,
        }
    }

    fn on_composer_event(
        &mut self,
        _composer: Entity<RequestComposer>,
        event: &RequestComposerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            RequestComposerEvent::Execute(pending) => {
                cx.emit(RequestWorkspaceEvent::Execute(pending.clone()))
            }
            RequestComposerEvent::Abort(send_id) => cx.emit(RequestWorkspaceEvent::Abort(*send_id)),
        }
    }

    fn update_view_model<R>(
        &self,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut WorkspaceViewModel) -> R,
    ) -> R {
        let result = self.view_model.update(cx, |view_model, cx| {
            let result = update(view_model);
            cx.notify();
            result
        });
        cx.notify();
        result
    }

    fn cancel_send(&mut self, send_id: SendId, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.cancel_send(send_id));
        cx.emit(RequestWorkspaceEvent::Abort(send_id));
    }

    fn project_active_request(&self, cx: &mut Context<Self>) {
        self.composer
            .update(cx, RequestComposer::project_active_request);
    }

    pub(super) fn new_request(&mut self, cx: &mut Context<Self>) {
        self.update_view_model(cx, WorkspaceViewModel::new_request);
        self.project_active_request(cx);
    }

    pub(super) fn select_request_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.update_view_model(cx, |view_model| view_model.select_tab(index)) {
            self.project_active_request(cx);
        }
    }

    pub(super) fn close_request_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(send_id) = self.view_model.read(cx).send_id_for_tab(index) {
            self.cancel_send(send_id, cx);
        }
        if self.update_view_model(cx, |view_model| view_model.close_tab(index)) {
            self.project_active_request(cx);
        }
    }

    pub(super) fn load_history_entry(&mut self, entry: &HistoryEntry, cx: &mut Context<Self>) {
        if let Some(send_id) = self.view_model.read(cx).active_send_id() {
            self.cancel_send(send_id, cx);
        }
        self.update_view_model(cx, |view_model| view_model.load_history_entry(entry));
        self.project_active_request(cx);
    }
}

impl Render for RequestWorkspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .child(self.render_request_tabs_bar(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_3()
                    .child(self.composer.clone())
                    .child(
                        div()
                            .id("response-container")
                            .debug_selector(|| "response-container".into())
                            .flex_1()
                            .min_h_0()
                            .child(self.response_viewer.clone()),
                    ),
            )
    }
}

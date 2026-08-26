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
    app::{PendingRequest, RequestTabId, SendId, WorkspaceViewModel},
    models::HistoryEntry,
    ui::theme::BG,
};
use gpui::{
    div, rgb, AppContext, Context, Entity, EventEmitter, FocusHandle, InteractiveElement,
    IntoElement, ParentElement, Render, Styled, Subscription, Window,
};
use std::collections::HashMap;

mod chrome;
mod composer;
mod composer_chrome;
mod layout;
mod panes;
mod response_panel;

use chrome::setup_request_tab_key_bindings;
use composer::{RequestComposer, RequestComposerEvent};
pub(super) use panes::{CookiePane, CookiePaneEvent};
use response_panel::{setup_response_viewer_key_bindings, ResponseViewer, ResponseViewerEvent};

#[derive(Clone, Debug)]
pub(super) enum RequestWorkspaceEvent {
    Execute(PendingRequest),
    Abort(SendId),
    OpenCookieJar,
}

/// Workspace composition for request tabs, the active composer, and its response.
///
/// Request panes share one business ViewModel but retain their own GPUI control lifetimes inside
/// the composer. HTTP execution remains owned by RequestRunner above this entity.
pub(super) struct RequestWorkspace {
    view_model: Entity<WorkspaceViewModel>,
    composer: Entity<RequestComposer>,
    response_viewer: Entity<ResponseViewer>,
    tab_focus_handles: HashMap<RequestTabId, FocusHandle>,
    tab_close_focus_handles: HashMap<RequestTabId, FocusHandle>,
    new_tab_focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<RequestWorkspaceEvent> for RequestWorkspace {}

impl RequestWorkspace {
    pub(super) fn new(view_model: Entity<WorkspaceViewModel>, cx: &mut Context<Self>) -> Self {
        cx.bind_keys(setup_response_viewer_key_bindings());
        cx.bind_keys(setup_request_tab_key_bindings());

        let composer = cx.new(|cx| RequestComposer::new(view_model.clone(), cx));
        let response_viewer = cx.new(|cx| ResponseViewer::new(view_model.clone(), cx));
        let subscriptions = vec![
            cx.subscribe(&composer, Self::on_composer_event),
            cx.subscribe(&response_viewer, Self::on_response_viewer_event),
            cx.observe(&view_model, |_, _, cx| cx.notify()),
        ];

        Self {
            view_model,
            composer,
            response_viewer,
            tab_focus_handles: HashMap::new(),
            tab_close_focus_handles: HashMap::new(),
            new_tab_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
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

    fn on_response_viewer_event(
        &mut self,
        _viewer: Entity<ResponseViewer>,
        event: &ResponseViewerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ResponseViewerEvent::OpenCookieJar => cx.emit(RequestWorkspaceEvent::OpenCookieJar),
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

    pub(super) fn activate_request_tab(&mut self, tab_id: RequestTabId, cx: &mut Context<Self>) {
        if self.update_view_model(cx, |view_model| view_model.select_tab_by_id(tab_id)) {
            self.project_active_request(cx);
        }
    }

    pub(super) fn close_request_tab(&mut self, tab_id: RequestTabId, cx: &mut Context<Self>) {
        if let Some(send_id) = self.view_model.read(cx).send_id_for_tab_id(tab_id) {
            self.cancel_send(send_id, cx);
        }
        if self.update_view_model(cx, |view_model| view_model.close_tab_by_id(tab_id)) {
            self.project_active_request(cx);
        }
    }

    pub(super) fn focus_active_request_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active_tab_id = {
            let view_model = self.view_model.read(cx);
            view_model.tabs()[view_model.active_tab_index()].tab_id()
        };
        self.tab_focus_handles
            .entry(active_tab_id)
            .or_insert_with(|| cx.focus_handle().tab_index(0).tab_stop(true))
            .focus(window, cx);
    }

    pub(super) fn send_or_cancel(&mut self, cx: &mut Context<Self>) {
        self.composer
            .update(cx, |composer, cx| composer.send_or_cancel(cx));
    }

    pub(super) fn focus_url(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer
            .update(cx, |composer, cx| composer.focus_url(window, cx));
    }

    pub(super) fn close_active_request(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active_tab_id = {
            let view_model = self.view_model.read(cx);
            view_model.tabs()[view_model.active_tab_index()].tab_id()
        };
        self.close_request_tab(active_tab_id, cx);
        self.focus_active_request_tab(window, cx);
    }

    pub(super) fn activate_relative_request(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab_id = {
            let view_model = self.view_model.read(cx);
            let count = view_model.tabs().len();
            if count == 0 {
                return;
            }
            let next = (view_model.active_tab_index() as isize + delta).rem_euclid(count as isize)
                as usize;
            view_model.tabs()[next].tab_id()
        };
        self.activate_request_tab(tab_id, cx);
        self.focus_active_request_tab(window, cx);
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .child(self.render_request_tabs_bar(window, cx))
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

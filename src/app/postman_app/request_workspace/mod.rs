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
    ui::theme::{BG, LINE},
};
use gpui::{
    deferred, div, px, rgb, AppContext, Context, DragMoveEvent, Entity, EventEmitter, FocusHandle,
    InteractiveElement, IntoElement, MouseButton, MouseDownEvent, MouseUpEvent, ParentElement,
    Pixels, Render, StatefulInteractiveElement, Styled, Subscription, Window,
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
use layout::{
    resizable_request_panel_height_bounds, RequestPanelLayout, RESPONSE_RESIZE_TRACK_HEIGHT,
    WORKSPACE_CONTENT_PADDING,
};
pub(super) use panes::{CookiePane, CookiePaneEvent};
use response_panel::{setup_response_viewer_key_bindings, ResponseViewer, ResponseViewerEvent};

struct ResponsePanelResize;

#[derive(Clone, Copy)]
struct ResponseResizeOrigin {
    pointer_y: Pixels,
    request_panel_height: f32,
}

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
    panel_layout: Entity<RequestPanelLayout>,
    composer: Entity<RequestComposer>,
    response_viewer: Entity<ResponseViewer>,
    response_resize_origin: Option<ResponseResizeOrigin>,
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

        let panel_layout = cx.new(|_| RequestPanelLayout::default());
        let composer =
            cx.new(|cx| RequestComposer::new(view_model.clone(), panel_layout.clone(), cx));
        let response_viewer = cx.new(|cx| ResponseViewer::new(view_model.clone(), cx));
        let subscriptions = vec![
            cx.subscribe(&composer, Self::on_composer_event),
            cx.subscribe(&response_viewer, Self::on_response_viewer_event),
            cx.observe(&view_model, |_, _, cx| cx.notify()),
            cx.observe(&panel_layout, |_, _, cx| cx.notify()),
        ];

        Self {
            view_model,
            panel_layout,
            composer,
            response_viewer,
            response_resize_origin: None,
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
        let Some(active_tab_id) = self.view_model.read(cx).active_tab_id() else {
            return;
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
        let Some(active_tab_id) = self.view_model.read(cx).active_tab_id() else {
            return;
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
            let Some(active_index) = view_model.active_tab_index() else {
                return;
            };
            let next = (active_index as isize + delta).rem_euclid(count as isize) as usize;
            view_model.tabs()[next].tab_id()
        };
        self.activate_request_tab(tab_id, cx);
        self.focus_active_request_tab(window, cx);
    }

    pub(super) fn load_history_entry(&mut self, entry: &HistoryEntry, cx: &mut Context<Self>) {
        if let Some(send_id) = self.view_model.read(cx).active_send_id() {
            self.cancel_send(send_id, cx);
        }
        if self.update_view_model(cx, |view_model| view_model.load_history_entry(entry)) {
            self.project_active_request(cx);
        }
    }

    fn start_response_resize(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(request_panel_height) = self.composer.read(cx).request_panel_height(window, cx)
        else {
            return;
        };
        self.response_resize_origin = Some(ResponseResizeOrigin {
            pointer_y: event.position.y,
            request_panel_height,
        });
        cx.stop_propagation();
    }

    fn resize_response_panel(
        &mut self,
        event: &DragMoveEvent<ResponsePanelResize>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(origin) = self.response_resize_origin else {
            return;
        };
        let (minimum, maximum) =
            resizable_request_panel_height_bounds(event.bounds.size.height.as_f32());
        let pointer_delta = (event.event.position.y - origin.pointer_y).as_f32();
        let height = (origin.request_panel_height + pointer_delta).clamp(minimum, maximum);
        self.panel_layout.update(cx, |layout, cx| {
            if layout.set_manual_height(height) {
                cx.notify();
            }
        });
    }

    fn finish_response_resize(
        &mut self,
        _resize: &ResponsePanelResize,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.response_resize_origin = None;
    }

    fn reset_response_resize(
        &mut self,
        event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.response_resize_origin = None;
        if event.click_count >= 2 {
            self.panel_layout.update(cx, |layout, cx| {
                if layout.reset() {
                    cx.notify();
                }
            });
        }
        cx.stop_propagation();
    }

    fn render_response_resize_track(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("response-resize-track")
            .relative()
            .h(px(RESPONSE_RESIZE_TRACK_HEIGHT))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .child(div().w(px(48.0)).h(px(3.0)).rounded_full().bg(rgb(LINE)))
            .child(deferred(
                div()
                    .id("response-resize-handle")
                    .debug_selector(|| "response-resize-handle".into())
                    .absolute()
                    .inset_0()
                    .cursor_row_resize()
                    .aria_label("Resize Response panel")
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::start_response_resize))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::reset_response_resize))
                    .on_drag(ResponsePanelResize, |_, _, _, cx| {
                        cx.stop_propagation();
                        cx.new(|_| gpui::Empty)
                    }),
            ))
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
                    .p(px(WORKSPACE_CONTENT_PADDING))
                    .on_drag_move::<ResponsePanelResize>(cx.listener(Self::resize_response_panel))
                    .on_drop::<ResponsePanelResize>(cx.listener(Self::finish_response_resize))
                    .child(self.composer.clone())
                    .child(self.render_response_resize_track(cx))
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

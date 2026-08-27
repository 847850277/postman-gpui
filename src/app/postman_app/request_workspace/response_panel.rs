use gpui::{
    actions, div, point, prelude::FluentBuilder, px, rgb, App, Bounds, ClipboardItem, Context,
    CursorStyle, Element, ElementId, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    GlobalElementId, InteractiveElement, IntoElement, KeyBinding, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, ParentElement, Pixels, Point, Render,
    Role, SharedString, StatefulInteractiveElement, Style, Styled, Subscription, TextAlign,
    TextRun, Window,
};
use std::{collections::BTreeMap, time::Duration};

mod headers;

use headers::render_response_headers;

use crate::{
    app::{ActivateControl, CookieJarEntry, ResponseState, WorkspaceViewModel},
    models::{HistoricalResponseBody, RedirectHop},
    ui::components::common::edit_context_menu::{
        edit_context_menu, EditContextAction, READ_ONLY_ACTIONS,
    },
    ui::text_editor::{ReadOnlyTextSelection, TextOffset},
    ui::text_layout::{line_ranges, MultilineTextLayout},
    ui::theme::{
        ACCENT, ACCENT_SOFT, CODE_BG, CODE_TEXT, ERROR, FONT_HEADING, FONT_MONO, FONT_UI, INFO,
        INFO_SOFT, LINE, MUTED, OK, OK_SOFT, PANEL, PANEL_ALT, SUBTEXT, TEXT,
    },
    utils::formatter::format_response_body,
};

const COPIED_FEEDBACK_DURATION: Duration = Duration::from_secs(2);

actions!(
    response_viewer,
    [
        Copy,
        SelectAll,
        CopyResponseBody,
        ActivateResponsePaneTab,
        FocusNextResponsePaneTab,
        FocusPreviousResponsePaneTab,
        ActivateNextResponsePane,
        ActivatePreviousResponsePane,
        DismissResponseContextMenu
    ]
);

pub fn setup_response_viewer_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-c", Copy, Some("ResponseContent")),
        KeyBinding::new("ctrl-c", Copy, Some("ResponseContent")),
        KeyBinding::new("cmd-a", SelectAll, Some("ResponseContent")),
        KeyBinding::new("ctrl-a", SelectAll, Some("ResponseContent")),
        KeyBinding::new(
            "escape",
            DismissResponseContextMenu,
            Some("ResponseContent"),
        ),
        KeyBinding::new("enter", CopyResponseBody, Some("ResponseCopyButton")),
        KeyBinding::new("space", CopyResponseBody, Some("ResponseCopyButton")),
        KeyBinding::new("enter", ActivateResponsePaneTab, Some("ResponsePaneTab")),
        KeyBinding::new("space", ActivateResponsePaneTab, Some("ResponsePaneTab")),
        KeyBinding::new("tab", FocusNextResponsePaneTab, Some("ResponsePaneTab")),
        KeyBinding::new(
            "shift-tab",
            FocusPreviousResponsePaneTab,
            Some("ResponsePaneTab"),
        ),
        KeyBinding::new("right", ActivateNextResponsePane, Some("ResponsePaneTab")),
        KeyBinding::new("down", ActivateNextResponsePane, Some("ResponsePaneTab")),
        KeyBinding::new(
            "left",
            ActivatePreviousResponsePane,
            Some("ResponsePaneTab"),
        ),
        KeyBinding::new("up", ActivatePreviousResponsePane, Some("ResponsePaneTab")),
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResponsePane {
    Body,
    Headers,
    Cookies,
}

const RESPONSE_PANES: [ResponsePane; 3] = [
    ResponsePane::Body,
    ResponsePane::Headers,
    ResponsePane::Cookies,
];

fn response_pane_index(pane: ResponsePane) -> usize {
    RESPONSE_PANES
        .iter()
        .position(|candidate| *candidate == pane)
        .expect("all response panes are represented in keyboard order")
}

fn response_text_projection(state: &ResponseState, pane: ResponsePane) -> Option<String> {
    match (state, pane) {
        (ResponseState::Success { body, .. }, ResponsePane::Body) => {
            Some(format_response_body(body))
        }
        (ResponseState::Historical { response, .. }, ResponsePane::Body) => Some(
            response
                .body
                .preview()
                .map(format_response_body)
                .unwrap_or_else(|| match &response.body {
                    HistoricalResponseBody::Empty => "Empty response body".to_string(),
                    HistoricalResponseBody::Unsupported => "Body not stored".to_string(),
                    HistoricalResponseBody::Text(_) | HistoricalResponseBody::TruncatedText(_) => {
                        unreachable!()
                    }
                }),
        ),
        (ResponseState::HistoricalUnavailable { .. }, _) => {
            Some("This older History entry did not store a response.".to_string())
        }
        (ResponseState::Error { message }, _) => Some(message.clone()),
        (ResponseState::Cancelled, _) => Some("Request cancelled by user".to_string()),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ResponseCookieEvidence {
    name: String,
    origin: String,
    captured_by_cookie_jar: bool,
    stored_now: bool,
}

#[derive(Clone, Debug)]
pub(super) enum ResponseViewerEvent {
    OpenCookieJar,
}

/// Response surface owned by the request workspace.
pub struct ResponseViewer {
    view_model: Entity<WorkspaceViewModel>,
    pane: ResponsePane,
    focus_handle: FocusHandle,
    body_tab_focus_handle: FocusHandle,
    headers_tab_focus_handle: FocusHandle,
    cookies_tab_focus_handle: FocusHandle,
    copy_focus_handle: FocusHandle,
    open_cookie_focus_handle: FocusHandle,
    copied_feedback: bool,
    copy_generation: u64,
    selection: ReadOnlyTextSelection,
    text_layout: Option<MultilineTextLayout>,
    context_menu_position: Option<Point<Pixels>>,
    _view_model_subscription: Subscription,
}

impl EventEmitter<ResponseViewerEvent> for ResponseViewer {}

impl Focusable for ResponseViewer {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ResponseViewer {
    pub fn new(view_model: Entity<WorkspaceViewModel>, cx: &mut Context<Self>) -> Self {
        let view_model_subscription = cx.observe(&view_model, |this, _, cx| {
            this.copied_feedback = false;
            this.copy_generation = this.copy_generation.wrapping_add(1);
            cx.notify();
        });
        Self {
            view_model,
            pane: ResponsePane::Body,
            focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            body_tab_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            headers_tab_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            cookies_tab_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            copy_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            open_cookie_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            copied_feedback: false,
            copy_generation: 0,
            selection: ReadOnlyTextSelection::new(),
            text_layout: None,
            context_menu_position: None,
            _view_model_subscription: view_model_subscription,
        }
    }

    fn raw_response_body(&self, cx: &App) -> Option<String> {
        match self.view_model.read(cx).active_request()?.response() {
            ResponseState::Success { body, .. } if !body.is_empty() => Some(body.clone()),
            ResponseState::Historical { response, .. } => response
                .body
                .preview()
                .filter(|preview| !preview.is_empty())
                .map(str::to_string),
            _ => None,
        }
    }

    fn copy_raw_response_body(&mut self, cx: &mut Context<Self>) {
        let Some(body) = self.raw_response_body(cx) else {
            return;
        };

        cx.write_to_clipboard(ClipboardItem::new_string(body));
        self.copy_generation = self.copy_generation.wrapping_add(1);
        let copy_generation = self.copy_generation;
        self.copied_feedback = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(COPIED_FEEDBACK_DURATION)
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.copy_generation == copy_generation {
                    this.copied_feedback = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn copy_response_body(
        &mut self,
        _: &CopyResponseBody,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.copy_raw_response_body(cx);
    }

    fn click_copy_response_body(
        &mut self,
        _: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        self.copy_focus_handle.focus(window, cx);
        self.copy_raw_response_body(cx);
    }

    fn pane_tab(
        &self,
        pane: ResponsePane,
        label: impl Into<SharedString>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.pane == pane;
        let label = label.into();
        let selector = match pane {
            ResponsePane::Body => "response-pane-body",
            ResponsePane::Headers => "response-pane-headers",
            ResponsePane::Cookies => "response-pane-cookies",
        };
        let state_selector = match (pane, active) {
            (ResponsePane::Body, true) => "response-pane-body-active",
            (ResponsePane::Body, false) => "response-pane-body-inactive",
            (ResponsePane::Headers, true) => "response-pane-headers-active",
            (ResponsePane::Headers, false) => "response-pane-headers-inactive",
            (ResponsePane::Cookies, true) => "response-pane-cookies-active",
            (ResponsePane::Cookies, false) => "response-pane-cookies-inactive",
        };
        let focus_handle = self.pane_focus_handle(pane).clone();
        let click_focus_handle = focus_handle.clone();
        let focused = focus_handle.is_focused(window);
        div()
            .id(selector)
            .debug_selector(move || selector.into())
            .track_focus(&focus_handle)
            .key_context("ResponsePaneTab")
            .role(Role::Tab)
            .aria_label(format!("{label} response pane"))
            .aria_selected(active)
            .h_full()
            .flex()
            .items_center()
            .px_2()
            .cursor_pointer()
            .when(active, |d| {
                d.border_b_2()
                    .border_color(rgb(ACCENT))
                    .text_color(rgb(TEXT))
                    .font_weight(FontWeight::SEMIBOLD)
            })
            .when(!active, |d| {
                d.text_color(rgb(MUTED))
                    .hover(|s| s.text_color(rgb(SUBTEXT)))
            })
            .when(focused, |d| {
                d.bg(rgb(ACCENT_SOFT)).border_1().border_color(rgb(ACCENT))
            })
            .text_size(px(12.0))
            .font_family(FONT_UI)
            .on_action(cx.listener(Self::activate_response_pane_tab))
            .on_action(cx.listener(Self::focus_next_response_pane_tab))
            .on_action(cx.listener(Self::focus_previous_response_pane_tab))
            .on_action(cx.listener(Self::activate_next_response_pane))
            .on_action(cx.listener(Self::activate_previous_response_pane))
            .child(
                div()
                    .debug_selector(move || state_selector.into())
                    .child(label),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    click_focus_handle.focus(window, cx);
                    this.select_pane(pane, cx);
                }),
            )
    }

    fn pane_focus_handle(&self, pane: ResponsePane) -> &FocusHandle {
        match pane {
            ResponsePane::Body => &self.body_tab_focus_handle,
            ResponsePane::Headers => &self.headers_tab_focus_handle,
            ResponsePane::Cookies => &self.cookies_tab_focus_handle,
        }
    }

    fn select_pane(&mut self, pane: ResponsePane, cx: &mut Context<Self>) {
        self.pane = pane;
        self.selection.reset_selection();
        self.text_layout = None;
        self.context_menu_position = None;
        cx.notify();
    }

    fn activate_response_pane_tab(
        &mut self,
        _: &ActivateResponsePaneTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focused_pane = [
            ResponsePane::Body,
            ResponsePane::Headers,
            ResponsePane::Cookies,
        ]
        .into_iter()
        .find(|pane| self.pane_focus_handle(*pane).is_focused(window));
        if let Some(pane) = focused_pane {
            self.select_pane(pane, cx);
        }
    }

    fn focus_next_response_pane_tab(
        &mut self,
        _: &FocusNextResponsePaneTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus_next(cx);
    }

    fn focus_previous_response_pane_tab(
        &mut self,
        _: &FocusPreviousResponsePaneTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus_prev(cx);
    }

    fn activate_next_response_pane(
        &mut self,
        _: &ActivateNextResponsePane,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activate_relative_response_pane(1, window, cx);
    }

    fn activate_previous_response_pane(
        &mut self,
        _: &ActivatePreviousResponsePane,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activate_relative_response_pane(-1, window, cx);
    }

    fn activate_relative_response_pane(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pane_count = if matches!(
            self.view_model
                .read(cx)
                .active_request()
                .map(|request| request.response()),
            Some(ResponseState::Success { .. })
        ) {
            RESPONSE_PANES.len()
        } else {
            RESPONSE_PANES.len() - 1
        };
        let current = RESPONSE_PANES[..pane_count]
            .iter()
            .position(|pane| self.pane_focus_handle(*pane).is_focused(window))
            .unwrap_or_else(|| response_pane_index(self.pane));
        let next = (current as isize + delta).rem_euclid(pane_count as isize) as usize;
        let pane = RESPONSE_PANES[next];
        self.pane_focus_handle(pane).focus(window, cx);
        self.select_pane(pane, cx);
    }

    fn open_cookie_jar(
        &mut self,
        _event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_cookie_focus_handle.focus(window, cx);
        cx.emit(ResponseViewerEvent::OpenCookieJar);
    }

    fn open_cookie_jar_with_keyboard(
        &mut self,
        _: &ActivateControl,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.emit(ResponseViewerEvent::OpenCookieJar);
    }

    fn copy(&mut self, _: &Copy, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(selected_text) = self.selection.selected_text_for_copy() {
            cx.write_to_clipboard(ClipboardItem::new_string(selected_text.to_string()));
        }
    }

    fn select_all(&mut self, _: &SelectAll, _window: &mut Window, cx: &mut Context<Self>) {
        if self.selection.select_all() {
            cx.notify();
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let menu_was_open = self.context_menu_position.take().is_some();
        self.focus_handle.focus(window, cx);
        let offset = self.offset_for_mouse_position(event.position);
        let changed = self
            .selection
            .pointer_down(offset, event.modifiers.shift, event.click_count)
            .unwrap_or(false);
        if changed || menu_was_open {
            cx.notify();
        }
    }

    fn on_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.selection.pointer_up();
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selection.is_dragging() {
            let offset = self.offset_for_mouse_position(event.position);
            if self.selection.pointer_move(offset).unwrap_or(false) {
                cx.notify();
            }
        }
    }

    fn open_context_menu(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        self.selection.pointer_up();
        self.context_menu_position = Some(event.position);
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn dismiss_context_menu(
        &mut self,
        _: &DismissResponseContextMenu,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let changed = if self.context_menu_position.take().is_some() {
            true
        } else {
            self.selection.clear_selection()
        };
        if changed {
            cx.notify();
        }
    }

    fn handle_context_menu_action(
        &mut self,
        action: EditContextAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            EditContextAction::Copy => self.copy(&Copy, window, cx),
            EditContextAction::SelectAll => self.select_all(&SelectAll, window, cx),
            EditContextAction::Undo
            | EditContextAction::Redo
            | EditContextAction::Cut
            | EditContextAction::Paste
            | EditContextAction::Dismiss => {}
        }
        self.context_menu_position = None;
        cx.notify();
    }

    fn offset_for_mouse_position(&self, position: Point<Pixels>) -> TextOffset {
        let fallback = self.selection.selection().cursor().utf8();
        let utf8 = self
            .text_layout
            .as_ref()
            .map(|layout| layout.hit_test_utf8(self.selection.text(), position, fallback))
            .unwrap_or(fallback);
        self.selection
            .offset_from_utf8(utf8)
            .expect("shared response layout must return a UTF-8 boundary")
    }

    fn render_selectable_content(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("response-content")
            .debug_selector(|| "response-content".into())
            .cursor(CursorStyle::IBeam)
            .track_focus(&self.focus_handle(cx))
            .key_context("ResponseContent")
            .border_1()
            .border_color(if self.focus_handle.is_focused(window) {
                rgb(INFO)
            } else {
                rgb(CODE_BG)
            })
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::open_context_menu))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::dismiss_context_menu))
            .cursor_text()
            .w_full()
            .h_full()
            .min_h_0()
            .p_3()
            .bg(rgb(CODE_BG))
            .text_color(rgb(CODE_TEXT))
            .font_family(FONT_MONO)
            .text_size(px(13.0))
            .overflow_scroll()
            .child(ResponseTextElement {
                viewer: cx.entity().clone(),
            })
    }

    fn render_cookie_content(
        &self,
        cookies: Vec<ResponseCookieEvidence>,
        jar_count: usize,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let cookie_count = cookies.len();
        let has_cookies = cookie_count > 0;

        div()
            .debug_selector(|| "response-cookies-panel".into())
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .bg(rgb(CODE_BG))
            .child(
                div()
                    .h(px(42.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .font_family(FONT_UI)
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(11.0))
                                    .text_color(rgb(TEXT))
                                    .child(format!(
                                        "CURRENT RESPONSE / REDIRECT CHAIN · COOKIES ({cookie_count})"
                                    )),
                            )
                            .child(
                                div()
                                    .font_family(FONT_UI)
                                    .text_size(px(9.0))
                                    .text_color(rgb(SUBTEXT))
                                    .child(format!(
                                        "Response-scoped observation · Cookie Jar now has {jar_count} stored"
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .id("response-open-cookie-jar")
                            .debug_selector(|| "response-open-cookie-jar".into())
                            .track_focus(&self.open_cookie_focus_handle)
                            .key_context("KeyboardButton OverlayTrigger")
                            .role(Role::Button)
                            .aria_label("Open Cookie Jar")
                            .h(px(30.0))
                            .px_3()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap_2()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(INFO))
                            .bg(rgb(PANEL))
                            .font_family(FONT_UI)
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(10.0))
                            .text_color(rgb(INFO))
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(INFO_SOFT)))
                            .when(self.open_cookie_focus_handle.is_focused(window), |button| {
                                button.border_2().border_color(rgb(ACCENT))
                            })
                            .child("↗")
                            .child("Open Cookie Jar")
                            .on_action(cx.listener(Self::open_cookie_jar_with_keyboard))
                            .on_mouse_up(MouseButton::Left, cx.listener(Self::open_cookie_jar)),
                    ),
            )
            .when(!has_cookies, |panel| {
                panel.child(
                    div()
                        .debug_selector(|| "response-cookies-empty".into())
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap_2()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(LINE))
                        .bg(rgb(PANEL_ALT))
                        .child(
                            div()
                                .font_family(FONT_HEADING)
                                .font_weight(FontWeight::BOLD)
                                .text_size(px(16.0))
                                .text_color(rgb(TEXT))
                                .child("No Set-Cookie received"),
                        )
                        .child(
                            div()
                                .font_family(FONT_UI)
                                .text_size(px(11.0))
                                .text_color(rgb(SUBTEXT))
                                .child(format!(
                                    "This response stored no new cookies. Cookie Jar remains {jar_count}."
                                )),
                        ),
                )
            })
            .when(has_cookies, |panel| {
                panel.child(
                    div()
                        .debug_selector(|| "response-cookie-list".into())
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .children(cookies.into_iter().enumerate().map(|(index, cookie)| {
                            let source = if cookie.captured_by_cookie_jar {
                                if cookie.stored_now {
                                    "CAPTURED · STORED"
                                } else {
                                    "CAPTURED · CLEARED"
                                }
                            } else {
                                "SET-COOKIE HEADER"
                            };
                            div()
                                .debug_selector(move || format!("response-cookie-row-{index}"))
                                .h(px(54.0))
                                .flex_none()
                                .flex()
                                .items_center()
                                .gap_3()
                                .px_3()
                                .rounded_lg()
                                .border_1()
                                .border_color(rgb(LINE))
                                .bg(rgb(INFO_SOFT))
                                .child(
                                    div()
                                        .debug_selector(move || {
                                            format!("response-cookie-name-{index}")
                                        })
                                        .w(px(150.0))
                                        .flex_none()
                                        .font_family(FONT_MONO)
                                        .font_weight(FontWeight::BOLD)
                                        .text_size(px(11.0))
                                        .text_color(rgb(TEXT))
                                        .child(cookie.name),
                                )
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex_1()
                                        .overflow_hidden()
                                        .font_family(FONT_MONO)
                                        .text_size(px(10.0))
                                        .text_color(rgb(SUBTEXT))
                                        .child(cookie.origin),
                                )
                                .child(
                                    div()
                                        .h(px(24.0))
                                        .px_2()
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .rounded_lg()
                                        .bg(rgb(PANEL))
                                        .font_family(FONT_UI)
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_size(px(9.0))
                                        .text_color(rgb(MUTED))
                                        .child("VALUE PROTECTED"),
                                )
                                .child(
                                    div()
                                        .debug_selector(move || {
                                            format!("response-cookie-storage-{index}")
                                        })
                                        .h(px(24.0))
                                        .px_2()
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .rounded_lg()
                                        .bg(rgb(if cookie.stored_now { OK_SOFT } else { PANEL_ALT }))
                                        .font_family(FONT_UI)
                                        .font_weight(FontWeight::BOLD)
                                        .text_size(px(9.0))
                                        .text_color(rgb(if cookie.stored_now { OK } else { MUTED }))
                                        .child(source),
                                )
                        })),
                )
            })
    }
}

fn response_cookie_evidence(view_model: &WorkspaceViewModel) -> Vec<ResponseCookieEvidence> {
    let mut cookies = BTreeMap::<(String, String), bool>::new();
    let Some(request) = view_model.active_request() else {
        return Vec::new();
    };

    for cookie in request.response_stored_cookies() {
        cookies.insert((cookie.origin.clone(), cookie.name.clone()), true);
    }

    if let ResponseState::Success { headers, .. } = request.response() {
        let origin = response_origin(&request.effective_url());
        for (_, value) in headers
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case("set-cookie"))
        {
            if let Some(name) = set_cookie_name(value) {
                cookies.entry((origin.clone(), name)).or_insert(false);
            }
        }
    }

    cookies
        .into_iter()
        .map(|((origin, name), captured_by_cookie_jar)| {
            let stored_now = view_model
                .cookies()
                .iter()
                .any(|cookie: &CookieJarEntry| cookie.origin == origin && cookie.name == name);
            ResponseCookieEvidence {
                name,
                origin,
                captured_by_cookie_jar,
                stored_now,
            }
        })
        .collect()
}

fn set_cookie_name(value: &str) -> Option<String> {
    value
        .split(';')
        .next()?
        .split_once('=')
        .map(|(name, _)| name.trim())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

fn response_origin(url: &str) -> String {
    let Some((scheme, remainder)) = url.split_once("://") else {
        return url.to_string();
    };
    let authority = remainder.split('/').next().unwrap_or(remainder);
    format!("{scheme}://{authority}")
}

/// GPUI adapter for the immutable response projection. Text, selection, hit-testing, copy ranges,
/// and painted highlights all use the same UTF-8 byte-based contracts.
struct ResponseTextElement {
    viewer: Entity<ResponseViewer>,
}

struct ResponseTextPrepaintState {
    layout: MultilineTextLayout,
    selections: Vec<PaintQuad>,
    cursor: Option<PaintQuad>,
}

impl IntoElement for ResponseTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ResponseTextElement {
    type RequestLayoutState = ();
    type PrepaintState = ResponseTextPrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = gpui::relative(1.).into();

        let viewer = self.viewer.read(cx);
        let line_count = line_ranges(viewer.selection.text()).len();
        let line_height = window.line_height();
        style.size.height = (line_height * line_count as f32).into();

        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let (content, selected_range) = {
            let viewer = self.viewer.read(cx);
            (
                viewer.selection.text().to_string(),
                viewer.selection.selected_range(),
            )
        };

        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();
        let ranges = line_ranges(&content);
        let lines = ranges
            .iter()
            .map(|range| {
                let display: SharedString = content[range.start..range.end].to_string().into();
                let run = TextRun {
                    len: display.len(),
                    font: style.font(),
                    color: style.color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                window
                    .text_system()
                    .shape_line(display, font_size, &[run], None)
            })
            .collect();
        let layout = MultilineTextLayout::new(lines, ranges, bounds, line_height);
        let selections = layout.selection_quads(&content, selected_range);
        let cursor = (selected_range.is_empty() && !content.is_empty())
            .then(|| layout.cursor_quad(&content, selected_range.start().utf8(), rgb(INFO).into()))
            .flatten();

        self.viewer.update(cx, |viewer, _cx| {
            viewer.text_layout = Some(layout.clone());
        });

        ResponseTextPrepaintState {
            layout,
            selections,
            cursor,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        for selection in prepaint.selections.drain(..) {
            window.paint_quad(selection);
        }

        for (line_idx, shaped_line) in prepaint.layout.lines.iter().enumerate() {
            let origin = point(
                prepaint.layout.bounds.origin.x,
                prepaint.layout.bounds.origin.y + prepaint.layout.line_height * line_idx as f32,
            );
            shaped_line
                .paint(
                    origin,
                    prepaint.layout.line_height,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                )
                .ok();
        }

        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }
    }
}

impl Render for ResponseViewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (state, redirect_chain, is_httpbingo, response_cookies, jar_count) = {
            let view_model = self.view_model.read(cx);
            let active = view_model.active_request();
            (
                active
                    .map(|request| request.response().clone())
                    .unwrap_or(ResponseState::NotSent),
                active
                    .map(|request| request.redirect_chain().to_vec())
                    .unwrap_or_default(),
                active.is_some_and(|request| request.effective_url().contains("httpbingo.org")),
                response_cookie_evidence(view_model),
                view_model.cookie_count(),
            )
        };
        if matches!(&state, ResponseState::Historical { .. }) && self.pane == ResponsePane::Cookies
        {
            self.pane = ResponsePane::Body;
            self.selection.reset_selection();
            self.text_layout = None;
            self.context_menu_position = None;
        }
        let projection = response_text_projection(&state, self.pane).unwrap_or_default();
        if self.selection.project_text(projection) {
            self.text_layout = None;
        }
        let pane = self.pane;
        let context_menu_position = self.context_menu_position;
        let response_header_count = match &state {
            ResponseState::Success { headers, .. } => headers.len(),
            ResponseState::Historical { response, .. } => response.headers.len(),
            _ => 0,
        };
        let body_tab = self.pane_tab(ResponsePane::Body, "Body", window, cx);
        let headers_tab = self.pane_tab(
            ResponsePane::Headers,
            format!("Headers ({response_header_count})"),
            window,
            cx,
        );
        let cookies_tab = self.pane_tab(
            ResponsePane::Cookies,
            format!("Cookies ({})", response_cookies.len()),
            window,
            cx,
        );
        let has_completed_response = matches!(
            &state,
            ResponseState::Success { .. } | ResponseState::Historical { .. }
        );
        let has_copyable_body = match &state {
            ResponseState::Success { body, .. } => !body.is_empty(),
            ResponseState::Historical { response, .. } => response
                .body
                .preview()
                .is_some_and(|preview| !preview.is_empty()),
            _ => false,
        };
        let is_historical = matches!(
            &state,
            ResponseState::Historical { .. } | ResponseState::HistoricalUnavailable { .. }
        );
        let historical_truncated = matches!(
            &state,
            ResponseState::Historical { response, .. } if response.body.is_truncated()
        );
        let copied_feedback = has_copyable_body && self.copied_feedback;
        let copy_is_focused = self.copy_focus_handle.is_focused(window);
        let completed_status = match &state {
            ResponseState::Success { status, .. } => Some(*status),
            ResponseState::Historical { response, .. } => Some(response.status),
            _ => None,
        };
        let is_transport_failure = matches!(&state, ResponseState::Error { .. });
        let is_timeout = matches!(
            &state,
            ResponseState::Error { message } if message.starts_with("Request timed out after")
        );
        let is_cancelled = matches!(&state, ResponseState::Cancelled);
        let redirect_response_count = redirect_chain
            .iter()
            .filter(|hop| (300..400).contains(&hop.status))
            .count();
        let has_redirect_chain = !redirect_chain.is_empty();
        let redirect_chain_is_partial = matches!(&state, ResponseState::Error { .. });

        let (status, elapsed, size, status_color) = match &state {
            ResponseState::Success {
                status,
                body,
                elapsed_ms,
                ..
            } => (
                status_label(*status),
                format!("{elapsed_ms} ms"),
                format_bytes(body.len()),
                if *status < 400 { OK } else { ERROR },
            ),
            ResponseState::Historical { response, .. } => (
                status_label(response.status),
                format!("{} ms", response.elapsed_ms),
                format_bytes(response.original_size),
                if response.status < 400 { OK } else { ERROR },
            ),
            ResponseState::HistoricalUnavailable { .. } => (
                "Response unavailable".to_string(),
                String::new(),
                String::new(),
                MUTED,
            ),
            ResponseState::Loading => ("Sending…".to_string(), String::new(), String::new(), MUTED),
            ResponseState::Cancelled => {
                ("Cancelled".to_string(), String::new(), String::new(), MUTED)
            }
            ResponseState::Error { .. } if is_timeout => {
                ("Timed out".to_string(), String::new(), String::new(), ERROR)
            }
            ResponseState::Error { .. } => (
                "Request failed".to_string(),
                String::new(),
                String::new(),
                ERROR,
            ),
            ResponseState::NotSent => ("Not sent".to_string(), String::new(), String::new(), MUTED),
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(LINE))
            .rounded(px(14.0))
            .when(context_menu_position.is_none(), |root| {
                root.overflow_hidden()
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h_12()
                    .px_4()
                    .bg(rgb(PANEL_ALT))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .h_full()
                            .gap_3()
                            .child(
                                div()
                                    .child("Response")
                                    .text_size(px(16.0))
                                    .font_family(FONT_HEADING)
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(TEXT)),
                            )
                            .when(is_historical, |row| {
                                row.child(
                                    div()
                                        .debug_selector(|| "response-historical-badge".into())
                                        .px_2()
                                        .py_1()
                                        .rounded(px(6.0))
                                        .bg(rgb(INFO_SOFT))
                                        .font_family(FONT_UI)
                                        .font_weight(FontWeight::BOLD)
                                        .text_size(px(10.0))
                                        .text_color(rgb(INFO))
                                        .child("Historical"),
                                )
                            })
                            .when(has_redirect_chain, |row| {
                                row.child(
                                    div()
                                        .debug_selector(|| "response-redirect-count".into())
                                        .px_2()
                                        .py_1()
                                        .rounded(px(6.0))
                                        .bg(rgb(INFO_SOFT))
                                        .font_family(FONT_MONO)
                                        .font_weight(FontWeight::BOLD)
                                        .text_size(px(10.0))
                                        .text_color(rgb(INFO))
                                        .child(format!("Redirects ({redirect_response_count})")),
                                )
                            })
                            .when(has_completed_response, |row| {
                                row.child(
                                    div()
                                        .flex()
                                        .h_full()
                                        .child(body_tab)
                                        .child(headers_tab)
                                        .when(
                                            matches!(&state, ResponseState::Success { .. }),
                                            |tabs| tabs.child(cookies_tab),
                                        ),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .font_family(FONT_UI)
                            .text_size(px(13.0))
                            .when(has_copyable_body, |row| {
                                row.child(
                                    div()
                                        .id("response-copy-button")
                                        .debug_selector(|| "response-copy-button".into())
                                        .track_focus(&self.copy_focus_handle)
                                        .key_context("ResponseCopyButton")
                                        .role(Role::Button)
                                        .aria_label("Copy full response body")
                                        .h(px(30.0))
                                        .min_w(px(72.0))
                                        .px_2()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .gap_1()
                                        .rounded(px(7.0))
                                        .border_1()
                                        .border_color(rgb(LINE))
                                        .bg(rgb(PANEL))
                                        .text_color(rgb(SUBTEXT))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .cursor_pointer()
                                        .when(!copied_feedback, |button| {
                                            button.hover(|style| {
                                                style
                                                    .bg(rgb(INFO_SOFT))
                                                    .border_color(rgb(INFO))
                                                    .text_color(rgb(INFO))
                                            })
                                        })
                                        .when(copied_feedback, |button| {
                                            button
                                                .bg(rgb(OK_SOFT))
                                                .border_color(rgb(OK))
                                                .text_color(rgb(OK))
                                        })
                                        .when(copy_is_focused, |button| {
                                            button.border_color(rgb(INFO))
                                        })
                                        .on_action(cx.listener(Self::copy_response_body))
                                        .on_mouse_up(
                                            MouseButton::Left,
                                            cx.listener(Self::click_copy_response_body),
                                        )
                                        .child(if copied_feedback { "✓" } else { "⧉" })
                                        .child(
                                            div()
                                                .when(copied_feedback, |label| {
                                                    label.debug_selector(|| {
                                                        "response-copy-feedback".into()
                                                    })
                                                })
                                                .child(if copied_feedback {
                                                    "Copied"
                                                } else {
                                                    "Copy"
                                                }),
                                        ),
                                )
                            })
                            .child(
                                div()
                                    .debug_selector(|| "response-status".into())
                                    .text_color(rgb(status_color))
                                    .font_weight(FontWeight::BOLD)
                                    .child(
                                        div()
                                            .when_some(completed_status, |label, status| {
                                                label.debug_selector(move || {
                                                    format!("response-status-{status}")
                                                })
                                            })
                                            .when(is_transport_failure, |label| {
                                                label.debug_selector(|| {
                                                    "response-transport-error".into()
                                                })
                                            })
                                            .when(is_timeout, |label| {
                                                label.debug_selector(|| {
                                                    "response-timeout-error".into()
                                                })
                                            })
                                            .when(is_cancelled, |label| {
                                                label.debug_selector(|| "response-cancelled".into())
                                            })
                                            .child(status),
                                    ),
                            )
                            .when(!elapsed.is_empty(), |row| {
                                row.child(
                                    div()
                                        .text_color(rgb(SUBTEXT))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(elapsed),
                                )
                            })
                            .when(!size.is_empty(), |row| {
                                row.child(
                                    div()
                                        .text_color(rgb(SUBTEXT))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(size),
                                )
                            }),
                    ),
            )
            .when(has_completed_response, |root| {
                root.child(
                    div()
                        .debug_selector(|| "response-echo-bar".into())
                        .h(px(36.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_4()
                        .bg(rgb(PANEL_ALT))
                        .border_b_1()
                        .border_color(rgb(LINE))
                        .font_family(FONT_UI)
                        .text_size(px(11.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(OK))
                                .child("●")
                                .child(if pane == ResponsePane::Cookies {
                                    "Response cookie evidence"
                                } else if is_historical {
                                    "Persisted historical response"
                                } else if is_httpbingo {
                                    "HTTPBingo echo"
                                } else {
                                    "Response payload"
                                }),
                        )
                        .when(is_historical, |bar| {
                            bar.child(
                                div()
                                    .debug_selector(|| "response-historical-storage".into())
                                    .text_color(rgb(SUBTEXT))
                                    .child(if historical_truncated {
                                        "Stored preview · truncated at 256 KiB"
                                    } else {
                                        "Stored sanitized response"
                                    }),
                            )
                        })
                        .when(is_httpbingo && !is_historical, |bar| {
                            bar.child(div().text_color(rgb(SUBTEXT)).child("stable subset"))
                        }),
                )
            })
            .when(has_redirect_chain, |root| {
                root.child(render_redirect_chain(
                    &redirect_chain,
                    redirect_chain_is_partial,
                ))
            })
            .child(match state {
                ResponseState::NotSent => div()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .items_start()
                    .justify_start()
                    .gap_2()
                    .p_5()
                    .bg(rgb(PANEL_ALT))
                    .child(
                        div()
                            .font_family(FONT_HEADING)
                            .text_size(px(20.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(TEXT))
                            .child("Send a request to view response"),
                    )
                    .child(
                        div()
                            .font_family(FONT_UI)
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(SUBTEXT))
                            .child("Status, headers, and payload will appear here."),
                    ),
                ResponseState::Loading => div()
                    .debug_selector(|| "response-loading".into())
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgb(CODE_BG))
                    .font_family(FONT_MONO)
                    .text_size(px(13.0))
                    .text_color(rgb(CODE_TEXT))
                    .child("Waiting for the server…"),
                ResponseState::Cancelled => div()
                    .debug_selector(|| "response-cancelled-content".into())
                    .flex_1()
                    .min_h_0()
                    .child(self.render_selectable_content(window, cx)),
                ResponseState::Success {
                    body: _, headers, ..
                } => match pane {
                    ResponsePane::Body => div()
                        .flex_1()
                        .min_h_0()
                        .child(self.render_selectable_content(window, cx)),
                    ResponsePane::Headers => div()
                        .flex_1()
                        .min_h_0()
                        .child(render_response_headers(&headers)),
                    ResponsePane::Cookies => div()
                        .flex_1()
                        .min_h_0()
                        .child(self.render_cookie_content(response_cookies, jar_count, window, cx)),
                },
                ResponseState::Historical { response, .. } => match pane {
                    ResponsePane::Body => match response.body {
                        HistoricalResponseBody::Empty => div()
                            .debug_selector(|| "response-historical-empty".into())
                            .flex_1()
                            .min_h_0()
                            .child(self.render_selectable_content(window, cx)),
                        HistoricalResponseBody::Text(_body) => div()
                            .flex_1()
                            .min_h_0()
                            .child(self.render_selectable_content(window, cx)),
                        HistoricalResponseBody::TruncatedText(_body) => div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h_0()
                            .child(
                                div()
                                    .debug_selector(|| "response-historical-truncated".into())
                                    .flex_none()
                                    .px_4()
                                    .py_2()
                                    .bg(rgb(INFO_SOFT))
                                    .font_family(FONT_UI)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_size(px(11.0))
                                    .text_color(rgb(INFO))
                                    .child("Persisted preview is truncated at 256 KiB."),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .child(self.render_selectable_content(window, cx)),
                            ),
                        HistoricalResponseBody::Unsupported => div()
                            .debug_selector(|| "response-historical-body-not-stored".into())
                            .flex_1()
                            .min_h_0()
                            .child(self.render_selectable_content(window, cx)),
                    },
                    ResponsePane::Headers => div()
                        .flex_1()
                        .min_h_0()
                        .child(render_response_headers(&response.headers)),
                    ResponsePane::Cookies => div()
                        .flex_1()
                        .min_h_0()
                        .child(self.render_selectable_content(window, cx)),
                },
                ResponseState::HistoricalUnavailable { .. } => div()
                    .debug_selector(|| "response-historical-unavailable".into())
                    .flex_1()
                    .min_h_0()
                    .child(self.render_selectable_content(window, cx)),
                ResponseState::Error { message: _ } => div()
                    .when(is_timeout, |content| {
                        content.debug_selector(|| "response-timeout-content".into())
                    })
                    .flex_1()
                    .min_h_0()
                    .child(self.render_selectable_content(window, cx)),
            })
            .when_some(context_menu_position, |root, position| {
                root.child(edit_context_menu(
                    position,
                    "response-edit-menu",
                    READ_ONLY_ACTIONS,
                    Self::handle_context_menu_action,
                    window,
                    cx,
                ))
            })
    }
}

fn render_redirect_chain(chain: &[RedirectHop], partial: bool) -> impl IntoElement {
    let redirect_count = chain
        .iter()
        .filter(|hop| (300..400).contains(&hop.status))
        .count();
    div()
        .id("redirect-chain-scroll")
        .debug_selector(|| "redirect-chain".into())
        .max_h(px(164.0))
        .flex_none()
        .overflow_y_scroll()
        .bg(rgb(PANEL_ALT))
        .border_b_1()
        .border_color(rgb(LINE))
        .font_family(FONT_MONO)
        .child(
            div()
                .debug_selector(|| "redirect-chain-count".into())
                .h(px(30.0))
                .px_4()
                .flex()
                .items_center()
                .gap_2()
                .border_b_1()
                .border_color(rgb(LINE))
                .font_family(FONT_UI)
                .font_weight(FontWeight::BOLD)
                .text_size(px(10.0))
                .text_color(rgb(if partial { ERROR } else { INFO }))
                .when(partial, |header| {
                    header.child(
                        div()
                            .debug_selector(|| "redirect-chain-partial".into())
                            .child("incomplete"),
                    )
                })
                .child(if partial {
                    format!("Partial redirect chain · {redirect_count} observed")
                } else {
                    format!("Redirect chain · {redirect_count} observed")
                }),
        )
        .children(chain.iter().enumerate().map(|(index, hop)| {
            let row_selector = format!("redirect-hop-{index}");
            let status_selector = format!("redirect-hop-status-{index}");
            let location_selector = format!("redirect-hop-location-{index}");
            let is_terminal = !(300..400).contains(&hop.status);
            div()
                .debug_selector(move || row_selector.clone())
                .min_h(px(34.0))
                .px_4()
                .py_1()
                .flex()
                .items_center()
                .gap_3()
                .border_b_1()
                .border_color(rgb(LINE))
                .text_size(px(10.0))
                .child(
                    div()
                        .debug_selector(move || status_selector.clone())
                        .w(px(36.0))
                        .flex_none()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(if is_terminal { OK } else { INFO }))
                        .child(hop.status.to_string()),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .text_color(rgb(TEXT))
                        .child(hop.url.clone()),
                )
                .child(match &hop.location {
                    Some(location) => div()
                        .debug_selector(move || location_selector.clone())
                        .w(px(280.0))
                        .flex_none()
                        .text_color(rgb(SUBTEXT))
                        .child(format!("Location: {location}")),
                    None => div()
                        .w(px(280.0))
                        .flex_none()
                        .text_color(rgb(OK))
                        .child("terminal response"),
                })
        }))
}

fn status_label(status: u16) -> String {
    let reason = match status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Server Error",
        502 => "Bad Gateway",
        503 => "Unavailable",
        _ => "Response",
    };
    format!("{status} {reason}")
}

fn format_bytes(bytes: usize) -> String {
    if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        response_origin, response_text_projection, set_cookie_name, status_label, Copy,
        DismissResponseContextMenu, ResponsePane, ResponseViewer, SelectAll,
    };
    use crate::{
        app::{ResponseState, WorkspaceViewModel},
        http::executor::RequestResult,
        models::{HistoricalResponse, HistoricalResponseBody},
        ui::text_editor::TextRange,
        utils::formatter::format_response_body,
    };
    use gpui::{
        point, px, AppContext, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
        MouseUpEvent, ScrollDelta, ScrollWheelEvent, TestAppContext,
    };

    #[test]
    fn unknown_success_reason_keeps_the_exact_http_status_visible() {
        assert_eq!(status_label(418), "418 Response");
    }

    #[test]
    fn response_cookie_projection_keeps_only_name_and_origin() {
        assert_eq!(
            set_cookie_name("session=super-secret; Path=/; HttpOnly"),
            Some("session".to_string())
        );
        assert_eq!(
            response_origin("https://httpbingo.org/cookies?source=response"),
            "https://httpbingo.org"
        );
    }

    #[test]
    fn response_projection_covers_formatted_raw_empty_and_unsupported_bodies() {
        let json = r#"{"emoji":"😀","nested":{"value":"中"}}"#;
        let success = ResponseState::Success {
            status: 200,
            body: json.to_string(),
            headers: Vec::new(),
            elapsed_ms: 1,
        };
        assert_eq!(
            response_text_projection(&success, ResponsePane::Body),
            Some(format_response_body(json))
        );
        assert_eq!(
            response_text_projection(&success, ResponsePane::Headers),
            None
        );

        let plain = "raw 😀 中\nsecond line";
        let raw = ResponseState::Success {
            status: 200,
            body: plain.to_string(),
            headers: Vec::new(),
            elapsed_ms: 1,
        };
        assert_eq!(
            response_text_projection(&raw, ResponsePane::Body),
            Some(plain.to_string())
        );

        for (body, expected) in [
            (HistoricalResponseBody::Empty, "Empty response body"),
            (HistoricalResponseBody::Unsupported, "Body not stored"),
        ] {
            let historical = ResponseState::Historical {
                entry_id: "history-1".to_string(),
                response: HistoricalResponse {
                    status: 200,
                    headers: Vec::new(),
                    body,
                    media_type: None,
                    elapsed_ms: 1,
                    original_size: 0,
                    persisted_size: 0,
                },
            };
            assert_eq!(
                response_text_projection(&historical, ResponsePane::Body),
                Some(expected.to_string())
            );
        }
    }

    #[gpui::test]
    fn response_unicode_drag_copy_word_select_all_and_clear_share_one_range(
        cx: &mut TestAppContext,
    ) {
        let body = std::iter::once("A😀中 emoji".to_string())
            .chain((0..60).map(|line| format!("line-{line:02}")))
            .collect::<Vec<_>>()
            .join("\n");
        let expected_body = body.clone();
        let workspace = cx.new(|_| {
            let mut workspace = WorkspaceViewModel::new();
            workspace
                .active_request_mut()
                .expect("default request")
                .set_url("https://example.test/response-selection");
            let pending = workspace.begin_send().expect("send should start");
            assert!(workspace.complete_send(pending, Ok(RequestResult::success(body))));
            workspace
        });
        let (viewer, visual) =
            cx.add_window_view(move |_, cx| ResponseViewer::new(workspace.clone(), cx));
        visual.run_until_parked();

        let word_utf8 = expected_body.find("emoji").unwrap() + 2;
        let (drag_start, drag_end, word_position) = viewer.read_with(visual, |viewer, _| {
            let layout = viewer
                .text_layout
                .as_ref()
                .expect("response text should be painted");
            let position_for_utf8 = |utf8: usize| {
                let offset = viewer.selection.offset_from_utf8(utf8).unwrap();
                layout
                    .bounds_for_range(viewer.selection.text(), TextRange::collapsed(offset))
                    .expect("offset should have layout geometry")
                    .center()
            };
            (
                position_for_utf8(1),
                position_for_utf8("A😀中".len()),
                position_for_utf8(word_utf8),
            )
        });

        visual.update(|window, app| {
            viewer.update(app, |viewer, cx| {
                viewer.on_mouse_down(
                    &MouseDownEvent {
                        position: drag_start,
                        modifiers: Modifiers::none(),
                        button: MouseButton::Left,
                        click_count: 1,
                        first_mouse: false,
                    },
                    window,
                    cx,
                );
                viewer.on_mouse_move(
                    &MouseMoveEvent {
                        position: drag_end,
                        modifiers: Modifiers::none(),
                        pressed_button: Some(MouseButton::Left),
                    },
                    window,
                    cx,
                );
                viewer.on_mouse_up(
                    &MouseUpEvent {
                        position: drag_end,
                        modifiers: Modifiers::none(),
                        button: MouseButton::Left,
                        click_count: 1,
                    },
                    window,
                    cx,
                );
                viewer.copy(&Copy, window, cx);
            });
        });
        assert_eq!(
            viewer.read_with(visual, |viewer, _| viewer
                .selection
                .selected_text()
                .to_string()),
            "😀中"
        );
        assert_eq!(
            visual
                .read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("😀中")
        );
        assert_eq!(
            viewer.read_with(visual, |viewer, _| viewer
                .text_layout
                .as_ref()
                .unwrap()
                .selection_quads(viewer.selection.text(), viewer.selection.selected_range())
                .len()),
            1,
            "the copied UTF-8 range must produce the visible highlight"
        );

        visual.update(|window, app| {
            viewer.update(app, |viewer, cx| {
                viewer.on_mouse_down(
                    &MouseDownEvent {
                        position: word_position,
                        modifiers: Modifiers::none(),
                        button: MouseButton::Left,
                        click_count: 2,
                        first_mouse: false,
                    },
                    window,
                    cx,
                );
            });
        });
        assert_eq!(
            viewer.read_with(visual, |viewer, _| viewer
                .selection
                .selected_text()
                .to_string()),
            "emoji"
        );

        visual.update(|window, app| {
            viewer.update(app, |viewer, cx| {
                viewer.select_all(&SelectAll, window, cx);
                viewer.copy(&Copy, window, cx);
            });
        });
        assert_eq!(
            visual
                .read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some(expected_body.as_str())
        );
        visual.update(|window, app| {
            viewer.update(app, |viewer, cx| {
                viewer.dismiss_context_menu(&DismissResponseContextMenu, window, cx);
            });
        });
        assert!(viewer.read_with(visual, |viewer, _| viewer
            .selection
            .selected_range()
            .is_empty()));
    }

    #[gpui::test]
    fn response_selection_survives_scroll_and_off_viewport_drag(cx: &mut TestAppContext) {
        let body = (0..80)
            .map(|line| format!("行-{line:02}-😀"))
            .collect::<Vec<_>>()
            .join("\n");
        let expected_body = body.clone();
        let workspace = cx.new(|_| {
            let mut workspace = WorkspaceViewModel::new();
            workspace
                .active_request_mut()
                .expect("default request")
                .set_url("https://example.test/response-scroll-selection");
            let pending = workspace.begin_send().expect("send should start");
            assert!(workspace.complete_send(pending, Ok(RequestResult::success(body))));
            workspace
        });
        let (viewer, visual) =
            cx.add_window_view(move |_, cx| ResponseViewer::new(workspace.clone(), cx));
        visual.run_until_parked();

        let (start, below_document) = viewer.read_with(visual, |viewer, _| {
            let layout = viewer
                .text_layout
                .as_ref()
                .expect("painted response layout");
            let offset = viewer.selection.offset_from_utf8(0).unwrap();
            let start = layout
                .bounds_for_range(viewer.selection.text(), TextRange::collapsed(offset))
                .unwrap()
                .center();
            (
                start,
                point(layout.bounds.left(), layout.bounds.bottom() + px(20.0)),
            )
        });
        visual.update(|window, app| {
            viewer.update(app, |viewer, cx| {
                viewer.on_mouse_down(
                    &MouseDownEvent {
                        position: start,
                        modifiers: Modifiers::none(),
                        button: MouseButton::Left,
                        click_count: 1,
                        first_mouse: false,
                    },
                    window,
                    cx,
                );
                viewer.on_mouse_move(
                    &MouseMoveEvent {
                        position: below_document,
                        modifiers: Modifiers::none(),
                        pressed_button: Some(MouseButton::Left),
                    },
                    window,
                    cx,
                );
                viewer.on_mouse_up(
                    &MouseUpEvent {
                        position: below_document,
                        modifiers: Modifiers::none(),
                        button: MouseButton::Left,
                        click_count: 1,
                    },
                    window,
                    cx,
                );
            });
        });
        assert_eq!(
            viewer.read_with(visual, |viewer, _| viewer
                .selection
                .selected_text()
                .to_string()),
            expected_body.clone()
        );

        let viewport = visual
            .debug_bounds("response-content")
            .expect("response viewport should be rendered");
        visual.simulate_event(ScrollWheelEvent {
            position: viewport.center(),
            delta: ScrollDelta::Pixels(point(px(0.0), px(-400.0))),
            ..Default::default()
        });
        visual.run_until_parked();
        assert_eq!(
            viewer.read_with(visual, |viewer, _| viewer
                .selection
                .selected_text()
                .to_string()),
            expected_body,
            "scrolling and repainting must not change the canonical selection"
        );
        assert!(!viewer.read_with(visual, |viewer, _| viewer
            .text_layout
            .as_ref()
            .unwrap()
            .selection_quads(viewer.selection.text(), viewer.selection.selected_range())
            .is_empty()));
    }
}

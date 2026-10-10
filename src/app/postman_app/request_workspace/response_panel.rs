use crate::app::RequestTabId;
use crate::ui::{
    components::kit_controls,
    theme::{metrics as m, ERROR_SOFT, SIDEBAR},
};
use gpui::{
    actions, div, point, prelude::FluentBuilder, px, rems, App, Bounds, ClipboardItem, Context,
    CursorStyle, Element, ElementId, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    GlobalElementId, InteractiveElement, IntoElement, KeyBinding, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, ParentElement, Pixels, Point, Render,
    Role, SharedString, StatefulInteractiveElement, Style, Styled, Subscription, TextAlign,
    TextRun, Window,
};
use gpui_kit::base::ElementExt;
use gpui_kit::{
    assets::IconName,
    base::Tab,
    component::{
        scroll::{ScrollableElement, ScrollbarAxis},
        Icon,
    },
};
use std::{
    collections::{BTreeMap, HashMap},
    time::Duration,
};

mod headers;

use headers::render_response_headers;

use crate::{
    app::{ActivateControl, CookieJarEntry, ResponseState, WorkspaceViewModel},
    models::{HistoricalResponseBody, RedirectHop},
    ui::components::common::edit_context_menu::{
        edit_context_menu, EditContextAction, READ_ONLY_ACTIONS,
    },
    ui::text_editor::{ReadOnlyTextSelection, TextOffset},
    ui::text_layout::{line_ranges, LineRange, MultilineTextLayout},
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
    ToggleLayout,
    PanelSizes,
}

/// Response surface owned by the request workspace.
pub struct ResponseViewer {
    view_model: Entity<WorkspaceViewModel>,
    pane: ResponsePane,
    pretty: bool,
    stacked: bool,
    width: Pixels,
    active_tab: Option<RequestTabId>,
    retained: HashMap<RequestTabId, ResponsePresentation>,
    body_scroll: gpui::ScrollHandle,
    header_scroll: gpui::ScrollHandle,
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

struct ResponsePresentation {
    pane: ResponsePane,
    pretty: bool,
    selection: ReadOnlyTextSelection,
    body_scroll: gpui::ScrollHandle,
    header_scroll: gpui::ScrollHandle,
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
            pretty: true,
            stacked: false,
            width: px(600.),
            active_tab: None,
            retained: HashMap::new(),
            body_scroll: gpui::ScrollHandle::new(),
            header_scroll: gpui::ScrollHandle::new(),
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

    pub(super) fn set_stacked(&mut self, stacked: bool, cx: &mut Context<Self>) {
        if self.stacked != stacked {
            self.stacked = stacked;
            cx.notify();
        }
    }

    fn retain_active_presentation(&mut self, cx: &App) {
        let model = self.view_model.read(cx);
        let active = model.active_tab_id();
        if active == self.active_tab {
            return;
        }
        if let Some(previous) = self.active_tab {
            self.retained.insert(
                previous,
                ResponsePresentation {
                    pane: self.pane,
                    pretty: self.pretty,
                    selection: std::mem::take(&mut self.selection),
                    body_scroll: std::mem::take(&mut self.body_scroll),
                    header_scroll: std::mem::take(&mut self.header_scroll),
                },
            );
        }
        self.retained
            .retain(|id, _| model.tabs().iter().any(|tab| tab.tab_id() == *id));
        if let Some(saved) = active.and_then(|id| self.retained.remove(&id)) {
            self.pane = saved.pane;
            self.pretty = saved.pretty;
            self.selection = saved.selection;
            self.body_scroll = saved.body_scroll;
            self.header_scroll = saved.header_scroll;
        } else {
            self.pane = ResponsePane::Body;
            self.pretty = true;
            self.selection = ReadOnlyTextSelection::new();
            self.body_scroll = gpui::ScrollHandle::new();
            self.header_scroll = gpui::ScrollHandle::new();
        }
        self.active_tab = active;
        self.text_layout = None;
        self.context_menu_position = None;
    }

    fn set_pretty(&mut self, pretty: bool, cx: &mut Context<Self>) {
        if self.pretty != pretty {
            self.pretty = pretty;
            self.selection.reset_selection();
            self.body_scroll.set_offset(point(px(0.), px(0.)));
            cx.notify();
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
        Tab::new(selector)
            .selected(active)
            .debug_selector(move || selector.into())
            .track_focus(&focus_handle)
            .key_context("ResponsePaneTab")
            .accessibility_label(format!("{label} response pane"))
            .h(m::PANE_TAB)
            .flex()
            .items_center()
            .px_0()
            .cursor_pointer()
            .when(active, |d| {
                d.border_b_2()
                    .border_color(ACCENT.resolve(cx))
                    .text_color(TEXT.resolve(cx))
                    .font_weight(FontWeight::MEDIUM)
            })
            .when(!active, |d| {
                d.text_color(MUTED.resolve(cx))
                    .hover(|s| s.text_color(SUBTEXT.resolve(cx)))
            })
            .when(focused, |d| {
                d.bg(ACCENT_SOFT.resolve(cx))
                    .border_1()
                    .border_color(ACCENT.resolve(cx))
            })
            .text_size(rems(11. / 16.))
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
            .on_click(cx.listener(move |this, _, window, cx| {
                click_focus_handle.focus(window, cx);
                this.select_pane(pane, cx);
            }))
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
        let content = div()
            .id("response-content")
            .debug_selector(|| "response-content".into())
            .cursor(CursorStyle::IBeam)
            .track_focus(&self.focus_handle(cx))
            .key_context("ResponseContent")
            .border_1()
            .border_color(if self.focus_handle.is_focused(window) {
                INFO.resolve(cx)
            } else {
                CODE_BG.resolve(cx)
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
            .min_w_0()
            .bg(SIDEBAR.resolve(cx))
            .text_color(CODE_TEXT.resolve(cx))
            .font_family(FONT_MONO)
            .text_size(m::CODE)
            .line_height(gpui::relative(m::CODE_LINE_HEIGHT))
            .flex()
            .flex_col()
            .items_start()
            .overflow_scroll()
            .track_scroll(&self.body_scroll)
            .child(
                div()
                    .debug_selector(|| "response-document".into())
                    .flex_none()
                    .pt(rems(19. / 16.))
                    .pb_6()
                    .min_w_full()
                    .child(ResponseTextElement {
                        viewer: cx.entity().clone(),
                    }),
            );
        // The scrollbar overlays the viewport, not its scrolling children. Putting
        // it inside `content` moves the track with the document and creates overflow.
        div()
            .relative()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(content)
            .scrollbar(&self.body_scroll, ScrollbarAxis::Both)
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
            .bg(CODE_BG.resolve(cx))
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
                                    .text_color(TEXT.resolve(cx))
                                    .child(format!(
                                        "CURRENT RESPONSE / REDIRECT CHAIN · COOKIES ({cookie_count})"
                                    )),
                            )
                            .child(
                                div()
                                    .font_family(FONT_UI)
                                    .text_size(px(9.0))
                                    .text_color(SUBTEXT.resolve(cx))
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
                            .border_color(INFO.resolve(cx))
                            .bg(PANEL.resolve(cx))
                            .font_family(FONT_UI)
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(10.0))
                            .text_color(INFO.resolve(cx))
                            .cursor_pointer()
                            .hover(|style| style.bg(INFO_SOFT.resolve(cx)))
                            .when(self.open_cookie_focus_handle.is_focused(window), |button| {
                                button.border_2().border_color(ACCENT.resolve(cx))
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
                        .border_color(LINE.resolve(cx))
                        .bg(PANEL_ALT.resolve(cx))
                        .child(
                            div()
                                .font_family(FONT_HEADING)
                                .font_weight(FontWeight::BOLD)
                                .text_size(px(16.0))
                                .text_color(TEXT.resolve(cx))
                                .child("No Set-Cookie received"),
                        )
                        .child(
                            div()
                                .font_family(FONT_UI)
                                .text_size(px(11.0))
                                .text_color(SUBTEXT.resolve(cx))
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
                                .border_color(LINE.resolve(cx))
                                .bg(INFO_SOFT.resolve(cx))
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
                                        .text_color(TEXT.resolve(cx))
                                        .child(cookie.name),
                                )
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex_1()
                                        .overflow_hidden()
                                        .font_family(FONT_MONO)
                                        .text_size(px(10.0))
                                        .text_color(SUBTEXT.resolve(cx))
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
                                        .bg(PANEL.resolve(cx))
                                        .font_family(FONT_UI)
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_size(px(9.0))
                                        .text_color(MUTED.resolve(cx))
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
                                        .bg((if cookie.stored_now { OK_SOFT } else { PANEL_ALT }).resolve(cx))
                                        .font_family(FONT_UI)
                                        .font_weight(FontWeight::BOLD)
                                        .text_size(px(9.0))
                                        .text_color((if cookie.stored_now { OK } else { MUTED }).resolve(cx))
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
    line_numbers: bool,
}

impl IntoElement for ResponseTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ResponseTextElement {
    type RequestLayoutState = (Vec<gpui::ShapedLine>, Vec<LineRange>, Pixels, bool);
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
        let text_style = window.text_style();
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let viewer = self.viewer.read(cx);
        let ranges = line_ranges(viewer.selection.text());
        let lines: Vec<_> = ranges
            .iter()
            .map(|range| {
                let text: SharedString = viewer.selection.text()[range.start..range.end]
                    .to_string()
                    .into();
                let run = TextRun {
                    len: text.len(),
                    font: text_style.font(),
                    color: text_style.color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                window
                    .text_system()
                    .shape_line(text, font_size, &[run], None)
            })
            .collect();
        let line_numbers = viewer.pretty && viewer.view_model.read(cx).active_request().is_some_and(|request| {
            matches!(request.response(), ResponseState::Success { body, .. } if !body.is_empty()) || matches!(request.response(), ResponseState::Historical { response, .. } if response.body.preview().is_some_and(|body| !body.is_empty()))
        });
        let gutter = rems(if line_numbers { 48. } else { 22. } / 16.).to_pixels(window.rem_size());
        let width = lines
            .iter()
            .map(|line| line.width)
            .fold(px(0.), |a, b| a.max(b));
        style.size.width = (width + gutter + rems(20. / 16.).to_pixels(window.rem_size())).into();
        style.min_size.width = gpui::relative(1.).into();
        let line_count = ranges.len();
        let line_height = window.line_height();
        style.size.height = (line_height * line_count as f32).into();

        (
            window.request_layout(style, [], cx),
            (lines, ranges, gutter, line_numbers),
        )
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
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

        let line_height = window.line_height();
        let (lines, ranges, gutter, line_numbers) = request_layout;
        let mut text_bounds = bounds;
        text_bounds.origin.x += *gutter;
        text_bounds.size.width -= *gutter;
        let layout = MultilineTextLayout::new(
            std::mem::take(lines),
            std::mem::take(ranges),
            text_bounds,
            line_height,
        );
        let selections = layout.selection_quads(
            &content,
            selected_range,
            crate::ui::theme::ACCENT_SOFT.resolve(cx),
        );
        let cursor = (self.viewer.read(cx).focus_handle.is_focused(window)
            && selected_range.is_empty()
            && !content.is_empty())
        .then(|| {
            layout.cursor_quad(
                &content,
                selected_range.start().utf8(),
                INFO.resolve(cx).into(),
            )
        })
        .flatten();

        self.viewer.update(cx, |viewer, _cx| {
            viewer.text_layout = Some(layout.clone());
        });

        ResponseTextPrepaintState {
            layout,
            selections,
            cursor,
            line_numbers: *line_numbers,
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
            if prepaint.line_numbers {
                let number: SharedString = (line_idx + 1).to_string().into();
                let style = window.text_style();
                let run = TextRun {
                    len: number.len(),
                    font: style.font(),
                    color: MUTED.resolve(cx).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let number = window.text_system().shape_line(
                    number,
                    rems(10. / 16.).to_pixels(window.rem_size()),
                    &[run],
                    None,
                );
                let number_origin = point(
                    origin.x - rems(17. / 16.).to_pixels(window.rem_size()) - number.width,
                    origin.y,
                );
                number
                    .paint(
                        number_origin,
                        prepaint.layout.line_height,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    )
                    .ok();
            }
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
        self.retain_active_presentation(cx);
        let (state, redirect_chain, response_cookies, jar_count) = {
            let view_model = self.view_model.read(cx);
            let active = view_model.active_request();
            (
                active
                    .map(|request| request.response().clone())
                    .unwrap_or(ResponseState::NotSent),
                active
                    .map(|request| request.redirect_chain().to_vec())
                    .unwrap_or_default(),
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
        let projection = if self.pane == ResponsePane::Body && !self.pretty {
            self.raw_response_body(cx)
                .or_else(|| response_text_projection(&state, self.pane))
        } else {
            response_text_projection(&state, self.pane)
        }
        .unwrap_or_default();
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

        let media_type = match &state {
            ResponseState::Success { headers, .. } => headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                .map(|(_, v)| v.clone()),
            ResponseState::Historical { response, .. } => response.media_type.clone(),
            _ => None,
        };
        let empty_body = matches!(&state, ResponseState::Success { body, .. } if body.is_empty());
        let body_format = if media_type.as_deref().is_some_and(|m| m.contains("json")) {
            "JSON"
        } else {
            "Text"
        };
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(SIDEBAR.resolve(cx))
            .on_prepaint({
                let this = cx.weak_entity();
                let width = self.width;
                move |bounds, window, cx| {
                    if bounds.size.width != width {
                        window.defer(cx, move |_, cx| {
                            let _ = this.update(cx, |this, cx| {
                                this.width = bounds.size.width;
                                cx.notify();
                            });
                        });
                    }
                }
            })
            .when(context_menu_position.is_none(), |root| {
                root.overflow_hidden()
            })
            .child(
                div()
                    .debug_selector(|| "response-heading".into())
                    .h(rems(50. / 16.))
                    .flex_none()
                    .px(rems(22. / 16.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .child(
                        div()
                            .font_family(FONT_UI)
                            .text_size(m::LABEL)
                            .font_weight(m::MEDIUM)
                            .text_color(TEXT.resolve(cx))
                            .child("Response"),
                    )
                    .child(
                        div()
                            .debug_selector(|| "response-status".into())
                            .flex_none()
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .font_family(FONT_MONO)
                            .text_size(m::CAPTION)
                            .text_color(status_color.resolve(cx))
                            .when(has_completed_response, |d| {
                                d.bg(if completed_status.is_some_and(|n| n >= 400) {
                                    ERROR_SOFT
                                } else {
                                    OK_SOFT
                                }
                                .resolve(cx))
                            })
                            .child(
                                div()
                                    .when_some(completed_status, |d, status| {
                                        d.debug_selector(move || {
                                            format!("response-status-{status}")
                                        })
                                    })
                                    .when(is_transport_failure, |d| {
                                        d.debug_selector(|| "response-transport-error".into())
                                    })
                                    .when(is_timeout, |d| {
                                        d.debug_selector(|| "response-timeout-error".into())
                                    })
                                    .when(is_cancelled, |d| {
                                        d.debug_selector(|| "response-cancelled".into())
                                    })
                                    .child(status),
                            ),
                    )
                    .when(!elapsed.is_empty(), |d| {
                        d.child(
                            div()
                                .debug_selector(|| "response-elapsed".into())
                                .font_family(FONT_MONO)
                                .text_size(m::CAPTION)
                                .text_color(MUTED.resolve(cx))
                                .child(elapsed),
                        )
                    })
                    .when(!size.is_empty() && self.width > px(410.), |d| {
                        d.child(
                            div()
                                .debug_selector(|| "response-size".into())
                                .font_family(FONT_MONO)
                                .text_size(m::CAPTION)
                                .text_color(MUTED.resolve(cx))
                                .child(size),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        kit_controls::icon_button(
                            "response-panel-sizes",
                            IconName::Settings2,
                            "Panel sizes…",
                        )
                        .size_7()
                        .debug_selector(|| "response-panel-sizes".into())
                        .on_click(
                            cx.listener(|_, _, _, cx| cx.emit(ResponseViewerEvent::PanelSizes)),
                        ),
                    )
                    .child(
                        kit_controls::icon_button(
                            "response-layout-toggle",
                            if self.stacked {
                                IconName::Columns2
                            } else {
                                IconName::Rows2
                            },
                            if self.stacked {
                                "Use automatic response layout"
                            } else {
                                "Stack request and response"
                            },
                        )
                        .size_7()
                        .debug_selector(|| "response-layout-toggle".into())
                        .on_click(
                            cx.listener(|_, _, _, cx| cx.emit(ResponseViewerEvent::ToggleLayout)),
                        ),
                    ),
            )
            .when(has_completed_response, |root| {
                root.child(
                    div()
                        .id("response-tabs")
                        .debug_selector(|| "response-tabs".into())
                        .min_h(m::PANE_TAB)
                        .flex_none()
                        .px(rems(22. / 16.))
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(rems(18. / 16.))
                        .border_b_1()
                        .border_color(LINE.resolve(cx))
                        .child(body_tab)
                        .child(headers_tab)
                        .when(matches!(&state, ResponseState::Success { .. }), |d| {
                            d.child(cookies_tab)
                        })
                        .when(pane == ResponsePane::Body && has_copyable_body, |d| {
                            d.child(
                                div()
                                    .flex()
                                    .border_1()
                                    .border_color(LINE.resolve(cx))
                                    .rounded(m::RADIUS)
                                    .p_0p5()
                                    .children(
                                        [
                                            (true, "Pretty", "response-pretty"),
                                            (false, "Raw", "response-raw"),
                                        ]
                                        .into_iter()
                                        .map(
                                            |(pretty, label, id)| {
                                                kit_controls::editor_button(id, label, cx)
                                                    .debug_selector(move || id.into())
                                                    .h(rems(24. / 16.))
                                                    .px(rems(7. / 16.))
                                                    .font_weight(FontWeight::NORMAL)
                                                    .text_size(rems(10. / 16.))
                                                    .when(self.pretty == pretty, |b| {
                                                        b.bg(PANEL.resolve(cx))
                                                            .text_color(TEXT.resolve(cx))
                                                    })
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.set_pretty(pretty, cx)
                                                    }))
                                            },
                                        ),
                                    ),
                            )
                        })
                        .child(div().flex_1())
                        .when(pane == ResponsePane::Body && has_copyable_body, |d| {
                            d.child(
                                div()
                                    .text_size(m::CAPTION)
                                    .text_color(MUTED.resolve(cx))
                                    .child(body_format),
                            )
                        })
                        .when(has_copyable_body, |d| {
                            d.child(
                                kit_controls::editor_button("response-copy-button", "", cx)
                                    .debug_selector(|| "response-copy-button".into())
                                    .accessibility_label("Copy full response body")
                                    .track_focus(&self.copy_focus_handle)
                                    .key_context("ResponseCopyButton")
                                    .on_action(cx.listener(Self::copy_response_body))
                                    .size(m::ICON_BUTTON)
                                    .px_0()
                                    .child(
                                        Icon::new(if copied_feedback {
                                            IconName::Check
                                        } else {
                                            IconName::Copy
                                        })
                                        .size(m::SMALL_ICON),
                                    )
                                    .when(copied_feedback, |b| {
                                        b.child(
                                            div()
                                                .debug_selector(|| "response-copy-feedback".into()),
                                        )
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.copy_raw_response_body(cx)
                                    })),
                            )
                        }),
                )
            })
            .when(is_historical, |root| {
                root.child(
                    div()
                        .debug_selector(|| "response-historical-badge".into())
                        .flex_none()
                        .px_4()
                        .py_1()
                        .font_family(FONT_UI)
                        .text_size(m::CAPTION)
                        .text_color(SUBTEXT.resolve(cx))
                        .child(
                            div()
                                .debug_selector(|| "response-historical-storage".into())
                                .child(if historical_truncated {
                                    "Historical · stored sanitized preview · truncated at 256 KiB"
                                } else {
                                    "Historical · stored sanitized response"
                                }),
                        ),
                )
            })
            .when(has_redirect_chain, |root| {
                root.child(render_redirect_chain(
                    &redirect_chain,
                    redirect_chain_is_partial,
                    cx,
                ))
            })
            .child(match state {
                ResponseState::NotSent => div()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .p_5()
                    .bg(SIDEBAR.resolve(cx))
                    .child(
                        div()
                            .font_family(FONT_UI)
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(TEXT.resolve(cx))
                            .child("Send a request to view response"),
                    )
                    .child(
                        div()
                            .font_family(FONT_UI)
                            .text_size(px(11.0))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(SUBTEXT.resolve(cx))
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
                    .bg(CODE_BG.resolve(cx))
                    .font_family(FONT_MONO)
                    .text_size(px(13.0))
                    .text_color(CODE_TEXT.resolve(cx))
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
                        .min_w_0()
                        .when(empty_body, |d| {
                            d.child(
                                div()
                                    .debug_selector(|| "response-content".into())
                                    .child(div().debug_selector(|| "response-empty-body".into()))
                                    .p_4()
                                    .text_size(m::BODY)
                                    .text_color(MUTED.resolve(cx))
                                    .child("Empty response body"),
                            )
                        })
                        .when(!empty_body, |d| {
                            d.child(self.render_selectable_content(window, cx))
                        }),
                    ResponsePane::Headers => div()
                        .flex_1()
                        .min_h_0()
                        .child(render_response_headers(&headers, &self.header_scroll, cx)),
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
                                    .bg(INFO_SOFT.resolve(cx))
                                    .font_family(FONT_UI)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_size(px(11.0))
                                    .text_color(INFO.resolve(cx))
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
                    ResponsePane::Headers => div().flex_1().min_h_0().child(
                        render_response_headers(&response.headers, &self.header_scroll, cx),
                    ),
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
            .child(
                div()
                    .debug_selector(|| "response-footer".into())
                    .h(rems(28. / 16.))
                    .flex_none()
                    .px(rems(22. / 16.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_t_1()
                    .border_color(LINE.resolve(cx))
                    .font_family(FONT_MONO)
                    .text_size(m::CAPTION)
                    .text_color(MUTED.resolve(cx))
                    .child(div().min_w_0().overflow_hidden().text_ellipsis().child(
                        media_type.map_or_else(|| "UTF-8".into(), |t| format!("UTF-8 · {t}")),
                    ))
                    .child("Read only"),
            )
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

fn render_redirect_chain(chain: &[RedirectHop], partial: bool, cx: &gpui::App) -> impl IntoElement {
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
        .bg(PANEL_ALT.resolve(cx))
        .border_b_1()
        .border_color(LINE.resolve(cx))
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
                .border_color(LINE.resolve(cx))
                .font_family(FONT_UI)
                .font_weight(FontWeight::BOLD)
                .text_size(px(10.0))
                .text_color((if partial { ERROR } else { INFO }).resolve(cx))
                .when(partial, |header| {
                    header.child(
                        div()
                            .debug_selector(|| "redirect-chain-partial".into())
                            .child("incomplete"),
                    )
                })
                .child(
                    div()
                        .debug_selector(|| "response-redirect-count".into())
                        .child(if partial {
                            format!("Partial redirect chain · {redirect_count} observed")
                        } else {
                            format!("Redirect chain · {redirect_count} observed")
                        }),
                ),
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
                .border_color(LINE.resolve(cx))
                .text_size(px(10.0))
                .child(
                    div()
                        .debug_selector(move || status_selector.clone())
                        .w(px(36.0))
                        .flex_none()
                        .font_weight(FontWeight::BOLD)
                        .text_color((if is_terminal { OK } else { INFO }).resolve(cx))
                        .child(hop.status.to_string()),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .text_color(TEXT.resolve(cx))
                        .child(hop.url.clone()),
                )
                .child(match &hop.location {
                    Some(location) => div()
                        .debug_selector(move || location_selector.clone())
                        .w(px(280.0))
                        .flex_none()
                        .text_color(SUBTEXT.resolve(cx))
                        .child(format!("Location: {location}")),
                    None => div()
                        .w(px(280.0))
                        .flex_none()
                        .text_color(OK.resolve(cx))
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
        models::{HistoricalResponse, HistoricalResponseBody},
        ui::text_editor::TextRange,
        utils::formatter::format_response_body,
    };
    use gpui::{
        point, px, AppContext, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
        MouseUpEvent, ScrollDelta, ScrollWheelEvent, TestAppContext,
    };
    use postman_http::HttpResponse;

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
            assert!(workspace.complete_send(pending, Ok(HttpResponse::success(body))));
            workspace
        });
        cx.update(crate::ui::kit::init);
        let viewer = cx.new(|cx| ResponseViewer::new(workspace, cx));
        let content = viewer.clone();
        let (_, visual) = cx
            .add_window_view(move |window, cx| gpui_kit::component::Root::new(content, window, cx));
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
            viewer.read_with(visual, |viewer, cx| viewer
                .text_layout
                .as_ref()
                .unwrap()
                .selection_quads(
                    viewer.selection.text(),
                    viewer.selection.selected_range(),
                    crate::ui::theme::ACCENT_SOFT.resolve(cx)
                )
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
            assert!(workspace.complete_send(pending, Ok(HttpResponse::success(body))));
            workspace
        });
        cx.update(crate::ui::kit::init);
        let viewer = cx.new(|cx| ResponseViewer::new(workspace, cx));
        let content = viewer.clone();
        let (_, visual) = cx
            .add_window_view(move |window, cx| gpui_kit::component::Root::new(content, window, cx));
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
        assert!(!viewer.read_with(visual, |viewer, cx| viewer
            .text_layout
            .as_ref()
            .unwrap()
            .selection_quads(
                viewer.selection.text(),
                viewer.selection.selected_range(),
                crate::ui::theme::ACCENT_SOFT.resolve(cx)
            )
            .is_empty()));
    }
}

use crate::app::RequestTabId;
use crate::ui::{
    components::kit_controls,
    theme::{metrics as m, ERROR_SOFT, SIDEBAR},
};
use gpui::{
    actions, div, prelude::FluentBuilder, px, rems, App, AppContext, ClipboardItem, Context,
    Entity, EventEmitter, FocusHandle, Focusable, FontWeight, InteractiveElement, IntoElement,
    KeyBinding, ParentElement, Pixels, Render, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Window,
};
use gpui_kit::base::{ElementExt, TestSupportExt};
use gpui_kit::{
    assets::IconName,
    base::Tab,
    component::{
        input::{Copy, SelectAll},
        notification::Notification,
        Icon, Sizable, WindowExt,
    },
};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::Duration,
};

mod body;
mod headers;
use body::ResponseBody;

use headers::render_response_headers;

use crate::{
    app::{CookieJarEntry, ResponseState, WorkspaceViewModel},
    models::{HistoricalResponseBody, RedirectHop},
    ui::theme::{
        ACCENT, ACCENT_SOFT, CODE_BG, CODE_TEXT, ERROR, FONT_HEADING, FONT_MONO, FONT_UI, INFO,
        INFO_SOFT, LINE, MUTED, OK, OK_SOFT, PANEL, PANEL_ALT, SUBTEXT, TEXT,
    },
};

const COPIED_FEEDBACK_DURATION: Duration = Duration::from_secs(2);

actions!(
    response_viewer,
    [
        CopyResponseBody,
        ActivateResponsePaneTab,
        FocusNextResponsePaneTab,
        FocusPreviousResponsePaneTab,
        ActivateNextResponsePane,
        ActivatePreviousResponsePane
    ]
);

pub fn setup_response_viewer_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-c", Copy, Some("ResponseContent > Input")),
        KeyBinding::new("ctrl-c", Copy, Some("ResponseContent > Input")),
        KeyBinding::new("cmd-a", SelectAll, Some("ResponseContent > Input")),
        KeyBinding::new("ctrl-a", SelectAll, Some("ResponseContent > Input")),
        KeyBinding::new(
            "tab",
            crate::app::FocusNextControl,
            Some("ResponseContent > Input"),
        ),
        KeyBinding::new(
            "shift-tab",
            crate::app::FocusPreviousControl,
            Some("ResponseContent > Input"),
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
    response: Arc<ResponseState>,
    cookie_revision: u64,
    pane: ResponsePane,
    pretty: bool,
    stacked: bool,
    width: Pixels,
    active_tab: Option<RequestTabId>,
    retained: HashMap<RequestTabId, ResponsePresentation>,
    body: Entity<ResponseBody>,
    header_scroll: gpui::ScrollHandle,
    body_tab_focus_handle: FocusHandle,
    headers_tab_focus_handle: FocusHandle,
    cookies_tab_focus_handle: FocusHandle,
    copy_focus_handle: FocusHandle,
    copied_feedback: bool,
    copy_generation: u64,
    _view_model_subscription: Subscription,
}

struct ResponsePresentation {
    pane: ResponsePane,
    pretty: bool,
    body: Entity<ResponseBody>,
    header_scroll: gpui::ScrollHandle,
}

impl EventEmitter<ResponseViewerEvent> for ResponseViewer {}

impl Focusable for ResponseViewer {
    fn focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.body.read(cx).focus_handle(cx)
    }
}

impl ResponseViewer {
    pub fn new(
        view_model: Entity<WorkspaceViewModel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let view_model_subscription = cx.observe_in(&view_model, window, |this, _, window, cx| {
            this.sync_presentation(window, cx);
        });
        let active_tab = view_model.read(cx).active_tab_id();
        let response = view_model
            .read(cx)
            .active_request()
            .map(|request| request.response_snapshot())
            .unwrap_or_else(|| Arc::new(ResponseState::NotSent));
        let cookie_revision = view_model.read(cx).cookie_revision();
        let viewer = Self {
            view_model,
            response,
            cookie_revision,
            pane: ResponsePane::Body,
            pretty: true,
            stacked: false,
            width: px(600.),
            active_tab,
            retained: HashMap::new(),
            body: cx.new(|cx| ResponseBody::new(window, cx)),
            header_scroll: gpui::ScrollHandle::new(),
            body_tab_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            headers_tab_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            cookies_tab_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            copy_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            copied_feedback: false,
            copy_generation: 0,
            _view_model_subscription: view_model_subscription,
        };
        viewer.project_body(window, cx);
        viewer
    }

    pub(super) fn set_stacked(&mut self, stacked: bool, cx: &mut Context<Self>) {
        if self.stacked != stacked {
            self.stacked = stacked;
            cx.notify();
        }
    }

    fn retain_active_presentation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let model = self.view_model.read(cx);
        let active = model.active_tab_id();
        self.retained
            .retain(|id, _| model.tabs().iter().any(|tab| tab.tab_id() == *id));
        if active == self.active_tab {
            return;
        }
        if let Some(previous) = self
            .active_tab
            .filter(|id| model.tabs().iter().any(|tab| tab.tab_id() == *id))
        {
            self.retained.insert(
                previous,
                ResponsePresentation {
                    pane: self.pane,
                    pretty: self.pretty,
                    body: self.body.clone(),
                    header_scroll: std::mem::take(&mut self.header_scroll),
                },
            );
        }
        if let Some(saved) = active.and_then(|id| self.retained.remove(&id)) {
            self.pane = saved.pane;
            self.pretty = saved.pretty;
            self.body = saved.body;
            self.header_scroll = saved.header_scroll;
        } else {
            self.pane = ResponsePane::Body;
            self.pretty = true;
            self.body = cx.new(|cx| ResponseBody::new(window, cx));
            self.header_scroll = gpui::ScrollHandle::new();
        }
        self.active_tab = active;
    }

    fn sync_presentation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.retain_active_presentation(window, cx);
        let model = self.view_model.read(cx);
        let response = model
            .active_request()
            .map(|request| request.response_snapshot());
        let response_changed = response
            .as_ref()
            .is_some_and(|response| !Arc::ptr_eq(response, &self.response))
            || (response.is_none() && !matches!(self.response.as_ref(), ResponseState::NotSent));
        let cookie_revision = model.cookie_revision();
        let cookies_changed = self.cookie_revision != cookie_revision;
        self.cookie_revision = cookie_revision;
        if response_changed {
            self.response = response.unwrap_or_else(|| Arc::new(ResponseState::NotSent));
            if self.pane == ResponsePane::Cookies
                && matches!(self.response.as_ref(), ResponseState::Historical { .. })
            {
                self.pane = ResponsePane::Body;
            }
            self.project_body(window, cx);
            self.copied_feedback = false;
            self.copy_generation = self.copy_generation.wrapping_add(1);
        }
        // URL/Body edits, History reloads and other tabs' sends do not invalidate this document.
        if response_changed || cookies_changed {
            cx.notify();
        }
    }

    fn project_body(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.body.update(cx, |body, cx| {
            body.project(self.response.clone(), self.pretty, window, cx)
        });
    }

    fn set_pretty(&mut self, pretty: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.pretty != pretty {
            self.pretty = pretty;
            self.project_body(window, cx);
            cx.notify();
        }
    }

    fn raw_response_body(&self, cx: &App) -> Option<String> {
        match self.view_model.read(cx).active_request()?.response() {
            ResponseState::Success {
                body,
                is_binary: false,
                ..
            } if !body.is_empty() => Some(body.clone()),
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

    fn save_response_body(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ResponseState::Success {
            body_bytes,
            is_binary,
            ..
        } = self.response.as_ref()
        else {
            return;
        };
        // Capture this response before opening the picker: switching tabs or sending again
        // must not change the bytes written by an already-started save.
        let bytes = body_bytes.clone();
        let directory = directories::UserDirs::new()
            .map(|dirs| dirs.download_dir().unwrap_or(dirs.home_dir()).to_path_buf())
            .unwrap_or_default();
        let selection = cx.prompt_for_new_path(
            &directory,
            Some(if *is_binary {
                "response.bin"
            } else {
                "response.txt"
            }),
        );
        cx.spawn_in(window, async move |_, cx| {
            let result = match selection.await {
                Ok(Ok(Some(path))) => {
                    cx.background_executor()
                        .spawn(async move {
                            std::fs::write(&path, &bytes).map_err(|error| {
                                format!("Couldn’t save {}: {error}", path.display())
                            })
                        })
                        .await
                }
                Ok(Ok(None)) => return,
                Ok(Err(error)) => Err(format!("Couldn’t open the save dialog: {error}")),
                Err(_) => return,
            };
            let _ = cx.update(|window, cx| {
                window.push_notification(
                    match result {
                        Ok(()) => Notification::success("Response saved"),
                        Err(message) => Notification::error(message),
                    },
                    cx,
                );
            });
        })
        .detach();
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
        if self.pane == pane {
            return;
        }
        self.pane = pane;
        self.body.update(cx, |body, cx| body.clear_selection(cx));
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

    fn render_cookie_content(
        &self,
        cookies: Vec<ResponseCookieEvidence>,
        jar_count: usize,
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
                        kit_controls::editor_button("response-open-cookie-jar", "", cx)
                            .debug_selector(|| "response-open-cookie-jar".into())
                            .key_context("OverlayTrigger")
                            .accessibility_label("Open Cookie Jar")
                            .h_8()
                            .gap_2()
                            .border_1()
                            .border_color(INFO.resolve(cx))
                            .bg(PANEL.resolve(cx))
                            .text_size(m::CAPTION)
                            .text_color(INFO.resolve(cx))
                            .child("↗")
                            .child("Open Cookie Jar")
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.emit(ResponseViewerEvent::OpenCookieJar)
                            })),
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

impl Render for ResponseViewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.response.clone();
        let state = snapshot.as_ref();
        let (redirect_chain, response_cookies, jar_count) = {
            let view_model = self.view_model.read(cx);
            let active = view_model.active_request();
            (
                active
                    .map(|request| request.redirect_chain().to_vec())
                    .unwrap_or_default(),
                response_cookie_evidence(view_model),
                view_model.cookie_count(),
            )
        };
        let pane = self.pane;
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
            ResponseState::Success {
                body, is_binary, ..
            } => !is_binary && !body.is_empty(),
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
                body_bytes,
                elapsed_ms,
                ..
            } => (
                status_label(*status),
                format!("{elapsed_ms} ms"),
                format_bytes(body_bytes.len()),
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
        let empty_body =
            matches!(&state, ResponseState::Success { body_bytes, .. } if body_bytes.is_empty());
        let binary_body = matches!(
            &state,
            ResponseState::Success {
                is_binary: true,
                ..
            }
        );
        // The editor contains Unicode while Save retains the original response bytes.
        // Describe the content without claiming that those bytes are always UTF-8.
        let body_metadata = {
            let format = if binary_body { "Binary" } else { "Text" };
            media_type
                .as_deref()
                .map_or_else(|| format.to_owned(), |t| format!("{format} · {t}"))
        };
        let can_save_body =
            matches!(&state, ResponseState::Success { body_bytes, .. } if !body_bytes.is_empty());
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
            .overflow_hidden()
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
                                                    .on_click(cx.listener(
                                                        move |this, _, window, cx| {
                                                            this.set_pretty(pretty, window, cx)
                                                        },
                                                    ))
                                            },
                                        ),
                                    ),
                            )
                        })
                        .child(div().flex_1())
                        .when(pane == ResponsePane::Body && can_save_body, |d| {
                            d.child(gpui_kit::component::button::Button::new("response-save")
                                .label("Save as…")
                                .small()
                                .debug_selector(|| "response-save".into())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.save_response_body(window, cx)
                                })))
                        })
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
                    .child(self.body.clone()),
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
                        .when(binary_body, |d| {
                            d.child(div()
                                .debug_selector(|| "response-binary-body".into())
                                .p_4()
                                .text_size(m::BODY)
                                .text_color(MUTED.resolve(cx))
                                .child("Binary content. Save the response to view it in another application."))
                        })
                        .when(!empty_body && !binary_body, |d| d.child(self.body.clone())),
                    ResponsePane::Headers => div()
                        .flex_1()
                        .min_h_0()
                        .child(render_response_headers(headers, &self.header_scroll, cx)),
                    ResponsePane::Cookies => div()
                        .flex_1()
                        .min_h_0()
                        .child(self.render_cookie_content(response_cookies, jar_count, cx)),
                },
                ResponseState::Historical { response, .. } => match pane {
                    ResponsePane::Body => match &response.body {
                        HistoricalResponseBody::Empty => div()
                            .debug_selector(|| "response-historical-empty".into())
                            .flex_1()
                            .min_h_0()
                            .child(self.body.clone()),
                        HistoricalResponseBody::Text(_body) => {
                            div().flex_1().min_h_0().child(self.body.clone())
                        }
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
                            .child(div().flex_1().min_h_0().child(self.body.clone())),
                        HistoricalResponseBody::Unsupported => div()
                            .debug_selector(|| "response-historical-body-not-stored".into())
                            .flex_1()
                            .min_h_0()
                            .child(self.body.clone()),
                    },
                    ResponsePane::Headers => div().flex_1().min_h_0().child(
                        render_response_headers(&response.headers, &self.header_scroll, cx),
                    ),
                    ResponsePane::Cookies => div().flex_1().min_h_0().child(self.body.clone()),
                },
                ResponseState::HistoricalUnavailable { .. } => div()
                    .debug_selector(|| "response-historical-unavailable".into())
                    .flex_1()
                    .min_h_0()
                    .child(self.body.clone()),
                ResponseState::Error { message: _ } => div()
                    .when(is_timeout, |content| {
                        content.debug_selector(|| "response-timeout-content".into())
                    })
                    .flex_1()
                    .min_h_0()
                    .child(self.body.clone()),
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
                    .child(div()
                        .id("response-body-metadata")
                        .role(gpui::Role::Status)
                        .test_support()
                        .aria_label(body_metadata.clone())
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(body_metadata))
                    .child("Read only"),
            )
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
mod tests;

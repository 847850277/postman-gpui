use super::composer::RequestComposer;
use crate::{
    app::{ActivateControl, AuthorizationKind, RequestPane},
    ui::theme::{
        ACCENT, ACCENT_INK, ACCENT_SOFT, ACCENT_VIVID, ERROR, FONT_HEADING, FONT_UI, INFO,
        INFO_SOFT, LINE, MUTED, PANEL, PANEL_ALT, TEXT,
    },
};
use gpui::{
    actions, div, prelude::FluentBuilder, px, rgb, Context, FontWeight, InteractiveElement,
    IntoElement, KeyBinding, ParentElement, Role, StatefulInteractiveElement, Styled, Window,
};

actions!(request_pane_tabs, [NextRequestPane, PreviousRequestPane]);

pub(super) fn setup_request_pane_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("right", NextRequestPane, Some("RequestPaneTab")),
        KeyBinding::new("down", NextRequestPane, Some("RequestPaneTab")),
        KeyBinding::new("left", PreviousRequestPane, Some("RequestPaneTab")),
        KeyBinding::new("up", PreviousRequestPane, Some("RequestPaneTab")),
    ]
}

impl RequestComposer {
    pub(super) fn request_tab(
        &self,
        pane: RequestPane,
        label: impl Into<String>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self
            .view_model
            .read(cx)
            .active_request()
            .is_some_and(|request| request.request_pane() == pane);
        let selector = request_pane_selector(pane);
        let label = label.into();
        let accessible_label = format!("{label} request pane");
        let focus_handle = self.request_pane_focus_handles[request_pane_index(pane)].clone();
        let mouse_focus_handle = focus_handle.clone();
        let focused = focus_handle.is_focused(window);
        div()
            .id(selector)
            .debug_selector(move || selector.into())
            .track_focus(&focus_handle)
            .key_context("KeyboardButton RequestPaneTab")
            .role(Role::Tab)
            .aria_label(accessible_label)
            .aria_selected(active)
            .h_full()
            .flex()
            .items_center()
            .px_2()
            .cursor_pointer()
            .font_family(FONT_UI)
            .text_size(px(13.0))
            .font_weight(if active {
                FontWeight::BOLD
            } else {
                FontWeight::SEMIBOLD
            })
            .text_color(rgb(if active { TEXT } else { MUTED }))
            .hover(|style| style.text_color(rgb(TEXT)))
            .when(focused, |tab| {
                tab.bg(rgb(ACCENT_SOFT))
                    .border_1()
                    .border_color(rgb(ACCENT))
            })
            .child(label)
            .on_action(
                cx.listener(move |this, _: &ActivateControl, _, cx| {
                    this.set_request_pane(pane, cx)
                }),
            )
            .on_action(cx.listener(move |this, _: &NextRequestPane, window, cx| {
                this.activate_relative_request_pane(pane, 1, window, cx)
            }))
            .on_action(
                cx.listener(move |this, _: &PreviousRequestPane, window, cx| {
                    this.activate_relative_request_pane(pane, -1, window, cx)
                }),
            )
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    mouse_focus_handle.focus(window, cx);
                    this.set_request_pane(pane, cx);
                }),
            )
    }

    fn activate_relative_request_pane(
        &mut self,
        pane: RequestPane,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = (request_pane_index(pane) as isize + delta).rem_euclid(7) as usize;
        let pane = REQUEST_PANES[next];
        self.request_pane_focus_handles[next].focus(window, cx);
        self.set_request_pane(pane, cx);
    }

    pub(super) fn render_request_head(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (is_sending, url_query_count, request_id, in_flight_count) = {
            let view_model = self.view_model.read(cx);
            let active = view_model.active_request();
            (
                active.is_some_and(|request| request.is_sending()),
                active.map_or(0, |request| request.url_query_parameter_count()),
                view_model.active_request_id(),
                view_model.in_flight_count(),
            )
        };
        div()
            .debug_selector(|| "request-head".into())
            .h(px(46.0))
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .child(self.method_selector.clone())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(self.url_input.clone())
                    .when(url_query_count > 0, |url| {
                        url.child(
                            div()
                                .debug_selector(|| "url-query-count".into())
                                .h(px(28.0))
                                .px_2()
                                .flex_none()
                                .flex()
                                .items_center()
                                .rounded_lg()
                                .bg(rgb(INFO_SOFT))
                                .font_family(FONT_UI)
                                .font_weight(FontWeight::BOLD)
                                .text_size(px(11.0))
                                .text_color(rgb(INFO))
                                .child(format!("{url_query_count} in URL")),
                        )
                    }),
            )
            .when_some(request_id, |head, request_id| {
                head.child(
                    div()
                        .debug_selector(|| "request-in-flight-id".into())
                        .h(px(28.0))
                        .px_2()
                        .flex_none()
                        .flex()
                        .items_center()
                        .rounded_lg()
                        .bg(rgb(INFO_SOFT))
                        .font_family(FONT_UI)
                        .font_weight(FontWeight::BOLD)
                        .text_size(px(10.0))
                        .text_color(rgb(INFO))
                        .child(format!("{request_id} · in_flight={in_flight_count}")),
                )
            })
            .child(
                div()
                    .id("send-button")
                    .debug_selector(|| "send-button".into())
                    .track_focus(&self.send_focus_handle)
                    .key_context("KeyboardButton")
                    .role(Role::Button)
                    .aria_label(if is_sending {
                        "Cancel active request"
                    } else {
                        "Send active request"
                    })
                    .w(px(110.0))
                    .h_full()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_lg()
                    .bg(rgb(if is_sending { ERROR } else { ACCENT_VIVID }))
                    .text_color(rgb(if is_sending { PANEL } else { ACCENT_INK }))
                    .font_family(FONT_HEADING)
                    .text_size(px(15.0))
                    .font_weight(FontWeight::BOLD)
                    .cursor_pointer()
                    .hover(move |style| {
                        if is_sending {
                            style.bg(rgb(0x00a8_2f2f))
                        } else {
                            style.bg(rgb(ACCENT)).text_color(rgb(PANEL))
                        }
                    })
                    .when(self.send_focus_handle.is_focused(window), |button| {
                        button.border_2().border_color(rgb(INFO))
                    })
                    .child(
                        div()
                            .when(is_sending, |label| {
                                label.debug_selector(|| "cancel-send-control".into())
                            })
                            .child(if is_sending { "Cancel" } else { "Send" }),
                    )
                    .on_action(
                        cx.listener(|this, _: &ActivateControl, _window, cx| this.click_send(cx)),
                    )
                    .on_mouse_up(gpui::MouseButton::Left, cx.listener(Self::on_send_clicked)),
            )
    }

    pub(super) fn render_request_menu(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (header_count, authorization_kind, has_authorization, has_body, has_script, has_tests) = {
            let view_model = self.view_model.read(cx);
            view_model.active_request().map_or(
                (0, AuthorizationKind::Bearer, false, false, false, false),
                |request| {
                    (
                        request.headers().iter().filter(|row| row.enabled).count(),
                        request.authorization_kind(),
                        match request.authorization_kind() {
                            AuthorizationKind::Bearer => !request.bearer_token().is_empty(),
                            AuthorizationKind::Basic => {
                                !request.basic_username().is_empty()
                                    || !request.basic_password().is_empty()
                            }
                        },
                        !request.request_body().is_empty(),
                        !request.pre_request_script().is_empty(),
                        !request.tests_script().is_empty(),
                    )
                },
            )
        };
        div()
            .h(px(44.0))
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .bg(rgb(PANEL_ALT))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(self.request_tab(RequestPane::Params, "Params", window, cx))
            .child(self.request_tab(
                RequestPane::Authorization,
                format!(
                    "Authorization ({}){}",
                    match authorization_kind {
                        AuthorizationKind::Bearer => "Bearer",
                        AuthorizationKind::Basic => "Basic",
                    },
                    if has_authorization { " ●" } else { "" }
                ),
                window,
                cx,
            ))
            .child(self.request_tab(
                RequestPane::Headers,
                format!("Headers ({header_count})"),
                window,
                cx,
            ))
            .child(self.request_tab(
                RequestPane::Body,
                if has_body { "Body ●" } else { "Body" },
                window,
                cx,
            ))
            .child(self.request_tab(
                RequestPane::Scripts,
                if has_script { "Scripts ●" } else { "Scripts" },
                window,
                cx,
            ))
            .child(self.request_tab(
                RequestPane::Tests,
                if has_tests { "Tests ●" } else { "Tests" },
                window,
                cx,
            ))
            .child(self.request_tab(RequestPane::Options, "Options", window, cx))
    }
}

const REQUEST_PANES: [RequestPane; 7] = [
    RequestPane::Params,
    RequestPane::Authorization,
    RequestPane::Headers,
    RequestPane::Body,
    RequestPane::Scripts,
    RequestPane::Tests,
    RequestPane::Options,
];

fn request_pane_index(pane: RequestPane) -> usize {
    REQUEST_PANES
        .iter()
        .position(|candidate| *candidate == pane)
        .expect("all request panes are represented in keyboard order")
}
fn request_pane_selector(pane: RequestPane) -> &'static str {
    match pane {
        RequestPane::Params => "request-pane-params",
        RequestPane::Authorization => "request-pane-authorization",
        RequestPane::Headers => "request-pane-headers",
        RequestPane::Body => "request-pane-body",
        RequestPane::Scripts => "request-pane-scripts",
        RequestPane::Tests => "request-pane-tests",
        RequestPane::Options => "request-pane-options",
    }
}

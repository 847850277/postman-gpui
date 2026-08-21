use super::composer::RequestComposer;
use crate::{
    app::{AuthorizationKind, RequestPane},
    ui::theme::{
        ACCENT, ACCENT_INK, ACCENT_VIVID, ERROR, FONT_HEADING, FONT_UI, INFO, INFO_SOFT, LINE,
        MUTED, PANEL, PANEL_ALT, TEXT,
    },
};
use gpui::{
    div, prelude::FluentBuilder, px, rgb, Context, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Styled,
};

impl RequestComposer {
    pub(super) fn request_tab(
        &self,
        pane: RequestPane,
        label: impl Into<String>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.view_model.read(cx).request_pane() == pane;
        let selector = request_pane_selector(pane);
        div()
            .debug_selector(move || selector.into())
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
            .child(label.into())
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, _, cx| this.set_request_pane(pane, cx)),
            )
    }

    pub(super) fn render_request_head(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (is_sending, url_query_count, request_id, in_flight_count) = {
            let view_model = self.view_model.read(cx);
            (
                view_model.is_sending(),
                view_model.url_query_parameter_count(),
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
                    .debug_selector(|| "send-button".into())
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
                    .child(
                        div()
                            .when(is_sending, |label| {
                                label.debug_selector(|| "cancel-send-control".into())
                            })
                            .child(if is_sending { "Cancel" } else { "Send" }),
                    )
                    .on_mouse_up(gpui::MouseButton::Left, cx.listener(Self::on_send_clicked)),
            )
    }

    pub(super) fn render_request_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (header_count, authorization_kind, has_authorization, has_body, has_script, has_tests) = {
            let view_model = self.view_model.read(cx);
            (
                view_model
                    .headers()
                    .iter()
                    .filter(|row| row.enabled)
                    .count(),
                view_model.authorization_kind(),
                match view_model.authorization_kind() {
                    AuthorizationKind::Bearer => !view_model.bearer_token().is_empty(),
                    AuthorizationKind::Basic => {
                        !view_model.basic_username().is_empty()
                            || !view_model.basic_password().is_empty()
                    }
                },
                !view_model.request_body().is_empty(),
                !view_model.pre_request_script().is_empty(),
                !view_model.tests_script().is_empty(),
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
            .child(self.request_tab(RequestPane::Params, "Params", cx))
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
                cx,
            ))
            .child(self.request_tab(
                RequestPane::Headers,
                format!("Headers ({header_count})"),
                cx,
            ))
            .child(self.request_tab(
                RequestPane::Body,
                if has_body { "Body ●" } else { "Body" },
                cx,
            ))
            .child(self.request_tab(
                RequestPane::Scripts,
                if has_script { "Scripts ●" } else { "Scripts" },
                cx,
            ))
            .child(self.request_tab(
                RequestPane::Tests,
                if has_tests { "Tests ●" } else { "Tests" },
                cx,
            ))
            .child(self.request_tab(RequestPane::Options, "Options", cx))
    }
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

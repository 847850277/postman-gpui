use super::*;

impl PostmanApp {
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

    pub(super) fn render_top_header(&self) -> impl IntoElement {
        div()
            .debug_selector(|| "top-header".into())
            .h(px(72.0))
            .flex_none()
            .flex()
            .items_center()
            .px_5()
            .bg(rgb(PANEL))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().size(px(20.0)).rounded_full().bg(rgb(ACCENT)))
                    .child(
                        div()
                            .font_family(FONT_HEADING)
                            .text_size(px(22.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(TEXT))
                            .child("Postman GPUI"),
                    ),
            )
    }

    pub(super) fn render_left_rail(&self) -> impl IntoElement {
        let slots = ["⌂", "↻", "◫", "◇", "⌘", "⚙", "?"];
        div()
            .debug_selector(|| "left-rail".into())
            .w(px(72.0))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap_4()
            .px_2()
            .py_3()
            .bg(rgb(PANEL))
            .border_r_1()
            .border_color(rgb(LINE))
            .children(slots.into_iter().enumerate().map(|(index, label)| {
                div()
                    .id(("rail-slot", index))
                    .size(px(40.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_lg()
                    .bg(rgb(if index == 1 { ACCENT_SOFT } else { PANEL_ALT }))
                    .text_color(rgb(if index == 1 { ACCENT_DARK } else { SUBTEXT }))
                    .font_family(FONT_UI)
                    .text_size(px(16.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(label)
            }))
    }

    pub(super) fn render_request_tabs_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs: Vec<_> = {
            let view_model = self.view_model.read(cx);
            let active_index = view_model.active_tab_index();
            view_model
                .tabs()
                .iter()
                .enumerate()
                .map(|(index, request)| {
                    (
                        index,
                        request.method(),
                        request.tab_title(),
                        request.is_dirty(),
                        index == active_index,
                    )
                })
                .collect()
        };

        div()
            .debug_selector(|| "request-tabs-bar".into())
            .h(px(54.0))
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .bg(rgb(PANEL))
            .border_b_1()
            .border_color(rgb(LINE))
            .children(
                tabs.into_iter()
                    .map(|(index, method, title, dirty, active)| {
                        div()
                            .debug_selector(move || format!("request-tab-{index}"))
                            .h_full()
                            .max_w(px(280.0))
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .bg(rgb(if active { PANEL } else { PANEL_ALT }))
                            .rounded_t_lg()
                            .font_family(FONT_UI)
                            .text_size(px(12.0))
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(PANEL)))
                            .child(
                                div()
                                    .text_color(rgb(method_color(method)))
                                    .font_weight(FontWeight::BOLD)
                                    .child(method.to_string()),
                            )
                            .child(
                                div()
                                    .max_w(px(180.0))
                                    .overflow_hidden()
                                    .text_color(rgb(if active { SUBTEXT } else { MUTED }))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .when(dirty, |tab| {
                                tab.child(div().size(px(6.0)).rounded_full().bg(rgb(ACCENT)))
                            })
                            .child(
                                div()
                                    .debug_selector(move || format!("close-tab-{index}"))
                                    .size(px(20.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .text_color(rgb(MUTED))
                                    .hover(|style| {
                                        style.bg(rgb(ACCENT_SOFT)).text_color(rgb(ACCENT_DARK))
                                    })
                                    .child("×")
                                    .on_mouse_up(
                                        gpui::MouseButton::Left,
                                        cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.close_request_tab(index, cx);
                                        }),
                                    ),
                            )
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    this.select_request_tab(index, cx)
                                }),
                            )
                    }),
            )
            .child(
                div()
                    .debug_selector(|| "new-tab-button".into())
                    .size(px(32.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_lg()
                    .bg(rgb(PANEL_ALT))
                    .text_color(rgb(SUBTEXT))
                    .font_family(FONT_HEADING)
                    .text_size(px(20.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(ACCENT_SOFT)).text_color(rgb(ACCENT_DARK)))
                    .child("+")
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.new_request(cx)),
                    ),
            )
    }

    pub(super) fn render_request_head(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_sending = self.view_model.read(cx).is_sending();
        div()
            .debug_selector(|| "request-head".into())
            .h(px(46.0))
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .child(self.method_selector.clone())
            .child(self.url_input.clone())
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
                    .bg(rgb(if is_sending { ERROR } else { ACCENT }))
                    .text_color(rgb(PANEL))
                    .font_family(FONT_HEADING)
                    .text_size(px(15.0))
                    .font_weight(FontWeight::BOLD)
                    .cursor_pointer()
                    .hover(move |style| {
                        style.bg(rgb(if is_sending { 0x00b9_1c1c } else { ACCENT_DARK }))
                    })
                    .child(if is_sending { "Cancel" } else { "Send" })
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
                if has_body {
                    "Body ●".to_string()
                } else {
                    "Body".to_string()
                },
                cx,
            ))
            .child(self.request_tab(
                RequestPane::Scripts,
                if has_script {
                    "Scripts ●".to_string()
                } else {
                    "Scripts".to_string()
                },
                cx,
            ))
            .child(self.request_tab(
                RequestPane::Tests,
                if has_tests {
                    "Tests ●".to_string()
                } else {
                    "Tests".to_string()
                },
                cx,
            ))
    }
}

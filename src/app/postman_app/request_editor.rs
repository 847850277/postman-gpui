use super::*;

impl PostmanApp {
    pub(super) fn render_request_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let request_pane = self.view_model.read(cx).request_pane();
        let editor = match request_pane {
            RequestPane::Params => {
                let rows = self.view_model.read(cx).params().to_vec();
                self.render_key_value_editor(
                    "Query parameters",
                    rows,
                    Self::toggle_param,
                    Self::remove_param,
                    cx,
                )
            }
            RequestPane::Authorization => self.render_authorization_editor(cx),
            RequestPane::Headers => {
                let rows = self.view_model.read(cx).headers().to_vec();
                self.render_key_value_editor(
                    "Request headers",
                    rows,
                    Self::toggle_header,
                    Self::remove_header,
                    cx,
                )
            }
            RequestPane::Body => self.render_body_editor(cx),
            RequestPane::Scripts => self.render_script_editor(
                "Pre-request script",
                "Saved with this request tab.",
                self.script_input.clone(),
                "script-editor",
            ),
            RequestPane::Tests => self.render_script_editor(
                "Response tests",
                "Saved with this request tab for the test runner.",
                self.tests_input.clone(),
                "tests-editor",
            ),
        };

        div()
            .debug_selector(|| "request-panel".into())
            .h(px(360.0))
            .flex_none()
            .flex()
            .flex_col()
            .min_w_0()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(LINE))
            .rounded(px(14.0))
            .overflow_hidden()
            .child(self.render_request_menu(cx))
            .child(editor)
    }

    pub(super) fn render_authorization_editor(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let authorization_kind = self.view_model.read(cx).authorization_kind();
        let editor = match authorization_kind {
            AuthorizationKind::Bearer => div()
                .h(px(48.0))
                .flex()
                .items_center()
                .gap_4()
                .px_3()
                .rounded_lg()
                .bg(rgb(CODE_PANEL))
                .child(
                    div()
                        .w(px(120.0))
                        .flex_none()
                        .font_family(FONT_UI)
                        .font_weight(FontWeight::BOLD)
                        .text_size(px(12.0))
                        .text_color(rgb(0x0093_c5fd))
                        .child("Bearer token"),
                )
                .child(
                    div()
                        .debug_selector(|| "authorization-input".into())
                        .h(px(34.0))
                        .flex_1()
                        .child(self.authorization_input.clone()),
                )
                .into_any_element(),
            AuthorizationKind::Basic => div()
                .flex()
                .flex_col()
                .gap_2()
                .child(self.render_basic_auth_field(
                    "Username",
                    "basic-auth-username-input",
                    self.basic_username_input.clone(),
                ))
                .child(self.render_basic_auth_field(
                    "Password",
                    "basic-auth-password-input",
                    self.basic_password_input.clone(),
                ))
                .into_any_element(),
        };
        let hint = match authorization_kind {
            AuthorizationKind::Bearer => "The request will include Authorization: Bearer <token>.",
            AuthorizationKind::Basic => {
                "The request will Base64-encode username:password in Authorization: Basic."
            }
        };

        div()
            .flex_1()
            .min_h_0()
            .p_4()
            .bg(rgb(CODE_BG))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .mb_3()
                    .child(self.render_authorization_kind_button(
                        AuthorizationKind::Bearer,
                        "Bearer Token",
                        "auth-kind-bearer",
                        authorization_kind == AuthorizationKind::Bearer,
                        cx,
                    ))
                    .child(self.render_authorization_kind_button(
                        AuthorizationKind::Basic,
                        "Basic Auth",
                        "auth-kind-basic",
                        authorization_kind == AuthorizationKind::Basic,
                        cx,
                    )),
            )
            .child(editor)
            .child(
                div()
                    .mt_3()
                    .font_family(FONT_UI)
                    .text_size(px(12.0))
                    .text_color(rgb(MUTED))
                    .child(hint),
            )
            .into_any_element()
    }

    pub(super) fn render_authorization_kind_button(
        &self,
        kind: AuthorizationKind,
        label: &'static str,
        selector: &'static str,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .debug_selector(move || selector.into())
            .h(px(30.0))
            .px_3()
            .flex()
            .items_center()
            .rounded_md()
            .border_1()
            .border_color(rgb(if selected { ACCENT } else { LINE }))
            .bg(rgb(if selected { ACCENT_SOFT } else { CODE_PANEL }))
            .font_family(FONT_UI)
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(px(12.0))
            .text_color(rgb(if selected { ACCENT_DARK } else { MUTED }))
            .cursor_pointer()
            .hover(|style| style.border_color(rgb(ACCENT)).text_color(rgb(ACCENT_DARK)))
            .child(label)
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, _, cx| this.set_authorization_kind(kind, cx)),
            )
    }

    pub(super) fn render_basic_auth_field(
        &self,
        label: &'static str,
        selector: &'static str,
        input: Entity<HeaderInput>,
    ) -> impl IntoElement {
        div()
            .h(px(48.0))
            .flex()
            .items_center()
            .gap_4()
            .px_3()
            .rounded_lg()
            .bg(rgb(CODE_PANEL))
            .child(
                div()
                    .w(px(120.0))
                    .flex_none()
                    .font_family(FONT_UI)
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(12.0))
                    .text_color(rgb(0x0093_c5fd))
                    .child(label),
            )
            .child(
                div()
                    .debug_selector(move || selector.into())
                    .h(px(34.0))
                    .flex_1()
                    .child(input),
            )
    }

    pub(super) fn render_script_editor(
        &self,
        title: &'static str,
        hint: &'static str,
        input: Entity<BodyInput>,
        selector: &'static str,
    ) -> gpui::AnyElement {
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .bg(rgb(CODE_BG))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .font_family(FONT_UI)
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(CODE_TEXT))
                            .child(title),
                    )
                    .child(div().text_size(px(11.0)).text_color(rgb(MUTED)).child(hint)),
            )
            .child(
                div()
                    .debug_selector(move || selector.into())
                    .flex_1()
                    .min_h_0()
                    .child(input),
            )
            .into_any_element()
    }

    pub(super) fn render_key_value_editor(
        &self,
        title: &'static str,
        rows: Vec<KeyValueRow>,
        toggle: fn(&mut Self, usize, &mut Context<Self>),
        remove: fn(&mut Self, usize, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let enabled = rows.iter().filter(|row| row.enabled).count();
        let row_selector_prefix = match self.view_model.read(cx).request_pane() {
            RequestPane::Params => "param-row-toggle",
            RequestPane::Headers => "header-row-toggle",
            RequestPane::Authorization
            | RequestPane::Body
            | RequestPane::Scripts
            | RequestPane::Tests => "row-toggle",
        };
        div()
            .id("key-value-editor-scroll")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .overflow_scroll()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .font_family(FONT_UI)
                    .text_size(px(12.0))
                    .child(
                        div()
                            .text_color(rgb(TEXT))
                            .font_weight(FontWeight::BOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_color(rgb(MUTED))
                            .child(format!("{enabled} enabled")),
                    ),
            )
            .children(rows.iter().enumerate().map(|(index, row)| {
                let is_enabled = row.enabled;
                let toggle_selector = format!("{row_selector_prefix}-{index}");
                div()
                    .h(px(36.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .font_family(FONT_MONO)
                    .text_size(px(12.0))
                    .child(
                        div()
                            .debug_selector(move || toggle_selector.clone())
                            .size(px(18.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .border_1()
                            .border_color(rgb(if is_enabled { ACCENT } else { LINE }))
                            .bg(rgb(if is_enabled { ACCENT } else { PANEL }))
                            .text_color(rgb(PANEL))
                            .cursor_pointer()
                            .child(if is_enabled { "✓" } else { "" })
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(move |this, _, _, cx| toggle(this, index, cx)),
                            ),
                    )
                    .child(
                        div()
                            .h_full()
                            .flex_1()
                            .flex()
                            .items_center()
                            .px_3()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(LINE))
                            .bg(rgb(PANEL_ALT))
                            .text_color(rgb(if is_enabled { TEXT } else { MUTED }))
                            .child(row.key.clone()),
                    )
                    .child(
                        div()
                            .h_full()
                            .flex_1()
                            .flex()
                            .items_center()
                            .px_3()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(LINE))
                            .bg(rgb(PANEL_ALT))
                            .text_color(rgb(if is_enabled { TEXT } else { MUTED }))
                            .child(row.value.clone()),
                    )
                    .child(
                        div()
                            .size(px(32.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_lg()
                            .cursor_pointer()
                            .text_color(rgb(MUTED))
                            .hover(|style| style.bg(rgb(0x00fe_f2f2)).text_color(rgb(ERROR)))
                            .child("×")
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(move |this, _, _, cx| remove(this, index, cx)),
                            ),
                    )
            }))
            .when(rows.is_empty(), |editor| {
                editor.child(
                    div()
                        .h(px(44.0))
                        .flex()
                        .items_center()
                        .px_3()
                        .rounded_lg()
                        .bg(rgb(PANEL_ALT))
                        .font_family(FONT_UI)
                        .text_size(px(12.0))
                        .text_color(rgb(MUTED))
                        .child("No rows yet — add a key and value below."),
                )
            })
            .child(
                div()
                    .h(px(38.0))
                    .flex_none()
                    .flex()
                    .gap_2()
                    .child(div().w(px(18.0)))
                    .child(
                        div()
                            .debug_selector(|| "row-key-input".into())
                            .h_full()
                            .flex_1()
                            .child(self.row_key_input.clone()),
                    )
                    .child(
                        div()
                            .debug_selector(|| "row-value-input".into())
                            .h_full()
                            .flex_1()
                            .child(self.row_value_input.clone()),
                    )
                    .child(
                        div()
                            .debug_selector(|| "add-row-button".into())
                            .w(px(64.0))
                            .h_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_lg()
                            .bg(rgb(ACCENT))
                            .text_color(rgb(PANEL))
                            .font_family(FONT_UI)
                            .text_size(px(12.0))
                            .font_weight(FontWeight::BOLD)
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(ACCENT_DARK)))
                            .child("Add")
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.add_current_row(cx)),
                            ),
                    ),
            )
            .when(
                self.view_model.read(cx).request_pane() == RequestPane::Headers,
                |editor| {
                    editor.child(
                        div()
                            .flex()
                            .gap_2()
                            .font_family(FONT_UI)
                            .text_size(px(11.0))
                            .child(
                                div()
                                    .debug_selector(|| "body-sample-json".into())
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(rgb(PANEL_ALT))
                                    .text_color(rgb(SUBTEXT))
                                    .cursor_pointer()
                                    .child("JSON")
                                    .on_mouse_up(
                                        gpui::MouseButton::Left,
                                        cx.listener(|this, _, _, cx| {
                                            this.set_header_input_values(
                                                "Content-Type",
                                                "application/json",
                                                cx,
                                            )
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "body-clear-button".into())
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(rgb(PANEL_ALT))
                                    .text_color(rgb(SUBTEXT))
                                    .cursor_pointer()
                                    .child("Bearer token")
                                    .on_mouse_up(
                                        gpui::MouseButton::Left,
                                        cx.listener(|this, _, _, cx| {
                                            this.set_header_input_values(
                                                "Authorization",
                                                "Bearer ",
                                                cx,
                                            )
                                        }),
                                    ),
                            ),
                    )
                },
            )
            .into_any_element()
    }

    pub(super) fn render_body_editor(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let kind = self.view_model.read(cx).body_kind();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(CODE_BG))
            .child(
                div()
                    .h(px(40.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_4()
                    .px_4()
                    .bg(rgb(PANEL))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(self.body_kind_option("○", "none", Some(BodyKind::None), kind, cx))
                    .child(self.body_kind_option(
                        "○",
                        "form-data",
                        Some(BodyKind::Multipart),
                        kind,
                        cx,
                    ))
                    .child(self.body_kind_option(
                        "○",
                        "x-www-form-urlencoded",
                        Some(BodyKind::UrlEncoded),
                        kind,
                        cx,
                    ))
                    .child(self.body_kind_option("●", "raw", Some(BodyKind::Raw), kind, cx))
                    .child(self.body_kind_option("●", "JSON ▾", Some(BodyKind::Json), kind, cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .p_3()
                    .bg(rgb(CODE_BG))
                    .child(
                        div()
                            .debug_selector(|| "body-input".into())
                            .flex_1()
                            .min_h_0()
                            .child(self.body_input.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .mt_2()
                            .font_family(FONT_UI)
                            .text_size(px(11.0))
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(rgb(CODE_PANEL))
                                    .text_color(rgb(CODE_TEXT))
                                    .cursor_pointer()
                                    .child("Sample JSON")
                                    .on_mouse_up(
                                        gpui::MouseButton::Left,
                                        cx.listener(|this, _, _, cx| this.use_sample_json(cx)),
                                    ),
                            )
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(rgb(CODE_PANEL))
                                    .text_color(rgb(MUTED))
                                    .cursor_pointer()
                                    .child("Clear")
                                    .on_mouse_up(
                                        gpui::MouseButton::Left,
                                        cx.listener(|this, _, _, cx| this.clear_body(cx)),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn body_kind_option(
        &self,
        marker: &'static str,
        label: &'static str,
        option: Option<BodyKind>,
        selected: BodyKind,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = option == Some(selected);
        let debug_selector = match option {
            None => "body-kind-none",
            Some(BodyKind::None) => "body-kind-none",
            Some(BodyKind::Multipart) => "body-kind-form-data",
            Some(BodyKind::UrlEncoded) => "body-kind-url-encoded",
            Some(BodyKind::Raw) => "body-kind-raw",
            Some(BodyKind::Json) => "body-kind-json",
        };
        let element = div()
            .debug_selector(move || debug_selector.into())
            .flex()
            .items_center()
            .gap_1()
            .font_family(FONT_UI)
            .text_size(px(12.0))
            .font_weight(if active {
                FontWeight::BOLD
            } else {
                FontWeight::NORMAL
            })
            .text_color(rgb(if active { TEXT } else { SUBTEXT }))
            .child(
                div()
                    .text_color(rgb(if active { 0x0025_63eb } else { MUTED }))
                    .child(marker),
            )
            .child(label);
        if let Some(kind) = option {
            element.cursor_pointer().on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, _, cx| this.set_body_kind(kind, cx)),
            )
        } else {
            element
        }
    }
}

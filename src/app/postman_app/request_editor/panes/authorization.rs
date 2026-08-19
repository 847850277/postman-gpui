use super::super::RequestEditor;
use crate::{
    app::AuthorizationKind,
    ui::{
        components::header_input::HeaderInput,
        theme::{
            ACCENT, ACCENT_DARK, ACCENT_SOFT, CODE_PANEL, FONT_MONO, FONT_UI, INFO, INFO_SOFT,
            LINE, MUTED, OK, OK_SOFT, PANEL, PANEL_ALT, SUBTEXT, TEXT,
        },
    },
};
use gpui::{
    div, px, rgb, Context, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Styled,
};

impl RequestEditor {
    pub(in crate::app::postman_app::request_editor) fn render_authorization_editor(
        &self,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let (
            authorization_kind,
            normalized_token,
            header_preview,
            basic_username_saved,
            basic_password_saved,
            auth_ready,
        ) = {
            let view_model = self.view_model.read(cx);
            let authorization_kind = view_model.authorization_kind();
            let normalized_token = view_model.normalized_bearer_token();
            let header_preview = view_model.authorization_header_preview();
            let basic_username_saved = !view_model.basic_username().is_empty();
            let basic_password_saved = !view_model.basic_password().is_empty();
            let auth_ready = match authorization_kind {
                AuthorizationKind::Bearer => !normalized_token.is_empty(),
                AuthorizationKind::Basic => basic_username_saved && basic_password_saved,
            };
            (
                authorization_kind,
                normalized_token,
                header_preview,
                basic_username_saved,
                basic_password_saved,
                auth_ready,
            )
        };
        let editor = match authorization_kind {
            AuthorizationKind::Bearer => div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .gap_2()
                .p_3()
                .child(
                    div()
                        .h(px(48.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap_3()
                        .px_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(LINE))
                        .bg(rgb(PANEL_ALT))
                        .child(
                            div()
                                .w(px(120.0))
                                .flex_none()
                                .font_family(FONT_UI)
                                .font_weight(FontWeight::BOLD)
                                .text_size(px(12.0))
                                .text_color(rgb(INFO))
                                .child("Bearer token"),
                        )
                        .child(
                            div()
                                .debug_selector(|| "authorization-input".into())
                                .h(px(34.0))
                                .min_w_0()
                                .flex_1()
                                .child(self.authorization_input.clone()),
                        )
                        .child(
                            div()
                                .h(px(24.0))
                                .px_2()
                                .flex_none()
                                .flex()
                                .items_center()
                                .rounded_lg()
                                .bg(rgb(OK_SOFT))
                                .font_family(FONT_UI)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(px(9.0))
                                .text_color(rgb(OK))
                                .child("LIVE · SAVED"),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
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
                                .w(px(190.0))
                                .min_w_0()
                                .flex_none()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .font_family(FONT_UI)
                                        .font_weight(FontWeight::BOLD)
                                        .text_size(px(9.0))
                                        .text_color(rgb(INFO))
                                        .child("NORMALIZED TOKEN"),
                                )
                                .child(
                                    div()
                                        .debug_selector(|| "authorization-normalized-token".into())
                                        .overflow_hidden()
                                        .font_family(FONT_MONO)
                                        .text_size(px(11.0))
                                        .text_color(rgb(if auth_ready { TEXT } else { MUTED }))
                                        .child(if normalized_token.is_empty() {
                                            "—".to_string()
                                        } else {
                                            normalized_token
                                        }),
                                )
                                .child(
                                    div()
                                        .font_family(FONT_UI)
                                        .text_size(px(9.0))
                                        .text_color(rgb(SUBTEXT))
                                        .child("Optional Bearer prefix removed once"),
                                ),
                        )
                        .child(
                            div()
                                .flex_none()
                                .font_family(FONT_UI)
                                .text_size(px(16.0))
                                .text_color(rgb(INFO))
                                .child("→"),
                        )
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
                                        .text_size(px(9.0))
                                        .text_color(rgb(INFO))
                                        .child("OUTGOING HEADER"),
                                )
                                .child(
                                    div()
                                        .debug_selector(|| "authorization-header-preview".into())
                                        .overflow_hidden()
                                        .font_family(FONT_MONO)
                                        .text_size(px(11.0))
                                        .text_color(rgb(if auth_ready { TEXT } else { MUTED }))
                                        .child(header_preview.unwrap_or_else(|| {
                                            "Authorization header will appear here".to_string()
                                        })),
                                )
                                .child(
                                    div()
                                        .font_family(FONT_UI)
                                        .text_size(px(9.0))
                                        .text_color(rgb(SUBTEXT))
                                        .child("One canonical header · no duplicated prefix"),
                                ),
                        )
                        .child(
                            div()
                                .h(px(24.0))
                                .px_2()
                                .flex_none()
                                .flex()
                                .items_center()
                                .rounded_lg()
                                .bg(rgb(if auth_ready { OK_SOFT } else { PANEL }))
                                .font_family(FONT_UI)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(px(9.0))
                                .text_color(rgb(if auth_ready { OK } else { MUTED }))
                                .child(if auth_ready { "1 PREFIX" } else { "WAITING" }),
                        ),
                )
                .into_any_element(),
            AuthorizationKind::Basic => div()
                .debug_selector(|| "basic-auth-credentials".into())
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .gap_2()
                .px_3()
                .child(
                    div()
                        .h(px(20.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_between()
                        .font_family(FONT_UI)
                        .child(
                            div()
                                .font_weight(FontWeight::BOLD)
                                .text_size(px(12.0))
                                .text_color(rgb(TEXT))
                                .child("Basic Auth credentials"),
                        )
                        .child(
                            div()
                                .debug_selector(|| "basic-auth-password-masked".into())
                                .text_size(px(10.0))
                                .text_color(rgb(SUBTEXT))
                                .child("Password remains masked in the View"),
                        ),
                )
                .child(
                    div()
                        .h(px(72.0))
                        .flex_none()
                        .flex()
                        .gap_3()
                        .child(self.render_basic_auth_field(
                            "Username",
                            "basic-auth-username-field",
                            "basic-auth-username-input",
                            "basic-auth-username-saved",
                            self.basic_username_input.clone(),
                            basic_username_saved,
                        ))
                        .child(self.render_basic_auth_field(
                            "Password",
                            "basic-auth-password-field",
                            "basic-auth-password-input",
                            "basic-auth-password-saved",
                            self.basic_password_input.clone(),
                            basic_password_saved,
                        )),
                )
                .child(
                    div()
                        .debug_selector(|| "basic-auth-header-preview".into())
                        .h(px(58.0))
                        .flex_none()
                        .flex()
                        .flex_col()
                        .justify_center()
                        .gap_1()
                        .px_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(INFO))
                        .bg(rgb(INFO_SOFT))
                        .child(
                            div()
                                .font_family(FONT_UI)
                                .font_weight(FontWeight::BOLD)
                                .text_size(px(9.0))
                                .text_color(rgb(INFO))
                                .child("OUTGOING HEADER · ONE CANONICAL VALUE"),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .font_family(FONT_MONO)
                                .text_size(px(11.0))
                                .text_color(rgb(if header_preview.is_some() {
                                    TEXT
                                } else {
                                    MUTED
                                }))
                                .child(header_preview.unwrap_or_else(|| {
                                    "Authorization header will appear here".to_string()
                                })),
                        ),
                )
                .child(
                    div()
                        .debug_selector(|| "basic-auth-projection-note".into())
                        .h(px(20.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap_2()
                        .font_family(FONT_UI)
                        .text_size(px(10.0))
                        .text_color(rgb(SUBTEXT))
                        .child(div().text_color(rgb(OK)).child("●"))
                        .child(
                            "View fields → RequestViewModel → Basic encoder; no blur, Enter, or Tab required.",
                        ),
                )
                .into_any_element(),
        };
        let (mode_label, ready_message) = match authorization_kind {
            AuthorizationKind::Bearer => (
                "Bearer Token",
                if auth_ready {
                    "Ready to send — the active token is already in the ViewModel"
                } else {
                    "Enter a token — input is saved to the ViewModel as you type"
                },
            ),
            AuthorizationKind::Basic => (
                "Basic Auth",
                if auth_ready {
                    "Ready to send — the active password is already in the ViewModel"
                } else {
                    "Enter username and password — each input is saved as you type"
                },
            ),
        };

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .child(
                div()
                    .debug_selector(|| "authorization-summary".into())
                    .h(px(42.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .px_3()
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .font_family(FONT_UI)
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_none()
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(12.0))
                                    .text_color(rgb(TEXT))
                                    .child("Authorization"),
                            )
                            .child(
                                div()
                                    .overflow_hidden()
                                    .text_size(px(11.0))
                                    .text_color(rgb(SUBTEXT))
                                    .child("Managed header · saved as you type"),
                            ),
                    )
                    .child(
                        div()
                            .debug_selector(|| "authorization-status".into())
                            .h(px(24.0))
                            .px_2()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap_1()
                            .rounded_lg()
                            .bg(rgb(if auth_ready { OK_SOFT } else { PANEL_ALT }))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(10.0))
                            .text_color(rgb(if auth_ready { OK } else { MUTED }))
                            .child(if auth_ready { "●" } else { "○" })
                            .child(format!(
                                "{} · {}",
                                mode_label,
                                if auth_ready { "ready" } else { "empty" }
                            )),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "authorization-kind-selector".into())
                    .h(px(44.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .bg(rgb(PANEL_ALT))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .mr_2()
                            .font_family(FONT_UI)
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(10.0))
                            .text_color(rgb(SUBTEXT))
                            .child("AUTH TYPE"),
                    )
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
                    .debug_selector(|| "authorization-ready-indicator".into())
                    .h(px(34.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .font_family(FONT_UI)
                    .text_size(px(10.0))
                    .text_color(rgb(SUBTEXT))
                    .child(
                        div()
                            .text_color(rgb(if auth_ready { OK } else { MUTED }))
                            .child(if auth_ready { "✓" } else { "○" }),
                    )
                    .child(ready_message),
            )
            .into_any_element()
    }

    pub(in crate::app::postman_app::request_editor) fn render_authorization_kind_button(
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

    pub(in crate::app::postman_app::request_editor) fn render_basic_auth_field(
        &self,
        label: &'static str,
        field_selector: &'static str,
        input_selector: &'static str,
        saved_selector: &'static str,
        input: Entity<HeaderInput>,
        saved: bool,
    ) -> impl IntoElement {
        div()
            .debug_selector(move || field_selector.into())
            .min_w_0()
            .flex_1()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .font_family(FONT_UI)
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(9.0))
                    .text_color(rgb(SUBTEXT))
                    .child(label.to_ascii_uppercase()),
            )
            .child(
                div()
                    .h(px(48.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(INFO))
                    .bg(rgb(PANEL))
                    .child(
                        div()
                            .debug_selector(move || input_selector.into())
                            .min_w_0()
                            .h(px(34.0))
                            .flex_1()
                            .child(input),
                    )
                    .child(
                        div()
                            .debug_selector(move || saved_selector.into())
                            .h(px(24.0))
                            .px_2()
                            .flex_none()
                            .flex()
                            .items_center()
                            .rounded_lg()
                            .bg(rgb(if saved { OK_SOFT } else { PANEL_ALT }))
                            .font_family(FONT_UI)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(9.0))
                            .text_color(rgb(if saved { OK } else { MUTED }))
                            .child(if saved { "SAVED" } else { "EMPTY" }),
                    ),
            )
    }
}

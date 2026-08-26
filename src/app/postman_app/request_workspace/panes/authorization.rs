use crate::{
    app::{ActivateControl, AuthorizationKind, WorkspaceViewModel},
    ui::{
        components::input::header_input::{HeaderInput, HeaderInputEvent},
        theme::{
            ACCENT, ACCENT_DARK, ACCENT_SOFT, CODE_PANEL, FONT_MONO, FONT_UI, INFO, INFO_SOFT,
            LINE, MUTED, OK, OK_SOFT, PANEL, PANEL_ALT, SUBTEXT, TEXT,
        },
    },
};
use gpui::{
    actions, div, prelude::FluentBuilder, px, rgb, AppContext, Context, Entity, FocusHandle,
    FontWeight, InteractiveElement, IntoElement, KeyBinding, ParentElement, Render, Role,
    StatefulInteractiveElement, Styled, Subscription, Window,
};

actions!(
    authorization_kind,
    [NextAuthorizationKind, PreviousAuthorizationKind]
);

fn setup_authorization_kind_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("right", NextAuthorizationKind, Some("AuthorizationKind")),
        KeyBinding::new("down", NextAuthorizationKind, Some("AuthorizationKind")),
        KeyBinding::new("left", PreviousAuthorizationKind, Some("AuthorizationKind")),
        KeyBinding::new("up", PreviousAuthorizationKind, Some("AuthorizationKind")),
    ]
}

/// Authorization controls own cursor, masking, and subscription state; credentials remain in the
/// shared WorkspaceViewModel.
pub(in crate::app::postman_app::request_workspace) struct AuthorizationPane {
    view_model: Entity<WorkspaceViewModel>,
    authorization_input: Entity<HeaderInput>,
    basic_username_input: Entity<HeaderInput>,
    basic_password_input: Entity<HeaderInput>,
    kind_focus_handles: Vec<FocusHandle>,
    _subscriptions: Vec<Subscription>,
}

impl AuthorizationPane {
    pub(in crate::app::postman_app::request_workspace) fn new(
        view_model: Entity<WorkspaceViewModel>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.bind_keys(setup_authorization_kind_key_bindings());
        let authorization_input =
            cx.new(|cx| HeaderInput::new(cx).with_placeholder("Token or Bearer token"));
        let basic_username_input = cx.new(|cx| {
            HeaderInput::new(cx)
                .with_placeholder("Username")
                .with_embedded_chrome(true)
        });
        let basic_password_input = cx.new(|cx| {
            HeaderInput::new(cx)
                .with_placeholder("Password")
                .with_masked(true)
                .with_embedded_chrome(true)
        });
        let subscriptions = vec![
            cx.subscribe(&authorization_input, Self::on_authorization_event),
            cx.subscribe(&basic_username_input, Self::on_basic_username_event),
            cx.subscribe(&basic_password_input, Self::on_basic_password_event),
        ];
        let mut pane = Self {
            view_model,
            authorization_input,
            basic_username_input,
            basic_password_input,
            kind_focus_handles: (0..2)
                .map(|_| cx.focus_handle().tab_index(0).tab_stop(true))
                .collect(),
            _subscriptions: subscriptions,
        };
        pane.project_active_request(cx);
        pane
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

    fn on_authorization_event(
        &mut self,
        _input: Entity<HeaderInput>,
        event: &HeaderInputEvent,
        cx: &mut Context<Self>,
    ) {
        if let HeaderInputEvent::ValueChanged(token) = event {
            self.update_view_model(cx, |view_model| view_model.set_bearer_token(token));
        }
    }

    fn on_basic_username_event(
        &mut self,
        _input: Entity<HeaderInput>,
        event: &HeaderInputEvent,
        cx: &mut Context<Self>,
    ) {
        if let HeaderInputEvent::ValueChanged(username) = event {
            self.update_view_model(cx, |view_model| view_model.set_basic_username(username));
        }
    }

    fn on_basic_password_event(
        &mut self,
        _input: Entity<HeaderInput>,
        event: &HeaderInputEvent,
        cx: &mut Context<Self>,
    ) {
        if let HeaderInputEvent::ValueChanged(password) = event {
            self.update_view_model(cx, |view_model| view_model.set_basic_password(password));
        }
    }

    fn set_authorization_kind(&mut self, kind: AuthorizationKind, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.set_authorization_kind(kind));
    }

    pub(in crate::app::postman_app::request_workspace) fn project_active_request(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let (bearer_token, basic_username, basic_password) = {
            let view_model = self.view_model.read(cx);
            (
                view_model.bearer_token().to_string(),
                view_model.basic_username().to_string(),
                view_model.basic_password().to_string(),
            )
        };
        self.authorization_input
            .update(cx, |input, cx| input.project_content(bearer_token, cx));
        self.basic_username_input
            .update(cx, |input, cx| input.project_content(basic_username, cx));
        self.basic_password_input
            .update(cx, |input, cx| input.project_content(basic_password, cx));
        cx.notify();
    }

    fn render_authorization_editor(
        &self,
        window: &Window,
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
                        window,
                        cx,
                    ))
                    .child(self.render_authorization_kind_button(
                        AuthorizationKind::Basic,
                        "Basic Auth",
                        "auth-kind-basic",
                        authorization_kind == AuthorizationKind::Basic,
                        window,
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

    fn render_authorization_kind_button(
        &self,
        kind: AuthorizationKind,
        label: &'static str,
        selector: &'static str,
        selected: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let index = match kind {
            AuthorizationKind::Bearer => 0,
            AuthorizationKind::Basic => 1,
        };
        let focus_handle = self.kind_focus_handles[index].clone();
        let mouse_focus_handle = focus_handle.clone();
        let focused = focus_handle.is_focused(window);
        div()
            .id(selector)
            .debug_selector(move || selector.into())
            .track_focus(&focus_handle)
            .key_context("KeyboardButton AuthorizationKind")
            .role(Role::RadioButton)
            .aria_label(label)
            .aria_selected(selected)
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
            .when(focused, |button| button.border_2().border_color(rgb(INFO)))
            .child(label)
            .on_action(cx.listener(move |this, _: &ActivateControl, _, cx| {
                this.set_authorization_kind(kind, cx)
            }))
            .on_action(
                cx.listener(move |this, _: &NextAuthorizationKind, window, cx| {
                    this.select_relative_authorization_kind(kind, 1, window, cx)
                }),
            )
            .on_action(
                cx.listener(move |this, _: &PreviousAuthorizationKind, window, cx| {
                    this.select_relative_authorization_kind(kind, -1, window, cx)
                }),
            )
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    mouse_focus_handle.focus(window, cx);
                    this.set_authorization_kind(kind, cx);
                }),
            )
    }

    fn select_relative_authorization_kind(
        &mut self,
        kind: AuthorizationKind,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let index = match kind {
            AuthorizationKind::Bearer => 0,
            AuthorizationKind::Basic => 1,
        };
        let next = (index as isize + delta).rem_euclid(2) as usize;
        let kind = if next == 0 {
            AuthorizationKind::Bearer
        } else {
            AuthorizationKind::Basic
        };
        self.kind_focus_handles[next].focus(window, cx);
        self.set_authorization_kind(kind, cx);
    }

    fn render_basic_auth_field(
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

impl Render for AuthorizationPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_authorization_editor(window, cx)
    }
}

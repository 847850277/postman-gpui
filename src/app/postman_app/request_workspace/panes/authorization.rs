use crate::{
    app::{AuthorizationKind, RequestViewModel, WorkspaceViewModel},
    ui::{
        components::input::header_input::{HeaderInput, HeaderInputEvent},
        theme::{ACCENT, ACCENT_DARK, ACCENT_SOFT, LINE, MUTED, PANEL, PANEL_ALT, TEXT},
    },
};
use gpui::{
    actions, div, prelude::FluentBuilder, AppContext, Context, Entity, FocusHandle,
    InteractiveElement, IntoElement, KeyBinding, ParentElement, Render, StatefulInteractiveElement,
    Styled, Subscription, Window,
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
        let authorization_input = cx.new(|cx| {
            HeaderInput::new(cx)
                .with_placeholder("Token or Bearer token")
                .with_embedded_chrome(true)
        });
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

    fn update_active_request<R>(
        &self,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut RequestViewModel) -> R,
    ) -> Option<R> {
        let result = self.view_model.update(cx, |view_model, cx| {
            let result = view_model.update_active_request(update);
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
            self.update_active_request(cx, |request| request.set_bearer_token(token));
        }
    }

    fn on_basic_username_event(
        &mut self,
        _input: Entity<HeaderInput>,
        event: &HeaderInputEvent,
        cx: &mut Context<Self>,
    ) {
        if let HeaderInputEvent::ValueChanged(username) = event {
            self.update_active_request(cx, |request| request.set_basic_username(username));
        }
    }

    fn on_basic_password_event(
        &mut self,
        _input: Entity<HeaderInput>,
        event: &HeaderInputEvent,
        cx: &mut Context<Self>,
    ) {
        if let HeaderInputEvent::ValueChanged(password) = event {
            self.update_active_request(cx, |request| request.set_basic_password(password));
        }
    }

    fn set_authorization_kind(&mut self, kind: AuthorizationKind, cx: &mut Context<Self>) {
        self.update_active_request(cx, |request| request.set_authorization_kind(kind));
    }

    pub(in crate::app::postman_app::request_workspace) fn project_active_request(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let (bearer_token, basic_username, basic_password) = {
            let view_model = self.view_model.read(cx);
            view_model.active_request().map_or_else(
                || (String::new(), String::new(), String::new()),
                |request| {
                    (
                        request.bearer_token().to_string(),
                        request.basic_username().to_string(),
                        request.basic_password().to_string(),
                    )
                },
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
        use crate::ui::theme::metrics as m;
        let model = self.view_model.read(cx);
        let Some(request) = model.active_request() else {
            return div().into_any_element();
        };
        let kind = request.authorization_kind();
        let ready = request.authorization_header_preview().is_some();
        let basic = kind == AuthorizationKind::Basic;
        let mut fields = div()
            .debug_selector(|| "authorization-fields".into())
            .flex()
            .flex_col()
            .gap_4();
        if basic {
            fields = fields.child(
                div()
                    .debug_selector(|| "basic-auth-credentials".into())
                    .flex()
                    .gap_4()
                    .child(Self::render_auth_field(
                        "Username",
                        "basic-auth-username-input",
                        self.basic_username_input.clone(),
                        cx,
                    ))
                    .child(Self::render_auth_field(
                        "Password",
                        "basic-auth-password-input",
                        self.basic_password_input.clone(),
                        cx,
                    )),
            );
        } else {
            fields = fields.child(Self::render_auth_field(
                "Token",
                "authorization-input",
                self.authorization_input.clone(),
                cx,
            ));
        }
        div()
            .id("authorization-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px_7()
            .py_4()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .debug_selector(|| "authorization-kind-selector".into())
                    .flex()
                    .items_center()
                    .gap_4()
                    .text_size(m::LABEL)
                    .child(
                        div()
                            .w(gpui::rems(7.))
                            .text_color(MUTED.resolve(cx))
                            .child("Auth type"),
                    )
                    .child(self.render_authorization_kind_button(
                        AuthorizationKind::Bearer,
                        "Bearer Token",
                        "auth-kind-bearer",
                        !basic,
                        window,
                        cx,
                    ))
                    .child(self.render_authorization_kind_button(
                        AuthorizationKind::Basic,
                        "Basic Auth",
                        "auth-kind-basic",
                        basic,
                        window,
                        cx,
                    )),
            )
            .child(fields)
            .child(
                div()
                    .debug_selector(|| "authorization-status".into())
                    .text_size(m::LABEL)
                    .text_color(MUTED.resolve(cx))
                    .child(if ready {
                        "Authorization is added to this request automatically."
                    } else {
                        "Enter credentials to add an Authorization header."
                    }),
            )
            .when(basic, |pane| {
                pane.child(
                    div()
                        .debug_selector(|| "basic-auth-password-masked".into())
                        .text_size(m::CAPTION)
                        .text_color(MUTED.resolve(cx))
                        .child("Password is hidden."),
                )
            })
            .into_any_element()
    }

    fn render_auth_field(
        label: &'static str,
        selector: &'static str,
        input: Entity<HeaderInput>,
        cx: &gpui::App,
    ) -> impl IntoElement {
        use crate::ui::theme::metrics as m;
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(m::LABEL)
                    .text_color(TEXT.resolve(cx))
                    .child(label),
            )
            .child(
                div()
                    .debug_selector(move || selector.into())
                    .h(m::TABLE_ROW)
                    .px_3()
                    .border_1()
                    .border_color(LINE.resolve(cx))
                    .rounded(m::RADIUS)
                    .bg(PANEL_ALT.resolve(cx))
                    .child(input),
            )
    }
    fn render_authorization_kind_button(
        &self,
        kind: AuthorizationKind,
        label: &'static str,
        selector: &'static str,
        selected: bool,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let index = match kind {
            AuthorizationKind::Bearer => 0,
            AuthorizationKind::Basic => 1,
        };
        let focus_handle = self.kind_focus_handles[index].clone();
        let on_select = cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
            this.set_authorization_kind(kind, cx)
        });
        gpui_kit::base::Radio::new(selector)
            .debug_selector(move || selector.into())
            .checked(selected)
            .accessibility_label(label)
            .track_focus(&focus_handle)
            .key_context("AuthorizationKind")
            .h(gpui::rems(2.))
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .rounded(crate::ui::theme::metrics::RADIUS)
            .border_1()
            .border_color((if selected { ACCENT } else { LINE }).resolve(cx))
            .bg((if selected { ACCENT_SOFT } else { PANEL }).resolve(cx))
            .text_size(crate::ui::theme::metrics::LABEL)
            .text_color((if selected { ACCENT_DARK } else { MUTED }).resolve(cx))
            .focus_visible(|s| s.border_color(ACCENT.resolve(cx)))
            .child(label)
            .on_change(move |_, event, window, cx| on_select(event, window, cx))
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
}
impl Render for AuthorizationPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_authorization_editor(window, cx)
    }
}

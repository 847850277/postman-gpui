use crate::{
    app::{AuthorizationKind, RequestViewModel, WorkspaceViewModel},
    ui::{
        components::input::header_input::{HeaderInput, HeaderInputEvent},
        theme::{LINE, MUTED, PANEL_ALT},
    },
};
use gpui::{
    div, prelude::FluentBuilder, AppContext, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, Subscription, Window,
};

use crate::ui::components::kit_controls::MethodState;
use gpui_kit::component::{
    searchable_list::SearchableVec,
    select::{Select, SelectEvent, SelectState},
    IndexPath,
};

/// Authorization controls own cursor, masking, and subscription state; credentials remain in the
/// shared WorkspaceViewModel.
pub(in crate::app::postman_app::request_workspace) struct AuthorizationPane {
    view_model: Entity<WorkspaceViewModel>,
    authorization_input: Entity<HeaderInput>,
    basic_username_input: Entity<HeaderInput>,
    basic_password_input: Entity<HeaderInput>,
    kind_selector: Entity<MethodState>,
    projected_basic: Option<bool>,
    _subscriptions: Vec<Subscription>,
}

impl AuthorizationPane {
    pub(in crate::app::postman_app::request_workspace) fn new(
        view_model: Entity<WorkspaceViewModel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let kind_selector = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(vec!["Bearer token", "Basic auth"]),
                Some(IndexPath::new(0)),
                window,
                cx,
            )
        });
        let authorization_input = cx.new(|cx| {
            HeaderInput::new(cx)
                .with_placeholder("Enter your access token")
                .with_masked(true)
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
            cx.subscribe(
                &kind_selector,
                |this, _, event: &SelectEvent<SearchableVec<&'static str>>, cx| {
                    if let SelectEvent::Confirm(Some(label)) = event {
                        this.set_authorization_kind(
                            if *label == "Basic auth" {
                                AuthorizationKind::Basic
                            } else {
                                AuthorizationKind::Bearer
                            },
                            cx,
                        );
                    }
                },
            ),
            cx.subscribe(&authorization_input, Self::on_authorization_event),
            cx.subscribe(&basic_username_input, Self::on_basic_username_event),
            cx.subscribe(&basic_password_input, Self::on_basic_password_event),
        ];
        let mut pane = Self {
            view_model,
            authorization_input,
            basic_username_input,
            basic_password_input,
            kind_selector,
            projected_basic: None,
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
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        use crate::ui::theme::metrics as m;
        let model = self.view_model.read(cx);
        let Some(request) = model.active_request() else {
            return div().into_any_element();
        };
        let kind = request.authorization_kind();
        let basic = kind == AuthorizationKind::Basic;
        let mut fields = div()
            .debug_selector(|| "authorization-fields".into())
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_4();
        if basic {
            fields = fields.child(
                div()
                    .debug_selector(|| "basic-auth-credentials".into())
                    .min_w_0()
                    .flex()
                    .flex_col()
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
        let selector = if basic {
            "auth-kind-basic"
        } else {
            "auth-kind-bearer"
        };
        div()
            .id("authorization-scroll")
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll()
            .px_7()
            .pt(gpui::rems(22. / 16.))
            .pb_2()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(gpui::rems(18. / 16.))
                    .min_w_0()
                    .child(
                        div()
                            .debug_selector(|| "authorization-kind-selector".into())
                            .w(gpui::rems(140. / 16.))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(gpui::rems(11. / 16.))
                                    .text_color(MUTED.resolve(cx))
                                    .child("Authorization type"),
                            )
                            .child(
                                div().debug_selector(move || selector.into()).child(
                                    Select::new(&self.kind_selector)
                                        .id("authorization-kind-select")
                                        .accessibility_label("Authorization type")
                                        .h(gpui::rems(34. / 16.))
                                        .w_full()
                                        .text_size(gpui::rems(11. / 16.))
                                        .bg(PANEL_ALT.resolve(cx))
                                        .rounded(m::RADIUS),
                                ),
                            ),
                    )
                    .child(fields),
            )
            .child(
                div()
                    .debug_selector(|| "authorization-status".into())
                    .text_size(gpui::rems(11. / 16.))
                    .text_color(MUTED.resolve(cx))
                    .child(if basic {
                        "Username and password are included in the Authorization header."
                    } else {
                        "The token is included in the Authorization header when Bearer token is selected."
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
                    .text_size(gpui::rems(11. / 16.))
                    .text_color(MUTED.resolve(cx))
                    .child(label),
            )
            .child(
                div()
                    .debug_selector(move || selector.into())
                    .h(gpui::rems(34. / 16.))
                    .px_3()
                    .border_1()
                    .border_color(LINE.resolve(cx))
                    .rounded(m::RADIUS)
                    .bg(PANEL_ALT.resolve(cx))
                    .child(input),
            )
    }
}

impl Render for AuthorizationPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let basic = self
            .view_model
            .read(cx)
            .active_request()
            .is_some_and(|r| r.authorization_kind() == AuthorizationKind::Basic);
        let label = if basic { "Basic auth" } else { "Bearer token" };
        if self.projected_basic != Some(basic) {
            self.projected_basic = Some(basic);
            self.kind_selector
                .update(cx, |state, cx| state.set_selected_value(&label, window, cx));
        }
        self.render_authorization_editor(window, cx)
    }
}

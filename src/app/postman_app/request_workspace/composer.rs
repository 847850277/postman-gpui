use super::{
    composer_chrome::setup_request_pane_key_bindings,
    layout::{RequestPanelLayout, REQUEST_COMPOSER_GAP},
    panes::{
        AuthorizationPane, BodyPane, KeyValueRowsKind, KeyValueRowsPane, KeyValueRowsPaneEvent,
        OptionsPane, ScriptPane, ScriptPaneKind,
    },
};
use crate::{
    app::{BodyKind, PendingRequest, RequestPane, RequestViewModel, SendId, WorkspaceViewModel},
    ui::{
        components::input::{
            body_input::setup_body_input_key_bindings,
            header_input::setup_header_input_key_bindings,
            method_selector::{MethodSelector, MethodSelectorEvent},
            url_input::{setup_url_input_key_bindings, UrlInput, UrlInputEvent},
        },
        theme::{LINE, PANEL},
    },
};
use gpui::{
    div, px, rgb, App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, Styled, Subscription, Window,
};

#[derive(Clone, Debug)]
pub(super) enum RequestComposerEvent {
    Execute(PendingRequest),
    Abort(SendId),
}

/// Method/URL/Send controls plus the currently selected request pane.
///
/// Pane business values stay in WorkspaceViewModel. Child entities own only GPUI inputs,
/// subscriptions, focus/selection state, and scroll handles.
pub(super) struct RequestComposer {
    pub(super) view_model: Entity<WorkspaceViewModel>,
    panel_layout: Entity<RequestPanelLayout>,
    pub(super) method_selector: Entity<MethodSelector>,
    pub(super) url_input: Entity<UrlInput>,
    params_pane: Entity<KeyValueRowsPane>,
    headers_pane: Entity<KeyValueRowsPane>,
    authorization_pane: Entity<AuthorizationPane>,
    body_pane: Entity<BodyPane>,
    script_pane: Entity<ScriptPane>,
    tests_pane: Entity<ScriptPane>,
    options_pane: Entity<OptionsPane>,
    pub(super) request_pane_focus_handles: Vec<FocusHandle>,
    pub(super) send_focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<RequestComposerEvent> for RequestComposer {}

impl RequestComposer {
    pub(super) fn new(
        view_model: Entity<WorkspaceViewModel>,
        panel_layout: Entity<RequestPanelLayout>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.bind_keys(setup_url_input_key_bindings());
        cx.bind_keys(setup_header_input_key_bindings());
        cx.bind_keys(setup_body_input_key_bindings());
        cx.bind_keys(setup_request_pane_key_bindings());

        let method_selector = cx.new(MethodSelector::new);
        let url_input = cx.new(|cx| UrlInput::new(cx).with_placeholder("Enter request URL"));
        let params_pane = cx.new(|cx| {
            KeyValueRowsPane::new(
                view_model.clone(),
                panel_layout.clone(),
                KeyValueRowsKind::Params,
                cx,
            )
        });
        let headers_pane = cx.new(|cx| {
            KeyValueRowsPane::new(
                view_model.clone(),
                panel_layout.clone(),
                KeyValueRowsKind::Headers,
                cx,
            )
        });
        let authorization_pane = cx.new(|cx| AuthorizationPane::new(view_model.clone(), cx));
        let body_pane = cx.new(|cx| BodyPane::new(view_model.clone(), cx));
        let script_pane =
            cx.new(|cx| ScriptPane::new(view_model.clone(), ScriptPaneKind::PreRequest, cx));
        let tests_pane =
            cx.new(|cx| ScriptPane::new(view_model.clone(), ScriptPaneKind::Tests, cx));
        let options_pane = cx.new(|cx| OptionsPane::new(view_model.clone(), cx));

        let subscriptions = vec![
            cx.subscribe(&method_selector, Self::on_method_changed),
            cx.subscribe(&url_input, Self::on_url_event),
            cx.subscribe(&params_pane, Self::on_key_value_pane_event),
            cx.subscribe(&headers_pane, Self::on_key_value_pane_event),
            cx.observe(&view_model, |_, _, cx| cx.notify()),
            cx.observe(&panel_layout, |_, _, cx| cx.notify()),
        ];

        let mut composer = Self {
            view_model,
            panel_layout,
            method_selector,
            url_input,
            params_pane,
            headers_pane,
            authorization_pane,
            body_pane,
            script_pane,
            tests_pane,
            options_pane,
            request_pane_focus_handles: (0..7)
                .map(|_| cx.focus_handle().tab_index(0).tab_stop(true))
                .collect(),
            send_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            _subscriptions: subscriptions,
        };
        composer.project_active_request(cx);
        composer
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

    fn update_active_request<R>(
        &self,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut RequestViewModel) -> R,
    ) -> Option<R> {
        self.update_view_model(cx, |view_model| view_model.update_active_request(update))
    }

    fn on_method_changed(
        &mut self,
        _selector: Entity<MethodSelector>,
        event: &MethodSelectorEvent,
        cx: &mut Context<Self>,
    ) {
        let MethodSelectorEvent::MethodChanged(method) = event;
        self.update_active_request(cx, |request| request.set_method(*method));
        self.project_selected_pane(cx);
    }

    fn on_url_event(
        &mut self,
        _input: Entity<UrlInput>,
        event: &UrlInputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            UrlInputEvent::UrlChanged(url) => {
                self.update_active_request(cx, |request| request.set_url(url));
                if self
                    .view_model
                    .read(cx)
                    .active_request()
                    .is_some_and(|request| request.request_pane() == RequestPane::Params)
                {
                    self.params_pane
                        .update(cx, KeyValueRowsPane::project_active_request);
                }
            }
            UrlInputEvent::SubmitRequested => self.click_send(cx),
        }
    }

    fn on_key_value_pane_event(
        &mut self,
        _pane: Entity<KeyValueRowsPane>,
        event: &KeyValueRowsPaneEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            KeyValueRowsPaneEvent::EffectiveUrlChanged => self.project_url(cx),
        }
    }

    fn project_method(&self, cx: &mut Context<Self>) {
        let Some(method) = self
            .view_model
            .read(cx)
            .active_request()
            .map(RequestViewModel::method)
        else {
            return;
        };
        self.method_selector
            .update(cx, |selector, cx| selector.project_method(method, cx));
    }

    fn project_url(&self, cx: &mut Context<Self>) {
        let url = self
            .view_model
            .read(cx)
            .active_request()
            .map(|request| request.url().to_string())
            .unwrap_or_default();
        self.url_input
            .update(cx, |input, cx| input.project_url(url, cx));
    }

    fn project_selected_pane(&self, cx: &mut Context<Self>) {
        let Some(request_pane) = self
            .view_model
            .read(cx)
            .active_request()
            .map(RequestViewModel::request_pane)
        else {
            return;
        };
        match request_pane {
            RequestPane::Params => self
                .params_pane
                .update(cx, KeyValueRowsPane::project_active_request),
            RequestPane::Headers => self
                .headers_pane
                .update(cx, KeyValueRowsPane::project_active_request),
            RequestPane::Authorization => self
                .authorization_pane
                .update(cx, AuthorizationPane::project_active_request),
            RequestPane::Body => self.body_pane.update(cx, BodyPane::project_active_request),
            RequestPane::Scripts => self
                .script_pane
                .update(cx, ScriptPane::project_active_request),
            RequestPane::Tests => self
                .tests_pane
                .update(cx, ScriptPane::project_active_request),
            RequestPane::Options => self
                .options_pane
                .update(cx, OptionsPane::project_active_request),
        }
    }

    /// Full projection happens only when the active request changes. Ordinary edits notify the
    /// pane that owns the control; inactive panes are projected lazily when selected.
    pub(super) fn project_active_request(&mut self, cx: &mut Context<Self>) {
        self.project_method(cx);
        self.project_url(cx);
        self.params_pane
            .update(cx, KeyValueRowsPane::project_active_request);
        self.headers_pane
            .update(cx, KeyValueRowsPane::project_active_request);
        self.authorization_pane
            .update(cx, AuthorizationPane::project_active_request);
        self.body_pane.update(cx, BodyPane::project_active_request);
        self.script_pane
            .update(cx, ScriptPane::project_active_request);
        self.tests_pane
            .update(cx, ScriptPane::project_active_request);
        self.options_pane
            .update(cx, OptionsPane::project_active_request);
        cx.notify();
    }

    pub(super) fn click_send(&mut self, cx: &mut Context<Self>) {
        if let Some(send_id) = self.view_model.read(cx).active_send_id() {
            self.cancel_send(send_id, cx);
            return;
        }

        let Some(pending) = self.update_view_model(cx, WorkspaceViewModel::begin_send) else {
            return;
        };
        self.authorization_pane
            .update(cx, AuthorizationPane::project_active_request);
        cx.emit(RequestComposerEvent::Execute(pending));
    }

    pub(super) fn send_or_cancel(&mut self, cx: &mut Context<Self>) {
        self.click_send(cx);
    }

    pub(super) fn focus_url(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.url_input.read(cx).focus_handle(cx).focus(window, cx);
    }

    fn cancel_send(&mut self, send_id: SendId, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.cancel_send(send_id));
        cx.emit(RequestComposerEvent::Abort(send_id));
    }

    pub(super) fn on_send_clicked(
        &mut self,
        _event: &gpui::MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.click_send(cx);
    }

    pub(super) fn set_request_pane(&mut self, pane: RequestPane, cx: &mut Context<Self>) {
        self.update_active_request(cx, |request| request.set_request_pane(pane));
        self.project_selected_pane(cx);
    }

    fn request_pane_and_visible_rows(&self, cx: &App) -> Option<(RequestPane, usize)> {
        let view_model = self.view_model.read(cx);
        let request = view_model.active_request()?;
        let request_pane = request.request_pane();
        let visible_rows = match request_pane {
            RequestPane::Params => request.visible_param_row_count(),
            RequestPane::Headers => request.visible_header_row_count(),
            RequestPane::Body
                if matches!(
                    request.body_kind(),
                    BodyKind::UrlEncoded | BodyKind::Multipart
                ) =>
            {
                let body_input = self.body_pane.read(cx).input_entity();
                body_input.read(cx).form_data_entry_count()
            }
            RequestPane::Authorization
            | RequestPane::Scripts
            | RequestPane::Tests
            | RequestPane::Options
            | RequestPane::Body => 0,
        };
        Some((request_pane, visible_rows))
    }

    pub(super) fn request_panel_height(&self, window: &Window, cx: &App) -> Option<f32> {
        let (request_pane, visible_rows) = self.request_pane_and_visible_rows(cx)?;
        Some(self.panel_layout.read(cx).resolved_height(
            request_pane,
            visible_rows,
            window.viewport_size().height.as_f32(),
        ))
    }

    fn render_request_panel(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some((request_pane, _)) = self.request_pane_and_visible_rows(cx) else {
            return div().into_any_element();
        };
        let panel_height = self
            .request_panel_height(window, cx)
            .expect("the active request pane was resolved above");
        let editor = match request_pane {
            RequestPane::Params => self.params_pane.clone().into_any_element(),
            RequestPane::Authorization => self.authorization_pane.clone().into_any_element(),
            RequestPane::Headers => self.headers_pane.clone().into_any_element(),
            RequestPane::Body => self.body_pane.clone().into_any_element(),
            RequestPane::Scripts => self.script_pane.clone().into_any_element(),
            RequestPane::Tests => self.tests_pane.clone().into_any_element(),
            RequestPane::Options => self.options_pane.clone().into_any_element(),
        };

        div()
            .debug_selector(|| "request-panel".into())
            .h(px(panel_height))
            .flex_none()
            .flex()
            .flex_col()
            .min_w_0()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(LINE))
            .rounded(px(14.0))
            .overflow_hidden()
            .child(self.render_request_menu(window, cx))
            .child(editor)
            .into_any_element()
    }
}

impl Render for RequestComposer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(REQUEST_COMPOSER_GAP))
            .child(self.render_request_head(window, cx))
            .child(self.render_request_panel(window, cx))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        app::{PostmanApp, ResponseState, WorkspaceViewModel},
        models::HttpMethod,
    };
    use gpui::{AppContext, TestAppContext};
    use mockito::Matcher;

    #[gpui::test]
    fn send_command_is_built_only_from_the_view_model(cx: &mut TestAppContext) {
        let mut server = mockito::Server::new();
        let expected_body = r#"{"source":"view-model"}"#;
        let request = server
            .mock("POST", "/single-source")
            .match_body(Matcher::Exact(expected_body.to_string()))
            .with_status(200)
            .with_body("single-source-ok")
            .create();
        let workspace = cx.new(|_| WorkspaceViewModel::new());
        workspace.update(cx, |workspace, _| {
            workspace
                .active_request_mut()
                .unwrap()
                .set_method(HttpMethod::POST);
            workspace
                .active_request_mut()
                .unwrap()
                .set_url(format!("{}/single-source", server.url()));
            workspace
                .active_request_mut()
                .unwrap()
                .set_body(expected_body);
        });
        let observed = workspace.clone();
        let (app, cx) =
            cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));
        let request_workspace = app.read_with(cx, |app, _| app.request_workspace.clone());
        let composer = request_workspace.read_with(cx, |workspace, _| workspace.composer.clone());

        composer.update(cx, |composer, cx| {
            composer.method_selector.update(cx, |selector, cx| {
                selector.project_method(HttpMethod::GET, cx)
            });
            composer.url_input.update(cx, |input, cx| {
                input.project_url("http://127.0.0.1:1/stale-control", cx)
            });
            let body_input = composer.body_pane.read(cx).input_entity();
            body_input.update(cx, |input, cx| input.project_content("stale-body", cx));
            composer.click_send(cx);
        });
        cx.run_until_parked();

        assert!(matches!(
            workspace.read_with(cx, |workspace, _| workspace
                .active_request()
                .unwrap()
                .response()
                .clone()),
            ResponseState::Success { status: 200, .. }
        ));
        request.assert();
    }
}

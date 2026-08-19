use crate::{
    app::{
        AuthorizationKind, BodyKind, KeyValueRow, MultipartDraftPart, MultipartDraftValue,
        PendingRequest, RequestPane, SendId, WorkspaceViewModel,
    },
    models::Request,
    ui::{
        components::{
            body_input::{setup_body_input_key_bindings, BodyInput, BodyInputEvent},
            header_input::{setup_header_input_key_bindings, HeaderInput, HeaderInputEvent},
            method_selector::{MethodSelector, MethodSelectorEvent},
            response_viewer::{setup_response_viewer_key_bindings, ResponseViewer},
            url_input::{setup_url_input_key_bindings, UrlInput, UrlInputEvent},
        },
        theme::{BG, LINE, PANEL},
    },
};
use gpui::{
    div, px, rgb, AppContext, Context, Entity, EventEmitter, InteractiveElement, IntoElement,
    ParentElement, Render, ScrollHandle, Styled, Subscription, Window,
};

mod chrome;
mod layout;
mod panes;
mod projection;
mod row_editor;

use layout::adaptive_request_panel_height;
use row_editor::PersistentRowEditor;

#[derive(Clone, Debug)]
pub(super) enum RequestEditorEvent {
    Execute(PendingRequest),
    Abort(SendId),
}

/// Request-workspace child view. It owns editor entities, their subscriptions, and one-way
/// ViewModel projection. HTTP execution remains in `RequestRunner`.
pub(super) struct RequestEditor {
    pub(super) view_model: Entity<WorkspaceViewModel>,
    pub(super) method_selector: Entity<MethodSelector>,
    pub(super) url_input: Entity<UrlInput>,
    body_input: Entity<BodyInput>,
    param_row_editors: Vec<Entity<PersistentRowEditor>>,
    param_row_subscriptions: Vec<Subscription>,
    param_rows_scroll_handle: ScrollHandle,
    header_row_editors: Vec<Entity<PersistentRowEditor>>,
    header_row_subscriptions: Vec<Subscription>,
    header_rows_scroll_handle: ScrollHandle,
    row_key_input: Entity<HeaderInput>,
    row_value_input: Entity<HeaderInput>,
    authorization_input: Entity<HeaderInput>,
    basic_username_input: Entity<HeaderInput>,
    basic_password_input: Entity<HeaderInput>,
    script_input: Entity<BodyInput>,
    tests_input: Entity<BodyInput>,
    response_viewer: Entity<ResponseViewer>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<RequestEditorEvent> for RequestEditor {}

impl RequestEditor {
    pub(super) fn new(view_model: Entity<WorkspaceViewModel>, cx: &mut Context<Self>) -> Self {
        cx.bind_keys(setup_url_input_key_bindings());
        cx.bind_keys(setup_header_input_key_bindings());
        cx.bind_keys(setup_body_input_key_bindings());
        cx.bind_keys(setup_response_viewer_key_bindings());

        let method_selector = cx.new(MethodSelector::new);
        let url_input = cx.new(|cx| UrlInput::new(cx).with_placeholder("Enter request URL"));
        let body_input = cx.new(|cx| {
            BodyInput::new(cx)
                .with_placeholder("Enter request body (JSON, form data, etc.)")
                .with_type_tabs(false)
        });
        let row_key_input = cx.new(|cx| HeaderInput::new(cx).with_placeholder("Key"));
        let row_value_input = cx.new(|cx| HeaderInput::new(cx).with_placeholder("Value"));
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
        let script_input = cx.new(|cx| {
            BodyInput::new(cx)
                .with_placeholder("Pre-request script")
                .with_type_tabs(false)
        });
        let tests_input = cx.new(|cx| {
            BodyInput::new(cx)
                .with_placeholder("Response tests")
                .with_type_tabs(false)
        });
        let response_viewer = cx.new(|cx| ResponseViewer::new(view_model.clone(), cx));

        let subscriptions = vec![
            cx.subscribe(&method_selector, Self::on_method_changed),
            cx.subscribe(&url_input, Self::on_url_event),
            cx.subscribe(&body_input, Self::on_body_event),
            cx.subscribe(&row_key_input, Self::on_row_key_event),
            cx.subscribe(&row_value_input, Self::on_row_value_event),
            cx.subscribe(&authorization_input, Self::on_authorization_event),
            cx.subscribe(&basic_username_input, Self::on_basic_username_event),
            cx.subscribe(&basic_password_input, Self::on_basic_password_event),
            cx.subscribe(&script_input, Self::on_script_event),
            cx.subscribe(&tests_input, Self::on_tests_event),
            cx.observe(&view_model, |_, _, cx| cx.notify()),
        ];

        let mut editor = Self {
            view_model,
            method_selector,
            url_input,
            body_input,
            param_row_editors: Vec::new(),
            param_row_subscriptions: Vec::new(),
            param_rows_scroll_handle: ScrollHandle::new(),
            header_row_editors: Vec::new(),
            header_row_subscriptions: Vec::new(),
            header_rows_scroll_handle: ScrollHandle::new(),
            row_key_input,
            row_value_input,
            authorization_input,
            basic_username_input,
            basic_password_input,
            script_input,
            tests_input,
            response_viewer,
            _subscriptions: subscriptions,
        };
        editor.project_active_request(cx);
        editor
    }

    pub(super) fn update_view_model<R>(
        &self,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut WorkspaceViewModel) -> R,
    ) -> R {
        self.view_model.update(cx, |view_model, cx| {
            let result = update(view_model);
            cx.notify();
            result
        })
    }

    fn on_method_changed(
        &mut self,
        _selector: Entity<MethodSelector>,
        event: &MethodSelectorEvent,
        cx: &mut Context<Self>,
    ) {
        let MethodSelectorEvent::MethodChanged(method) = event;
        self.update_view_model(cx, |view_model| view_model.set_method(*method));
        self.project_body(cx);
    }

    fn on_url_event(
        &mut self,
        _input: Entity<UrlInput>,
        event: &UrlInputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            UrlInputEvent::UrlChanged(url) => {
                self.update_view_model(cx, |view_model| view_model.set_url(url));
                self.rebuild_param_row_editors(cx);
                self.project_row_draft(cx);
            }
            UrlInputEvent::SubmitRequested => self.click_send(cx),
        }
    }

    fn on_body_event(
        &mut self,
        _input: Entity<BodyInput>,
        event: &BodyInputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            BodyInputEvent::ValueChanged(value) => {
                self.update_view_model(cx, |view_model| view_model.set_body(value));
            }
            BodyInputEvent::FormDataChanged(entries) => {
                let entries = entries.clone();
                self.update_view_model(cx, |view_model| match view_model.body_kind() {
                    BodyKind::UrlEncoded => {
                        view_model.set_url_encoded_rows(
                            entries
                                .into_iter()
                                .map(|entry| KeyValueRow {
                                    enabled: entry.enabled,
                                    key: entry.key,
                                    value: entry.value,
                                })
                                .collect(),
                        );
                    }
                    BodyKind::Multipart => {
                        let parts = entries
                            .into_iter()
                            .map(|entry| {
                                let value = match entry.file {
                                    Some(file) => MultipartDraftValue::File {
                                        path: file.path,
                                        file_name: file.file_name,
                                        content_type: file.content_type,
                                    },
                                    None => MultipartDraftValue::Text(entry.value),
                                };
                                MultipartDraftPart {
                                    enabled: entry.enabled,
                                    name: entry.key,
                                    value,
                                }
                            })
                            .collect();
                        view_model.set_multipart_draft_parts(parts);
                    }
                    BodyKind::None | BodyKind::Json | BodyKind::Raw => {}
                });
            }
        }
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

    fn on_script_event(
        &mut self,
        _input: Entity<BodyInput>,
        event: &BodyInputEvent,
        cx: &mut Context<Self>,
    ) {
        if let BodyInputEvent::ValueChanged(script) = event {
            self.update_view_model(cx, |view_model| view_model.set_pre_request_script(script));
        }
    }

    fn on_tests_event(
        &mut self,
        _input: Entity<BodyInput>,
        event: &BodyInputEvent,
        cx: &mut Context<Self>,
    ) {
        if let BodyInputEvent::ValueChanged(script) = event {
            self.update_view_model(cx, |view_model| view_model.set_tests_script(script));
        }
    }

    fn click_send(&mut self, cx: &mut Context<Self>) {
        if let Some(send_id) = self.view_model.read(cx).active_send_id() {
            self.cancel_send(send_id, cx);
            return;
        }

        let pending = self.update_view_model(cx, WorkspaceViewModel::begin_send);
        self.project_authorization(cx);
        cx.emit(RequestEditorEvent::Execute(pending));
    }

    fn cancel_send(&mut self, send_id: SendId, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.cancel_send(send_id));
        cx.emit(RequestEditorEvent::Abort(send_id));
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
        self.update_view_model(cx, |view_model| view_model.set_request_pane(pane));
        match pane {
            RequestPane::Params => self.rebuild_param_row_editors(cx),
            RequestPane::Headers => self.rebuild_header_row_editors(cx),
            RequestPane::Authorization
            | RequestPane::Body
            | RequestPane::Scripts
            | RequestPane::Tests => {}
        }
        self.project_row_draft(cx);
    }

    pub(super) fn set_authorization_kind(
        &mut self,
        kind: AuthorizationKind,
        cx: &mut Context<Self>,
    ) {
        self.update_view_model(cx, |view_model| view_model.set_authorization_kind(kind));
    }

    pub(super) fn set_body_kind(&mut self, kind: BodyKind, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| {
            let current = view_model.body_kind();
            let current_is_form = matches!(current, BodyKind::UrlEncoded | BodyKind::Multipart);
            let next_is_form = matches!(kind, BodyKind::UrlEncoded | BodyKind::Multipart);
            if current != kind && current_is_form != next_is_form {
                view_model.clear_body();
            }
            view_model.set_body_kind(kind);
        });
        self.project_body(cx);
    }

    pub(super) fn use_sample_json(&mut self, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| {
            view_model.set_body_kind(BodyKind::Json);
            view_model.set_body(
                r#"{
  "name": "Ada Lovelace",
  "email": "ada@example.com",
  "active": true
}"#,
            );
        });
        self.project_body(cx);
    }

    pub(super) fn clear_body(&mut self, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.clear_body());
        self.project_body(cx);
    }

    pub(super) fn new_request(&mut self, cx: &mut Context<Self>) {
        self.update_view_model(cx, WorkspaceViewModel::new_request);
        self.project_active_request(cx);
    }

    pub(super) fn select_request_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.update_view_model(cx, |view_model| view_model.select_tab(index)) {
            self.project_active_request(cx);
        }
    }

    pub(super) fn close_request_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(send_id) = self.view_model.read(cx).send_id_for_tab(index) {
            self.cancel_send(send_id, cx);
        }
        if self.update_view_model(cx, |view_model| view_model.close_tab(index)) {
            self.project_active_request(cx);
        }
    }

    pub(super) fn load_request(&mut self, request: &Request, cx: &mut Context<Self>) {
        if let Some(send_id) = self.view_model.read(cx).active_send_id() {
            self.cancel_send(send_id, cx);
        }
        self.update_view_model(cx, |view_model| view_model.load_request(request));
        self.project_active_request(cx);
    }

    pub(super) fn render_request_panel(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let request_pane = self.view_model.read(cx).request_pane();
        let visible_rows = match request_pane {
            RequestPane::Params => self.view_model.read(cx).visible_param_row_count(),
            RequestPane::Headers => self.view_model.read(cx).visible_header_row_count(),
            RequestPane::Body if self.view_model.read(cx).body_kind() == BodyKind::UrlEncoded => {
                self.body_input.read(cx).form_data_entry_count()
            }
            RequestPane::Authorization
            | RequestPane::Scripts
            | RequestPane::Tests
            | RequestPane::Body => 0,
        };
        let panel_height = adaptive_request_panel_height(
            request_pane,
            visible_rows,
            window.viewport_size().height.as_f32(),
        );
        let editor = match request_pane {
            RequestPane::Params => self.render_params_editor(panel_height, cx),
            RequestPane::Authorization => self.render_authorization_editor(cx),
            RequestPane::Headers => self.render_headers_editor(panel_height, cx),
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
            .child(self.render_request_menu(cx))
            .child(editor)
    }
}
impl Render for RequestEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .child(self.render_request_tabs_bar(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_3()
                    .child(self.render_request_head(cx))
                    .child(self.render_request_panel(window, cx))
                    .child(
                        div()
                            .id("response-container")
                            .debug_selector(|| "response-container".into())
                            .flex_1()
                            .min_h_0()
                            .child(self.response_viewer.clone()),
                    ),
            )
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
            workspace.set_method(HttpMethod::POST);
            workspace.set_url(format!("{}/single-source", server.url()));
            workspace.set_body(expected_body);
        });
        let observed = workspace.clone();
        let (app, cx) =
            cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));
        let editor = app.read_with(cx, |app, _| app.request_editor.clone());

        editor.update(cx, |editor, cx| {
            editor.method_selector.update(cx, |selector, cx| {
                selector.project_method(HttpMethod::GET, cx)
            });
            editor.url_input.update(cx, |input, cx| {
                input.project_url("http://127.0.0.1:1/stale-control", cx)
            });
            editor
                .body_input
                .update(cx, |input, cx| input.project_content("stale-body", cx));
            editor.click_send(cx);
        });
        cx.run_until_parked();

        assert!(matches!(
            workspace.read_with(cx, |workspace, _| workspace.response().clone()),
            ResponseState::Success { status: 200, .. }
        ));
        request.assert();
    }
}

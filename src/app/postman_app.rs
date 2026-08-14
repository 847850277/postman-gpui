use crate::{
    app::{
        view_model::detect_body_kind, AuthorizationKind, BodyKind, KeyValueRow, RequestPane,
        ResponseState, SendId, WorkspaceViewModel,
    },
    http::executor::RequestExecutor,
    models::{HttpMethod, Request},
    ui::{
        components::{
            body_input::{
                setup_body_input_key_bindings, BodyInput, BodyInputEvent, BodyType, FormDataEntry,
            },
            header_input::{setup_header_input_key_bindings, HeaderInput, HeaderInputEvent},
            history_list::{HistoryList, HistoryListEvent},
            method_selector::{MethodSelector, MethodSelectorEvent},
            response_viewer::{setup_response_viewer_key_bindings, ResponseViewer},
            url_input::{setup_url_input_key_bindings, UrlInput, UrlInputEvent},
        },
        theme::{
            ACCENT, ACCENT_DARK, ACCENT_SOFT, BG, CODE_BG, CODE_PANEL, CODE_TEXT, ERROR,
            FONT_HEADING, FONT_MONO, FONT_UI, LINE, MUTED, PANEL, PANEL_ALT, SUBTEXT, TEXT,
        },
    },
};
use gpui::{
    div, prelude::FluentBuilder, px, rgb, App, AppContext, Context, Entity, FontWeight,
    InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled,
    Subscription, Window,
};
use std::collections::HashMap;

mod chrome;
mod request_editor;

pub struct PostmanApp {
    view_model: Entity<WorkspaceViewModel>,
    executor: RequestExecutor,
    method_selector: Entity<MethodSelector>,
    url_input: Entity<UrlInput>,
    body_input: Entity<BodyInput>,
    row_key_input: Entity<HeaderInput>,
    row_value_input: Entity<HeaderInput>,
    authorization_input: Entity<HeaderInput>,
    basic_username_input: Entity<HeaderInput>,
    basic_password_input: Entity<HeaderInput>,
    script_input: Entity<BodyInput>,
    tests_input: Entity<BodyInput>,
    history_list: Entity<HistoryList>,
    response_viewer: Entity<ResponseViewer>,
    in_flight: HashMap<SendId, tokio::task::AbortHandle>,
    _subscriptions: Vec<Subscription>,
}

impl PostmanApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.bind_keys(setup_url_input_key_bindings());
        cx.bind_keys(setup_header_input_key_bindings());
        cx.bind_keys(setup_body_input_key_bindings());
        cx.bind_keys(setup_response_viewer_key_bindings());

        let view_model = cx.new(|_| WorkspaceViewModel::new());
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
            cx.new(|cx| HeaderInput::new(cx).with_placeholder("Enter bearer token"));
        let basic_username_input = cx.new(|cx| HeaderInput::new(cx).with_placeholder("Username"));
        let basic_password_input = cx.new(|cx| {
            HeaderInput::new(cx)
                .with_placeholder("Password")
                .with_masked(true)
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
        let history_list = cx.new(|cx| HistoryList::new(view_model.clone(), cx));
        let response_viewer = cx.new(|cx| ResponseViewer::new(view_model.clone(), cx));

        let subscriptions = vec![
            cx.subscribe(&method_selector, Self::on_method_changed),
            cx.subscribe(&url_input, Self::on_url_event),
            cx.subscribe(&body_input, Self::on_body_event),
            cx.subscribe(&row_key_input, Self::on_row_input_event),
            cx.subscribe(&row_value_input, Self::on_row_input_event),
            cx.subscribe(&authorization_input, Self::on_authorization_event),
            cx.subscribe(&basic_username_input, Self::on_basic_username_event),
            cx.subscribe(&basic_password_input, Self::on_basic_password_event),
            cx.subscribe(&script_input, Self::on_script_event),
            cx.subscribe(&tests_input, Self::on_tests_event),
            cx.subscribe(&history_list, Self::on_history_selected),
            cx.observe(&view_model, |_, _, cx| cx.notify()),
        ];

        Self {
            view_model,
            executor: RequestExecutor::new(),
            method_selector,
            url_input,
            body_input,
            row_key_input,
            row_value_input,
            authorization_input,
            basic_username_input,
            basic_password_input,
            script_input,
            tests_input,
            history_list,
            response_viewer,
            in_flight: HashMap::new(),
            _subscriptions: subscriptions,
        }
    }

    fn update_view_model<R>(
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
        let BodyInputEvent::ValueChanged(value) = event;
        self.update_view_model(cx, |view_model| view_model.set_body(value));
    }

    fn on_row_input_event(
        &mut self,
        _input: Entity<HeaderInput>,
        event: &HeaderInputEvent,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, HeaderInputEvent::SubmitRequested) {
            self.add_current_row(cx);
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
        let BodyInputEvent::ValueChanged(script) = event;
        self.update_view_model(cx, |view_model| view_model.set_pre_request_script(script));
    }

    fn on_tests_event(
        &mut self,
        _input: Entity<BodyInput>,
        event: &BodyInputEvent,
        cx: &mut Context<Self>,
    ) {
        let BodyInputEvent::ValueChanged(script) = event;
        self.update_view_model(cx, |view_model| view_model.set_tests_script(script));
    }

    fn on_history_selected(
        &mut self,
        _list: Entity<HistoryList>,
        event: &HistoryListEvent,
        cx: &mut Context<Self>,
    ) {
        let HistoryListEvent::RequestSelected(request) = event;
        if let Some(send_id) = self.view_model.read(cx).active_send_id() {
            self.cancel_send(send_id, cx);
        }
        self.update_view_model(cx, |view_model| view_model.load_request(request));
        self.project_active_request(cx);
    }

    pub fn type_url(&mut self, url: &str, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.set_url(url));
        self.project_url(cx);
    }

    pub fn set_body(&mut self, body: &str, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| {
            view_model.set_body(body);
            view_model.set_body_kind(detect_body_kind(body));
        });
        self.project_body(cx);
    }

    pub fn choose_method(&mut self, method: HttpMethod, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.set_method(method));
        self.project_method(cx);
        self.project_body(cx);
    }

    pub fn click_send(&mut self, cx: &mut Context<Self>) {
        if let Some(send_id) = self.view_model.read(cx).active_send_id() {
            self.cancel_send(send_id, cx);
            return;
        }

        let pending = self.update_view_model(cx, WorkspaceViewModel::begin_send);
        self.project_authorization(cx);
        let send_id = pending.send_id();
        let (abort_handle, request_task) = self.executor.spawn_request(pending.request().clone());
        self.in_flight.insert(send_id, abort_handle);
        let executor = self.executor.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { executor.wait_for_request(request_task) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.in_flight.remove(&send_id);
                this.update_view_model(cx, |view_model| view_model.complete_send(pending, result));
            });
        })
        .detach();
    }

    fn cancel_send(&mut self, send_id: SendId, cx: &mut Context<Self>) {
        if let Some(handle) = self.in_flight.remove(&send_id) {
            handle.abort();
        }
        self.update_view_model(cx, |view_model| view_model.cancel_send(send_id));
    }

    pub fn response_state(&self, cx: &App) -> ResponseState {
        self.view_model.read(cx).response().clone()
    }

    pub fn history_len(&self, cx: &App) -> usize {
        self.view_model.read(cx).history_len()
    }

    pub fn latest_history_request(&self, cx: &App) -> Option<Request> {
        self.view_model
            .read(cx)
            .history()
            .first()
            .map(|entry| entry.request.clone())
    }

    pub fn visible_history_len(&self, cx: &App) -> usize {
        self.history_list.read(cx).visible_entry_count(cx)
    }

    pub fn tab_count(&self, cx: &App) -> usize {
        self.view_model.read(cx).tab_count()
    }

    pub fn active_tab_index(&self, cx: &App) -> usize {
        self.view_model.read(cx).active_tab_index()
    }

    pub fn current_url(&self, cx: &App) -> String {
        self.view_model.read(cx).url().to_string()
    }

    pub fn current_params(&self, cx: &App) -> Vec<KeyValueRow> {
        self.view_model.read(cx).params().to_vec()
    }

    pub fn current_headers(&self, cx: &App) -> Vec<KeyValueRow> {
        self.view_model.read(cx).headers().to_vec()
    }

    pub fn current_bearer_token(&self, cx: &App) -> String {
        self.view_model.read(cx).bearer_token().to_string()
    }

    pub fn current_authorization_kind(&self, cx: &App) -> AuthorizationKind {
        self.view_model.read(cx).authorization_kind()
    }

    pub fn current_basic_username(&self, cx: &App) -> String {
        self.view_model.read(cx).basic_username().to_string()
    }

    pub fn current_basic_password(&self, cx: &App) -> String {
        self.view_model.read(cx).basic_password().to_string()
    }

    pub fn current_pre_request_script(&self, cx: &App) -> String {
        self.view_model.read(cx).pre_request_script().to_string()
    }

    pub fn current_tests_script(&self, cx: &App) -> String {
        self.view_model.read(cx).tests_script().to_string()
    }

    pub fn set_bearer_token(&mut self, token: &str, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.set_bearer_token(token));
        self.project_authorization(cx);
    }

    fn set_authorization_kind(&mut self, kind: AuthorizationKind, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.set_authorization_kind(kind));
    }

    pub fn set_pre_request_script(&mut self, script: &str, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.set_pre_request_script(script));
        self.project_scripts(cx);
    }

    pub fn set_tests_script(&mut self, script: &str, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.set_tests_script(script));
        self.project_scripts(cx);
    }

    fn on_send_clicked(
        &mut self,
        _event: &gpui::MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.click_send(cx);
    }

    fn set_request_pane(&mut self, pane: RequestPane, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.set_request_pane(pane));
    }

    fn add_current_row(&mut self, cx: &mut Context<Self>) {
        let key = self.row_key_input.read(cx).get_content().trim().to_string();
        let value = self
            .row_value_input
            .read(cx)
            .get_content()
            .trim()
            .to_string();
        let request_pane = self.view_model.read(cx).request_pane();
        match request_pane {
            RequestPane::Params => {
                self.update_view_model(cx, |view_model| view_model.upsert_param(key, value));
                self.project_url(cx);
            }
            RequestPane::Headers => {
                self.update_view_model(cx, |view_model| view_model.upsert_header(key, value));
            }
            RequestPane::Authorization
            | RequestPane::Body
            | RequestPane::Scripts
            | RequestPane::Tests => return,
        }
        self.row_key_input.update(cx, |input, cx| input.clear(cx));
        self.row_value_input.update(cx, |input, cx| input.clear(cx));
    }

    fn toggle_param(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.toggle_param(index));
        self.project_url(cx);
    }

    fn remove_param(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.remove_param(index));
        self.project_url(cx);
    }

    fn toggle_header(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.toggle_header(index));
    }

    fn remove_header(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.remove_header(index));
    }

    fn set_header_input_values(&mut self, key: &str, value: &str, cx: &mut Context<Self>) {
        self.row_key_input
            .update(cx, |input, cx| input.set_content(key.to_string(), cx));
        self.row_value_input
            .update(cx, |input, cx| input.set_content(value.to_string(), cx));
    }

    fn set_body_kind(&mut self, kind: BodyKind, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.set_body_kind(kind));
        self.project_body(cx);
    }

    fn use_sample_json(&mut self, cx: &mut Context<Self>) {
        self.set_body(
            r#"{
  "name": "Ada Lovelace",
  "email": "ada@example.com",
  "active": true
}"#,
            cx,
        );
    }

    fn clear_body(&mut self, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.set_body(""));
        self.project_body(cx);
    }

    fn new_request(&mut self, cx: &mut Context<Self>) {
        self.update_view_model(cx, WorkspaceViewModel::new_request);
        self.clear_staged_row(cx);
        self.project_active_request(cx);
    }

    fn select_request_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.update_view_model(cx, |view_model| view_model.select_tab(index)) {
            self.clear_staged_row(cx);
            self.project_active_request(cx);
        }
    }

    fn close_request_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(send_id) = self.view_model.read(cx).send_id_for_tab(index) {
            self.cancel_send(send_id, cx);
        }
        if self.update_view_model(cx, |view_model| view_model.close_tab(index)) {
            self.clear_staged_row(cx);
            self.project_active_request(cx);
        }
    }

    fn clear_staged_row(&self, cx: &mut Context<Self>) {
        self.row_key_input.update(cx, |input, cx| input.clear(cx));
        self.row_value_input.update(cx, |input, cx| input.clear(cx));
    }

    /// One-way VM -> editor projection. Editor buffers retain cursor/selection state, but they
    /// never participate in request construction.
    fn project_active_request(&self, cx: &mut Context<Self>) {
        self.project_method(cx);
        self.project_url(cx);
        self.project_body(cx);
        self.project_authorization(cx);
        self.project_scripts(cx);
    }

    fn project_method(&self, cx: &mut Context<Self>) {
        let method = self.view_model.read(cx).method();
        self.method_selector
            .update(cx, |selector, cx| selector.project_method(method, cx));
    }

    fn project_url(&self, cx: &mut Context<Self>) {
        let url = self.view_model.read(cx).url().to_string();
        self.url_input
            .update(cx, |input, cx| input.project_url(url, cx));
    }

    fn project_body(&self, cx: &mut Context<Self>) {
        let (body, body_form_rows, body_kind) = {
            let view_model = self.view_model.read(cx);
            (
                view_model.body().to_string(),
                view_model.body_form_rows(),
                view_model.body_kind(),
            )
        };
        self.body_input.update(cx, |input, cx| {
            input.set_type_silent(body_type_from_kind(body_kind), cx);
            if matches!(body_kind, BodyKind::UrlEncoded | BodyKind::Multipart) {
                project_form_rows(input, &body_form_rows, cx);
            } else {
                input.project_content(body, cx);
            }
        });
    }

    fn project_authorization(&self, cx: &mut Context<Self>) {
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
    }

    fn project_scripts(&self, cx: &mut Context<Self>) {
        let (pre_request_script, tests_script) = {
            let view_model = self.view_model.read(cx);
            (
                view_model.pre_request_script().to_string(),
                view_model.tests_script().to_string(),
            )
        };
        self.script_input.update(cx, |input, cx| {
            input.set_type_silent(BodyType::Json, cx);
            input.project_content(pre_request_script, cx);
        });

        self.tests_input.update(cx, |input, cx| {
            input.set_type_silent(BodyType::Json, cx);
            input.project_content(tests_script, cx);
        });
    }
}

impl Drop for PostmanApp {
    fn drop(&mut self) {
        for (_, handle) in self.in_flight.drain() {
            handle.abort();
        }
    }
}

impl Render for PostmanApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("main-container")
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .font_family(FONT_UI)
            .child(self.render_top_header())
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.render_left_rail())
                    .child(self.history_list.clone())
                    .child(
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
                                    .child(self.render_request_panel(cx))
                                    .child(
                                        div()
                                            .id("response-container")
                                            .debug_selector(|| "response-container".into())
                                            .flex_1()
                                            .min_h_0()
                                            .child(self.response_viewer.clone()),
                                    ),
                            ),
                    ),
            )
    }
}

fn body_type_from_kind(kind: BodyKind) -> BodyType {
    match kind {
        BodyKind::Json => BodyType::Json,
        BodyKind::UrlEncoded | BodyKind::Multipart => BodyType::FormData,
        BodyKind::None | BodyKind::Raw => BodyType::Raw,
    }
}

fn project_form_rows(input: &mut BodyInput, rows: &[KeyValueRow], cx: &mut Context<BodyInput>) {
    let mut entries: Vec<FormDataEntry> = rows
        .iter()
        .map(|row| FormDataEntry {
            key: row.key.clone(),
            value: row.value.clone(),
            enabled: row.enabled,
        })
        .collect();
    if entries.is_empty() {
        entries.push(FormDataEntry {
            key: String::new(),
            value: String::new(),
            enabled: true,
        });
    }
    input.project_form_data_entries(entries, cx);
}

fn method_color(method: HttpMethod) -> u32 {
    match method {
        HttpMethod::GET => 0x0016_a34a,
        HttpMethod::POST => ACCENT,
        HttpMethod::PUT => 0x0025_63eb,
        HttpMethod::DELETE => ERROR,
        HttpMethod::PATCH => 0x007c_3aed,
        HttpMethod::HEAD | HttpMethod::OPTIONS => SUBTEXT,
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use mockito::Matcher;

    #[gpui::test]
    fn send_uses_view_model_when_editor_buffers_are_stale(cx: &mut TestAppContext) {
        let mut server = mockito::Server::new();
        let expected_body = r#"{"source":"view-model"}"#;
        let request = server
            .mock("POST", "/single-source")
            .match_body(Matcher::Exact(expected_body.to_string()))
            .with_status(200)
            .with_body("single-source-ok")
            .create();
        let url = format!("{}/single-source", server.url());
        let (app, cx) = cx.add_window_view(|_window, cx| PostmanApp::new(cx));

        app.update(cx, |app, cx| {
            app.choose_method(HttpMethod::POST, cx);
            app.type_url(&url, cx);
            app.set_body(expected_body, cx);
        });

        app.update(cx, |app, cx| {
            app.method_selector.update(cx, |selector, cx| {
                selector.project_method(HttpMethod::GET, cx)
            });
            app.url_input.update(cx, |input, cx| {
                input.project_url("http://127.0.0.1:1/stale-control", cx)
            });
            app.body_input
                .update(cx, |input, cx| input.project_content("stale-body", cx));

            assert_eq!(
                app.url_input.read(cx).editor_buffer(),
                "http://127.0.0.1:1/stale-control"
            );
            assert_eq!(app.body_input.read(cx).editor_buffer(), "stale-body");

            app.click_send(cx);
        });
        cx.run_until_parked();

        assert!(matches!(
            app.read_with(cx, |app, cx| app.response_state(cx)),
            ResponseState::Success { status: 200, .. }
        ));
        request.assert();
    }

    #[gpui::test]
    fn active_tab_is_projected_into_editor_buffers(cx: &mut TestAppContext) {
        let (app, cx) = cx.add_window_view(|_window, cx| PostmanApp::new(cx));

        app.update(cx, |app, cx| {
            app.type_url("https://first.example/users", cx);
            app.set_body(r#"{"tab":1}"#, cx);
        });
        app.update(cx, |app, cx| app.new_request(cx));
        app.update(cx, |app, cx| {
            app.type_url("https://second.example/orders", cx);
            app.set_body(r#"{"tab":2}"#, cx);
        });

        let second_projection = app.read_with(cx, |app, cx| {
            (
                app.current_url(cx),
                app.url_input.read(cx).editor_buffer().to_string(),
                app.body_input.read(cx).editor_buffer(),
            )
        });
        assert_eq!(
            second_projection,
            (
                "https://second.example/orders".to_string(),
                "https://second.example/orders".to_string(),
                r#"{"tab":2}"#.to_string(),
            )
        );

        app.update(cx, |app, cx| app.select_request_tab(0, cx));
        let first_projection = app.read_with(cx, |app, cx| {
            (
                app.current_url(cx),
                app.url_input.read(cx).editor_buffer().to_string(),
                app.body_input.read(cx).editor_buffer(),
            )
        });
        assert_eq!(
            first_projection,
            (
                "https://first.example/users".to_string(),
                "https://first.example/users".to_string(),
                r#"{"tab":1}"#.to_string(),
            )
        );
    }
}

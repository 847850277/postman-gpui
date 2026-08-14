use super::*;

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

        let editor = Self {
            view_model,
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
            response_viewer,
            _subscriptions: subscriptions,
        };
        editor.project_active_request(cx);
        editor
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
                        let mut serializer = form_urlencoded::Serializer::new(String::new());
                        for entry in entries
                            .iter()
                            .filter(|entry| entry.enabled && !entry.key.is_empty())
                        {
                            serializer.append_pair(&entry.key, &entry.value);
                        }
                        view_model.set_body(serializer.finish());
                    }
                    BodyKind::Multipart => {
                        let parts = entries
                            .into_iter()
                            .filter(|entry| entry.enabled && !entry.key.is_empty())
                            .filter_map(|entry| {
                                let value = match entry.file {
                                    Some(file) if !file.path.as_os_str().is_empty() => {
                                        MultipartValue::File {
                                            path: file.path,
                                            file_name: file.file_name,
                                            content_type: file.content_type,
                                        }
                                    }
                                    Some(_) => return None,
                                    None => MultipartValue::Text(entry.value),
                                };
                                Some(MultipartPart {
                                    name: entry.key,
                                    value,
                                })
                            })
                            .collect();
                        view_model.set_multipart_parts(parts);
                    }
                    BodyKind::None | BodyKind::Json | BodyKind::Raw => {}
                });
            }
        }
    }

    fn on_row_key_event(
        &mut self,
        _input: Entity<HeaderInput>,
        event: &HeaderInputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            HeaderInputEvent::ValueChanged(key) => {
                let pane = self.view_model.read(cx).request_pane();
                self.update_view_model(cx, |view_model| view_model.set_row_draft_key(pane, key));
                if pane == RequestPane::Params {
                    self.project_url(cx);
                }
            }
            HeaderInputEvent::SubmitRequested => self.add_current_row(cx),
        }
    }

    fn on_row_value_event(
        &mut self,
        _input: Entity<HeaderInput>,
        event: &HeaderInputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            HeaderInputEvent::ValueChanged(value) => {
                let pane = self.view_model.read(cx).request_pane();
                self.update_view_model(cx, |view_model| {
                    view_model.set_row_draft_value(pane, value)
                });
                if pane == RequestPane::Params {
                    self.project_url(cx);
                }
            }
            HeaderInputEvent::SubmitRequested => self.add_current_row(cx),
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
        self.project_row_draft(cx);
    }

    fn add_current_row(&mut self, cx: &mut Context<Self>) {
        let request_pane = self.view_model.read(cx).request_pane();
        match request_pane {
            RequestPane::Params => {
                self.update_view_model(cx, |view_model| {
                    view_model.commit_row_draft(RequestPane::Params)
                });
                self.project_url(cx);
            }
            RequestPane::Headers => {
                self.update_view_model(cx, |view_model| {
                    view_model.commit_row_draft(RequestPane::Headers)
                });
            }
            RequestPane::Authorization
            | RequestPane::Body
            | RequestPane::Scripts
            | RequestPane::Tests => return,
        }
        self.project_row_draft(cx);
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

    fn set_authorization_kind(&mut self, kind: AuthorizationKind, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.set_authorization_kind(kind));
    }

    fn set_body_kind(&mut self, kind: BodyKind, cx: &mut Context<Self>) {
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

    fn use_sample_json(&mut self, cx: &mut Context<Self>) {
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

    fn clear_body(&mut self, cx: &mut Context<Self>) {
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

    /// One-way VM -> editor projection. Editor buffers retain cursor/selection state, but they
    /// never participate in request construction.
    fn project_active_request(&self, cx: &mut Context<Self>) {
        self.project_method(cx);
        self.project_url(cx);
        self.project_row_draft(cx);
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

    fn project_row_draft(&self, cx: &mut Context<Self>) {
        let (key, value) = {
            let view_model = self.view_model.read(cx);
            view_model
                .row_draft(view_model.request_pane())
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .unwrap_or_default()
        };
        self.row_key_input
            .update(cx, |input, cx| input.project_content(key, cx));
        self.row_value_input
            .update(cx, |input, cx| input.project_content(value, cx));
    }

    fn project_body(&self, cx: &mut Context<Self>) {
        let (body, body_kind) = {
            let view_model = self.view_model.read(cx);
            (view_model.request_body().clone(), view_model.body_kind())
        };
        self.body_input.update(cx, |input, cx| {
            input.set_type_silent(body_type_from_kind(body_kind), cx);
            input.set_form_data_allows_files(body_kind == BodyKind::Multipart, cx);
            match body {
                RequestBody::None => input.project_content("", cx),
                RequestBody::Json(body) | RequestBody::Raw(body) => input.project_content(body, cx),
                RequestBody::UrlEncoded(body) => {
                    let entries = form_urlencoded::parse(body.as_bytes())
                        .map(|(key, value)| {
                            FormDataEntry::text(key.into_owned(), value.into_owned(), true)
                        })
                        .collect();
                    input.project_form_data_entries(entries, cx);
                }
                RequestBody::Multipart(parts) => {
                    let entries = parts
                        .into_iter()
                        .map(|part| match part.value {
                            MultipartValue::Text(value) => {
                                FormDataEntry::text(part.name, value, true)
                            }
                            MultipartValue::File {
                                path,
                                file_name,
                                content_type,
                            } => {
                                FormDataEntry::file(part.name, path, file_name, content_type, true)
                            }
                        })
                        .collect();
                    input.project_form_data_entries(entries, cx);
                }
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
            input.set_type_silent(BodyType::Raw, cx);
            input.project_content(pre_request_script, cx);
        });

        self.tests_input.update(cx, |input, cx| {
            input.set_type_silent(BodyType::Raw, cx);
            input.project_content(tests_script, cx);
        });
    }

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
                                    .debug_selector(|| "body-sample-json".into())
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
                                    .debug_selector(|| "body-clear-button".into())
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

impl Render for RequestEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ResponseState;
    use crate::models::HttpMethod;
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

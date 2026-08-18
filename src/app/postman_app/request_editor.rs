use super::*;

const REQUEST_PANEL_BASE_HEIGHT: f32 = 360.0;
const PARAM_ROWS_AT_BASE_HEIGHT: usize = 2;
const PARAM_ROW_PITCH: f32 = 46.0;
const PARAM_PANEL_MAX_VISIBLE_ROWS: usize = 6;
const HEADER_PANEL_MAX_VISIBLE_ROWS: usize = 4;
const REQUEST_EDITOR_RESERVED_HEIGHT: f32 = 400.0;

#[derive(Clone, Debug)]
pub(super) enum RequestEditorEvent {
    Execute(PendingRequest),
    Abort(SendId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PersistentRowKind {
    Params,
    Headers,
}

#[derive(Clone, Debug)]
enum PersistentRowEditorEvent {
    KeyChanged {
        kind: PersistentRowKind,
        index: usize,
        value: String,
    },
    ValueChanged {
        kind: PersistentRowKind,
        index: usize,
        value: String,
    },
    SubmitRequested {
        kind: PersistentRowKind,
    },
}

/// Editing buffers for one persistent Params or Headers row. Business values remain in the
/// ViewModel; these entities only retain cursor and selection state.
struct PersistentRowEditor {
    kind: PersistentRowKind,
    index: usize,
    key_input: Entity<HeaderInput>,
    value_input: Entity<HeaderInput>,
    _subscriptions: Vec<Subscription>,
}

impl PersistentRowEditor {
    fn new(
        kind: PersistentRowKind,
        index: usize,
        row: KeyValueRow,
        cx: &mut Context<Self>,
    ) -> Self {
        let KeyValueRow { key, value, .. } = row;
        let (key_placeholder, value_placeholder) = match kind {
            PersistentRowKind::Params => ("Key", "Value"),
            PersistentRowKind::Headers => ("Header name", "Header value"),
        };
        let key_input = cx.new(|cx| {
            let mut input = HeaderInput::new(cx).with_placeholder(key_placeholder);
            input.project_content(key, cx);
            input
        });
        let value_input = cx.new(|cx| {
            let mut input = HeaderInput::new(cx).with_placeholder(value_placeholder);
            input.project_content(value, cx);
            input
        });
        let subscriptions = vec![
            cx.subscribe(&key_input, Self::on_key_event),
            cx.subscribe(&value_input, Self::on_value_event),
        ];
        Self {
            kind,
            index,
            key_input,
            value_input,
            _subscriptions: subscriptions,
        }
    }

    fn on_key_event(
        &mut self,
        _input: Entity<HeaderInput>,
        event: &HeaderInputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            HeaderInputEvent::ValueChanged(value) => {
                cx.emit(PersistentRowEditorEvent::KeyChanged {
                    kind: self.kind,
                    index: self.index,
                    value: value.clone(),
                })
            }
            HeaderInputEvent::SubmitRequested => {
                cx.emit(PersistentRowEditorEvent::SubmitRequested { kind: self.kind })
            }
        }
    }

    fn on_value_event(
        &mut self,
        _input: Entity<HeaderInput>,
        event: &HeaderInputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            HeaderInputEvent::ValueChanged(value) => {
                cx.emit(PersistentRowEditorEvent::ValueChanged {
                    kind: self.kind,
                    index: self.index,
                    value: value.clone(),
                })
            }
            HeaderInputEvent::SubmitRequested => {
                cx.emit(PersistentRowEditorEvent::SubmitRequested { kind: self.kind })
            }
        }
    }
}

impl EventEmitter<PersistentRowEditorEvent> for PersistentRowEditor {}

impl Render for PersistentRowEditor {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let (key_cell_selector, key_input_selector, value_cell_selector, value_input_selector) =
            match self.kind {
                PersistentRowKind::Params => (
                    format!("param-row-key-input-{}", self.index),
                    None,
                    format!("param-row-value-input-{}", self.index),
                    None,
                ),
                PersistentRowKind::Headers => (
                    format!("header-row-key-{}", self.index),
                    Some(format!("header-row-key-input-{}", self.index)),
                    format!("header-row-value-{}", self.index),
                    Some(format!("header-row-value-input-{}", self.index)),
                ),
            };
        div()
            .h_full()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .debug_selector(move || key_cell_selector.clone())
                    .h_full()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .when_some(key_input_selector, |this, selector| {
                                this.debug_selector(move || selector.clone())
                            })
                            .h_full()
                            .child(self.key_input.clone()),
                    ),
            )
            .child(
                div()
                    .debug_selector(move || value_cell_selector.clone())
                    .h_full()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .when_some(value_input_selector, |this, selector| {
                                this.debug_selector(move || selector.clone())
                            })
                            .h_full()
                            .child(self.value_input.clone()),
                    ),
            )
    }
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

    fn rebuild_param_row_editors(&mut self, cx: &mut Context<Self>) {
        let rows = self.view_model.read(cx).params().to_vec();
        self.param_row_editors.clear();
        self.param_row_subscriptions.clear();
        for (index, row) in rows.into_iter().enumerate() {
            let editor =
                cx.new(|cx| PersistentRowEditor::new(PersistentRowKind::Params, index, row, cx));
            let subscription = cx.subscribe(&editor, Self::on_persistent_row_event);
            self.param_row_editors.push(editor);
            self.param_row_subscriptions.push(subscription);
        }
    }

    fn rebuild_header_row_editors(&mut self, cx: &mut Context<Self>) {
        let rows = self.view_model.read(cx).headers().to_vec();
        self.header_row_editors.clear();
        self.header_row_subscriptions.clear();
        for (index, row) in rows.into_iter().enumerate() {
            let editor =
                cx.new(|cx| PersistentRowEditor::new(PersistentRowKind::Headers, index, row, cx));
            let subscription = cx.subscribe(&editor, Self::on_persistent_row_event);
            self.header_row_editors.push(editor);
            self.header_row_subscriptions.push(subscription);
        }
    }

    fn on_persistent_row_event(
        &mut self,
        _editor: Entity<PersistentRowEditor>,
        event: &PersistentRowEditorEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            PersistentRowEditorEvent::KeyChanged { kind, index, value } => match kind {
                PersistentRowKind::Params => {
                    self.update_view_model(cx, |view_model| {
                        view_model.set_param_key(*index, value.clone())
                    });
                    self.project_url(cx);
                }
                PersistentRowKind::Headers => {
                    self.update_view_model(cx, |view_model| {
                        view_model.set_header_key(*index, value.clone())
                    });
                }
            },
            PersistentRowEditorEvent::ValueChanged { kind, index, value } => match kind {
                PersistentRowKind::Params => {
                    self.update_view_model(cx, |view_model| {
                        view_model.set_param_value(*index, value.clone())
                    });
                    self.project_url(cx);
                }
                PersistentRowKind::Headers => {
                    self.update_view_model(cx, |view_model| {
                        view_model.set_header_value(*index, value.clone())
                    });
                }
            },
            PersistentRowEditorEvent::SubmitRequested { kind } => {
                self.append_row(
                    match kind {
                        PersistentRowKind::Params => RequestPane::Params,
                        PersistentRowKind::Headers => RequestPane::Headers,
                    },
                    cx,
                );
            }
        }
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

    fn add_current_row(&mut self, cx: &mut Context<Self>) {
        let request_pane = self.view_model.read(cx).request_pane();
        self.append_row(request_pane, cx);
    }

    fn append_row(&mut self, request_pane: RequestPane, cx: &mut Context<Self>) {
        match request_pane {
            RequestPane::Params => {
                self.update_view_model(cx, |view_model| view_model.append_param_row());
                self.rebuild_param_row_editors(cx);
                self.param_rows_scroll_handle.scroll_to_bottom();
                self.project_url(cx);
            }
            RequestPane::Headers => {
                self.update_view_model(cx, |view_model| view_model.append_header_row());
                self.rebuild_header_row_editors(cx);
                self.header_rows_scroll_handle.scroll_to_bottom();
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
        self.rebuild_param_row_editors(cx);
        self.project_url(cx);
    }

    fn toggle_header(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.toggle_header(index));
    }

    fn toggle_header_draft(&mut self, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| {
            let index = view_model.headers().len();
            view_model.append_header_row();
            view_model.toggle_header(index);
        });
        self.rebuild_header_row_editors(cx);
        self.header_rows_scroll_handle.scroll_to_bottom();
        self.project_row_draft(cx);
    }

    fn remove_header(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.remove_header(index));
        self.rebuild_header_row_editors(cx);
    }

    fn clear_header_draft(&mut self, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.clear_header_draft());
        self.project_row_draft(cx);
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
    fn project_active_request(&mut self, cx: &mut Context<Self>) {
        self.project_method(cx);
        self.project_url(cx);
        self.rebuild_param_row_editors(cx);
        self.rebuild_header_row_editors(cx);
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
        let (key, value, key_placeholder, value_placeholder) = {
            let view_model = self.view_model.read(cx);
            let (key, value) = view_model
                .row_draft(view_model.request_pane())
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .unwrap_or_default();
            let placeholders = match view_model.request_pane() {
                RequestPane::Headers => ("Header name", "Header value"),
                RequestPane::Params
                | RequestPane::Authorization
                | RequestPane::Body
                | RequestPane::Scripts
                | RequestPane::Tests => ("Key", "Value"),
            };
            (key, value, placeholders.0, placeholders.1)
        };
        self.row_key_input.update(cx, |input, cx| {
            input.project_placeholder(key_placeholder, cx);
            input.project_content(key, cx);
        });
        self.row_value_input.update(cx, |input, cx| {
            input.project_placeholder(value_placeholder, cx);
            input.project_content(value, cx);
        });
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

    pub(super) fn render_request_panel(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let request_pane = self.view_model.read(cx).request_pane();
        let visible_rows = match request_pane {
            RequestPane::Params => self.view_model.read(cx).visible_param_row_count(),
            RequestPane::Headers => self.view_model.read(cx).visible_header_row_count(),
            RequestPane::Authorization
            | RequestPane::Body
            | RequestPane::Scripts
            | RequestPane::Tests => 0,
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

    pub(super) fn render_authorization_editor(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
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

    pub(super) fn render_params_editor(
        &self,
        panel_height: f32,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let row_editors = self.param_row_editors.clone();
        let (rows, draft_key, visible_row_count, enabled_count, effective_url) = {
            let view_model = self.view_model.read(cx);
            let (draft_key, _) = view_model
                .row_draft(RequestPane::Params)
                .unwrap_or_default();
            (
                view_model.params().to_vec(),
                draft_key.to_string(),
                view_model.visible_param_row_count(),
                view_model.enabled_param_count(),
                view_model.effective_url(),
            )
        };
        let draft_enabled = !draft_key.trim().is_empty();
        let draft_index = visible_row_count - 1;
        let draft_row_selector = format!("param-row-{draft_index}");
        let draft_key_selector = format!("param-row-key-input-{draft_index}");
        let draft_value_selector = format!("param-row-value-input-{draft_index}");
        let visible_capacity = visible_row_capacity(RequestPane::Params, panel_height);
        let show_scrollbar = visible_row_count > visible_capacity;
        let scrollbar = row_scrollbar_geometry(
            visible_row_count,
            visible_capacity,
            self.param_rows_scroll_handle.offset().y.as_f32(),
            self.param_rows_scroll_handle.max_offset().y.as_f32(),
        );

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .child(
                div()
                    .h(px(42.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .px_3()
                    .font_family(FONT_UI)
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(12.0))
                                    .text_color(rgb(TEXT))
                                    .child("Query parameters"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(rgb(SUBTEXT))
                                    .child("Synchronized with the URL query string"),
                            ),
                    )
                    .child(
                        div()
                            .debug_selector(|| "params-enabled-count".into())
                            .h(px(24.0))
                            .px_2()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap_1()
                            .rounded_lg()
                            .bg(rgb(OK_SOFT))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(10.0))
                            .text_color(rgb(OK))
                            .child("●")
                            .child(format!("{enabled_count} enabled")),
                    ),
            )
            .child(
                div()
                    .h(px(32.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .bg(rgb(PANEL_ALT))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .font_family(FONT_UI)
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(10.0))
                    .text_color(rgb(SUBTEXT))
                    .child(div().w(px(18.0)))
                    .child(div().flex_1().child("KEY"))
                    .child(div().flex_1().child("VALUE"))
                    .child(
                        div()
                            .w(px(56.0))
                            .text_align(gpui::TextAlign::Center)
                            .child("ACTION"),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .relative()
                    .child(
                        div()
                            .id("params-rows-scroll")
                            .debug_selector(|| "params-rows-scroll".into())
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .p_3()
                            .when(show_scrollbar, |this| this.pr(px(22.0)))
                            .overflow_y_scroll()
                            .track_scroll(&self.param_rows_scroll_handle)
                            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
                            .children(rows.into_iter().zip(row_editors).enumerate().map(
                                |(index, (row, row_editor))| {
                                    let is_enabled = row.enabled;
                                    let row_selector = format!("param-row-{index}");
                                    let toggle_selector = format!("param-row-toggle-{index}");
                                    let delete_selector = format!("param-row-delete-{index}");
                                    div()
                                        .debug_selector(move || row_selector.clone())
                                        .h(px(38.0))
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
                                                .border_color(rgb(if is_enabled {
                                                    INFO
                                                } else {
                                                    LINE
                                                }))
                                                .bg(rgb(if is_enabled { INFO } else { PANEL }))
                                                .text_color(rgb(PANEL))
                                                .cursor_pointer()
                                                .child(if is_enabled { "✓" } else { "" })
                                                .on_mouse_up(
                                                    gpui::MouseButton::Left,
                                                    cx.listener(move |this, _, _, cx| {
                                                        this.toggle_param(index, cx)
                                                    }),
                                                ),
                                        )
                                        .child(row_editor)
                                        .child(
                                            div()
                                                .debug_selector(move || delete_selector.clone())
                                                .w(px(56.0))
                                                .h(px(32.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded_lg()
                                                .cursor_pointer()
                                                .text_color(rgb(MUTED))
                                                .hover(|style| {
                                                    style
                                                        .bg(rgb(ACCENT_SOFT))
                                                        .text_color(rgb(ERROR))
                                                })
                                                .child("×")
                                                .on_mouse_up(
                                                    gpui::MouseButton::Left,
                                                    cx.listener(move |this, _, _, cx| {
                                                        this.remove_param(index, cx)
                                                    }),
                                                ),
                                        )
                                },
                            ))
                            .child(
                                div()
                                    .debug_selector(move || draft_row_selector.clone())
                                    .h(px(38.0))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .debug_selector(|| "params-draft-toggle".into())
                                            .size(px(18.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_sm()
                                            .border_1()
                                            .border_color(rgb(if draft_enabled {
                                                INFO
                                            } else {
                                                LINE
                                            }))
                                            .bg(rgb(if draft_enabled { INFO } else { PANEL }))
                                            .text_color(rgb(PANEL))
                                            .child(if draft_enabled { "✓" } else { "" }),
                                    )
                                    .child(
                                        div()
                                            .debug_selector(move || draft_key_selector.clone())
                                            .h_full()
                                            .flex_1()
                                            .child(
                                                div()
                                                    .debug_selector(|| "row-key-input".into())
                                                    .h_full()
                                                    .child(self.row_key_input.clone()),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .debug_selector(move || draft_value_selector.clone())
                                            .h_full()
                                            .flex_1()
                                            .child(
                                                div()
                                                    .debug_selector(|| "row-value-input".into())
                                                    .h_full()
                                                    .child(self.row_value_input.clone()),
                                            ),
                                    )
                                    .child(div().w(px(56.0)).h(px(32.0))),
                            ),
                    )
                    .when_some(scrollbar, |this, scrollbar| {
                        this.child(
                            div()
                                .debug_selector(|| "params-scrollbar".into())
                                .absolute()
                                .top(px(8.0))
                                .right(px(5.0))
                                .bottom(px(8.0))
                                .w(px(8.0))
                                .rounded_full()
                                .bg(rgb(PANEL_ALT))
                                .border_1()
                                .border_color(rgb(LINE))
                                .child(
                                    div()
                                        .debug_selector(|| "params-scrollbar-thumb".into())
                                        .absolute()
                                        .top(relative(scrollbar.thumb_top))
                                        .w_full()
                                        .h(relative(scrollbar.thumb_height))
                                        .rounded_full()
                                        .bg(rgb(INFO)),
                                ),
                        )
                    }),
            )
            .child(
                div()
                    .h(px(44.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .px_3()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(PANEL))
                    .child(
                        div()
                            .debug_selector(|| "add-row-button".into())
                            .h(px(32.0))
                            .w_full()
                            .px_3()
                            .flex()
                            .items_center()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(LINE))
                            .bg(rgb(PANEL_ALT))
                            .text_color(rgb(SUBTEXT))
                            .font_family(FONT_UI)
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .hover(|style| {
                                style
                                    .bg(rgb(INFO_SOFT))
                                    .border_color(rgb(INFO))
                                    .text_color(rgb(INFO))
                            })
                            .child("＋ Add parameter")
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.add_current_row(cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "effective-url-preview".into())
                    .h(px(64.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .px_3()
                    .bg(rgb(INFO_SOFT))
                    .border_b_1()
                    .border_color(rgb(LINE))
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
                                    .text_size(px(10.0))
                                    .text_color(rgb(INFO))
                                    .child("↗  EFFECTIVE URL"),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "effective-url-value".into())
                                    .overflow_hidden()
                                    .font_family(FONT_MONO)
                                    .text_size(px(11.0))
                                    .text_color(rgb(TEXT))
                                    .child(effective_url),
                            ),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .flex_none()
                            .rounded_lg()
                            .bg(rgb(PANEL))
                            .font_family(FONT_UI)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(10.0))
                            .text_color(rgb(INFO))
                            .child("encoded"),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "params-ready-indicator".into())
                    .h(px(34.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .font_family(FONT_UI)
                    .text_size(px(10.0))
                    .text_color(rgb(SUBTEXT))
                    .child(div().text_color(rgb(OK)).child("✓"))
                    .child("Ready to send — the active value is already in the ViewModel"),
            )
            .into_any_element()
    }

    pub(super) fn render_headers_editor(
        &self,
        panel_height: f32,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let row_editors = self.header_row_editors.clone();
        let (rows, draft_key, draft_value, visible_row_count, enabled_count) = {
            let view_model = self.view_model.read(cx);
            let (draft_key, draft_value) = view_model
                .row_draft(RequestPane::Headers)
                .unwrap_or_default();
            (
                view_model.headers().to_vec(),
                draft_key.to_string(),
                draft_value.to_string(),
                view_model.visible_header_row_count(),
                view_model.enabled_header_count(),
            )
        };
        let disabled_count = rows
            .iter()
            .filter(|row| header_row_complete(row) && !row.enabled)
            .count();
        let draft_complete = !draft_key.trim().is_empty() && !draft_value.trim().is_empty();
        let draft_index = visible_row_count - 1;
        let draft_row_selector = format!("header-row-{draft_index}");
        let draft_toggle_selector = format!("header-row-toggle-{draft_index}");
        let draft_key_selector = format!("header-row-key-{draft_index}");
        let draft_key_input_selector = format!("header-row-key-input-{draft_index}");
        let draft_value_selector = format!("header-row-value-{draft_index}");
        let draft_value_input_selector = format!("header-row-value-input-{draft_index}");
        let draft_status_selector = format!("header-row-status-{draft_index}");
        let draft_delete_selector = format!("header-row-delete-{draft_index}");
        let visible_capacity = visible_row_capacity(RequestPane::Headers, panel_height);
        let show_scrollbar = visible_row_count > visible_capacity;
        let scrollbar = row_scrollbar_geometry(
            visible_row_count,
            visible_capacity,
            self.header_rows_scroll_handle.offset().y.as_f32(),
            self.header_rows_scroll_handle.max_offset().y.as_f32(),
        );

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .child(
                div()
                    .debug_selector(|| "headers-summary".into())
                    .h(px(42.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .px_3()
                    .font_family(FONT_UI)
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_none()
                                    .text_color(rgb(TEXT))
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(12.0))
                                    .child("Request headers"),
                            )
                            .child(
                                div()
                                    .overflow_hidden()
                                    .text_size(px(11.0))
                                    .text_color(rgb(SUBTEXT))
                                    .child("Disabled rows stay saved but are excluded from Send"),
                            ),
                    )
                    .child(
                        div()
                            .debug_selector(|| "headers-enabled-count".into())
                            .h(px(24.0))
                            .px_2()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap_1()
                            .rounded_lg()
                            .bg(rgb(OK_SOFT))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(10.0))
                            .text_color(rgb(OK))
                            .child("●")
                            .child(format!(
                                "{enabled_count} enabled · {disabled_count} disabled"
                            )),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "headers-table-header".into())
                    .h(px(32.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .bg(rgb(PANEL_ALT))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .font_family(FONT_UI)
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(10.0))
                    .text_color(rgb(SUBTEXT))
                    .child(div().w(px(18.0)))
                    .child(div().flex_1().child("KEY"))
                    .child(div().flex_1().child("VALUE"))
                    .child(
                        div()
                            .w(px(112.0))
                            .text_align(gpui::TextAlign::Center)
                            .child("ACTION"),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .relative()
                    .child(
                        div()
                            .id("headers-rows-scroll")
                            .debug_selector(|| "headers-rows-scroll".into())
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .p_3()
                            .when(show_scrollbar, |this| this.pr(px(22.0)))
                            .overflow_y_scroll()
                            .track_scroll(&self.header_rows_scroll_handle)
                            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
                            .children(rows.into_iter().zip(row_editors).enumerate().map(
                                |(index, (row, row_editor))| {
                                    let is_complete = header_row_complete(&row);
                                    let is_sent = row.enabled && is_complete;
                                    let (status, status_bg, status_color) = if !is_complete {
                                        ("DRAFT", PANEL_ALT, SUBTEXT)
                                    } else if row.enabled {
                                        ("SENT", OK_SOFT, OK)
                                    } else {
                                        ("EXCLUDED", ACCENT_SOFT, ACCENT)
                                    };
                                    let row_selector = format!("header-row-{index}");
                                    let toggle_selector = format!("header-row-toggle-{index}");
                                    let status_selector = format!("header-row-status-{index}");
                                    let delete_selector = format!("header-row-delete-{index}");

                                    div()
                                        .debug_selector(move || row_selector.clone())
                                        .h(px(40.0))
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
                                                .flex_none()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded_sm()
                                                .border_1()
                                                .border_color(rgb(if is_sent {
                                                    INFO
                                                } else {
                                                    LINE
                                                }))
                                                .bg(rgb(if is_sent { INFO } else { PANEL }))
                                                .text_color(rgb(PANEL))
                                                .cursor_pointer()
                                                .child(if is_sent { "✓" } else { "" })
                                                .on_mouse_up(
                                                    gpui::MouseButton::Left,
                                                    cx.listener(move |this, _, _, cx| {
                                                        this.toggle_header(index, cx)
                                                    }),
                                                ),
                                        )
                                        .child(row_editor)
                                        .child(
                                            div()
                                                .w(px(112.0))
                                                .flex_none()
                                                .flex()
                                                .items_center()
                                                .justify_between()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .debug_selector(move || {
                                                            status_selector.clone()
                                                        })
                                                        .h(px(24.0))
                                                        .w(px(76.0))
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .rounded_lg()
                                                        .bg(rgb(status_bg))
                                                        .font_family(FONT_UI)
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .text_size(px(9.0))
                                                        .text_color(rgb(status_color))
                                                        .child(status),
                                                )
                                                .child(
                                                    div()
                                                        .debug_selector(move || {
                                                            delete_selector.clone()
                                                        })
                                                        .size(px(28.0))
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .rounded_lg()
                                                        .cursor_pointer()
                                                        .text_color(rgb(MUTED))
                                                        .hover(|style| {
                                                            style
                                                                .bg(rgb(ACCENT_SOFT))
                                                                .text_color(rgb(ERROR))
                                                        })
                                                        .child("×")
                                                        .on_mouse_up(
                                                            gpui::MouseButton::Left,
                                                            cx.listener(move |this, _, _, cx| {
                                                                this.remove_header(index, cx)
                                                            }),
                                                        ),
                                                ),
                                        )
                                },
                            ))
                            .child(
                                div()
                                    .debug_selector(move || draft_row_selector.clone())
                                    .h(px(40.0))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .font_family(FONT_MONO)
                                    .text_size(px(12.0))
                                    .child(
                                        div()
                                            .debug_selector(move || draft_toggle_selector.clone())
                                            .size(px(18.0))
                                            .flex_none()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_sm()
                                            .border_1()
                                            .border_color(rgb(if draft_complete {
                                                INFO
                                            } else {
                                                LINE
                                            }))
                                            .bg(rgb(if draft_complete { INFO } else { PANEL }))
                                            .text_color(rgb(PANEL))
                                            .child(if draft_complete { "✓" } else { "" })
                                            .when(draft_complete, |this| {
                                                this.cursor_pointer().on_mouse_up(
                                                    gpui::MouseButton::Left,
                                                    cx.listener(|this, _, _, cx| {
                                                        this.toggle_header_draft(cx)
                                                    }),
                                                )
                                            }),
                                    )
                                    .child(
                                        div()
                                            .debug_selector(move || draft_key_selector.clone())
                                            .h_full()
                                            .min_w_0()
                                            .flex_1()
                                            .child(
                                                div()
                                                    .debug_selector(move || {
                                                        draft_key_input_selector.clone()
                                                    })
                                                    .h_full()
                                                    .child(
                                                        div()
                                                            .debug_selector(|| {
                                                                "row-key-input".into()
                                                            })
                                                            .h_full()
                                                            .child(self.row_key_input.clone()),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .debug_selector(move || draft_value_selector.clone())
                                            .h_full()
                                            .min_w_0()
                                            .flex_1()
                                            .child(
                                                div()
                                                    .debug_selector(move || {
                                                        draft_value_input_selector.clone()
                                                    })
                                                    .h_full()
                                                    .child(
                                                        div()
                                                            .debug_selector(|| {
                                                                "row-value-input".into()
                                                            })
                                                            .h_full()
                                                            .child(self.row_value_input.clone()),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .w(px(112.0))
                                            .flex_none()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .debug_selector(move || {
                                                        draft_status_selector.clone()
                                                    })
                                                    .h(px(24.0))
                                                    .w(px(76.0))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .rounded_lg()
                                                    .bg(rgb(if draft_complete {
                                                        OK_SOFT
                                                    } else {
                                                        PANEL_ALT
                                                    }))
                                                    .font_family(FONT_UI)
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_size(px(9.0))
                                                    .text_color(rgb(if draft_complete {
                                                        OK
                                                    } else {
                                                        SUBTEXT
                                                    }))
                                                    .child(if draft_complete {
                                                        "SENT"
                                                    } else {
                                                        "DRAFT"
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .debug_selector(move || {
                                                        draft_delete_selector.clone()
                                                    })
                                                    .size(px(28.0))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .rounded_lg()
                                                    .cursor_pointer()
                                                    .text_color(rgb(MUTED))
                                                    .hover(|style| {
                                                        style
                                                            .bg(rgb(ACCENT_SOFT))
                                                            .text_color(rgb(ERROR))
                                                    })
                                                    .child("×")
                                                    .on_mouse_up(
                                                        gpui::MouseButton::Left,
                                                        cx.listener(|this, _, _, cx| {
                                                            this.clear_header_draft(cx)
                                                        }),
                                                    ),
                                            ),
                                    ),
                            ),
                    )
                    .when_some(scrollbar, |this, scrollbar| {
                        this.child(
                            div()
                                .debug_selector(|| "headers-scrollbar".into())
                                .absolute()
                                .top(px(8.0))
                                .right(px(5.0))
                                .bottom(px(8.0))
                                .w(px(8.0))
                                .rounded_full()
                                .bg(rgb(PANEL_ALT))
                                .border_1()
                                .border_color(rgb(LINE))
                                .child(
                                    div()
                                        .debug_selector(|| "headers-scrollbar-thumb".into())
                                        .absolute()
                                        .top(relative(scrollbar.thumb_top))
                                        .w_full()
                                        .h(relative(scrollbar.thumb_height))
                                        .rounded_full()
                                        .bg(rgb(INFO)),
                                ),
                        )
                    }),
            )
            .child(
                div()
                    .h(px(44.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .px_3()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(INFO_SOFT))
                    .child(
                        div()
                            .debug_selector(|| "add-row-button".into())
                            .h(px(32.0))
                            .w_full()
                            .px_3()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(LINE))
                            .bg(rgb(PANEL_ALT))
                            .text_color(rgb(SUBTEXT))
                            .font_family(FONT_UI)
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .hover(|style| {
                                style
                                    .bg(rgb(INFO_SOFT))
                                    .border_color(rgb(INFO))
                                    .text_color(rgb(INFO))
                            })
                            .child("＋ Add another header row")
                            .child(
                                div()
                                    .font_weight(FontWeight::NORMAL)
                                    .text_color(rgb(MUTED))
                                    .child("Click repeatedly — rows are unlimited"),
                            )
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.add_current_row(cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "headers-ready-indicator".into())
                    .h(px(54.0))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap_2()
                    .px_3()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .font_family(FONT_UI)
                    .text_size(px(10.0))
                    .text_color(rgb(SUBTEXT))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().text_color(rgb(OK)).child("✓"))
                            .child("Ready to send — active values are already in the ViewModel"),
                    )
                    .child(
                        div().font_family(FONT_MONO).text_color(rgb(INFO)).child(
                            "Only complete, checked rows participate in request construction",
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

fn header_row_complete(row: &KeyValueRow) -> bool {
    !row.key.trim().is_empty() && !row.value.trim().is_empty()
}

fn adaptive_request_panel_height(
    pane: RequestPane,
    visible_param_rows: usize,
    viewport_height: f32,
) -> f32 {
    if !matches!(pane, RequestPane::Params | RequestPane::Headers) {
        return REQUEST_PANEL_BASE_HEIGHT;
    }

    let max_visible_rows = match pane {
        RequestPane::Params => PARAM_PANEL_MAX_VISIBLE_ROWS,
        RequestPane::Headers => HEADER_PANEL_MAX_VISIBLE_ROWS,
        RequestPane::Authorization
        | RequestPane::Body
        | RequestPane::Scripts
        | RequestPane::Tests => unreachable!("non-row panes returned above"),
    };
    let expandable_rows = max_visible_rows - PARAM_ROWS_AT_BASE_HEIGHT;
    let added_rows = visible_param_rows
        .saturating_sub(PARAM_ROWS_AT_BASE_HEIGHT)
        .min(expandable_rows);
    let desired_height = REQUEST_PANEL_BASE_HEIGHT + PARAM_ROW_PITCH * added_rows as f32;
    let maximum_height = REQUEST_PANEL_BASE_HEIGHT + PARAM_ROW_PITCH * expandable_rows as f32;
    let viewport_height = (viewport_height - REQUEST_EDITOR_RESERVED_HEIGHT)
        .clamp(REQUEST_PANEL_BASE_HEIGHT, maximum_height);

    desired_height.min(viewport_height)
}

fn visible_row_capacity(pane: RequestPane, panel_height: f32) -> usize {
    let max_visible_rows = match pane {
        RequestPane::Params => PARAM_PANEL_MAX_VISIBLE_ROWS,
        RequestPane::Headers => HEADER_PANEL_MAX_VISIBLE_ROWS,
        RequestPane::Authorization
        | RequestPane::Body
        | RequestPane::Scripts
        | RequestPane::Tests => return 0,
    };
    let additional_rows = ((panel_height - REQUEST_PANEL_BASE_HEIGHT) / PARAM_ROW_PITCH)
        .max(0.0)
        .floor() as usize;
    (PARAM_ROWS_AT_BASE_HEIGHT + additional_rows).min(max_visible_rows)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RowScrollbarGeometry {
    thumb_top: f32,
    thumb_height: f32,
}

fn row_scrollbar_geometry(
    visible_rows: usize,
    visible_capacity: usize,
    offset_y: f32,
    max_offset_y: f32,
) -> Option<RowScrollbarGeometry> {
    if visible_rows <= visible_capacity || visible_capacity == 0 {
        return None;
    }

    let thumb_height = (visible_capacity as f32 / visible_rows as f32).clamp(0.18, 0.9);
    let progress = if max_offset_y > 0.0 {
        (-offset_y / max_offset_y).clamp(0.0, 1.0)
    } else {
        0.0
    };

    Some(RowScrollbarGeometry {
        thumb_top: progress * (1.0 - thumb_height),
        thumb_height,
    })
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

    #[test]
    fn params_panel_grows_by_row_then_caps_for_scrolling() {
        assert_eq!(
            adaptive_request_panel_height(RequestPane::Params, 1, 980.0),
            360.0
        );
        assert_eq!(
            adaptive_request_panel_height(RequestPane::Params, 2, 980.0),
            360.0
        );
        assert_eq!(
            adaptive_request_panel_height(RequestPane::Params, 3, 980.0),
            406.0
        );
        assert_eq!(
            adaptive_request_panel_height(RequestPane::Params, 6, 980.0),
            544.0
        );
        assert_eq!(
            adaptive_request_panel_height(RequestPane::Params, 30, 980.0),
            544.0
        );
        assert_eq!(
            adaptive_request_panel_height(RequestPane::Params, 6, 820.0),
            420.0
        );
        assert_eq!(
            adaptive_request_panel_height(RequestPane::Headers, 30, 980.0),
            452.0
        );

        assert_eq!(visible_row_capacity(RequestPane::Params, 360.0), 2);
        assert_eq!(visible_row_capacity(RequestPane::Params, 406.0), 3);
        assert_eq!(visible_row_capacity(RequestPane::Params, 544.0), 6);
        assert_eq!(visible_row_capacity(RequestPane::Headers, 452.0), 4);
        assert_eq!(row_scrollbar_geometry(6, 6, 0.0, 0.0), None);
        assert_eq!(
            row_scrollbar_geometry(12, 6, -100.0, 200.0),
            Some(RowScrollbarGeometry {
                thumb_top: 0.25,
                thumb_height: 0.5,
            })
        );
    }

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

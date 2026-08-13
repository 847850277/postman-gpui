use crate::{
    app::{
        view_model::detect_body_kind, BodyKind, KeyValueRow, RequestPane, ResponseState,
        WorkspaceViewModel,
    },
    models::HttpMethod,
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

pub struct PostmanApp {
    view_model: Entity<WorkspaceViewModel>,
    method_selector: Entity<MethodSelector>,
    url_input: Entity<UrlInput>,
    body_input: Entity<BodyInput>,
    row_key_input: Entity<HeaderInput>,
    row_value_input: Entity<HeaderInput>,
    authorization_input: Entity<HeaderInput>,
    script_input: Entity<BodyInput>,
    tests_input: Entity<BodyInput>,
    history_list: Entity<HistoryList>,
    response_viewer: Entity<ResponseViewer>,
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
            cx.subscribe(&script_input, Self::on_script_event),
            cx.subscribe(&tests_input, Self::on_tests_event),
            cx.subscribe(&history_list, Self::on_history_selected),
            cx.observe(&view_model, |this, _, cx| {
                this.project_active_request(cx);
                cx.notify();
            }),
        ];

        Self {
            view_model,
            method_selector,
            url_input,
            body_input,
            row_key_input,
            row_value_input,
            authorization_input,
            script_input,
            tests_input,
            history_list,
            response_viewer,
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
        cx.notify();
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
                cx.notify();
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
        cx.notify();
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
            cx.notify();
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
        cx.notify();
    }

    fn on_tests_event(
        &mut self,
        _input: Entity<BodyInput>,
        event: &BodyInputEvent,
        cx: &mut Context<Self>,
    ) {
        let BodyInputEvent::ValueChanged(script) = event;
        self.update_view_model(cx, |view_model| view_model.set_tests_script(script));
        cx.notify();
    }

    fn on_history_selected(
        &mut self,
        _list: Entity<HistoryList>,
        event: &HistoryListEvent,
        cx: &mut Context<Self>,
    ) {
        let HistoryListEvent::RequestSelected(request) = event;
        self.update_view_model(cx, |view_model| view_model.load_request(request));
        cx.notify();
    }

    pub fn type_url(&mut self, url: &str, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.set_url(url));
    }

    pub fn set_body(&mut self, body: &str, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| {
            view_model.set_body(body);
            view_model.set_body_kind(detect_body_kind(body));
        });
    }

    pub fn choose_method(&mut self, method: HttpMethod, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.set_method(method));
    }

    pub fn click_send(&mut self, cx: &mut Context<Self>) {
        self.update_view_model(cx, WorkspaceViewModel::send);
        cx.notify();
    }

    pub fn response_state(&self, cx: &App) -> ResponseState {
        self.view_model.read(cx).response().clone()
    }

    pub fn history_len(&self, cx: &App) -> usize {
        self.view_model.read(cx).history_len()
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

    pub fn current_bearer_token(&self, cx: &App) -> String {
        self.view_model.read(cx).bearer_token().to_string()
    }

    pub fn current_pre_request_script(&self, cx: &App) -> String {
        self.view_model.read(cx).pre_request_script().to_string()
    }

    pub fn current_tests_script(&self, cx: &App) -> String {
        self.view_model.read(cx).tests_script().to_string()
    }

    pub fn set_bearer_token(&mut self, token: &str, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.set_bearer_token(token));
    }

    pub fn set_pre_request_script(&mut self, script: &str, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.set_pre_request_script(script));
    }

    pub fn set_tests_script(&mut self, script: &str, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.set_tests_script(script));
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
        cx.notify();
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
        cx.notify();
    }

    fn toggle_param(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.toggle_param(index));
        cx.notify();
    }

    fn remove_param(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.remove_param(index));
        cx.notify();
    }

    fn toggle_header(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.toggle_header(index));
        cx.notify();
    }

    fn remove_header(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.remove_header(index));
        cx.notify();
    }

    fn set_header_input_values(&mut self, key: &str, value: &str, cx: &mut Context<Self>) {
        self.row_key_input
            .update(cx, |input, cx| input.set_content(key.to_string(), cx));
        self.row_value_input
            .update(cx, |input, cx| input.set_content(value.to_string(), cx));
    }

    fn set_body_kind(&mut self, kind: BodyKind, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.set_body_kind(kind));
        cx.notify();
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
        cx.notify();
    }

    fn new_request(&mut self, cx: &mut Context<Self>) {
        self.update_view_model(cx, WorkspaceViewModel::new_request);
        self.clear_staged_row(cx);
        cx.notify();
    }

    fn select_request_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.update_view_model(cx, |view_model| view_model.select_tab(index)) {
            self.clear_staged_row(cx);
            cx.notify();
        }
    }

    fn close_request_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.update_view_model(cx, |view_model| view_model.close_tab(index)) {
            self.clear_staged_row(cx);
            cx.notify();
        }
    }

    fn clear_staged_row(&self, cx: &mut Context<Self>) {
        self.row_key_input.update(cx, |input, cx| input.clear(cx));
        self.row_value_input.update(cx, |input, cx| input.clear(cx));
    }

    /// One-way VM -> editor projection. Editor buffers retain cursor/selection state, but they
    /// never participate in request construction.
    fn project_active_request(&self, cx: &mut Context<Self>) {
        let (method, url, body, kind, bearer_token, pre_request_script, tests_script) = {
            let view_model = self.view_model.read(cx);
            (
                view_model.method(),
                view_model.url().to_string(),
                view_model.body().to_string(),
                view_model.body_kind(),
                view_model.bearer_token().to_string(),
                view_model.pre_request_script().to_string(),
                view_model.tests_script().to_string(),
            )
        };

        self.method_selector
            .update(cx, |selector, cx| selector.project_method(method, cx));
        self.url_input
            .update(cx, |input, cx| input.project_url(url, cx));

        self.body_input.update(cx, |input, cx| {
            input.set_type_silent(body_type_from_kind(kind), cx);
            if kind == BodyKind::FormData {
                project_form_data(input, &body, cx);
            } else {
                input.project_content(body, cx);
            }
        });

        self.authorization_input
            .update(cx, |input, cx| input.project_content(bearer_token, cx));

        self.script_input.update(cx, |input, cx| {
            input.set_type_silent(BodyType::Json, cx);
            input.project_content(pre_request_script, cx);
        });

        self.tests_input.update(cx, |input, cx| {
            input.set_type_silent(BodyType::Json, cx);
            input.project_content(tests_script, cx);
        });
    }

    fn request_tab(
        &self,
        pane: RequestPane,
        label: impl Into<String>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.view_model.read(cx).request_pane() == pane;
        let selector = request_pane_selector(pane);
        div()
            .debug_selector(move || selector.into())
            .h_full()
            .flex()
            .items_center()
            .px_2()
            .cursor_pointer()
            .font_family(FONT_UI)
            .text_size(px(13.0))
            .font_weight(if active {
                FontWeight::BOLD
            } else {
                FontWeight::SEMIBOLD
            })
            .text_color(rgb(if active { TEXT } else { MUTED }))
            .hover(|style| style.text_color(rgb(TEXT)))
            .child(label.into())
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, _, cx| this.set_request_pane(pane, cx)),
            )
    }

    fn render_top_header(&self) -> impl IntoElement {
        div()
            .debug_selector(|| "top-header".into())
            .h(px(72.0))
            .flex_none()
            .flex()
            .items_center()
            .px_5()
            .bg(rgb(PANEL))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().size(px(20.0)).rounded_full().bg(rgb(ACCENT)))
                    .child(
                        div()
                            .font_family(FONT_HEADING)
                            .text_size(px(22.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(TEXT))
                            .child("Postman GPUI"),
                    ),
            )
    }

    fn render_left_rail(&self) -> impl IntoElement {
        let slots = ["⌂", "↻", "◫", "◇", "⌘", "⚙", "?"];
        div()
            .debug_selector(|| "left-rail".into())
            .w(px(72.0))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap_4()
            .px_2()
            .py_3()
            .bg(rgb(PANEL))
            .border_r_1()
            .border_color(rgb(LINE))
            .children(slots.into_iter().enumerate().map(|(index, label)| {
                div()
                    .id(("rail-slot", index))
                    .size(px(40.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_lg()
                    .bg(rgb(if index == 1 { ACCENT_SOFT } else { PANEL_ALT }))
                    .text_color(rgb(if index == 1 { ACCENT_DARK } else { SUBTEXT }))
                    .font_family(FONT_UI)
                    .text_size(px(16.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(label)
            }))
    }

    fn render_request_tabs_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs: Vec<_> = {
            let view_model = self.view_model.read(cx);
            let active_index = view_model.active_tab_index();
            view_model
                .tabs()
                .iter()
                .enumerate()
                .map(|(index, request)| {
                    (
                        index,
                        request.method(),
                        request.tab_title(),
                        request.is_dirty(),
                        index == active_index,
                    )
                })
                .collect()
        };

        div()
            .debug_selector(|| "request-tabs-bar".into())
            .h(px(54.0))
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .bg(rgb(PANEL))
            .border_b_1()
            .border_color(rgb(LINE))
            .children(
                tabs.into_iter()
                    .map(|(index, method, title, dirty, active)| {
                        div()
                            .debug_selector(move || format!("request-tab-{index}").into())
                            .h_full()
                            .max_w(px(280.0))
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .bg(rgb(if active { PANEL } else { PANEL_ALT }))
                            .rounded_t_lg()
                            .font_family(FONT_UI)
                            .text_size(px(12.0))
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(PANEL)))
                            .child(
                                div()
                                    .text_color(rgb(method_color(method)))
                                    .font_weight(FontWeight::BOLD)
                                    .child(method.to_string()),
                            )
                            .child(
                                div()
                                    .max_w(px(180.0))
                                    .overflow_hidden()
                                    .text_color(rgb(if active { SUBTEXT } else { MUTED }))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .when(dirty, |tab| {
                                tab.child(div().size(px(6.0)).rounded_full().bg(rgb(ACCENT)))
                            })
                            .child(
                                div()
                                    .debug_selector(move || format!("close-tab-{index}").into())
                                    .size(px(20.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .text_color(rgb(MUTED))
                                    .hover(|style| {
                                        style.bg(rgb(ACCENT_SOFT)).text_color(rgb(ACCENT_DARK))
                                    })
                                    .child("×")
                                    .on_mouse_up(
                                        gpui::MouseButton::Left,
                                        cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.close_request_tab(index, cx);
                                        }),
                                    ),
                            )
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    this.select_request_tab(index, cx)
                                }),
                            )
                    }),
            )
            .child(
                div()
                    .debug_selector(|| "new-tab-button".into())
                    .size(px(32.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_lg()
                    .bg(rgb(PANEL_ALT))
                    .text_color(rgb(SUBTEXT))
                    .font_family(FONT_HEADING)
                    .text_size(px(20.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(ACCENT_SOFT)).text_color(rgb(ACCENT_DARK)))
                    .child("+")
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.new_request(cx)),
                    ),
            )
    }

    fn render_request_head(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .debug_selector(|| "request-head".into())
            .h(px(46.0))
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .child(self.method_selector.clone())
            .child(self.url_input.clone())
            .child(
                div()
                    .debug_selector(|| "send-button".into())
                    .w(px(110.0))
                    .h_full()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_lg()
                    .bg(rgb(ACCENT))
                    .text_color(rgb(PANEL))
                    .font_family(FONT_HEADING)
                    .text_size(px(15.0))
                    .font_weight(FontWeight::BOLD)
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(ACCENT_DARK)))
                    .child("Send")
                    .on_mouse_up(gpui::MouseButton::Left, cx.listener(Self::on_send_clicked)),
            )
    }

    fn render_request_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (header_count, has_bearer, has_body, has_script, has_tests) = {
            let view_model = self.view_model.read(cx);
            (
                view_model
                    .headers()
                    .iter()
                    .filter(|row| row.enabled)
                    .count(),
                !view_model.bearer_token().is_empty(),
                !view_model.body().is_empty(),
                !view_model.pre_request_script().is_empty(),
                !view_model.tests_script().is_empty(),
            )
        };
        div()
            .h(px(44.0))
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .bg(rgb(PANEL_ALT))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(self.request_tab(RequestPane::Params, "Params", cx))
            .child(self.request_tab(
                RequestPane::Authorization,
                if has_bearer {
                    "Authorization (Bearer) ●".to_string()
                } else {
                    "Authorization (Bearer)".to_string()
                },
                cx,
            ))
            .child(self.request_tab(
                RequestPane::Headers,
                format!("Headers ({header_count})"),
                cx,
            ))
            .child(self.request_tab(
                RequestPane::Body,
                if has_body {
                    "Body ●".to_string()
                } else {
                    "Body".to_string()
                },
                cx,
            ))
            .child(self.request_tab(
                RequestPane::Scripts,
                if has_script {
                    "Scripts ●".to_string()
                } else {
                    "Scripts".to_string()
                },
                cx,
            ))
            .child(self.request_tab(
                RequestPane::Tests,
                if has_tests {
                    "Tests ●".to_string()
                } else {
                    "Tests".to_string()
                },
                cx,
            ))
    }

    fn render_request_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
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

    fn render_authorization_editor(&self, _cx: &mut Context<Self>) -> gpui::AnyElement {
        div()
            .flex_1()
            .min_h_0()
            .p_4()
            .bg(rgb(CODE_BG))
            .child(
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
                            .child("Bearer token"),
                    )
                    .child(
                        div()
                            .debug_selector(|| "authorization-input".into())
                            .h(px(34.0))
                            .flex_1()
                            .child(self.authorization_input.clone()),
                    ),
            )
            .child(
                div()
                    .mt_3()
                    .font_family(FONT_UI)
                    .text_size(px(12.0))
                    .text_color(rgb(MUTED))
                    .child("The request will include Authorization: Bearer <token>."),
            )
            .into_any_element()
    }

    fn render_script_editor(
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

    fn render_key_value_editor(
        &self,
        title: &'static str,
        rows: Vec<KeyValueRow>,
        toggle: fn(&mut Self, usize, &mut Context<Self>),
        remove: fn(&mut Self, usize, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let enabled = rows.iter().filter(|row| row.enabled).count();
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
                    .child(self.row_key_input.clone())
                    .child(self.row_value_input.clone())
                    .child(
                        div()
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

    fn render_body_editor(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
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
                    .child(self.body_kind_option("○", "none", None, kind, cx))
                    .child(self.body_kind_option(
                        "○",
                        "form-data",
                        Some(BodyKind::FormData),
                        kind,
                        cx,
                    ))
                    .child(self.body_kind_option(
                        "○",
                        "x-www-form-urlencoded",
                        Some(BodyKind::FormData),
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
                    .child(div().flex_1().min_h_0().child(self.body_input.clone()))
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

    fn body_kind_option(
        &self,
        marker: &'static str,
        label: &'static str,
        option: Option<BodyKind>,
        selected: BodyKind,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = option == Some(selected);
        let element = div()
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
        BodyKind::FormData => BodyType::FormData,
        BodyKind::Raw => BodyType::Raw,
    }
}

fn project_form_data(input: &mut BodyInput, body: &str, cx: &mut Context<BodyInput>) {
    let mut entries: Vec<FormDataEntry> = form_urlencoded::parse(body.as_bytes())
        .map(|(key, value)| FormDataEntry {
            key: key.into_owned(),
            value: value.into_owned(),
            enabled: true,
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

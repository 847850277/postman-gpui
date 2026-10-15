//! One retained inspector per (flow, stable step ID). The library owns the typed document;
//! these Kit buffers own unfinished source, selection, composition and undo history.
#[cfg(test)]
mod tests;
mod view;

use super::{
    model::{literal_url, request_summary, StepEdit},
    request_source,
};
use crate::ui::components::{
    input::multiline_input::{MultilineInput, MultilineInputEvent},
    kit_controls::MethodState,
};
use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, ScrollHandle,
    Subscription, Window,
};
use gpui_kit::component::{
    input::{InputEvent, InputState},
    searchable_list::SearchableVec,
    select::{SelectEvent, SelectState},
    IndexPath,
};
use postman_flow::{
    parse_body_expression, parse_header_expressions, parse_text_expression, write_body_expression,
    write_header_expressions, write_text_expression, BodyTemplate, HttpRequestSource,
    HttpStepDefinition, TextTemplate,
};

const METHODS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

pub(super) struct RequestInspector {
    step_id: String,
    name: Entity<InputState>,
    url: Entity<InputState>,
    method: Entity<MethodState>,
    url_expression: Entity<MultilineInput>,
    headers: Entity<MultilineInput>,
    header_expression: Entity<MultilineInput>,
    body: Entity<MultilineInput>,
    expression_url: bool,
    expression_headers: bool,
    simple_url_available: bool,
    simple_headers_available: bool,
    url_error: Option<String>,
    headers_error: Option<String>,
    body_error: Option<String>,
    headers_open: bool,
    header_count: usize,
    variables: Vec<String>,
    readonly: Option<String>,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<StepEdit> for RequestInspector {}
impl Focusable for RequestInspector {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.name.focus_handle(cx)
    }
}

fn multiline(
    label: &'static str,
    placeholder: &'static str,
    source: String,
    window: &mut Window,
    cx: &mut Context<RequestInspector>,
) -> Entity<MultilineInput> {
    cx.new(|cx| {
        let mut input = MultilineInput::new(label, placeholder, window, cx);
        input.project_content(source, window, cx);
        input
    })
}

impl RequestInspector {
    #[cfg(test)]
    pub(super) fn name(&self) -> &Entity<InputState> {
        &self.name
    }
    #[cfg(test)]
    pub(super) fn url(&self) -> &Entity<InputState> {
        &self.url
    }
    pub(super) fn new(
        step: &HttpStepDefinition,
        variables: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let request = match &step.request {
            HttpRequestSource::Inline(request) => Some(request),
            _ => None,
        };
        let literal = request.and_then(|r| literal_url(&r.url));
        let simple_headers = request.and_then(|r| request_source::simple_headers(&r.headers));
        let name = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("Step name");
            input.set_value(step.name.clone(), window, cx);
            input
        });
        let url = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("https://httpbingo.org/get");
            input.set_value(literal.unwrap_or("").to_owned(), window, cx);
            input
        });
        let method = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(METHODS.to_vec()),
                request
                    .and_then(|r| METHODS.iter().position(|m| *m == r.method.to_string()))
                    .map(IndexPath::new),
                window,
                cx,
            )
        });
        let url_expression = multiline(
            "URL expression",
            "Native text expression (JSON)",
            request
                .map(|r| write_text_expression(&r.url).expect("serializable text expression"))
                .unwrap_or_default(),
            window,
            cx,
        );
        let headers = multiline(
            "Literal headers",
            "Name: value",
            simple_headers.clone().unwrap_or_default(),
            window,
            cx,
        );
        let header_expression = multiline(
            "Header expressions",
            "Ordered array of {name, value} expressions",
            request
                .map(|r| {
                    write_header_expressions(&r.headers).expect("serializable header expressions")
                })
                .unwrap_or_default(),
            window,
            cx,
        );
        let body = multiline(
            "Body expression",
            "No request body",
            request
                .filter(|r| r.body != BodyTemplate::None)
                .map(|r| write_body_expression(&r.body).expect("serializable body expression"))
                .unwrap_or_default(),
            window,
            cx,
        );
        let mut subscriptions = Vec::new();
        subscriptions.push(cx.subscribe(&name, |_, input, event, cx| {
            if matches!(event, InputEvent::Change) {
                cx.emit(StepEdit::Name(input.read(cx).value().to_string()));
                cx.notify();
            }
        }));
        subscriptions.push(cx.subscribe(&method, |this, _, event, cx| {
            if this.readonly.is_none() {
                if let SelectEvent::Confirm(Some(method)) = event {
                    cx.emit(StepEdit::Method((*method).into()));
                    cx.notify();
                }
            }
        }));
        subscriptions.push(cx.subscribe(&url, |this, input, event, cx| {
            if !this.expression_url
                && this.readonly.is_none()
                && matches!(event, InputEvent::Change)
            {
                cx.emit(StepEdit::Url(TextTemplate::literal(
                    input.read(cx).value().to_string(),
                )));
                cx.notify();
            }
        }));
        subscriptions.push(cx.subscribe(&url_expression, |this, _, event, cx| {
            if !this.expression_url || this.readonly.is_some() {
                return;
            }
            let MultilineInputEvent::ValueChanged(source) = event;
            let parsed = parse_text_expression(source);
            this.simple_url_available = parsed
                .as_ref()
                .ok()
                .and_then(literal_url)
                .is_some_and(|s| !s.contains(['\r', '\n']));
            this.url_error = parsed.as_ref().err().map(ToString::to_string);
            cx.emit(parsed.map(StepEdit::Url).unwrap_or(StepEdit::Invalid));
            cx.notify();
        }));
        for (input, expression) in [(&headers, false), (&header_expression, true)] {
            subscriptions.push(cx.subscribe(input, move |this, _, event, cx| {
                if this.expression_headers != expression || this.readonly.is_some() {
                    return;
                }
                let MultilineInputEvent::ValueChanged(source) = event;
                let parsed = if expression {
                    parse_header_expressions(source).map_err(|e| e.to_string())
                } else {
                    request_source::parse_headers(source)
                };
                this.simple_headers_available = parsed
                    .as_ref()
                    .ok()
                    .and_then(|h| request_source::simple_headers(h))
                    .is_some();
                this.headers_error = parsed.as_ref().err().cloned();
                if let Ok(headers) = &parsed {
                    this.header_count = headers.len();
                }
                cx.emit(parsed.map(StepEdit::Headers).unwrap_or(StepEdit::Invalid));
                cx.notify();
            }));
        }
        subscriptions.push(cx.subscribe(&body, |this, _, event, cx| {
            if this.readonly.is_some() {
                return;
            }
            let MultilineInputEvent::ValueChanged(source) = event;
            let parsed = parse_body_expression(source);
            this.body_error = parsed.as_ref().err().map(ToString::to_string);
            cx.emit(parsed.map(StepEdit::Body).unwrap_or(StepEdit::Invalid));
            cx.notify();
        }));
        Self {
            step_id: step.id.clone(),
            name,
            url,
            method,
            url_expression,
            headers,
            header_expression,
            body,
            expression_url: literal.is_none_or(|s| s.contains(['\r', '\n'])),
            expression_headers: simple_headers.is_none(),
            simple_url_available: literal.is_some_and(|s| !s.contains(['\r', '\n'])),
            simple_headers_available: simple_headers.is_some(),
            url_error: None,
            headers_error: None,
            body_error: None,
            headers_open: false,
            header_count: request.map_or(0, |r| r.headers.len()),
            variables,
            readonly: request.is_none().then(|| request_summary(step).1),
            scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    fn switch_url(&mut self, expression: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.expression_url == expression || self.url_error.is_some() {
            return;
        }
        if expression {
            let value = TextTemplate::literal(self.url.read(cx).value().to_string());
            let source = write_text_expression(&value).expect("serializable literal");
            // Preserve the alternate buffer and its undo if it already expresses the same value.
            if parse_text_expression(&self.url_expression.read(cx).value()).ok() != Some(value) {
                self.url_expression
                    .update(cx, |input, cx| input.project_content(source, window, cx));
            }
        } else {
            let Ok(value) = parse_text_expression(&self.url_expression.read(cx).value()) else {
                return;
            };
            let Some(literal) = literal_url(&value).filter(|s| !s.contains(['\r', '\n'])) else {
                return;
            };
            if self.url.read(cx).value().as_ref() != literal {
                self.url.update(cx, |input, cx| {
                    input.set_value(literal.to_owned(), window, cx)
                });
            }
        }
        self.expression_url = expression;
        self.simple_url_available = true;
        cx.notify();
    }

    fn switch_headers(&mut self, expression: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.expression_headers == expression || self.headers_error.is_some() {
            return;
        }
        if expression {
            let Ok(value) = request_source::parse_headers(&self.headers.read(cx).value()) else {
                return;
            };
            if parse_header_expressions(&self.header_expression.read(cx).value()).ok()
                != Some(value.clone())
            {
                let source = write_header_expressions(&value).expect("serializable headers");
                self.header_expression
                    .update(cx, |input, cx| input.project_content(source, window, cx));
            }
        } else {
            let Ok(value) = parse_header_expressions(&self.header_expression.read(cx).value())
            else {
                return;
            };
            let Some(source) = request_source::simple_headers(&value) else {
                return;
            };
            if request_source::parse_headers(&self.headers.read(cx).value()).ok() != Some(value) {
                self.headers
                    .update(cx, |input, cx| input.project_content(source, window, cx));
            }
        }
        self.expression_headers = expression;
        self.simple_headers_available = true;
        cx.notify();
    }
}

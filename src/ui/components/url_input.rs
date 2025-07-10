use gpui::{
    div, rgb, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, Window,
};

#[derive(Debug, Clone)]
pub enum UrlInputEvent {
    UrlChanged(String),
    SubmitRequested,
}

pub struct UrlInput {
    url: String,
    placeholder: String,
    focus_handle: FocusHandle,
}

impl UrlInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            url: String::new(),
            placeholder: "Enter request URL".to_string(),
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn get_url(&self) -> &str {
        &self.url
    }

    pub fn set_url(&mut self, url: impl Into<String>, cx: &mut Context<Self>) {
        let new_url = url.into();
        if self.url != new_url {
            self.url = new_url.clone();
            cx.emit(UrlInputEvent::UrlChanged(new_url));
            cx.notify();
        }
    }

    pub fn submit_url(&mut self, cx: &mut Context<Self>) {
        println!("Submitted URL: {}", self.url);
        cx.emit(UrlInputEvent::SubmitRequested);
    }

    fn handle_input(&mut self, input: &str, cx: &mut Context<Self>) {
        self.url = input.to_string();
        cx.emit(UrlInputEvent::UrlChanged(self.url.clone()));
        cx.notify();
    }
}

impl EventEmitter<UrlInputEvent> for UrlInput {}

impl Focusable for UrlInput {
    fn focus_handle(&self, _: &gpui::App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for UrlInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("url_input")
            .flex_1()
            .px_4()
            .py_2()
            .bg(rgb(0xffffff))
            .border_1()
            .border_color(rgb(0xcccccc))
            .rounded_md()
            .cursor_text()
            .track_focus(&self.focus_handle)
            .on_click(cx.listener(|this, _event, window, cx| {
                window.focus(&this.focus_handle);
            }))
            .child(
                div()
                    .text_color(if self.url.is_empty() {
                        rgb(0x999999)
                    } else {
                        rgb(0x333333)
                    })
                    .child(if self.url.is_empty() {
                        self.placeholder.clone()
                    } else {
                        self.url.clone()
                    }),
            )
    }
}

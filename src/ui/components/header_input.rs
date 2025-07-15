use gpui::{div, px, rgb, Context, IntoElement, ParentElement, Render, Styled, Window};

pub struct HeaderInput {
    content: String,
    placeholder: String,
}

impl HeaderInput {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            content: String::new(),
            placeholder: "Enter value...".to_string(),
        }
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn get_content(&self) -> &str {
        &self.content
    }

    pub fn set_content(&mut self, content: String, cx: &mut Context<Self>) {
        self.content = content;
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.content.clear();
        cx.notify();
    }

    pub fn append_char(&mut self, ch: char, cx: &mut Context<Self>) {
        self.content.push(ch);
        cx.notify();
    }

    pub fn backspace(&mut self, cx: &mut Context<Self>) {
        if !self.content.is_empty() {
            self.content.pop();
            cx.notify();
        }
    }
}

impl Render for HeaderInput {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_1()
            .px_3()
            .py_2()
            .bg(rgb(0xffffff))
            .border_1()
            .border_color(rgb(0xcccccc))
            .rounded_md()
            .text_size(px(14.0))
            .child(if self.content.is_empty() {
                self.placeholder.clone()
            } else {
                self.content.clone()
            })
            .cursor_text()
    }
}

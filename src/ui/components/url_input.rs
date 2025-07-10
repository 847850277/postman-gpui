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
    is_editing: bool,
}

impl UrlInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            url: String::new(),
            placeholder: "Enter request URL".to_string(),
            focus_handle: cx.focus_handle(),
            is_editing: false,
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

    // 简单的编辑功能
    fn toggle_edit(&mut self, cx: &mut Context<Self>) {
        self.is_editing = !self.is_editing;

        if self.is_editing {
            // 开始编辑 - 可以在这里添加更复杂的输入处理
            println!("开始编辑URL: {}", self.url);
        } else {
            // 结束编辑
            println!("结束编辑URL: {}", self.url);
        }

        cx.notify();
    }

    // 模拟文本输入 - 这里可以替换为真正的键盘输入处理
    fn simulate_input(&mut self, new_text: String, cx: &mut Context<Self>) {
        self.set_url(new_text, cx);
    }

    fn display_text(&self) -> String {
        if self.url.is_empty() {
            self.placeholder.clone()
        } else {
            let mut display = self.url.clone();
            if self.is_editing {
                display.push('|'); // 简单的光标显示
            }
            display
        }
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
                this.toggle_edit(cx);
            }))
            .child(
                div()
                    .text_color(if self.url.is_empty() {
                        rgb(0x999999) // 占位符颜色
                    } else {
                        rgb(0x333333) // 正常文本颜色
                    })
                    .child(self.display_text()),
            )
    }
}

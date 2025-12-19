use gpui::{
    actions, div, px, rgb, ClipboardItem, Context, FocusHandle, Focusable, FontWeight,
    InteractiveElement, IntoElement, KeyBinding, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window,
};

actions!(
    response_viewer,
    [Copy, SelectAll]
);

pub fn setup_response_viewer_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-c", Copy, None),
        KeyBinding::new("ctrl-c", Copy, None),
        KeyBinding::new("cmd-a", SelectAll, None),
        KeyBinding::new("ctrl-a", SelectAll, None),
    ]
}

/// Response 状态
#[derive(Clone, Debug)]
pub enum ResponseState {
    /// 未发送请求
    NotSent,
    /// 加载中
    Loading,
    /// 已收到响应
    Success { status: u16, body: String },
    /// 请求失败
    Error { message: String },
}

/// Response 查看器组件
pub struct ResponseViewer {
    state: ResponseState,
    focus_handle: FocusHandle,
}

impl Focusable for ResponseViewer {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ResponseViewer {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            state: ResponseState::NotSent,
            focus_handle: cx.focus_handle(),
        }
    }

    /// 设置为加载状态
    pub fn set_loading(&mut self, cx: &mut Context<Self>) {
        self.state = ResponseState::Loading;
        cx.notify();
    }

    /// 设置成功响应
    pub fn set_success(&mut self, status: u16, body: String, cx: &mut Context<Self>) {
        self.state = ResponseState::Success { status, body };
        cx.notify();
    }

    /// 设置错误状态
    pub fn set_error(&mut self, message: String, cx: &mut Context<Self>) {
        self.state = ResponseState::Error { message };
        cx.notify();
    }

    /// 清空响应
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.state = ResponseState::NotSent;
        cx.notify();
    }

    /// 获取当前状态
    pub fn get_state(&self) -> &ResponseState {
        &self.state
    }

    fn get_content(&self) -> String {
        match &self.state {
            ResponseState::Success { body, .. } => body.clone(),
            ResponseState::Error { message } => message.clone(),
            _ => String::new(),
        }
    }

    fn copy(&mut self, _: &Copy, _window: &mut Window, cx: &mut Context<Self>) {
        let content = self.get_content();
        if !content.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(content));
        }
    }

    fn select_all(&mut self, _: &SelectAll, _window: &mut Window, _cx: &mut Context<Self>) {
        // Text selection is handled by the browser/OS for simple text elements
        // This action is kept for keyboard shortcut compatibility
    }

    fn render_selectable_content(&self, content: &str, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("response-content")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::select_all))
            .cursor_text()
            .w_full()
            .h_64()
            .px_3()
            .py_2()
            .bg(rgb(0x00f8_f9fa))
            .border_1()
            .border_color(rgb(0x00cc_cccc))
            .overflow_scroll()
            .text_size(px(12.0))
            .child(content.to_string())
    }
}

impl Render for ResponseViewer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .child("Response")
                    .text_size(px(16.0))
                    .font_weight(FontWeight::MEDIUM),
            )
            .child(match &self.state {
                ResponseState::NotSent => {
                    // 未发送请求状态
                    div()
                        .w_full()
                        .h_64()
                        .px_3()
                        .py_2()
                        .bg(rgb(0x00f8_f9fa))
                        .border_1()
                        .border_color(rgb(0x00cc_cccc))
                        .child("No response yet...")
                }
                ResponseState::Loading => {
                    // 加载中状态
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .child("🔄 发送请求中...")
                                .text_color(rgb(0x0000_7acc))
                                .font_weight(FontWeight::MEDIUM),
                        )
                        .child(
                            div()
                                .w_full()
                                .h_64()
                                .px_3()
                                .py_2()
                                .bg(rgb(0x00f8_f9fa))
                                .border_1()
                                .border_color(rgb(0x00cc_cccc))
                                .child("请稍等，正在处理请求..."),
                        )
                }
                ResponseState::Success { status, body } => {
                    // 成功响应状态
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .child(format!("Status: {status}"))
                                .text_color(if *status < 400 {
                                    rgb(0x0028_a745) // 成功
                                } else {
                                    rgb(0x00dc_3545) // 客户端/服务器错误
                                })
                                .font_weight(FontWeight::MEDIUM),
                        )
                        .child(self.render_selectable_content(body, cx))
                }
                ResponseState::Error { message } => {
                    // 错误状态
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .child("Status: Error")
                                .text_color(rgb(0x00dc_3545))
                                .font_weight(FontWeight::MEDIUM),
                        )
                        .child(self.render_selectable_content(message, cx))
                }
            })
    }
}

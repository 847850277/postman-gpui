use gpui::{
    actions, div, px, rgb, ClipboardItem, Context, FocusHandle, Focusable, FontWeight,
    InteractiveElement, IntoElement, KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement, Pixels, Point, Render, StatefulInteractiveElement, Styled,
    Window,
};
use std::ops::Range;

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
    selected_range: Range<usize>,
    is_selecting: bool,
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
            selected_range: 0..0,
            is_selecting: false,
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
        self.selected_range = 0..0;
        cx.notify();
    }

    /// 设置错误状态
    pub fn set_error(&mut self, message: String, cx: &mut Context<Self>) {
        self.state = ResponseState::Error { message };
        self.selected_range = 0..0;
        cx.notify();
    }

    /// 清空响应
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.state = ResponseState::NotSent;
        self.selected_range = 0..0;
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
        if !self.selected_range.is_empty() {
            let content = self.get_content();
            if !content.is_empty() {
                // Ensure indices are within bounds and on character boundaries
                let start = self.selected_range.start.min(content.len());
                let end = self.selected_range.end.min(content.len());
                
                // Use character-based slicing to avoid UTF-8 boundary issues
                let selected_text: String = content
                    .chars()
                    .skip(start)
                    .take(end.saturating_sub(start))
                    .collect();
                
                if !selected_text.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(selected_text));
                }
            }
        }
    }

    fn select_all(&mut self, _: &SelectAll, _window: &mut Window, cx: &mut Context<Self>) {
        let content = self.get_content();
        // Use character count instead of byte length for consistency with character-based indexing
        self.selected_range = 0..content.chars().count();
        cx.notify();
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = true;
        let index = self.index_for_mouse_position(event.position);
        self.selected_range = index..index;
        cx.notify();
    }

    fn on_mouse_up(&mut self, _event: &MouseUpEvent, _window: &mut Window, _cx: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_selecting {
            let index = self.index_for_mouse_position(event.position);
            let start = self.selected_range.start;
            
            // Normalize the range: always ensure start <= end
            if index < start {
                self.selected_range = index..start;
            } else {
                self.selected_range = start..index;
            }
            
            cx.notify();
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        // LIMITATION: This is a rough approximation using fixed thresholds
        // A production implementation should use GPUI's text layout APIs for accurate positioning
        // The current approach provides basic functionality but won't be pixel-perfect
        // 
        // The hardcoded thresholds work reasonably well for typical response sizes
        // but may be inaccurate for very long lines or different font sizes
        
        let content = self.get_content();
        if content.is_empty() {
            return 0;
        }
        
        // Approximate character dimensions for 12px monospace font
        // These are rough estimates - actual positioning would require text layout info
        let char_width_px = 7.2;
        let line_height_px = 16.0;
        let padding_px = 12.0; // px_3() = 12px padding
        
        // Convert position to approximate line and column
        // Note: We can't access Pixels internal value directly, so we work around it
        // by using comparison with known pixel values
        let adjusted_y = if position.y > px(padding_px) {
            // Approximate by comparing with reference pixels
            let lines = if position.y > px(padding_px + line_height_px * 10.0) {
                10
            } else if position.y > px(padding_px + line_height_px * 5.0) {
                5
            } else if position.y > px(padding_px + line_height_px * 2.0) {
                2
            } else if position.y > px(padding_px + line_height_px) {
                1
            } else {
                0
            };
            lines
        } else {
            0
        };
        
        let adjusted_x = if position.x > px(padding_px) {
            // Similar approximation for column
            let cols = if position.x > px(padding_px + char_width_px * 50.0) {
                50
            } else if position.x > px(padding_px + char_width_px * 20.0) {
                20
            } else if position.x > px(padding_px + char_width_px * 10.0) {
                10
            } else if position.x > px(padding_px + char_width_px * 5.0) {
                5
            } else if position.x > px(padding_px + char_width_px) {
                1
            } else {
                0
            };
            cols
        } else {
            0
        };
        
        // Calculate character index from line and column
        let lines: Vec<&str> = content.lines().collect();
        let mut char_index = 0;
        
        for (i, line) in lines.iter().enumerate() {
            if i < adjusted_y {
                char_index += line.chars().count() + 1; // +1 for newline
            } else if i == adjusted_y {
                let line_chars: Vec<char> = line.chars().collect();
                char_index += adjusted_x.min(line_chars.len());
                break;
            }
        }
        
        // Ensure index is within bounds
        char_index.min(content.chars().count())
    }

    fn render_selectable_content(&self, content: &str, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("response-content")
            .track_focus(&self.focus_handle(cx))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
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
            .child(
                div()
                    .text_size(px(12.0))
                    .font_family("monospace")
                    .child(content.to_string()),
            )
    }
}

impl Render for ResponseViewer {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
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
                        .child(self.render_selectable_content(body, _cx))
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
                        .child(self.render_selectable_content(message, _cx))
                }
            })
    }
}

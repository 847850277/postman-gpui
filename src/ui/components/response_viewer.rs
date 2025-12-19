use gpui::{
    actions, div, px, rgb, ClipboardItem, Context, FocusHandle, Focusable, FontWeight,
    InteractiveElement, IntoElement, KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement, Pixels, Point, Render, StatefulInteractiveElement, Styled,
    Window,
};
use std::ops::Range;

// Approximate font metrics for 12px monospace font
const APPROX_CHAR_WIDTH_PX: f32 = 7.2;
const APPROX_LINE_HEIGHT_PX: f32 = 16.0;
const CONTENT_PADDING_PX: f32 = 12.0; // px_3() = 12px padding

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
                // Use character-based slicing to avoid UTF-8 boundary issues
                // chars().skip().take() naturally handles out-of-bounds indices
                let selected_text: String = content
                    .chars()
                    .skip(self.selected_range.start)
                    .take(self.selected_range.end.saturating_sub(self.selected_range.start))
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
            let selection_start = self.selected_range.start;
            
            // Normalize the range: always ensure start <= end
            if index < selection_start {
                self.selected_range = index..selection_start;
            } else {
                self.selected_range = selection_start..index;
            }
            
            cx.notify();
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        // LIMITATION: This is a rough approximation for monospace text
        // A production implementation should use GPUI's text layout APIs for accurate positioning
        
        let content = self.get_content();
        if content.is_empty() {
            return 0;
        }
        
        // Use mathematical calculation to estimate line and column based on position
        // This uses approximate metrics but works better than hardcoded thresholds
        let estimated_line = {
            // Calculate approximate line by dividing Y position by line height
            let mut line_estimate = 0;
            for threshold in 1..=100 {
                if position.y > px(CONTENT_PADDING_PX + APPROX_LINE_HEIGHT_PX * threshold as f32) {
                    line_estimate = threshold;
                } else {
                    break;
                }
            }
            line_estimate
        };
        
        let estimated_column = {
            // Calculate approximate column by dividing X position by character width
            let mut col_estimate = 0;
            for threshold in 1..=200 {
                if position.x > px(CONTENT_PADDING_PX + APPROX_CHAR_WIDTH_PX * threshold as f32) {
                    col_estimate = threshold;
                } else {
                    break;
                }
            }
            col_estimate
        };
        
        // Convert line and column to character index
        let lines: Vec<&str> = content.lines().collect();
        let mut char_index = 0;
        
        for (i, line) in lines.iter().enumerate() {
            if i < estimated_line {
                char_index += line.chars().count() + 1; // +1 for newline
            } else if i == estimated_line {
                let line_char_count = line.chars().count();
                char_index += estimated_column.min(line_char_count);
                break;
            }
        }
        
        // Handle case where click is beyond the last line
        if estimated_line >= lines.len() {
            char_index = content.chars().count();
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

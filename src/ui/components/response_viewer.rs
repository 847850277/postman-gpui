use gpui::{
    actions, div, fill, point, px, rgb, rgba, App, Bounds, ClipboardItem, Context, Element,
    ElementId, Entity, FocusHandle, Focusable, FontWeight, GlobalElementId, InteractiveElement,
    IntoElement, KeyBinding, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    PaintQuad, ParentElement, Pixels, Point, Render, ShapedLine, StatefulInteractiveElement,
    Style, Styled, TextRun, Window,
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
    last_bounds: Option<Bounds<Pixels>>,
    last_layout: Option<ShapedLine>,
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
            last_bounds: None,
            last_layout: None,
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
        // Try to use accurate text layout if available
        if let (Some(last_bounds), Some(last_layout)) = (&self.last_bounds, &self.last_layout) {
            // Convert to local coordinates
            if let Some(line_point) = last_bounds.localize(&position) {
                if let Some(index) = last_layout.index_for_x(line_point.x) {
                    return index;
                }
            }
        }
        
        // Fallback to approximation if layout not available
        let content = self.get_content();
        if content.is_empty() {
            return 0;
        }
        
        // Use mathematical calculation to estimate line and column based on position
        let estimated_line = {
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

    fn render_selectable_content(&self, _content: &str, cx: &mut Context<Self>) -> impl IntoElement {
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
            .child(ResponseTextElement {
                viewer: cx.entity().clone(),
            })
    }
}

// Custom text element for rendering response content with selection
struct ResponseTextElement {
    viewer: Entity<ResponseViewer>,
}

struct PrepaintState {
    line: Option<ShapedLine>,
    selection: Option<PaintQuad>,
    cursor: Option<PaintQuad>,
}

impl IntoElement for ResponseTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ResponseTextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = gpui::relative(1.).into();
        
        // Calculate height based on content line count
        let viewer = self.viewer.read(cx);
        let content = viewer.get_content();
        let line_count = content.lines().count().max(1);
        let line_height = window.line_height();
        // Use relative height - let the container handle scrolling
        style.size.height = (line_height * line_count as f32).into();
        
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let viewer = self.viewer.read(cx);
        let content = viewer.get_content();
        let selected_range = viewer.selected_range.clone();
        
        let style = window.text_style();
        let font_size = px(12.0); // Match the text size in the old implementation
        
        let run = TextRun {
            len: content.len(),
            font: style.font(),
            color: style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        
        let shaped_line = window
            .text_system()
            .shape_line(content.clone().into(), font_size.into(), &[run], None);
        
        // Calculate cursor position
        let cursor_pos = if content.is_empty() {
            px(0.0)
        } else {
            shaped_line.x_for_index(selected_range.start)
        };
        
        // Calculate selection visual and cursor (cursor only shows when no selection)
        let (selection, cursor) = if selected_range.is_empty() {
            // Show cursor when no selection
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_pos, bounds.top()),
                        gpui::size(px(2.), bounds.bottom() - bounds.top()),
                    ),
                    rgb(0x0000_7acc), // Blue cursor
                )),
            )
        } else if !content.is_empty() {
            // Show selection when text is selected
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() + shaped_line.x_for_index(selected_range.start),
                            bounds.top(),
                        ),
                        point(
                            bounds.left() + shaped_line.x_for_index(selected_range.end),
                            bounds.bottom(),
                        ),
                    ),
                    rgba(0x3366_ff55), // Semi-transparent blue selection
                )),
                None,
            )
        } else {
            (None, None)
        };
        
        // Store layout for click handling
        self.viewer.update(cx, |viewer, _cx| {
            viewer.last_layout = Some(shaped_line.clone());
            viewer.last_bounds = Some(bounds);
        });
        
        PrepaintState {
            line: Some(shaped_line),
            selection,
            cursor,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        _cx: &mut App,
    ) {
        // Paint selection first (behind text)
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        
        // Paint cursor (behind text but after selection)
        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }
        
        // Paint text
        if let Some(line) = &prepaint.line {
            line.paint(bounds.origin, window.line_height(), window, _cx).ok();
        }
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

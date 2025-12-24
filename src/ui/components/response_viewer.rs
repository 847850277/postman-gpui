use gpui::{
    actions, div, fill, point, px, rgb, rgba, App, Bounds, ClipboardItem, Context, CursorStyle,
    Element, ElementId, Entity, FocusHandle, Focusable, FontWeight, GlobalElementId,
    InteractiveElement, IntoElement, KeyBinding, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PaintQuad, ParentElement, Pixels, Point, Render, ShapedLine,
    StatefulInteractiveElement, Style, Styled, TextAlign, TextRun, Window,
};
use std::ops::Range;

// Approximate font metrics for 12px monospace font
const APPROX_CHAR_WIDTH_PX: f32 = 7.2;
const APPROX_LINE_HEIGHT_PX: f32 = 16.0;
const CONTENT_PADDING_PX: f32 = 12.0; // px_3() = 12px padding

actions!(response_viewer, [Copy, SelectAll]);

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
    selection_reversed: bool,
    is_selecting: bool,
    last_bounds: Option<Bounds<Pixels>>,
    last_lines_layout: Vec<(ShapedLine, usize)>, // (shaped_line, char_offset)
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
            selection_reversed: false,
            is_selecting: false,
            last_bounds: None,
            last_lines_layout: Vec::new(),
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
                let selected_text: String = content
                    .chars()
                    .skip(self.selected_range.start)
                    .take(
                        self.selected_range
                            .end
                            .saturating_sub(self.selected_range.start),
                    )
                    .collect();

                if !selected_text.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(selected_text));
                }
            }
        }
    }

    fn select_all(&mut self, _: &SelectAll, _window: &mut Window, cx: &mut Context<Self>) {
        let content = self.get_content();
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
        if event.modifiers.shift {
            self.response_select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.response_move_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn response_move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        cx.notify();
    }

    fn on_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.is_selecting = false;
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_selecting {
            let offset = self.index_for_mouse_position(event.position);
            self.response_select_to(offset, cx);
        }
    }

    fn response_select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }

        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify();
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        let content = self.get_content();
        if content.is_empty() {
            return 0;
        }

        let Some(bounds) = self.last_bounds.as_ref() else {
            return 0;
        };

        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return content.chars().count();
        }

        if self.last_lines_layout.is_empty() {
            return 0;
        }

        let line_height = bounds.size.height / self.last_lines_layout.len() as f32;
        let mut line_index = ((position.y - bounds.top()) / line_height).floor() as usize;
        line_index = line_index.min(self.last_lines_layout.len().saturating_sub(1));

        let (shaped_line, line_char_offset) = &self.last_lines_layout[line_index];
        let x_in_line = position.x - bounds.left();
        let offset_in_line = shaped_line.closest_index_for_x(x_in_line);

        let absolute_offset = line_char_offset.saturating_add(offset_in_line);
        absolute_offset.min(content.chars().count())
    }

    fn render_selectable_content(
        &self,
        _content: &str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("response-content")
            .border_1()
            .border_color(rgb(0x00cc_cccc))
            .rounded_md()
            .cursor(CursorStyle::IBeam)
            .track_focus(&self.focus_handle(cx))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::select_all))
            .cursor_text()
            .w_full()
            .h_64()
            .px_3()
            .py_2()
            .bg(rgb(0x00f8_f9fa))
            .overflow_scroll()
            .child(MultiLineTextElement {
                viewer: cx.entity().clone(),
            })
    }
}

// Custom text element for rendering multi-line response content with selection
struct MultiLineTextElement {
    viewer: Entity<ResponseViewer>,
}

struct PrepaintState {
    lines: Vec<(ShapedLine, usize)>,
    selections: Vec<PaintQuad>,
    cursor: Option<PaintQuad>,
}

impl IntoElement for MultiLineTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for MultiLineTextElement {
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

        let viewer = self.viewer.read(cx);
        let content = viewer.get_content();
        let line_count = content.lines().count().max(1);
        let line_height = window.line_height();
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
        let font_size = px(12.0);
        let line_height = window.line_height();

        let lines: Vec<&str> = content.lines().collect();
        let mut shaped_lines = Vec::new();
        let mut char_offset = 0;

        for line in &lines {
            let run = TextRun {
                len: line.len(),
                font: style.font(),
                color: style.color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };

            let shaped_line = window.text_system().shape_line(
                (*line).to_string().into(),
                font_size.into(),
                &[run],
                None,
            );

            shaped_lines.push((shaped_line, char_offset));
            char_offset += line.chars().count() + 1;
        }

        let mut selections = Vec::new();
        let mut cursor = None;

        if selected_range.is_empty() && !content.is_empty() {
            let cursor_char = selected_range.start;
            let mut current_offset = 0;

            for (line_idx, (_shaped_line, _)) in shaped_lines.iter().enumerate() {
                let line_len = if line_idx < lines.len() {
                    lines[line_idx].chars().count()
                } else {
                    0
                };

                if cursor_char >= current_offset && cursor_char <= current_offset + line_len {
                    let local_pos = cursor_char - current_offset;
                    let x_pos = if local_pos == 0 {
                        px(0.0)
                    } else {
                        let line_text: String = lines[line_idx].chars().take(local_pos).collect();
                        let temp_run = TextRun {
                            len: line_text.len(),
                            font: style.font(),
                            color: style.color,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        let temp_line = window.text_system().shape_line(
                            line_text.into(),
                            font_size.into(),
                            &[temp_run],
                            None,
                        );
                        temp_line.x_for_index(temp_line.len())
                    };

                    cursor = Some(fill(
                        Bounds::new(
                            point(
                                bounds.left() + x_pos,
                                bounds.top() + line_height * line_idx as f32,
                            ),
                            gpui::size(px(2.), line_height),
                        ),
                        rgb(0x0000_7acc),
                    ));
                    break;
                }

                current_offset += line_len + 1;
            }
        } else if !selected_range.is_empty() && !content.is_empty() {
            let mut current_offset = 0;

            for (line_idx, (shaped_line, _)) in shaped_lines.iter().enumerate() {
                let line_len = if line_idx < lines.len() {
                    lines[line_idx].chars().count()
                } else {
                    0
                };

                let line_start = current_offset;
                let line_end = current_offset + line_len;

                if selected_range.end > line_start && selected_range.start < line_end {
                    let sel_start = selected_range.start.max(line_start).min(line_end);
                    let sel_end = selected_range.end.max(line_start).min(line_end);

                    let local_start = sel_start - line_start;
                    let local_end = sel_end - line_start;

                    let start_x = if local_start == 0 {
                        px(0.0)
                    } else {
                        let text_before: String =
                            lines[line_idx].chars().take(local_start).collect();
                        let temp_run = TextRun {
                            len: text_before.len(),
                            font: style.font(),
                            color: style.color,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        let temp_line = window.text_system().shape_line(
                            text_before.into(),
                            font_size.into(),
                            &[temp_run],
                            None,
                        );
                        temp_line.x_for_index(temp_line.len())
                    };

                    let end_x = if local_end == 0 {
                        px(0.0)
                    } else if local_end >= line_len {
                        shaped_line.width
                    } else {
                        let text_before: String = lines[line_idx].chars().take(local_end).collect();
                        let temp_run = TextRun {
                            len: text_before.len(),
                            font: style.font(),
                            color: style.color,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        let temp_line = window.text_system().shape_line(
                            text_before.into(),
                            font_size.into(),
                            &[temp_run],
                            None,
                        );
                        temp_line.x_for_index(temp_line.len())
                    };

                    selections.push(fill(
                        Bounds::from_corners(
                            point(
                                bounds.left() + start_x,
                                bounds.top() + line_height * line_idx as f32,
                            ),
                            point(
                                bounds.left() + end_x,
                                bounds.top() + line_height * (line_idx + 1) as f32,
                            ),
                        ),
                        rgba(0x3366_ff55),
                    ));
                }

                current_offset += line_len + 1;
            }
        }

        self.viewer.update(cx, |viewer, _cx| {
            viewer.last_lines_layout = shaped_lines.clone();
            viewer.last_bounds = Some(bounds);
        });

        PrepaintState {
            lines: shaped_lines,
            selections,
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
        cx: &mut App,
    ) {
        let line_height = window.line_height();

        for selection in &prepaint.selections {
            window.paint_quad(selection.clone());
        }

        if let Some(cursor) = &prepaint.cursor {
            window.paint_quad(cursor.clone());
        }

        for (line_idx, (shaped_line, _)) in prepaint.lines.iter().enumerate() {
            let origin = point(
                bounds.origin.x,
                bounds.origin.y + line_height * line_idx as f32,
            );
            shaped_line
                .paint(origin, line_height, TextAlign::Left, None, window, cx)
                .ok();
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

use gpui::{
    actions, div, fill, hsla, point, prelude::*, px, relative, rgb, rgba, size, App, Bounds,
    ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, IntoElement,
    KeyBinding, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad,
    ParentElement, Pixels, Point, Render, ShapedLine, SharedString, Style, Styled, TextRun,
    UTF16Selection, Window, FontWeight,
};
use std::ops::Range;
use unicode_segmentation::*;

// 定义actions - 这些是键盘快捷键对应的动作
actions!(
    body_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectAll,
        Home,
        End,
        Paste,
        Cut,
        Copy,
        NewLine,
        Tab,
    ]
);

#[derive(Debug, Clone, PartialEq)]
pub enum BodyType {
    Json,
    FormData,
    Raw,
}

#[derive(Debug, Clone)]
pub enum BodyInputEvent {
    ValueChanged(String),
    TypeChanged(BodyType),
}

#[derive(Debug, Clone)]
pub struct FormDataEntry {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

pub struct BodyInput {
    focus_handle: FocusHandle,
    // JSON 内容
    json_content: SharedString,
    json_placeholder: SharedString,
    json_selected_range: Range<usize>,
    json_selection_reversed: bool,
    json_marked_range: Option<Range<usize>>,
    json_last_layout: Option<Vec<ShapedLine>>,
    json_last_bounds: Option<Bounds<Pixels>>,
    
    // Form Data 内容
    form_data_entries: Vec<FormDataEntry>,
    
    // 当前选中的类型
    current_type: BodyType,
    
    // Raw 内容（兼容旧版本）
    raw_content: SharedString,
    raw_placeholder: SharedString,
    raw_selected_range: Range<usize>,
    raw_selection_reversed: bool,
    raw_marked_range: Option<Range<usize>>,
    raw_last_layout: Option<Vec<ShapedLine>>,
    raw_last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
}

impl BodyInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            // JSON 初始化
            json_content: "".into(),
            json_placeholder: "Enter JSON body...".into(),
            json_selected_range: 0..0,
            json_selection_reversed: false,
            json_marked_range: None,
            json_last_layout: None,
            json_last_bounds: None,
            
            // Form Data 初始化
            form_data_entries: vec![FormDataEntry {
                key: String::new(),
                value: String::new(),
                enabled: true,
            }],
            
            // 默认类型
            current_type: BodyType::Json,
            
            // Raw 初始化
            raw_content: "".into(),
            raw_placeholder: "Enter raw body...".into(),
            raw_selected_range: 0..0,
            raw_selection_reversed: false,
            raw_marked_range: None,
            raw_last_layout: None,
            raw_last_bounds: None,
            is_selecting: false,
        }
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        let placeholder_str = placeholder.into();
        self.json_placeholder = format!("{} (JSON)", placeholder_str).into();
        self.raw_placeholder = format!("{} (Raw)", placeholder_str).into();
        self
    }

    pub fn get_current_type(&self) -> &BodyType {
        &self.current_type
    }

    pub fn set_type(&mut self, body_type: BodyType, cx: &mut Context<Self>) {
        if self.current_type != body_type {
            self.current_type = body_type.clone();
            cx.emit(BodyInputEvent::TypeChanged(body_type));
            cx.notify();
        }
    }

    pub fn get_content(&self) -> &str {
        match &self.current_type {
            BodyType::Json => &self.json_content,
            BodyType::Raw => &self.raw_content,
            BodyType::FormData => {
                // 对于 FormData，返回一个简化的表示
                ""
            }
        }
    }

    pub fn get_json_content(&self) -> &str {
        &self.json_content
    }

    pub fn get_form_data_entries(&self) -> &Vec<FormDataEntry> {
        &self.form_data_entries
    }

    pub fn set_content(&mut self, content: impl Into<String>, cx: &mut Context<Self>) {
        let new_content: SharedString = content.into().into();
        
        match &self.current_type {
            BodyType::Json => {
                if self.json_content != new_content {
                    self.json_content = new_content.clone();
                    let cursor_position = self.json_selected_range.start.min(self.json_content.len());
                    self.json_selected_range = cursor_position..cursor_position;
                    self.json_selection_reversed = false;
                    cx.emit(BodyInputEvent::ValueChanged(new_content.to_string()));
                    cx.notify();
                }
            }
            BodyType::Raw => {
                if self.raw_content != new_content {
                    self.raw_content = new_content.clone();
                    let cursor_position = self.raw_selected_range.start.min(self.raw_content.len());
                    self.raw_selected_range = cursor_position..cursor_position;
                    self.raw_selection_reversed = false;
                    cx.emit(BodyInputEvent::ValueChanged(new_content.to_string()));
                    cx.notify();
                }
            }
            BodyType::FormData => {
                // FormData 不支持直接设置内容
            }
        }
    }

    pub fn add_form_data_entry(&mut self, cx: &mut Context<Self>) {
        self.form_data_entries.push(FormDataEntry {
            key: String::new(),
            value: String::new(),
            enabled: true,
        });
        cx.notify();
    }

    pub fn remove_form_data_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.form_data_entries.len() {
            self.form_data_entries.remove(index);
            if self.form_data_entries.is_empty() {
                // 至少保留一个空条目
                self.form_data_entries.push(FormDataEntry {
                    key: String::new(),
                    value: String::new(),
                    enabled: true,
                });
            }
            cx.notify();
        }
    }

    pub fn update_form_data_entry(&mut self, index: usize, key: String, value: String, enabled: bool, cx: &mut Context<Self>) {
        if let Some(entry) = self.form_data_entries.get_mut(index) {
            entry.key = key;
            entry.value = value;
            entry.enabled = enabled;
            cx.emit(BodyInputEvent::ValueChanged(self.get_form_data_as_string()));
            cx.notify();
        }
    }

    pub fn get_form_data_as_string(&self) -> String {
        self.form_data_entries
            .iter()
            .filter(|entry| entry.enabled && !entry.key.is_empty())
            .map(|entry| format!("{}={}", entry.key, entry.value))
            .collect::<Vec<_>>()
            .join("&")
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        match &self.current_type {
            BodyType::Json => {
                self.json_content = "".into();
                self.json_selected_range = 0..0;
                self.json_selection_reversed = false;
            }
            BodyType::Raw => {
                self.raw_content = "".into();
                self.raw_selected_range = 0..0;
                self.raw_selection_reversed = false;
            }
            BodyType::FormData => {
                self.form_data_entries = vec![FormDataEntry {
                    key: String::new(),
                    value: String::new(),
                    enabled: true,
                }];
            }
        }
        cx.emit(BodyInputEvent::ValueChanged(String::new()));
        cx.notify();
    }

    pub fn is_empty(&self) -> bool {
        match &self.current_type {
            BodyType::Json => self.json_content.is_empty(),
            BodyType::Raw => self.raw_content.is_empty(),
            BodyType::FormData => {
                self.form_data_entries.iter().all(|entry| entry.key.is_empty() && entry.value.is_empty())
            }
        }
    }
    }

    // Action handlers - 这些方法处理键盘动作
    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        let cursor = self.cursor_offset();
        if let Some(new_offset) = self.offset_for_line_up(cursor) {
            self.move_to(new_offset, cx);
        }
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        let cursor = self.cursor_offset();
        if let Some(new_offset) = self.offset_for_line_down(cursor) {
            self.move_to(new_offset, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        let cursor = self.cursor_offset();
        if let Some(new_offset) = self.offset_for_line_up(cursor) {
            self.select_to(new_offset, cx);
        }
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        let cursor = self.cursor_offset();
        if let Some(new_offset) = self.offset_for_line_down(cursor) {
            self.select_to(new_offset, cx);
        }
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        let cursor = self.cursor_offset();
        let line_start = self.line_start_for_offset(cursor);
        self.move_to(line_start, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        let cursor = self.cursor_offset();
        let line_end = self.line_end_for_offset(cursor);
        self.move_to(line_end, cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn new_line(&mut self, _: &NewLine, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_text_in_range(None, "\n", window, cx);
    }

    fn tab(&mut self, _: &Tab, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_text_in_range(None, "    ", window, cx); // 4 spaces for tab
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    // Mouse event handlers
    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = true;

        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    // Helper methods
    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }

        let (Some(bounds), Some(lines)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };

        if position.y < bounds.top() {
            return 0;
        }

        // 计算点击在哪一行
        let line_height = (bounds.bottom() - bounds.top()).0 / lines.len() as f32;
        let line_index = ((position.y - bounds.top()).0 / line_height).floor() as usize;

        if line_index >= lines.len() {
            return self.content.len();
        }

        let line = &lines[line_index];
        let x_offset = position.x - bounds.left();

        // 计算当前行在原始文本中的起始位置
        let mut line_start = 0;
        for (i, _) in self.content.split('\n').enumerate() {
            if i == line_index {
                break;
            }
            line_start += self.content.split('\n').nth(i).unwrap_or("").len() + 1;
            // +1 for \n
        }

        line_start + line.closest_index_for_x(x_offset)
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
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

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>, content: &str) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start, content)..self.offset_from_utf16(range_utf16.end, content)
    }

    fn range_to_utf16(&self, range: &Range<usize>, content: &str) -> Range<usize> {
        self.offset_to_utf16(range.start, content)..self.offset_to_utf16(range.end, content)
    }

    fn offset_from_utf16(&self, offset_utf16: usize, content: &str) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in content.chars() {
            if utf16_count >= offset_utf16 {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset.min(content.len())
    }

    fn offset_to_utf16(&self, offset_utf8: usize, content: &str) -> usize {
        content[..offset_utf8.min(content.len())]
            .chars()
            .map(|ch| ch.len_utf16())
            .sum()
    }

    fn previous_boundary(&self, offset: usize, content: &str) -> usize {
        content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize, content: &str) -> usize {
        content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(content.len())
    }

    fn line_start_for_offset(&self, offset: usize, content: &str) -> usize {
        let text_before_offset = &content[..offset];
        text_before_offset
            .rfind('\n')
            .map(|pos| pos + 1)
            .unwrap_or(0)
    }

    fn line_end_for_offset(&self, offset: usize, content: &str) -> usize {
        let text_after_offset = &content[offset..];
        text_after_offset
            .find('\n')
            .map(|pos| offset + pos)
            .unwrap_or(content.len())
    }

    fn offset_for_line_up(&self, offset: usize) -> Option<usize> {
        let line_start = self.line_start_for_offset(offset);
        if line_start == 0 {
            return None; // Already at first line
        }

        let column = offset - line_start;
        let prev_line_end = line_start - 1; // -1 to skip the \n
        let prev_line_start = self.line_start_for_offset(prev_line_end);
        let prev_line_length = prev_line_end - prev_line_start;

        Some(prev_line_start + column.min(prev_line_length))
    }

    fn offset_for_line_down(&self, offset: usize) -> Option<usize> {
        let line_start = self.line_start_for_offset(offset);
        let line_end = self.line_end_for_offset(offset);

        if line_end == self.content.len() {
            return None; // Already at last line
        }

        let column = offset - line_start;
        let next_line_start = line_end + 1; // +1 to skip the \n
        let next_line_end = self.line_end_for_offset(next_line_start);
        let next_line_length = next_line_end - next_line_start;

        Some(next_line_start + column.min(next_line_length))
    }
}

// 实现 EntityInputHandler 来处理系统级输入
impl EntityInputHandler for BodyInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let content = match &self.current_type {
            BodyType::Json => &self.json_content,
            BodyType::Raw => &self.raw_content,
            BodyType::FormData => return None, // FormData 不支持文本输入处理
        };
        
        let range = self.range_from_utf16(&range_utf16, content);
        actual_range.replace(self.range_to_utf16(&range, content));
        Some(content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let (selected_range, selection_reversed) = match &self.current_type {
            BodyType::Json => (&self.json_selected_range, self.json_selection_reversed),
            BodyType::Raw => (&self.raw_selected_range, self.raw_selection_reversed),
            BodyType::FormData => return None,
        };
        
        let content = match &self.current_type {
            BodyType::Json => &self.json_content,
            BodyType::Raw => &self.raw_content,
            BodyType::FormData => return None,
        };
        
        Some(UTF16Selection {
            range: self.range_to_utf16(selected_range, content),
            reversed: selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        let (marked_range, content) = match &self.current_type {
            BodyType::Json => (&self.json_marked_range, &self.json_content),
            BodyType::Raw => (&self.raw_marked_range, &self.raw_content),
            BodyType::FormData => return None,
        };
        
        marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range, content))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        match &self.current_type {
            BodyType::Json => self.json_marked_range = None,
            BodyType::Raw => self.raw_marked_range = None,
            BodyType::FormData => {},
        }
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range.take();

        // 发送值变化事件
        cx.emit(BodyInputEvent::ValueChanged(self.content.to_string()));
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.marked_range = Some(range.start..range.start + new_text.len());
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());

        cx.emit(BodyInputEvent::ValueChanged(self.content.to_string()));
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);

        // For multi-line text, we need to calculate bounds differently
        // This is a simplified version - for now just return the line bounds
        last_layout.first().map(|first_line| Bounds::from_corners(
                point(
                    bounds.left() + first_line.x_for_index(range.start.min(first_line.len())),
                    bounds.top(),
                ),
                point(
                    bounds.left() + first_line.x_for_index(range.end.min(first_line.len())),
                    bounds.bottom(),
                ),
            ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let line_point = self.last_bounds?.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;

        // 如果内容为空但显示的是placeholder，则点击应该定位到开头
        if self.content.is_empty() {
            return Some(0);
        }

        // For multi-line, find the appropriate line
        if let Some(first_line) = last_layout.first() {
            let utf8_index = first_line.index_for_x(point.x - line_point.x)?;
            Some(self.offset_to_utf16(utf8_index))
        } else {
            Some(0)
        }
    }
}

// 自定义文本元素，用于渲染和处理输入
struct BodyTextElement {
    input: Entity<BodyInput>,
}

struct PrepaintState {
    lines: Option<Vec<ShapedLine>>,
    cursor: Option<PaintQuad>,
    selection: Option<Vec<PaintQuad>>,
}

impl IntoElement for BodyTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for BodyTextElement {
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
        style.size.width = relative(1.).into();
        style.size.height = px(200.0).into(); // Fixed height for multi-line input
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
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let style = window.text_style();

        let (display_text, text_color) = if content.is_empty() {
            (input.placeholder.clone(), hsla(0., 0., 0., 0.4))
        } else {
            (content.clone(), style.color)
        };

        let font_size = style.font_size.to_pixels(window.rem_size());

        // Split text into lines for multi-line rendering
        let lines: Vec<String> = if display_text.is_empty() {
            vec![String::new()]
        } else {
            display_text.split('\n').map(|s| s.to_string()).collect()
        };

        let shaped_lines: Vec<ShapedLine> = lines
            .iter()
            .map(|line| {
                let run = TextRun {
                    len: line.len(),
                    font: style.font(),
                    color: text_color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                window
                    .text_system()
                    .shape_line(line.clone().into(), font_size, &[run])
            })
            .collect();

        // Calculate cursor and selection for content (not display text)
        let content_lines: Vec<String> = if content.is_empty() {
            vec![String::new()]
        } else {
            content.split('\n').map(|s| s.to_string()).collect()
        };

        let content_shaped_lines: Vec<ShapedLine> = content_lines
            .iter()
            .map(|line| {
                let run = TextRun {
                    len: line.len(),
                    font: style.font(),
                    color: style.color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                window
                    .text_system()
                    .shape_line(line.clone().into(), font_size, &[run])
            })
            .collect();

        let line_height = window.line_height();

        let (selection, cursor) = if selected_range.is_empty() && !content.is_empty() {
            // Find cursor line and position
            let (line_index, line_offset) = self.line_and_offset_for_index(cursor, &content_lines);
            let cursor_x = if line_index < content_shaped_lines.len() {
                content_shaped_lines[line_index].x_for_index(line_offset)
            } else {
                px(0.0)
            };

            (
                None,
                Some(fill(
                    Bounds::new(
                        point(
                            bounds.left() + cursor_x,
                            bounds.top() + line_height * line_index as f32,
                        ),
                        size(px(2.), line_height),
                    ),
                    rgb(0x0000_7acc),
                )),
            )
        } else if !selected_range.is_empty() && !content.is_empty() {
            // Create selection quads for each line in the selection
            let selection_quads = self.create_selection_quads(
                selected_range,
                &content_lines,
                &content_shaped_lines,
                bounds,
                line_height,
            );
            (Some(selection_quads), None)
        } else {
            (None, None)
        };

        PrepaintState {
            lines: Some(shaped_lines),
            cursor,
            selection,
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
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );

        if let Some(selection_quads) = prepaint.selection.take() {
            for quad in selection_quads {
                window.paint_quad(quad);
            }
        }

        if let Some(lines) = prepaint.lines.take() {
            let line_height = window.line_height();
            for (i, line) in lines.iter().enumerate() {
                let line_origin = point(bounds.left(), bounds.top() + line_height * i as f32);
                let _ = line.paint(line_origin, line_height, window, cx);
            }
        }

        if focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }

        // Save layout for position calculations
        self.input.update(cx, |input, _cx| {
            let style = window.text_style();
            let font_size = style.font_size.to_pixels(window.rem_size());

            let content_lines: Vec<String> = if input.content.is_empty() {
                vec![String::new()]
            } else {
                input.content.split('\n').map(|s| s.to_string()).collect()
            };

            let content_shaped_lines: Vec<ShapedLine> = content_lines
                .iter()
                .map(|line| {
                    let run = TextRun {
                        len: line.len(),
                        font: style.font(),
                        color: style.color,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    window
                        .text_system()
                        .shape_line(line.clone().into(), font_size, &[run])
                })
                .collect();

            input.last_layout = Some(content_shaped_lines);
            input.last_bounds = Some(bounds);
        });
    }
}

impl BodyTextElement {
    fn line_and_offset_for_index(&self, index: usize, lines: &[String]) -> (usize, usize) {
        let mut current_index = 0;
        for (line_idx, line) in lines.iter().enumerate() {
            if current_index + line.len() >= index {
                return (line_idx, index - current_index);
            }
            current_index += line.len() + 1; // +1 for newline
        }
        (lines.len().saturating_sub(1), 0)
    }

    fn create_selection_quads(
        &self,
        selection_range: Range<usize>,
        lines: &[String],
        shaped_lines: &[ShapedLine],
        bounds: Bounds<Pixels>,
        line_height: Pixels,
    ) -> Vec<PaintQuad> {
        let mut quads = Vec::new();
        let (start_line, start_offset) =
            self.line_and_offset_for_index(selection_range.start, lines);
        let (end_line, end_offset) = self.line_and_offset_for_index(selection_range.end, lines);

        for line_idx in start_line..=end_line {
            if line_idx >= shaped_lines.len() {
                break;
            }

            let line = &shaped_lines[line_idx];
            let line_start = if line_idx == start_line {
                start_offset
            } else {
                0
            };
            let line_end = if line_idx == end_line {
                end_offset
            } else {
                lines[line_idx].len()
            };

            if line_start < line_end {
                let start_x = line.x_for_index(line_start);
                let end_x = line.x_for_index(line_end);

                quads.push(fill(
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
                    rgba(0x3366ff33),
                ));
            }
        }

        quads
    }
}

impl EventEmitter<BodyInputEvent> for BodyInput {}

impl Focusable for BodyInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for BodyInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w_full()
            .min_h_32()
            .max_h_96()
            .px_3()
            .py_2()
            .bg(rgb(0x00ff_ffff))
            .border_1()
            .border_color(if self.focus_handle.is_focused(window) {
                rgb(0x0000_7acc)
            } else {
                rgb(0x00cc_cccc)
            })
            .rounded_md()
            .font_family("monospace")
            .text_size(px(13.0))
            .overflow_hidden()
            .cursor(CursorStyle::IBeam)
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::new_line))
            .on_action(cx.listener(Self::tab))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .child(BodyTextElement {
                input: cx.entity().clone(),
            })
    }
}

// 导出KeyBinding设置函数，供主应用使用
pub fn setup_body_input_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("backspace", Backspace, None),
        KeyBinding::new("delete", Delete, None),
        KeyBinding::new("left", Left, None),
        KeyBinding::new("right", Right, None),
        KeyBinding::new("up", Up, None),
        KeyBinding::new("down", Down, None),
        KeyBinding::new("shift-left", SelectLeft, None),
        KeyBinding::new("shift-right", SelectRight, None),
        KeyBinding::new("shift-up", SelectUp, None),
        KeyBinding::new("shift-down", SelectDown, None),
        KeyBinding::new("cmd-a", SelectAll, None),
        KeyBinding::new("cmd-v", Paste, None),
        KeyBinding::new("cmd-c", Copy, None),
        KeyBinding::new("cmd-x", Cut, None),
        KeyBinding::new("home", Home, None),
        KeyBinding::new("end", End, None),
        KeyBinding::new("enter", NewLine, None),
        KeyBinding::new("tab", Tab, None),
    ]
}

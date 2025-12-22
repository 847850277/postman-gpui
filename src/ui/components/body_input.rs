use gpui::{
    actions, div, fill, hsla, point, prelude::FluentBuilder, px, relative, rgb, rgba, size, App,
    Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, InteractiveElement,
    IntoElement, KeyBinding, KeyDownEvent, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PaintQuad, ParentElement, Pixels, Point, Render, ShapedLine, SharedString, Style,
    Styled, TextAlign, TextRun, UTF16Selection, Window,
};
use std::ops::Range;
use unicode_segmentation::*;

actions!(
    body_input,
    [
        Backspace,
        Delete,
        Enter,
        Escape,
        Tab,
        ShiftTab,
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
}

#[derive(Debug, Clone)]
pub struct FormDataEntry {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

pub struct BodyInput {
    focus_handle: FocusHandle,
    current_type: BodyType,
    json_content: String,
    form_data_entries: Vec<FormDataEntry>,
    raw_content: String,
    editing_key_index: Option<usize>,
    editing_value_index: Option<usize>,
    temp_key_value: String,
    temp_value_value: String,
    // JSON input fields (similar to UrlInput)
    json_selected_range: Range<usize>,
    json_selection_reversed: bool,
    json_marked_range: Option<Range<usize>>,
    json_last_layout: Vec<ShapedLine>,
    json_last_bounds: Option<Bounds<Pixels>>,
    json_is_selecting: bool,
}

impl EventEmitter<BodyInputEvent> for BodyInput {}

impl Focusable for BodyInput {
    fn focus_handle(&self, _cx: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

// Implement EntityInputHandler for JSON input
impl EntityInputHandler for BodyInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        if self.current_type != BodyType::Json {
            return None;
        }
        let range = self.json_range_from_utf16(&range_utf16);
        actual_range.replace(self.json_range_to_utf16(&range));
        Some(self.json_content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        if self.current_type != BodyType::Json {
            return None;
        }
        Some(UTF16Selection {
            range: self.json_range_to_utf16(&self.json_selected_range),
            reversed: self.json_selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        if self.current_type != BodyType::Json {
            return None;
        }
        self.json_marked_range
            .as_ref()
            .map(|range| self.json_range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if self.current_type == BodyType::Json {
            self.json_marked_range = None;
        }
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.current_type == BodyType::Json {
            self.json_replace_text_in_range(range_utf16, new_text, window, cx);
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.current_type != BodyType::Json {
            return;
        }

        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.json_range_from_utf16(range_utf16))
            .or(self.json_marked_range.clone())
            .unwrap_or(self.json_selected_range.clone());

        self.json_content = (self.json_content[0..range.start].to_owned()
            + new_text
            + &self.json_content[range.end..])
            .into();
        self.json_marked_range = Some(range.start..range.start + new_text.len());
        self.json_selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.json_range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());

        cx.emit(BodyInputEvent::ValueChanged(self.json_content.clone()));
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if self.current_type != BodyType::Json || self.json_last_layout.is_empty() {
            return None;
        }
        let _range = self.json_range_from_utf16(&range_utf16);

        // For multi-line, approximate bounds
        let line_height = bounds.size.height / self.json_last_layout.len() as f32;
        Some(Bounds::new(
            point(bounds.left(), bounds.top()),
            size(px(100.0), line_height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        if self.current_type != BodyType::Json {
            return None;
        }
        if self.json_content.is_empty() {
            return Some(0);
        }
        let utf8_index = self.json_index_for_mouse_position(point);
        Some(self.json_offset_to_utf16(utf8_index))
    }
}

impl BodyInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            current_type: BodyType::Json,
            json_content: String::new(),
            form_data_entries: vec![FormDataEntry {
                key: String::new(),
                value: String::new(),
                enabled: true,
            }],
            raw_content: String::new(),
            editing_key_index: None,
            editing_value_index: None,
            temp_key_value: String::new(),
            temp_value_value: String::new(),
            json_selected_range: 0..0,
            json_selection_reversed: false,
            json_marked_range: None,
            json_last_layout: Vec::new(),
            json_last_bounds: None,
            json_is_selecting: false,
        }
    }

    pub fn with_placeholder(self, _placeholder: &str) -> Self {
        self
    }

    pub fn get_current_type(&self) -> &BodyType {
        &self.current_type
    }

    pub fn get_json_content(&self) -> &str {
        &self.json_content
    }

    pub fn get_form_data_entries(&self) -> &[FormDataEntry] {
        &self.form_data_entries
    }

    pub fn is_empty(&self) -> bool {
        match &self.current_type {
            BodyType::Json => self.json_content.is_empty(),
            BodyType::Raw => self.raw_content.is_empty(),
            BodyType::FormData => self
                .form_data_entries
                .iter()
                .all(|entry| entry.key.is_empty() && entry.value.is_empty()),
        }
    }

    pub fn set_type(&mut self, body_type: BodyType, cx: &mut Context<Self>) {
        if self.current_type != body_type {
            self.current_type = body_type;
            let content = match &self.current_type {
                BodyType::Json => self.json_content.clone(),
                BodyType::Raw => self.raw_content.clone(),
                BodyType::FormData => self.get_form_data_as_string(),
            };
            cx.emit(BodyInputEvent::ValueChanged(content));
            cx.notify();
        }
    }

    pub fn get_content(&self) -> String {
        match &self.current_type {
            BodyType::Json => self.json_content.clone(),
            BodyType::Raw => self.raw_content.clone(),
            BodyType::FormData => self.get_form_data_as_string(),
        }
    }

    pub fn set_content(&mut self, content: impl Into<String>, cx: &mut Context<Self>) {
        let new_content = content.into();

        match &self.current_type {
            BodyType::Json => {
                if self.json_content != new_content {
                    self.json_content.clone_from(&new_content);
                    cx.emit(BodyInputEvent::ValueChanged(new_content));
                    cx.notify();
                }
            }
            BodyType::Raw => {
                if self.raw_content != new_content {
                    self.raw_content.clone_from(&new_content);
                    cx.emit(BodyInputEvent::ValueChanged(new_content));
                    cx.notify();
                }
            }
            BodyType::FormData => {
                // FormData does not support direct content setting
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
                self.form_data_entries.push(FormDataEntry {
                    key: String::new(),
                    value: String::new(),
                    enabled: true,
                });
            }
            cx.notify();
        }
    }

    pub fn toggle_form_data_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(entry) = self.form_data_entries.get_mut(index) {
            entry.enabled = !entry.enabled;
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
                self.json_content.clear();
            }
            BodyType::Raw => {
                self.raw_content.clear();
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

    pub fn start_editing_key(&mut self, index: usize, cx: &mut Context<Self>) {
        // 首先完成任何现有的编辑
        if self.editing_value_index.is_some() {
            self.finish_value_editing_only(cx);
        }

        if let Some(entry) = self.form_data_entries.get(index) {
            self.editing_key_index = Some(index);
            self.editing_value_index = None;
            self.temp_key_value = entry.key.clone();
            cx.notify();
        }
    }

    pub fn start_editing_value(&mut self, index: usize, cx: &mut Context<Self>) {
        // 首先完成任何现有的编辑
        if self.editing_key_index.is_some() {
            self.finish_key_editing_only(cx);
        }

        if let Some(entry) = self.form_data_entries.get(index) {
            self.editing_value_index = Some(index);
            self.editing_key_index = None;
            self.temp_value_value = entry.value.clone();
            cx.notify();
        }
    }

    pub fn finish_editing(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.editing_key_index {
            if let Some(entry) = self.form_data_entries.get_mut(index) {
                entry.key = self.temp_key_value.clone();
                cx.emit(BodyInputEvent::ValueChanged(self.get_form_data_as_string()));
            }
        }
        if let Some(index) = self.editing_value_index {
            if let Some(entry) = self.form_data_entries.get_mut(index) {
                entry.value = self.temp_value_value.clone();
                cx.emit(BodyInputEvent::ValueChanged(self.get_form_data_as_string()));
            }
        }
        self.editing_key_index = None;
        self.editing_value_index = None;
        self.temp_key_value.clear();
        self.temp_value_value.clear();
        cx.notify();
    }

    pub fn finish_key_editing_only(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.editing_key_index {
            if let Some(entry) = self.form_data_entries.get_mut(index) {
                entry.key = self.temp_key_value.clone();
                cx.emit(BodyInputEvent::ValueChanged(self.get_form_data_as_string()));
            }
            self.editing_key_index = None;
            self.temp_key_value.clear();
        }
        cx.notify();
    }

    pub fn finish_value_editing_only(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.editing_value_index {
            if let Some(entry) = self.form_data_entries.get_mut(index) {
                entry.value = self.temp_value_value.clone();
                cx.emit(BodyInputEvent::ValueChanged(self.get_form_data_as_string()));
            }
            self.editing_value_index = None;
            self.temp_value_value.clear();
        }
        cx.notify();
    }

    pub fn cancel_editing(&mut self, cx: &mut Context<Self>) {
        self.editing_key_index = None;
        self.editing_value_index = None;
        self.temp_key_value.clear();
        self.temp_value_value.clear();
        cx.notify();
    }

    pub fn update_temp_key(&mut self, value: String, cx: &mut Context<Self>) {
        self.temp_key_value = value;
        cx.notify();
    }

    pub fn update_temp_value(&mut self, value: String, cx: &mut Context<Self>) {
        self.temp_value_value = value;
        cx.notify();
    }

    // Action handlers for keyboard shortcuts
    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(_index) = self.editing_key_index {
            self.temp_key_value.pop();
            cx.notify();
        } else if let Some(_index) = self.editing_value_index {
            self.temp_value_value.pop();
            cx.notify();
        }
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        // 对于简单的文本输入，delete 和 backspace 行为相同
        self.backspace(&Backspace, window, cx);
    }

    fn enter(&mut self, _: &Enter, _: &mut Window, cx: &mut Context<Self>) {
        if self.editing_key_index.is_some() || self.editing_value_index.is_some() {
            self.finish_editing(cx);
        }
    }

    fn escape(&mut self, _: &Escape, _: &mut Window, cx: &mut Context<Self>) {
        if self.editing_key_index.is_some() || self.editing_value_index.is_some() {
            self.cancel_editing(cx);
        }
    }

    fn tab(&mut self, _: &Tab, _: &mut Window, cx: &mut Context<Self>) {
        // Tab 键在 FormData 条目之间导航
        if let Some(index) = self.editing_key_index {
            // 从 key 切换到 value - start_editing_value 会自动完成 key 编辑
            self.start_editing_value(index, cx);
        } else if let Some(index) = self.editing_value_index {
            // 从 value 切换到下一行的 key，或者添加新行
            if index + 1 < self.form_data_entries.len() {
                self.start_editing_key(index + 1, cx);
            } else {
                self.add_form_data_entry(cx);
                self.start_editing_key(self.form_data_entries.len() - 1, cx);
            }
        }
    }

    fn shift_tab(&mut self, _: &ShiftTab, _: &mut Window, cx: &mut Context<Self>) {
        // Shift+Tab 键反向导航
        if let Some(index) = self.editing_value_index {
            // 从 value 切换到 key - start_editing_key 会自动完成 value 编辑
            self.start_editing_key(index, cx);
        } else if let Some(index) = self.editing_key_index {
            // 从 key 切换到上一行的 value
            if index > 0 {
                self.start_editing_value(index - 1, cx);
            }
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        // 只在编辑模式下处理字符输入
        if self.editing_key_index.is_none() && self.editing_value_index.is_none() {
            return;
        }

        // 处理普通字符输入
        if let Some(key_char) = &event.keystroke.key_char {
            // 过滤掉特殊键和控制字符
            if key_char.len() == 1 && !key_char.chars().any(|c| c.is_control()) {
                if self.editing_key_index.is_some() {
                    self.temp_key_value.push_str(key_char);
                    cx.notify();
                } else if self.editing_value_index.is_some() {
                    self.temp_value_value.push_str(key_char);
                    cx.notify();
                }
            }
        }
    }

    // JSON input action handlers
    fn json_left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.current_type != BodyType::Json {
            return;
        }
        if self.json_selected_range.is_empty() {
            self.json_move_to(self.json_previous_boundary(self.json_cursor_offset()), cx);
        } else {
            self.json_move_to(self.json_selected_range.start, cx);
        }
    }

    fn json_right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.current_type != BodyType::Json {
            return;
        }
        if self.json_selected_range.is_empty() {
            self.json_move_to(self.json_next_boundary(self.json_selected_range.end), cx);
        } else {
            self.json_move_to(self.json_selected_range.end, cx);
        }
    }

    fn json_up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        if self.current_type != BodyType::Json {
            return;
        }
        let new_offset = self.json_offset_for_line_up(self.json_cursor_offset());
        self.json_move_to(new_offset, cx);
    }

    fn json_down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        if self.current_type != BodyType::Json {
            return;
        }
        let new_offset = self.json_offset_for_line_down(self.json_cursor_offset());
        self.json_move_to(new_offset, cx);
    }

    fn json_select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        if self.current_type != BodyType::Json {
            return;
        }
        self.json_select_to(self.json_previous_boundary(self.json_cursor_offset()), cx);
    }

    fn json_select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        if self.current_type != BodyType::Json {
            return;
        }
        self.json_select_to(self.json_next_boundary(self.json_cursor_offset()), cx);
    }

    fn json_select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        if self.current_type != BodyType::Json {
            return;
        }
        let new_offset = self.json_offset_for_line_up(self.json_cursor_offset());
        self.json_select_to(new_offset, cx);
    }

    fn json_select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        if self.current_type != BodyType::Json {
            return;
        }
        let new_offset = self.json_offset_for_line_down(self.json_cursor_offset());
        self.json_select_to(new_offset, cx);
    }

    fn json_select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        if self.current_type != BodyType::Json {
            return;
        }
        self.json_move_to(0, cx);
        self.json_select_to(self.json_content.len(), cx);
    }

    fn json_home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        if self.current_type != BodyType::Json {
            return;
        }
        let line_start = self.json_line_start(self.json_cursor_offset());
        self.json_move_to(line_start, cx);
    }

    fn json_end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        if self.current_type != BodyType::Json {
            return;
        }
        let line_end = self.json_line_end(self.json_cursor_offset());
        self.json_move_to(line_end, cx);
    }

    fn json_backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.current_type != BodyType::Json {
            return;
        }
        if self.json_selected_range.is_empty() {
            self.json_select_to(self.json_previous_boundary(self.json_cursor_offset()), cx);
        }
        self.json_replace_text_in_range(None, "", window, cx);
    }

    fn json_delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.current_type != BodyType::Json {
            return;
        }
        if self.json_selected_range.is_empty() {
            self.json_select_to(self.json_next_boundary(self.json_cursor_offset()), cx);
        }
        self.json_replace_text_in_range(None, "", window, cx);
    }

    fn json_enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        if self.current_type != BodyType::Json {
            return;
        }
        self.json_replace_text_in_range(None, "\n", window, cx);
    }

    fn json_paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if self.current_type != BodyType::Json {
            return;
        }
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.json_replace_text_in_range(None, &text, window, cx);
        }
    }

    fn json_copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if self.current_type != BodyType::Json {
            return;
        }
        if !self.json_selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.json_content[self.json_selected_range.clone()].to_string(),
            ));
        }
    }

    fn json_cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if self.current_type != BodyType::Json {
            return;
        }
        if !self.json_selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.json_content[self.json_selected_range.clone()].to_string(),
            ));
            self.json_replace_text_in_range(None, "", window, cx);
        }
    }

    // JSON input helper methods
    fn json_move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.json_selected_range = offset..offset;
        cx.notify();
    }

    fn json_cursor_offset(&self) -> usize {
        if self.json_selection_reversed {
            self.json_selected_range.start
        } else {
            self.json_selected_range.end
        }
    }

    fn json_select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.json_selection_reversed {
            self.json_selected_range.start = offset;
        } else {
            self.json_selected_range.end = offset;
        }

        if self.json_selected_range.end < self.json_selected_range.start {
            self.json_selection_reversed = !self.json_selection_reversed;
            self.json_selected_range = self.json_selected_range.end..self.json_selected_range.start;
        }
        cx.notify();
    }

    fn json_previous_boundary(&self, offset: usize) -> usize {
        self.json_content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn json_next_boundary(&self, offset: usize) -> usize {
        self.json_content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.json_content.len())
    }

    fn json_line_start(&self, offset: usize) -> usize {
        self.json_content[..offset]
            .rfind('\n')
            .map(|pos| pos + 1)
            .unwrap_or(0)
    }

    fn json_line_end(&self, offset: usize) -> usize {
        self.json_content[offset..]
            .find('\n')
            .map(|pos| offset + pos)
            .unwrap_or(self.json_content.len())
    }

    fn json_offset_for_line_up(&self, offset: usize) -> usize {
        let current_line_start = self.json_line_start(offset);
        if current_line_start == 0 {
            return 0; // Already at first line
        }
        let prev_line_end = current_line_start - 1;
        let prev_line_start = self.json_line_start(prev_line_end);
        let column = offset - current_line_start;
        let prev_line_len = prev_line_end - prev_line_start;
        prev_line_start + column.min(prev_line_len)
    }

    fn json_offset_for_line_down(&self, offset: usize) -> usize {
        let current_line_start = self.json_line_start(offset);
        let current_line_end = self.json_line_end(offset);
        if current_line_end >= self.json_content.len() {
            return self.json_content.len(); // Already at last line
        }
        let next_line_start = current_line_end + 1;
        let next_line_end = self.json_line_end(next_line_start);
        let column = offset - current_line_start;
        let next_line_len = next_line_end - next_line_start;
        next_line_start + column.min(next_line_len)
    }

    fn json_offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.json_content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    fn json_offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.json_content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn json_range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.json_offset_to_utf16(range.start)..self.json_offset_to_utf16(range.end)
    }

    fn json_range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.json_offset_from_utf16(range_utf16.start)..self.json_offset_from_utf16(range_utf16.end)
    }

    fn json_replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.json_range_from_utf16(range_utf16))
            .or(self.json_marked_range.clone())
            .unwrap_or(self.json_selected_range.clone());

        self.json_content = (self.json_content[0..range.start].to_owned()
            + new_text
            + &self.json_content[range.end..])
            .into();
        self.json_selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.json_marked_range.take();

        cx.emit(BodyInputEvent::ValueChanged(self.json_content.clone()));
        cx.notify();
    }

    fn json_index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.json_content.is_empty() {
            return 0;
        }

        let Some(bounds) = self.json_last_bounds.as_ref() else {
            return 0;
        };

        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.json_content.len();
        }

        // Find which line the mouse is on
        let line_height = if !self.json_last_layout.is_empty() {
            bounds.size.height / self.json_last_layout.len() as f32
        } else {
            return 0;
        };

        let line_index = ((position.y - bounds.top()) / line_height).floor() as usize;
        let line_index = line_index.min(self.json_last_layout.len().saturating_sub(1));

        let line = &self.json_last_layout[line_index];
        let x_in_line = position.x - bounds.left();
        let offset_in_line = line.closest_index_for_x(x_in_line);

        // Calculate the absolute offset
        let mut absolute_offset = 0;
        for (i, layout_line) in self.json_last_layout.iter().enumerate() {
            if i < line_index {
                absolute_offset += layout_line.text.len() + 1; // +1 for newline
            } else {
                break;
            }
        }
        absolute_offset + offset_in_line
    }

    fn json_on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.current_type != BodyType::Json {
            return;
        }
        self.json_is_selecting = true;

        if event.modifiers.shift {
            self.json_select_to(self.json_index_for_mouse_position(event.position), cx);
        } else {
            self.json_move_to(self.json_index_for_mouse_position(event.position), cx);
        }
    }

    fn json_on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        if self.current_type != BodyType::Json {
            return;
        }
        self.json_is_selecting = false;
    }

    fn json_on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.current_type != BodyType::Json {
            return;
        }
        if self.json_is_selecting {
            self.json_select_to(self.json_index_for_mouse_position(event.position), cx);
        }
    }
}

// Custom JsonTextElement for rendering JSON input with cursor and selection
struct JsonTextElement {
    input: Entity<BodyInput>,
}

struct JsonPrepaintState {
    lines: Vec<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Vec<PaintQuad>,
}

impl IntoElement for JsonTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for JsonTextElement {
    type RequestLayoutState = ();
    type PrepaintState = JsonPrepaintState;

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
        let input = self.input.read(cx);
        let content = &input.json_content;

        let mut style = Style::default();
        style.size.width = relative(1.).into();

        // Calculate height based on number of lines
        let line_count = if content.is_empty() {
            1
        } else {
            content.lines().count().max(1)
        };
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
        let input = self.input.read(cx);
        let content = input.json_content.clone(); // Clone to own the data
        let selected_range = input.json_selected_range.clone();
        let cursor = input.json_cursor_offset();
        let style = window.text_style();

        let text_color = if content.is_empty() {
            hsla(0., 0., 0., 0.4)
        } else {
            style.color
        };

        // Split content into lines for multi-line rendering
        let lines_text: Vec<String> = if content.is_empty() {
            vec!["Enter JSON body here...".to_string()]
        } else {
            content.lines().map(|s| s.to_string()).collect()
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let mut shaped_lines = Vec::new();

        for line_text in lines_text.iter() {
            let line_str: SharedString = line_text.clone().into();
            let run = TextRun {
                len: line_str.len(),
                font: style.font(),
                color: text_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped_line = window
                .text_system()
                .shape_line(line_str, font_size, &[run], None);
            shaped_lines.push(shaped_line);
        }

        // Calculate cursor and selection
        let line_height = window.line_height();
        let (selection, cursor_quad) = if selected_range.is_empty() && !content.is_empty() {
            // Find which line the cursor is on
            let (line_idx, offset_in_line) = Self::find_line_for_offset(&content, cursor);
            let cursor_x = if line_idx < shaped_lines.len() {
                shaped_lines[line_idx].x_for_index(offset_in_line)
            } else {
                px(0.0)
            };
            let cursor_y = line_height * line_idx as f32;

            (
                vec![],
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_x, bounds.top() + cursor_y),
                        size(px(2.), line_height),
                    ),
                    rgb(0x0000_7acc),
                )),
            )
        } else if !selected_range.is_empty() && !content.is_empty() {
            // Calculate selection rectangles for multi-line selection
            let selection_quads = Self::calculate_selection_quads(
                &content,
                &shaped_lines,
                &selected_range,
                bounds,
                line_height,
            );
            (selection_quads, None)
        } else {
            (vec![], None)
        };

        JsonPrepaintState {
            lines: shaped_lines,
            cursor: cursor_quad,
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

        // Register input handler
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );

        // Paint selection
        for selection_quad in &prepaint.selection {
            window.paint_quad(selection_quad.clone());
        }

        // Paint text lines
        let line_height = window.line_height();
        for (i, line) in prepaint.lines.iter().enumerate() {
            let y_offset = line_height * i as f32;
            let _ = line.paint(
                point(bounds.left(), bounds.top() + y_offset),
                line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            );
        }

        // Paint cursor if focused
        if focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }

        // Save layout for mouse interaction
        self.input.update(cx, |input, _cx| {
            input.json_last_layout = prepaint.lines.clone();
            input.json_last_bounds = Some(bounds);
        });
    }
}

impl JsonTextElement {
    fn find_line_for_offset(content: &str, offset: usize) -> (usize, usize) {
        let mut current_offset = 0;
        for (line_idx, line) in content.lines().enumerate() {
            let line_len = line.len();
            if current_offset + line_len >= offset {
                return (line_idx, offset - current_offset);
            }
            current_offset += line_len + 1; // +1 for newline
        }
        // If offset is at the end, return last line
        let line_count = content.lines().count();
        (
            line_count.saturating_sub(1),
            content.lines().last().map(|l| l.len()).unwrap_or(0),
        )
    }

    fn calculate_selection_quads(
        content: &str,
        shaped_lines: &[ShapedLine],
        selected_range: &Range<usize>,
        bounds: Bounds<Pixels>,
        line_height: Pixels,
    ) -> Vec<PaintQuad> {
        let mut quads = Vec::new();
        let (start_line, start_offset) = Self::find_line_for_offset(content, selected_range.start);
        let (end_line, end_offset) = Self::find_line_for_offset(content, selected_range.end);

        if start_line == end_line {
            // Single line selection
            if start_line < shaped_lines.len() {
                let line = &shaped_lines[start_line];
                let start_x = line.x_for_index(start_offset);
                let end_x = line.x_for_index(end_offset);
                let y = line_height * start_line as f32;
                quads.push(fill(
                    Bounds::from_corners(
                        point(bounds.left() + start_x, bounds.top() + y),
                        point(bounds.left() + end_x, bounds.top() + y + line_height),
                    ),
                    rgba(0x3366_ff33),
                ));
            }
        } else {
            // Multi-line selection
            for line_idx in start_line..=end_line {
                if line_idx >= shaped_lines.len() {
                    break;
                }
                let line = &shaped_lines[line_idx];
                let y = line_height * line_idx as f32;

                if line_idx == start_line {
                    // First line: from start_offset to end of line
                    let start_x = line.x_for_index(start_offset);
                    let end_x = line.x_for_index(line.text.len());
                    quads.push(fill(
                        Bounds::from_corners(
                            point(bounds.left() + start_x, bounds.top() + y),
                            point(bounds.left() + end_x, bounds.top() + y + line_height),
                        ),
                        rgba(0x3366_ff33),
                    ));
                } else if line_idx == end_line {
                    // Last line: from start of line to end_offset
                    let end_x = line.x_for_index(end_offset);
                    quads.push(fill(
                        Bounds::from_corners(
                            point(bounds.left(), bounds.top() + y),
                            point(bounds.left() + end_x, bounds.top() + y + line_height),
                        ),
                        rgba(0x3366_ff33),
                    ));
                } else {
                    // Middle lines: entire line
                    let end_x = line.x_for_index(line.text.len());
                    quads.push(fill(
                        Bounds::from_corners(
                            point(bounds.left(), bounds.top() + y),
                            point(bounds.left() + end_x, bounds.top() + y + line_height),
                        ),
                        rgba(0x3366_ff33),
                    ));
                }
            }
        }

        quads
    }
}

impl Render for BodyInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current_type = self.current_type.clone();
        let raw_content = self.raw_content.clone();
        let form_data_entries = self.form_data_entries.clone();

        div()
            .flex()
            .flex_col()
            .gap_3()
            .w_full()
            .h_full()
            // Tab headers
            .child(
                div()
                    .flex()
                    .border_b_1()
                    .border_color(rgb(0x00dd_dddd))
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .cursor_pointer()
                            .when(current_type == BodyType::Json, |div| {
                                div.bg(rgb(0x0000_7acc)).text_color(rgb(0x00ff_ffff))
                            })
                            .when(current_type != BodyType::Json, |div| {
                                div.bg(rgb(0x00f8_f9fa))
                                    .text_color(rgb(0x006c_757d))
                                    .hover(|style| style.bg(rgb(0x00e9_ecef)))
                            })
                            .child("JSON")
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.set_type(BodyType::Json, cx);
                                }),
                            ),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .cursor_pointer()
                            .when(current_type == BodyType::FormData, |div| {
                                div.bg(rgb(0x0000_7acc)).text_color(rgb(0x00ff_ffff))
                            })
                            .when(current_type != BodyType::FormData, |div| {
                                div.bg(rgb(0x00f8_f9fa))
                                    .text_color(rgb(0x006c_757d))
                                    .hover(|style| style.bg(rgb(0x00e9_ecef)))
                            })
                            .child("Form Data")
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.set_type(BodyType::FormData, cx);
                                }),
                            ),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .cursor_pointer()
                            .when(current_type == BodyType::Raw, |div| {
                                div.bg(rgb(0x0000_7acc)).text_color(rgb(0x00ff_ffff))
                            })
                            .when(current_type != BodyType::Raw, |div| {
                                div.bg(rgb(0x00f8_f9fa))
                                    .text_color(rgb(0x006c_757d))
                                    .hover(|style| style.bg(rgb(0x00e9_ecef)))
                            })
                            .child("Raw")
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.set_type(BodyType::Raw, cx);
                                }),
                            ),
                    ),
            )
            // Content area
            .child(match current_type {
                BodyType::Json => div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .w_full()
                            .min_h_16()
                            .px_3()
                            .py_2()
                            .bg(rgb(0x00ff_ffff))
                            .border_1()
                            .border_color(
                                if self.focus_handle.is_focused(_window)
                                    && self.current_type == BodyType::Json
                                {
                                    rgb(0x0000_7acc)
                                } else {
                                    rgb(0x00cc_cccc)
                                },
                            )
                            .rounded_md()
                            .cursor(CursorStyle::IBeam)
                            .track_focus(&self.focus_handle(cx))
                            .on_action(cx.listener(Self::json_backspace))
                            .on_action(cx.listener(Self::json_delete))
                            .on_action(cx.listener(Self::json_left))
                            .on_action(cx.listener(Self::json_right))
                            .on_action(cx.listener(Self::json_up))
                            .on_action(cx.listener(Self::json_down))
                            .on_action(cx.listener(Self::json_select_left))
                            .on_action(cx.listener(Self::json_select_right))
                            .on_action(cx.listener(Self::json_select_up))
                            .on_action(cx.listener(Self::json_select_down))
                            .on_action(cx.listener(Self::json_select_all))
                            .on_action(cx.listener(Self::json_home))
                            .on_action(cx.listener(Self::json_end))
                            .on_action(cx.listener(Self::json_paste))
                            .on_action(cx.listener(Self::json_cut))
                            .on_action(cx.listener(Self::json_copy))
                            .on_action(cx.listener(Self::json_enter))
                            .on_mouse_down(MouseButton::Left, cx.listener(Self::json_on_mouse_down))
                            .on_mouse_up(MouseButton::Left, cx.listener(Self::json_on_mouse_up))
                            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::json_on_mouse_up))
                            .on_mouse_move(cx.listener(Self::json_on_mouse_move))
                            .child(JsonTextElement {
                                input: cx.entity().clone(),
                            }),
                    )
                    .into_any_element(),
                BodyType::FormData => div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .track_focus(&self.focus_handle(cx))
                    .on_action(cx.listener(Self::backspace))
                    .on_action(cx.listener(Self::delete))
                    .on_action(cx.listener(Self::enter))
                    .on_action(cx.listener(Self::escape))
                    .on_action(cx.listener(Self::tab))
                    .on_action(cx.listener(Self::shift_tab))
                    .on_key_down(cx.listener(Self::on_key_down))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .p_2()
                            .bg(rgb(0x00f8_f9fa))
                            .border_1()
                            .border_color(rgb(0x00de_e2e6))
                            .child(
                                div()
                                    .w_4()
                                    .text_size(px(12.0))
                                    .text_color(rgb(0x006c_757d))
                                    .child("✓"),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(12.0))
                                    .text_color(rgb(0x006c_757d))
                                    .child("Key"),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(12.0))
                                    .text_color(rgb(0x006c_757d))
                                    .child("Value"),
                            )
                            .child(
                                div()
                                    .w_16()
                                    .text_size(px(12.0))
                                    .text_color(rgb(0x006c_757d))
                                    .child("Action"),
                            ),
                    )
                    .child(div().flex().flex_col().gap_2().children(
                        form_data_entries.iter().enumerate().map(|(index, entry)| {
                            let entry_key = entry.key.clone();
                            let entry_value = entry.value.clone();
                            let entry_enabled = entry.enabled;

                            div()
                                .flex()
                                .gap_2()
                                .items_center()
                                .child(
                                    // Checkbox
                                    div()
                                        .w_4()
                                        .h_4()
                                        .border_1()
                                        .border_color(rgb(0x00cc_cccc))
                                        .rounded_sm()
                                        .cursor_pointer()
                                        .when(entry_enabled, |style| {
                                            style.bg(rgb(0x0000_7acc)).child(
                                                div().w_2().h_2().bg(rgb(0x00ff_ffff)).m_auto(),
                                            )
                                        })
                                        .on_mouse_up(
                                            gpui::MouseButton::Left,
                                            cx.listener(move |this, _event, _window, cx| {
                                                this.toggle_form_data_entry(index, cx);
                                            }),
                                        ),
                                )
                                .child(
                                    // Key input - 可点击编辑
                                    div()
                                        .flex_1()
                                        .px_3()
                                        .py_2()
                                        .bg(rgb(0x00ff_ffff))
                                        .border_1()
                                        .border_color(if self.editing_key_index == Some(index) {
                                            rgb(0x0000_7acc)
                                        } else {
                                            rgb(0x00cc_cccc)
                                        })
                                        .rounded_md()
                                        .text_size(px(14.0))
                                        .cursor(CursorStyle::IBeam)
                                        .when(self.editing_key_index == Some(index), |div| {
                                            div.child(self.temp_key_value.clone())
                                        })
                                        .when(self.editing_key_index != Some(index), |div| {
                                            div.when(entry_key.is_empty(), |div| {
                                                div.text_color(rgb(0x006c_757d))
                                                    .child("Enter key...")
                                            })
                                            .when(!entry_key.is_empty(), |div| {
                                                div.text_color(rgb(0x0021_2529))
                                                    .child(entry_key.clone())
                                            })
                                            .on_mouse_up(
                                                gpui::MouseButton::Left,
                                                cx.listener(move |this, _event, _window, cx| {
                                                    this.start_editing_key(index, cx);
                                                }),
                                            )
                                        }),
                                )
                                .child(
                                    // Value input - 可点击编辑
                                    div()
                                        .flex_1()
                                        .px_3()
                                        .py_2()
                                        .bg(rgb(0x00ff_ffff))
                                        .border_1()
                                        .border_color(if self.editing_value_index == Some(index) {
                                            rgb(0x0000_7acc)
                                        } else {
                                            rgb(0x00cc_cccc)
                                        })
                                        .rounded_md()
                                        .text_size(px(14.0))
                                        .cursor(CursorStyle::IBeam)
                                        .when(self.editing_value_index == Some(index), |div| {
                                            div.child(self.temp_value_value.clone())
                                        })
                                        .when(self.editing_value_index != Some(index), |div| {
                                            div.when(entry_value.is_empty(), |div| {
                                                div.text_color(rgb(0x006c_757d))
                                                    .child("Enter value...")
                                            })
                                            .when(!entry_value.is_empty(), |div| {
                                                div.text_color(rgb(0x0021_2529))
                                                    .child(entry_value.clone())
                                            })
                                            .on_mouse_up(
                                                gpui::MouseButton::Left,
                                                cx.listener(move |this, _event, _window, cx| {
                                                    this.start_editing_value(index, cx);
                                                }),
                                            )
                                        }),
                                )
                                .child(
                                    // Delete button
                                    div()
                                        .px_3()
                                        .py_2()
                                        .bg(rgb(0x00dc_3545))
                                        .text_color(rgb(0x00ff_ffff))
                                        .rounded_md()
                                        .cursor_pointer()
                                        .hover(|style| style.bg(rgb(0x00c8_2333)))
                                        .child("Delete")
                                        .text_size(px(12.0))
                                        .on_mouse_up(
                                            gpui::MouseButton::Left,
                                            cx.listener(move |this, _event, _window, cx| {
                                                this.remove_form_data_entry(index, cx);
                                            }),
                                        ),
                                )
                        }),
                    ))
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .bg(rgb(0x0028_a745))
                            .text_color(rgb(0x00ff_ffff))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0x0021_8838)))
                            .child("Add Row")
                            .text_size(px(14.0))
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.add_form_data_entry(cx);
                                }),
                            ),
                    )
                    .into_any_element(),
                BodyType::Raw => div()
                    .w_full()
                    .h_64()
                    .px_3()
                    .py_2()
                    .bg(rgb(0x00ff_ffff))
                    .border_1()
                    .border_color(rgb(0x00cc_cccc))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_family("monospace")
                            .child(if raw_content.is_empty() {
                                "Enter raw body here...".to_string()
                            } else {
                                raw_content
                            })
                            .when(self.raw_content.is_empty(), |div| {
                                div.text_color(rgb(0x006c_757d))
                            }),
                    )
                    .into_any_element(),
            })
    }
}

pub fn setup_body_input_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("backspace", Backspace, None),
        KeyBinding::new("delete", Delete, None),
        KeyBinding::new("enter", Enter, None),
        KeyBinding::new("escape", Escape, None),
        KeyBinding::new("tab", Tab, None),
        KeyBinding::new("shift-tab", ShiftTab, None),
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
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_line_start() {
        let mut body = BodyInput {
            focus_handle: unsafe { std::mem::zeroed() }, // For testing only
            current_type: BodyType::Json,
            json_content: "Line 1\nLine 2\nLine 3".to_string(),
            form_data_entries: vec![],
            raw_content: String::new(),
            editing_key_index: None,
            editing_value_index: None,
            temp_key_value: String::new(),
            temp_value_value: String::new(),
            json_selected_range: 0..0,
            json_selection_reversed: false,
            json_marked_range: None,
            json_last_layout: Vec::new(),
            json_last_bounds: None,
            json_is_selecting: false,
        };

        // Test at start of first line
        assert_eq!(body.json_line_start(0), 0);
        // Test in middle of first line
        assert_eq!(body.json_line_start(3), 0);
        // Test at start of second line
        assert_eq!(body.json_line_start(7), 7);
        // Test in middle of second line
        assert_eq!(body.json_line_start(10), 7);
    }

    #[test]
    fn test_json_line_end() {
        let mut body = BodyInput {
            focus_handle: unsafe { std::mem::zeroed() },
            current_type: BodyType::Json,
            json_content: "Line 1\nLine 2\nLine 3".to_string(),
            form_data_entries: vec![],
            raw_content: String::new(),
            editing_key_index: None,
            editing_value_index: None,
            temp_key_value: String::new(),
            temp_value_value: String::new(),
            json_selected_range: 0..0,
            json_selection_reversed: false,
            json_marked_range: None,
            json_last_layout: Vec::new(),
            json_last_bounds: None,
            json_is_selecting: false,
        };

        // Test from start of first line
        assert_eq!(body.json_line_end(0), 6);
        // Test from middle of first line
        assert_eq!(body.json_line_end(3), 6);
        // Test from start of second line
        assert_eq!(body.json_line_end(7), 13);
        // Test from last line
        assert_eq!(body.json_line_end(14), 20);
    }

    #[test]
    fn test_json_offset_to_utf16() {
        let mut body = BodyInput {
            focus_handle: unsafe { std::mem::zeroed() },
            current_type: BodyType::Json,
            json_content: "Hello 世界".to_string(), // "世" and "界" are 3 bytes each in UTF-8
            form_data_entries: vec![],
            raw_content: String::new(),
            editing_key_index: None,
            editing_value_index: None,
            temp_key_value: String::new(),
            temp_value_value: String::new(),
            json_selected_range: 0..0,
            json_selection_reversed: false,
            json_marked_range: None,
            json_last_layout: Vec::new(),
            json_last_bounds: None,
            json_is_selecting: false,
        };

        // "Hello " is 6 bytes in UTF-8 and UTF-16
        assert_eq!(body.json_offset_to_utf16(6), 6);
        // "世" is 3 bytes in UTF-8 but 1 code unit in UTF-16
        assert_eq!(body.json_offset_to_utf16(9), 7);
    }

    #[test]
    fn test_find_line_for_offset() {
        let content = "Line 1\nLine 2\nLine 3";

        // Test at start
        assert_eq!(JsonTextElement::find_line_for_offset(content, 0), (0, 0));
        // Test in first line
        assert_eq!(JsonTextElement::find_line_for_offset(content, 3), (0, 3));
        // Test at newline
        assert_eq!(JsonTextElement::find_line_for_offset(content, 6), (0, 6));
        // Test in second line
        assert_eq!(JsonTextElement::find_line_for_offset(content, 10), (1, 3));
        // Test in third line
        assert_eq!(JsonTextElement::find_line_for_offset(content, 17), (2, 3));
    }
}

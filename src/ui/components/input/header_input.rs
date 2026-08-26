use gpui::{
    actions, div, fill, hsla, point, prelude::*, px, relative, rgb, rgba, size, App, Bounds,
    ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, IntoElement,
    KeyBinding, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad,
    ParentElement, Pixels, Point, Render, ShapedLine, SharedString, Style, Styled, TextAlign,
    TextRun, UTF16Selection, Window,
};
use std::ops::Range;
use unicode_segmentation::*;

use crate::ui::components::common::edit_context_menu::{
    edit_context_menu, EditContextAction, EDITABLE_ACTIONS, MASKED_EDITABLE_ACTIONS,
};
use crate::ui::components::input::edit_history::{
    next_word_boundary, previous_word_boundary, EditHistory, TextEditSnapshot,
};
use crate::ui::theme::{FONT_MONO, INFO, LINE, PANEL, TEXT};

const MASK_GLYPH: &str = "•";

// 定义actions - 这些是键盘快捷键对应的动作
actions!(
    header_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        WordLeft,
        WordRight,
        SelectLeft,
        SelectRight,
        SelectWordLeft,
        SelectWordRight,
        SelectAll,
        Home,
        End,
        Paste,
        Cut,
        Copy,
        Undo,
        Redo,
        Submit,
        FocusNext,
        FocusPrevious,
        Dismiss,
    ]
);

#[derive(Debug, Clone)]
pub enum HeaderInputEvent {
    ValueChanged(String),
    SubmitRequested,
}

pub struct HeaderInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    masked: bool,
    embedded: bool,
    font_family: &'static str,
    context_menu_position: Option<Point<Pixels>>,
    edit_history: EditHistory<TextEditSnapshot>,
}

impl HeaderInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            content: "".into(),
            placeholder: "Enter value...".into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
            masked: false,
            embedded: false,
            font_family: FONT_MONO,
            context_menu_position: None,
            edit_history: EditHistory::default(),
        }
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into().into();
        self
    }

    /// Projects contextual placeholder copy without treating it as an input edit.
    pub fn project_placeholder(&mut self, placeholder: impl Into<String>, cx: &mut Context<Self>) {
        let placeholder: SharedString = placeholder.into().into();
        if self.placeholder != placeholder {
            self.placeholder = placeholder;
            cx.notify();
        }
    }

    /// Masks the rendered value while retaining the real value for editing and submission.
    pub fn with_masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        self
    }

    /// Lets a parent component provide the field background, border, radius, and padding.
    pub fn with_embedded_chrome(mut self, embedded: bool) -> Self {
        self.embedded = embedded;
        self
    }

    pub fn with_font_family(mut self, font_family: &'static str) -> Self {
        self.font_family = font_family;
        self
    }

    pub fn set_content(&mut self, content: impl Into<String>, cx: &mut Context<Self>) {
        let new_content: SharedString = content.into().into();
        if self.content != new_content {
            self.record_edit();
            self.replace_projected_content(new_content.clone());
            cx.emit(HeaderInputEvent::ValueChanged(new_content.to_string()));
            cx.notify();
        }
    }

    /// Projects a ViewModel value into the editor buffer without producing a user edit event.
    pub fn project_content(&mut self, content: impl Into<String>, cx: &mut Context<Self>) {
        let new_content: SharedString = content.into().into();
        self.edit_history.clear();
        if self.content != new_content {
            self.replace_projected_content(new_content);
            cx.notify();
        }
    }

    fn replace_projected_content(&mut self, content: SharedString) {
        self.content = content;
        let cursor_position = self.selected_range.start.min(self.content.len());
        self.selected_range = cursor_position..cursor_position;
        self.selection_reversed = false;
        self.marked_range = None;
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        if self.content.is_empty() {
            return;
        }
        self.record_edit();
        self.content = "".into();
        self.selected_range = 0..0;
        self.selection_reversed = false;
        cx.emit(HeaderInputEvent::ValueChanged(String::new()));
        cx.notify();
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

    fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        let offset = if self.selected_range.is_empty() {
            previous_word_boundary(&self.content, self.cursor_offset())
        } else {
            self.selected_range.start
        };
        self.move_to(offset, cx);
    }

    fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        let offset = if self.selected_range.is_empty() {
            next_word_boundary(&self.content, self.cursor_offset())
        } else {
            self.selected_range.end
        };
        self.move_to(offset, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(
            previous_word_boundary(&self.content, self.cursor_offset()),
            cx,
        );
    }

    fn select_word_right(&mut self, _: &SelectWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(next_word_boundary(&self.content, self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        let before = self.snapshot();
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range_internal(None, "", false, false, window, cx);
        if self.snapshot() != before {
            self.edit_history.record(before);
        }
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        let before = self.snapshot();
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range_internal(None, "", false, false, window, cx);
        if self.snapshot() != before {
            self.edit_history.record(before);
        }
    }

    fn submit(&mut self, _: &Submit, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(HeaderInputEvent::SubmitRequested);
    }

    fn focus_next(&mut self, _: &FocusNext, window: &mut Window, cx: &mut Context<Self>) {
        window.focus_next(cx);
    }

    fn focus_previous(&mut self, _: &FocusPrevious, window: &mut Window, cx: &mut Context<Self>) {
        window.focus_prev(cx);
    }

    fn dismiss(&mut self, _: &Dismiss, _: &mut Window, cx: &mut Context<Self>) {
        if self.context_menu_position.take().is_some() {
            cx.notify();
        }
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range_internal(
                None,
                &text.replace('\n', ""),
                true,
                false,
                window,
                cx,
            );
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if self.masked {
            return;
        }
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if self.masked {
            return;
        }
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range_internal(None, "", true, false, window, cx);
        }
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        let current = self.snapshot();
        if let Some(previous) = self.edit_history.undo(current) {
            self.restore_snapshot(previous, cx);
        }
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        let current = self.snapshot();
        if let Some(next) = self.edit_history.redo(current) {
            self.restore_snapshot(next, cx);
        }
    }

    // Mouse event handlers
    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.context_menu_position = None;
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

    fn open_context_menu(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        self.is_selecting = false;
        self.context_menu_position = Some(event.position);
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn handle_context_menu_action(
        &mut self,
        action: EditContextAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            EditContextAction::Undo => self.undo(&Undo, window, cx),
            EditContextAction::Redo => self.redo(&Redo, window, cx),
            EditContextAction::Cut => self.cut(&Cut, window, cx),
            EditContextAction::Copy => self.copy(&Copy, window, cx),
            EditContextAction::Paste => self.paste(&Paste, window, cx),
            EditContextAction::SelectAll => self.select_all(&SelectAll, window, cx),
            EditContextAction::Dismiss => {}
        }
        self.context_menu_position = None;
        cx.notify();
    }

    // Helper methods
    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.edit_history.break_typing_group();
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        cx.notify();
    }

    fn snapshot(&self) -> TextEditSnapshot {
        TextEditSnapshot {
            text: self.content.to_string(),
            selection: self.selected_range.clone(),
            selection_reversed: self.selection_reversed,
        }
    }

    fn record_edit(&mut self) {
        self.edit_history.record(self.snapshot());
    }

    fn record_typing(&mut self) {
        self.edit_history.record_typing(self.snapshot());
    }

    fn restore_snapshot(&mut self, snapshot: TextEditSnapshot, cx: &mut Context<Self>) {
        self.content = snapshot.text.into();
        self.selected_range = snapshot.selection;
        self.selection_reversed = snapshot.selection_reversed;
        self.marked_range = None;
        cx.emit(HeaderInputEvent::ValueChanged(self.content.to_string()));
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

        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };

        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        let display_offset = line.closest_index_for_x(position.x - bounds.left());
        self.content_offset_for_display_offset(display_offset)
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.edit_history.break_typing_group();
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

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }

    fn display_content(&self) -> SharedString {
        if !self.masked {
            return self.content.clone();
        }

        masked_content(&self.content).into()
    }

    fn display_offset_for_content_offset(&self, content_offset: usize) -> usize {
        if !self.masked {
            return content_offset;
        }

        self.content
            .grapheme_indices(true)
            .take_while(|(offset, _)| *offset < content_offset)
            .count()
            * MASK_GLYPH.len()
    }

    fn content_offset_for_display_offset(&self, display_offset: usize) -> usize {
        if !self.masked {
            return display_offset;
        }

        let grapheme_index = display_offset / MASK_GLYPH.len();
        self.content
            .grapheme_indices(true)
            .nth(grapheme_index)
            .map(|(offset, _)| offset)
            .unwrap_or(self.content.len())
    }
}

// 实现 EntityInputHandler 来处理系统级输入
impl EntityInputHandler for HeaderInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_text_in_range_internal(range_utf16, new_text, true, true, window, cx);
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

        let replacement =
            self.content[0..range.start].to_owned() + new_text + &self.content[range.end..];
        if replacement == self.content.as_ref() {
            return;
        }
        self.record_typing();
        self.content = replacement.into();
        self.marked_range = Some(range.start..range.start + new_text.len());
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.start)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());

        cx.emit(HeaderInputEvent::ValueChanged(self.content.to_string()));
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
        let display_range = self.display_offset_for_content_offset(range.start)
            ..self.display_offset_for_content_offset(range.end);
        Some(Bounds::from_corners(
            point(
                bounds.left() + last_layout.x_for_index(display_range.start),
                bounds.top(),
            ),
            point(
                bounds.left() + last_layout.x_for_index(display_range.end),
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

        let display_index = last_layout.index_for_x(point.x - line_point.x)?;
        let utf8_index = self.content_offset_for_display_offset(display_index);
        Some(self.offset_to_utf16(utf8_index))
    }
}

impl HeaderInput {
    fn replace_text_in_range_internal(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        record_history: bool,
        coalesce_typing: bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        let replacement =
            self.content[0..range.start].to_owned() + new_text + &self.content[range.end..];
        if replacement == self.content.as_ref() {
            return;
        }
        if record_history {
            if coalesce_typing {
                self.record_typing();
            } else {
                self.record_edit();
            }
        }
        self.content = replacement.into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.selection_reversed = false;
        self.marked_range.take();
        cx.emit(HeaderInputEvent::ValueChanged(self.content.to_string()));
        cx.notify();
    }
}

// 自定义文本元素，用于渲染和处理输入
struct HeaderTextElement {
    input: Entity<HeaderInput>,
}

struct PrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for HeaderTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for HeaderTextElement {
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
        style.size.height = window.line_height().into();
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
            (input.display_content(), style.color)
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let display_line = window
            .text_system()
            .shape_line(display_text, font_size, &[run], None);

        let cursor_pos = if content.is_empty() {
            px(0.0)
        } else {
            display_line.x_for_index(input.display_offset_for_content_offset(cursor))
        };

        let (selection, cursor) = if selected_range.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_pos, bounds.top()),
                        size(px(2.), bounds.bottom() - bounds.top()),
                    ),
                    rgb(INFO),
                )),
            )
        } else if !content.is_empty() {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left()
                                + display_line.x_for_index(
                                    input.display_offset_for_content_offset(selected_range.start),
                                ),
                            bounds.top(),
                        ),
                        point(
                            bounds.left()
                                + display_line.x_for_index(
                                    input.display_offset_for_content_offset(selected_range.end),
                                ),
                            bounds.bottom(),
                        ),
                    ),
                    rgba(0x3366_ff33),
                )),
                None,
            )
        } else {
            (None, None)
        };

        PrepaintState {
            line: Some(display_line),
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

        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }

        let display_line = prepaint.line.take().unwrap();
        let _ = display_line.paint(
            bounds.origin,
            window.line_height(),
            TextAlign::Left,
            None,
            window,
            cx,
        );

        if focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }

        // Store the same layout that was painted so masked input hit-testing remains aligned.
        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(display_line);
            input.last_bounds = Some(bounds);
        });
    }
}

impl EventEmitter<HeaderInputEvent> for HeaderInput {}

impl Focusable for HeaderInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for HeaderInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let context_menu_position = self.context_menu_position;
        div()
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .when(!self.embedded, |field| {
                field
                    .px_3()
                    .bg(rgb(PANEL))
                    .border_1()
                    .border_color(if self.focus_handle.is_focused(window) {
                        rgb(INFO)
                    } else {
                        rgb(LINE)
                    })
                    .rounded_lg()
            })
            .font_family(self.font_family)
            .text_size(px(12.0))
            .text_color(rgb(TEXT))
            .cursor(CursorStyle::IBeam)
            .track_focus(&self.focus_handle(cx))
            .key_context("HeaderInput")
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::focus_next))
            .on_action(cx.listener(Self::focus_previous))
            .on_action(cx.listener(Self::dismiss))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::open_context_menu))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .child(HeaderTextElement {
                input: cx.entity().clone(),
            })
            .when_some(context_menu_position, |root, position| {
                root.child(edit_context_menu(
                    position,
                    "header-edit-menu",
                    if self.masked {
                        MASKED_EDITABLE_ACTIONS
                    } else {
                        EDITABLE_ACTIONS
                    },
                    Self::handle_context_menu_action,
                    window,
                    cx,
                ))
            })
    }
}

fn masked_content(content: &str) -> String {
    MASK_GLYPH.repeat(content.graphemes(true).count())
}

// 导出KeyBinding设置函数，供主应用使用
pub fn setup_header_input_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("backspace", Backspace, Some("HeaderInput")),
        KeyBinding::new("delete", Delete, Some("HeaderInput")),
        KeyBinding::new("left", Left, Some("HeaderInput")),
        KeyBinding::new("right", Right, Some("HeaderInput")),
        KeyBinding::new("alt-left", WordLeft, Some("HeaderInput")),
        KeyBinding::new("ctrl-left", WordLeft, Some("HeaderInput")),
        KeyBinding::new("alt-right", WordRight, Some("HeaderInput")),
        KeyBinding::new("ctrl-right", WordRight, Some("HeaderInput")),
        KeyBinding::new("shift-left", SelectLeft, Some("HeaderInput")),
        KeyBinding::new("shift-right", SelectRight, Some("HeaderInput")),
        KeyBinding::new("alt-shift-left", SelectWordLeft, Some("HeaderInput")),
        KeyBinding::new("ctrl-shift-left", SelectWordLeft, Some("HeaderInput")),
        KeyBinding::new("alt-shift-right", SelectWordRight, Some("HeaderInput")),
        KeyBinding::new("ctrl-shift-right", SelectWordRight, Some("HeaderInput")),
        KeyBinding::new("cmd-a", SelectAll, Some("HeaderInput")),
        KeyBinding::new("ctrl-a", SelectAll, Some("HeaderInput")),
        KeyBinding::new("cmd-v", Paste, Some("HeaderInput")),
        KeyBinding::new("ctrl-v", Paste, Some("HeaderInput")),
        KeyBinding::new("cmd-c", Copy, Some("HeaderInput")),
        KeyBinding::new("ctrl-c", Copy, Some("HeaderInput")),
        KeyBinding::new("cmd-x", Cut, Some("HeaderInput")),
        KeyBinding::new("ctrl-x", Cut, Some("HeaderInput")),
        KeyBinding::new("cmd-z", Undo, Some("HeaderInput")),
        KeyBinding::new("ctrl-z", Undo, Some("HeaderInput")),
        KeyBinding::new("cmd-shift-z", Redo, Some("HeaderInput")),
        KeyBinding::new("ctrl-shift-z", Redo, Some("HeaderInput")),
        KeyBinding::new("ctrl-y", Redo, Some("HeaderInput")),
        KeyBinding::new("home", Home, Some("HeaderInput")),
        KeyBinding::new("end", End, Some("HeaderInput")),
        KeyBinding::new("cmd-left", Home, Some("HeaderInput")),
        KeyBinding::new("cmd-right", End, Some("HeaderInput")),
        KeyBinding::new("enter", Submit, Some("HeaderInput")),
        KeyBinding::new("tab", FocusNext, Some("HeaderInput && !HistorySearch")),
        KeyBinding::new(
            "shift-tab",
            FocusPrevious,
            Some("HeaderInput && !HistorySearch"),
        ),
        KeyBinding::new("escape", Dismiss, Some("HeaderInput")),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masking_preserves_grapheme_count_without_exposing_text() {
        assert_eq!(masked_content("secret"), "••••••");
        assert_eq!(masked_content("á👩‍💻"), "••");
        assert!(!masked_content("secret").contains("secret"));
    }
}

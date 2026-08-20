use super::{
    Backspace, Copy, Cut, Delete, Down, End, Enter, Home, Left, Paste, Right, SelectAll,
    SelectDown, SelectLeft, SelectRight, SelectUp, Up,
};
use crate::ui::{
    components::common::edit_context_menu::{
        edit_context_menu, EditContextAction, EDITABLE_ACTIONS,
    },
    theme::{CODE_BG, CODE_TEXT, INFO, LINE},
};
use gpui::{
    div, fill, hsla, point, prelude::FluentBuilder, px, relative, rgb, rgba, size, App, Bounds,
    ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, InteractiveElement,
    IntoElement, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad,
    ParentElement, Pixels, Point, Render, ShapedLine, SharedString, Style, Styled, TextAlign,
    TextRun, UTF16Selection, Window,
};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug)]
pub(super) enum TextBodyInputEvent {
    ValueChanged(String),
}

/// Stateful JSON/raw body editor. It owns text editing mechanics only; request body values remain in
/// the workspace ViewModel and arrive through silent projection.
pub(super) struct TextBodyInput {
    focus_handle: FocusHandle,
    json_content: String,
    json_selected_range: Range<usize>,
    json_selection_reversed: bool,
    json_marked_range: Option<Range<usize>>,
    json_last_layout: Vec<ShapedLine>,
    json_last_bounds: Option<Bounds<Pixels>>,
    json_is_selecting: bool,
    context_menu_position: Option<Point<Pixels>>,
}

impl EventEmitter<TextBodyInputEvent> for TextBodyInput {}

impl Focusable for TextBodyInput {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for TextBodyInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
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
        self.json_marked_range
            .as_ref()
            .map(|range| self.json_range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.json_marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.json_replace_text_in_range(range_utf16, new_text, window, cx);
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
            .map(|range_utf16| self.json_range_from_utf16(range_utf16))
            .or(self.json_marked_range.clone())
            .unwrap_or(self.json_selected_range.clone());

        self.json_content = self.json_content[0..range.start].to_owned()
            + new_text
            + &self.json_content[range.end..];
        self.json_marked_range = Some(range.start..range.start + new_text.len());
        self.json_selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.json_range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());

        cx.emit(TextBodyInputEvent::ValueChanged(self.json_content.clone()));
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if self.json_last_layout.is_empty() {
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
        if self.json_content.is_empty() {
            return Some(0);
        }
        let utf8_index = self.json_index_for_mouse_position(point);
        Some(self.json_offset_to_utf16(utf8_index))
    }
}
impl TextBodyInput {
    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            json_content: String::new(),
            json_selected_range: 0..0,
            json_selection_reversed: false,
            json_marked_range: None,
            json_last_layout: Vec::new(),
            json_last_bounds: None,
            json_is_selecting: false,
            context_menu_position: None,
        }
    }

    pub(super) fn content(&self) -> &str {
        &self.json_content
    }

    pub(super) fn set_content(&mut self, content: impl Into<String>, cx: &mut Context<Self>) {
        let content = content.into();
        if self.json_content != content {
            self.json_content = content;
            cx.emit(TextBodyInputEvent::ValueChanged(self.json_content.clone()));
            cx.notify();
        }
    }

    pub(super) fn project_content(&mut self, content: impl Into<String>, cx: &mut Context<Self>) {
        let content = content.into();
        if self.json_content != content {
            self.json_content = content;
            let cursor = clamp_to_char_boundary(&self.json_content, self.json_selected_range.start);
            self.json_selected_range = cursor..cursor;
            self.json_selection_reversed = false;
            self.json_marked_range = None;
            cx.notify();
        }
    }

    pub(super) fn clear(&mut self, cx: &mut Context<Self>) {
        self.json_content.clear();
        self.json_selected_range = 0..0;
        self.json_selection_reversed = false;
        self.json_marked_range = None;
        cx.emit(TextBodyInputEvent::ValueChanged(String::new()));
        cx.notify();
    }

    fn json_left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.json_selected_range.is_empty() {
            self.json_move_to(self.json_previous_boundary(self.json_cursor_offset()), cx);
        } else {
            self.json_move_to(self.json_selected_range.start, cx);
        }
    }

    fn json_right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.json_selected_range.is_empty() {
            self.json_move_to(self.json_next_boundary(self.json_selected_range.end), cx);
        } else {
            self.json_move_to(self.json_selected_range.end, cx);
        }
    }

    fn json_up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        let new_offset = self.json_offset_for_line_up(self.json_cursor_offset());
        self.json_move_to(new_offset, cx);
    }

    fn json_down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        let new_offset = self.json_offset_for_line_down(self.json_cursor_offset());
        self.json_move_to(new_offset, cx);
    }

    fn json_select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.json_select_to(self.json_previous_boundary(self.json_cursor_offset()), cx);
    }

    fn json_select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.json_select_to(self.json_next_boundary(self.json_cursor_offset()), cx);
    }

    fn json_select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        let new_offset = self.json_offset_for_line_up(self.json_cursor_offset());
        self.json_select_to(new_offset, cx);
    }

    fn json_select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        let new_offset = self.json_offset_for_line_down(self.json_cursor_offset());
        self.json_select_to(new_offset, cx);
    }

    fn json_select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.json_move_to(0, cx);
        self.json_select_to(self.json_content.len(), cx);
    }

    fn json_home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        let line_start = self.json_line_start(self.json_cursor_offset());
        self.json_move_to(line_start, cx);
    }

    fn json_end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        let line_end = self.json_line_end(self.json_cursor_offset());
        self.json_move_to(line_end, cx);
    }

    fn json_backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.json_selected_range.is_empty() {
            self.json_select_to(self.json_previous_boundary(self.json_cursor_offset()), cx);
        }
        self.json_replace_text_in_range(None, "", window, cx);
    }

    fn json_delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.json_selected_range.is_empty() {
            self.json_select_to(self.json_next_boundary(self.json_cursor_offset()), cx);
        }
        self.json_replace_text_in_range(None, "", window, cx);
    }

    fn json_enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        self.json_replace_text_in_range(None, "\n", window, cx);
    }

    fn json_paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.json_replace_text_in_range(None, &text, window, cx);
        }
    }

    fn json_copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.json_selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.json_content[self.json_selected_range.clone()].to_string(),
            ));
        }
    }

    fn json_cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
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
        previous_grapheme_boundary(&self.json_content, offset)
    }

    fn json_next_boundary(&self, offset: usize) -> usize {
        next_grapheme_boundary(&self.json_content, offset)
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
        offset_from_utf16(&self.json_content, offset)
    }

    fn json_offset_to_utf16(&self, offset: usize) -> usize {
        offset_to_utf16(&self.json_content, offset)
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

        self.json_content = self.json_content[0..range.start].to_owned()
            + new_text
            + &self.json_content[range.end..];
        self.json_selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.json_marked_range.take();

        cx.emit(TextBodyInputEvent::ValueChanged(self.json_content.clone()));
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
        self.context_menu_position = None;
        self.json_is_selecting = true;

        if event.modifiers.shift {
            self.json_select_to(self.json_index_for_mouse_position(event.position), cx);
        } else {
            self.json_move_to(self.json_index_for_mouse_position(event.position), cx);
        }
    }

    fn json_on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        self.json_is_selecting = false;
    }

    fn json_on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.json_is_selecting {
            self.json_select_to(self.json_index_for_mouse_position(event.position), cx);
        }
    }

    fn open_json_context_menu(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        self.json_is_selecting = false;
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
            EditContextAction::Dismiss => {}
            EditContextAction::Cut => self.json_cut(&Cut, window, cx),
            EditContextAction::Copy => self.json_copy(&Copy, window, cx),
            EditContextAction::Paste => self.json_paste(&Paste, window, cx),
            EditContextAction::SelectAll => self.json_select_all(&SelectAll, window, cx),
        }
        self.context_menu_position = None;
        cx.notify();
    }
}

fn clamp_to_char_boundary(content: &str, offset: usize) -> usize {
    let mut offset = offset.min(content.len());
    while !content.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn previous_grapheme_boundary(content: &str, offset: usize) -> usize {
    content
        .grapheme_indices(true)
        .rev()
        .find_map(|(index, _)| (index < offset).then_some(index))
        .unwrap_or(0)
}

fn next_grapheme_boundary(content: &str, offset: usize) -> usize {
    content
        .grapheme_indices(true)
        .find_map(|(index, _)| (index > offset).then_some(index))
        .unwrap_or(content.len())
}

fn offset_from_utf16(content: &str, offset: usize) -> usize {
    let mut utf8_offset = 0;
    let mut utf16_count = 0;

    for character in content.chars() {
        if utf16_count >= offset {
            break;
        }
        utf16_count += character.len_utf16();
        utf8_offset += character.len_utf8();
    }

    utf8_offset
}

fn offset_to_utf16(content: &str, offset: usize) -> usize {
    let mut utf16_offset = 0;
    let mut utf8_count = 0;

    for character in content.chars() {
        if utf8_count >= offset {
            break;
        }
        utf8_count += character.len_utf8();
        utf16_offset += character.len_utf16();
    }

    utf16_offset
}

struct JsonTextElement {
    input: Entity<TextBodyInput>,
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
                    rgb(INFO),
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

// Custom FormTextElement for rendering FormData key/value with cursor and selection
impl Render for TextBodyInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let context_menu_position = self.context_menu_position;
        let editor = div().flex_1().min_h_0().flex().flex_col().child(
            div()
                .w_full()
                .h_full()
                .min_h_0()
                .px_3()
                .py_2()
                .bg(rgb(CODE_BG))
                .border_1()
                .border_color(if self.focus_handle.is_focused(window) {
                    rgb(INFO)
                } else {
                    rgb(LINE)
                })
                .rounded_lg()
                .font_family("Menlo")
                .text_size(px(13.0))
                .text_color(rgb(CODE_TEXT))
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
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(Self::open_json_context_menu),
                )
                .on_mouse_up(MouseButton::Left, cx.listener(Self::json_on_mouse_up))
                .on_mouse_up_out(MouseButton::Left, cx.listener(Self::json_on_mouse_up))
                .on_mouse_move(cx.listener(Self::json_on_mouse_move))
                .child(JsonTextElement {
                    input: cx.entity().clone(),
                }),
        );
        editor.when_some(context_menu_position, |root, position| {
            root.child(edit_context_menu(
                position,
                "body-edit-menu",
                EDITABLE_ACTIONS,
                Self::handle_context_menu_action,
                window,
                cx,
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        next_grapheme_boundary, offset_from_utf16, offset_to_utf16, previous_grapheme_boundary,
        JsonTextElement,
    };

    #[test]
    fn cursor_boundaries_follow_complete_unicode_graphemes() {
        let content = "a🇨🇳e\u{301}z";
        let flag_start = "a".len();
        let accented_start = "a🇨🇳".len();
        let z_start = "a🇨🇳e\u{301}".len();

        assert_eq!(next_grapheme_boundary(content, 0), flag_start);
        assert_eq!(next_grapheme_boundary(content, flag_start), accented_start);
        assert_eq!(next_grapheme_boundary(content, accented_start), z_start);
        assert_eq!(previous_grapheme_boundary(content, z_start), accented_start);
        assert_eq!(
            previous_grapheme_boundary(content, accented_start),
            flag_start
        );
    }

    #[test]
    fn utf8_and_utf16_offsets_round_trip_unicode_boundaries() {
        let content = "A😀中";
        for (utf8, utf16) in [(0, 0), (1, 1), (5, 3), (8, 4)] {
            assert_eq!(offset_to_utf16(content, utf8), utf16);
            assert_eq!(offset_from_utf16(content, utf16), utf8);
        }
    }

    #[test]
    fn multiline_selection_offsets_resolve_to_line_local_boundaries() {
        let content = "one\n二三\nlast";
        assert_eq!(JsonTextElement::find_line_for_offset(content, 0), (0, 0));
        assert_eq!(JsonTextElement::find_line_for_offset(content, 4), (1, 0));
        assert_eq!(
            JsonTextElement::find_line_for_offset(content, "one\n二三".len()),
            (1, "二三".len())
        );
    }
}

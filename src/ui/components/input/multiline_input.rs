use super::body_input::{
    Backspace, Copy, Cut, Delete, Down, End, Enter, Escape, Home, Left, Paste, Redo, Right,
    SelectAll, SelectDown, SelectLeft, SelectRight, SelectUp, SelectWordLeft, SelectWordRight,
    ShiftTab, Tab, Undo, Up, WordLeft, WordRight,
};
use crate::ui::{
    text_editor::{
        EditOutcome, EditTransaction, TextEditorError, TextEditorPolicy, TextEditorState,
        TextMovement,
    },
    theme::INFO,
};
use gpui::{
    fill, hsla, point, px, relative, rgb, rgba, size, App, Bounds, ClipboardItem, Context, Element,
    ElementId, ElementInputHandler, Entity, EntityInputHandler, FocusHandle, GlobalElementId,
    IntoElement, LayoutId, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    ScrollHandle, ShapedLine, SharedString, Style, TextAlign, TextRun, UTF16Selection, Window,
};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

const NEWLINE_SELECTION_WIDTH: Pixels = px(4.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LineRange {
    start: usize,
    end: usize,
    next_start: usize,
}

impl LineRange {
    fn len(self) -> usize {
        self.end - self.start
    }
}

struct MultilineLayout {
    lines: Vec<ShapedLine>,
    ranges: Vec<LineRange>,
    bounds: Bounds<Pixels>,
    line_height: Pixels,
}

impl MultilineLayout {
    fn matches(&self, text: &str, ranges: &[LineRange]) -> bool {
        self.ranges == ranges
            && self.lines.len() == ranges.len()
            && (text.is_empty()
                || self
                    .lines
                    .iter()
                    .zip(ranges)
                    .all(|(line, range)| line.text.as_ref() == &text[range.start..range.end]))
    }
}

#[derive(Clone, Copy)]
struct PreferredColumn {
    x: Option<Pixels>,
    grapheme: usize,
}

#[derive(Clone, Copy)]
enum VerticalDirection {
    Up,
    Down,
}

/// Shared GPUI-facing state for a multiline editor. Logical text state lives exclusively in
/// `TextEditorState`; this adapter owns only line shaping, visual-column, mouse, and scroll data.
pub(crate) struct MultilineInputState {
    editor: TextEditorState,
    placeholder: SharedString,
    layout: Option<MultilineLayout>,
    scroll_handle: ScrollHandle,
    preferred_column: Option<PreferredColumn>,
    is_selecting: bool,
    context_menu_position: Option<Point<Pixels>>,
    scroll_to_caret_requested: bool,
}

impl MultilineInputState {
    pub(crate) fn new(placeholder: impl Into<SharedString>) -> Self {
        Self {
            editor: TextEditorState::new(String::new(), TextEditorPolicy::multiline()),
            placeholder: placeholder.into(),
            layout: None,
            scroll_handle: ScrollHandle::new(),
            preferred_column: None,
            is_selecting: false,
            context_menu_position: None,
            scroll_to_caret_requested: false,
        }
    }

    pub(crate) fn text(&self) -> &str {
        self.editor.text()
    }

    pub(crate) fn placeholder(&self) -> &SharedString {
        &self.placeholder
    }

    pub(crate) fn scroll_handle(&self) -> &ScrollHandle {
        &self.scroll_handle
    }

    pub(crate) fn context_menu_position(&self) -> Option<Point<Pixels>> {
        self.context_menu_position
    }

    pub(crate) fn dismiss_context_menu(&mut self) -> bool {
        self.context_menu_position.take().is_some()
    }

    /// Programmatic user mutation used by the Body compatibility surface. The previous selection
    /// is retained where possible, while the complete replacement remains one Undo transaction.
    pub(crate) fn set_text(&mut self, text: impl Into<String>) -> bool {
        let text = text.into();
        if self.editor.text() == text {
            return false;
        }
        self.editor.commit_composition();
        let selection = self.editor.selection();
        let full_range = self
            .editor
            .range_from_utf8(0..self.editor.text().len())
            .expect("complete editor text is always a valid range");
        let before = self.editor.text().to_string();
        let _ = self
            .editor
            .replace_range(full_range, &text, EditTransaction::Discrete);
        let anchor = clamped_offset(&self.editor, selection.anchor().utf8());
        let cursor = clamped_offset(&self.editor, selection.cursor().utf8());
        let _ = self.editor.set_selection(anchor, cursor);
        self.after_cursor_or_text_change();
        self.editor.text() != before
    }

    /// Silent model projection. Undo cannot cross this boundary and the previous normalized
    /// selection start remains the collapsed cursor, matching the legacy Body projection.
    pub(crate) fn project_text(&mut self, text: impl Into<String>) -> bool {
        let text = text.into();
        if self.editor.text() == text {
            self.editor.clear_edit_history();
            return false;
        }
        let cursor = self.editor.selected_range().start().utf8();
        let before = self.editor.text().to_string();
        self.editor.project_text(text);
        let cursor = clamped_offset(&self.editor, cursor);
        let _ = self.editor.set_selection(cursor, cursor);
        self.after_cursor_or_text_change();
        self.editor.text() != before
    }

    pub(crate) fn clear(&mut self) -> bool {
        if self.editor.text().is_empty() {
            return false;
        }
        self.editor.commit_composition();
        let full_range = self
            .editor
            .range_from_utf8(0..self.editor.text().len())
            .expect("complete editor text is always a valid range");
        let changed = matches!(
            self.editor
                .replace_range(full_range, "", EditTransaction::Discrete),
            Ok(EditOutcome::Changed)
        );
        if changed {
            self.after_cursor_or_text_change();
        }
        changed
    }

    fn reset_preferred_column(&mut self) {
        self.preferred_column = None;
    }

    fn request_caret_visibility(&mut self) {
        self.scroll_to_caret_requested = true;
    }

    fn after_cursor_or_text_change(&mut self) {
        self.reset_preferred_column();
        self.request_caret_visibility();
    }

    fn move_vertical(
        &mut self,
        direction: VerticalDirection,
        extend_selection: bool,
    ) -> Result<EditOutcome, TextEditorError> {
        let text = self.editor.text();
        let ranges = line_ranges(text);
        let selection = self.editor.selection();
        let cursor = selection.cursor().utf8();
        let current_line = line_index_for_offset(&ranges, cursor);
        let current_range = ranges[current_line];
        let current_local =
            cursor.clamp(current_range.start, current_range.end) - current_range.start;
        let layout = self
            .layout
            .as_ref()
            .filter(|layout| layout.matches(text, &ranges));

        let preferred = self.preferred_column.unwrap_or_else(|| PreferredColumn {
            x: layout.map(|layout| layout.lines[current_line].x_for_index(current_local)),
            grapheme: grapheme_column(&text[current_range.start..current_range.end], current_local),
        });

        let target_line = match direction {
            VerticalDirection::Up if current_line > 0 => Some(current_line - 1),
            VerticalDirection::Down if current_line + 1 < ranges.len() => Some(current_line + 1),
            _ => None,
        };
        let target = if let Some(target_line) = target_line {
            let target_range = ranges[target_line];
            let target_text = &text[target_range.start..target_range.end];
            let local = preferred
                .x
                .and_then(|x| {
                    layout.map(|layout| {
                        floor_char_boundary(
                            target_text,
                            layout.lines[target_line]
                                .closest_index_for_x(x)
                                .min(target_text.len()),
                        )
                    })
                })
                .unwrap_or_else(|| offset_for_grapheme_column(target_text, preferred.grapheme));
            target_range.start + local
        } else {
            match direction {
                VerticalDirection::Up => 0,
                VerticalDirection::Down => text.len(),
            }
        };

        self.preferred_column = Some(preferred);
        let target = self.editor.offset_from_utf8(target)?;
        let outcome = if extend_selection {
            self.editor.set_selection(selection.anchor(), target)?
        } else {
            self.editor.set_selection(target, target)?
        };
        if outcome == EditOutcome::Changed {
            self.request_caret_visibility();
        }
        Ok(outcome)
    }

    fn move_to_line_edge(&mut self, end: bool) -> Result<EditOutcome, TextEditorError> {
        let cursor = self.editor.selection().cursor().utf8();
        let ranges = line_ranges(self.editor.text());
        let range = ranges[line_index_for_offset(&ranges, cursor)];
        let target = if end { range.end } else { range.start };
        let target = self.editor.offset_from_utf8(target)?;
        let outcome = self.editor.set_selection(target, target)?;
        self.after_cursor_or_text_change();
        Ok(outcome)
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.editor.text().is_empty() {
            return 0;
        }
        let Some(layout) = self.layout.as_ref() else {
            return 0;
        };
        let ranges = line_ranges(self.editor.text());
        if !layout.matches(self.editor.text(), &ranges) {
            return self.editor.selection().cursor().utf8();
        }
        if position.y < layout.bounds.top() {
            return 0;
        }
        if position.y > layout.bounds.bottom() {
            return self.editor.text().len();
        }

        let line_index = (((position.y - layout.bounds.top()) / layout.line_height).floor()
            as usize)
            .min(layout.lines.len().saturating_sub(1));
        let range = ranges[line_index];
        let local = if position.x <= layout.bounds.left() {
            0
        } else if position.x >= layout.bounds.right() {
            range.len()
        } else {
            floor_char_boundary(
                &self.editor.text()[range.start..range.end],
                layout.lines[line_index]
                    .closest_index_for_x(position.x - layout.bounds.left())
                    .min(range.len()),
            )
        };
        range.start + local
    }

    fn install_layout(&mut self, layout: MultilineLayout) -> bool {
        self.layout = Some(layout);
        if !self.scroll_to_caret_requested {
            return false;
        }
        self.scroll_to_caret_requested = false;
        self.scroll_caret_into_view()
    }

    fn scroll_caret_into_view(&mut self) -> bool {
        let Some(layout) = self.layout.as_ref() else {
            return false;
        };
        let viewport = self.scroll_handle.bounds();
        if viewport.size.height <= px(0.0) || layout.lines.is_empty() {
            return false;
        }
        let ranges = line_ranges(self.editor.text());
        if !layout.matches(self.editor.text(), &ranges) {
            return false;
        }
        let cursor = self.editor.selection().cursor().utf8();
        let line = line_index_for_offset(&ranges, cursor);
        let caret_top = layout.bounds.top() + layout.line_height * line as f32;
        let caret_bottom = caret_top + layout.line_height;
        let mut next = self.scroll_handle.offset();
        let content_inset = (layout.bounds.top() - next.y - viewport.top()).max(px(0.0));
        let visible_top = viewport.top() + content_inset;
        let visible_bottom = viewport.bottom() - content_inset;
        if caret_top < visible_top {
            next.y += visible_top - caret_top;
        } else if caret_bottom > visible_bottom {
            next.y -= caret_bottom - visible_bottom;
        }

        let maximum = self.scroll_handle.max_offset().y;
        if next.y > px(0.0) {
            next.y = px(0.0);
        }
        if next.y < -maximum {
            next.y = -maximum;
        }
        if next == self.scroll_handle.offset() {
            false
        } else {
            self.scroll_handle.set_offset(next);
            true
        }
    }
}

pub(crate) trait MultilineInputHost: EntityInputHandler + Sized + 'static {
    fn multiline_input(&self) -> &MultilineInputState;
    fn multiline_input_mut(&mut self) -> &mut MultilineInputState;
    fn multiline_focus_handle(&self) -> &FocusHandle;
    fn emit_multiline_changed(&mut self, value: String, cx: &mut Context<Self>);
}

pub(crate) fn backspace<H: MultilineInputHost>(
    host: &mut H,
    _: &Backspace,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    run_edit(host, cx, TextEditorState::delete_backward);
}

pub(crate) fn delete<H: MultilineInputHost>(
    host: &mut H,
    _: &Delete,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    run_edit(host, cx, TextEditorState::delete_forward);
}

pub(crate) fn left<H: MultilineInputHost>(
    host: &mut H,
    _: &Left,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    move_horizontal(host, TextMovement::PreviousGrapheme, false, cx);
}

pub(crate) fn right<H: MultilineInputHost>(
    host: &mut H,
    _: &Right,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    move_horizontal(host, TextMovement::NextGrapheme, false, cx);
}

pub(crate) fn word_left<H: MultilineInputHost>(
    host: &mut H,
    _: &WordLeft,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    move_horizontal(host, TextMovement::PreviousWord, false, cx);
}

pub(crate) fn word_right<H: MultilineInputHost>(
    host: &mut H,
    _: &WordRight,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    move_horizontal(host, TextMovement::NextWord, false, cx);
}

pub(crate) fn up<H: MultilineInputHost>(host: &mut H, _: &Up, _: &mut Window, cx: &mut Context<H>) {
    move_vertical(host, VerticalDirection::Up, false, cx);
}

pub(crate) fn down<H: MultilineInputHost>(
    host: &mut H,
    _: &Down,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    move_vertical(host, VerticalDirection::Down, false, cx);
}

pub(crate) fn select_left<H: MultilineInputHost>(
    host: &mut H,
    _: &SelectLeft,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    move_horizontal(host, TextMovement::PreviousGrapheme, true, cx);
}

pub(crate) fn select_right<H: MultilineInputHost>(
    host: &mut H,
    _: &SelectRight,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    move_horizontal(host, TextMovement::NextGrapheme, true, cx);
}

pub(crate) fn select_word_left<H: MultilineInputHost>(
    host: &mut H,
    _: &SelectWordLeft,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    move_horizontal(host, TextMovement::PreviousWord, true, cx);
}

pub(crate) fn select_word_right<H: MultilineInputHost>(
    host: &mut H,
    _: &SelectWordRight,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    move_horizontal(host, TextMovement::NextWord, true, cx);
}

pub(crate) fn select_up<H: MultilineInputHost>(
    host: &mut H,
    _: &SelectUp,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    move_vertical(host, VerticalDirection::Up, true, cx);
}

pub(crate) fn select_down<H: MultilineInputHost>(
    host: &mut H,
    _: &SelectDown,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    move_vertical(host, VerticalDirection::Down, true, cx);
}

pub(crate) fn select_all<H: MultilineInputHost>(
    host: &mut H,
    _: &SelectAll,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    commit_composition(host);
    let input = host.multiline_input_mut();
    input.reset_preferred_column();
    if matches!(input.editor.select_all(), Ok(EditOutcome::Changed)) {
        input.request_caret_visibility();
        cx.notify();
    }
}

pub(crate) fn home<H: MultilineInputHost>(
    host: &mut H,
    _: &Home,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    commit_composition(host);
    if matches!(
        host.multiline_input_mut().move_to_line_edge(false),
        Ok(EditOutcome::Changed)
    ) {
        cx.notify();
    }
}

pub(crate) fn end<H: MultilineInputHost>(
    host: &mut H,
    _: &End,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    commit_composition(host);
    if matches!(
        host.multiline_input_mut().move_to_line_edge(true),
        Ok(EditOutcome::Changed)
    ) {
        cx.notify();
    }
}

pub(crate) fn enter<H: MultilineInputHost>(
    host: &mut H,
    _: &Enter,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    run_edit(host, cx, |editor| {
        editor.insert_text("\n", EditTransaction::Discrete)
    });
}

pub(crate) fn paste<H: MultilineInputHost>(
    host: &mut H,
    _: &Paste,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
        run_edit(host, cx, |editor| {
            editor.insert_text(&text, EditTransaction::Discrete)
        });
    }
}

pub(crate) fn copy<H: MultilineInputHost>(
    host: &mut H,
    _: &Copy,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    if let Some(text) = host.multiline_input().editor.selected_text_for_copy() {
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
    }
}

pub(crate) fn cut<H: MultilineInputHost>(
    host: &mut H,
    _: &Cut,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    let selected = host
        .multiline_input()
        .editor
        .selected_text_for_copy()
        .map(str::to_string);
    if let Some(selected) = selected {
        cx.write_to_clipboard(ClipboardItem::new_string(selected));
        run_edit(host, cx, |editor| {
            editor.insert_text("", EditTransaction::Discrete)
        });
    }
}

pub(crate) fn undo<H: MultilineInputHost>(
    host: &mut H,
    _: &Undo,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    run_edit(host, cx, TextEditorState::undo);
}

pub(crate) fn redo<H: MultilineInputHost>(
    host: &mut H,
    _: &Redo,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    run_edit(host, cx, TextEditorState::redo);
}

pub(crate) fn focus_next<H: MultilineInputHost>(
    _: &mut H,
    _: &Tab,
    window: &mut Window,
    cx: &mut Context<H>,
) {
    window.focus_next(cx);
}

pub(crate) fn focus_previous<H: MultilineInputHost>(
    _: &mut H,
    _: &ShiftTab,
    window: &mut Window,
    cx: &mut Context<H>,
) {
    window.focus_prev(cx);
}

pub(crate) fn dismiss<H: MultilineInputHost>(
    host: &mut H,
    _: &Escape,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    if host.multiline_input_mut().dismiss_context_menu() {
        cx.notify();
    }
}

pub(crate) fn on_mouse_down<H: MultilineInputHost>(
    host: &mut H,
    event: &MouseDownEvent,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    commit_composition(host);
    let offset = host
        .multiline_input()
        .index_for_mouse_position(event.position);
    let offset = host
        .multiline_input()
        .editor
        .offset_from_utf8(offset)
        .expect("layout hit tests must resolve to UTF-8 boundaries");
    let input = host.multiline_input_mut();
    input.context_menu_position = None;
    input.is_selecting = event.click_count < 2;
    input.reset_preferred_column();
    let result = if event.click_count >= 2 {
        input.editor.select_word_at(offset)
    } else if event.modifiers.shift {
        input
            .editor
            .set_selection(input.editor.selection().anchor(), offset)
    } else {
        input.editor.set_selection(offset, offset)
    };
    if matches!(result, Ok(EditOutcome::Changed)) {
        input.request_caret_visibility();
        cx.notify();
    }
}

pub(crate) fn on_mouse_up<H: MultilineInputHost>(
    host: &mut H,
    _: &MouseUpEvent,
    _: &mut Window,
    _: &mut Context<H>,
) {
    host.multiline_input_mut().is_selecting = false;
}

pub(crate) fn on_mouse_move<H: MultilineInputHost>(
    host: &mut H,
    event: &MouseMoveEvent,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    if !host.multiline_input().is_selecting {
        return;
    }
    let offset = host
        .multiline_input()
        .index_for_mouse_position(event.position);
    let offset = host
        .multiline_input()
        .editor
        .offset_from_utf8(offset)
        .expect("layout hit tests must resolve to UTF-8 boundaries");
    let input = host.multiline_input_mut();
    input.reset_preferred_column();
    if matches!(
        input
            .editor
            .set_selection(input.editor.selection().anchor(), offset),
        Ok(EditOutcome::Changed)
    ) {
        input.request_caret_visibility();
        cx.notify();
    }
}

pub(crate) fn open_context_menu<H: MultilineInputHost>(
    host: &mut H,
    event: &MouseDownEvent,
    window: &mut Window,
    cx: &mut Context<H>,
) {
    cx.stop_propagation();
    let focus_handle = host.multiline_focus_handle().clone();
    let input = host.multiline_input_mut();
    input.is_selecting = false;
    input.context_menu_position = Some(event.position);
    focus_handle.focus(window, cx);
    cx.notify();
}

pub(crate) fn handle_context_menu_action<H: MultilineInputHost>(
    host: &mut H,
    action: crate::ui::components::common::edit_context_menu::EditContextAction,
    window: &mut Window,
    cx: &mut Context<H>,
) {
    use crate::ui::components::common::edit_context_menu::EditContextAction;
    match action {
        EditContextAction::Undo => undo(host, &Undo, window, cx),
        EditContextAction::Redo => redo(host, &Redo, window, cx),
        EditContextAction::Cut => cut(host, &Cut, window, cx),
        EditContextAction::Copy => copy(host, &Copy, window, cx),
        EditContextAction::Paste => paste(host, &Paste, window, cx),
        EditContextAction::SelectAll => select_all(host, &SelectAll, window, cx),
        EditContextAction::Dismiss => {}
    }
    host.multiline_input_mut().context_menu_position = None;
    cx.notify();
}

pub(crate) fn text_for_range<H: MultilineInputHost>(
    host: &mut H,
    range_utf16: Range<usize>,
    actual_range: &mut Option<Range<usize>>,
) -> Option<String> {
    let (text, actual) = host
        .multiline_input()
        .editor
        .text_for_utf16_range(range_utf16)
        .ok()?;
    actual_range.replace(actual);
    Some(text.to_string())
}

pub(crate) fn selected_text_range<H: MultilineInputHost>(host: &mut H) -> UTF16Selection {
    let selection = host.multiline_input().editor.selection_utf16();
    UTF16Selection {
        range: selection.range,
        reversed: selection.reversed,
    }
}

pub(crate) fn marked_text_range<H: MultilineInputHost>(host: &H) -> Option<Range<usize>> {
    host.multiline_input().editor.composition_range_utf16()
}

pub(crate) fn unmark_text<H: MultilineInputHost>(host: &mut H) {
    host.multiline_input_mut().editor.commit_composition();
}

pub(crate) fn replace_text_in_range<H: MultilineInputHost>(
    host: &mut H,
    range_utf16: Option<Range<usize>>,
    new_text: &str,
    cx: &mut Context<H>,
) {
    let before = host.multiline_input().text().to_string();
    let has_composition = host.multiline_input().editor.composition_range().is_some();
    let changed = if has_composition {
        let updated = host
            .multiline_input_mut()
            .editor
            .update_composition_utf16(range_utf16, new_text, None)
            .is_ok();
        if updated {
            host.multiline_input_mut().editor.commit_composition();
        }
        updated
    } else {
        matches!(
            host.multiline_input_mut().editor.replace_utf16_range(
                range_utf16,
                new_text,
                EditTransaction::Typing,
            ),
            Ok(EditOutcome::Changed)
        )
    };
    if changed {
        host.multiline_input_mut().after_cursor_or_text_change();
    }
    finish_mutation(host, before, changed, cx);
}

pub(crate) fn replace_and_mark_text_in_range<H: MultilineInputHost>(
    host: &mut H,
    range_utf16: Option<Range<usize>>,
    new_text: &str,
    new_selected_range_utf16: Option<Range<usize>>,
    cx: &mut Context<H>,
) {
    let before = host.multiline_input().text().to_string();
    let changed = host
        .multiline_input_mut()
        .editor
        .update_composition_utf16(range_utf16, new_text, new_selected_range_utf16)
        .is_ok();
    if changed {
        host.multiline_input_mut().after_cursor_or_text_change();
    }
    finish_mutation(host, before, changed, cx);
}

pub(crate) fn bounds_for_range<H: MultilineInputHost>(
    host: &mut H,
    range_utf16: Range<usize>,
) -> Option<Bounds<Pixels>> {
    let input = host.multiline_input();
    let layout = input.layout.as_ref()?;
    let range = input.editor.range_from_utf16(range_utf16).ok()?;
    let ranges = line_ranges(input.editor.text());
    if !layout.matches(input.editor.text(), &ranges) {
        return None;
    }
    let start_line = line_index_for_offset(&ranges, range.start().utf8());
    let end_line = line_index_for_offset(&ranges, range.end().utf8());
    let start_range = ranges[start_line];
    let end_range = ranges[end_line];
    let start_local = range
        .start()
        .utf8()
        .clamp(start_range.start, start_range.end)
        - start_range.start;
    let end_local = range.end().utf8().clamp(end_range.start, end_range.end) - end_range.start;
    let top = layout.bounds.top() + layout.line_height * start_line as f32;
    let bottom = layout.bounds.top() + layout.line_height * (end_line + 1) as f32;
    if start_line == end_line {
        Some(Bounds::from_corners(
            point(
                layout.bounds.left() + layout.lines[start_line].x_for_index(start_local),
                top,
            ),
            point(
                layout.bounds.left() + layout.lines[end_line].x_for_index(end_local),
                bottom,
            ),
        ))
    } else {
        Some(Bounds::from_corners(
            point(layout.bounds.left(), top),
            point(layout.bounds.right(), bottom),
        ))
    }
}

pub(crate) fn character_index_for_point<H: MultilineInputHost>(
    host: &mut H,
    point: Point<Pixels>,
) -> Option<usize> {
    let input = host.multiline_input();
    input.layout.as_ref()?;
    let utf8 = input.index_for_mouse_position(point);
    input
        .editor
        .offset_to_utf16(input.editor.offset_from_utf8(utf8).ok()?)
        .ok()
}

fn move_horizontal<H: MultilineInputHost>(
    host: &mut H,
    movement: TextMovement,
    extend_selection: bool,
    cx: &mut Context<H>,
) {
    commit_composition(host);
    let input = host.multiline_input_mut();
    input.reset_preferred_column();
    if matches!(
        input.editor.move_cursor(movement, extend_selection),
        Ok(EditOutcome::Changed)
    ) {
        input.request_caret_visibility();
        cx.notify();
    }
}

fn move_vertical<H: MultilineInputHost>(
    host: &mut H,
    direction: VerticalDirection,
    extend_selection: bool,
    cx: &mut Context<H>,
) {
    commit_composition(host);
    if matches!(
        host.multiline_input_mut()
            .move_vertical(direction, extend_selection),
        Ok(EditOutcome::Changed)
    ) {
        cx.notify();
    }
}

fn run_edit<H, F>(host: &mut H, cx: &mut Context<H>, edit: F)
where
    H: MultilineInputHost,
    F: FnOnce(&mut TextEditorState) -> Result<EditOutcome, TextEditorError>,
{
    commit_composition(host);
    let before = host.multiline_input().text().to_string();
    let input = host.multiline_input_mut();
    input.reset_preferred_column();
    let changed = matches!(edit(&mut input.editor), Ok(EditOutcome::Changed));
    if changed {
        input.request_caret_visibility();
    }
    finish_mutation(host, before, changed, cx);
}

fn finish_mutation<H: MultilineInputHost>(
    host: &mut H,
    before: String,
    state_changed: bool,
    cx: &mut Context<H>,
) {
    if !state_changed {
        return;
    }
    let after = host.multiline_input().text().to_string();
    if after != before {
        host.emit_multiline_changed(after, cx);
    }
    cx.notify();
}

fn commit_composition<H: MultilineInputHost>(host: &mut H) {
    host.multiline_input_mut().editor.commit_composition();
}

pub(crate) struct MultilineTextElement<H: MultilineInputHost> {
    input: Entity<H>,
}

impl<H: MultilineInputHost> MultilineTextElement<H> {
    pub(crate) fn new(input: Entity<H>) -> Self {
        Self { input }
    }
}

pub(crate) struct PrepaintState {
    lines: Vec<ShapedLine>,
    ranges: Vec<LineRange>,
    cursor: Option<PaintQuad>,
    selection: Vec<PaintQuad>,
}

impl<H: MultilineInputHost> IntoElement for MultilineTextElement<H> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl<H: MultilineInputHost> Element for MultilineTextElement<H> {
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
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let line_count = line_ranges(self.input.read(cx).multiline_input().text()).len();
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = (window.line_height() * line_count as f32).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let host = self.input.read(cx);
        let input = host.multiline_input();
        let text = input.text();
        let ranges = line_ranges(text);
        let content_empty = text.is_empty();
        let style = window.text_style();
        let color = if content_empty {
            hsla(0.0, 0.0, 0.0, 0.4)
        } else {
            style.color
        };
        let font_size = style.font_size.to_pixels(window.rem_size());
        let lines = ranges
            .iter()
            .enumerate()
            .map(|(index, range)| {
                let display: SharedString = if content_empty && index == 0 {
                    input.placeholder().clone()
                } else {
                    text[range.start..range.end].to_string().into()
                };
                let run = TextRun {
                    len: display.len(),
                    font: style.font(),
                    color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                window
                    .text_system()
                    .shape_line(display, font_size, &[run], None)
            })
            .collect::<Vec<_>>();

        let line_height = window.line_height();
        let selection = input.editor.selected_range().utf8();
        let cursor = input.editor.selection().cursor().utf8();
        let cursor_line = line_index_for_offset(&ranges, cursor);
        let cursor_range = ranges[cursor_line];
        let cursor_local = cursor.clamp(cursor_range.start, cursor_range.end) - cursor_range.start;
        let cursor_quad = selection.is_empty().then(|| {
            fill(
                Bounds::new(
                    point(
                        bounds.left()
                            + if content_empty {
                                px(0.0)
                            } else {
                                lines[cursor_line].x_for_index(cursor_local)
                            },
                        bounds.top() + line_height * cursor_line as f32,
                    ),
                    size(px(2.0), line_height),
                ),
                rgb(INFO),
            )
        });
        let selection_quads = if selection.is_empty() || content_empty {
            Vec::new()
        } else {
            selection_quads(&ranges, &lines, selection, bounds, line_height)
        };

        PrepaintState {
            lines,
            ranges,
            cursor: cursor_quad,
            selection: selection_quads,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).multiline_focus_handle().clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        for selection in prepaint.selection.drain(..) {
            window.paint_quad(selection);
        }
        let line_height = window.line_height();
        for (index, line) in prepaint.lines.iter().enumerate() {
            let _ = line.paint(
                point(bounds.left(), bounds.top() + line_height * index as f32),
                line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            );
        }
        if focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }

        let layout = MultilineLayout {
            lines: prepaint.lines.clone(),
            ranges: prepaint.ranges.clone(),
            bounds,
            line_height,
        };
        self.input.update(cx, |host, cx| {
            if host.multiline_input_mut().install_layout(layout) {
                cx.notify();
            }
        });
    }
}

fn selection_quads(
    ranges: &[LineRange],
    lines: &[ShapedLine],
    selection: Range<usize>,
    bounds: Bounds<Pixels>,
    line_height: Pixels,
) -> Vec<PaintQuad> {
    let start_line = line_index_for_offset(ranges, selection.start);
    let end_line = line_index_for_offset(ranges, selection.end);
    let mut quads = Vec::new();
    for line_index in start_line..=end_line {
        let range = ranges[line_index];
        let local_start = if line_index == start_line {
            selection.start.clamp(range.start, range.end) - range.start
        } else {
            0
        };
        let local_end = if line_index == end_line {
            selection.end.clamp(range.start, range.end) - range.start
        } else {
            range.len()
        };
        let start_x = lines[line_index].x_for_index(local_start);
        let mut end_x = lines[line_index].x_for_index(local_end);
        if selection.end > range.end && range.next_start > range.end {
            end_x += NEWLINE_SELECTION_WIDTH;
        }
        if end_x <= start_x {
            continue;
        }
        let top = bounds.top() + line_height * line_index as f32;
        quads.push(fill(
            Bounds::from_corners(
                point(bounds.left() + start_x, top),
                point(bounds.left() + end_x, top + line_height),
            ),
            rgba(0x3366_ff33),
        ));
    }
    quads
}

fn clamped_offset(
    editor: &TextEditorState,
    requested: usize,
) -> crate::ui::text_editor::TextOffset {
    let mut requested = requested.min(editor.text().len());
    while editor.offset_from_utf8(requested).is_err() {
        requested -= 1;
    }
    editor
        .offset_from_utf8(requested)
        .expect("clamped offset must be a UTF-8 boundary")
}

fn line_ranges(text: &str) -> Vec<LineRange> {
    let bytes = text.as_bytes();
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                let next_start = if bytes.get(index + 1) == Some(&b'\n') {
                    index + 2
                } else {
                    index + 1
                };
                ranges.push(LineRange {
                    start,
                    end: index,
                    next_start,
                });
                start = next_start;
                index = next_start;
            }
            b'\n' => {
                let next_start = index + 1;
                ranges.push(LineRange {
                    start,
                    end: index,
                    next_start,
                });
                start = next_start;
                index = next_start;
            }
            _ => index += 1,
        }
    }
    ranges.push(LineRange {
        start,
        end: text.len(),
        next_start: text.len(),
    });
    ranges
}

fn line_index_for_offset(ranges: &[LineRange], offset: usize) -> usize {
    ranges
        .iter()
        .enumerate()
        .find_map(|(index, range)| (offset < range.next_start).then_some(index))
        .unwrap_or_else(|| ranges.len().saturating_sub(1))
}

fn grapheme_column(line: &str, offset: usize) -> usize {
    line.grapheme_indices(true)
        .take_while(|(start, _)| *start < offset)
        .count()
}

fn offset_for_grapheme_column(line: &str, column: usize) -> usize {
    line.grapheme_indices(true)
        .nth(column)
        .map(|(offset, _)| offset)
        .unwrap_or(line.len())
}

fn floor_char_boundary(text: &str, mut offset: usize) -> usize {
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_model_keeps_empty_trailing_lines_and_crlf_as_one_boundary() {
        assert_eq!(
            line_ranges("one\r\n二\n"),
            vec![
                LineRange {
                    start: 0,
                    end: 3,
                    next_start: 5,
                },
                LineRange {
                    start: 5,
                    end: 8,
                    next_start: 9,
                },
                LineRange {
                    start: 9,
                    end: 9,
                    next_start: 9,
                },
            ]
        );
        assert_eq!(line_index_for_offset(&line_ranges("a\n"), 1), 0);
        assert_eq!(line_index_for_offset(&line_ranges("a\n"), 2), 1);
    }

    #[test]
    fn vertical_fallback_preserves_grapheme_column_across_a_short_line() {
        let mut input = MultilineInputState::new("placeholder");
        input.set_text("abcd\n中\nwxyz");
        let start = input.editor.offset_from_utf8(3).unwrap();
        input.editor.set_selection(start, start).unwrap();

        input.move_vertical(VerticalDirection::Down, false).unwrap();
        assert_eq!(input.editor.selection().cursor().utf8(), "abcd\n中".len());
        input.move_vertical(VerticalDirection::Down, false).unwrap();
        assert_eq!(
            input.editor.selection().cursor().utf8(),
            "abcd\n中\nwxy".len()
        );
    }
}

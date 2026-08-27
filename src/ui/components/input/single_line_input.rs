use crate::ui::{
    text_editor::{
        EditOutcome, EditTransaction, TextEditorError, TextEditorPolicy, TextEditorState,
        TextMovement,
    },
    theme::INFO,
};
use gpui::{
    actions, fill, hsla, point, px, relative, rgb, rgba, size, App, Bounds, ClipboardItem, Context,
    Element, ElementId, ElementInputHandler, Entity, EntityInputHandler, FocusHandle,
    GlobalElementId, IntoElement, LayoutId, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    PaintQuad, Pixels, Point, ShapedLine, SharedString, Style, TextAlign, TextRun, UTF16Selection,
    Window,
};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

const MASK_GLYPH: &str = "•";

actions!(
    single_line_input,
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

/// Shared GPUI-facing state for one single-line editor. Text, cursor, selection, composition, and
/// history have one source of truth in `TextEditorState`; this adapter owns layout-only data.
pub(crate) struct SingleLineInputState {
    editor: TextEditorState,
    placeholder: SharedString,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    context_menu_position: Option<Point<Pixels>>,
}

impl SingleLineInputState {
    pub(crate) fn new(placeholder: impl Into<SharedString>) -> Self {
        Self {
            editor: TextEditorState::new(String::new(), TextEditorPolicy::single_line()),
            placeholder: placeholder.into(),
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
            context_menu_position: None,
        }
    }

    pub(crate) fn text(&self) -> &str {
        self.editor.text()
    }

    pub(crate) fn placeholder(&self) -> &SharedString {
        &self.placeholder
    }

    pub(crate) fn set_placeholder(&mut self, placeholder: impl Into<SharedString>) -> bool {
        let placeholder = placeholder.into();
        if self.placeholder == placeholder {
            false
        } else {
            self.placeholder = placeholder;
            true
        }
    }

    pub(crate) fn set_masked(&mut self, masked: bool) {
        self.editor.set_masked(masked);
    }

    pub(crate) fn is_masked(&self) -> bool {
        self.editor.policy().is_masked()
    }

    /// Programmatic user mutation used by the existing `set_url`/`set_content` APIs.
    pub(crate) fn set_text(&mut self, text: impl Into<String>) -> bool {
        let text = text.into();
        if self.editor.text() == text {
            return false;
        }
        self.editor.cancel_composition();
        let cursor = self.editor.selected_range().start().utf8();
        let full_range = self
            .editor
            .range_from_utf8(0..self.editor.text().len())
            .expect("complete editor text is always a valid range");
        let before = self.editor.text().to_string();
        let _ = self
            .editor
            .replace_range(full_range, &text, EditTransaction::Discrete);
        self.collapse_to_clamped_utf8(cursor);
        self.editor.text() != before
    }

    /// Silent model projection. Undo cannot cross this boundary; changed content preserves the
    /// legacy behavior of collapsing at the previous normalized selection start.
    pub(crate) fn project_text(&mut self, text: impl Into<String>) -> bool {
        let text = text.into();
        if self.editor.text() == text {
            self.editor.clear_edit_history();
            return false;
        }
        let cursor = self.editor.selected_range().start().utf8();
        let before = self.editor.text().to_string();
        self.editor.project_text(text);
        self.collapse_to_clamped_utf8(cursor);
        self.editor.text() != before
    }

    pub(crate) fn clear(&mut self) -> bool {
        if self.editor.text().is_empty() {
            return false;
        }
        self.editor.cancel_composition();
        let full_range = self
            .editor
            .range_from_utf8(0..self.editor.text().len())
            .expect("complete editor text is always a valid range");
        matches!(
            self.editor
                .replace_range(full_range, "", EditTransaction::Discrete),
            Ok(EditOutcome::Changed)
        )
    }

    pub(crate) fn context_menu_position(&self) -> Option<Point<Pixels>> {
        self.context_menu_position
    }

    pub(crate) fn dismiss_context_menu(&mut self) -> bool {
        self.context_menu_position.take().is_some()
    }

    pub(crate) fn display_text(&self) -> SharedString {
        if self.is_masked() {
            mask_text(self.editor.text()).into()
        } else {
            self.editor.text().to_string().into()
        }
    }

    fn display_offset_for_content_offset(&self, content_offset: usize) -> usize {
        if !self.is_masked() {
            return content_offset;
        }
        self.editor
            .text()
            .grapheme_indices(true)
            .take_while(|(offset, _)| *offset < content_offset)
            .count()
            * MASK_GLYPH.len()
    }

    fn content_offset_for_display_offset(&self, display_offset: usize) -> usize {
        if !self.is_masked() {
            return display_offset.min(self.editor.text().len());
        }
        let grapheme_index = display_offset / MASK_GLYPH.len();
        self.editor
            .text()
            .grapheme_indices(true)
            .nth(grapheme_index)
            .map(|(offset, _)| offset)
            .unwrap_or(self.editor.text().len())
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.editor.text().is_empty() {
            return 0;
        }
        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        let display_offset =
            resolve_hit_test_offset(position, *bounds, line.text.len(), |local_x| {
                line.closest_index_for_x(local_x)
            });
        self.content_offset_for_display_offset(display_offset)
    }

    fn collapse_to_clamped_utf8(&mut self, requested: usize) {
        let mut requested = requested.min(self.editor.text().len());
        while self.editor.offset_from_utf8(requested).is_err() {
            requested -= 1;
        }
        let offset = self
            .editor
            .offset_from_utf8(requested)
            .expect("clamped offset must be a UTF-8 boundary");
        let _ = self.editor.set_selection(offset, offset);
    }
}

pub(crate) trait SingleLineInputHost: EntityInputHandler + Sized + 'static {
    fn single_line_input(&self) -> &SingleLineInputState;
    fn single_line_input_mut(&mut self) -> &mut SingleLineInputState;
    fn single_line_focus_handle(&self) -> &FocusHandle;
    fn emit_single_line_changed(&mut self, value: String, cx: &mut Context<Self>);
    fn emit_single_line_submit(&mut self, cx: &mut Context<Self>);
}

pub(crate) fn backspace<H: SingleLineInputHost>(
    host: &mut H,
    _: &Backspace,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    run_edit(host, cx, |editor| editor.delete_backward());
}

pub(crate) fn delete<H: SingleLineInputHost>(
    host: &mut H,
    _: &Delete,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    run_edit(host, cx, |editor| editor.delete_forward());
}

pub(crate) fn left<H: SingleLineInputHost>(
    host: &mut H,
    _: &Left,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    navigate(host, TextMovement::PreviousGrapheme, false, cx);
}

pub(crate) fn right<H: SingleLineInputHost>(
    host: &mut H,
    _: &Right,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    navigate(host, TextMovement::NextGrapheme, false, cx);
}

pub(crate) fn word_left<H: SingleLineInputHost>(
    host: &mut H,
    _: &WordLeft,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    navigate(host, TextMovement::PreviousWord, false, cx);
}

pub(crate) fn word_right<H: SingleLineInputHost>(
    host: &mut H,
    _: &WordRight,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    navigate(host, TextMovement::NextWord, false, cx);
}

pub(crate) fn select_left<H: SingleLineInputHost>(
    host: &mut H,
    _: &SelectLeft,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    navigate(host, TextMovement::PreviousGrapheme, true, cx);
}

pub(crate) fn select_right<H: SingleLineInputHost>(
    host: &mut H,
    _: &SelectRight,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    navigate(host, TextMovement::NextGrapheme, true, cx);
}

pub(crate) fn select_word_left<H: SingleLineInputHost>(
    host: &mut H,
    _: &SelectWordLeft,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    navigate(host, TextMovement::PreviousWord, true, cx);
}

pub(crate) fn select_word_right<H: SingleLineInputHost>(
    host: &mut H,
    _: &SelectWordRight,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    navigate(host, TextMovement::NextWord, true, cx);
}

pub(crate) fn select_all<H: SingleLineInputHost>(
    host: &mut H,
    _: &SelectAll,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    commit_composition(host);
    if matches!(
        host.single_line_input_mut().editor.select_all(),
        Ok(EditOutcome::Changed)
    ) {
        cx.notify();
    }
}

pub(crate) fn home<H: SingleLineInputHost>(
    host: &mut H,
    _: &Home,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    navigate(host, TextMovement::DocumentStart, false, cx);
}

pub(crate) fn end<H: SingleLineInputHost>(
    host: &mut H,
    _: &End,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    navigate(host, TextMovement::DocumentEnd, false, cx);
}

pub(crate) fn paste<H: SingleLineInputHost>(
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

pub(crate) fn copy<H: SingleLineInputHost>(
    host: &mut H,
    _: &Copy,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    if let Some(text) = host.single_line_input().editor.selected_text_for_copy() {
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
    }
}

pub(crate) fn cut<H: SingleLineInputHost>(
    host: &mut H,
    _: &Cut,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    let selected = host
        .single_line_input()
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

pub(crate) fn undo<H: SingleLineInputHost>(
    host: &mut H,
    _: &Undo,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    run_edit(host, cx, TextEditorState::undo);
}

pub(crate) fn redo<H: SingleLineInputHost>(
    host: &mut H,
    _: &Redo,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    run_edit(host, cx, TextEditorState::redo);
}

pub(crate) fn submit<H: SingleLineInputHost>(
    host: &mut H,
    _: &Submit,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    host.emit_single_line_submit(cx);
}

pub(crate) fn focus_next<H: SingleLineInputHost>(
    _: &mut H,
    _: &FocusNext,
    window: &mut Window,
    cx: &mut Context<H>,
) {
    window.focus_next(cx);
}

pub(crate) fn focus_previous<H: SingleLineInputHost>(
    _: &mut H,
    _: &FocusPrevious,
    window: &mut Window,
    cx: &mut Context<H>,
) {
    window.focus_prev(cx);
}

pub(crate) fn dismiss<H: SingleLineInputHost>(
    host: &mut H,
    _: &Dismiss,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    if host.single_line_input_mut().dismiss_context_menu() {
        cx.notify();
    }
}

pub(crate) fn on_mouse_down<H: SingleLineInputHost>(
    host: &mut H,
    event: &MouseDownEvent,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    commit_composition(host);
    let offset = host
        .single_line_input()
        .index_for_mouse_position(event.position);
    let offset = host
        .single_line_input()
        .editor
        .offset_from_utf8(offset)
        .expect("layout hit tests must resolve to UTF-8 boundaries");
    let input = host.single_line_input_mut();
    input.context_menu_position = None;
    input.is_selecting = event.click_count < 2;
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
        cx.notify();
    }
}

pub(crate) fn on_mouse_up<H: SingleLineInputHost>(
    host: &mut H,
    _: &MouseUpEvent,
    _: &mut Window,
    _: &mut Context<H>,
) {
    host.single_line_input_mut().is_selecting = false;
}

pub(crate) fn on_mouse_move<H: SingleLineInputHost>(
    host: &mut H,
    event: &MouseMoveEvent,
    _: &mut Window,
    cx: &mut Context<H>,
) {
    if !host.single_line_input().is_selecting {
        return;
    }
    let offset = host
        .single_line_input()
        .index_for_mouse_position(event.position);
    let offset = host
        .single_line_input()
        .editor
        .offset_from_utf8(offset)
        .expect("layout hit tests must resolve to UTF-8 boundaries");
    let input = host.single_line_input_mut();
    if matches!(
        input
            .editor
            .set_selection(input.editor.selection().anchor(), offset),
        Ok(EditOutcome::Changed)
    ) {
        cx.notify();
    }
}

pub(crate) fn open_context_menu<H: SingleLineInputHost>(
    host: &mut H,
    event: &MouseDownEvent,
    window: &mut Window,
    cx: &mut Context<H>,
) {
    cx.stop_propagation();
    let focus_handle = host.single_line_focus_handle().clone();
    let input = host.single_line_input_mut();
    input.is_selecting = false;
    input.context_menu_position = Some(event.position);
    focus_handle.focus(window, cx);
    cx.notify();
}

pub(crate) fn handle_context_menu_action<H: SingleLineInputHost>(
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
    host.single_line_input_mut().context_menu_position = None;
    cx.notify();
}

pub(crate) fn text_for_range<H: SingleLineInputHost>(
    host: &mut H,
    range_utf16: Range<usize>,
    actual_range: &mut Option<Range<usize>>,
) -> Option<String> {
    let (text, actual) = host
        .single_line_input()
        .editor
        .text_for_utf16_range(range_utf16)
        .ok()?;
    actual_range.replace(actual);
    Some(text.to_string())
}

pub(crate) fn selected_text_range<H: SingleLineInputHost>(host: &mut H) -> UTF16Selection {
    let selection = host.single_line_input().editor.selection_utf16();
    UTF16Selection {
        range: selection.range,
        reversed: selection.reversed,
    }
}

pub(crate) fn marked_text_range<H: SingleLineInputHost>(host: &H) -> Option<Range<usize>> {
    host.single_line_input().editor.composition_range_utf16()
}

pub(crate) fn unmark_text<H: SingleLineInputHost>(host: &mut H) {
    host.single_line_input_mut().editor.commit_composition();
}

pub(crate) fn replace_text_in_range<H: SingleLineInputHost>(
    host: &mut H,
    range_utf16: Option<Range<usize>>,
    new_text: &str,
    cx: &mut Context<H>,
) {
    let before = host.single_line_input().text().to_string();
    let has_composition = host
        .single_line_input()
        .editor
        .composition_range()
        .is_some();
    let changed = if has_composition {
        let updated = host
            .single_line_input_mut()
            .editor
            .update_composition_utf16(range_utf16, new_text, None)
            .is_ok();
        if updated {
            host.single_line_input_mut().editor.commit_composition();
        }
        updated
    } else {
        matches!(
            host.single_line_input_mut().editor.replace_utf16_range(
                range_utf16,
                new_text,
                EditTransaction::Typing,
            ),
            Ok(EditOutcome::Changed)
        )
    };
    finish_mutation(host, before, changed, cx);
}

pub(crate) fn replace_and_mark_text_in_range<H: SingleLineInputHost>(
    host: &mut H,
    range_utf16: Option<Range<usize>>,
    new_text: &str,
    new_selected_range_utf16: Option<Range<usize>>,
    cx: &mut Context<H>,
) {
    let before = host.single_line_input().text().to_string();
    let changed = host
        .single_line_input_mut()
        .editor
        .update_composition_utf16(range_utf16, new_text, new_selected_range_utf16)
        .is_ok();
    finish_mutation(host, before, changed, cx);
}

pub(crate) fn bounds_for_range<H: SingleLineInputHost>(
    host: &mut H,
    range_utf16: Range<usize>,
    bounds: Bounds<Pixels>,
) -> Option<Bounds<Pixels>> {
    let input = host.single_line_input();
    let layout = input.last_layout.as_ref()?;
    let range = input.editor.range_from_utf16(range_utf16).ok()?;
    let display_start = input.display_offset_for_content_offset(range.start().utf8());
    let display_end = input.display_offset_for_content_offset(range.end().utf8());
    Some(Bounds::from_corners(
        point(
            bounds.left() + layout.x_for_index(display_start),
            bounds.top(),
        ),
        point(
            bounds.left() + layout.x_for_index(display_end),
            bounds.bottom(),
        ),
    ))
}

pub(crate) fn character_index_for_point<H: SingleLineInputHost>(
    host: &mut H,
    point: Point<Pixels>,
) -> Option<usize> {
    let input = host.single_line_input();
    input.last_bounds.as_ref()?;
    input.last_layout.as_ref()?;
    let utf8 = input.index_for_mouse_position(point);
    input
        .editor
        .offset_to_utf16(input.editor.offset_from_utf8(utf8).ok()?)
        .ok()
}

fn navigate<H: SingleLineInputHost>(
    host: &mut H,
    movement: TextMovement,
    extend_selection: bool,
    cx: &mut Context<H>,
) {
    commit_composition(host);
    if matches!(
        host.single_line_input_mut()
            .editor
            .move_cursor(movement, extend_selection),
        Ok(EditOutcome::Changed)
    ) {
        cx.notify();
    }
}

fn run_edit<H, F>(host: &mut H, cx: &mut Context<H>, edit: F)
where
    H: SingleLineInputHost,
    F: FnOnce(&mut TextEditorState) -> Result<EditOutcome, TextEditorError>,
{
    commit_composition(host);
    let before = host.single_line_input().text().to_string();
    let changed = matches!(
        edit(&mut host.single_line_input_mut().editor),
        Ok(EditOutcome::Changed)
    );
    finish_mutation(host, before, changed, cx);
}

fn finish_mutation<H: SingleLineInputHost>(
    host: &mut H,
    before: String,
    state_changed: bool,
    cx: &mut Context<H>,
) {
    if !state_changed {
        return;
    }
    let after = host.single_line_input().text().to_string();
    if after != before {
        host.emit_single_line_changed(after, cx);
    }
    cx.notify();
}

fn commit_composition<H: SingleLineInputHost>(host: &mut H) {
    host.single_line_input_mut().editor.commit_composition();
}

pub(crate) struct SingleLineTextElement<H: SingleLineInputHost> {
    input: Entity<H>,
}

impl<H: SingleLineInputHost> SingleLineTextElement<H> {
    pub(crate) fn new(input: Entity<H>) -> Self {
        Self { input }
    }
}

pub(crate) struct PrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl<H: SingleLineInputHost> IntoElement for SingleLineTextElement<H> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl<H: SingleLineInputHost> Element for SingleLineTextElement<H> {
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
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
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
        let input = host.single_line_input();
        let content_empty = input.text().is_empty();
        let selection = input.editor.selected_range().utf8();
        let cursor = input.editor.selection().cursor().utf8();
        let style = window.text_style();
        let (display_text, color) = if content_empty {
            (input.placeholder().clone(), hsla(0., 0., 0., 0.4))
        } else {
            (input.display_text(), style.color)
        };
        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &[run], None);
        let cursor_x = if content_empty {
            px(0.)
        } else {
            line.x_for_index(input.display_offset_for_content_offset(cursor))
        };
        let (selection_quad, cursor_quad) = if selection.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_x, bounds.top()),
                        size(px(2.), bounds.bottom() - bounds.top()),
                    ),
                    rgb(INFO),
                )),
            )
        } else if !content_empty {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left()
                                + line.x_for_index(
                                    input.display_offset_for_content_offset(selection.start),
                                ),
                            bounds.top(),
                        ),
                        point(
                            bounds.left()
                                + line.x_for_index(
                                    input.display_offset_for_content_offset(selection.end),
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
            line: Some(line),
            cursor: cursor_quad,
            selection: selection_quad,
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
        let focus_handle = self.input.read(cx).single_line_focus_handle().clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        let line = prepaint
            .line
            .take()
            .expect("single-line prepaint always shapes one line");
        let _ = line.paint(
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
        self.input.update(cx, |host, _| {
            let input = host.single_line_input_mut();
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

fn mask_text(text: &str) -> String {
    MASK_GLYPH.repeat(text.graphemes(true).count())
}

fn resolve_hit_test_offset(
    position: Point<Pixels>,
    bounds: Bounds<Pixels>,
    display_len: usize,
    closest_index: impl FnOnce(Pixels) -> usize,
) -> usize {
    if position.y < bounds.top() || position.x < bounds.left() {
        0
    } else if position.y > bounds.bottom() || position.x > bounds.right() {
        display_len
    } else {
        closest_index(position.x - bounds.left()).min(display_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::components::input::header_input::HeaderInput;
    use gpui::{AppContext, Modifiers, MouseButton, TestAppContext};

    #[test]
    fn masked_display_maps_graphemes_without_exposing_source_text() {
        let mut input = SingleLineInputState::new("placeholder");
        input.set_masked(true);
        assert!(input.set_text("a\u{301}👩‍💻secret"));

        let display = input.display_text();
        assert_eq!(display, "••••••••");
        assert!(!display.contains("secret"));
        assert_eq!(input.display_offset_for_content_offset(0), 0);
        assert_eq!(input.display_offset_for_content_offset(3), MASK_GLYPH.len());
        assert_eq!(input.content_offset_for_display_offset(MASK_GLYPH.len()), 3);
    }

    #[test]
    fn hit_testing_covers_start_middle_end_and_every_outside_edge() {
        let bounds = Bounds::new(point(px(10.), px(20.)), size(px(100.), px(20.)));
        let closest = |x: Pixels| {
            if x < px(30.) {
                1
            } else if x < px(70.) {
                4
            } else {
                8
            }
        };

        assert_eq!(
            resolve_hit_test_offset(point(px(10.), px(30.)), bounds, 8, closest),
            1
        );
        assert_eq!(
            resolve_hit_test_offset(point(px(60.), px(30.)), bounds, 8, closest),
            4
        );
        assert_eq!(
            resolve_hit_test_offset(point(px(110.), px(30.)), bounds, 8, closest),
            8
        );
        for point in [point(px(0.), px(30.)), point(px(50.), px(10.))] {
            assert_eq!(resolve_hit_test_offset(point, bounds, 8, closest), 0);
        }
        for point in [point(px(120.), px(30.)), point(px(50.), px(50.))] {
            assert_eq!(resolve_hit_test_offset(point, bounds, 8, closest), 8);
        }
    }

    #[test]
    fn shared_double_click_word_selection_uses_the_editor_core() {
        let mut input = SingleLineInputState::new("placeholder");
        input.set_text("alpha 中文 value_2");
        let offset = input.editor.offset_from_utf8(6).unwrap();
        input.editor.select_word_at(offset).unwrap();
        assert_eq!(input.editor.selected_text(), "中文");

        let separator = input.editor.offset_from_utf8(5).unwrap();
        input.editor.select_word_at(separator).unwrap();
        assert_eq!(input.editor.selected_text(), " ");
    }

    #[gpui::test]
    fn ime_bridge_keeps_utf16_selection_composition_and_history_in_sync(cx: &mut TestAppContext) {
        let input = cx.new(HeaderInput::new);

        input.update(cx, |host, cx| {
            replace_text_in_range(host, None, "base ", cx);
            replace_and_mark_text_in_range(host, None, "A😀中", Some(1..3), cx);

            assert_eq!(host.single_line_input().text(), "base A😀中");
            assert_eq!(marked_text_range(host), Some(5..9));
            assert_eq!(selected_text_range(host).range, 6..8);

            replace_and_mark_text_in_range(host, None, "候选", Some(1..1), cx);
            assert_eq!(host.single_line_input().text(), "base 候选");
            assert_eq!(marked_text_range(host), Some(5..7));
            assert_eq!(selected_text_range(host).range, 6..6);

            replace_text_in_range(host, None, "完成", cx);
            assert_eq!(host.single_line_input().text(), "base 完成");
            assert_eq!(marked_text_range(host), None);
            assert_eq!(selected_text_range(host).range, 7..7);

            assert_eq!(
                host.single_line_input_mut().editor.undo(),
                Ok(EditOutcome::Changed)
            );
            assert_eq!(host.single_line_input().text(), "base ");
        });
    }

    #[gpui::test]
    fn mouse_drag_and_double_click_share_layout_hit_testing(cx: &mut TestAppContext) {
        let (input, visual) = cx.add_window_view(|_, cx| HeaderInput::new(cx));
        input.update(visual, |host, cx| host.set_content("alpha beta", cx));

        let (drag_start, drag_end, word_position) = input.read_with(visual, |host, _| {
            let state = host.single_line_input();
            let bounds = state.last_bounds.expect("input should have painted bounds");
            let line = state
                .last_layout
                .as_ref()
                .expect("input should have painted a shaped line");
            let at = |index| point(bounds.left() + line.x_for_index(index), bounds.center().y);
            (at(1), at(5), at(7))
        });

        visual.update(|window, app| {
            input.update(app, |host, cx| {
                on_mouse_down(
                    host,
                    &MouseDownEvent {
                        position: drag_start,
                        modifiers: Modifiers::none(),
                        button: MouseButton::Left,
                        click_count: 1,
                        first_mouse: false,
                    },
                    window,
                    cx,
                );
                on_mouse_move(
                    host,
                    &MouseMoveEvent {
                        position: drag_end,
                        modifiers: Modifiers::none(),
                        pressed_button: Some(MouseButton::Left),
                    },
                    window,
                    cx,
                );
                on_mouse_up(
                    host,
                    &MouseUpEvent {
                        position: drag_end,
                        modifiers: Modifiers::none(),
                        button: MouseButton::Left,
                        click_count: 1,
                    },
                    window,
                    cx,
                );
            });
        });
        assert_eq!(
            input.read_with(visual, |host, _| host
                .single_line_input()
                .editor
                .selected_text()
                .to_string()),
            "lpha"
        );

        visual.update(|window, app| {
            input.update(app, |host, cx| {
                on_mouse_down(
                    host,
                    &MouseDownEvent {
                        position: word_position,
                        modifiers: Modifiers::none(),
                        button: MouseButton::Left,
                        click_count: 2,
                        first_mouse: false,
                    },
                    window,
                    cx,
                );
            });
        });
        assert_eq!(
            input.read_with(visual, |host, _| host
                .single_line_input()
                .editor
                .selected_text()
                .to_string()),
            "beta"
        );
    }
}

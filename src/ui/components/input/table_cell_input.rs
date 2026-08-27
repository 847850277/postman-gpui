use super::single_line_input::{
    self as single_line, SingleLineInputHost, SingleLineInputState, SingleLineTextElement,
};
use crate::ui::{
    components::common::edit_context_menu::{edit_context_menu, EDITABLE_ACTIONS},
    theme::{FONT_MONO, INFO, LINE, PANEL, TEXT},
};
use gpui::{
    div, prelude::FluentBuilder, px, rgb, App, Bounds, Context, CursorStyle, EntityInputHandler,
    EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Pixels, Point, Render, Styled, UTF16Selection, Window,
};
use std::{
    ops::Range,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TABLE_ROW_ID: AtomicU64 = AtomicU64::new(1);

/// UI-session identity for one logical table row. It is deliberately independent of a render
/// index so inserting or deleting a neighbor cannot move editor history to a different row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TableRowId(u64);

impl TableRowId {
    pub(crate) fn next() -> Self {
        Self(NEXT_TABLE_ROW_ID.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TableCellColumn {
    Key,
    Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TableCellId {
    row: TableRowId,
    column: TableCellColumn,
}

impl TableCellId {
    pub(crate) const fn new(row: TableRowId, column: TableCellColumn) -> Self {
        Self { row, column }
    }

    pub(crate) const fn row(self) -> TableRowId {
        self.row
    }

    pub(crate) const fn column(self) -> TableCellColumn {
        self.column
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TableCellTraversal {
    Forward,
    Backward,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TableCellInputEvent {
    ValueChanged {
        cell: TableCellId,
        value: String,
    },
    SubmitRequested {
        cell: TableCellId,
    },
    TraversalRequested {
        cell: TableCellId,
        direction: TableCellTraversal,
    },
}

/// Reusable single-line table cell. Text, selection, composition, and per-cell Undo/Redo are
/// owned by `SingleLineInputState`/`TextEditorState`; this shell only adds stable identity and
/// delegates table traversal to its parent.
pub(crate) struct TableCellInput {
    identity: TableCellId,
    focus_handle: FocusHandle,
    input: SingleLineInputState,
    context_menu_id: &'static str,
}

impl TableCellInput {
    pub(crate) fn new(
        identity: TableCellId,
        placeholder: impl Into<String>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            identity,
            focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            input: SingleLineInputState::new(placeholder.into()),
            context_menu_id: "header-edit-menu",
        }
    }

    pub(crate) fn with_context_menu_id(mut self, id: &'static str) -> Self {
        self.context_menu_id = id;
        self
    }

    #[cfg(test)]
    pub(crate) const fn identity(&self) -> TableCellId {
        self.identity
    }

    pub(crate) fn content(&self) -> &str {
        self.input.text()
    }

    /// Silent domain projection. This is also the explicit history boundary used when a cell is
    /// rebound to a new request/table projection.
    pub(crate) fn project_content(&mut self, value: impl Into<String>, cx: &mut Context<Self>) {
        if self.input.project_text(value) {
            cx.notify();
        }
    }
}

impl SingleLineInputHost for TableCellInput {
    fn single_line_input(&self) -> &SingleLineInputState {
        &self.input
    }

    fn single_line_input_mut(&mut self) -> &mut SingleLineInputState {
        &mut self.input
    }

    fn single_line_focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    fn emit_single_line_changed(&mut self, value: String, cx: &mut Context<Self>) {
        cx.emit(TableCellInputEvent::ValueChanged {
            cell: self.identity,
            value,
        });
    }

    fn emit_single_line_submit(&mut self, cx: &mut Context<Self>) {
        cx.emit(TableCellInputEvent::SubmitRequested {
            cell: self.identity,
        });
    }

    fn focus_next_single_line(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(TableCellInputEvent::TraversalRequested {
            cell: self.identity,
            direction: TableCellTraversal::Forward,
        });
    }

    fn focus_previous_single_line(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(TableCellInputEvent::TraversalRequested {
            cell: self.identity,
            direction: TableCellTraversal::Backward,
        });
    }
}

impl EntityInputHandler for TableCellInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        single_line::text_for_range(self, range_utf16, actual_range)
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(single_line::selected_text_range(self))
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        single_line::marked_text_range(self)
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        single_line::unmark_text(self);
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        single_line::replace_text_in_range(self, range_utf16, new_text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        single_line::replace_and_mark_text_in_range(
            self,
            range_utf16,
            new_text,
            new_selected_range_utf16,
            cx,
        );
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        single_line::bounds_for_range(self, range_utf16, bounds)
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        single_line::character_index_for_point(self, point)
    }
}

impl EventEmitter<TableCellInputEvent> for TableCellInput {}

impl Focusable for TableCellInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TableCellInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let context_menu_position = self.input.context_menu_position();
        let context_menu_id = self.context_menu_id;
        div()
            .flex_1()
            .h_full()
            .min_w_0()
            .flex()
            .items_center()
            .px_3()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(if self.focus_handle.is_focused(window) {
                rgb(INFO)
            } else {
                rgb(LINE)
            })
            .rounded_lg()
            .text_color(rgb(TEXT))
            .font_family(FONT_MONO)
            .text_size(px(12.0))
            .cursor(CursorStyle::IBeam)
            .track_focus(&self.focus_handle(cx))
            // Reuse the established single-line bindings; Tab is intercepted by the host hooks
            // above and resolved by the parent table from the stable cell identity.
            .key_context("HeaderInput")
            .on_action(cx.listener(single_line::backspace::<Self>))
            .on_action(cx.listener(single_line::delete::<Self>))
            .on_action(cx.listener(single_line::left::<Self>))
            .on_action(cx.listener(single_line::right::<Self>))
            .on_action(cx.listener(single_line::word_left::<Self>))
            .on_action(cx.listener(single_line::word_right::<Self>))
            .on_action(cx.listener(single_line::select_left::<Self>))
            .on_action(cx.listener(single_line::select_right::<Self>))
            .on_action(cx.listener(single_line::select_word_left::<Self>))
            .on_action(cx.listener(single_line::select_word_right::<Self>))
            .on_action(cx.listener(single_line::select_all::<Self>))
            .on_action(cx.listener(single_line::home::<Self>))
            .on_action(cx.listener(single_line::end::<Self>))
            .on_action(cx.listener(single_line::paste::<Self>))
            .on_action(cx.listener(single_line::cut::<Self>))
            .on_action(cx.listener(single_line::copy::<Self>))
            .on_action(cx.listener(single_line::undo::<Self>))
            .on_action(cx.listener(single_line::redo::<Self>))
            .on_action(cx.listener(single_line::submit::<Self>))
            .on_action(cx.listener(single_line::focus_next::<Self>))
            .on_action(cx.listener(single_line::focus_previous::<Self>))
            .on_action(cx.listener(single_line::dismiss::<Self>))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(single_line::on_mouse_down::<Self>),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(single_line::open_context_menu::<Self>),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(single_line::on_mouse_up::<Self>),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(single_line::on_mouse_up::<Self>),
            )
            .on_mouse_move(cx.listener(single_line::on_mouse_move::<Self>))
            .child(SingleLineTextElement::new(cx.entity().clone()))
            .when_some(context_menu_position, |root, position| {
                root.child(edit_context_menu(
                    position,
                    context_menu_id,
                    EDITABLE_ACTIONS,
                    single_line::handle_context_menu_action::<Self>,
                    window,
                    cx,
                ))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::components::input::single_line_input::Undo;
    use gpui::{AppContext, Entity, TestAppContext};

    struct CellPair {
        first: Entity<TableCellInput>,
        second: Entity<TableCellInput>,
    }

    impl Render for CellPair {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().child(self.first.clone()).child(self.second.clone())
        }
    }

    #[gpui::test]
    fn cell_identity_survives_unicode_ime_and_per_cell_history(cx: &mut TestAppContext) {
        let row = TableRowId::next();
        let identity = TableCellId::new(row, TableCellColumn::Value);
        let (cell, visual) = cx.add_window_view(|_, cx| TableCellInput::new(identity, "Value", cx));

        cell.update(visual, |cell, cx| {
            single_line::replace_and_mark_text_in_range(cell, None, "A😀中", Some(1..3), cx);
            assert_eq!(cell.identity(), identity);
            assert_eq!(cell.content(), "A😀中");
            assert_eq!(single_line::marked_text_range(cell), Some(0..4));
            assert_eq!(single_line::selected_text_range(cell).range, 1..3);

            single_line::replace_text_in_range(cell, None, "完成", cx);
            assert_eq!(cell.content(), "完成");
        });

        visual.update(|window, app| {
            cell.update(app, |cell, cx| {
                single_line::undo(cell, &Undo, window, cx);
            });
        });
        assert_eq!(
            cell.read_with(visual, |cell, _| cell.content().to_string()),
            ""
        );
    }

    #[gpui::test]
    fn undo_history_is_isolated_between_neighboring_cells(cx: &mut TestAppContext) {
        let (pair, visual) = cx.add_window_view(|_, cx| {
            let first_row = TableRowId::next();
            let second_row = TableRowId::next();
            CellPair {
                first: cx.new(|cx| {
                    TableCellInput::new(
                        TableCellId::new(first_row, TableCellColumn::Key),
                        "Key",
                        cx,
                    )
                }),
                second: cx.new(|cx| {
                    TableCellInput::new(
                        TableCellId::new(second_row, TableCellColumn::Key),
                        "Key",
                        cx,
                    )
                }),
            }
        });
        let (first, second) =
            pair.read_with(visual, |pair, _| (pair.first.clone(), pair.second.clone()));
        first.update(visual, |cell, cx| {
            single_line::replace_text_in_range(cell, None, "first😀", cx);
        });
        second.update(visual, |cell, cx| {
            single_line::replace_text_in_range(cell, None, "second中", cx);
        });

        visual.update(|window, app| {
            first.update(app, |cell, cx| {
                single_line::undo(cell, &Undo, window, cx);
            });
        });
        assert_eq!(
            first.read_with(visual, |cell, _| cell.content().to_string()),
            ""
        );
        assert_eq!(
            second.read_with(visual, |cell, _| cell.content().to_string()),
            "second中"
        );
    }
}

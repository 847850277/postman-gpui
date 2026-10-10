use crate::ui::theme::{FONT_MONO, PANEL, TEXT};
use gpui::{
    actions, div, App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, Global,
    InteractiveElement, IntoElement, KeyBinding, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_kit::component::{
    input::{Input, InputEvent, InputState},
    Sizable,
};
use std::sync::atomic::{AtomicU64, Ordering};

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

actions!(table_cell_input, [TraverseForward, TraverseBackward]);

struct TableCellBindings;
impl Global for TableCellBindings {}

/// Stable table identity and parent traversal around Kit's retained editing state.
/// Kit owns text editing, selection, IME, undo, and the input context menu.
pub(crate) struct TableCellInput {
    identity: TableCellId,
    input: Entity<InputState>,
    // Kit may emit Change after Enter even when its single-line value is unchanged.
    last_value: SharedString,
    _subscription: Subscription,
}

impl TableCellInput {
    pub(crate) fn new(
        identity: TableCellId,
        placeholder: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        if !cx.has_global::<TableCellBindings>() {
            cx.bind_keys([
                KeyBinding::new("tab", TraverseForward, Some("TableCellInput > Input")),
                KeyBinding::new(
                    "shift-tab",
                    TraverseBackward,
                    Some("TableCellInput > Input"),
                ),
            ]);
            cx.set_global(TableCellBindings);
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let subscription = cx.subscribe(&input, |this, input, event, cx| match event {
            InputEvent::Change => {
                let value = input.read(cx).value();
                if value != this.last_value {
                    this.last_value = value.clone();
                    cx.emit(TableCellInputEvent::ValueChanged {
                        cell: this.identity,
                        value: value.to_string(),
                    });
                }
            }
            InputEvent::PressEnter { .. } => cx.emit(TableCellInputEvent::SubmitRequested {
                cell: this.identity,
            }),
            InputEvent::Focus | InputEvent::Blur => {}
        });
        Self {
            identity,
            input,
            last_value: SharedString::default(),
            _subscription: subscription,
        }
    }

    #[cfg(test)]
    pub(crate) const fn identity(&self) -> TableCellId {
        self.identity
    }

    pub(crate) fn content(&self, cx: &App) -> SharedString {
        self.input.read(cx).value()
    }

    /// Silent domain projection. Equal values retain the user's selection and undo history.
    pub(crate) fn project_content(
        &mut self,
        value: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = value.into();
        if self.content(cx) != value {
            self.input
                .update(cx, |input, cx| input.set_value(value, window, cx));
        }
        self.last_value = self.content(cx);
    }

    fn traverse_forward(&mut self, _: &TraverseForward, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(TableCellInputEvent::TraversalRequested {
            cell: self.identity,
            direction: TableCellTraversal::Forward,
        });
    }

    fn traverse_backward(&mut self, _: &TraverseBackward, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(TableCellInputEvent::TraversalRequested {
            cell: self.identity,
            direction: TableCellTraversal::Backward,
        });
    }
}

impl EventEmitter<TableCellInputEvent> for TableCellInput {}

impl Focusable for TableCellInput {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }
}

impl Render for TableCellInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_1()
            .h_full()
            .min_w_0()
            .flex()
            .items_center()
            .key_context("TableCellInput")
            .on_action(cx.listener(Self::traverse_forward))
            .on_action(cx.listener(Self::traverse_backward))
            .child(
                Input::new(&self.input)
                    .small()
                    .flex_1()
                    .min_w_0()
                    .rounded_none()
                    .bordered(false)
                    .bg(PANEL.resolve(cx))
                    .text_color(TEXT.resolve(cx))
                    .font_family(FONT_MONO)
                    .text_size(crate::ui::theme::metrics::CODE),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{EntityInputHandler, TestAppContext};
    use gpui_kit::component::input::{Redo, Undo};

    struct CellPair {
        first: Entity<TableCellInput>,
        second: Entity<TableCellInput>,
        events: Vec<TableCellInputEvent>,
        _subscriptions: Vec<Subscription>,
    }

    impl CellPair {
        fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
            let first = cx.new(|cx| {
                TableCellInput::new(
                    TableCellId::new(TableRowId::next(), TableCellColumn::Key),
                    "Key",
                    window,
                    cx,
                )
            });
            let second = cx.new(|cx| {
                TableCellInput::new(
                    TableCellId::new(TableRowId::next(), TableCellColumn::Value),
                    "Value",
                    window,
                    cx,
                )
            });
            let subscriptions = [&first, &second]
                .into_iter()
                .map(|cell| {
                    cx.subscribe(cell, |this, _, event: &TableCellInputEvent, _| {
                        this.events.push(event.clone());
                    })
                })
                .collect();
            Self {
                first,
                second,
                events: Vec::new(),
                _subscriptions: subscriptions,
            }
        }
    }

    impl Render for CellPair {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .w_96()
                .flex()
                .flex_col()
                .child(div().h_8().child(self.first.clone()))
                .child(div().h_8().child(self.second.clone()))
        }
    }

    #[gpui::test]
    fn cell_identity_survives_unicode_ime_and_per_cell_history(cx: &mut TestAppContext) {
        cx.update(crate::ui::kit::init);
        let (pair, visual) = cx.add_window_view(CellPair::new);
        let cell = pair.read_with(visual, |pair, _| pair.first.clone());
        let identity = cell.read_with(visual, |cell, _| cell.identity());
        let state = cell.read_with(visual, |cell, _| cell.input.clone());
        cell.update_in(visual, |cell, window, cx| {
            cell.focus_handle(cx).focus(window, cx)
        });
        state.update_in(visual, |state, window, cx| {
            state.replace_and_mark_text_in_range(None, "A😀中", Some(1..3), window, cx);
            assert_eq!(state.value(), "A😀中");
            assert_eq!(state.marked_text_range(window, cx), Some(0..4));
            assert_eq!(
                state.selected_text_range(false, window, cx).unwrap().range,
                1..3
            );
            state.replace_text_in_range(None, "完成", window, cx);
            assert_eq!(state.value(), "完成");
            assert_eq!(state.marked_text_range(window, cx), None);
        });
        assert_eq!(cell.read_with(visual, |cell, _| cell.identity()), identity);
        visual.dispatch_action(Undo);
        assert_eq!(cell.read_with(visual, |cell, cx| cell.content(cx)), "");
        visual.dispatch_action(Redo);
        assert_eq!(cell.read_with(visual, |cell, cx| cell.content(cx)), "完成");
        assert!(
            pair.read_with(visual, |pair, _| pair.events.iter().all(|event| {
                matches!(event, TableCellInputEvent::ValueChanged { cell, .. } if *cell == identity)
            }))
        );
    }

    #[gpui::test]
    fn undo_history_is_isolated_between_neighboring_cells(cx: &mut TestAppContext) {
        cx.update(crate::ui::kit::init);
        let (pair, visual) = cx.add_window_view(CellPair::new);
        let (first, second) =
            pair.read_with(visual, |pair, _| (pair.first.clone(), pair.second.clone()));
        first.update_in(visual, |cell, window, cx| {
            cell.focus_handle(cx).focus(window, cx)
        });
        visual.simulate_input("first😀");
        second.update_in(visual, |cell, window, cx| {
            cell.focus_handle(cx).focus(window, cx)
        });
        visual.simulate_input("second中");
        first.update_in(visual, |cell, window, cx| {
            cell.focus_handle(cx).focus(window, cx)
        });
        visual.dispatch_action(Undo);
        assert_eq!(first.read_with(visual, |cell, cx| cell.content(cx)), "");
        assert_eq!(
            second.read_with(visual, |cell, cx| cell.content(cx)),
            "second中"
        );
    }

    #[gpui::test]
    fn projection_is_silent_and_equal_values_preserve_selection_and_undo(cx: &mut TestAppContext) {
        cx.update(crate::ui::kit::init);
        let (pair, visual) = cx.add_window_view(CellPair::new);
        let cell = pair.read_with(visual, |pair, _| pair.first.clone());
        cell.update_in(visual, |cell, window, cx| {
            cell.project_content("seed", window, cx);
            cell.focus_handle(cx).focus(window, cx);
        });
        assert!(pair.read_with(visual, |pair, _| pair.events.is_empty()));
        visual.simulate_input("😀");
        visual.simulate_keystrokes("shift-left");
        let state = cell.read_with(visual, |cell, _| cell.input.clone());
        let before = state.update_in(visual, |input, window, cx| {
            input.selected_text_range(false, window, cx).unwrap()
        });
        cell.update_in(visual, |cell, window, cx| {
            cell.project_content("seed😀", window, cx)
        });
        let after = state.update_in(visual, |input, window, cx| {
            input.selected_text_range(false, window, cx).unwrap()
        });
        assert_eq!(before.range, after.range);
        assert_eq!(pair.read_with(visual, |pair, _| pair.events.len()), 1);
        visual.dispatch_action(Undo);
        assert_eq!(cell.read_with(visual, |cell, cx| cell.content(cx)), "seed");
        cell.update_in(visual, |cell, window, cx| {
            cell.project_content("rebound", window, cx)
        });
        visual.dispatch_action(Undo);
        assert_eq!(
            cell.read_with(visual, |cell, cx| cell.content(cx)),
            "rebound"
        );
    }

    #[gpui::test]
    fn kit_typing_submit_and_tab_emit_stable_table_events(cx: &mut TestAppContext) {
        cx.update(crate::ui::kit::init);
        let (pair, visual) = cx.add_window_view(CellPair::new);
        let cell = pair.read_with(visual, |pair, _| pair.first.clone());
        let identity = cell.read_with(visual, |cell, _| cell.identity());
        cell.update_in(visual, |cell, window, cx| {
            cell.focus_handle(cx).focus(window, cx)
        });
        visual.simulate_input("key");
        visual.simulate_keystrokes("enter tab shift-tab");
        assert_eq!(cell.read_with(visual, |cell, cx| cell.content(cx)), "key");
        assert_eq!(
            pair.read_with(visual, |pair, _| pair.events.clone()),
            vec![
                TableCellInputEvent::ValueChanged {
                    cell: identity,
                    value: "k".into()
                },
                TableCellInputEvent::ValueChanged {
                    cell: identity,
                    value: "ke".into()
                },
                TableCellInputEvent::ValueChanged {
                    cell: identity,
                    value: "key".into()
                },
                TableCellInputEvent::SubmitRequested { cell: identity },
                TableCellInputEvent::TraversalRequested {
                    cell: identity,
                    direction: TableCellTraversal::Forward
                },
                TableCellInputEvent::TraversalRequested {
                    cell: identity,
                    direction: TableCellTraversal::Backward
                },
            ]
        );
    }
}

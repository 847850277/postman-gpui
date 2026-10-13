//! Kit 0.7.3 navigates/deletes Unicode scalars, which splits combining characters and ZWJ
//! emoji. Repeat its native action only when crossing such a cluster. Kit retains ownership of
//! selection direction, edit transactions and undo. Remove when Kit becomes grapheme-aware.
use gpui::{
    Action, App, Div, Entity, EntityInputHandler, Focusable, InteractiveElement, MouseButton,
    Stateful, Window,
};
use gpui_kit::{
    base::{
        actions::{SelectLeft, SelectRight},
        input::{AddCursorAbove, AddCursorBelow},
    },
    component::input::{Backspace, Delete, MoveLeft, MoveRight, TextareaState},
};
use std::{cell::Cell, rc::Rc};
use unicode_segmentation::UnicodeSegmentation;

pub(super) fn with_grapheme_boundaries(
    element: Stateful<Div>,
    input: &Entity<TextareaState>,
) -> Stateful<Div> {
    // Dispatch is synchronous. The guard lets the repeated actions reach Kit unchanged.
    let replaying = Rc::new(Cell::new(false));
    element
        // Preserve the existing single-caret editor contract. Kit applies each replay to every
        // caret, but exposes only the active range, so mixed clusters cannot be replayed safely.
        .capture_action(|_: &AddCursorAbove, _, cx| cx.stop_propagation())
        .capture_action(|_: &AddCursorBelow, _, cx| cx.stop_propagation())
        .capture_any_mouse_down(|event, _, cx| {
            if event.button == MouseButton::Left && event.modifiers.alt {
                cx.stop_propagation();
            }
        })
        .capture_action(handler::<MoveLeft>(
            input.clone(),
            replaying.clone(),
            true,
            false,
        ))
        .capture_action(handler::<MoveRight>(
            input.clone(),
            replaying.clone(),
            false,
            false,
        ))
        .capture_action(handler::<SelectLeft>(
            input.clone(),
            replaying.clone(),
            true,
            true,
        ))
        .capture_action(handler::<SelectRight>(
            input.clone(),
            replaying.clone(),
            false,
            true,
        ))
        .capture_action(handler::<Backspace>(
            input.clone(),
            replaying.clone(),
            true,
            false,
        ))
        .capture_action(handler::<Delete>(input.clone(), replaying, false, false))
}

fn handler<A: Action>(
    input: Entity<TextareaState>,
    replaying: Rc<Cell<bool>>,
    backward: bool,
    extend: bool,
) -> impl Fn(&A, &mut Window, &mut App) {
    move |action, window, cx| {
        if replaying.get() {
            return;
        }
        let composing = input.update(cx, |state, cx| {
            state.marked_text_range(window, cx).is_some()
        });
        let state = input.read(cx);
        if composing || (!extend && !state.selected_range().is_empty()) {
            return;
        }
        let text = state.value();
        let cursor = state.cursor();
        let target = if backward {
            text.grapheme_indices(true)
                .map(|(i, _)| i)
                .take_while(|i| *i < cursor)
                .last()
                .unwrap_or(0)
        } else {
            text.grapheme_indices(true)
                .map(|(i, _)| i)
                .find(|i| *i > cursor)
                .unwrap_or(text.len())
        };
        let cluster = &text[cursor.min(target)..cursor.max(target)];
        let steps = cluster.chars().count();
        // Kit already treats CRLF as one movement/deletion.
        if steps <= 1 || cluster == "\r\n" {
            return;
        }
        let focus = state.focus_handle(cx);
        replaying.set(true);
        for _ in 0..steps {
            focus.dispatch_action(action, window, cx);
        }
        replaying.set(false);
        cx.stop_propagation();
    }
}

use super::single_line_input::{
    self as single_line, Backspace, Copy, Cut, Delete, Dismiss, End, FocusNext, FocusPrevious,
    Home, Left, Paste, Redo, Right, SelectAll, SelectLeft, SelectRight, SelectWordLeft,
    SelectWordRight, SingleLineInputHost, SingleLineInputState, SingleLineTextElement, Submit,
    Undo, WordLeft, WordRight,
};
use crate::ui::{
    components::common::edit_context_menu::{edit_context_menu, EDITABLE_ACTIONS},
    theme::{FONT_MONO, INFO, LINE, PANEL, TEXT},
};
use gpui::{
    div, prelude::*, px, rgb, App, Bounds, Context, CursorStyle, EntityInputHandler, EventEmitter,
    FocusHandle, Focusable, IntoElement, KeyBinding, MouseButton, Pixels, Point, Render, Styled,
    UTF16Selection, Window,
};
use std::ops::Range;

#[derive(Debug, Clone)]
pub enum UrlInputEvent {
    UrlChanged(String),
    SubmitRequested,
}

/// URL-specific shell around the shared single-line GPUI adapter.
pub struct UrlInput {
    focus_handle: FocusHandle,
    input: SingleLineInputState,
}

impl UrlInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            input: SingleLineInputState::new("Enter request URL"),
        }
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.input.set_placeholder(placeholder.into());
        self
    }

    pub fn set_url(&mut self, url: impl Into<String>, cx: &mut Context<Self>) {
        if self.input.set_text(url) {
            cx.emit(UrlInputEvent::UrlChanged(self.input.text().to_string()));
            cx.notify();
        }
    }

    /// Projects the ViewModel value into the editor buffer without producing a user edit event.
    pub fn project_url(&mut self, url: impl Into<String>, cx: &mut Context<Self>) {
        if self.input.project_text(url) {
            cx.notify();
        }
    }
}

impl SingleLineInputHost for UrlInput {
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
        cx.emit(UrlInputEvent::UrlChanged(value));
    }

    fn emit_single_line_submit(&mut self, cx: &mut Context<Self>) {
        cx.emit(UrlInputEvent::SubmitRequested);
    }
}

impl EntityInputHandler for UrlInput {
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

impl EventEmitter<UrlInputEvent> for UrlInput {}

impl Focusable for UrlInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for UrlInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let context_menu_position = self.input.context_menu_position();
        div()
            .debug_selector(|| "url-input".into())
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .px_4()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(if self.focus_handle.is_focused(window) {
                rgb(INFO)
            } else {
                rgb(LINE)
            })
            .rounded_lg()
            .font_family(FONT_MONO)
            .text_size(px(13.0))
            .text_color(rgb(TEXT))
            .cursor(CursorStyle::IBeam)
            .track_focus(&self.focus_handle(cx))
            .key_context("UrlInput")
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
                    "url-edit-menu",
                    EDITABLE_ACTIONS,
                    single_line::handle_context_menu_action::<Self>,
                    window,
                    cx,
                ))
            })
    }
}

pub fn setup_url_input_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("backspace", Backspace, Some("UrlInput")),
        KeyBinding::new("delete", Delete, Some("UrlInput")),
        KeyBinding::new("left", Left, Some("UrlInput")),
        KeyBinding::new("right", Right, Some("UrlInput")),
        KeyBinding::new("alt-left", WordLeft, Some("UrlInput")),
        KeyBinding::new("ctrl-left", WordLeft, Some("UrlInput")),
        KeyBinding::new("alt-right", WordRight, Some("UrlInput")),
        KeyBinding::new("ctrl-right", WordRight, Some("UrlInput")),
        KeyBinding::new("shift-left", SelectLeft, Some("UrlInput")),
        KeyBinding::new("shift-right", SelectRight, Some("UrlInput")),
        KeyBinding::new("alt-shift-left", SelectWordLeft, Some("UrlInput")),
        KeyBinding::new("ctrl-shift-left", SelectWordLeft, Some("UrlInput")),
        KeyBinding::new("alt-shift-right", SelectWordRight, Some("UrlInput")),
        KeyBinding::new("ctrl-shift-right", SelectWordRight, Some("UrlInput")),
        KeyBinding::new("cmd-a", SelectAll, Some("UrlInput")),
        KeyBinding::new("ctrl-a", SelectAll, Some("UrlInput")),
        KeyBinding::new("cmd-v", Paste, Some("UrlInput")),
        KeyBinding::new("ctrl-v", Paste, Some("UrlInput")),
        KeyBinding::new("cmd-c", Copy, Some("UrlInput")),
        KeyBinding::new("ctrl-c", Copy, Some("UrlInput")),
        KeyBinding::new("cmd-x", Cut, Some("UrlInput")),
        KeyBinding::new("ctrl-x", Cut, Some("UrlInput")),
        KeyBinding::new("cmd-z", Undo, Some("UrlInput")),
        KeyBinding::new("ctrl-z", Undo, Some("UrlInput")),
        KeyBinding::new("cmd-shift-z", Redo, Some("UrlInput")),
        KeyBinding::new("ctrl-shift-z", Redo, Some("UrlInput")),
        KeyBinding::new("ctrl-y", Redo, Some("UrlInput")),
        KeyBinding::new("home", Home, Some("UrlInput")),
        KeyBinding::new("end", End, Some("UrlInput")),
        KeyBinding::new("cmd-left", Home, Some("UrlInput")),
        KeyBinding::new("cmd-right", End, Some("UrlInput")),
        KeyBinding::new("enter", Submit, Some("UrlInput")),
        KeyBinding::new("tab", FocusNext, Some("UrlInput")),
        KeyBinding::new("shift-tab", FocusPrevious, Some("UrlInput")),
        KeyBinding::new("escape", Dismiss, Some("UrlInput")),
    ]
}

use crate::ui::{
    components::{
        common::edit_context_menu::{edit_context_menu, EDITABLE_ACTIONS},
        input::multiline_input::{
            self as multiline, MultilineInputHost, MultilineInputState, MultilineTextElement,
        },
    },
    theme::{CODE_BG, CODE_TEXT, FONT_MONO, INFO, LINE},
};
use gpui::{
    div, prelude::FluentBuilder, px, rgb, App, Bounds, Context, CursorStyle, EntityInputHandler,
    EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Pixels, Point, Render, StatefulInteractiveElement, Styled, UTF16Selection,
    Window,
};
use std::ops::Range;

#[derive(Clone, Debug)]
pub(super) enum TextBodyInputEvent {
    ValueChanged(String),
}

/// Raw/JSON/XML shell around the shared multiline editor adapter. Request normalization,
/// content-type derivation, and saved-state ownership remain in the parent Body/ViewModel layers.
pub(super) struct TextBodyInput {
    focus_handle: FocusHandle,
    input: MultilineInputState,
}

impl TextBodyInput {
    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            input: MultilineInputState::new("Enter JSON body here..."),
        }
    }

    pub(super) fn content(&self) -> &str {
        self.input.text()
    }

    pub(super) fn set_content(&mut self, content: impl Into<String>, cx: &mut Context<Self>) {
        if self.input.set_text(content) {
            cx.emit(TextBodyInputEvent::ValueChanged(
                self.input.text().to_string(),
            ));
            cx.notify();
        }
    }

    pub(super) fn project_content(&mut self, content: impl Into<String>, cx: &mut Context<Self>) {
        if self.input.project_text(content) {
            cx.notify();
        }
    }

    pub(super) fn clear(&mut self, cx: &mut Context<Self>) {
        if self.input.clear() {
            cx.emit(TextBodyInputEvent::ValueChanged(String::new()));
            cx.notify();
        }
    }
}

impl MultilineInputHost for TextBodyInput {
    fn multiline_input(&self) -> &MultilineInputState {
        &self.input
    }

    fn multiline_input_mut(&mut self) -> &mut MultilineInputState {
        &mut self.input
    }

    fn multiline_focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    fn emit_multiline_changed(&mut self, value: String, cx: &mut Context<Self>) {
        cx.emit(TextBodyInputEvent::ValueChanged(value));
    }
}

impl EventEmitter<TextBodyInputEvent> for TextBodyInput {}

impl Focusable for TextBodyInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for TextBodyInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        multiline::text_for_range(self, range_utf16, actual_range)
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(multiline::selected_text_range(self))
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        multiline::marked_text_range(self)
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        multiline::unmark_text(self);
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        multiline::replace_text_in_range(self, range_utf16, new_text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        multiline::replace_and_mark_text_in_range(
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
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        multiline::bounds_for_range(self, range_utf16)
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        multiline::character_index_for_point(self, point)
    }
}

impl Render for TextBodyInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let context_menu_position = self.input.context_menu_position();
        let scroll_handle = self.input.scroll_handle().clone();
        let editor = div().flex_1().min_h_0().flex().flex_col().child(
            div()
                .id("body-text-scroll")
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
                .font_family(FONT_MONO)
                .text_size(px(13.0))
                .text_color(rgb(CODE_TEXT))
                .cursor(CursorStyle::IBeam)
                .track_focus(&self.focus_handle(cx))
                .key_context("BodyInput")
                .overflow_y_scroll()
                .track_scroll(&scroll_handle)
                .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
                .on_action(cx.listener(multiline::backspace::<Self>))
                .on_action(cx.listener(multiline::delete::<Self>))
                .on_action(cx.listener(multiline::left::<Self>))
                .on_action(cx.listener(multiline::right::<Self>))
                .on_action(cx.listener(multiline::word_left::<Self>))
                .on_action(cx.listener(multiline::word_right::<Self>))
                .on_action(cx.listener(multiline::up::<Self>))
                .on_action(cx.listener(multiline::down::<Self>))
                .on_action(cx.listener(multiline::select_left::<Self>))
                .on_action(cx.listener(multiline::select_right::<Self>))
                .on_action(cx.listener(multiline::select_word_left::<Self>))
                .on_action(cx.listener(multiline::select_word_right::<Self>))
                .on_action(cx.listener(multiline::select_up::<Self>))
                .on_action(cx.listener(multiline::select_down::<Self>))
                .on_action(cx.listener(multiline::select_all::<Self>))
                .on_action(cx.listener(multiline::home::<Self>))
                .on_action(cx.listener(multiline::end::<Self>))
                .on_action(cx.listener(multiline::paste::<Self>))
                .on_action(cx.listener(multiline::cut::<Self>))
                .on_action(cx.listener(multiline::copy::<Self>))
                .on_action(cx.listener(multiline::undo::<Self>))
                .on_action(cx.listener(multiline::redo::<Self>))
                .on_action(cx.listener(multiline::enter::<Self>))
                .on_action(cx.listener(multiline::focus_next::<Self>))
                .on_action(cx.listener(multiline::focus_previous::<Self>))
                .on_action(cx.listener(multiline::dismiss::<Self>))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(multiline::on_mouse_down::<Self>),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(multiline::open_context_menu::<Self>),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(multiline::on_mouse_up::<Self>),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(multiline::on_mouse_up::<Self>),
                )
                .on_mouse_move(cx.listener(multiline::on_mouse_move::<Self>))
                .child(MultilineTextElement::new(cx.entity().clone())),
        );
        editor.when_some(context_menu_position, |root, position| {
            root.child(edit_context_menu(
                position,
                "body-edit-menu",
                EDITABLE_ACTIONS,
                multiline::handle_context_menu_action::<Self>,
                window,
                cx,
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::components::input::body_input::{Down, Redo, Undo};
    use gpui::{
        Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, TestAppContext,
    };

    #[gpui::test]
    fn ime_bridge_preserves_multiline_utf16_ranges_and_undo(cx: &mut TestAppContext) {
        let (input, visual) = cx.add_window_view(|_, cx| TextBodyInput::new(cx));
        input.update(visual, |host, cx| {
            multiline::replace_text_in_range(host, None, "first\n", cx);
            multiline::replace_and_mark_text_in_range(host, None, "A😀中", Some(1..3), cx);
            assert_eq!(host.content(), "first\nA😀中");
            assert_eq!(multiline::marked_text_range(host), Some(6..10));
            assert_eq!(multiline::selected_text_range(host).range, 7..9);

            multiline::replace_text_in_range(host, None, "完成", cx);
            assert_eq!(host.content(), "first\n完成");
            assert_eq!(multiline::marked_text_range(host), None);
        });

        visual.update(|window, app| {
            input.update(app, |host, cx| multiline::undo(host, &Undo, window, cx));
        });
        assert_eq!(
            input.read_with(visual, |host, _| host.content().to_string()),
            "first\n"
        );
        visual.update(|window, app| {
            input.update(app, |host, cx| multiline::redo(host, &Redo, window, cx));
        });
        assert_eq!(
            input.read_with(visual, |host, _| host.content().to_string()),
            "first\n完成"
        );
    }

    #[gpui::test]
    fn visual_column_survives_short_lines_and_long_content_scrolls_to_caret(
        cx: &mut TestAppContext,
    ) {
        let (input, visual) = cx.add_window_view(|_, cx| TextBodyInput::new(cx));
        input.update(visual, |host, cx| {
            host.set_content("abcd\nx\nwxyz", cx);
            multiline::replace_text_in_range(host, Some(3..3), "", cx);
        });

        for expected in [6..6, 10..10] {
            visual.update(|window, app| {
                input.update(app, |host, cx| multiline::down(host, &Down, window, cx));
            });
            assert_eq!(
                input.update(visual, |host, _| multiline::selected_text_range(host).range),
                expected
            );
        }

        let long_body = (0..80)
            .map(|line| format!("line-{line:02}"))
            .collect::<Vec<_>>()
            .join("\n");
        let end = long_body.encode_utf16().count();
        input.update(visual, |host, cx| {
            host.set_content(long_body, cx);
            multiline::replace_text_in_range(host, Some(end..end), "", cx);
        });
        visual.run_until_parked();
        assert!(
            input.read_with(visual, |host, _| host.input.scroll_handle().offset().y
                < px(0.0)),
            "moving the caret to the final line should scroll the editor"
        );

        input.update(visual, |host, cx| {
            multiline::replace_text_in_range(host, Some(0..0), "", cx);
        });
        visual.run_until_parked();
        assert_eq!(
            input.read_with(visual, |host, _| host.input.scroll_handle().offset().y),
            px(0.0)
        );
    }

    #[gpui::test]
    fn mouse_drag_selects_across_shaped_lines(cx: &mut TestAppContext) {
        let (input, visual) = cx.add_window_view(|_, cx| TextBodyInput::new(cx));
        input.update(visual, |host, cx| host.set_content("alpha\nbeta", cx));
        let (start, end) = input.update(visual, |host, _| {
            (
                multiline::bounds_for_range(host, 1..1)
                    .expect("first line offset should be laid out")
                    .center(),
                multiline::bounds_for_range(host, 9..9)
                    .expect("second line offset should be laid out")
                    .center(),
            )
        });

        visual.update(|window, app| {
            input.update(app, |host, cx| {
                multiline::on_mouse_down(
                    host,
                    &MouseDownEvent {
                        position: start,
                        modifiers: Modifiers::none(),
                        button: MouseButton::Left,
                        click_count: 1,
                        first_mouse: false,
                    },
                    window,
                    cx,
                );
                multiline::on_mouse_move(
                    host,
                    &MouseMoveEvent {
                        position: end,
                        modifiers: Modifiers::none(),
                        pressed_button: Some(MouseButton::Left),
                    },
                    window,
                    cx,
                );
                multiline::on_mouse_up(
                    host,
                    &MouseUpEvent {
                        position: end,
                        modifiers: Modifiers::none(),
                        button: MouseButton::Left,
                        click_count: 1,
                    },
                    window,
                    cx,
                );
            });
        });
        let selected = input.update(visual, |host, _| {
            let selection = multiline::selected_text_range(host).range;
            let mut actual = None;
            multiline::text_for_range(host, selection, &mut actual).unwrap()
        });
        assert_eq!(selected, "lpha\nbet");
    }
}

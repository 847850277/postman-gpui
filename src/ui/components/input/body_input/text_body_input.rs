use crate::ui::{
    components::{
        common::edit_context_menu::edit_popup_menu,
        input::multiline_input::{
            self as multiline, MultilineInputHost, MultilineInputState, MultilineTextElement,
        },
    },
    theme::{CODE_BG, FONT_MONO, INFO, LINE},
};
use gpui::{
    div, prelude::FluentBuilder, App, Bounds, Context, CursorStyle, EntityInputHandler,
    EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Pixels, Point, Render, StatefulInteractiveElement, Styled, UTF16Selection,
    Window,
};
use gpui_kit::{
    base::ElementExt,
    component::{
        menu::ContextMenuExt,
        scroll::{Scrollbar, ScrollbarMode},
    },
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
    has_overflow: bool,
}

impl TextBodyInput {
    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            input: MultilineInputState::new("Enter request body…"),
            has_overflow: false,
        }
    }

    #[cfg(test)]
    pub(super) fn content(&self) -> &str {
        self.input.text()
    }

    #[cfg(test)]
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

    #[cfg(test)]
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
        let scroll_handle = self.input.scroll_handle().clone();
        let menu_focus = self.focus_handle.clone();
        div()
            .id("body-text-editor")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .relative()
            .child(
                div()
                    .id("body-text-scroll")
                    .debug_selector(|| "body-text-scroll".into())
                    .w_full()
                    .h_full()
                    .min_h_0()
                    .px_3()
                    .py_3()
                    .when(self.has_overflow, |editor| editor.pr_5())
                    .bg(CODE_BG.resolve(cx))
                    .border_1()
                    .border_color(if self.focus_handle.is_focused(window) {
                        INFO.resolve(cx)
                    } else {
                        LINE.resolve(cx)
                    })
                    .rounded(crate::ui::theme::metrics::RADIUS)
                    .font_family(FONT_MONO)
                    .text_size(crate::ui::theme::metrics::CODE)
                    .line_height(gpui::relative(1.8))
                    .text_color(crate::ui::theme::TEXT.resolve(cx))
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
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(multiline::on_mouse_down::<Self>),
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
            )
            // Keep the observation canvas outside the padded scroll content.
            .on_prepaint({
                let this = cx.weak_entity();
                let scroll = scroll_handle.clone();
                let previous = self.has_overflow;
                move |_, window, cx| {
                    let has_overflow = scroll.max_offset().y > gpui::Pixels::ZERO;
                    if has_overflow != previous {
                        window.defer(cx, move |_, cx| {
                            let _ = this.update(cx, |this, cx| {
                                if this.has_overflow != has_overflow {
                                    this.has_overflow = has_overflow;
                                    cx.notify();
                                }
                            });
                        });
                    }
                }
            })
            .when(self.has_overflow, |editor| {
                editor.child(
                    div()
                        .debug_selector(|| "body-text-scrollbar".into())
                        .absolute()
                        .top_0()
                        .right_0()
                        .bottom_0()
                        .w(Scrollbar::width())
                        .child(
                            Scrollbar::vertical(&scroll_handle)
                                .id("body-text-scrollbar-control")
                                .mode(ScrollbarMode::Always),
                        ),
                )
            })
            .capture_any_mouse_down(cx.listener(multiline::prepare_context_menu::<Self>))
            .context_menu(move |menu, _, _| {
                use super::{Copy, Cut, Paste, Redo, SelectAll, Undo};
                edit_popup_menu(
                    menu,
                    menu_focus.clone(),
                    vec![
                        ("Undo", Box::new(Undo)),
                        ("Redo", Box::new(Redo)),
                        ("Cut", Box::new(Cut)),
                        ("Copy", Box::new(Copy)),
                        ("Paste", Box::new(Paste)),
                        ("Select All", Box::new(SelectAll)),
                    ],
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::components::input::body_input::{Down, Redo, Undo};
    use gpui::{
        px, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, TestAppContext,
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
        let scrollbar = visual
            .debug_bounds("body-text-scrollbar")
            .expect("long multiline content should expose a visible scrollbar");
        let viewport = visual.debug_bounds("body-text-scroll").unwrap();
        assert_eq!(scrollbar.top(), viewport.top());
        assert_eq!(scrollbar.bottom(), viewport.bottom());
        assert_eq!(scrollbar.right(), viewport.right());
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
    fn scrollbar_pointer_scroll_preserves_selection_and_caret_geometry(cx: &mut TestAppContext) {
        use gpui_kit::test::TestWindowExt;

        let (input, visual) = cx.add_window_view(|_, cx| TextBodyInput::new(cx));
        let body = (0..80)
            .map(|line| format!("line-{line:02}"))
            .collect::<Vec<_>>()
            .join("\n");
        input.update(visual, |host, cx| {
            host.set_content(body.clone(), cx);
            multiline::replace_text_in_range(host, Some(0..0), "", cx);
        });
        visual.run_until_parked();
        let viewport = visual.debug_bounds("body-text-scroll").unwrap();
        let bar = visual.debug_bounds("body-text-scrollbar").unwrap();
        let initial_caret = input.update(visual, |host, _| {
            multiline::bounds_for_range(host, 0..0).unwrap()
        });
        let start = gpui::point(bar.center().x, bar.top() + px(8.));
        let end = gpui::point(start.x, bar.center().y);
        visual.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
        visual.simulate_mouse_move(
            gpui::point(start.x, start.y + px(6.)),
            MouseButton::Left,
            Modifiers::none(),
        );
        visual.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
        visual.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
        let dragged_offset = input.update(visual, |host, _| {
            let offset = host.input.scroll_handle().offset().y;
            assert!(
                offset < px(0.),
                "dragging the real Kit thumb must scroll the text"
            );
            assert_eq!(multiline::selected_text_range(host).range, 0..0);
            assert_eq!(host.content(), body);
            let caret = multiline::bounds_for_range(host, 0..0).unwrap();
            assert_eq!(caret.top() - initial_caret.top(), offset);
            offset
        });
        assert_eq!(visual.debug_bounds("body-text-scrollbar").unwrap(), bar);
        assert_eq!(visual.debug_bounds("body-text-scroll").unwrap(), viewport);
        // An unrelated repaint must not snap manual scrolling back to the caret.
        input.update(visual, |_, cx| cx.notify());
        visual.run_until_parked();
        assert_eq!(
            input.update(visual, |host, _| host.input.scroll_handle().offset().y),
            dragged_offset
        );

        // The drag may already reach the end. Reveal the first line before
        // independently testing a click beyond the thumb at the track's bottom.
        input.update(visual, |host, cx| {
            multiline::replace_text_in_range(host, Some(1..1), "", cx);
        });
        visual.run_until_parked();
        assert_eq!(
            input.read_with(visual, |host, _| host.input.scroll_handle().offset().y),
            px(0.)
        );
        let bottom = gpui::point(bar.center().x, bar.bottom() - px(2.));
        visual.simulate_mouse_down(bottom, MouseButton::Left, Modifiers::none());
        visual.simulate_mouse_up(bottom, MouseButton::Left, Modifiers::none());
        input.update(visual, |host, _| {
            let scroll = host.input.scroll_handle();
            assert!(
                scroll.offset().y < px(0.),
                "clicking the track must scroll the text"
            );
            assert_eq!(
                scroll.offset().y,
                -scroll.max_offset().y,
                "track-end click should reveal the final line"
            );
            assert_eq!(multiline::selected_text_range(host).range, 1..1);
        });

        // A subsequent caret movement still reveals the editor's first line.
        input.update(visual, |host, cx| {
            multiline::replace_text_in_range(host, Some(2..2), "", cx);
        });
        visual.run_until_parked();
        visual.update(|window, cx| window.render_frame(cx));
        input.update(visual, |host, _| {
            assert_eq!(host.input.scroll_handle().offset().y, px(0.));
            let caret = multiline::bounds_for_range(host, 2..2).unwrap();
            assert!(caret.top() >= viewport.top());
            assert!(caret.bottom() <= viewport.bottom());
            assert_eq!(host.content(), body);
        });
        input.update(visual, |host, cx| host.clear(cx));
        visual.run_until_parked();
        assert!(visual.debug_bounds("body-text-scrollbar").is_none());
        input.read_with(visual, |host, _| {
            assert_eq!(host.input.scroll_handle().max_offset().y, px(0.));
            assert_eq!(host.input.scroll_handle().offset().y, px(0.));
        });
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

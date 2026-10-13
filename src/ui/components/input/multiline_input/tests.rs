use super::*;
use gpui::{
    point, px, size, AnyWindowHandle, Bounds, EntityInputHandler, ScrollDelta, TestAppContext,
    WindowBounds, WindowOptions,
};
use gpui_kit::test::TestWindowExt;
use std::{cell::RefCell, rc::Rc};

fn open(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<MultilineInput>) {
    cx.update(crate::ui::kit::init);
    cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(500.), px(240.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| MultilineInput::new("Request text", "", window, cx)),
        )
    })
    .unwrap()
}

#[gpui::test]
fn kit_ime_commits_one_undo_unit_and_projection_is_silent_even_when_equal(cx: &mut TestAppContext) {
    let (handle, input) = open(cx);
    let values = Rc::new(RefCell::new(Vec::new()));
    let observed = values.clone();
    let _subscription = cx.update(|cx| {
        cx.subscribe(&input, move |_, event, _| {
            let MultilineInputEvent::ValueChanged(value) = event;
            observed.borrow_mut().push(value.clone());
        })
    });
    cx.update_window(handle, |_, window, cx| {
        input.update(cx, |input, cx| input.project_content("first\n", window, cx));
        let state = input.read(cx).input.clone();
        state.update(cx, |state, cx| {
            state.focus(window, cx);
            state.set_selected_range(6..6, cx);
            state.replace_and_mark_text_in_range(None, "A😀中", Some(1..3), window, cx);
            assert_eq!(state.marked_text_range(window, cx), Some(6..10));
            assert_eq!(
                state.selected_text_range(false, window, cx).unwrap().range,
                7..9
            );
            state.replace_text_in_range(None, "完成", window, cx);
            assert_eq!(state.marked_text_range(window, cx), None);
        });
        window.press("ctrl-z", cx);
        assert_eq!(state.read(cx).value().as_ref(), "first\n");
        window.press("ctrl-y", cx);
        assert_eq!(state.read(cx).value().as_ref(), "first\n完成");
    })
    .unwrap();
    assert_eq!(values.borrow().last().unwrap(), "first\n完成");
    values.borrow_mut().clear();
    cx.update_window(handle, |_, window, cx| {
        input.update(cx, |input, cx| {
            input.project_content("first\n完成", window, cx)
        });
        window.press("ctrl-z", cx);
        assert_eq!(
            input.read(cx).input.read(cx).value().as_ref(),
            "first\n完成"
        );
    })
    .unwrap();
    assert!(values.borrow().is_empty());
}

#[gpui::test]
fn grapheme_navigation_selection_and_deletion_keep_kit_undo_and_direction(cx: &mut TestAppContext) {
    let (handle, input) = open(cx);
    for cluster in ["e\u{301}", "👩‍💻", "🇨🇳", "\r\n"] {
        cx.update_window(handle, |_, window, cx| {
            let text = format!("A{cluster}B");
            input.update(cx, |input, cx| {
                input.project_content(text.clone(), window, cx)
            });
            let state = input.read(cx).input.clone();
            state.update(cx, |s, cx| {
                s.focus(window, cx);
                s.set_selected_range(1..1, cx);
            });
            window.render_frame(cx);
            window.press("right", cx);
            assert_eq!(state.read(cx).cursor(), 1 + cluster.len(), "{cluster:?}");
            window.press("shift-left", cx);
            assert_eq!(state.read(cx).selected_value().as_ref(), cluster);
            assert_eq!(
                state.read(cx).cursor(),
                1,
                "reversed selection keeps its active end"
            );
            window.press("shift-right", cx);
            assert!(state.read(cx).selected_range().is_empty());
            window.press("backspace", cx);
            assert_eq!(state.read(cx).value().as_ref(), "AB");
            window.press("ctrl-z", cx);
            assert_eq!(state.read(cx).value().as_ref(), text);
            assert_eq!(state.read(cx).cursor(), 1 + cluster.len());
            window.press("left", cx);
            window.press("delete", cx);
            assert_eq!(state.read(cx).value().as_ref(), "AB");
            window.press("ctrl-z", cx);
            assert_eq!(state.read(cx).value().as_ref(), text);
            assert_eq!(state.read(cx).cursor(), 1);
        })
        .unwrap();
    }
}

#[gpui::test]
fn request_editors_keep_one_caret_for_unicode_edits(cx: &mut TestAppContext) {
    use gpui::{InputEvent as _, Modifiers, MouseDownEvent, MouseUpEvent};
    use gpui_kit::base::input::{AddCursorAbove, AddCursorBelow};

    let (handle, input) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        let state = input.read(cx).input.clone();
        for (text, action) in [
            (
                "abc\nAe\u{301}",
                Box::new(AddCursorAbove) as Box<dyn gpui::Action>,
            ),
            ("Ae\u{301}\nabc", Box::new(AddCursorBelow)),
        ] {
            input.update(cx, |input, cx| input.project_content(text, window, cx));
            let cursor = text.find('e').unwrap() + "e\u{301}".len();
            state.update(cx, |s, cx| {
                s.focus(window, cx);
                s.set_selected_range(cursor..cursor, cx);
            });
            window.render_frame(cx);
            state
                .read(cx)
                .focus_handle(cx)
                .dispatch_action(&*action, window, cx);
            window.press("backspace", cx);
            assert_eq!(
                state.read(cx).value().as_ref(),
                text.replace("e\u{301}", "")
            );
        }

        input.update(cx, |input, cx| {
            input.project_content("Ae\u{301}\nabc", window, cx)
        });
        state.update(cx, |s, cx| s.set_selected_range(4..4, cx));
        window.render_frame(cx);
        let bounds = state.read(cx).text_bounds().unwrap();
        let position = bounds.origin + point(px(4.), state.read(cx).line_height().unwrap() * 1.5);
        let modifiers = Modifiers {
            alt: true,
            ..Default::default()
        };
        window.dispatch_event(
            MouseDownEvent {
                position,
                button: MouseButton::Left,
                modifiers,
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseUpEvent {
                position,
                button: MouseButton::Left,
                modifiers,
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
        window.press("backspace", cx);
        assert_eq!(state.read(cx).value().as_ref(), "A\nabc");
    })
    .unwrap();
}

#[gpui::test]
fn kit_viewport_scrolls_to_caret_and_thumb_drag_preserves_text_and_selection(
    cx: &mut TestAppContext,
) {
    let (handle, input) = open(cx);
    let text = (0..100)
        .map(|i| format!("line-{i:03}\n"))
        .collect::<String>();
    cx.update_window(handle, |_, window, cx| {
        input.update(cx, |input, cx| {
            input.project_content(text.clone(), window, cx)
        });
        let state = input.read(cx).input.clone();
        state.update(cx, |s, cx| s.focus(window, cx));
        window.render_frame(cx);
        let id = ("input", state.entity_id());
        let viewport = window.find(id).bounds();
        // Kit paints its own thumb in the input viewport, outside the text's scrolling content.
        let start = point(viewport.right() - px(6.), viewport.top() + px(12.));
        window.drag(start, point(start.x, viewport.center().y), cx);
        assert!(
            state.read(cx).scroll_offset().y < px(0.),
            "the native thumb must scroll text"
        );
        let offset = state.read(cx).scroll_offset();
        assert_eq!(state.read(cx).selected_range(), 0..0);
        assert_eq!(state.read(cx).value().as_ref(), text);
        window.render_frame(cx);
        assert_eq!(
            state.read(cx).scroll_offset(),
            offset,
            "repaint must not undo manual scrolling"
        );
        assert_eq!(window.find(id).bounds(), viewport);

        window.press("right", cx);
        assert_eq!(
            state.read(cx).scroll_offset().y,
            px(0.),
            "caret navigation reveals the first line"
        );
        window.press(
            if cfg!(target_os = "macos") {
                "cmd-down"
            } else {
                "ctrl-end"
            },
            cx,
        );
        assert!(
            state.read(cx).scroll_offset().y < px(0.),
            "the final caret must be visible"
        );
        for value in ["", "short"] {
            input.update(cx, |input, cx| input.project_content(value, window, cx));
            window.render_frame(cx);
            window.scroll(id, ScrollDelta::Pixels(point(px(0.), px(-1000.))), cx);
            assert_eq!(
                state.read(cx).scroll_offset().y,
                px(0.),
                "fitting content has no scroll extent"
            );
            assert_eq!(window.find(id).bounds(), viewport);
        }
    })
    .unwrap();
}

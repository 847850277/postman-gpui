use super::*;
use gpui::{
    point, size, AnyWindowHandle, Bounds, EntityInputHandler, InputEvent, MouseButton,
    MouseDownEvent, MouseUpEvent, ScrollDelta, TestAppContext, WindowBounds, WindowOptions,
};
use gpui_kit::{component::input::EditorState, test::TestWindowExt};
use postman_http::HttpResponse;

fn open(
    text: impl Into<String>,
    cx: &mut TestAppContext,
) -> (
    AnyWindowHandle,
    Entity<ResponseViewer>,
    Entity<WorkspaceViewModel>,
) {
    let model = cx.new(|_| {
        let mut model = WorkspaceViewModel::new();
        model
            .active_request_mut()
            .unwrap()
            .set_url("https://httpbingo.org/anything");
        let pending = model.begin_send().unwrap();
        assert!(model.complete_send(pending, Ok(HttpResponse::success(text.into()))));
        model
    });
    let observed = model.clone();
    cx.update(|cx| {
        crate::ui::kit::init(cx);
        cx.bind_keys(setup_response_viewer_key_bindings());
    });
    let (handle, viewer) = cx
        .update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(0.), px(0.)),
                        size(px(600.), px(440.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| cx.new(|cx| ResponseViewer::new(observed, window, cx)),
            )
        })
        .unwrap();
    (handle, viewer, model)
}

fn editor(viewer: &Entity<ResponseViewer>, cx: &App) -> Entity<EditorState> {
    viewer.read(cx).body.read(cx).editor.clone()
}

#[test]
fn response_metadata_preserves_status_and_only_projects_cookie_name_and_origin() {
    assert_eq!(status_label(418), "418 Response");
    assert_eq!(
        set_cookie_name("session=secret; Path=/; HttpOnly"),
        Some("session".to_string())
    );
    assert_eq!(
        response_origin("https://httpbingo.org/cookies?source=response"),
        "https://httpbingo.org"
    );
}

#[gpui::test]
fn kit_response_selection_handles_unicode_copy_and_readonly_edit_paths(cx: &mut TestAppContext) {
    let text = "A😀中 emoji\nsecond line";
    let (handle, viewer, _) = open(text, cx);
    cx.update_window(handle, |_, window, cx| {
        // Raw has no gutter, so use Kit's byte-range geometry directly.
        viewer.update(cx, |v, cx| v.set_pretty(false, window, cx));
        window.render_frame(cx);
        let state = editor(&viewer, cx);
        let at = |offset| {
            state
                .read(cx)
                .range_to_bounds(&(offset..offset))
                .unwrap()
                .center()
        };
        window.drag(at(1), at("A😀中".len()), cx);
        assert_eq!(state.read(cx).selected_value().as_ref(), "😀中");
        window.press("cmd-c", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("😀中")
        );

        let word = state.read(cx).range_to_bounds(&(10..10)).unwrap().center();
        window.dispatch_event(
            MouseDownEvent {
                position: word,
                button: MouseButton::Left,
                click_count: 2,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseUpEvent {
                position: word,
                button: MouseButton::Left,
                click_count: 2,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        assert_eq!(state.read(cx).selected_value().as_ref(), "emoji");
        window.press("ctrl-a", cx);
        window.press("ctrl-c", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some(text)
        );

        for key in [
            "backspace",
            "delete",
            "enter",
            "ctrl-x",
            "cmd-x",
            "ctrl-v",
            "cmd-v",
            "ctrl-z",
            "ctrl-y",
        ] {
            window.press(key, cx);
            assert_eq!(state.read(cx).value().as_ref(), text, "readonly: {key}");
        }
        window.input("replacement", cx);
        state.update(cx, |s, cx| {
            s.replace_and_mark_text_in_range(None, "拼音", Some(0..2), window, cx);
            s.replace_text_in_range(None, "输入", window, cx);
        });
        assert_eq!(
            state.read(cx).value().as_ref(),
            text,
            "IME cannot mutate a response"
        );
        window.press("escape", cx);
        assert!(state.read(cx).selected_range().is_empty());
        cx.write_to_clipboard(ClipboardItem::new_string("unchanged".into()));
        window.press("cmd-c", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("unchanged"),
            "no implicit copy-current-line"
        );
        viewer.update(cx, |v, cx| v.copy_raw_response_body(cx));
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some(text)
        );
    })
    .unwrap();
}

#[gpui::test]
fn kit_response_scrollbars_stay_in_viewport_and_raw_wraps_without_horizontal_overflow(
    cx: &mut TestAppContext,
) {
    let text = (0..100)
        .map(|i| format!("line-{i:03} 中😀 {}", "wide".repeat(100)))
        .collect::<Vec<_>>()
        .join("\n");
    let (handle, viewer, _) = open(text.clone(), cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let state = editor(&viewer, cx);
        let id = ("input", state.entity_id());
        let viewport = window.find(id).bounds();
        let start = point(viewport.right() - px(6.), viewport.top() + px(12.));
        window.drag(start, point(start.x, viewport.center().y), cx);
        assert!(
            state.read(cx).scroll_offset().y < px(0.),
            "Kit's viewport thumb scrolls the document"
        );
        assert_eq!(
            state.read(cx).selected_range(),
            0..0,
            "scrollbar drag must not select text"
        );
        window.scroll(id, ScrollDelta::Pixels(point(px(-400.), px(0.))), cx);
        assert!(
            state.read(cx).scroll_offset().x < px(0.),
            "Pretty keeps long lines horizontally scrollable"
        );
        let offset = state.read(cx).scroll_offset();
        window.render_frame(cx);
        assert_eq!(state.read(cx).scroll_offset(), offset);
        assert_eq!(window.find(id).bounds(), viewport);

        viewer.update(cx, |v, cx| v.set_pretty(false, window, cx));
        window.render_frame(cx);
        window.scroll(id, ScrollDelta::Pixels(point(px(-400.), px(-100.))), cx);
        assert_eq!(
            state.read(cx).scroll_offset().x,
            px(0.),
            "Raw wraps like the prototype"
        );
        assert!(state.read(cx).scroll_offset().y < px(0.));
        assert_eq!(
            state.read(cx).value().as_ref(),
            text,
            "wrapping leaves raw bytes unchanged"
        );
        let raw_viewport = window.find(id).bounds();
        assert_eq!(raw_viewport.right(), viewport.right());
        assert_eq!(raw_viewport.bottom(), viewport.bottom());

        for pretty in [true, false] {
            viewer.read(cx).body.clone().update(cx, |body, cx| {
                body.project(
                    &ResponseState::Success {
                        status: 200,
                        body: "short".into(),
                        headers: Vec::new(),
                        elapsed_ms: 1,
                    },
                    pretty,
                    window,
                    cx,
                )
            });
            window.render_frame(cx);
            window.scroll(id, ScrollDelta::Pixels(point(px(-400.), px(-400.))), cx);
            assert_eq!(
                state.read(cx).scroll_offset(),
                point(px(0.), px(0.)),
                "short content has no phantom scrollbars"
            );
        }
    })
    .unwrap();
}

#[gpui::test]
fn response_document_survives_theme_model_updates_and_request_switches(cx: &mut TestAppContext) {
    let raw = serde_json::to_string(
        &(0..100)
            .map(|i| format!("item-{i:03} 中😀"))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let (handle, viewer, model) = open(raw.clone(), cx);
    let state = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let state = editor(&viewer, cx);
            window.click(("input", state.entity_id()), cx);
            window.press("cmd-a", cx);
            window.scroll(
                ("input", state.entity_id()),
                ScrollDelta::Pixels(point(px(0.), px(-300.))),
                cx,
            );
            assert!(state.read(cx).scroll_offset().y < px(0.));
            state
        })
        .unwrap();
    let (offset, range) = state.read_with(cx, |s, _| (s.scroll_offset(), s.selected_range()));
    model.update(cx, |_, cx| cx.notify());
    cx.update(|cx| crate::ui::theme::apply(gpui_kit::component::ThemeMode::Dark, cx));
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(state.read(cx).scroll_offset(), offset);
        assert_eq!(state.read(cx).selected_range(), range);
    })
    .unwrap();
    model.update(cx, |m, cx| {
        m.new_request();
        cx.notify();
    });
    cx.run_until_parked();
    assert_ne!(
        viewer.read_with(cx, |v, cx| v.body.read(cx).editor.entity_id()),
        state.entity_id()
    );
    model.update(cx, |m, cx| {
        assert!(m.select_tab(0));
        cx.notify();
    });
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(editor(&viewer, cx), state);
        assert_eq!(state.read(cx).selected_range(), range);
        assert_eq!(state.read(cx).scroll_offset(), offset);
        viewer.update(cx, |v, cx| v.set_pretty(false, window, cx));
        assert_eq!(state.read(cx).value().as_ref(), raw);
    })
    .unwrap();
    model.update(cx, |m, cx| {
        assert!(m.select_tab(1));
        cx.notify();
    });
    cx.run_until_parked();
    model.update(cx, |m, cx| {
        assert!(m.close_tab(0));
        cx.notify();
    });
    cx.run_until_parked();
    assert!(
        viewer.read_with(cx, |v, _| v.retained.is_empty()),
        "closed background requests release their retained editor"
    );
}

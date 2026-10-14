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
    cx.run_until_parked();
    (handle, viewer, model)
}

fn set_pretty(
    handle: AnyWindowHandle,
    viewer: &Entity<ResponseViewer>,
    pretty: bool,
    cx: &mut TestAppContext,
) {
    cx.update_window(handle, |_, window, cx| {
        viewer.update(cx, |v, cx| v.set_pretty(pretty, window, cx))
    })
    .unwrap();
    cx.run_until_parked();
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
    // Raw has no gutter, so use Kit's byte-range geometry directly.
    set_pretty(handle, &viewer, false, cx);
    cx.update_window(handle, |_, window, cx| {
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
    let viewport = cx
        .update_window(handle, |_, window, cx| {
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

            viewport
        })
        .unwrap();
    set_pretty(handle, &viewer, false, cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let state = editor(&viewer, cx);
        let id = ("input", state.entity_id());
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
    })
    .unwrap();
    for pretty in [true, false] {
        cx.update_window(handle, |_, window, cx| {
            viewer.read(cx).body.clone().update(cx, |body, cx| {
                body.project(
                    Arc::new(ResponseState::Success {
                        status: 200,
                        body: "short".into(),
                        headers: Vec::new(),
                        elapsed_ms: 1,
                    }),
                    pretty,
                    window,
                    cx,
                )
            });
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            let state = editor(&viewer, cx);
            let id = ("input", state.entity_id());
            window.render_frame(cx);
            window.scroll(id, ScrollDelta::Pixels(point(px(-400.), px(-400.))), cx);
            assert_eq!(
                state.read(cx).scroll_offset(),
                point(px(0.), px(0.)),
                "short content has no phantom scrollbars"
            );
        })
        .unwrap();
    }
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
    })
    .unwrap();
    set_pretty(handle, &viewer, false, cx);
    state.read_with(cx, |state, _| assert_eq!(state.value().as_ref(), raw));
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

#[gpui::test]
fn large_response_is_shared_and_draft_edits_do_not_notify_or_reset_the_viewer(
    cx: &mut TestAppContext,
) {
    use std::{cell::Cell, rc::Rc};
    // More than 1 MiB, with bounded line width so this exercises document preparation rather
    // than an artificial single-line shaping limit in the native text backend.
    let raw = serde_json::to_string(&vec!["中😀 payload ".repeat(40); 2048]).unwrap();
    assert!(raw.len() > 1024 * 1024);
    let (handle, viewer, model) = open(raw.clone(), cx);
    let snapshot = model.read_with(cx, |m, _| m.active_request().unwrap().response_snapshot());
    let state = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(Arc::ptr_eq(&snapshot, &viewer.read(cx).response));
            let state = editor(&viewer, cx);
            let formatted = state.read(cx).value();
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&formatted).unwrap(),
                serde_json::from_str::<serde_json::Value>(&raw).unwrap()
            );
            window.click(("input", state.entity_id()), cx);
            window.press("cmd-a", cx);
            window.scroll(
                ("input", state.entity_id()),
                ScrollDelta::Pixels(point(px(0.), px(-300.))),
                cx,
            );
            state
        })
        .unwrap();
    let (text, selection, scroll) = state.read_with(cx, |s, _| {
        (s.value(), s.selected_range(), s.scroll_offset())
    });
    let notifications = Rc::new(Cell::new(0));
    let count = notifications.clone();
    let _subscription = cx.update(|cx| cx.observe(&viewer, move |_, _| count.set(count.get() + 1)));
    let body_notifications = Rc::new(Cell::new(0));
    let body_count = body_notifications.clone();
    let _body_subscription = cx.update(|cx| {
        cx.observe(&viewer.read(cx).body.clone(), move |_, _| {
            body_count.set(body_count.get() + 1)
        })
    });
    for page in 0..20 {
        model.update(cx, |m, cx| {
            let request = m.active_request_mut().unwrap();
            request.set_url(format!("https://httpbingo.org/anything?page={page}"));
            request.set_body(format!("draft {page}"));
            cx.notify();
        });
        cx.run_until_parked();
    }
    assert_eq!(
        notifications.get(),
        0,
        "draft notifications must not invalidate the response viewer"
    );
    assert!(model.read_with(cx, |m, _| Arc::ptr_eq(
        &snapshot,
        &m.active_request().unwrap().response_snapshot()
    )));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let input = state.read(cx);
        assert_eq!(input.selected_range(), selection);
        assert_eq!(input.scroll_offset(), scroll);
        assert_eq!(input.value(), text);
        assert_eq!(
            body_notifications.get(),
            0,
            "the retained document must not be rebuilt"
        );
        viewer.update(cx, |v, cx| v.copy_raw_response_body(cx));
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some(raw.as_str())
        );
    })
    .unwrap();

    // Equal-size Cookie changes still update response evidence, without replacing its body.
    for name in ["first", "replacement"] {
        model.update(cx, |m, cx| {
            m.sync_cookie_jar(vec![("https://httpbingo.org".into(), name.into())]);
            cx.notify();
        });
        cx.run_until_parked();
    }
    assert!(notifications.get() >= 2);
    assert_eq!(state.read_with(cx, |s, _| s.value()), text);
    assert_eq!(body_notifications.get(), 0);
}

#[gpui::test]
fn pending_formatting_stays_with_its_tab_and_closed_tabs_release_the_editor(
    cx: &mut TestAppContext,
) {
    let (handle, viewer, model) = open("previous response", cx);
    let previous = model.read_with(cx, |m, _| m.active_tab_id().unwrap());
    cx.update_window(handle, |_, window, cx| {
        // Queue work without yielding the foreground. Native controls remain operable while
        // the document is prepared; multiple mode changes must resolve to the last selection.
        window.render_frame(cx);
        window.click("response-raw", cx);
        window.click("response-pretty", cx);
        window.click("response-raw", cx);
        model.update(cx, |m, cx| {
            m.new_request();
            m.active_request_mut()
                .unwrap()
                .set_url("https://httpbingo.org/get");
            let pending = m.begin_send().unwrap();
            m.complete_send(
                pending,
                Ok(HttpResponse::success(r#"{"tab":"second"}"#.into())),
            );
            cx.notify();
        });
    })
    .unwrap();
    cx.run_until_parked();
    let second_editor = viewer.read_with(cx, |v, cx| v.body.read(cx).editor.clone());
    assert!(second_editor.read_with(cx, |s, _| s.value().contains("second")));
    model.update(cx, |m, cx| {
        m.select_tab_by_id(previous);
        cx.notify();
    });
    cx.run_until_parked();
    let closed_editor = viewer.read_with(cx, |v, cx| v.body.read(cx).editor.downgrade());
    assert!(viewer.read_with(cx, |v, cx| !v.pretty
        && v.body.read(cx).editor.read(cx).value().as_ref() == "previous response"));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("response-pretty", cx);
        model.update(cx, |m, cx| {
            m.close_tab_by_id(previous);
            cx.notify();
        });
    })
    .unwrap();
    cx.run_until_parked();
    assert!(closed_editor.upgrade().is_none());
    assert!(viewer.read_with(cx, |v, _| v.retained.is_empty()));
    assert!(second_editor.read_with(cx, |s, _| s.value().contains("second")));
}

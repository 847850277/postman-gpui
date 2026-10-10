//! Native interactions for P3 response presentation and Kit split panes.
#[path = "common/ui.rs"]
mod ui;
use gpui::{
    point, px, size, AppContext, Modifiers, MouseButton, TestAppContext, VisualTestContext,
};
use postman_gpui::app::{PostmanApp, WorkspaceViewModel};
use ui::click;

fn resize(cx: &mut VisualTestContext, width: f32, height: f32) {
    let handle = cx.update(|window, _| window.window_handle());
    cx.simulate_window_resize(handle, size(px(width), px(height)));
    cx.run_until_parked();
    // Kit measures constraints, then the owner applies its preferred ratio on the settling frame.
    for _ in 0..3 {
        let _ = cx.debug_bounds("http-split");
    }
}
fn share(cx: &mut VisualTestContext, stacked: bool) -> f32 {
    let request = cx.debug_bounds("request-container").unwrap();
    let response = cx.debug_bounds("response-container").unwrap();
    let split = cx.debug_bounds("http-split").unwrap();
    if stacked {
        assert!(
            (request.left() - response.left()).abs() <= px(1.),
            "{request:?} {response:?}"
        );
        assert!(
            (request.bottom() - response.top()).abs() <= px(1.),
            "{request:?} {response:?}"
        );
        request.size.height.as_f32() / split.size.height.as_f32()
    } else {
        assert!(
            (request.top() - response.top()).abs() <= px(1.),
            "{request:?} {response:?}"
        );
        assert!(
            (request.right() - response.left()).abs() <= px(1.),
            "{request:?} {response:?}"
        );
        request.size.width.as_f32() / split.size.width.as_f32()
    }
}
fn drag(cx: &mut VisualTestContext, dx: f32, dy: f32, cancel: bool) {
    let start = cx.debug_bounds("response-resize-handle").unwrap().center();
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        start + point(px(dx.signum() * 6.), px(dy.signum() * 6.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    cx.simulate_mouse_move(
        start + point(px(dx), px(dy)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    if cancel {
        ui::press(cx, "escape");
    }
    cx.simulate_mouse_up(
        start + point(px(dx), px(dy)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
}

#[gpui::test]
fn response_scrollbars_stay_at_viewport_edges_after_scrolling_and_resizing(
    cx: &mut TestAppContext,
) {
    use postman_gpui::models::{HistoricalResponse, HistoryEntry, HttpMethod, Request};
    let body =
        serde_json::to_string(&(0..80).map(|n| format!("item {n}")).collect::<Vec<_>>()).unwrap();
    let headers = (0..50)
        .map(|n| (format!("x-header-{n}"), format!("value {n}")))
        .collect();
    let entry = HistoryEntry::completed(
        Request::new(HttpMethod::GET, "https://example.test/scroll"),
        "Scrollable response".into(),
        200,
        2,
        body.len(),
    )
    .with_historical_response(HistoricalResponse::completed(200, headers, body, 2));
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    click(cx, "nav-http").unwrap();
    resize(cx, 1440., 960.);
    click(cx, "rail-history").unwrap();
    click(cx, "response-layout-toggle").unwrap();
    model.update(cx, |model, cx| {
        assert!(model.load_history_entry(&entry));
        cx.notify();
    });
    for (pane, viewport, first_item) in [
        (
            "response-pane-body",
            "response-content",
            "response-document",
        ),
        (
            "response-pane-headers",
            "response-headers-rows",
            "response-header-row-0",
        ),
    ] {
        click(cx, pane).unwrap();
        let initial = cx.debug_bounds(first_item).unwrap();
        ui::scroll_down(cx, viewport, 100.).unwrap();
        assert!(cx.debug_bounds(first_item).unwrap().top() < initial.top());
        let assert_edges = |cx: &mut VisualTestContext| {
            let viewport = cx.debug_bounds(viewport).unwrap();
            let overlay = cx.debug_bounds("scrollbar-overlay").unwrap();
            assert!(
                (overlay.left() - viewport.left()).abs() <= px(1.)
                    && (overlay.top() - viewport.top()).abs() <= px(1.)
                    && (overlay.right() - viewport.right()).abs() <= px(1.)
                    && (overlay.bottom() - viewport.bottom()).abs() <= px(1.),
                "{pane}: scrollbar must stay at the viewport edge: {overlay:?} {viewport:?}"
            );
        };
        assert_edges(cx);
        for dy in [-80., 160.] {
            drag(cx, 0., dy, false);
            assert_edges(cx);
        }
        let before_horizontal = cx.debug_bounds(first_item).unwrap();
        let position = cx.debug_bounds(viewport).unwrap().center();
        cx.simulate_event(gpui::ScrollWheelEvent {
            position,
            delta: gpui::ScrollDelta::Pixels(point(px(-80.), px(0.))),
            ..Default::default()
        });
        assert_eq!(
            cx.debug_bounds(first_item).unwrap().left(),
            before_horizontal.left(),
            "short content must not have phantom horizontal overflow"
        );
        assert_edges(cx);
        click(cx, "response-layout-toggle").unwrap();
        drag(cx, 100., 0., false);
        assert_edges(cx);
        ui::scroll_down(cx, viewport, 75.).unwrap();
        assert_edges(cx);
        resize(cx, 1024., 768.);
        assert_edges(cx);
        resize(cx, 1440., 960.);
        assert_edges(cx);
        click(cx, "response-layout-toggle").unwrap();
    }
}

#[gpui::test]
fn split_uses_editor_width_restores_preferences_and_keeps_controls_inside_window(
    cx: &mut TestAppContext,
) {
    let model = cx.new(|_| WorkspaceViewModel::new());
    let (_, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(model, window, cx)
        })
    });
    click(cx, "nav-http").unwrap();
    for mode in [
        gpui_kit::component::ThemeMode::Light,
        gpui_kit::component::ThemeMode::Dark,
    ] {
        cx.update(|_, cx| postman_gpui::ui::theme::apply(mode, cx));
        for scale in [1., 2.] {
            let handle = cx.update(|window, _| window.window_handle());
            cx.simulate_window_scale_factor_change(handle, scale);
            for (w, h) in [(1440., 960.), (1920., 1080.), (1024., 768.), (960., 640.)] {
                resize(cx, w, h);
                let stacked = w - 72. < 900.;
                let _ = share(cx, stacked);
                let split = cx.debug_bounds("http-split").unwrap();
                assert!(split.right() <= px(w), "{split:?}");
                for id in [
                    "send-button",
                    "response-layout-toggle",
                    "response-panel-sizes",
                    "response-footer",
                ] {
                    let bounds = cx.debug_bounds(id).unwrap();
                    assert!(bounds.right() <= px(w), "{id}: {bounds:?}");
                    assert!(bounds.bottom() <= px(h), "{id}: {bounds:?}");
                }
            }
        }
    }
    resize(cx, 1440., 960.);
    assert!(
        (share(cx, false) - 0.46).abs() < 0.01,
        "initial share {}",
        share(cx, false)
    );
    drag(cx, 120., 0., false);
    let preferred = share(cx, false);
    assert!(preferred > 0.5, "dragged share {preferred}");
    resize(cx, 1024., 768.);
    click(cx, "rail-history").unwrap();
    assert!(share(cx, true) > 0.);
    click(cx, "rail-history").unwrap();
    let _ = share(cx, false);
    resize(cx, 1440., 960.);
    assert!((share(cx, false) - preferred).abs() < 0.01);
    click(cx, "response-layout-toggle").unwrap();
    resize(cx, 1440., 960.);
    let stacked = share(cx, true);
    drag(cx, 0., 50., false);
    assert!(share(cx, true) > stacked);
    let vertical = share(cx, true);
    resize(cx, 1920., 1080.);
    assert!(
        (share(cx, true) - vertical).abs() < 0.01,
        "explicit stack persists"
    );
    click(cx, "response-layout-toggle").unwrap();
    assert!((share(cx, false) - preferred).abs() < 0.01);
}
#[gpui::test]
fn divider_keyboard_escape_reset_and_numeric_controls_use_the_same_split(cx: &mut TestAppContext) {
    let model = cx.new(|_| WorkspaceViewModel::new());
    let (_, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(model, window, cx)
        })
    });
    click(cx, "nav-http").unwrap();
    resize(cx, 1440., 960.);
    click(cx, "response-resize-handle").unwrap();
    let initial = share(cx, false);
    ui::press(cx, "right");
    assert!((share(cx, false) - initial - 0.02).abs() < 0.01);
    ui::press(cx, "shift-right");
    assert!((share(cx, false) - initial - 0.12).abs() < 0.01);
    let before = share(cx, false);
    drag(cx, -180., 0., true);
    assert!(
        (share(cx, false) - before).abs() < 0.01,
        "Escape restores the pre-drag preference"
    );
    ui::press(cx, "home");
    assert!((cx.debug_bounds("request-container").unwrap().size.width - px(360.)).abs() < px(1.));
    ui::press(cx, "end");
    assert!((cx.debug_bounds("response-container").unwrap().size.width - px(360.)).abs() < px(1.));
    let handle = cx.debug_bounds("response-resize-handle").unwrap().center();
    cx.simulate_event(gpui::MouseDownEvent {
        position: handle,
        button: MouseButton::Left,
        click_count: 2,
        ..Default::default()
    });
    cx.simulate_mouse_up(handle, MouseButton::Left, Modifiers::none());
    assert!(
        (share(cx, false) - 0.46).abs() < 0.01,
        "double click resets current arrangement: {}",
        share(cx, false)
    );
    ui::press(cx, "enter");
    use gpui_kit::test::TestWindowExt;
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("http-apply-sizes").is_some());
    });
    ui::replace_text(cx, "http-size-input", "62").unwrap();
    cx.update(|window, cx| window.click("http-apply-sizes", cx));
    assert!(
        (share(cx, false) - 0.62).abs() < 0.01,
        "numeric Apply sets the actual split"
    );
    click(cx, "response-panel-sizes").unwrap();
    ui::replace_text(cx, "http-size-input", "55").unwrap();
    ui::press(cx, "enter");
    assert!(
        (share(cx, false) - 0.55).abs() < 0.01,
        "Enter applies the numeric value"
    );
    click(cx, "response-panel-sizes").unwrap();
    cx.update(|window, cx| window.click("http-reset-sizes", cx));
    assert!((share(cx, false) - 0.46).abs() < 0.01);
    ui::press(cx, "escape");
    ui::press(cx, "left");
    assert!(
        (share(cx, false) - 0.44).abs() < 0.01,
        "dialog Escape returns focus to divider"
    );
}

#[gpui::test]
async fn pretty_raw_long_content_and_per_tab_view_state_preserve_the_received_response(
    cx: &mut TestAppContext,
) {
    use postman_gpui::app::ResponseState;
    let mut server = mockito::Server::new_async().await;
    let raw = format!(
        "{{\"message\":\"中😀{}\",\"value\":42}}",
        "long".repeat(300)
    );
    let received = server
        .mock("GET", "/response")
        .with_status(422)
        .with_header("content-type", "application/json")
        .with_header("x-repeated", "first")
        .with_header("x-repeated", "second")
        .with_body(&raw)
        .expect(1)
        .create_async()
        .await;
    let url = format!("{}/response", server.url());
    let model = cx.new(|_| {
        let mut m = WorkspaceViewModel::new();
        m.active_request_mut().unwrap().set_url(url);
        m
    });
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    click(cx, "nav-http").unwrap();
    resize(cx, 1440., 960.);
    click(cx, "send-button").unwrap();
    cx.run_until_parked();
    for _ in 0..300 {
        if model.read_with(cx, |m, _| {
            matches!(
                m.active_request().unwrap().response(),
                ResponseState::Success { .. }
            )
        }) {
            break;
        }
        cx.background_executor
            .timer(std::time::Duration::from_millis(10))
            .await;
        cx.run_until_parked();
    }
    assert!(cx.debug_bounds("response-status-422").is_some());
    click(cx, "response-content").unwrap();
    cx.simulate_keystrokes("cmd-a cmd-c");
    let pretty = cx.read_from_clipboard().unwrap().text().unwrap();
    assert!(pretty.contains('\n'));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&pretty).unwrap(),
        serde_json::from_str::<serde_json::Value>(&raw).unwrap()
    );
    click(cx, "response-copy-button").unwrap();
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().as_deref(),
        Some(raw.as_str())
    );
    click(cx, "response-raw").unwrap();
    click(cx, "response-content").unwrap();
    cx.simulate_keystrokes("cmd-a cmd-c");
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().as_deref(),
        Some(raw.as_str())
    );
    let viewport = cx.debug_bounds("response-content").unwrap();
    let document = cx.debug_bounds("response-document").unwrap();
    assert!(
        document.size.width > viewport.size.width,
        "long lines must scroll, not resize the window: {document:?} {viewport:?}"
    );
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: viewport.center(),
        delta: gpui::ScrollDelta::Pixels(point(px(-400.), px(0.))),
        ..Default::default()
    });
    let scrolled = cx.debug_bounds("response-document").unwrap();
    assert!(scrolled.left() < document.left());
    click(cx, "nav-home").unwrap();
    click(cx, "nav-http").unwrap();
    assert_eq!(
        cx.debug_bounds("response-document").unwrap().left(),
        scrolled.left()
    );
    click(cx, "new-tab-button").unwrap();
    click(cx, "request-tab-0").unwrap();
    click(cx, "response-content").unwrap();
    cx.simulate_keystrokes("cmd-a cmd-c");
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().as_deref(),
        Some(raw.as_str()),
        "raw preference follows its request"
    );
    click(cx, "response-pane-headers").unwrap();
    assert!(cx.debug_bounds("response-headers-table").is_some());
    let (body, headers, history) = model.read_with(cx, |m, _| {
        let ResponseState::Success { body, headers, .. } = m.active_request().unwrap().response()
        else {
            panic!("completed response");
        };
        (body.clone(), headers.clone(), m.history().len())
    });
    assert_eq!(body, raw);
    assert_eq!(
        headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("x-repeated"))
            .count(),
        2
    );
    assert_eq!(history, 1);
    received.assert_async().await;
}

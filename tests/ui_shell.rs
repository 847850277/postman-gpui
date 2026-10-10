//! P2 through the production composition root: routes, retained drafts/tasks, native controls.
#[path = "common/ui.rs"]
mod ui;
use gpui::{px, size, AppContext, TestAppContext};
use gpui_kit::{
    component::{Theme, ThemeMode},
    test::TestWindowExt,
};
use postman_gpui::{
    app::{PostmanApp, ResponseState, WorkspaceViewModel},
    models::HttpMethod,
    ui::theme,
};
use ui::{click, replace_text};

#[gpui::test]
fn home_cards_and_navigation_support_native_keyboard_and_shared_theme(cx: &mut TestAppContext) {
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    assert!(cx.debug_bounds("home-screen").is_some());
    assert!(cx.debug_bounds("home-empty-requests").is_some());
    assert!(cx.debug_bounds("url-input").is_none());
    assert_eq!(cx.update(|_, cx| Theme::global(cx).mode), ThemeMode::Light);
    click(cx, "home-open-http").unwrap();
    assert!(cx.debug_bounds("url-input").is_some());
    assert!(cx.debug_bounds("history-panel").is_none());
    click(cx, "nav-home").unwrap();
    click(cx, "home-open-flows").unwrap();
    assert!(cx.debug_bounds("flows-screen").is_some());
    // Traverse the visible shell to each Home card using real Tab and key-up activation.
    for (stops, key, destination) in [(7, "enter", "url-input"), (8, "space", "flows-screen")] {
        click(cx, "nav-home").unwrap();
        cx.update(|window, cx| {
            for _ in 0..stops {
                window.focus_next(cx);
            }
            window.press(key, cx);
        });
        assert!(cx.debug_bounds(destination).is_some());
    }
    // Native button keyboard clicks need key-up, unlike the legacy custom action controls.
    click(cx, "nav-home").unwrap();
    cx.update(|window, cx| {
        window.focus_next(cx); // Search
        window.focus_next(cx); // Home
        window.press("enter", cx);
    });
    assert!(cx.debug_bounds("home-screen").is_some());
    cx.update(|window, cx| {
        window.focus_next(cx); // Search (navigation focuses the visible root)
        window.focus_next(cx); // Home
        window.focus_next(cx); // HTTP
        window.press("space", cx);
    });
    assert!(cx.debug_bounds("url-input").is_some());
    click(cx, "appearance-toggle").unwrap();
    assert_eq!(cx.update(|_, cx| Theme::global(cx).mode), ThemeMode::Dark);
    for route in ["nav-flows", "nav-home", "nav-http"] {
        click(cx, route).unwrap();
        assert_eq!(cx.update(|_, cx| Theme::global(cx).mode), ThemeMode::Dark);
    }
}

#[gpui::test]
fn hidden_http_does_not_accept_input_or_commands_and_recent_rows_reuse_tabs(
    cx: &mut TestAppContext,
) {
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    click(cx, "home-open-http").unwrap();
    replace_text(cx, "url-input", "https://example.test/first").unwrap();
    ui::choose_method(cx, "POST").unwrap();
    click(cx, "request-pane-body").unwrap();
    ui::choose_body_kind(cx, "body-kind-json").unwrap();
    replace_text(cx, "body-input", r#"{"name":"draft"}"#).unwrap();
    let (first_id, draft) = model.read_with(cx, |m, _| {
        (
            m.active_tab_id().unwrap(),
            m.active_request().unwrap().request_draft().clone(),
        )
    });
    cx.simulate_keystrokes("ctrl-t");
    replace_text(cx, "url-input", "https://example.test/second").unwrap();
    let second_id = model.read_with(cx, |m, _| m.active_tab_id().unwrap());
    for route in ["nav-home", "nav-flows"] {
        click(cx, route).unwrap();
        cx.simulate_input("SHOULD NOT REACH URL");
        cx.simulate_keystrokes("ctrl-enter ctrl-w ctrl-l ctrl-tab ctrl-shift-tab");
        model.read_with(cx, |m, _| {
            assert_eq!(m.tab_count(), 2);
            assert_eq!(m.active_tab_id(), Some(second_id));
            assert_eq!(
                m.active_request().unwrap().url(),
                "https://example.test/second"
            );
            assert!(matches!(
                m.active_request().unwrap().response(),
                ResponseState::NotSent
            ));
            assert_eq!(m.request_for_tab(first_id).unwrap().request_draft(), &draft);
        });
    }
    click(cx, "nav-home").unwrap();
    let selector: &'static str = Box::leak(format!("recent-request-{first_id}").into_boxed_str());
    click(cx, selector).unwrap();
    model.read_with(cx, |m, _| {
        assert_eq!(m.active_tab_id(), Some(first_id));
        assert_eq!(m.tab_count(), 2);
        assert_eq!(m.active_request().unwrap().request_draft(), &draft);
    });
    assert!(cx.debug_bounds("body-input").is_some());
    cx.simulate_keystrokes("ctrl-w");
    click(cx, "nav-home").unwrap();
    assert!(cx.debug_bounds(selector).is_none());
    assert_eq!(
        model.read_with(cx, |m, _| m
            .recent_requests()
            .map(|r| r.tab_id())
            .collect::<Vec<_>>()),
        vec![second_id]
    );
    // Supporting entry points work from Home and intentionally navigate to HTTP.
    cx.simulate_keystrokes("ctrl-shift-f");
    assert!(cx.debug_bounds("history-panel").is_some());
}

#[gpui::test]
fn completion_while_away_targets_original_tab_and_persists_once(cx: &mut TestAppContext) {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        time::Duration,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/delayed", listener.local_addr().unwrap());
    let (release, gate) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut bytes = [0; 4096];
        let count = socket.read(&mut bytes).unwrap();
        assert!(String::from_utf8_lossy(&bytes[..count]).starts_with("GET /delayed HTTP/1.1"));
        gate.recv_timeout(Duration::from_secs(10)).unwrap();
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nkept-ok")
            .unwrap();
    });
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    click(cx, "nav-http").unwrap();
    replace_text(cx, "url-input", &url).unwrap();
    let first = model.read_with(cx, |m, _| m.active_tab_id().unwrap());
    ui::click_without_wait(cx, "send-button").unwrap();
    ui::click_without_wait(cx, "new-tab-button").unwrap();
    let second = model.read_with(cx, |m, _| m.active_tab_id().unwrap());
    assert_ne!(first, second);
    ui::click_without_wait(cx, "nav-home").unwrap();
    assert!(cx.debug_bounds("home-screen").is_some());
    assert!(model.read_with(cx, |m, _| m.request_for_tab(first).unwrap().is_sending()));
    ui::click_without_wait(cx, "nav-flows").unwrap();
    assert!(cx.debug_bounds("flows-screen").is_some());
    release.send(()).unwrap();
    server.join().unwrap();
    cx.run_until_parked();
    model.read_with(cx, |m, _| {
        assert!(matches!(m.request_for_tab(first).unwrap().response(), ResponseState::Success {status:200, body, ..} if body == "kept-ok"));
        assert_eq!(m.history_len(), 1);
        assert_eq!(m.active_tab_id(), Some(second));
        assert!(matches!(m.request_for_tab(second).unwrap().response(), ResponseState::NotSent));
    });
    click(cx, "nav-http").unwrap();
    click(cx, "request-tab-0").unwrap();
    assert!(cx.debug_bounds("response-container").is_some());
    for route in ["nav-home", "nav-flows", "nav-http"] {
        click(cx, route).unwrap();
    }
    assert_eq!(model.read_with(cx, |m, _| m.history_len()), 1);
}

#[gpui::test]
fn shell_geometry_in_both_themes_at_reference_and_minimum_sizes(cx: &mut TestAppContext) {
    let model = cx.new(|_| WorkspaceViewModel::new());
    let (_, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(model, window, cx)
        })
    });
    for scale in [1., 2.] {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            for (width, height) in [(1440., 960.), (1920., 1080.), (1024., 768.), (960., 640.)] {
                let handle = cx.update(|window, cx| {
                    theme::apply(mode, cx);
                    window.window_handle()
                });
                cx.simulate_window_scale_factor_change(handle, scale);
                cx.simulate_window_resize(handle, size(px(width), px(height)));
                let root = cx.debug_bounds("main-container").unwrap();
                assert_eq!(root.size, size(px(width), px(height)));
                let header = cx.debug_bounds("top-header").unwrap();
                let rail = cx.debug_bounds("left-rail").unwrap();
                let footer = cx.debug_bounds("status-bar").unwrap();
                let home = cx.debug_bounds("home-screen").unwrap();
                let http = cx.debug_bounds("home-open-http").unwrap();
                let flows = cx.debug_bounds("home-open-flows").unwrap();
                assert!((f32::from(header.size.height) - 52.).abs() <= 0.5);
                assert!((f32::from(rail.size.width) - 72.).abs() <= 0.5);
                assert!((f32::from(footer.size.height) - 30.).abs() <= 0.5);
                assert_eq!(home.top(), header.bottom());
                assert_eq!(home.bottom(), footer.top());
                assert_eq!(footer.bottom(), root.bottom());
                assert!(http.left() > home.left() && http.right() < home.right());
                assert!(flows.left() > home.left() && flows.right() < home.right());
                if width >= 1440. {
                    assert_eq!(http.top(), flows.top());
                    assert!(http.right() < flows.left());
                } else {
                    assert!(http.bottom() < flows.top());
                }
                click(cx, "nav-http").unwrap();
                for control in [
                    "nav-home",
                    "nav-http",
                    "nav-flows",
                    "rail-new-request",
                    "rail-history",
                    "rail-search",
                    "cookie-jar-trigger",
                    "appearance-toggle",
                    "shortcut-help-button",
                ] {
                    let bounds = cx.debug_bounds(control).unwrap();
                    assert!(bounds.top() >= rail.top() && bounds.bottom() <= rail.bottom(),
                        "{control} must remain inside the rail at {width}x{height}: {bounds:?} vs {rail:?}");
                    assert!(
                        bounds.size.height >= px(28.),
                        "{control} must keep a usable hit target"
                    );
                }
                click(cx, "nav-home").unwrap();
            }
        }
    }
}

#[test]
fn recents_are_session_drafts_in_deterministic_mru_order() {
    let mut m = WorkspaceViewModel::new();
    assert_eq!(m.recent_requests().count(), 0);
    m.active_request_mut()
        .unwrap()
        .set_url("https://example.test/a");
    let first = m.active_tab_id().unwrap();
    m.new_request();
    m.active_request_mut().unwrap().set_method(HttpMethod::POST);
    let second = m.active_tab_id().unwrap();
    assert_eq!(
        m.recent_requests().map(|r| r.tab_id()).collect::<Vec<_>>(),
        vec![second, first]
    );
    m.select_tab_by_id(first);
    assert_eq!(
        m.recent_requests().map(|r| r.tab_id()).collect::<Vec<_>>(),
        vec![first, second]
    );
    m.close_tab_by_id(first);
    assert_eq!(
        m.recent_requests().map(|r| r.tab_id()).collect::<Vec<_>>(),
        vec![second]
    );
}

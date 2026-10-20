//! Exercise real Kit overlays against the production shell and authoritative request model.
#[path = "common/ui.rs"]
mod ui;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, TestAppContext, VisualTestContext};
use postman_gpui::{
    app::{PostmanApp, ResponseState, WorkspaceViewModel},
    models::{HttpMethod, RedirectPolicy, RequestBody},
};
fn text(cx: &mut VisualTestContext) -> String {
    cx.update(|w, cx| {
        w.render_frame(cx);
        gpui_kit::base::test_support::snapshots(w)
            .into_iter()
            .find(|v| matches!(v.label(), Some("cURL command" | "Generated cURL command")))
            .expect("Kit textarea exposes its native accessibility label")
            .value()
            .unwrap_or("")
            .to_string()
    })
}

#[gpui_kit::test]
fn import_preview_validation_cancel_and_latest_text_create_one_dirty_tab(cx: &mut TestAppContext) {
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| PostmanApp::with_view_model(observed, w, cx))
    });
    ui::open_http(cx);
    cx.run_until_parked();
    ui::replace_text(cx, "url-input", "https://original.test/keep").unwrap();
    ui::click(cx, "rail-import").unwrap();
    ui::click(cx, "curl-submit").unwrap();
    assert!(cx.debug_bounds("curl-import-dialog").is_some());
    cx.simulate_input(
        "curl 'https://imported.test/users?x=%2f&x=two' --json '{\"name\":\"用户\"}'",
    );
    assert!(cx.debug_bounds("curl-preview").is_some());

    model.read_with(cx, |m, _| {
        assert_eq!(m.tab_count(), 1);
        assert_eq!(m.in_flight_count(), 0);
        assert_eq!(m.history_len(), 0);
    });
    ui::replace_text(cx, "curl-input", "curl https://example.com --insecure").unwrap();
    assert!(cx.debug_bounds("curl-error").is_some());
    assert!(cx.debug_bounds("curl-preview").is_none());
    ui::click(cx, "curl-submit").unwrap();
    assert!(cx.debug_bounds("curl-import-dialog").is_some());
    ui::click(cx, "curl-cancel").unwrap();
    assert!(cx.debug_bounds("curl-import-dialog").is_none());
    model.read_with(cx, |m, _| {
        assert_eq!(
            m.active_request().unwrap().url(),
            "https://original.test/keep"
        )
    });
    ui::click(cx, "rail-import").unwrap();
    assert!(text(cx).is_empty());
    cx.simulate_input("curl https://imported.test/final --data-raw ''");
    ui::press(cx, "enter");
    assert!(text(cx).ends_with('\n'));
    ui::press(
        cx,
        if cfg!(target_os = "macos") {
            "cmd-enter"
        } else {
            "ctrl-enter"
        },
    );
    model.read_with(cx, |m, _| {
        assert_eq!(m.tab_count(), 1);
        assert_eq!(m.in_flight_count(), 0);
    });
    ui::click(cx, "curl-submit").unwrap();
    assert!(cx.debug_bounds("curl-import-dialog").is_none());
    model.read_with(cx, |m, _| {
        assert_eq!(m.tab_count(), 2);
        let r = m.active_request().unwrap();
        assert_eq!(r.url(), "https://imported.test/final");
        assert!(r.is_dirty());
        assert_eq!(r.method(), HttpMethod::POST);
        assert_eq!(r.request_body(), RequestBody::Raw(String::new()));
        assert_eq!(r.redirect_policy(), RedirectPolicy::DoNotFollow);
        assert!(matches!(r.response(), ResponseState::NotSent));
        assert_eq!(m.history_len(), 0);
    });
    // Imported URL and Body are projected to the actual editor, not just the model.
    ui::click(cx, "show-code").unwrap();
    assert!(text(cx).contains("https://imported.test/final"));
}

#[gpui_kit::test]
fn code_is_readonly_copy_is_exact_and_switch_to_import_uses_one_overlay(cx: &mut TestAppContext) {
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| PostmanApp::with_view_model(observed, w, cx))
    });
    ui::open_http(cx);
    cx.run_until_parked();
    ui::replace_text(cx, "url-input", "https://example.com/latest?x=1&x=2").unwrap();
    ui::click(cx, "show-code").unwrap();
    let displayed = text(cx);
    assert!(displayed.contains("latest?x=1&x=2"));
    cx.simulate_input("cannot edit");
    assert_eq!(text(cx), displayed);
    ui::click(cx, "curl-copy").unwrap();
    cx.update(|_, cx| assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), displayed));
    model.read_with(cx, |m, _| {
        assert!(m.active_request().unwrap().is_dirty());
        assert_eq!(m.in_flight_count(), 0);
    });
    ui::click(cx, "curl-code-import").unwrap();
    assert!(cx.debug_bounds("curl-code-dialog").is_none());
    assert!(text(cx).is_empty());
    cx.simulate_input("curl https://different.test");
    ui::press(cx, "escape");
    assert!(cx.debug_bounds("curl-import-dialog").is_none());
    assert!(cx.debug_bounds("curl-code-dialog").is_none());
    model.read_with(cx, |m, _| {
        assert_eq!(m.tab_count(), 1);
        assert!(m.active_request().unwrap().url().contains("latest"));
    });
    ui::click(cx, "show-code").unwrap();
    assert_eq!(text(cx), displayed);
    ui::click(cx, "curl-close").unwrap();
}

#[gpui_kit::test]
fn invalid_export_disables_copy_and_import_is_still_available(cx: &mut TestAppContext) {
    let model = cx.new(|_| WorkspaceViewModel::new());
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| PostmanApp::with_view_model(model, w, cx))
    });
    ui::open_http(cx);
    cx.run_until_parked();
    ui::click(cx, "show-code").unwrap();
    assert!(cx.debug_bounds("curl-error").is_some());
    ui::click(cx, "curl-copy").unwrap();
    assert!(cx.debug_bounds("curl-error").is_some());
    ui::press(cx, "escape");
    assert!(cx.debug_bounds("curl-code-dialog").is_none());
    ui::click(cx, "show-code").unwrap();
    ui::click(cx, "curl-code-import").unwrap();
    ui::click(cx, "curl-example").unwrap();
    assert!(cx.debug_bounds("curl-preview").is_some());
    ui::press(cx, "tab");
    ui::press(cx, "shift-tab");
    ui::press(cx, "escape");
    assert!(cx.debug_bounds("curl-import-dialog").is_none());
}

#[gpui_kit::test]
fn compact_window_keeps_entrypoints_and_dialog_actions_inside_viewport(cx: &mut TestAppContext) {
    use gpui_kit::{px, size};
    let model = cx.new(|_| WorkspaceViewModel::new());
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| PostmanApp::with_view_model(model, w, cx))
    });
    let h = cx.update(|w, _| w.window_handle());
    cx.simulate_window_resize(h, size(px(960.), px(640.)));
    ui::open_http(cx);
    cx.run_until_parked();
    for id in [
        "show-code",
        "environment-settings",
        "send-button",
        "rail-import",
    ] {
        let b = cx.debug_bounds(id).unwrap();
        assert!(
            b.right() <= px(960.) && b.bottom() <= px(640.),
            "{id}: {b:?}"
        );
    }
    ui::click(cx, "rail-import").unwrap();
    cx.simulate_input(&format!(
        "curl 'https://example.com/{}' --data-raw '{}'",
        "long/".repeat(100),
        "many lines\n".repeat(50)
    ));
    for id in ["curl-close", "curl-cancel", "curl-submit"] {
        let b = cx.debug_bounds(id).unwrap();
        assert!(
            b.right() <= px(960.) && b.bottom() <= px(640.) && b.top() >= px(0.),
            "{id}: {b:?}"
        );
    }
}

#[gpui_kit::test]
fn imported_request_sends_exact_query_duplicate_headers_auth_and_body(cx: &mut TestAppContext) {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        time::{Duration, Instant},
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!(
        "http://{}/echo?tag=%2f&tag=two",
        listener.local_addr().unwrap()
    );
    let (received, rx) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut socket = loop {
            if let Ok((s, _)) = listener.accept() {
                break s;
            }
            assert!(Instant::now() < deadline, "request never arrived");
            std::thread::sleep(Duration::from_millis(5));
        };
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut data = Vec::new();
        let mut buffer = [0u8; 4096];
        let split = loop {
            let n = socket.read(&mut buffer).unwrap();
            assert!(n > 0);
            data.extend_from_slice(&buffer[..n]);
            if let Some(i) = data.windows(4).position(|v| v == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let headers = String::from_utf8(data[..split].to_vec()).unwrap();
        let length: usize = headers
            .lines()
            .find_map(|l| {
                l.to_ascii_lowercase()
                    .strip_prefix("content-length: ")
                    .map(str::to_owned)
            })
            .unwrap()
            .parse()
            .unwrap();
        while data.len() < split + length {
            let n = socket.read(&mut buffer).unwrap();
            assert!(n > 0);
            data.extend_from_slice(&buffer[..n]);
        }
        received
            .send((headers, String::from_utf8(data[split..].to_vec()).unwrap()))
            .unwrap();
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}").unwrap();
    });
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| PostmanApp::with_view_model(observed, w, cx))
    });
    ui::open_http(cx);
    cx.run_until_parked();
    ui::click(cx, "rail-import").unwrap();
    let command=format!("curl '{url}' -H 'X-Tag: one' -H 'X-Tag: two' -H 'Authorization: Bearer first' -H 'Authorization: Bearer second' -H 'Content-Type: text/plain' --data-raw '用户 & body'");
    cx.simulate_input(&command);
    assert!(rx.try_recv().is_err());
    ui::click(cx, "curl-submit").unwrap();
    ui::click(cx, "show-code").unwrap();
    ui::click(cx, "curl-copy").unwrap();
    assert!(rx.try_recv().is_err());
    ui::click(cx, "curl-close").unwrap();
    ui::click(cx, "send-button").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while model.read_with(cx, |m, _| m.history_len()) == 0 {
        cx.run_until_parked();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    let (headers, body) = rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(headers.starts_with("POST /echo?tag=%2f&tag=two HTTP/1.1"));
    let lower = headers.to_ascii_lowercase();
    assert!(lower.contains("x-tag: one\r\n") && lower.contains("x-tag: two\r\n"));
    assert!(
        lower.contains("authorization: bearer first\r\n")
            && lower.contains("authorization: bearer second\r\n")
    );
    assert_eq!(body, "用户 & body");
    model.read_with(cx, |m, _| {
        assert_eq!(m.history_len(), 1);
        // Existing History redaction canonicalizes percent escapes; the wire assertion above
        // checks the imported raw query independently.
        assert_eq!(m.history()[0].request.url, url.replace("%2f", "%2F"));
    });
    server.join().unwrap();
}

#[gpui_kit::test]
fn imported_redirect_policy_controls_real_send(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let redirect = server
        .mock("POST", "/start")
        .with_status(303)
        .with_header("Location", "/end")
        .expect(2)
        .create();
    let end = server
        .mock("GET", "/end")
        .with_status(200)
        .with_body("finished")
        .expect(1)
        .create();
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| PostmanApp::with_view_model(observed, w, cx))
    });
    ui::open_http(cx);
    cx.run_until_parked();
    for (i, flag) in ["", "-L"].iter().enumerate() {
        ui::click(cx, "rail-import").unwrap();
        cx.simulate_input(&format!("curl {flag} {}/start -d 'a=1'", server.url()));
        ui::click(cx, "curl-submit").unwrap();
        if i == 1 {
            ui::click(cx, "show-code").unwrap();
            let code = text(cx);
            assert!(code.contains("--location"));
            assert!(!code.contains("--request POST"));
            let imported = postman_gpui::models::curl::parse(&code).unwrap();
            assert_eq!(
                imported.draft().construct().request().method,
                HttpMethod::POST
            );
            ui::click(cx, "curl-close").unwrap();
        }
        ui::click(cx, "send-button").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while model.read_with(cx, |m, _| m.history_len()) < i + 1 {
            cx.run_until_parked();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if i == 0 {
            assert!(!end.matched());
        }
    }
    redirect.assert();
    end.assert();
}

#[gpui_kit::test]
fn code_reads_focused_header_body_and_auth_edits_without_saving(cx: &mut TestAppContext) {
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| PostmanApp::with_view_model(observed, w, cx))
    });
    ui::open_http(cx);
    cx.run_until_parked();
    ui::replace_text(cx, "url-input", "https://example.com").unwrap();
    ui::click(cx, "request-pane-headers").unwrap();
    ui::type_into(cx, "row-key-input", "X-Live").unwrap();
    ui::type_into(cx, "row-value-input", "latest").unwrap();
    ui::click(cx, "show-code").unwrap();
    assert!(text(cx).contains("X-Live: latest"));
    ui::click(cx, "curl-close").unwrap();
    ui::choose_method(cx, "POST").unwrap();
    ui::click(cx, "request-pane-body").unwrap();
    ui::click(cx, "body-kind-raw").unwrap();
    ui::replace_text(cx, "body-input", "latest body 用户").unwrap();
    ui::click(cx, "show-code").unwrap();
    assert!(text(cx).contains("latest body 用户"));
    ui::click(cx, "curl-close").unwrap();
    ui::click(cx, "request-pane-authorization").unwrap();
    ui::replace_text(cx, "authorization-input", " Bearer test-only-token ").unwrap();
    ui::click(cx, "show-code").unwrap();
    assert!(text(cx).contains("Authorization: Bearer test-only-token"));
    model.read_with(cx, |m, _| {
        assert!(m.active_request().unwrap().is_dirty());
        assert_eq!(m.history_len(), 0);
        assert_eq!(m.in_flight_count(), 0);
    });
}

#[gpui_kit::test]
fn dismissal_restores_trigger_focus_and_keyboard_can_confirm(cx: &mut TestAppContext) {
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| PostmanApp::with_view_model(observed, w, cx))
    });
    ui::open_http(cx);
    cx.run_until_parked();
    ui::click(cx, "rail-import").unwrap();
    cx.simulate_input("curl https://example.com");
    ui::press(cx, "escape");
    assert!(cx.debug_bounds("curl-import-dialog").is_none());
    ui::press(cx, "space");
    assert!(cx.debug_bounds("curl-import-dialog").is_some());
    assert!(text(cx).is_empty());
    // A click on the overlay dismisses, without falling through to the navigation rail.
    cx.simulate_click(
        gpui_kit::point(gpui_kit::px(10.), gpui_kit::px(300.)),
        gpui_kit::Modifiers::none(),
    );
    assert!(cx.debug_bounds("curl-import-dialog").is_none());
    ui::click(cx, "rail-import").unwrap();
    cx.simulate_input("curl https://example.com/keyboard");
    // Tab/Shift+Tab leave command text unchanged and traverse actual Kit controls.
    let original = text(cx);
    ui::press(cx, "tab");
    ui::press(cx, "shift-tab");
    assert_eq!(text(cx), original);
    for _ in 0..8 {
        ui::press(cx, "tab");
        let focused = cx.update(|w, cx| {
            w.render_frame(cx);
            w.find("curl-submit").focused().unwrap_or(false)
        });
        if focused {
            break;
        }
    }
    ui::press(cx, "space");
    model.read_with(cx, |m, _| {
        assert_eq!(m.tab_count(), 2);
        assert_eq!(
            m.active_request().unwrap().url(),
            "https://example.com/keyboard"
        );
    });
    // Success deliberately focuses the new URL instead of restoring the Import trigger.
    ui::press(
        cx,
        if cfg!(target_os = "macos") {
            "cmd-a"
        } else {
            "ctrl-a"
        },
    );
    cx.simulate_input("https://example.com/edited-after-import");
    model.read_with(cx, |m, _| {
        assert_eq!(
            m.active_request().unwrap().url(),
            "https://example.com/edited-after-import"
        );
    });
    ui::click(cx, "show-code").unwrap();
    ui::press(cx, "escape");
    ui::press(cx, "space");
    assert!(cx.debug_bounds("curl-code-dialog").is_some());
}

#[gpui_kit::test]
fn importing_during_send_keeps_the_original_snapshot_and_environment(cx: &mut TestAppContext) {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        time::{Duration, Instant},
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let original = format!("http://{}/pending", listener.local_addr().unwrap());
    let (received, rx) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let server = std::thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut stream = loop {
            if let Ok((s, _)) = listener.accept() {
                break s;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut bytes = [0; 4096];
        let len = stream.read(&mut bytes).unwrap();
        assert!(String::from_utf8_lossy(&bytes[..len]).starts_with("GET /pending HTTP/1.1"));
        received.send(()).unwrap();
        gate.recv_timeout(Duration::from_secs(10)).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
            .unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let history = dir.path().join("history.sqlite3");
    let env = dir.path().join("environments.json");
    let env_json = r#"{"version":1,"active_id":"dev","environments":[{"id":"dev","name":"Development","base_url":"https://environment.test"}]}"#;
    std::fs::write(&env, env_json).unwrap();
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| {
            PostmanApp::with_view_model_and_history_path(observed, history, w, cx)
        })
    });
    ui::open_http(cx);
    cx.run_until_parked();
    ui::replace_text(cx, "url-input", &original).unwrap();
    let original_id = model.read_with(cx, |m, _| m.active_tab_id().unwrap());
    ui::click_without_wait(cx, "send-button").unwrap();
    rx.recv_timeout(Duration::from_secs(10)).unwrap();
    // Operate Kit directly while the test transport is held; draining would join its task.
    cx.update(|w, cx| w.click("rail-import", cx));
    cx.update(|w, cx| {
        w.render_frame(cx);
        w.input("curl https://unrelated.test/imported", cx);
    });
    cx.update(|w, cx| {
        w.render_frame(cx);
        w.click("curl-submit", cx);
    });
    model.read_with(cx, |m, _| {
        assert_eq!(m.tab_count(), 2);
        assert!(m.request_for_tab(original_id).unwrap().is_sending());
        assert_eq!(m.request_for_tab(original_id).unwrap().url(), original);
        assert_eq!(
            m.active_request().unwrap().url(),
            "https://unrelated.test/imported"
        );
        assert!(!m.active_request().unwrap().is_sending());
    });
    assert_eq!(std::fs::read_to_string(env).unwrap(), env_json);
    release.send(()).unwrap();
    server.join().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while model.read_with(cx, |m, _| m.history_len()) == 0 {
        cx.run_until_parked();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    model.read_with(cx, |m, _| {
        assert_eq!(m.history()[0].request.url, original);
        assert!(matches!(
            m.active_request().unwrap().response(),
            ResponseState::NotSent
        ));
    });
}

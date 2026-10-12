//! Real Kit environment interactions, durable commits, and request isolation.
#[path = "common/ui.rs"]
mod ui;
use gpui::{AppContext, TestAppContext, VisualTestContext};
use gpui_kit::test::TestWindowExt;
use postman_gpui::app::{PostmanApp, WorkspaceViewModel};
use serde_json::{json, Value};
use std::{fs, path::Path};

fn seed(path: &Path, old: &str, next: &str) {
    fs::write(
        path,
        json!({"version":1, "active_id":"dev", "environments":[
            {"id":"dev", "name":"Development", "base_url":old},
            {"id":"local", "name":"Local", "base_url":next}
        ]})
        .to_string(),
    )
    .unwrap();
}
fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn select(cx: &mut VisualTestContext, from: usize, to: usize) {
    cx.update(|w, cx| w.click("environment-select", cx));
    cx.run_until_parked();
    for _ in 0..from.abs_diff(to) {
        ui::press(cx, if to > from { "down" } else { "up" });
    }
    ui::press(cx, "enter");
    cx.run_until_parked();
}
fn selected(cx: &mut VisualTestContext) -> String {
    cx.update(|w, cx| {
        w.render_frame(cx);
        w.find("environment-select")
            .value()
            .unwrap_or("")
            .to_string()
    })
}

#[gpui_kit::test]
fn create_first_environment_cancel_validate_and_restart(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("environments.json");
    let history = dir.path().join("history.sqlite3");
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let history1 = history.clone();
    let (_, win) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| {
            PostmanApp::with_view_model_and_history_path(observed, history1, w, cx)
        })
    });
    ui::open_http(win);
    win.run_until_parked();
    ui::replace_text(win, "url-input", "https://unrelated.test/keep?a=1").unwrap();
    ui::click(win, "environment-settings").unwrap();
    ui::click(win, "environment-save").unwrap();
    assert!(win.debug_bounds("environment-dialog").is_some());
    assert!(!path.exists());
    // Invalid Save focuses the name; Tab moves through Kit inputs to the URL.
    win.simulate_input(" Development ");
    ui::press(win, "tab");
    win.simulate_input("https://api.example.com/api///");
    ui::click(win, "environment-cancel").unwrap();
    assert!(!path.exists());
    ui::click(win, "environment-settings").unwrap();
    win.simulate_input("Development");
    ui::press(win, "tab");
    win.simulate_input("https://api.example.com/api///");
    ui::press(win, "enter");
    win.run_until_parked();
    assert!(win.debug_bounds("environment-dialog").is_none());
    assert_eq!(selected(win), "Development");
    assert_eq!(
        read(&path)["environments"][0]["base_url"],
        "https://api.example.com/api"
    );
    model.read_with(win, |m, _| {
        assert_eq!(
            m.active_request().unwrap().url(),
            "https://unrelated.test/keep?a=1"
        )
    });
    let restarted = cx.new(|_| WorkspaceViewModel::new());
    let (_, win) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| {
            PostmanApp::with_view_model_and_history_path(restarted, history, w, cx)
        })
    });
    ui::open_http(win);
    win.run_until_parked();
    assert_eq!(selected(win), "Development");
}

#[gpui_kit::test]
fn shared_selection_rebases_all_matching_drafts_and_cancel_keeps_configuration(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("environments.json");
    seed(
        &path,
        "https://api.example.com/api",
        "http://localhost:3000/v2",
    );
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let history = dir.path().join("history.sqlite3");
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| {
            PostmanApp::with_view_model_and_history_path(observed, history, w, cx)
        })
    });
    ui::open_http(cx);
    cx.run_until_parked();
    ui::replace_text(
        cx,
        "url-input",
        "https://api.example.com/api/users%2F1?tag=a&tag=b&v=%2f+%20#part",
    )
    .unwrap();
    ui::click(cx, "new-tab-button").unwrap();
    ui::replace_text(cx, "url-input", "https://api.example.com/apiary/keep").unwrap();
    select(cx, 0, 1);
    assert_eq!(selected(cx), "Local");
    assert_eq!(read(&path)["active_id"], "local");
    model.read_with(cx, |m, _| {
        assert_eq!(
            m.tabs()[0].url(),
            "http://localhost:3000/v2/users%2F1?tag=a&tag=b&v=%2f+%20#part"
        );
        assert_eq!(m.tabs()[1].url(), "https://api.example.com/apiary/keep");
        assert_eq!(m.tabs()[0].url_query_parameter_count(), 3);
    });
    ui::click(cx, "nav-flows").unwrap();
    assert_eq!(selected(cx), "Local");
    select(cx, 1, 2); // Manage entry opens a dialog without changing active selection.
    assert_eq!(read(&path)["active_id"], "local");
    assert!(cx.debug_bounds("environment-dialog").is_some());
    ui::replace_text(cx, "environment-local-name", "Changed").unwrap();
    ui::press(cx, "escape");
    assert!(cx.debug_bounds("environment-dialog").is_none());
    assert_eq!(read(&path)["environments"][1]["name"], "Local");
    select(cx, 1, 0);
    ui::open_http(cx);
    assert_eq!(selected(cx), "Development");
    model.read_with(cx, |m, _| {
        assert_eq!(
            m.tabs()[0].url(),
            "https://api.example.com/api/users%2F1?tag=a&tag=b&v=%2f+%20#part"
        )
    });
}

#[gpui_kit::test]
fn save_failure_preserves_draft_and_retry_commits_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("environments.json");
    seed(&path, "https://api.example.com", "http://localhost:3000");
    let model = cx.new(|_| WorkspaceViewModel::new());
    let history = dir.path().join("history.sqlite3");
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| {
            PostmanApp::with_view_model_and_history_path(model, history, w, cx)
        })
    });
    ui::open_http(cx);
    cx.run_until_parked();
    ui::click(cx, "environment-settings").unwrap();
    ui::replace_text(cx, "environment-dev-name", "Renamed").unwrap();
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    ui::click(cx, "environment-save").unwrap();
    cx.run_until_parked();
    assert!(cx.debug_bounds("environment-save-error").is_some());
    assert!(cx.debug_bounds("environment-dialog").is_some());
    assert_eq!(selected(cx), "Development");
    fs::remove_dir(&path).unwrap();
    ui::click(cx, "environment-save").unwrap();
    cx.run_until_parked();
    assert_eq!(read(&path)["environments"][0]["name"], "Renamed");
    assert_eq!(selected(cx), "Renamed");
}

#[gpui_kit::test]
fn dialog_geometry_keyboard_validation_and_active_deletion(cx: &mut TestAppContext) {
    use gpui::{px, size};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("environments.json");
    seed(&path, "https://api.example.com", "http://localhost:3000");
    let model = cx.new(|_| WorkspaceViewModel::new());
    let history = dir.path().join("history.sqlite3");
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| {
            PostmanApp::with_view_model_and_history_path(model, history, w, cx)
        })
    });
    ui::open_http(cx);
    cx.run_until_parked();
    for mode in [
        gpui_kit::component::ThemeMode::Light,
        gpui_kit::component::ThemeMode::Dark,
    ] {
        cx.update(|_, cx| postman_gpui::ui::theme::apply(mode, cx));
        for (width, height) in [(1440., 960.), (960., 640.)] {
            let handle = cx.update(|w, _| w.window_handle());
            cx.simulate_window_resize(handle, size(px(width), px(height)));
            ui::click(cx, "environment-settings").unwrap();
            let modal = cx.debug_bounds("dialog-0").unwrap();
            let footer = cx.debug_bounds("environment-dialog-footer").unwrap();
            assert!(modal.left() >= px(0.) && modal.right() <= px(width));
            assert!(modal.top() >= px(0.) && modal.bottom() <= px(height));
            assert!(footer.bottom() <= modal.bottom());
            ui::replace_text(cx, "environment-local-name", "development").unwrap();
            ui::click(cx, "environment-save").unwrap();
            assert!(cx.debug_bounds("environment-dialog").is_some());
            ui::press(cx, "escape");
        }
    }
    ui::click(cx, "environment-settings").unwrap();
    ui::click(cx, "environment-remove-dev").unwrap();
    ui::click(cx, "environment-remove-local").unwrap(); // Last environment is protected.
    assert!(cx.debug_bounds("environment-local-name").is_some());
    ui::click(cx, "environment-save").unwrap();
    cx.run_until_parked();
    assert_eq!(read(&path)["active_id"], "local");
    assert_eq!(read(&path)["environments"].as_array().unwrap().len(), 1);
}

#[gpui_kit::test]
fn corrupt_configuration_is_not_overwritten_and_can_be_reloaded(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("environments.json");
    fs::write(&path, "broken configuration").unwrap();
    let model = cx.new(|_| WorkspaceViewModel::new());
    let history = dir.path().join("history.sqlite3");
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| {
            PostmanApp::with_view_model_and_history_path(model, history, w, cx)
        })
    });
    ui::open_http(cx);
    cx.run_until_parked();
    ui::click(cx, "environment-settings").unwrap();
    assert!(cx.debug_bounds("environment-save-error").is_some());
    ui::click(cx, "environment-save").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "broken configuration");
    seed(&path, "https://api.example.com", "http://localhost:3000");
    ui::click(cx, "environment-reload").unwrap();
    cx.run_until_parked();
    assert_eq!(selected(cx), "Development");
    assert!(cx.debug_bounds("environment-dev-name").is_some());
    // Recovery replaces retained inputs and transfers keyboard focus into the new draft.
    ui::press(
        cx,
        if cfg!(target_os = "macos") {
            "cmd-a"
        } else {
            "ctrl-a"
        },
    );
    cx.simulate_input("Recovered");
    ui::click(cx, "environment-save").unwrap();
    cx.run_until_parked();
    assert_eq!(selected(cx), "Recovered");
    ui::click(cx, "environment-settings").unwrap();
    ui::click(cx, "environment-cancel").unwrap();
}

#[gpui_kit::test]
fn environment_switch_during_http_keeps_sent_snapshot_and_uses_new_target_next(
    cx: &mut TestAppContext,
) {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        time::{Duration, Instant},
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let old = format!("http://{}", listener.local_addr().unwrap());
    let (received_tx, received_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = std::thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut stream = loop {
            if let Ok((stream, _)) = listener.accept() {
                break stream;
            }
            assert!(
                Instant::now() < deadline,
                "first endpoint did not receive request"
            );
            std::thread::sleep(Duration::from_millis(5));
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut bytes = [0u8; 4096];
        let length = stream.read(&mut bytes).unwrap();
        assert!(String::from_utf8_lossy(&bytes[..length])
            .starts_with("GET /users?tag=a&tag=b HTTP/1.1"));
        received_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nold")
            .unwrap();
    });
    let mut next = mockito::Server::new();
    let new_request = next
        .mock("GET", "/users")
        .match_query(mockito::Matcher::Exact("tag=a&tag=b".into()))
        .with_status(200)
        .with_body("new")
        .expect(1)
        .create();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("environments.json");
    seed(&path, &old, &next.url());
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let history = dir.path().join("history.sqlite3");
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| {
            PostmanApp::with_view_model_and_history_path(observed, history, w, cx)
        })
    });
    ui::open_http(cx);
    cx.run_until_parked();
    let original = format!("{old}/users?tag=a&tag=b");
    ui::replace_text(cx, "url-input", &original).unwrap();
    ui::click_without_wait(cx, "send-button").unwrap();
    received_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    // Do not drain GPUI's test background executor while the fixture is held: the existing
    // transport join runs synchronously there. Request selection through real Kit controls,
    // then release the fixture and drain both completions.
    cx.update(|w, cx| w.click("environment-select", cx));
    ui::press(cx, "down enter");
    model.read_with(cx, |m, _| assert!(m.active_request().unwrap().is_sending()));
    release_tx.send(()).unwrap();
    server.join().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        if model.read_with(cx, |m, _| m.history_len()) == 1 {
            break;
        }
        assert!(Instant::now() < deadline, "first history entry missing");
        std::thread::sleep(Duration::from_millis(5));
    }
    model.read_with(cx, |m, _| {
        assert_eq!(m.history()[0].request.url, original);
        assert!(m.active_request().unwrap().is_dirty());
    });
    ui::click(cx, "send-button").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        if model.read_with(cx, |m, _| m.history_len()) == 2 {
            break;
        }
        assert!(Instant::now() < deadline, "second history entry missing");
        std::thread::sleep(Duration::from_millis(5));
    }
    new_request.assert();
    model.read_with(cx, |m, _| {
        assert_eq!(
            m.history()[0].request.url,
            format!("{}/users?tag=a&tag=b", next.url())
        )
    });
}

#[gpui_kit::test]
fn long_environment_list_keeps_actions_visible_and_added_fields_focused(cx: &mut TestAppContext) {
    use gpui::{px, size};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("environments.json");
    let rows:Vec<_>=(0..8).map(|n|json!({"id":format!("env{n}"),"name":format!("Environment {n}"),"base_url":format!("https://host{n}.example.com")})).collect();
    fs::write(
        &path,
        json!({"version":1,"active_id":"env7","environments":rows}).to_string(),
    )
    .unwrap();
    let model = cx.new(|_| WorkspaceViewModel::new());
    let history = dir.path().join("history.sqlite3");
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| {
            PostmanApp::with_view_model_and_history_path(model, history, w, cx)
        })
    });
    let handle = cx.update(|w, _| w.window_handle());
    cx.simulate_window_resize(handle, size(px(960.), px(640.)));
    ui::open_http(cx);
    cx.run_until_parked();
    ui::click(cx, "environment-settings").unwrap();
    let header = cx.debug_bounds("environment-dialog-header").unwrap();
    let footer = cx.debug_bounds("environment-dialog-footer").unwrap();
    let content = cx.debug_bounds("environment-content").unwrap();
    // The test platform has no native frame loop; deliver the post-layout scroll callback.
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
        w.render_frame(cx);
    });
    let active = cx.debug_bounds("environment-env7-name").unwrap();
    assert!(
        active.top() >= content.top() && active.bottom() <= content.bottom(),
        "active={active:?} content={content:?}"
    );
    ui::scroll_down(cx, "environment-content", 2000.).unwrap();
    ui::click(cx, "environment-add").unwrap();
    cx.simulate_input("New environment");
    ui::press(cx, "tab");
    cx.simulate_input("https://new.example.com/api");
    assert_eq!(
        header,
        cx.debug_bounds("environment-dialog-header").unwrap()
    );
    assert_eq!(
        footer,
        cx.debug_bounds("environment-dialog-footer").unwrap()
    );
    ui::click(cx, "environment-save").unwrap();
    cx.run_until_parked();
    assert_eq!(read(&path)["environments"].as_array().unwrap().len(), 9);
    assert_eq!(read(&path)["environments"][8]["name"], "New environment");
    assert_eq!(read(&path)["active_id"], "env7");
    ui::click(cx, "environment-settings").unwrap();
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
        w.render_frame(cx);
    });
    ui::scroll_up(cx, "environment-content", 10000.).unwrap();
    ui::replace_text(cx, "environment-env0-name", " ").unwrap();
    ui::scroll_down(cx, "environment-content", 10000.).unwrap();
    ui::click(cx, "environment-save").unwrap();
    let invalid = cx.debug_bounds("environment-env0-name").unwrap();
    assert!(invalid.top() >= content.top() && invalid.bottom() <= content.bottom());
    assert_eq!(read(&path)["environments"][0]["name"], "Environment 0");
}

#[gpui_kit::test]
fn editing_active_base_rebases_only_after_save_and_inactive_edits_keep_urls(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("environments.json");
    seed(
        &path,
        "https://api.example.com/api",
        "http://localhost:3000",
    );
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let history = dir.path().join("history.sqlite3");
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| {
            PostmanApp::with_view_model_and_history_path(observed, history, w, cx)
        })
    });
    ui::open_http(cx);
    cx.run_until_parked();
    let original = "https://api.example.com/api/users?tag=a&tag=b#part";
    ui::replace_text(cx, "url-input", original).unwrap();
    ui::click(cx, "environment-settings").unwrap();
    ui::replace_text(cx, "environment-local-base-url", "http://localhost:4000/v2").unwrap();
    ui::replace_text(cx, "environment-dev-name", "Renamed").unwrap();
    ui::click(cx, "environment-save").unwrap();
    cx.run_until_parked();
    assert_eq!(selected(cx), "Renamed");
    assert_eq!(read(&path)["active_id"], "dev");
    model.read_with(cx, |m, _| {
        assert_eq!(m.active_request().unwrap().url(), original)
    });
    ui::click(cx, "environment-settings").unwrap();
    ui::replace_text(
        cx,
        "environment-dev-base-url",
        "https://staging.example.com/v2/",
    )
    .unwrap();
    model.read_with(cx, |m, _| {
        assert_eq!(m.active_request().unwrap().url(), original)
    });
    ui::click(cx, "environment-save").unwrap();
    cx.run_until_parked();
    model.read_with(cx, |m, _| {
        assert_eq!(
            m.active_request().unwrap().url(),
            "https://staging.example.com/v2/users?tag=a&tag=b#part"
        );
        assert!(m.active_request().unwrap().is_dirty());
    });
}

#[gpui_kit::test]
fn failed_selection_does_not_publish_a_new_active_environment(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("environments.json");
    seed(&path, "https://api.example.com", "http://localhost:3000");
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let history = dir.path().join("history.sqlite3");
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| {
            PostmanApp::with_view_model_and_history_path(observed, history, w, cx)
        })
    });
    ui::open_http(cx);
    cx.run_until_parked();
    ui::replace_text(cx, "url-input", "https://api.example.com/users").unwrap();
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    select(cx, 0, 1);
    assert_eq!(selected(cx), "Development");
    assert!(cx.debug_bounds("environment-error").is_some());
    model.read_with(cx, |m, _| {
        assert_eq!(
            m.active_request().unwrap().url(),
            "https://api.example.com/users"
        )
    });
    fs::remove_dir(&path).unwrap();
    select(cx, 0, 1);
    assert_eq!(selected(cx), "Local");
    assert_eq!(read(&path)["active_id"], "local");
    model.read_with(cx, |m, _| {
        assert_eq!(
            m.active_request().unwrap().url(),
            "http://localhost:3000/users"
        )
    });
}

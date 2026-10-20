//! Real Kit controls exercise durable Save, reopening, keyboard ownership and close protection.
#[path = "common/ui.rs"]
mod ui;
use gpui_kit::{AppContext, TestAppContext, VisualTestContext};
use postman_gpui::app::{PostmanApp, WorkspaceViewModel};

fn save_key(cx: &mut VisualTestContext) {
    ui::press(
        cx,
        if cfg!(target_os = "macos") {
            "cmd-s"
        } else {
            "ctrl-s"
        },
    );
}
fn close_key(cx: &mut VisualTestContext) {
    ui::press(
        cx,
        if cfg!(target_os = "macos") {
            "cmd-w"
        } else {
            "ctrl-w"
        },
    );
}
fn named_save(cx: &mut VisualTestContext, name: &str) {
    ui::click(cx, "save-request").unwrap();
    ui::replace_text(cx, "saved-request-name", name).unwrap();
    ui::click(cx, "request-save-confirm").unwrap();
    cx.run_until_parked();
    assert!(cx.debug_bounds("save-request-dialog").is_none());
}

#[gpui_kit::test]
fn latest_input_save_restart_search_and_reopen_are_lossless(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite3");
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let first = path.clone();
    let (_, win) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| {
            PostmanApp::with_view_model_and_history_path(observed, first, w, cx)
        })
    });
    ui::open_http(win);
    win.run_until_parked();
    ui::click(win, "rail-import").unwrap();
    win.simulate_input(
        "curl 'https://httpbingo.org/post?tag=a&tag=b' --json '{\"name\":\"Maya\"}'",
    );
    ui::click(win, "curl-submit").unwrap();
    ui::replace_text(win, "url-input", "https://httpbingo.org/post?latest=1").unwrap();
    save_key(win);
    assert!(win.debug_bounds("save-request-dialog").is_some());
    ui::replace_text(win, "saved-request-name", " ").unwrap();
    ui::press(win, "enter");
    assert!(win.debug_bounds("request-save-error").is_some());
    ui::replace_text(win, "saved-request-name", "Echo saved").unwrap();
    ui::click(win, "request-save-confirm").unwrap();
    win.run_until_parked();
    model.read_with(win, |m, _| {
        assert_eq!(m.saved_requests().len(), 1);
        assert_eq!(
            m.saved_requests()[0].draft().url(),
            "https://httpbingo.org/post?latest=1"
        );
        assert!(!m.active_request().unwrap().is_dirty());
        assert_eq!(m.history_len(), 0);
        assert_eq!(m.in_flight_count(), 0);
    });
    // Repeated Save commits to the same stable identity and captures the focused input.
    ui::replace_text(win, "url-input", "https://httpbingo.org/post?latest=2").unwrap();
    save_key(win);
    win.run_until_parked();
    model.read_with(win, |m, _| {
        assert_eq!(m.saved_requests().len(), 1);
        assert!(!m.active_request().unwrap().is_dirty());
    });
    close_key(win);
    win.run_until_parked();
    let restarted = cx.new(|_| WorkspaceViewModel::new());
    let observed = restarted.clone();
    let (_, win) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| {
            PostmanApp::with_view_model_and_history_path(observed, path, w, cx)
        })
    });
    win.run_until_parked();
    assert!(win.debug_bounds("home-saved-requests").is_some());
    ui::click(win, "saved-requests-browse").unwrap();
    win.simulate_input("Echo saved");
    ui::press(win, "enter");
    restarted.read_with(win, |m, _| {
        let r = m.active_request().unwrap();
        assert_eq!(r.tab_title(), "Echo saved");
        assert_eq!(r.url(), "https://httpbingo.org/post?latest=2");
        assert!(!r.is_dirty());
        assert_eq!(r.body(), "{\"name\":\"Maya\"}");
        assert_eq!(m.history_len(), 0);
        assert_eq!(m.in_flight_count(), 0);
    });
    let count = restarted.read_with(win, |m, _| m.tab_count());
    ui::press(
        win,
        if cfg!(target_os = "macos") {
            "cmd-k"
        } else {
            "ctrl-k"
        },
    );
    win.simulate_input("Echo saved");
    ui::press(win, "enter");
    assert_eq!(restarted.read_with(win, |m, _| m.tab_count()), count);
}

#[gpui_kit::test]
fn cancel_discard_save_on_close_and_modal_shortcut_ownership(cx: &mut TestAppContext) {
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| PostmanApp::with_view_model(observed, w, cx))
    });
    ui::open_http(cx);
    cx.run_until_parked();
    ui::replace_text(cx, "url-input", "https://httpbingo.org/get?keep=1").unwrap();
    named_save(cx, "Keep");
    ui::replace_text(cx, "url-input", "incomplete edit").unwrap();
    close_key(cx);
    assert!(cx.debug_bounds("unsaved-request-dialog").is_some());
    ui::click(cx, "request-save-cancel").unwrap();
    model.read_with(cx, |m, _| {
        assert_eq!(m.active_request().unwrap().url(), "incomplete edit")
    });
    close_key(cx);
    ui::press(cx, "escape");
    assert!(cx.debug_bounds("unsaved-request-dialog").is_none());
    ui::click(cx, "show-code").unwrap();
    save_key(cx);
    assert!(cx.debug_bounds("save-request-dialog").is_none());
    ui::press(cx, "escape");
    close_key(cx);
    ui::click(cx, "request-save-discard").unwrap();
    cx.run_until_parked();
    model.read_with(cx, |m, _| {
        assert_eq!(
            m.saved_requests()[0].draft().url(),
            "https://httpbingo.org/get?keep=1"
        );
        assert_eq!(m.active_request().unwrap().url(), "");
    });
    ui::replace_text(cx, "url-input", "unfinished URL").unwrap();
    close_key(cx);
    ui::replace_text(cx, "saved-request-name", "Unfinished").unwrap();
    ui::click(cx, "request-save-confirm").unwrap();
    cx.run_until_parked();
    model.read_with(cx, |m, _| {
        assert_eq!(m.saved_requests().len(), 2);
        assert_eq!(m.active_request().unwrap().url(), "");
    });
}

#[gpui_kit::test]
fn failed_commit_keeps_draft_and_last_version_then_retry_succeeds(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite3");
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| {
            PostmanApp::with_view_model_and_history_path(observed, path, w, cx)
        })
    });
    ui::open_http(cx);
    cx.run_until_parked();
    ui::replace_text(cx, "url-input", "https://httpbingo.org/get").unwrap();
    named_save(cx, "Echo");
    let connection = rusqlite::Connection::open(dir.path().join("requests.sqlite3")).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_update BEFORE UPDATE ON request_documents BEGIN SELECT RAISE(ABORT, 'fail'); END;").unwrap();
    ui::replace_text(cx, "url-input", "https://httpbingo.org/get?changed=1").unwrap();
    close_key(cx);
    ui::click(cx, "request-save-confirm").unwrap();
    cx.run_until_parked();
    assert!(cx.debug_bounds("request-save-error").is_some());
    model.read_with(cx, |m, _| {
        assert!(m.active_request().unwrap().is_dirty());
        assert_eq!(
            m.saved_requests()[0].draft().url(),
            "https://httpbingo.org/get"
        );
    });
    connection
        .execute_batch("DROP TRIGGER fail_update")
        .unwrap();
    ui::click(cx, "request-save-confirm").unwrap();
    cx.run_until_parked();
    assert!(cx.debug_bounds("unsaved-request-dialog").is_none());
    model.read_with(cx, |m, _| {
        assert_eq!(m.active_request().unwrap().url(), "");
        assert_eq!(
            m.saved_requests()[0].draft().url(),
            "https://httpbingo.org/get?changed=1"
        );
    });
}

#[gpui_kit::test]
fn quit_visits_each_dirty_document_and_cancel_keeps_all_tabs(cx: &mut TestAppContext) {
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| PostmanApp::with_view_model(observed, w, cx))
    });
    ui::open_http(cx);
    cx.run_until_parked();
    ui::replace_text(cx, "url-input", "https://httpbingo.org/get?first=1").unwrap();
    ui::click(cx, "new-tab-button").unwrap();
    ui::replace_text(cx, "url-input", "https://httpbingo.org/get?second=2").unwrap();
    cx.update(|w, cx| w.dispatch_action(Box::new(postman_gpui::app::Quit), cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("unsaved-request-dialog").is_some());
    ui::replace_text(cx, "saved-request-name", "First").unwrap();
    ui::click(cx, "request-save-confirm").unwrap();
    cx.run_until_parked();
    assert!(cx.debug_bounds("unsaved-request-dialog").is_some());
    model.read_with(cx, |m, _| {
        assert_eq!(m.saved_requests().len(), 1);
        assert!(!m.tabs()[0].is_dirty());
        assert!(m.tabs()[1].is_dirty());
    });
    ui::click(cx, "request-save-cancel").unwrap();
    model.read_with(cx, |m, _| assert_eq!(m.tab_count(), 2));
    cx.update(|w, cx| w.dispatch_action(Box::new(postman_gpui::app::Quit), cx));
    cx.run_until_parked();
    ui::press(cx, "escape");
    assert!(cx.debug_bounds("unsaved-request-dialog").is_none());
    model.read_with(cx, |m, _| assert!(m.tabs()[1].is_dirty()));
}

#[gpui_kit::test]
fn save_dialog_and_toolbar_fit_reference_and_compact_windows_in_both_themes(
    cx: &mut TestAppContext,
) {
    use gpui_kit::component::ThemeMode;
    use gpui_kit::{px, size};
    let model = cx.new(|_| WorkspaceViewModel::new());
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| PostmanApp::with_view_model(model, w, cx))
    });
    ui::open_http(cx);
    cx.run_until_parked();
    ui::replace_text(cx, "url-input", "https://httpbingo.org/get").unwrap();
    named_save(
        cx,
        "A long saved request title that must not push toolbar actions offscreen",
    );
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        for (width, height) in [(1440., 960.), (960., 640.)] {
            let handle = cx.update(|w, cx| {
                postman_gpui::ui::theme::apply(mode, cx);
                w.window_handle()
            });
            cx.simulate_window_resize(handle, size(px(width), px(height)));
            let save = cx.debug_bounds("save-request").unwrap();
            let code = cx.debug_bounds("show-code").unwrap();
            let rename = cx.debug_bounds("rename-request").unwrap();
            assert!(code.right() <= rename.left() && rename.right() <= save.left());
            assert!(save.right() <= px(width));
            ui::click(cx, "rename-request").unwrap();
            for selector in [
                "saved-request-name",
                "request-save-cancel",
                "request-save-confirm",
            ] {
                let b = cx.debug_bounds(selector).unwrap();
                assert!(b.left() >= px(0.) && b.right() <= px(width) && b.bottom() <= px(height));
            }
            ui::press(cx, "escape");
        }
    }
}

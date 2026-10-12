//! P4.1 modal discovery workflows over real Kit controls and SQLite.
#[path = "common/ui.rs"]
mod ui;
use gpui::{AppContext, TestAppContext};
use postman_gpui::{
    app::{PostmanApp, WorkspaceViewModel},
    models::{HistoryEntry, HttpMethod, Request},
    persistence::{
        HistoryRepository, SqliteHistoryRepository, VersionedHistorySnapshot,
        DEFAULT_HISTORY_RETENTION_LIMIT,
    },
};
use ui::{click, replace_text};

fn seed(path: &std::path::Path, count: usize) {
    let mut repo = SqliteHistoryRepository::new(path).unwrap();
    repo.initialize().unwrap();
    for n in 0..count {
        let entry = HistoryEntry::completed(
            Request::new(HttpMethod::GET, format!("https://example.test/record/{n}")),
            format!("Request {n}"),
            200,
            n as u128,
            0,
        );
        repo.append_and_trim(
            &VersionedHistorySnapshot::try_from(&entry).unwrap(),
            DEFAULT_HISTORY_RETENTION_LIMIT,
        )
        .unwrap();
    }
}

#[gpui::test]
fn modal_blocks_background_commands_traps_tab_and_restores_editor_focus(cx: &mut TestAppContext) {
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| PostmanApp::with_view_model(observed, w, cx))
    });
    ui::open_http(cx);
    replace_text(cx, "url-input", "https://unsent.example/draft").unwrap();
    for trigger in ["global-search-trigger", "rail-search"] {
        click(cx, trigger).unwrap();
        assert!(cx.debug_bounds("global-search-dialog").is_some());
        cx.simulate_input("no-match");
        cx.simulate_keystrokes("ctrl-t ctrl-w ctrl-enter ctrl-l");
        click(cx, "new-tab-button").unwrap(); // Covered background button must not act.
        for _ in 0..12 {
            ui::press(cx, "tab");
        }
        cx.simulate_keystrokes("ctrl-t ctrl-w ctrl-enter");
        model.read_with(cx, |m, _| {
            assert_eq!(m.tab_count(), 1);
            assert_eq!(m.history_len(), 0);
            assert_eq!(
                m.active_request().unwrap().url(),
                "https://unsent.example/draft"
            );
        });
        cx.simulate_keystrokes("escape");
        assert!(cx.debug_bounds("global-search-dialog").is_none());
    }
    click(cx, "url-input").unwrap();
    cx.simulate_keystrokes("cmd-k");
    cx.simulate_input("text");
    // Kit Input uses NSMenu on macOS; topmost menu Escape is checked in the native app.
    cx.simulate_keystrokes("escape");
    cx.simulate_input("/restored");
    assert_eq!(
        model.read_with(cx, |m, _| m.active_request().unwrap().url().to_string()),
        "https://unsent.example/draft/restored"
    );
    cx.simulate_keystrokes("cmd-/");
    assert!(cx.debug_bounds("shortcut-help-dialog").is_some());
    for _ in 0..8 {
        ui::press(cx, "shift-tab");
    }
    cx.simulate_keystrokes("ctrl-t ctrl-enter");
    assert_eq!(model.read_with(cx, |m, _| m.tab_count()), 1);
    cx.simulate_keystrokes("escape");
    assert!(cx.debug_bounds("shortcut-help-dialog").is_none());
}

#[gpui::test]
fn history_confirmation_cancel_refresh_and_long_list_keyboard_replay(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite3");
    seed(&path, 50);
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let app_path = path.clone();
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| {
            PostmanApp::with_view_model_and_history_path(observed, app_path, w, cx)
        })
    });
    ui::open_http(cx);
    cx.run_until_parked();
    ui::open_history(cx);
    click(cx, "history-clear-button").unwrap();
    assert!(cx.debug_bounds("dialog-1").is_some());
    cx.simulate_keystrokes("escape");
    assert!(cx.debug_bounds("dialog-1").is_none());
    assert!(cx.debug_bounds("history-panel").is_some());
    assert_eq!(model.read_with(cx, |m, _| m.history_len()), 50);
    click(cx, "history-search-input").unwrap();
    for _ in 0..30 {
        cx.simulate_keystrokes("down");
    }
    let target = model.read_with(cx, |m, _| m.history()[30].clone());
    let header = cx.debug_bounds("finder-header").unwrap();
    let footer = cx.debug_bounds("history-footer").unwrap();
    assert!(cx.debug_bounds("history-item-30").is_some());
    assert!(
        cx.debug_bounds("history-item-0").is_none(),
        "Kit list virtualizes scrolled rows"
    );
    seed(&path, 1); // A newer entry shifts indexes; selection must retain the old domain ID.
    click(cx, "history-refresh-button").unwrap();
    cx.run_until_parked();
    assert_eq!(header, cx.debug_bounds("finder-header").unwrap());
    assert_eq!(footer, cx.debug_bounds("history-footer").unwrap());
    click(cx, "history-search-input").unwrap();
    cx.simulate_keystrokes("enter");
    model.read_with(cx, |m, _| {
        assert_eq!(m.active_request().unwrap().url(), target.request.url);
        assert_eq!(m.history_len(), 50);
        assert_eq!(m.tab_count(), 1);
    });
    assert!(cx.debug_bounds("history-panel").is_none());
    // A fresh opening starts at the newest result, not the retained selection from last time.
    let newest = model.read_with(cx, |m, _| m.history()[0].request.url.clone());
    ui::open_history(cx);
    cx.simulate_keystrokes("tab enter");
    model.read_with(cx, |m, _| {
        assert_eq!(m.active_request().unwrap().url(), newest);
        assert_eq!(m.history_len(), 50);
    });
}

#[gpui::test]
fn dialogs_fit_all_reference_sizes_and_themes_and_reopen_without_stacking(cx: &mut TestAppContext) {
    use gpui::{px, size};
    use gpui_kit::component::ThemeMode;
    let model = cx.new(|_| WorkspaceViewModel::new());
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| PostmanApp::with_view_model(model, w, cx))
    });
    ui::open_http(cx);
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        cx.update(|_, cx| postman_gpui::ui::theme::apply(mode, cx));
        for (width, height) in [(1440., 960.), (1920., 1080.), (1024., 768.), (960., 640.)] {
            let handle = cx.update(|w, _| w.window_handle());
            cx.simulate_window_resize(handle, size(px(width), px(height)));
            for (trigger, surface, close) in [
                ("rail-history", "history-panel", "history-close"),
                (
                    "global-search-trigger",
                    "global-search-dialog",
                    "global-search-close",
                ),
                (
                    "shortcut-help-button",
                    "shortcut-help-dialog",
                    "shortcut-help-close",
                ),
            ] {
                for _ in 0..2 {
                    click(cx, trigger).unwrap();
                    assert!(cx.debug_bounds(surface).is_some());
                    let dialog = cx.debug_bounds("dialog-0").unwrap();
                    let close_bounds = cx.debug_bounds(close).unwrap();
                    assert!(dialog.left() >= px(0.) && dialog.right() <= px(width));
                    assert!(dialog.top() >= px(0.) && dialog.bottom() <= px(height));
                    assert!(
                        close_bounds.top() >= dialog.top()
                            && close_bounds.bottom() <= dialog.bottom()
                    );
                    assert!(cx.debug_bounds("dialog-1").is_none());
                    click(cx, close).unwrap();
                    assert!(cx.debug_bounds(surface).is_none());
                }
                click(cx, trigger).unwrap();
                cx.simulate_click(gpui::point(px(100.), px(200.)), gpui::Modifiers::none());
                assert!(
                    cx.debug_bounds(surface).is_none(),
                    "backdrop dismisses {surface}"
                );
            }
        }
    }
}

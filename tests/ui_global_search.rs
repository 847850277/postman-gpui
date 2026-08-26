//! Issue #70 acceptance coverage for application-wide request and History search.

#[path = "common/ui.rs"]
mod ui;

use chrono::{Duration, Utc};
use gpui::{AppContext, TestAppContext};
use postman_gpui::{
    app::{PostmanApp, WorkspaceViewModel},
    models::{HistoryEntry, HttpMethod, Request},
    persistence::{
        HistoryRepository, SqliteHistoryRepository, VersionedHistorySnapshot,
        DEFAULT_HISTORY_RETENTION_LIMIT,
    },
};
use std::path::Path;
use ui::{choose_method, click, replace_text, type_into};

fn seed_history(path: &Path, entries: &[HistoryEntry]) {
    let mut repository = SqliteHistoryRepository::new(path).unwrap();
    repository.initialize().unwrap();
    for entry in entries.iter().rev() {
        let snapshot = VersionedHistorySnapshot::try_from(entry).unwrap();
        repository
            .append_and_trim(&snapshot, DEFAULT_HISTORY_RETENTION_LIMIT)
            .unwrap();
    }
}

#[gpui::test]
fn global_search_filters_groups_and_executes_mouse_commands(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("global-search.sqlite3");
    let now = Utc::now();
    let mut form_history = HistoryEntry::completed(
        Request::new(HttpMethod::POST, "https://history.example/httpbingo/form"),
        "Archived HTTPBingo form submit".into(),
        200,
        8,
        1024,
    );
    form_history.timestamp = now;
    let mut json_history = HistoryEntry::completed(
        Request::new(HttpMethod::GET, "https://history.example/httpbingo/json"),
        "Archived HTTPBingo JSON response".into(),
        200,
        5,
        421,
    );
    json_history.timestamp = now - Duration::seconds(1);
    seed_history(&database_path, &[form_history.clone(), json_history]);

    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |_window, cx| {
        PostmanApp::with_view_model_and_history_path(observed, database_path, cx)
    });
    cx.run_until_parked();

    type_into(cx, "url-input", "https://alpha.example/users").unwrap();
    click(cx, "new-tab-button").unwrap();
    choose_method(cx, "POST").unwrap();
    type_into(cx, "url-input", "https://beta.example/httpbingo/orders").unwrap();

    type_into(cx, "global-search-input", "HTTPBINGO").unwrap();
    assert!(cx.debug_bounds("global-search-popover").is_some());
    assert!(cx.debug_bounds("global-search-requests-group").is_some());
    assert!(cx.debug_bounds("global-search-history-group").is_some());
    assert!(cx.debug_bounds("global-search-request-result-0").is_some());
    assert!(cx.debug_bounds("global-search-request-result-1").is_none());
    assert!(cx.debug_bounds("global-search-history-result-0").is_some());
    assert!(cx.debug_bounds("global-search-history-result-1").is_some());

    click(cx, "global-search-request-result-0").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_tab_index()),
        1
    );
    assert!(cx.debug_bounds("global-search-popover").is_none());

    type_into(cx, "global-search-input", "archived httpbingo form").unwrap();
    assert!(cx.debug_bounds("global-search-requests-group").is_none());
    assert!(cx.debug_bounds("global-search-history-group").is_some());
    click(cx, "global-search-history-result-0").unwrap();
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.active_tab_index(), 1);
        assert_eq!(workspace.url(), form_history.request.url);
        assert_eq!(
            workspace.response().historical_entry_id(),
            Some(form_history.id.as_str())
        );
        assert_eq!(workspace.tab_count(), 2);
        assert_eq!(workspace.history_len(), 2);
    });
}

#[gpui::test]
fn global_search_keyboard_selection_empty_clear_and_escape_restore_focus(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("global-search-keyboard.sqlite3");
    let archived = HistoryEntry::completed(
        Request::new(HttpMethod::GET, "https://shared.example/archive"),
        "Shared archived request".into(),
        200,
        4,
        16,
    );
    seed_history(&database_path, std::slice::from_ref(&archived));

    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |_window, cx| {
        PostmanApp::with_view_model_and_history_path(observed, database_path, cx)
    });
    cx.run_until_parked();

    type_into(cx, "url-input", "https://shared.example/open").unwrap();
    click(cx, "new-tab-button").unwrap();
    choose_method(cx, "POST").unwrap();
    type_into(cx, "url-input", "https://method.example/orders").unwrap();

    cx.simulate_keystrokes("ctrl-k");
    cx.simulate_input("pOsT");
    assert!(cx.debug_bounds("global-search-requests-group").is_some());
    assert!(cx.debug_bounds("global-search-history-group").is_none());
    cx.simulate_keystrokes("escape");
    assert!(cx.debug_bounds("global-search-popover").is_none());

    click(cx, "url-input").unwrap();
    cx.simulate_keystrokes("ctrl-k");
    cx.simulate_input("shared.example");
    assert!(cx.debug_bounds("global-search-requests-group").is_some());
    assert!(cx.debug_bounds("global-search-history-group").is_some());
    cx.simulate_keystrokes("down enter");
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.active_tab_index(), 1);
        assert_eq!(workspace.url(), archived.request.url);
        assert_eq!(
            workspace.response().historical_entry_id(),
            Some(archived.id.as_str())
        );
    });

    replace_text(cx, "url-input", "https://draft.example/base").unwrap();
    click(cx, "global-search-input").unwrap();
    cx.simulate_input("nothing-can-match-this-query");
    assert!(cx.debug_bounds("global-search-empty").is_some());
    let tab_count = workspace.read_with(cx, |workspace, _| workspace.tab_count());
    let history_count = workspace.read_with(cx, |workspace, _| workspace.history_len());
    click(cx, "global-search-empty-clear").unwrap();
    assert!(cx.debug_bounds("global-search-popover").is_none());
    assert_eq!(
        workspace.read_with(cx, |workspace, _| (
            workspace.tab_count(),
            workspace.history_len()
        )),
        (tab_count, history_count)
    );

    cx.simulate_input("second-empty-query");
    assert!(cx.debug_bounds("global-search-empty").is_some());
    cx.simulate_keystrokes("escape");
    assert!(cx.debug_bounds("global-search-popover").is_none());
    cx.simulate_input("/restored");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.url().to_string()),
        "https://draft.example/base/restored"
    );
}

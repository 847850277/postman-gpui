//! History deletion uses real Kit controls and a durable SQLite repository.
#[path = "common/ui.rs"]
mod ui;
use gpui::{AppContext, TestAppContext, VisualTestContext};
use gpui_kit::test::TestWindowExt;
use postman_gpui::{
    app::{HistoryStorageStage, HistoryStorageStatus, PostmanApp, WorkspaceViewModel},
    models::{HistoryEntry, HttpMethod, Request},
    persistence::{HistoryRepository, SqliteHistoryRepository, VersionedHistorySnapshot},
};
use std::path::Path;

fn seed(path: &Path, name: &str) -> HistoryEntry {
    // Multiple executions of the same URL must remain independently deletable.
    let entry = HistoryEntry::completed(
        Request::new(HttpMethod::GET, "https://httpbingo.org/get"),
        name.into(),
        200,
        10,
        0,
    );
    let mut repository = SqliteHistoryRepository::new(path).unwrap();
    repository.initialize().unwrap();
    repository
        .append_and_trim(&VersionedHistorySnapshot::try_from(&entry).unwrap(), 50)
        .unwrap();
    entry
}
fn stored_ids(path: &Path) -> Vec<String> {
    let mut repository = SqliteHistoryRepository::new(path).unwrap();
    repository.initialize().unwrap();
    repository
        .load_recent(50)
        .unwrap()
        .snapshots()
        .iter()
        .map(|s| s.as_v1().entry_id().to_owned())
        .collect()
}
fn delete_button(cx: &mut VisualTestContext, id: &str) {
    cx.update(|window, cx| window.click(format!("delete-history-{id}"), cx));
    cx.run_until_parked();
}
fn alert_button(cx: &mut VisualTestContext, id: &'static str) {
    cx.update(|window, cx| window.click(id, cx));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn delete_is_scoped_to_the_confirmed_id_and_never_opens_the_request(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite3");
    let target = seed(&path, "First run");
    let other = seed(&path, "Other run");
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
    ui::replace_text(cx, "url-input", "unfinished draft").unwrap();
    ui::open_history(cx);
    ui::replace_text(cx, "history-search-input", "First run").unwrap();
    delete_button(cx, &target.id);
    assert!(cx.debug_bounds("history-delete-confirmation").is_some());
    assert!(
        cx.debug_bounds("history-panel").is_some(),
        "delete must not activate the row"
    );
    assert_eq!(
        stored_ids(&path).len(),
        2,
        "opening confirmation does not delete"
    );
    alert_button(cx, "cancel");
    assert_eq!(stored_ids(&path).len(), 2);
    delete_button(cx, &target.id);
    ui::press(cx, "escape");
    assert!(cx.debug_bounds("history-delete-confirmation").is_none());
    assert_eq!(stored_ids(&path).len(), 2);
    // The delete control must be reachable and activatable through Kit's modal Tab traversal.
    for key in ["space", "enter"] {
        ui::click(cx, "history-search-input").unwrap();
        for _ in 0..8 {
            if cx.update(|window, _| {
                window
                    .find(format!("delete-history-{}", target.id))
                    .focused()
            }) == Some(true)
            {
                break;
            }
            ui::press(cx, "tab");
        }
        assert_eq!(
            cx.update(|window, _| window
                .find(format!("delete-history-{}", target.id))
                .focused()),
            Some(true)
        );
        ui::press(cx, key);
        assert!(cx.debug_bounds("history-delete-confirmation").is_some());
        assert!(cx.debug_bounds("history-panel").is_some());
        if key == "space" {
            alert_button(cx, "cancel");
        }
    }
    let newest = seed(&path, "Newest run"); // Changes durable row ordering during the confirmation.
    alert_button(cx, "ok");
    assert_eq!(stored_ids(&path), vec![newest.id.clone(), other.id.clone()]);
    model.read_with(cx, |m, _| {
        assert_eq!(
            m.history().iter().map(|e| e.id.clone()).collect::<Vec<_>>(),
            vec![newest.id, other.id]
        );
        assert_eq!(m.tab_count(), 1);
        assert_eq!(m.active_request().unwrap().url(), "unfinished draft");
        assert!(m.active_request().unwrap().is_dirty());
    });
    assert!(cx.debug_bounds("history-panel").is_some());
    assert!(cx.debug_bounds("history-empty-state").is_some());
    // Confirmed deletion moves focus out of the removed row, back to the filter.
    ui::press(
        cx,
        if cfg!(target_os = "macos") {
            "cmd-a"
        } else {
            "ctrl-a"
        },
    );
    cx.simulate_input("Other run");
    ui::press(cx, "enter");
    assert!(cx.debug_bounds("history-panel").is_none());
    model.read_with(cx, |m, _| assert_eq!(m.tab_count(), 2));
}

#[gpui_kit::test]
fn failed_delete_keeps_the_row_and_retry_removes_only_that_row(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite3");
    let target = seed(&path, "Target");
    let other = seed(&path, "Keep");
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
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_delete BEFORE DELETE ON history_entries BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
    ui::open_history(cx);
    delete_button(cx, &target.id);
    alert_button(cx, "ok");
    assert!(cx.debug_bounds("history-storage-error").is_some());
    assert_eq!(stored_ids(&path).len(), 2);
    model.read_with(cx, |m, _| {
        assert_eq!(m.history_len(), 2);
        assert!(matches!(
            m.history_storage_status(),
            HistoryStorageStatus::Error {
                stage: HistoryStorageStage::Delete,
                ..
            }
        ));
    });
    db.execute_batch("DROP TRIGGER fail_delete").unwrap();
    delete_button(cx, &target.id);
    alert_button(cx, "ok");
    assert_eq!(stored_ids(&path), vec![other.id]);
    model.read_with(cx, |m, _| assert_eq!(m.history_len(), 1));
    assert!(cx.debug_bounds("history-storage-error").is_none());
}

#[gpui_kit::test]
fn clear_requires_confirmation_and_keeps_saved_documents_and_open_drafts(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite3");
    seed(&path, "Visible run");
    seed(&path, "Hidden run");
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
    ui::replace_text(cx, "url-input", "https://httpbingo.org/get?saved=1").unwrap();
    ui::click(cx, "save-request").unwrap();
    ui::replace_text(cx, "saved-request-name", "Saved request").unwrap();
    ui::click(cx, "request-save-confirm").unwrap();
    cx.run_until_parked();
    ui::replace_text(cx, "url-input", "unfinished draft").unwrap();
    ui::open_history(cx);
    ui::replace_text(cx, "history-search-input", "Visible run").unwrap();
    ui::click(cx, "history-clear-button").unwrap();
    assert!(cx.debug_bounds("history-clear-confirmation").is_some());
    assert_eq!(stored_ids(&path).len(), 2);
    alert_button(cx, "cancel");
    assert_eq!(stored_ids(&path).len(), 2);
    ui::click(cx, "history-clear-button").unwrap();
    ui::press(cx, "escape");
    assert_eq!(stored_ids(&path).len(), 2);
    assert!(cx.debug_bounds("history-panel").is_some());
    ui::click(cx, "history-clear-button").unwrap();
    alert_button(cx, "ok");
    assert!(
        stored_ids(&path).is_empty(),
        "Clear includes history hidden by the filter"
    );
    model.read_with(cx, |m, _| {
        assert_eq!(m.history_len(), 0);
        assert_eq!(m.saved_requests().len(), 1);
        assert_eq!(
            m.saved_requests()[0].draft().url(),
            "https://httpbingo.org/get?saved=1"
        );
        assert_eq!(m.active_request().unwrap().url(), "unfinished draft");
        assert!(m.active_request().unwrap().is_dirty());
    });
}

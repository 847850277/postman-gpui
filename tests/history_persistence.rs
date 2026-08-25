//! Issue #130 application-lifecycle checks for SQLite-authoritative History.

#[path = "common/ui.rs"]
mod ui;

use gpui::{AppContext, TestAppContext};
use mockito::Matcher;
use postman_gpui::{
    app::{
        HistoryStorageStage, HistoryStorageStatus, PostmanApp, ResponseState, WorkspaceViewModel,
    },
    models::{HistoryEntry, HttpMethod, Request},
    persistence::{
        HistoryRepository, SqliteHistoryRepository, VersionedHistorySnapshot,
        DEFAULT_HISTORY_RETENTION_LIMIT,
    },
};
use std::path::Path;
use ui::{click, type_into};

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

fn stored_history_count(path: &Path) -> usize {
    let mut repository = SqliteHistoryRepository::new(path).unwrap();
    repository.initialize().unwrap();
    repository
        .load_recent(DEFAULT_HISTORY_RETENTION_LIMIT)
        .unwrap()
        .snapshots()
        .len()
}

#[gpui::test]
fn startup_loads_rows_from_sqlite_and_clear_requeries_the_database(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("history.sqlite3");
    let entry = HistoryEntry::completed(
        Request::new(HttpMethod::GET, "https://example.test/from-sqlite"),
        "from SQLite".to_string(),
        200,
        4,
        12,
    );
    seed_history(&database_path, &[entry]);

    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let app_database_path = database_path.clone();
    let (_app, cx) = cx.add_window_view(move |_window, cx| {
        PostmanApp::with_view_model_and_history_path(observed, app_database_path, cx)
    });
    cx.run_until_parked();

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.history_len(), 1);
        assert_eq!(
            workspace.history()[0].request.url,
            "https://example.test/from-sqlite"
        );
        assert_eq!(
            workspace.history_storage_status(),
            &HistoryStorageStatus::Ready { skipped_rows: 0 }
        );
    });
    assert!(cx.debug_bounds("history-storage-ready").is_some());

    click(cx, "history-item-0").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.url().to_string()),
        "https://example.test/from-sqlite"
    );

    click(cx, "history-clear-button").unwrap();
    cx.run_until_parked();

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.history_len(), 0);
        assert_eq!(
            workspace.history_storage_status(),
            &HistoryStorageStatus::Ready { skipped_rows: 0 }
        );
        assert_eq!(workspace.url(), "https://example.test/from-sqlite");
    });
    assert_eq!(stored_history_count(&database_path), 0);
}

#[gpui::test]
fn completed_response_is_sanitized_persisted_then_rendered_from_sqlite(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("history.sqlite3");
    let mut server = mockito::Server::new();
    let request = server
        .mock("GET", "/ok")
        .match_query(Matcher::UrlEncoded(
            "api_key".to_string(),
            "history-secret".to_string(),
        ))
        .with_status(200)
        .with_body("persisted")
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let app_database_path = database_path.clone();
    let (_app, cx) = cx.add_window_view(move |_window, cx| {
        PostmanApp::with_view_model_and_history_path(observed, app_database_path, cx)
    });
    cx.run_until_parked();

    let sent_url = format!("{}/ok?api_key=history-secret", server.url());
    type_into(cx, "url-input", &sent_url).unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    request.assert();
    workspace.read_with(cx, |workspace, _| {
        assert!(matches!(
            workspace.response(),
            ResponseState::Success { status: 200, body, .. } if body == "persisted"
        ));
        assert_eq!(workspace.history_len(), 1);
        assert_eq!(
            workspace.history()[0].request.url,
            format!("{}/ok", server.url())
        );
        assert!(!workspace.history()[0]
            .request
            .url
            .contains("history-secret"));
        assert_eq!(
            workspace.history_storage_status(),
            &HistoryStorageStatus::Ready { skipped_rows: 0 }
        );
    });
    assert_eq!(stored_history_count(&database_path), 1);
}

#[gpui::test]
fn append_failure_keeps_the_response_usable_without_a_volatile_history_row(
    cx: &mut TestAppContext,
) {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("history.sqlite3");
    seed_history(&database_path, &[]);
    rusqlite::Connection::open(&database_path)
        .unwrap()
        .execute_batch(
            r#"
            CREATE TRIGGER reject_history_append
            BEFORE INSERT ON history_entries
            BEGIN
                SELECT RAISE(ABORT, 'forced History append failure');
            END;
            "#,
        )
        .unwrap();
    let mut server = mockito::Server::new();
    let request = server
        .mock("GET", "/response-still-works")
        .with_status(202)
        .with_body("accepted")
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let app_database_path = database_path.clone();
    let (_app, cx) = cx.add_window_view(move |_window, cx| {
        PostmanApp::with_view_model_and_history_path(observed, app_database_path, cx)
    });
    cx.run_until_parked();

    type_into(
        cx,
        "url-input",
        &format!("{}/response-still-works", server.url()),
    )
    .unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    request.assert();
    workspace.read_with(cx, |workspace, _| {
        assert!(matches!(
            workspace.response(),
            ResponseState::Success { status: 202, body, .. } if body == "accepted"
        ));
        assert_eq!(workspace.history_len(), 0);
        assert!(matches!(
            workspace.history_storage_status(),
            HistoryStorageStatus::Error {
                stage: HistoryStorageStage::Append,
                ..
            }
        ));
    });
    assert_eq!(stored_history_count(&database_path), 0);
    assert!(cx.debug_bounds("history-storage-error").is_some());
}

#[gpui::test]
fn corrupt_database_shows_unavailable_state_without_an_in_memory_fallback(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("history.sqlite3");
    std::fs::write(&database_path, b"this is not a SQLite database").unwrap();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |_window, cx| {
        PostmanApp::with_view_model_and_history_path(observed, database_path, cx)
    });
    cx.run_until_parked();

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.history_len(), 0);
        assert!(matches!(
            workspace.history_storage_status(),
            HistoryStorageStatus::Error {
                stage: HistoryStorageStage::Initialize,
                ..
            }
        ));
    });
    assert!(cx.debug_bounds("history-storage-error").is_some());
}

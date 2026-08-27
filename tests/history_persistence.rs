//! Issues #130 and #136 application-lifecycle checks for SQLite-authoritative History.

#[path = "common/ui.rs"]
mod ui;

use gpui::{AppContext, ClipboardItem, TestAppContext};
use mockito::Matcher;
use postman_gpui::{
    app::{
        HistoryStorageStage, HistoryStorageStatus, PostmanApp, ResponseState, WorkspaceViewModel,
    },
    models::{HistoricalResponse, HistoricalResponseBody, HistoryEntry, HttpMethod, Request},
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
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .url()
            .to_string()),
        "https://example.test/from-sqlite"
    );
    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .response()
            .clone()),
        ResponseState::HistoricalUnavailable { .. }
    ));

    click(cx, "history-clear-button").unwrap();
    cx.run_until_parked();

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.history_len(), 0);
        assert_eq!(
            workspace.history_storage_status(),
            &HistoryStorageStatus::Ready { skipped_rows: 0 }
        );
        assert_eq!(
            workspace.active_request().unwrap().url(),
            "https://example.test/from-sqlite"
        );
        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::NotSent
        ));
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
            workspace.active_request().unwrap().response(),
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

    click(cx, "history-item-0").unwrap();
    workspace.read_with(cx, |workspace, _| {
        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::Historical { entry_id, response }
                if entry_id == &workspace.history()[0].id
                    && response.status == 200
                    && matches!(&response.body, HistoricalResponseBody::Text(body) if body == "persisted")
        ));
        assert!(workspace.active_request().unwrap().response_stored_cookies().is_empty());
    });
    assert!(cx.debug_bounds("response-historical-badge").is_some());
    assert!(cx.debug_bounds("response-copy-button").is_some());
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
            workspace.active_request().unwrap().response(),
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
fn historical_truncation_and_unsupported_body_states_are_rendered(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("history.sqlite3");
    let binary_body = "binary-body-must-not-reach-sqlite";
    let binary = HistoryEntry::completed(
        Request::new(HttpMethod::GET, "https://example.test/binary"),
        "binary".into(),
        200,
        3,
        binary_body.len(),
    )
    .with_historical_response(HistoricalResponse::completed(
        200,
        vec![("Content-Type".into(), "application/octet-stream".into())],
        binary_body.into(),
        3,
    ));
    let large_body =
        "x".repeat(postman_gpui::persistence::MAX_HISTORICAL_RESPONSE_PREVIEW_BYTES + 8);
    let truncated = HistoryEntry::completed(
        Request::new(HttpMethod::GET, "https://example.test/large"),
        "large".into(),
        200,
        4,
        large_body.len(),
    )
    .with_historical_response(HistoricalResponse::completed(
        200,
        vec![("Content-Type".into(), "text/plain".into())],
        large_body,
        4,
    ));
    seed_history(&database_path, &[binary, truncated]);

    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let app_database_path = database_path.clone();
    let (_app, cx) = cx.add_window_view(move |_window, cx| {
        PostmanApp::with_view_model_and_history_path(observed, app_database_path, cx)
    });
    cx.run_until_parked();

    let (binary_index, truncated_index) = workspace.read_with(cx, |workspace, _| {
        let binary_index = workspace
            .history()
            .iter()
            .position(|entry| entry.name == "binary")
            .unwrap();
        let truncated_index = workspace
            .history()
            .iter()
            .position(|entry| entry.name == "large")
            .unwrap();
        (binary_index, truncated_index)
    });

    if binary_index == 0 {
        click(cx, "history-item-0").unwrap();
    } else {
        click(cx, "history-item-1").unwrap();
    }
    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.active_request().unwrap().response().clone()),
        ResponseState::Historical { response, .. }
            if matches!(response.body, HistoricalResponseBody::Unsupported)
    ));
    assert!(cx
        .debug_bounds("response-historical-body-not-stored")
        .is_some());
    assert!(cx.debug_bounds("response-copy-button").is_none());

    if truncated_index == 0 {
        click(cx, "history-item-0").unwrap();
    } else {
        click(cx, "history-item-1").unwrap();
    }
    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.active_request().unwrap().response().clone()),
        ResponseState::Historical { response, .. }
            if matches!(response.body, HistoricalResponseBody::TruncatedText(_))
    ));
    assert!(cx.debug_bounds("response-historical-truncated").is_some());
    assert!(cx.debug_bounds("response-copy-button").is_some());
    cx.write_to_clipboard(ClipboardItem::new_string("sentinel".into()));
    click(cx, "response-copy-button").unwrap();
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .map(|preview| preview.len()),
        Some(postman_gpui::persistence::MAX_HISTORICAL_RESPONSE_PREVIEW_BYTES),
        "Historical Copy must expose only the persisted preview"
    );
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

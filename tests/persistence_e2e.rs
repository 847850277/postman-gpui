//! Issues #131 and #136 deterministic persistence E2E coverage.
//!
//! Every test uses the real `PostmanApp`, `RequestExecutor`, repository worker, and a unique
//! file-backed SQLite database. Public HTTP services and the production History path are never
//! used.

#[path = "common/ui.rs"]
mod ui;

use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::{TimeZone, Utc};
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
use mockito::Matcher;
use postman_gpui::{
    app::{
        AuthorizationKind, BodyKind, HistoryStorageStage, HistoryStorageStatus,
        MultipartDraftValue, PostmanApp, RequestBodyDraft, ResponseState, WorkspaceViewModel,
    },
    models::{
        HistoricalResponseBody, HistoryEntry, HttpMethod, MultipartEditorPart, MultipartPart,
        MultipartValue, Request, RequestBody, RequestEditorIntent,
    },
    persistence::{
        HistoryLoadResult, HistoryRepository, SqliteHistoryRepository, VersionedHistorySnapshot,
        DEFAULT_HISTORY_RETENTION_LIMIT,
    },
};
use rusqlite::{params, Connection};
use std::{
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Condvar, Mutex},
    time::{Duration, Instant},
};
use ui::{choose_method, click, click_without_wait, replace_text, type_into};

const BASE_TIMESTAMP_MS: i64 = 1_787_529_600_000;

fn launch_app<'a>(
    cx: &'a mut TestAppContext,
    database_path: &Path,
) -> (
    Entity<PostmanApp>,
    Entity<WorkspaceViewModel>,
    &'a mut VisualTestContext,
) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let app_database_path = database_path.to_path_buf();
    let (app, cx) = cx.add_window_view(move |_window, cx| {
        PostmanApp::with_view_model_and_history_path(observed, app_database_path, cx)
    });
    cx.run_until_parked();
    (app, workspace, cx)
}

fn close_app(app: Entity<PostmanApp>, cx: &mut VisualTestContext) {
    cx.update(|window, _| window.remove_window());
    cx.run_until_parked();
    drop(app);
}

fn seed_history(path: &Path, entries_newest_first: &[HistoryEntry]) {
    let mut repository = SqliteHistoryRepository::new(path).unwrap();
    repository.initialize().unwrap();
    for entry in entries_newest_first.iter().rev() {
        let snapshot = VersionedHistorySnapshot::try_from(entry).unwrap();
        repository
            .append_and_trim(&snapshot, DEFAULT_HISTORY_RETENTION_LIMIT)
            .unwrap();
    }
}

fn load_history_result(path: &Path) -> HistoryLoadResult {
    let mut repository = SqliteHistoryRepository::new(path).unwrap();
    repository
        .load_recent(DEFAULT_HISTORY_RETENTION_LIMIT)
        .unwrap()
}

fn load_history(path: &Path) -> Vec<HistoryEntry> {
    let result = load_history_result(path);
    assert!(
        result.warnings().is_empty(),
        "SQLite History should not skip rows: {:#?}",
        result.warnings()
    );
    result
        .into_parts()
        .0
        .into_iter()
        .map(HistoryEntry::try_from)
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

fn same_history_entry(left: &HistoryEntry, right: &HistoryEntry) -> bool {
    left.id == right.id
        && left.request == right.request
        && left.editor_intent == right.editor_intent
        && left.request_options == right.request_options
        && left.timestamp == right.timestamp
        && left.name == right.name
        && left.status == right.status
        && left.elapsed_ms == right.elapsed_ms
        && left.response_size == right.response_size
        && left.historical_response == right.historical_response
}

fn assert_visible_matches_sqlite(
    workspace: &Entity<WorkspaceViewModel>,
    cx: &mut VisualTestContext,
    path: &Path,
) -> Vec<HistoryEntry> {
    let stored = load_history(path);
    let visible = workspace.read_with(cx, |workspace, _| workspace.history().to_vec());
    assert_eq!(visible.len(), stored.len());
    for (visible, stored) in visible.iter().zip(&stored) {
        assert!(
            same_history_entry(visible, stored),
            "visible History must equal SQLite\n  visible: {visible:#?}\n  SQLite: {stored:#?}"
        );
    }
    stored
}

fn database_payload(path: &Path) -> String {
    let connection = Connection::open(path).unwrap();
    let mut statement = connection
        .prepare("SELECT snapshot_json FROM history_entries ORDER BY sequence")
        .unwrap();
    statement
        .query_map([], |row| row.get::<_, Vec<u8>>(0))
        .unwrap()
        .map(|payload| String::from_utf8(payload.unwrap()).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

fn database_files(path: &Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    for candidate in [
        path.to_path_buf(),
        PathBuf::from(format!("{}-wal", path.display())),
        PathBuf::from(format!("{}-shm", path.display())),
    ] {
        if candidate.is_file() {
            bytes.extend(std::fs::read(candidate).unwrap());
        }
    }
    bytes
}

fn contains_bytes(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

fn completed_entry(index: u64, url: impl Into<String>, status: u16) -> HistoryEntry {
    let mut entry = HistoryEntry::completed(
        Request::new(HttpMethod::GET, url.into()),
        format!("entry-{index}"),
        status,
        u128::from(index),
        usize::try_from(index).unwrap(),
    );
    entry.id = format!("00000000-0000-4000-8000-{index:012x}");
    entry.timestamp = Utc
        .timestamp_millis_opt(BASE_TIMESTAMP_MS + i64::try_from(index).unwrap())
        .single()
        .unwrap();
    entry
}

fn release(gate: &Arc<(Mutex<bool>, Condvar)>) {
    let (released, wake) = &**gate;
    *released.lock().expect("response gate should be usable") = true;
    wake.notify_all();
}

fn wait_for(
    cx: &mut VisualTestContext,
    description: &str,
    mut condition: impl FnMut(&mut VisualTestContext) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if condition(cx) {
            return;
        }
        let _ = cx.executor().tick();
        std::thread::yield_now();
    }
    panic!("timed out waiting for {description}");
}

#[gpui::test]
fn json_request_is_recovered_and_replayed_through_the_rendered_history_action(
    test_cx: &mut TestAppContext,
) {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("history.sqlite3");
    assert!(!database_path.exists());

    let json_body = r#"{"message":"body-secret-is-user-authored","active":true}"#;
    let response_body = "response-body-persisted-preview";
    let mut server = mockito::Server::new();
    let original = server
        .mock("POST", "/restart-json")
        .match_query(Matcher::Exact("tag=rust&api_key=query-secret".to_string()))
        .match_header("x-trace", "safe-trace")
        .match_header("authorization", "Bearer bearer-secret")
        .match_body(Matcher::Exact(json_body.to_string()))
        .with_status(201)
        .with_header("set-cookie", "session=jar-secret; Path=/")
        .with_body(response_body)
        .create();
    let replay = server
        .mock("POST", "/restart-json")
        .match_query(Matcher::Exact("tag=rust".to_string()))
        .match_header("x-trace", "safe-trace")
        .match_header("authorization", Matcher::Missing)
        .match_header("cookie", Matcher::Missing)
        .match_body(Matcher::Exact(json_body.to_string()))
        .with_status(202)
        .with_body("replayed")
        .create();

    let (first_app, first_workspace, cx) = launch_app(test_cx, &database_path);
    assert!(database_path.is_file());
    assert!(load_history(&database_path).is_empty());

    choose_method(cx, "POST").unwrap();
    type_into(
        cx,
        "url-input",
        &format!(
            "{}/restart-json?tag=rust&api_key=query-secret",
            server.url()
        ),
    )
    .unwrap();
    click(cx, "request-pane-headers").unwrap();
    type_into(cx, "row-key-input", "X-Trace").unwrap();
    type_into(cx, "row-value-input", "safe-trace").unwrap();
    click(cx, "request-pane-authorization").unwrap();
    type_into(cx, "authorization-input", "Bearer bearer-secret").unwrap();
    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-json").unwrap();
    replace_text(cx, "body-input", json_body).unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    original.assert();
    first_workspace.read_with(cx, |workspace, _| {
        assert!(matches!(
            workspace.response(),
            ResponseState::Success { status: 201, body, .. } if body == response_body
        ));
        assert_eq!(workspace.cookie_count(), 1);
        assert_eq!(
            workspace.history_storage_status(),
            &HistoryStorageStatus::Ready { skipped_rows: 0 }
        );
    });
    let persisted = assert_visible_matches_sqlite(&first_workspace, cx, &database_path);
    assert_eq!(persisted.len(), 1);
    let original_id = persisted[0].id.clone();
    assert_eq!(persisted[0].status, Some(201));
    assert_eq!(
        persisted[0].request.url,
        format!("{}/restart-json?tag=rust", server.url())
    );
    assert!(persisted[0]
        .request
        .headers
        .iter()
        .any(|(name, value)| name == "X-Trace" && value == "safe-trace"));
    assert!(persisted[0]
        .request
        .headers
        .iter()
        .all(|(name, _)| !name.eq_ignore_ascii_case("authorization")));
    assert_eq!(
        persisted[0].request.body,
        RequestBody::Json(json_body.to_string())
    );

    let payload = database_payload(&database_path);
    for denied in ["query-secret", "bearer-secret", "jar-secret"] {
        assert!(!payload.contains(denied), "SQLite leaked {denied}");
    }
    assert!(payload.contains(response_body));
    assert!(payload.contains("safe-trace"));
    assert!(payload.contains("body-secret-is-user-authored"));

    // Runtime-only workspace state must disappear with the first composition root.
    click(cx, "new-tab-button").unwrap();
    type_into(cx, "url-input", "https://unsent.example/runtime-only").unwrap();
    click(cx, "request-pane-scripts").unwrap();
    type_into(cx, "script-editor", "runtime-only-script()").unwrap();
    assert_eq!(
        first_workspace.read_with(cx, |workspace, _| workspace.tab_count()),
        2
    );
    close_app(first_app, cx);
    drop(first_workspace);

    let (second_app, second_workspace, cx) = launch_app(&mut cx.cx, &database_path);
    second_workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.tab_count(), 1);
        assert_eq!(workspace.active_tab_index(), 0);
        assert!(workspace.url().is_empty());
        assert!(workspace.pre_request_script().is_empty());
        assert!(matches!(workspace.response(), ResponseState::NotSent));
        assert_eq!(workspace.active_request_id(), None);
        assert_eq!(workspace.in_flight_count(), 0);
        assert_eq!(workspace.cookie_count(), 0);
        assert_eq!(workspace.history_len(), 1);
    });
    let recovered = assert_visible_matches_sqlite(&second_workspace, cx, &database_path);
    assert_eq!(recovered[0].id, original_id);
    assert!(cx.debug_bounds("history-item-0").is_some());

    click(cx, "history-item-0").unwrap();
    second_workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.method(), HttpMethod::POST);
        assert_eq!(
            workspace.url(),
            format!("{}/restart-json?tag=rust", server.url())
        );
        assert_eq!(workspace.authorization_kind(), AuthorizationKind::Bearer);
        assert!(workspace.bearer_token().is_empty());
        assert!(workspace.basic_username().is_empty());
        assert!(workspace.basic_password().is_empty());
        assert_eq!(workspace.body_kind(), BodyKind::Json);
        assert_eq!(
            workspace.request_body(),
            RequestBody::Json(json_body.to_string())
        );
        assert!(matches!(
            workspace.response(),
            ResponseState::Historical { entry_id, response }
                if entry_id == &original_id
                    && response.status == 201
                    && matches!(&response.body, HistoricalResponseBody::Text(body) if body == response_body)
        ));
        assert!(workspace.response_stored_cookies().is_empty());
        assert_eq!(workspace.cookie_count(), 0);
    });
    assert!(cx.debug_bounds("response-historical-badge").is_some());
    assert!(cx.debug_bounds("response-copy-button").is_some());
    assert!(cx.debug_bounds("response-pane-cookies").is_none());

    type_into(cx, "history-search-input", "does-not-match-selected-row").unwrap();
    assert!(cx.debug_bounds("history-item-0").is_none());
    assert!(matches!(
        second_workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Historical { entry_id, .. } if entry_id == original_id
    ));
    replace_text(cx, "history-search-input", "").unwrap();

    click(cx, "send-button").unwrap();
    cx.run_until_parked();
    replay.assert();
    assert!(matches!(
        second_workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Success { status: 202, body, .. } if body == "replayed"
    ));
    let replayed = assert_visible_matches_sqlite(&second_workspace, cx, &database_path);
    assert_eq!(replayed.len(), 2);
    assert_ne!(replayed[0].id, original_id);
    assert_eq!(replayed[1].id, original_id);
    assert_eq!(replayed[0].request, replayed[1].request);

    close_app(second_app, cx);
}

#[gpui::test]
fn raw_and_urlencoded_bodies_recover_and_replay_after_restart(test_cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("history.sqlite3");
    let raw_body = "raw body survives SQLite restart";
    let encoded_body = "name=Ada+Lovelace&tag=rust";
    let mut server = mockito::Server::new();
    let raw = server
        .mock("POST", "/raw-replay")
        .match_body(Matcher::Exact(raw_body.to_string()))
        .with_status(200)
        .with_body("raw-ok")
        .expect(2)
        .create();
    let form = server
        .mock("POST", "/form-replay")
        .match_header("content-type", "application/x-www-form-urlencoded")
        .match_body(Matcher::Exact(encoded_body.to_string()))
        .with_status(200)
        .with_body("form-ok")
        .expect(2)
        .create();

    let (first_app, first_workspace, cx) = launch_app(test_cx, &database_path);
    choose_method(cx, "POST").unwrap();
    type_into(cx, "url-input", &format!("{}/raw-replay", server.url())).unwrap();
    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-raw").unwrap();
    replace_text(cx, "body-input", raw_body).unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    click(cx, "rail-new-request").unwrap();
    choose_method(cx, "POST").unwrap();
    type_into(cx, "url-input", &format!("{}/form-replay", server.url())).unwrap();
    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-url-encoded").unwrap();
    type_into(cx, "body-form-key-0", "name").unwrap();
    type_into(cx, "body-form-value-0", "Ada Lovelace").unwrap();
    click(cx, "body-form-add-row").unwrap();
    type_into(cx, "body-form-key-1", "tag").unwrap();
    type_into(cx, "body-form-value-1", "rust").unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    let original = assert_visible_matches_sqlite(&first_workspace, cx, &database_path);
    assert_eq!(original.len(), 2);
    assert_eq!(
        original[0].request.body,
        RequestBody::UrlEncoded(encoded_body.to_string())
    );
    assert_eq!(
        original[1].request.body,
        RequestBody::Raw(raw_body.to_string())
    );
    let form_id = original[0].id.clone();
    let raw_id = original[1].id.clone();

    close_app(first_app, cx);
    drop(first_workspace);
    let (second_app, second_workspace, cx) = launch_app(&mut cx.cx, &database_path);
    assert_visible_matches_sqlite(&second_workspace, cx, &database_path);

    click(cx, "history-item-1").unwrap();
    second_workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.body_kind(), BodyKind::Raw);
        assert_eq!(
            workspace.request_body(),
            RequestBody::Raw(raw_body.to_string())
        );
        assert!(matches!(
            workspace.response(),
            ResponseState::Historical { entry_id, response }
                if entry_id == &raw_id
                    && matches!(&response.body, HistoricalResponseBody::Text(body) if body == "raw-ok")
        ));
    });
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    // The original URL-encoded entry remains index 1 after the new raw replay is inserted.
    click(cx, "history-item-1").unwrap();
    second_workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.body_kind(), BodyKind::UrlEncoded);
        assert_eq!(
            workspace.request_body(),
            RequestBody::UrlEncoded(encoded_body.to_string())
        );
        assert!(matches!(
            workspace.response(),
            ResponseState::Historical { entry_id, response }
                if entry_id == &form_id
                    && matches!(&response.body, HistoricalResponseBody::Text(body) if body == "form-ok")
        ));
    });
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    raw.assert();
    form.assert();
    let replayed = assert_visible_matches_sqlite(&second_workspace, cx, &database_path);
    assert_eq!(replayed.len(), 4);
    assert_eq!(replayed[2].id, form_id);
    assert_eq!(replayed[3].id, raw_id);
    let unique_ids = replayed
        .iter()
        .map(|entry| entry.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(unique_ids.len(), replayed.len());

    close_app(second_app, cx);
}

#[gpui::test]
fn multipart_file_and_editor_intent_recover_and_replay_after_restart(test_cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("history.sqlite3");
    let fixture_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/httpbingo-upload.txt");
    let fixture_contents = std::fs::read_to_string(&fixture_path).unwrap();
    let expected_body = RequestBody::Multipart(vec![
        MultipartPart::text("note", "persisted multipart"),
        MultipartPart {
            name: "upload".to_string(),
            value: MultipartValue::File {
                path: fixture_path.clone(),
                file_name: Some("httpbingo-upload.txt".to_string()),
                content_type: Some("text/plain".to_string()),
            },
        },
    ]);
    let expected_intent = RequestEditorIntent::Multipart(vec![
        MultipartEditorPart {
            enabled: true,
            name: "note".to_string(),
            value: MultipartValue::Text("persisted multipart".to_string()),
        },
        MultipartEditorPart {
            enabled: true,
            name: "upload".to_string(),
            value: MultipartValue::File {
                path: fixture_path.clone(),
                file_name: Some("httpbingo-upload.txt".to_string()),
                content_type: Some("text/plain".to_string()),
            },
        },
        MultipartEditorPart {
            enabled: false,
            name: "disabled-note".to_string(),
            value: MultipartValue::Text("editor-only".to_string()),
        },
    ]);
    let mut server = mockito::Server::new();
    let upload = server
        .mock("POST", "/multipart-replay")
        .match_header(
            "content-type",
            Matcher::Regex("^multipart/form-data; boundary=".to_string()),
        )
        .match_body(Matcher::AllOf(vec![
            Matcher::Regex(
                "(?s)name=\"note\".*persisted multipart.*name=\"upload\"; filename=\"httpbingo-upload.txt\""
                    .to_string(),
            ),
            Matcher::Regex("hello from postman-gpui fixture".to_string()),
        ]))
        .with_status(201)
        .with_body("multipart-ok")
        .expect(2)
        .create();

    let (first_app, first_workspace, cx) = launch_app(test_cx, &database_path);
    choose_method(cx, "POST").unwrap();
    type_into(
        cx,
        "url-input",
        &format!("{}/multipart-replay", server.url()),
    )
    .unwrap();
    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-form-data").unwrap();
    type_into(cx, "body-form-key-0", "note").unwrap();
    type_into(cx, "body-form-value-0", "persisted multipart").unwrap();

    click(cx, "body-form-add-row").unwrap();
    type_into(cx, "body-form-key-1", "upload").unwrap();
    click(cx, "body-form-type-1").unwrap();
    click(cx, "body-form-file-1").unwrap();
    let selected = fixture_path.clone();
    cx.simulate_path_prompt_response(move |_| Some(vec![selected]));
    cx.run_until_parked();

    click(cx, "body-form-add-row").unwrap();
    type_into(cx, "body-form-key-2", "disabled-note").unwrap();
    type_into(cx, "body-form-value-2", "editor-only").unwrap();
    click(cx, "body-form-toggle-2").unwrap();
    click(cx, "body-form-add-row").unwrap();
    assert!(cx.debug_bounds("body-form-row-3").is_some());

    click(cx, "send-button").unwrap();
    cx.run_until_parked();
    let original = assert_visible_matches_sqlite(&first_workspace, cx, &database_path);
    assert_eq!(original.len(), 1);
    assert_eq!(original[0].request.body, expected_body);
    assert_eq!(original[0].editor_intent, Some(expected_intent.clone()));
    let payload = database_payload(&database_path);
    assert!(payload.contains("httpbingo-upload.txt"));
    assert!(payload.contains("editor-only"));
    assert!(
        !payload.contains(fixture_contents.trim()),
        "multipart file bytes must not be copied into SQLite"
    );

    close_app(first_app, cx);
    drop(first_workspace);
    let (second_app, second_workspace, cx) = launch_app(&mut cx.cx, &database_path);
    let recovered = assert_visible_matches_sqlite(&second_workspace, cx, &database_path);
    assert_eq!(recovered[0].editor_intent, Some(expected_intent.clone()));
    click(cx, "history-item-0").unwrap();
    second_workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.body_kind(), BodyKind::Multipart);
        assert_eq!(workspace.request_body(), expected_body);
        assert_eq!(
            workspace.request_editor_intent(),
            Some(expected_intent.clone())
        );
        let RequestBodyDraft::Multipart(parts) = workspace.body_draft() else {
            panic!("recovered multipart History should restore its typed editor");
        };
        assert_eq!(parts.len(), 3, "empty placeholder rows must not recover");
        assert!(!parts[2].enabled);
        assert!(matches!(
            &parts[1].value,
            MultipartDraftValue::File { path, file_name, content_type }
                if path == &fixture_path
                    && file_name.as_deref() == Some("httpbingo-upload.txt")
                    && content_type.as_deref() == Some("text/plain")
        ));
        assert!(matches!(
            workspace.response(),
            ResponseState::Historical { response, .. }
                if matches!(&response.body, HistoricalResponseBody::Text(body) if body == "multipart-ok")
        ));
    });
    assert!(cx.debug_bounds("body-form-file-metadata-1").is_some());
    assert!(cx.debug_bounds("body-form-omitted-2").is_some());

    click(cx, "send-button").unwrap();
    cx.run_until_parked();
    upload.assert();
    let replayed = assert_visible_matches_sqlite(&second_workspace, cx, &database_path);
    assert_eq!(replayed.len(), 2);
    assert_eq!(replayed[0].request.body, expected_body);
    assert_eq!(replayed[1].request.body, expected_body);

    close_app(second_app, cx);
}

#[gpui::test]
fn missing_multipart_file_after_restart_uses_the_normal_validation_error(
    test_cx: &mut TestAppContext,
) {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("history.sqlite3");
    let upload_path = directory.path().join("removed-after-restart.txt");
    std::fs::write(&upload_path, "available for the first request").unwrap();
    let mut server = mockito::Server::new();
    let first_upload = server
        .mock("POST", "/missing-after-restart")
        .match_body(Matcher::Regex(
            "available for the first request".to_string(),
        ))
        .with_status(200)
        .with_body("stored")
        .create();

    let (first_app, first_workspace, cx) = launch_app(test_cx, &database_path);
    choose_method(cx, "POST").unwrap();
    type_into(
        cx,
        "url-input",
        &format!("{}/missing-after-restart", server.url()),
    )
    .unwrap();
    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-form-data").unwrap();
    type_into(cx, "body-form-key-0", "upload").unwrap();
    click(cx, "body-form-type-0").unwrap();
    click(cx, "body-form-file-0").unwrap();
    let selected = upload_path.clone();
    cx.simulate_path_prompt_response(move |_| Some(vec![selected]));
    cx.run_until_parked();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();
    first_upload.assert();
    assert_eq!(
        assert_visible_matches_sqlite(&first_workspace, cx, &database_path).len(),
        1
    );

    close_app(first_app, cx);
    drop(first_workspace);
    std::fs::remove_file(&upload_path).unwrap();

    let (second_app, second_workspace, cx) = launch_app(&mut cx.cx, &database_path);
    click(cx, "history-item-0").unwrap();
    second_workspace.read_with(cx, |workspace, _| {
        let RequestBodyDraft::Multipart(parts) = workspace.body_draft() else {
            panic!("missing replay file should remain a typed multipart row");
        };
        assert!(matches!(
            &parts[0].value,
            MultipartDraftValue::File { path, .. } if path == &upload_path
        ));
    });
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    match second_workspace.read_with(cx, |workspace, _| workspace.response().clone()) {
        ResponseState::Error { message } => {
            assert!(message.contains("failed to read multipart file"));
            assert!(message.contains("field `upload`"));
            assert!(message.contains("removed-after-restart.txt"));
        }
        other => panic!("missing replay file should fail normally, got {other:?}"),
    }
    assert!(cx.debug_bounds("body-multipart-file-error").is_some());
    assert_eq!(
        assert_visible_matches_sqlite(&second_workspace, cx, &database_path).len(),
        1,
        "a missing replay file must not fabricate a second History row"
    );

    close_app(second_app, cx);
}

#[gpui::test]
fn completed_response_is_usable_before_locked_sqlite_append_commits(test_cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("history.sqlite3");
    seed_history(&database_path, &[]);
    let mut server = mockito::Server::new();
    let completed = server
        .mock("GET", "/async-persistence")
        .with_status(200)
        .with_body("response-before-commit")
        .create();

    let (app, workspace, cx) = launch_app(test_cx, &database_path);
    let blocker = Connection::open(&database_path).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();

    type_into(
        cx,
        "url-input",
        &format!("{}/async-persistence", server.url()),
    )
    .unwrap();
    click_without_wait(cx, "send-button").unwrap();
    wait_for(cx, "the HTTP response while SQLite is locked", |cx| {
        workspace.read_with(cx, |workspace, _| {
            matches!(
                workspace.response(),
                ResponseState::Success { status: 200, body, .. }
                    if body == "response-before-commit"
            )
        })
    });

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.history_len(), 0);
        assert!(matches!(
            workspace.history_storage_status(),
            HistoryStorageStatus::Loading {
                stage: HistoryStorageStage::Append
            }
        ));
    });
    assert!(load_history(&database_path).is_empty());
    assert!(cx.debug_bounds("response-content").is_some());
    assert!(cx.debug_bounds("history-item-0").is_none());

    blocker.execute_batch("ROLLBACK").unwrap();
    drop(blocker);
    cx.run_until_parked();
    completed.assert();
    assert_eq!(
        assert_visible_matches_sqlite(&workspace, cx, &database_path).len(),
        1
    );
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(
            workspace.history_storage_status(),
            &HistoryStorageStatus::Ready { skipped_rows: 0 }
        );
    });

    close_app(app, cx);
}

#[gpui::test]
fn non_2xx_recovers_but_cancel_timeout_and_transport_failures_never_persist(
    test_cx: &mut TestAppContext,
) {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("history.sqlite3");
    let mut server = mockito::Server::new();
    let teapot = server
        .mock("GET", "/teapot")
        .with_status(418)
        .with_body("short and stout")
        .create();

    let (cancel_started_tx, cancel_started_rx) = mpsc::channel();
    let cancel_gate = Arc::new((Mutex::new(false), Condvar::new()));
    let cancel_response_gate = cancel_gate.clone();
    let cancelled = server
        .mock("GET", "/cancel")
        .with_chunked_body(move |writer| {
            writer.write_all(b"started")?;
            let _ = cancel_started_tx.send(());
            let (released, wake) = &*cancel_response_gate;
            let released = released.lock().expect("cancel gate should be usable");
            let _ = wake
                .wait_timeout_while(released, Duration::from_secs(2), |released| !*released)
                .expect("cancel gate should remain usable");
            Ok(())
        })
        .create();

    let timeout_gate = Arc::new((Mutex::new(false), Condvar::new()));
    let timeout_response_gate = timeout_gate.clone();
    let timed_out = server
        .mock("GET", "/timeout")
        .with_chunked_body(move |writer| {
            writer.write_all(b"started")?;
            let (released, wake) = &*timeout_response_gate;
            let released = released.lock().expect("timeout gate should be usable");
            let _ = wake
                .wait_timeout_while(released, Duration::from_secs(2), |released| !*released)
                .expect("timeout gate should remain usable");
            Ok(())
        })
        .create();

    let (first_app, first_workspace, cx) = launch_app(test_cx, &database_path);
    type_into(cx, "url-input", &format!("{}/teapot", server.url())).unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();
    assert!(matches!(
        first_workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Success { status: 418, .. }
    ));
    assert_eq!(
        assert_visible_matches_sqlite(&first_workspace, cx, &database_path).len(),
        1
    );

    click(cx, "rail-new-request").unwrap();
    type_into(cx, "url-input", &format!("{}/cancel", server.url())).unwrap();
    click_without_wait(cx, "send-button").unwrap();
    cancel_started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("cancel request should reach the local server");
    wait_for(cx, "the rendered Cancel state", |cx| {
        cx.debug_bounds("cancel-send-control").is_some()
    });
    click_without_wait(cx, "send-button").unwrap();
    release(&cancel_gate);
    cx.run_until_parked();
    assert!(matches!(
        first_workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Cancelled
    ));
    assert_eq!(load_history(&database_path).len(), 1);

    click(cx, "rail-new-request").unwrap();
    type_into(cx, "url-input", &format!("{}/timeout", server.url())).unwrap();
    click(cx, "request-pane-options").unwrap();
    type_into(cx, "request-timeout-input", "40").unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();
    release(&timeout_gate);
    cx.run_until_parked();
    assert!(matches!(
        first_workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Error { message } if message == "Request timed out after 40 ms"
    ));
    assert_eq!(load_history(&database_path).len(), 1);

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let unused_address = listener.local_addr().unwrap();
    drop(listener);
    click(cx, "rail-new-request").unwrap();
    type_into(
        cx,
        "url-input",
        &format!("http://{unused_address}/connection-failure"),
    )
    .unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();
    assert!(matches!(
        first_workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Error { .. }
    ));
    assert_eq!(
        assert_visible_matches_sqlite(&first_workspace, cx, &database_path).len(),
        1
    );

    teapot.assert();
    cancelled.assert();
    timed_out.assert();
    close_app(first_app, cx);
    drop(first_workspace);

    let (second_app, second_workspace, cx) = launch_app(&mut cx.cx, &database_path);
    let recovered = assert_visible_matches_sqlite(&second_workspace, cx, &database_path);
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].status, Some(418));
    assert!(recovered[0].request.url.ends_with("/teapot"));
    assert!(matches!(
        second_workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::NotSent
    ));

    close_app(second_app, cx);
}

#[gpui::test]
fn retention_retry_and_search_use_the_latest_fifty_sqlite_rows(test_cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("history.sqlite3");
    let mut server = mockito::Server::new();
    let live = server
        .mock("GET", "/retention-live")
        .match_query(Matcher::Any)
        .with_status(200)
        .with_body("live")
        .expect(52)
        .create();
    let (app, workspace, cx) = launch_app(test_cx, &database_path);

    for sequence in 0..DEFAULT_HISTORY_RETENTION_LIMIT {
        let url = format!("{}/retention-live?sequence={sequence}", server.url());
        if sequence == 0 {
            type_into(cx, "url-input", &url).unwrap();
        } else {
            replace_text(cx, "url-input", &url).unwrap();
        }
        click(cx, "send-button").unwrap();
        wait_for(
            cx,
            "completed request to reach SQLite-backed History",
            |cx| {
                workspace.read_with(cx, |workspace, _| {
                    workspace
                        .history()
                        .first()
                        .is_some_and(|entry| entry.request.url == url)
                })
            },
        );
    }

    let initial = assert_visible_matches_sqlite(&workspace, cx, &database_path);
    assert_eq!(initial.len(), DEFAULT_HISTORY_RETENTION_LIMIT);
    assert!(initial[0].request.url.ends_with("?sequence=49"));
    assert!(initial[49].request.url.ends_with("?sequence=0"));
    let oldest_id = initial[49].id.clone();
    let second_oldest_id = initial[48].id.clone();
    let third_oldest_id = initial[47].id.clone();

    for sequence in 50..52 {
        let url = format!("{}/retention-live?sequence={sequence}", server.url());
        replace_text(cx, "url-input", &url).unwrap();
        click(cx, "send-button").unwrap();
        wait_for(cx, "trimmed request to reach SQLite-backed History", |cx| {
            workspace.read_with(cx, |workspace, _| {
                workspace
                    .history()
                    .first()
                    .is_some_and(|entry| entry.request.url == url)
            })
        });
    }
    live.assert();

    let retained = assert_visible_matches_sqlite(&workspace, cx, &database_path);
    assert_eq!(retained.len(), DEFAULT_HISTORY_RETENTION_LIMIT);
    let ids = retained
        .iter()
        .map(|entry| entry.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(ids.len(), DEFAULT_HISTORY_RETENTION_LIMIT);
    assert!(!ids.contains(oldest_id.as_str()));
    assert!(!ids.contains(second_oldest_id.as_str()));
    assert!(ids.contains(third_oldest_id.as_str()));
    assert!(retained[0].request.url.ends_with("?sequence=51"));
    assert!(retained[1].request.url.ends_with("?sequence=50"));

    // Requeueing the exact same stable snapshot is an idempotent persistence retry.
    let retry = VersionedHistorySnapshot::try_from(&retained[0]).unwrap();
    let before_retry_ids = retained
        .iter()
        .map(|entry| entry.id.clone())
        .collect::<Vec<_>>();
    let mut repository = SqliteHistoryRepository::new(&database_path).unwrap();
    repository
        .append_and_trim(&retry, DEFAULT_HISTORY_RETENTION_LIMIT)
        .unwrap();
    repository
        .append_and_trim(&retry, DEFAULT_HISTORY_RETENTION_LIMIT)
        .unwrap();
    let after_retry_ids = load_history(&database_path)
        .into_iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();
    assert_eq!(after_retry_ids, before_retry_ids);

    click(cx, "history-refresh-button").unwrap();
    cx.run_until_parked();
    assert_visible_matches_sqlite(&workspace, cx, &database_path);
    type_into(cx, "history-search-input", "sequence=51").unwrap();
    assert!(cx.debug_bounds("history-item-0").is_some());
    assert!(cx.debug_bounds("history-item-1").is_none());
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.history_len()),
        DEFAULT_HISTORY_RETENTION_LIMIT,
        "search must not replace the SQLite query result"
    );
    assert_eq!(
        load_history(&database_path).len(),
        DEFAULT_HISTORY_RETENTION_LIMIT
    );

    close_app(app, cx);
}

#[gpui::test]
fn successful_clear_stays_empty_after_restart(test_cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("history.sqlite3");
    let mut server = mockito::Server::new();
    let first_request = server
        .mock("GET", "/clear-one")
        .with_status(200)
        .expect(1)
        .create();
    let second_request = server
        .mock("GET", "/clear-two")
        .with_status(201)
        .expect(1)
        .create();

    let (first_app, first_workspace, cx) = launch_app(test_cx, &database_path);
    let first_url = format!("{}/clear-one", server.url());
    type_into(cx, "url-input", &first_url).unwrap();
    click(cx, "send-button").unwrap();
    wait_for(cx, "first request to reach SQLite-backed History", |cx| {
        first_workspace.read_with(cx, |workspace, _| {
            workspace
                .history()
                .first()
                .is_some_and(|entry| entry.request.url == first_url)
        })
    });
    let second_url = format!("{}/clear-two", server.url());
    replace_text(cx, "url-input", &second_url).unwrap();
    click(cx, "send-button").unwrap();
    wait_for(cx, "second request to reach SQLite-backed History", |cx| {
        first_workspace.read_with(cx, |workspace, _| {
            workspace
                .history()
                .first()
                .is_some_and(|entry| entry.request.url == second_url)
        })
    });
    first_request.assert();
    second_request.assert();
    assert_eq!(
        assert_visible_matches_sqlite(&first_workspace, cx, &database_path).len(),
        2
    );
    replace_text(cx, "url-input", "https://unsent.example/keep-editor").unwrap();
    click(cx, "history-clear-button").unwrap();
    cx.run_until_parked();
    first_workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.history_len(), 0);
        assert_eq!(workspace.url(), "https://unsent.example/keep-editor");
        assert_eq!(
            workspace.history_storage_status(),
            &HistoryStorageStatus::Ready { skipped_rows: 0 }
        );
    });
    assert!(load_history(&database_path).is_empty());

    close_app(first_app, cx);
    drop(first_workspace);
    let (second_app, second_workspace, cx) = launch_app(&mut cx.cx, &database_path);
    assert!(assert_visible_matches_sqlite(&second_workspace, cx, &database_path).is_empty());
    assert!(second_workspace.read_with(cx, |workspace, _| workspace.url().is_empty()));
    assert!(cx.debug_bounds("history-item-0").is_none());

    close_app(second_app, cx);
}

#[gpui::test]
fn failed_clear_keeps_sqlite_and_the_last_successful_query_result(test_cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("history.sqlite3");
    seed_history(
        &database_path,
        &[
            completed_entry(2, "https://example.test/two", 200),
            completed_entry(1, "https://example.test/one", 201),
        ],
    );
    let (first_app, first_workspace, cx) = launch_app(test_cx, &database_path);
    let before = assert_visible_matches_sqlite(&first_workspace, cx, &database_path);
    let connection = Connection::open(&database_path).unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TRIGGER reject_history_clear
            BEFORE DELETE ON history_entries
            BEGIN
                SELECT RAISE(ABORT, 'forced History clear failure');
            END;
            "#,
        )
        .unwrap();
    drop(connection);

    click(cx, "history-clear-button").unwrap();
    cx.run_until_parked();
    first_workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.history_len(), before.len());
        let HistoryStorageStatus::Error { stage, message } = workspace.history_storage_status()
        else {
            panic!("failed Clear should expose a typed storage error");
        };
        assert_eq!(*stage, HistoryStorageStage::Clear);
        assert!(message.contains("clear"));
        assert!(message.contains("forced History clear failure"));
    });
    let after = assert_visible_matches_sqlite(&first_workspace, cx, &database_path);
    assert_eq!(
        after.iter().map(|entry| &entry.id).collect::<Vec<_>>(),
        before.iter().map(|entry| &entry.id).collect::<Vec<_>>()
    );
    assert!(cx.debug_bounds("history-storage-error").is_some());

    close_app(first_app, cx);
    drop(first_workspace);
    let (second_app, second_workspace, cx) = launch_app(&mut cx.cx, &database_path);
    let recovered = assert_visible_matches_sqlite(&second_workspace, cx, &database_path);
    assert_eq!(recovered.len(), before.len());
    assert_eq!(
        recovered.iter().map(|entry| &entry.id).collect::<Vec<_>>(),
        before.iter().map(|entry| &entry.id).collect::<Vec<_>>()
    );

    close_app(second_app, cx);
}

#[gpui::test]
fn direct_sqlite_inspection_excludes_auth_api_keys_and_cookies_but_keeps_user_bodies(
    test_cx: &mut TestAppContext,
) {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("history.sqlite3");
    let body = r#"{"password":"body-secret-is-user-authored","message":"persist me"}"#;
    let basic_value = format!("Basic {}", STANDARD.encode("security-user:security-pass"));
    let mut server = mockito::Server::new();
    let bearer = server
        .mock("POST", "/security")
        .match_query(Matcher::Exact(
            "tag=safe&api_key=query-secret&X-Amz-Signature=signature-secret".to_string(),
        ))
        .match_header("authorization", "Bearer bearer-secret")
        .match_header("cookie", "manual=manual-cookie-secret")
        .match_header("x-api-key", "header-api-secret")
        .match_header("x-trace", "safe-trace")
        .match_body(Matcher::Exact(body.to_string()))
        .with_status(200)
        .with_header("set-cookie", "session=jar-secret; Path=/")
        .with_header("content-type", "application/json")
        .with_body(r#"{"password":"response-secret","message":"stored-response"}"#)
        .create();
    let basic = server
        .mock("GET", "/basic-security")
        .match_query(Matcher::Exact(
            "access_token=access-secret&project=gpui&opaque=looks-private-but-user-authored"
                .to_string(),
        ))
        .match_header("authorization", basic_value.as_str())
        .match_header("cookie", "session=jar-secret")
        .with_status(200)
        .with_body("basic-ok")
        .create();

    let (app, workspace, cx) = launch_app(test_cx, &database_path);
    choose_method(cx, "POST").unwrap();
    type_into(
        cx,
        "url-input",
        &format!(
            "{}/security?tag=safe&api_key=query-secret&X-Amz-Signature=signature-secret",
            server.url()
        ),
    )
    .unwrap();
    click(cx, "request-pane-headers").unwrap();
    type_into(cx, "row-key-input", "X-Trace").unwrap();
    type_into(cx, "row-value-input", "safe-trace").unwrap();
    click(cx, "add-row-button").unwrap();
    type_into(cx, "row-key-input", "cOoKiE").unwrap();
    type_into(cx, "row-value-input", "manual=manual-cookie-secret").unwrap();
    click(cx, "add-row-button").unwrap();
    type_into(cx, "row-key-input", "x-API-key").unwrap();
    type_into(cx, "row-value-input", "header-api-secret").unwrap();
    click(cx, "request-pane-authorization").unwrap();
    type_into(cx, "authorization-input", "Bearer bearer-secret").unwrap();
    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-json").unwrap();
    replace_text(cx, "body-input", body).unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    click(cx, "rail-new-request").unwrap();
    type_into(
        cx,
        "url-input",
        &format!(
            "{}/basic-security?access_token=access-secret&project=gpui&opaque=looks-private-but-user-authored",
            server.url()
        ),
    )
    .unwrap();
    click(cx, "request-pane-authorization").unwrap();
    click(cx, "auth-kind-basic").unwrap();
    type_into(cx, "basic-auth-username-input", "security-user").unwrap();
    type_into(cx, "basic-auth-password-input", "security-pass").unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    bearer.assert();
    basic.assert();
    let stored = assert_visible_matches_sqlite(&workspace, cx, &database_path);
    assert_eq!(stored.len(), 2);
    assert_eq!(
        stored[0].request.url,
        format!(
            "{}/basic-security?project=gpui&opaque=looks-private-but-user-authored",
            server.url()
        )
    );
    assert_eq!(
        stored[1].request.url,
        format!("{}/security?tag=safe", server.url())
    );
    for entry in &stored {
        assert!(entry.request.headers.iter().all(|(name, _)| {
            !name.eq_ignore_ascii_case("authorization")
                && !name.eq_ignore_ascii_case("cookie")
                && !name.eq_ignore_ascii_case("set-cookie")
                && !name.eq_ignore_ascii_case("x-api-key")
        }));
        let response = entry
            .historical_response
            .as_ref()
            .expect("V2 rows should include response evidence");
        assert!(response.headers.iter().all(|(name, _)| {
            !postman_gpui::persistence::HistorySensitiveDataPolicy::is_sensitive_header_name(name)
        }));
    }
    assert_eq!(stored[1].request.body, RequestBody::Json(body.to_string()));

    let database_bytes = database_files(&database_path);
    for denied in [
        "query-secret",
        "signature-secret",
        "bearer-secret",
        "manual-cookie-secret",
        "header-api-secret",
        "access-secret",
        "security-user",
        "security-pass",
        basic_value.as_str(),
        "jar-secret",
        "response-secret",
    ] {
        assert!(
            !contains_bytes(&database_bytes, denied),
            "SQLite files leaked {denied}"
        );
    }
    let payload = database_payload(&database_path);
    assert!(payload.contains("safe-trace"));
    assert!(payload.contains("project"));
    assert!(payload.contains("looks-private-but-user-authored"));
    assert!(payload.contains("body-secret-is-user-authored"));
    for row in payload.lines() {
        let value: serde_json::Value = serde_json::from_str(row).unwrap();
        assert_eq!(value["version"], serde_json::Value::from(2));
        let top_level = value.as_object().unwrap();
        assert_eq!(
            top_level
                .keys()
                .map(String::as_str)
                .collect::<std::collections::BTreeSet<_>>(),
            std::collections::BTreeSet::from(["snapshot", "version"])
        );
        let snapshot = value["snapshot"].as_object().unwrap();
        assert!(snapshot.contains_key("response"));
        assert!(!snapshot.contains_key("cookies"));
        assert!(!snapshot.contains_key("tabs"));
        assert!(!snapshot.contains_key("pending_request_id"));
    }

    close_app(app, cx);
}

#[gpui::test]
fn legacy_schema_migrates_and_a_malformed_newer_row_is_skipped_after_restart(
    test_cx: &mut TestAppContext,
) {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("legacy.sqlite3");
    let connection = Connection::open(&database_path).unwrap();
    connection
        .execute_batch(include_str!("fixtures/history-schema-v1.sql"))
        .unwrap();
    drop(connection);

    let (first_app, first_workspace, cx) = launch_app(test_cx, &database_path);
    let migrated = assert_visible_matches_sqlite(&first_workspace, cx, &database_path);
    assert_eq!(migrated.len(), 1);
    assert_eq!(migrated[0].name, "Migrated V1 row");
    assert_eq!(migrated[0].id, "00000000-0000-4000-8000-000000000129");
    assert_eq!(migrated[0].status, Some(200));
    let connection = Connection::open(&database_path).unwrap();
    let schema_version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    let snapshot_version: i64 = connection
        .query_row("SELECT snapshot_version FROM history_entries", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(
        schema_version,
        postman_gpui::persistence::CURRENT_HISTORY_SCHEMA_VERSION
    );
    assert_eq!(snapshot_version, 1);
    drop(connection);

    click(cx, "history-item-0").unwrap();
    first_workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.method(), HttpMethod::GET);
        assert_eq!(workspace.url(), "https://example.com/legacy");
        assert_eq!(
            workspace.headers(),
            &[postman_gpui::app::KeyValueRow::enabled(
                "X-Legacy",
                "preserved"
            )]
        );
        assert!(matches!(
            workspace.response(),
            ResponseState::HistoricalUnavailable { entry_id }
                if entry_id == "00000000-0000-4000-8000-000000000129"
        ));
    });
    assert!(cx.debug_bounds("response-historical-unavailable").is_some());
    close_app(first_app, cx);
    drop(first_workspace);

    let corrupt_id = "00000000-0000-4000-8000-000000000131";
    let connection = Connection::open(&database_path).unwrap();
    connection
        .execute(
            r#"
            INSERT INTO history_entries (
                entry_id, created_at_ms, snapshot_json, snapshot_version
            ) VALUES (?1, ?2, ?3, ?4)
            "#,
            params![
                corrupt_id,
                BASE_TIMESTAMP_MS + 10,
                b"not-json".as_slice(),
                1_i64
            ],
        )
        .unwrap();
    drop(connection);

    let (second_app, second_workspace, cx) = launch_app(&mut cx.cx, &database_path);
    let result = load_history_result(&database_path);
    assert_eq!(result.snapshots().len(), 1);
    assert_eq!(result.warnings().len(), 1);
    assert_eq!(result.warnings()[0].entry_id(), corrupt_id);
    second_workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.history_len(), 1);
        assert_eq!(workspace.history()[0].name, "Migrated V1 row");
        assert_eq!(
            workspace.history_storage_status(),
            &HistoryStorageStatus::Ready { skipped_rows: 1 }
        );
    });
    assert!(cx
        .debug_bounds("history-storage-ready-with-warnings")
        .is_some());

    close_app(second_app, cx);
}

#[gpui::test]
fn future_schema_is_not_downgraded_and_does_not_enable_a_volatile_fallback(
    test_cx: &mut TestAppContext,
) {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("future.sqlite3");
    let connection = Connection::open(&database_path).unwrap();
    connection
        .pragma_update(None, "user_version", 999_u32)
        .unwrap();
    drop(connection);
    let mut server = mockito::Server::new();
    let request = server
        .mock("GET", "/still-usable")
        .with_status(200)
        .with_body("usable")
        .create();

    let (app, workspace, cx) = launch_app(test_cx, &database_path);
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.history_len(), 0);
        let HistoryStorageStatus::Error { stage, message } = workspace.history_storage_status()
        else {
            panic!("future schema should fail initialization explicitly");
        };
        assert_eq!(*stage, HistoryStorageStage::Initialize);
        assert!(message.contains("999"));
        assert!(message.contains("newer than supported"));
    });
    assert!(cx.debug_bounds("history-storage-error").is_some());

    type_into(cx, "url-input", &format!("{}/still-usable", server.url())).unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();
    request.assert();
    workspace.read_with(cx, |workspace, _| {
        assert!(matches!(
            workspace.response(),
            ResponseState::Success { status: 200, body, .. } if body == "usable"
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
    let connection = Connection::open(&database_path).unwrap();
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    let table_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'history_entries'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, 999);
    assert_eq!(table_count, 0);

    close_app(app, cx);
}

#[gpui::test]
fn refresh_load_failure_keeps_the_last_successful_sqlite_projection(test_cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("history.sqlite3");
    seed_history(
        &database_path,
        &[completed_entry(
            1,
            "https://example.test/last-good-query",
            200,
        )],
    );
    let (app, workspace, cx) = launch_app(test_cx, &database_path);
    let before = assert_visible_matches_sqlite(&workspace, cx, &database_path);

    let connection = Connection::open(&database_path).unwrap();
    connection
        .execute_batch(
            r#"
            ALTER TABLE history_entries RENAME TO history_entries_last_good;
            CREATE TABLE history_entries (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                entry_id TEXT NOT NULL UNIQUE,
                created_at_ms TEXT NOT NULL,
                snapshot_json BLOB NOT NULL,
                snapshot_version INTEGER NOT NULL DEFAULT 1
            );
            INSERT INTO history_entries (
                sequence, entry_id, created_at_ms, snapshot_json, snapshot_version
            )
            SELECT sequence, entry_id, 'not-an-integer', snapshot_json, snapshot_version
            FROM history_entries_last_good;
            "#,
        )
        .unwrap();
    drop(connection);
    click(cx, "history-refresh-button").unwrap();
    cx.run_until_parked();

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.history_len(), 1);
        assert_eq!(workspace.history()[0].id, before[0].id);
        let HistoryStorageStatus::Error { stage, message } = workspace.history_storage_status()
        else {
            panic!("failed reload should expose a typed storage error");
        };
        assert_eq!(*stage, HistoryStorageStage::Load);
        assert!(message.contains("load"));
        assert!(message.contains("column type"));
    });
    let connection = Connection::open(&database_path).unwrap();
    let row_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM history_entries", [], |row| row.get(0))
        .unwrap();
    assert_eq!(row_count, 1);
    assert!(cx.debug_bounds("history-item-0").is_some());
    assert!(cx.debug_bounds("history-storage-error").is_some());

    close_app(app, cx);
}

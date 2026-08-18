//! UI-driven acceptance tests for the send/response path.
//!
//! User flows mutate the application through rendered controls. The injected workspace entity is
//! used for assertions and deterministic async preconditions, not as a second View command API.

#[path = "common/ui.rs"]
mod ui;

use gpui::{AppContext, TestAppContext};
use mockito::Matcher;
use postman_gpui::{
    app::{BodyKind, KeyValueRow, PostmanApp, ResponseState, WorkspaceViewModel},
    models::{HttpMethod, MultipartValue, RequestBody},
};
use ui::{choose_method, click, replace_text, scroll_down, scroll_up, type_into};

#[gpui::test]
fn empty_url_shows_error_in_response_panel(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    match workspace.read_with(cx, |workspace, _| workspace.response().clone()) {
        ResponseState::Error { message } => assert!(
            message.to_lowercase().contains("url"),
            "error should mention URL, got: {message}"
        ),
        other => panic!("expected Error in the response panel, got {other:?}"),
    }
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.history_len()),
        0
    );
}

#[gpui::test]
fn get_404_shows_status_and_body_in_response_panel(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let mock = server
        .mock("GET", "/missing")
        .with_status(404)
        .with_header("content-type", "application/json")
        .with_body(r#"{"error":"missing"}"#)
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    type_into(cx, "url-input", &format!("{}/missing", server.url())).unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    match workspace.read_with(cx, |workspace, _| workspace.response().clone()) {
        ResponseState::Success { status, body, .. } => {
            assert_eq!(status, 404);
            assert!(body.contains("missing"));
        }
        other => panic!("404 is a response, not a send failure: {other:?}"),
    }
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.history_len()),
        1
    );
    mock.assert();
}

#[gpui::test]
fn delete_sends_no_body_and_keeps_method_response_and_history_in_sync(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let request = server
        .mock("DELETE", "/delete")
        .match_body(Matcher::Exact(String::new()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"method":"DELETE","data":""}"#)
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    choose_method(cx, "DELETE").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.method()),
        HttpMethod::DELETE,
        "the rendered method selector must save directly to the ViewModel"
    );
    assert!(cx.debug_bounds("method-dropdown-selected-value").is_some());
    assert!(cx.debug_bounds("request-tab-method-0").is_some());

    let url = format!("{}/delete", server.url());
    type_into(cx, "url-input", &url).unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.url().to_string()),
        url,
        "the active URL input must already be saved before blur"
    );

    // Send directly from the active URL input: no Enter, Tab, or explicit blur is involved.
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    match workspace.read_with(cx, |workspace, _| workspace.response().clone()) {
        ResponseState::Success { status, body, .. } => {
            assert_eq!(status, 200);
            let echo: serde_json::Value =
                serde_json::from_str(&body).expect("the mock should return a JSON echo");
            assert_eq!(echo["method"], "DELETE");
            assert_eq!(echo["data"], "");
        }
        other => panic!("DELETE should complete as a response: {other:?}"),
    }
    assert!(cx.debug_bounds("response-container").is_some());
    assert!(cx.debug_bounds("response-content").is_some());

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.history_len(), 1);
        let entry = &workspace.history()[0];
        assert_eq!(entry.request.method, HttpMethod::DELETE);
        assert_eq!(entry.request.url, url);
        assert_eq!(entry.request.body, RequestBody::None);
        assert_eq!(entry.status, Some(200));
    });
    assert!(cx.debug_bounds("history-method-0").is_some());
    request.assert();
}

#[gpui::test]
fn put_sends_json_body_and_shows_status(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let mock = server
        .mock("PUT", "/item")
        .match_body(r#"{"a":1}"#)
        .with_status(201)
        .with_body(r#"{"ok":true}"#)
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    choose_method(cx, "PUT").unwrap();
    type_into(cx, "url-input", &format!("{}/item", server.url())).unwrap();
    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-json").unwrap();
    replace_text(cx, "body-input", r#"{"a":1}"#).unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    match workspace.read_with(cx, |workspace, _| workspace.response().clone()) {
        ResponseState::Success { status, body, .. } => {
            assert_eq!(status, 201);
            assert!(body.contains("ok"));
        }
        other => panic!("PUT should complete as a response: {other:?}"),
    }
    mock.assert();
}

#[gpui::test]
fn patch_sends_active_json_body_and_keeps_response_and_history_in_sync(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let request = server
        .mock("PATCH", "/patch")
        .match_header("content-type", "application/json")
        .match_body(Matcher::Exact(r#"{"patched":true}"#.to_string()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"method":"PATCH","json":{"patched":true}}"#)
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    choose_method(cx, "PATCH").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.method()),
        HttpMethod::PATCH,
        "the rendered method selector must save PATCH directly to the ViewModel"
    );

    let url = format!("{}/patch", server.url());
    type_into(cx, "url-input", &url).unwrap();
    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-json").unwrap();
    replace_text(cx, "body-input", r#"{"patched":true}"#).unwrap();

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.body_kind(), BodyKind::Json);
        assert_eq!(
            workspace.request_body(),
            &RequestBody::Json(r#"{"patched":true}"#.to_string()),
            "the active JSON editor must save its latest value before blur"
        );
    });
    assert!(cx.debug_bounds("method-dropdown-selected-value").is_some());
    assert!(cx.debug_bounds("request-tab-method-0").is_some());
    assert!(cx.debug_bounds("body-kind-json").is_some());
    assert!(cx.debug_bounds("body-input").is_some());

    // Issue #56 sends directly from the active body editor: no Enter, Tab, or blur is involved.
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    match workspace.read_with(cx, |workspace, _| workspace.response().clone()) {
        ResponseState::Success { status, body, .. } => {
            assert_eq!(status, 200);
            let echo: serde_json::Value =
                serde_json::from_str(&body).expect("the mock should return a JSON echo");
            assert_eq!(echo["method"], "PATCH");
            assert_eq!(echo["json"]["patched"], true);
        }
        other => panic!("PATCH should complete as a response: {other:?}"),
    }
    assert!(cx.debug_bounds("response-container").is_some());
    assert!(cx.debug_bounds("response-content").is_some());

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.history_len(), 1);
        let entry = &workspace.history()[0];
        assert_eq!(entry.request.method, HttpMethod::PATCH);
        assert_eq!(entry.request.url, url);
        assert_eq!(
            entry.request.body,
            RequestBody::Json(r#"{"patched":true}"#.to_string())
        );
        assert!(entry.request.headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("content-type") && value == "application/json"
        }));
        assert_eq!(entry.status, Some(200));
    });
    assert!(cx.debug_bounds("history-method-0").is_some());
    request.assert();
}

#[gpui::test]
fn mouse_and_keyboard_get_reaches_local_server_and_renders_response(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let mock = server
        .mock("GET", "/health")
        .match_query(Matcher::UrlEncoded("source".into(), "gpui".into()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_header("x-test-server", "postman-gpui")
        .with_body(r#"{"message":"minimal-flow-ok"}"#)
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    type_into(
        cx,
        "url-input",
        &format!("{}/health?source=gpui", server.url()),
    )
    .unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    match workspace.read_with(cx, |workspace, _| workspace.response().clone()) {
        ResponseState::Success {
            status,
            body,
            headers,
            ..
        } => {
            assert_eq!(status, 200);
            assert!(body.contains("minimal-flow-ok"));
            assert!(headers.iter().any(|(name, value)| {
                name.eq_ignore_ascii_case("x-test-server") && value == "postman-gpui"
            }));
        }
        other => panic!("expected a completed response, got {other:?}"),
    }
    assert!(cx.debug_bounds("response-content").is_some());
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.history_len()),
        1
    );
    mock.assert();
}

#[gpui::test]
fn query_parameters_merge_encode_and_send_without_focus_change(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let request = server
        .mock("GET", "/live-query")
        .match_query(Matcher::Exact(
            "existing=1&q=rust+gpui&locale=%E4%B8%AD%E6%96%87".into(),
        ))
        .with_status(200)
        .with_body("query-saved")
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    let base_url = format!("{}/live-query?existing=1", server.url());
    type_into(cx, "url-input", &base_url).unwrap();
    click(cx, "request-pane-params").unwrap();
    type_into(cx, "row-key-input", "q").unwrap();
    type_into(cx, "row-value-input", "rust gpui").unwrap();
    click(cx, "add-row-button").unwrap();
    type_into(cx, "row-key-input", "locale").unwrap();
    type_into(cx, "row-value-input", "中文").unwrap();

    let synchronized_url = format!(
        "{}/live-query?existing=1&q=rust+gpui&locale=%E4%B8%AD%E6%96%87",
        server.url()
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.url().to_string()),
        synchronized_url
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.effective_url()),
        synchronized_url
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.url_query_parameter_count()),
        3
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.enabled_param_count()),
        3
    );
    assert!(cx.debug_bounds("url-query-count").is_some());
    assert!(cx.debug_bounds("params-enabled-count").is_some());
    assert!(cx.debug_bounds("effective-url-preview").is_some());
    assert!(cx.debug_bounds("params-ready-indicator").is_some());
    // Send while the final value editor is still active; that row was never committed with Add.
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Success { status: 200, .. }
    ));
    assert!(cx.debug_bounds("response-echo-bar").is_some());
    assert_eq!(
        workspace.read_with(cx, |workspace, _| {
            workspace
                .history()
                .first()
                .map(|entry| entry.request.url.clone())
        }),
        Some(synchronized_url)
    );
    request.assert();
}

#[gpui::test]
fn multiple_query_rows_can_be_created_before_editing_and_sent(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let request = server
        .mock("GET", "/multi-query")
        .match_query(Matcher::Exact(
            "q=rust+gpui&locale=%E4%B8%AD%E6%96%87".into(),
        ))
        .with_status(200)
        .with_body("multi-query-saved")
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    type_into(cx, "url-input", &format!("{}/multi-query", server.url())).unwrap();
    click(cx, "request-pane-params").unwrap();

    // The initial editor has exactly one visible Key/Value row. Every click must preserve that row
    // and append exactly one more: 1 -> 2 -> 3.
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.visible_param_row_count()),
        1
    );
    assert!(cx.debug_bounds("param-row-0").is_some());
    let newest_row_selectors = ["param-row-1", "param-row-2"];
    for (click_index, newest_row_selector) in newest_row_selectors.into_iter().enumerate() {
        click(cx, "add-row-button").unwrap();
        cx.run_until_parked();
        let expected_visible_rows = click_index + 2;
        assert_eq!(
            workspace.read_with(cx, |workspace, _| workspace.visible_param_row_count()),
            expected_visible_rows,
            "each Add click must add exactly one visible Key/Value row"
        );
        assert!(
            cx.debug_bounds(newest_row_selector).is_some(),
            "newly appended row must be rendered"
        );
    }
    scroll_up(cx, "params-rows-scroll", 1000.0).unwrap();
    type_into(cx, "param-row-key-input-0", "q").unwrap();
    type_into(cx, "param-row-value-input-0", "rust gpui").unwrap();
    scroll_down(cx, "params-rows-scroll", 90.0).unwrap();
    type_into(cx, "param-row-key-input-1", "locale").unwrap();
    type_into(cx, "param-row-value-input-1", "中文").unwrap();

    let synchronized_url = format!(
        "{}/multi-query?q=rust+gpui&locale=%E4%B8%AD%E6%96%87",
        server.url()
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.effective_url()),
        synchronized_url
    );
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.params().len(), 2);
        assert_eq!(
            workspace.params()[0],
            KeyValueRow::enabled("q", "rust gpui")
        );
        assert_eq!(
            workspace.params()[1],
            KeyValueRow::enabled("locale", "中文")
        );
    });

    // Send while the final blank row remains open; no blur or final Add is involved.
    scroll_down(cx, "params-rows-scroll", 90.0).unwrap();
    click(cx, "param-row-key-input-2").unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Success { status: 200, .. }
    ));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| {
            workspace
                .history()
                .first()
                .map(|entry| entry.request.url.clone())
        }),
        Some(synchronized_url)
    );

    // Delete targets only the selected row and leaves the other editors intact.
    click(cx, "param-row-delete-1").unwrap();
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.params().len(), 1);
        assert_eq!(workspace.params()[0].key, "q");
        assert_eq!(workspace.visible_param_row_count(), 2);
    });
    request.assert();
}

#[gpui::test]
fn add_parameter_has_no_row_limit_and_appends_one_blank_row_per_click(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    click(cx, "request-pane-params").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.visible_param_row_count()),
        1
    );

    let newest_row_selectors = [
        "param-row-1",
        "param-row-2",
        "param-row-3",
        "param-row-4",
        "param-row-5",
        "param-row-6",
        "param-row-7",
        "param-row-8",
        "param-row-9",
        "param-row-10",
        "param-row-11",
        "param-row-12",
    ];
    for (click_index, newest_row_selector) in newest_row_selectors.into_iter().enumerate() {
        click(cx, "add-row-button").unwrap();
        cx.run_until_parked();
        let click_count = click_index + 1;
        workspace.read_with(cx, |workspace, _| {
            assert_eq!(workspace.params().len(), click_count);
            assert_eq!(workspace.visible_param_row_count(), click_count + 1);
            assert!(workspace
                .params()
                .iter()
                .all(|row| row.key.is_empty() && row.value.is_empty()));
        });
        assert!(
            cx.debug_bounds(newest_row_selector).is_some(),
            "the blank row created by click {click_count} must be visible"
        );
    }
}

#[gpui::test]
fn pasting_a_complete_query_url_populates_params_and_sends_each_pair_once(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let request = server
        .mock("GET", "/pasted-query")
        .match_query(Matcher::Exact(
            "existing=1&q=rust+gpui&locale=%E4%B8%AD%E6%96%87".into(),
        ))
        .with_status(200)
        .with_body("pasted-query-saved")
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    let pasted_url = format!(
        "{}/pasted-query?existing=1&q=rust+gpui&locale=%E4%B8%AD%E6%96%87",
        server.url()
    );
    type_into(cx, "url-input", &pasted_url).unwrap();
    click(cx, "request-pane-params").unwrap();

    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.params().to_vec()),
        vec![
            KeyValueRow::enabled("existing", "1"),
            KeyValueRow::enabled("q", "rust gpui"),
            KeyValueRow::enabled("locale", "中文"),
        ]
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.enabled_param_count()),
        3
    );
    assert!(cx.debug_bounds("param-row-toggle-0").is_some());
    assert!(cx.debug_bounds("param-row-toggle-1").is_some());
    assert!(cx.debug_bounds("param-row-toggle-2").is_some());

    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Success { status: 200, .. }
    ));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| {
            workspace
                .history()
                .first()
                .map(|entry| entry.request.url.clone())
        }),
        Some(pasted_url)
    );
    request.assert();
}

#[gpui::test]
fn header_is_saved_before_add_or_focus_change(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let request = server
        .mock("GET", "/live-header")
        .match_header("x-live-input", "saved-before-add")
        .with_status(200)
        .with_body("header-saved")
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    type_into(cx, "url-input", &format!("{}/live-header", server.url())).unwrap();
    click(cx, "request-pane-headers").unwrap();
    type_into(cx, "row-key-input", "X-Live-Input").unwrap();
    type_into(cx, "row-value-input", "saved-before-add").unwrap();

    // Send while the value editor is still active; Add was never clicked.
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Success { status: 200, .. }
    ));
    request.assert();
}

#[gpui::test]
fn custom_and_disabled_headers_are_visible_but_only_enabled_headers_are_sent(
    cx: &mut TestAppContext,
) {
    let mut server = mockito::Server::new();
    let request = server
        .mock("GET", "/headers")
        .match_header("x-scenario", "httpbingo-headers")
        .match_header("x-disabled", Matcher::Missing)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"headers":{"X-Scenario":["httpbingo-headers"]}}"#)
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    type_into(cx, "url-input", &format!("{}/headers", server.url())).unwrap();
    click(cx, "request-pane-headers").unwrap();

    type_into(cx, "row-key-input", "X-Scenario").unwrap();
    type_into(cx, "row-value-input", "httpbingo-headers").unwrap();
    click(cx, "add-row-button").unwrap();
    type_into(cx, "row-key-input", "X-Disabled").unwrap();
    type_into(cx, "row-value-input", "must-not-be-sent").unwrap();
    click(cx, "add-row-button").unwrap();
    click(cx, "header-row-toggle-1").unwrap();

    for selector in [
        "headers-summary",
        "headers-enabled-count",
        "headers-table-header",
        "header-row-key-0",
        "header-row-value-0",
        "header-row-status-0",
        "header-row-key-1",
        "header-row-value-1",
        "header-row-status-1",
        "header-row-delete-1",
        "headers-ready-indicator",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "Headers contract element `{selector}` should be rendered"
        );
    }
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.headers().to_vec()),
        vec![
            KeyValueRow::enabled("X-Scenario", "httpbingo-headers"),
            KeyValueRow {
                enabled: false,
                key: "X-Disabled".to_string(),
                value: "must-not-be-sent".to_string(),
            },
        ]
    );

    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Success { status: 200, .. }
    ));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| {
            workspace
                .history()
                .first()
                .map(|entry| entry.request.headers.clone())
        }),
        Some(vec![(
            "X-Scenario".to_string(),
            "httpbingo-headers".to_string(),
        )])
    );
    assert!(
        !workspace.read_with(cx, |workspace, _| workspace.headers().to_vec())[1].enabled,
        "disabled rows remain saved in the editor after Send"
    );
    request.assert();
}

#[gpui::test]
fn multiple_header_rows_can_be_created_before_editing_and_sent(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let request = server
        .mock("GET", "/multiple-headers")
        .match_header("x-scenario", "multiple-header-rows")
        .match_header("x-locale", "zh-CN")
        .match_header("x-disabled", Matcher::Missing)
        .with_status(200)
        .with_body("multiple-headers-saved")
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    type_into(
        cx,
        "url-input",
        &format!("{}/multiple-headers", server.url()),
    )
    .unwrap();
    click(cx, "request-pane-headers").unwrap();

    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.visible_header_row_count()),
        1
    );
    for expected_rows in 2..=4 {
        click(cx, "add-row-button").unwrap();
        cx.run_until_parked();
        assert_eq!(
            workspace.read_with(cx, |workspace, _| workspace.visible_header_row_count()),
            expected_rows,
            "each Add Header click must append exactly one independent row"
        );
    }

    scroll_up(cx, "headers-rows-scroll", 1000.0).unwrap();
    type_into(cx, "header-row-key-input-0", "X-Scenario").unwrap();
    type_into(cx, "header-row-value-input-0", "multiple-header-rows").unwrap();
    type_into(cx, "header-row-key-input-1", "X-Locale").unwrap();
    type_into(cx, "header-row-value-input-1", "zh-CN").unwrap();
    type_into(cx, "header-row-key-input-2", "X-Disabled").unwrap();
    type_into(cx, "header-row-value-input-2", "must-not-be-sent").unwrap();
    click(cx, "header-row-toggle-2").unwrap();

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.headers().len(), 3);
        assert_eq!(
            workspace.headers()[0],
            KeyValueRow::enabled("X-Scenario", "multiple-header-rows")
        );
        assert_eq!(
            workspace.headers()[1],
            KeyValueRow::enabled("X-Locale", "zh-CN")
        );
        assert!(!workspace.headers()[2].enabled);
    });

    // The value is already in the ViewModel; focus need not leave X-Locale before Send.
    click(cx, "header-row-value-input-1").unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Success { status: 200, .. }
    ));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| {
            workspace
                .history()
                .first()
                .map(|entry| entry.request.headers.clone())
        }),
        Some(vec![
            ("X-Scenario".to_string(), "multiple-header-rows".to_string()),
            ("X-Locale".to_string(), "zh-CN".to_string()),
        ])
    );

    click(cx, "header-row-delete-0").unwrap();
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.headers().len(), 2);
        assert_eq!(workspace.headers()[0].key, "X-Locale");
        assert_eq!(workspace.headers()[1].key, "X-Disabled");
    });
    request.assert();
}

#[gpui::test]
fn add_header_has_no_row_limit_and_appends_one_blank_row_per_click(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    click(cx, "request-pane-headers").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.visible_header_row_count()),
        1
    );

    let newest_row_selectors = [
        "header-row-1",
        "header-row-2",
        "header-row-3",
        "header-row-4",
        "header-row-5",
        "header-row-6",
        "header-row-7",
        "header-row-8",
        "header-row-9",
        "header-row-10",
        "header-row-11",
        "header-row-12",
    ];
    for (click_index, newest_row_selector) in newest_row_selectors.into_iter().enumerate() {
        let click_count = click_index + 1;
        click(cx, "add-row-button").unwrap();
        cx.run_until_parked();
        workspace.read_with(cx, |workspace, _| {
            assert_eq!(workspace.headers().len(), click_count);
            assert_eq!(workspace.visible_header_row_count(), click_count + 1);
            assert!(workspace
                .headers()
                .iter()
                .all(|row| row.key.is_empty() && row.value.is_empty()));
        });
        assert!(
            cx.debug_bounds(newest_row_selector).is_some(),
            "the blank row created by click {click_count} must be rendered"
        );
    }
}

#[gpui::test]
fn clicking_send_again_cancels_an_in_flight_request(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let pending = workspace.update(cx, |workspace, _| {
        workspace.set_url("https://example.com/slow");
        workspace.begin_send()
    });
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Loading
    ));
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    let response = workspace.read_with(cx, |workspace, _| workspace.response().clone());
    assert!(
        matches!(response, ResponseState::Cancelled),
        "second Send click should cancel the request, got {response:?}"
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.history_len()),
        0
    );

    workspace.update(cx, |workspace, _| {
        assert!(!workspace.complete_send(
            pending,
            Ok(postman_gpui::http::executor::RequestResult::success(
                "too late".to_string(),
            )),
        ));
    });
    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Cancelled
    ));
}

#[gpui::test]
fn sample_and_clear_buttons_have_their_own_product_semantics(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    click(cx, "request-pane-body").unwrap();
    click(cx, "body-sample-json").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.body_kind()),
        BodyKind::Json
    );
    assert!(workspace.read_with(cx, |workspace, _| workspace.body().contains("Ada Lovelace")));

    click(cx, "body-clear-button").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.body_kind()),
        BodyKind::Json
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.body().to_string()),
        ""
    );
}

#[gpui::test]
fn urlencoded_editor_keeps_new_rows_visible_while_the_form_grows(cx: &mut TestAppContext) {
    const KEY_SELECTORS: [&str; 8] = [
        "body-form-key-0",
        "body-form-key-1",
        "body-form-key-2",
        "body-form-key-3",
        "body-form-key-4",
        "body-form-key-5",
        "body-form-key-6",
        "body-form-key-7",
    ];
    const VALUE_SELECTORS: [&str; 8] = [
        "body-form-value-0",
        "body-form-value-1",
        "body-form-value-2",
        "body-form-value-3",
        "body-form-value-4",
        "body-form-value-5",
        "body-form-value-6",
        "body-form-value-7",
    ];
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    choose_method(cx, "POST").unwrap();
    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-url-encoded").unwrap();
    for index in 0..KEY_SELECTORS.len() {
        if index > 0 {
            click(cx, "body-form-add-row").unwrap();
        }
        type_into(cx, KEY_SELECTORS[index], &format!("k{index}")).unwrap();
        type_into(cx, VALUE_SELECTORS[index], &format!("v{index}")).unwrap();
    }

    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.body().to_string()),
        "k0=v0&k1=v1&k2=v2&k3=v3&k4=v4&k5=v5&k6=v6&k7=v7"
    );
}

#[gpui::test]
fn multipart_text_value_is_saved_before_the_active_cell_loses_focus(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let submitted = server
        .mock("POST", "/form")
        .match_header(
            "content-type",
            Matcher::Regex("^multipart/form-data; boundary=".to_string()),
        )
        .match_body(Matcher::AllOf(vec![
            Matcher::Regex("name=\\\"comments\\\"".to_string()),
            Matcher::Regex("1234".to_string()),
        ]))
        .with_status(200)
        .with_body(r#"{"saved":true}"#)
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    choose_method(cx, "POST").unwrap();
    type_into(cx, "url-input", &format!("{}/form", server.url())).unwrap();
    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-form-data").unwrap();
    type_into(cx, "body-form-key-0", "comments").unwrap();
    type_into(cx, "body-form-value-0", "1234").unwrap();

    let body = workspace.read_with(cx, |workspace, _| workspace.request_body().clone());
    assert!(matches!(
        body,
        RequestBody::Multipart(parts)
            if matches!(
                parts.as_slice(),
                [part]
                    if part.name == "comments"
                        && matches!(&part.value, MultipartValue::Text(value) if value == "1234")
            )
    ));

    // This is the regression path: Send is clicked while the value cell is still active.
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Success { status: 200, .. }
    ));
    submitted.assert();
}

#[gpui::test]
fn multipart_file_picker_sends_a_typed_file_part(cx: &mut TestAppContext) {
    let fixture_path = std::env::temp_dir().join(format!(
        "postman-gpui-ui-upload-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should follow the Unix epoch")
            .as_nanos()
    ));
    std::fs::write(&fixture_path, "file payload from the GPUI editor")
        .expect("upload fixture should be writable");
    let mut server = mockito::Server::new();
    let upload = server
        .mock("POST", "/upload")
        .match_header(
            "content-type",
            Matcher::Regex("^multipart/form-data; boundary=".to_string()),
        )
        .match_body(Matcher::AllOf(vec![
            Matcher::Regex("name=\\\"upload\\\"".to_string()),
            Matcher::Regex("file payload from the GPUI editor".to_string()),
        ]))
        .with_status(201)
        .with_body(r#"{"uploaded":true}"#)
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    choose_method(cx, "POST").unwrap();
    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-form-data").unwrap();
    type_into(cx, "body-form-key-0", "upload").unwrap();
    cx.simulate_keystrokes("enter");
    click(cx, "body-form-type-0").unwrap();
    click(cx, "body-form-file-0").unwrap();
    assert!(cx.did_prompt_for_paths());

    let selected = fixture_path.clone();
    cx.simulate_path_prompt_response({
        let selected = selected.clone();
        move |options| {
            assert!(options.files);
            assert!(!options.directories);
            assert!(!options.multiple);
            Some(vec![selected])
        }
    });
    cx.run_until_parked();

    let body = workspace.read_with(cx, |workspace, _| workspace.request_body().clone());
    let RequestBody::Multipart(parts) = body else {
        panic!("form-data editor should produce a typed multipart body");
    };
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].name, "upload");
    assert!(matches!(
        &parts[0].value,
        MultipartValue::File { path, .. } if path == &fixture_path
    ));

    type_into(cx, "url-input", &format!("{}/upload", server.url())).unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Success { status: 201, .. }
    ));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.history_len()),
        1
    );
    upload.assert();
    let _ = std::fs::remove_file(&fixture_path);
}

//! End-to-end interaction coverage for workspace-level MVVM features.

#[path = "common/ui.rs"]
mod ui;

use gpui::{AppContext, TestAppContext};
use mockito::Matcher;
use postman_gpui::app::{
    AuthorizationKind, BodyKind, KeyValueRow, MultipartDraftPart, MultipartDraftValue, PostmanApp,
    RequestBodyDraft, RequestPane, ResponseState, WorkspaceViewModel,
};
use postman_gpui::models::{HttpMethod, MultipartPart, MultipartValue, RequestBody};
use ui::{choose_method, click, replace_text, scroll_down, scroll_up, type_into};

#[gpui::test]
fn new_switch_and_close_tabs_preserve_independent_drafts(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    type_into(cx, "url-input", "https://first.example/users").unwrap();
    click(cx, "new-tab-button").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.tab_count()),
        2
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .url()
            .to_string()),
        ""
    );

    type_into(cx, "url-input", "https://second.example/orders").unwrap();
    click(cx, "request-tab-0").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .url()
            .to_string()),
        "https://first.example/users"
    );

    click(cx, "url-input").unwrap();
    cx.simulate_keystrokes("end");
    cx.simulate_input("?projected=first");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .url()
            .to_string()),
        "https://first.example/users?projected=first"
    );

    click(cx, "close-tab-0").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.tab_count()),
        1
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .url()
            .to_string()),
        "https://second.example/orders"
    );
}

#[gpui::test]
fn multi_tab_mouse_enter_space_keep_requests_responses_and_history_isolated(
    cx: &mut TestAppContext,
) {
    let tab_b_body = r#"{"tab":"B","message":"isolated"}"#;
    let mut server = mockito::Server::new();
    let tab_a_request = server
        .mock("GET", "/get")
        .match_query(Matcher::Exact("tab=A&q=rust".to_string()))
        .match_header("x-tab", "A")
        .match_header("authorization", Matcher::Missing)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"tab":"A","method":"GET"}"#)
        .create();
    let tab_b_request = server
        .mock("POST", "/anything/tab-b")
        .match_query(Matcher::Exact("mode=json".to_string()))
        .match_header("x-tab", "B")
        .match_header("authorization", "Bearer tab-b-e2e-token")
        .match_header("content-type", "application/json")
        .match_body(Matcher::Exact(tab_b_body.to_string()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"tab":"B","method":"POST"}"#)
        .create();
    let tab_a_url = format!("{}/get?tab=A&q=rust", server.url());
    let tab_b_url = format!("{}/anything/tab-b?mode=json", server.url());
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    type_into(cx, "url-input", &tab_a_url).unwrap();
    click(cx, "request-pane-headers").unwrap();
    type_into(cx, "row-key-input", "X-Tab").unwrap();
    type_into(cx, "row-value-input", "A").unwrap();
    click(cx, "request-pane-params").unwrap();

    click(cx, "new-tab-button").unwrap();
    choose_method(cx, "POST").unwrap();
    type_into(cx, "url-input", &tab_b_url).unwrap();
    click(cx, "request-pane-headers").unwrap();
    type_into(cx, "row-key-input", "X-Tab").unwrap();
    type_into(cx, "row-value-input", "B").unwrap();
    click(cx, "request-pane-authorization").unwrap();
    type_into(cx, "authorization-input", "tab-b-e2e-token").unwrap();
    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-json").unwrap();
    replace_text(cx, "body-input", tab_b_body).unwrap();

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.tab_count(), 2);
        assert_eq!(workspace.active_tab_index().unwrap(), 1);
        assert_eq!(workspace.tabs()[0].tab_id().to_string(), "1");
        assert_eq!(workspace.tabs()[0].method(), HttpMethod::GET);
        assert_eq!(workspace.tabs()[0].url(), tab_a_url);
        assert_eq!(workspace.tabs()[0].request_pane(), RequestPane::Params);
        assert_eq!(workspace.tabs()[0].request_body(), RequestBody::None);
        assert!(workspace.tabs()[0].is_dirty());
        assert!(matches!(
            workspace.tabs()[0].response(),
            ResponseState::NotSent
        ));

        assert_eq!(workspace.tabs()[1].tab_id().to_string(), "2");
        assert_eq!(workspace.tabs()[1].method(), HttpMethod::POST);
        assert_eq!(workspace.tabs()[1].url(), tab_b_url);
        assert_eq!(workspace.tabs()[1].request_pane(), RequestPane::Body);
        assert_eq!(workspace.tabs()[1].bearer_token(), "tab-b-e2e-token");
        assert_eq!(
            workspace.tabs()[1].request_body(),
            RequestBody::Json(tab_b_body.to_string())
        );
        assert!(workspace.tabs()[1].is_dirty());
        assert!(matches!(
            workspace.tabs()[1].response(),
            ResponseState::NotSent
        ));
    });

    // Mouse and keyboard both enter the same stable-tab-id activation command. Mouse focuses
    // Tab A, Tab+Enter activates B, and Shift-Tab+Space activates A again.
    click(cx, "request-tab-0").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_tab_index().unwrap()),
        0
    );
    assert!(cx.debug_bounds("params-ready-indicator").is_some());
    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_tab_index().unwrap()),
        1
    );
    assert!(cx.debug_bounds("body-input").is_some());
    cx.simulate_keystrokes("shift-tab");
    cx.simulate_keystrokes("space");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_tab_index().unwrap()),
        0
    );

    click(cx, "send-button").unwrap();
    cx.run_until_parked();
    workspace.read_with(cx, |workspace, _| {
        assert!(matches!(
            workspace.tabs()[0].response(),
            ResponseState::Success {
                status: 200,
                body,
                ..
            } if body.contains(r#""tab":"A""#)
        ));
        assert!(!workspace.tabs()[0].is_dirty());
        assert!(matches!(
            workspace.tabs()[1].response(),
            ResponseState::NotSent
        ));
        assert!(workspace.tabs()[1].is_dirty());
        assert_eq!(workspace.history_len(), 1);
    });

    // Clicking the now-keyboard-focusable Send control moves focus to it. The application-level
    // request-cycle command switches tabs without relying on the pointer's previous focus target.
    cx.simulate_keystrokes("ctrl-tab");
    click(cx, "send-button").unwrap();
    cx.run_until_parked();
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.active_tab_index().unwrap(), 1);
        assert!(matches!(
            workspace.tabs()[0].response(),
            ResponseState::Success { body, .. } if body.contains(r#""tab":"A""#)
        ));
        assert!(matches!(
            workspace.tabs()[1].response(),
            ResponseState::Success {
                status: 200,
                body,
                ..
            } if body.contains(r#""tab":"B""#)
        ));
        assert!(!workspace.tabs()[1].is_dirty());
        assert_eq!(workspace.history_len(), 2);

        let newest = &workspace.history()[0].request;
        assert_eq!(newest.method, HttpMethod::POST);
        assert_eq!(newest.url, tab_b_url);
        assert_eq!(newest.body, RequestBody::Json(tab_b_body.to_string()));
        assert!(newest
            .headers
            .iter()
            .any(|(name, value)| name.eq_ignore_ascii_case("x-tab") && value == "B"));
        assert!(newest
            .headers
            .iter()
            .all(|(name, _)| !name.eq_ignore_ascii_case("authorization")));

        let oldest = &workspace.history()[1].request;
        assert_eq!(oldest.method, HttpMethod::GET);
        assert_eq!(oldest.url, tab_a_url);
        assert_eq!(oldest.body, RequestBody::None);
        assert!(oldest
            .headers
            .iter()
            .any(|(name, value)| name.eq_ignore_ascii_case("x-tab") && value == "A"));
    });

    click(cx, "close-tab-1").unwrap();
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.tab_count(), 1);
        assert_eq!(workspace.active_tab_index().unwrap(), 0);
        assert_eq!(workspace.active_request().unwrap().url(), tab_a_url);
        assert_eq!(
            workspace.active_request().unwrap().request_pane(),
            RequestPane::Params
        );
        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::Success { body, .. } if body.contains(r#""tab":"A""#)
        ));
        assert_eq!(workspace.history_len(), 2);
    });
    assert!(cx.debug_bounds("request-tab-0").is_some());
    assert!(cx.debug_bounds("request-tab-1").is_none());

    tab_a_request.assert();
    tab_b_request.assert();
}

#[gpui::test]
fn every_composer_pane_restores_active_edits_for_its_request_tab(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    choose_method(cx, "POST").unwrap();
    type_into(cx, "url-input", "https://first.example/items").unwrap();
    type_into(cx, "row-key-input", "first-param").unwrap();
    type_into(cx, "row-value-input", "one").unwrap();
    click(cx, "request-pane-headers").unwrap();
    type_into(cx, "row-key-input", "X-First").unwrap();
    type_into(cx, "row-value-input", "header-one").unwrap();
    click(cx, "request-pane-authorization").unwrap();
    type_into(cx, "authorization-input", "first-token").unwrap();
    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-json").unwrap();
    replace_text(cx, "body-input", r#"{"tab":1}"#).unwrap();
    click(cx, "request-pane-scripts").unwrap();
    type_into(cx, "script-editor", "prepare-first()").unwrap();
    click(cx, "request-pane-tests").unwrap();
    type_into(cx, "tests-editor", "assert-first()").unwrap();

    click(cx, "new-tab-button").unwrap();
    choose_method(cx, "PATCH").unwrap();
    type_into(cx, "url-input", "https://second.example/items").unwrap();
    type_into(cx, "row-key-input", "second-param").unwrap();
    type_into(cx, "row-value-input", "two").unwrap();
    click(cx, "request-pane-headers").unwrap();
    type_into(cx, "row-key-input", "X-Second").unwrap();
    type_into(cx, "row-value-input", "header-two").unwrap();
    click(cx, "request-pane-authorization").unwrap();
    click(cx, "auth-kind-basic").unwrap();
    type_into(cx, "basic-auth-username-input", "second-user").unwrap();
    type_into(cx, "basic-auth-password-input", "second-pass").unwrap();
    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-raw").unwrap();
    replace_text(cx, "body-input", "second-body").unwrap();
    click(cx, "request-pane-scripts").unwrap();
    type_into(cx, "script-editor", "prepare-second()").unwrap();
    click(cx, "request-pane-tests").unwrap();
    type_into(cx, "tests-editor", "assert-second()").unwrap();
    click(cx, "request-pane-body").unwrap();

    click(cx, "request-tab-0").unwrap();
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(
            workspace.active_request().unwrap().method(),
            HttpMethod::POST
        );
        assert_eq!(
            workspace.active_request().unwrap().url(),
            "https://first.example/items?first-param=one"
        );
        assert_eq!(
            workspace
                .active_request()
                .unwrap()
                .row_draft(RequestPane::Params),
            Some(("first-param", "one"))
        );
        assert_eq!(
            workspace
                .active_request()
                .unwrap()
                .row_draft(RequestPane::Headers),
            Some(("X-First", "header-one"))
        );
        assert_eq!(
            workspace.active_request().unwrap().authorization_kind(),
            AuthorizationKind::Bearer
        );
        assert_eq!(
            workspace.active_request().unwrap().bearer_token(),
            "first-token"
        );
        assert_eq!(
            workspace.active_request().unwrap().body_kind(),
            BodyKind::Json
        );
        assert_eq!(workspace.active_request().unwrap().body(), r#"{"tab":1}"#);
        assert_eq!(
            workspace.active_request().unwrap().pre_request_script(),
            "prepare-first()"
        );
        assert_eq!(
            workspace.active_request().unwrap().tests_script(),
            "assert-first()"
        );
        assert_eq!(
            workspace.active_request().unwrap().request_pane(),
            RequestPane::Tests
        );
    });

    // Continue typing into each restored control. This checks the explicit pane projection path,
    // not only the values already retained by WorkspaceViewModel.
    click(cx, "tests-editor").unwrap();
    cx.simulate_keystrokes("end");
    cx.simulate_input("-restored");
    click(cx, "request-pane-scripts").unwrap();
    click(cx, "script-editor").unwrap();
    cx.simulate_keystrokes("end");
    cx.simulate_input("-restored");
    click(cx, "request-pane-body").unwrap();
    click(cx, "body-input").unwrap();
    cx.simulate_keystrokes("end");
    cx.simulate_input(" ");
    click(cx, "request-pane-authorization").unwrap();
    click(cx, "authorization-input").unwrap();
    cx.simulate_keystrokes("end");
    cx.simulate_input("-restored");
    click(cx, "request-pane-headers").unwrap();
    click(cx, "row-value-input").unwrap();
    cx.simulate_keystrokes("end");
    cx.simulate_input("-restored");
    click(cx, "request-pane-params").unwrap();
    click(cx, "row-value-input").unwrap();
    cx.simulate_keystrokes("end");
    cx.simulate_input("-restored");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .row_draft(RequestPane::Params)
            .map(|(key, value)| (key.to_string(), value.to_string()))),
        Some(("first-param".to_string(), "one-restored".to_string()))
    );
    click(cx, "url-input").unwrap();
    cx.simulate_keystrokes("end");
    cx.simulate_input("#restored");

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(
            workspace.active_request().unwrap().url(),
            "https://first.example/items?first-param=one-restored#restored"
        );
        assert_eq!(
            workspace
                .active_request()
                .unwrap()
                .row_draft(RequestPane::Headers),
            Some(("X-First", "header-one-restored"))
        );
        assert_eq!(
            workspace.active_request().unwrap().bearer_token(),
            "first-token-restored"
        );
        assert_eq!(workspace.active_request().unwrap().body(), r#"{"tab":1} "#);
        assert_eq!(
            workspace.active_request().unwrap().pre_request_script(),
            "prepare-first()-restored"
        );
        assert_eq!(
            workspace.active_request().unwrap().tests_script(),
            "assert-first()-restored"
        );
    });

    click(cx, "request-tab-1").unwrap();
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(
            workspace.active_request().unwrap().method(),
            HttpMethod::PATCH
        );
        assert_eq!(
            workspace.active_request().unwrap().url(),
            "https://second.example/items?second-param=two"
        );
        assert_eq!(
            workspace
                .active_request()
                .unwrap()
                .row_draft(RequestPane::Params),
            Some(("second-param", "two"))
        );
        assert_eq!(
            workspace
                .active_request()
                .unwrap()
                .row_draft(RequestPane::Headers),
            Some(("X-Second", "header-two"))
        );
        assert_eq!(
            workspace.active_request().unwrap().authorization_kind(),
            AuthorizationKind::Basic
        );
        assert_eq!(
            workspace.active_request().unwrap().basic_username(),
            "second-user"
        );
        assert_eq!(
            workspace.active_request().unwrap().basic_password(),
            "second-pass"
        );
        assert_eq!(
            workspace.active_request().unwrap().body_kind(),
            BodyKind::Raw
        );
        assert_eq!(workspace.active_request().unwrap().body(), "second-body");
        assert_eq!(
            workspace.active_request().unwrap().pre_request_script(),
            "prepare-second()"
        );
        assert_eq!(
            workspace.active_request().unwrap().tests_script(),
            "assert-second()"
        );
        assert_eq!(
            workspace.active_request().unwrap().request_pane(),
            RequestPane::Body
        );
    });
}

#[gpui::test]
fn response_and_history_remain_wired_through_workspace_children(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let completed = server
        .mock("GET", "/workspace-flow")
        .with_status(200)
        .with_body("workspace-flow-ok")
        .create();
    let request_url = format!("{}/workspace-flow", server.url());
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    type_into(cx, "url-input", &request_url).unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .response()
            .clone()),
        ResponseState::Success { status: 200, .. }
    ));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.history_len()),
        1
    );
    assert!(cx.debug_bounds("response-content").is_some());
    assert!(cx.debug_bounds("history-item-0").is_some());

    replace_text(cx, "url-input", "https://draft.example/changed").unwrap();
    click(cx, "history-item-0").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .url()
            .to_string()),
        request_url
    );

    replace_text(cx, "url-input", "https://draft.example/enter").unwrap();
    click(cx, "history-search-input").unwrap();
    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .url()
            .to_string()),
        request_url
    );

    replace_text(cx, "url-input", "https://draft.example/space").unwrap();
    click(cx, "history-search-input").unwrap();
    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("space");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .url()
            .to_string()),
        request_url
    );
    completed.assert();
}

#[gpui::test]
fn row_editors_project_independent_pane_and_tab_drafts(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    type_into(cx, "url-input", "https://first.example/items").unwrap();
    click(cx, "request-pane-params").unwrap();
    type_into(cx, "row-key-input", "q").unwrap();
    type_into(cx, "row-value-input", "first").unwrap();
    click(cx, "request-pane-headers").unwrap();
    type_into(cx, "row-key-input", "X-Tab-Draft").unwrap();
    type_into(cx, "row-value-input", "one").unwrap();

    click(cx, "request-pane-params").unwrap();
    click(cx, "row-value-input").unwrap();
    cx.simulate_input("-params");
    assert!(workspace.read_with(cx, |workspace, _| {
        workspace
            .active_request()
            .unwrap()
            .row_draft(RequestPane::Params)
            == Some(("q", "first-params"))
    }));

    click(cx, "new-tab-button").unwrap();
    type_into(cx, "row-key-input", "q").unwrap();
    type_into(cx, "row-value-input", "second").unwrap();
    click(cx, "request-tab-0").unwrap();

    // Appending proves the first tab's VM value was projected back into the reused controls.
    click(cx, "row-value-input").unwrap();
    cx.simulate_input("-restored");
    assert!(workspace.read_with(cx, |workspace, _| {
        workspace
            .active_request()
            .unwrap()
            .row_draft(RequestPane::Params)
            == Some(("q", "first-params-restored"))
    }));
    assert!(workspace.read_with(cx, |workspace, _| {
        workspace
            .active_request()
            .unwrap()
            .row_draft(RequestPane::Headers)
            == Some(("X-Tab-Draft", "one"))
    }));
}

#[gpui::test]
fn url_encoded_rows_are_owned_by_the_tab_and_projected_without_normalization(
    cx: &mut TestAppContext,
) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-url-encoded").unwrap();
    type_into(cx, "body-form-key-0", "tag").unwrap();
    type_into(cx, "body-form-value-0", "rust 你好").unwrap();

    click(cx, "body-form-add-row").unwrap();
    type_into(cx, "body-form-key-1", "ignored").unwrap();
    type_into(cx, "body-form-value-1", "draft-only").unwrap();
    click(cx, "body-form-toggle-1").unwrap();

    click(cx, "body-form-add-row").unwrap();
    type_into(cx, "body-form-value-2", "blank-key-draft").unwrap();

    click(cx, "body-form-add-row").unwrap();

    click(cx, "body-form-add-row").unwrap();
    type_into(cx, "body-form-key-4", "tag").unwrap();
    type_into(cx, "body-form-value-4", "gpui").unwrap();

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(
            workspace.active_request().unwrap().body_draft(),
            &RequestBodyDraft::UrlEncoded(vec![
                KeyValueRow::enabled("tag", "rust 你好"),
                KeyValueRow {
                    enabled: false,
                    key: "ignored".to_string(),
                    value: "draft-only".to_string(),
                },
                KeyValueRow::enabled("", "blank-key-draft"),
                KeyValueRow::enabled("", ""),
                KeyValueRow::enabled("tag", "gpui"),
            ])
        );
    });

    click(cx, "new-tab-button").unwrap();
    click(cx, "request-tab-0").unwrap();
    assert!(cx.debug_bounds("body-form-row-4").is_some());

    // Re-enabling the restored disabled row proves its enabled state was projected from the
    // ViewModel rather than normalized to `true` while the tab was inactive.
    click(cx, "body-form-toggle-1").unwrap();
    click(cx, "body-form-value-4").unwrap();
    cx.simulate_keystrokes("end");
    cx.simulate_input("-restored");

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(
            workspace.active_request().unwrap().body_draft(),
            &RequestBodyDraft::UrlEncoded(vec![
                KeyValueRow::enabled("tag", "rust 你好"),
                KeyValueRow::enabled("ignored", "draft-only"),
                KeyValueRow::enabled("", "blank-key-draft"),
                KeyValueRow::enabled("", ""),
                KeyValueRow::enabled("tag", "gpui-restored"),
            ])
        );
        assert_eq!(
            workspace.active_request().unwrap().request_body(),
            RequestBody::UrlEncoded(
                "tag=rust+%E4%BD%A0%E5%A5%BD&ignored=draft-only&tag=gpui-restored".to_string()
            )
        );
    });
}

#[gpui::test]
fn multipart_rows_and_file_metadata_are_projected_after_switching_tabs(cx: &mut TestAppContext) {
    let fixture_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/httpbingo-upload.txt");
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-form-data").unwrap();
    type_into(cx, "body-form-key-0", "note").unwrap();
    type_into(cx, "body-form-value-0", "hello").unwrap();

    click(cx, "body-form-add-row").unwrap();
    type_into(cx, "body-form-key-1", "ignored-text").unwrap();
    type_into(cx, "body-form-value-1", "draft-only").unwrap();
    click(cx, "body-form-toggle-1").unwrap();

    click(cx, "body-form-add-row").unwrap();
    type_into(cx, "body-form-key-2", "upload").unwrap();
    cx.simulate_keystrokes("enter");
    click(cx, "body-form-type-2").unwrap();
    click(cx, "body-form-file-2").unwrap();
    assert!(cx.did_prompt_for_paths());
    let selected_path = fixture_path.clone();
    cx.simulate_path_prompt_response(move |_| Some(vec![selected_path]));
    cx.run_until_parked();
    click(cx, "body-form-toggle-2").unwrap();

    click(cx, "body-form-add-row").unwrap();
    type_into(cx, "body-form-value-3", "blank-key-draft").unwrap();

    // Content type is transport metadata that the current picker does not expose as an editable
    // field. Seed it in the authoritative draft, then prove projection and the next visible edit
    // round-trip it together with the path and optional file name.
    workspace.update(cx, |workspace, cx| {
        let RequestBodyDraft::Multipart(mut parts) =
            workspace.active_request().unwrap().body_draft().clone()
        else {
            panic!("form-data selection must create a multipart draft");
        };
        let MultipartDraftValue::File { content_type, .. } = &mut parts[2].value else {
            panic!("selected upload row must remain a file");
        };
        *content_type = Some("text/plain".to_string());
        workspace
            .active_request_mut()
            .unwrap()
            .set_multipart_draft_parts(parts);
        cx.notify();
    });

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(
            workspace.active_request().unwrap().body_draft(),
            &RequestBodyDraft::Multipart(vec![
                MultipartDraftPart::text("note", "hello", true),
                MultipartDraftPart::text("ignored-text", "draft-only", false),
                MultipartDraftPart::file(
                    "upload",
                    fixture_path.clone(),
                    Some("httpbingo-upload.txt".to_string()),
                    Some("text/plain".to_string()),
                    false,
                ),
                MultipartDraftPart::text("", "blank-key-draft", true),
            ])
        );
    });

    click(cx, "new-tab-button").unwrap();
    click(cx, "request-tab-0").unwrap();
    assert!(cx.debug_bounds("body-form-row-3").is_some());

    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .request_body()),
        RequestBody::Multipart(vec![MultipartPart::text("note", "hello")])
    );

    // If projection had normalized either row to enabled, these clicks would disable them and the
    // effective request assertion below would fail.
    scroll_up(cx, "body-form-scroll", 1_000.0).unwrap();
    click(cx, "body-form-toggle-1").unwrap();
    scroll_down(cx, "body-form-scroll", 1_000.0).unwrap();
    click(cx, "body-form-toggle-2").unwrap();

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(
            workspace.active_request().unwrap().request_body(),
            RequestBody::Multipart(vec![
                MultipartPart::text("note", "hello"),
                MultipartPart::text("ignored-text", "draft-only"),
                MultipartPart {
                    name: "upload".to_string(),
                    value: MultipartValue::File {
                        path: fixture_path,
                        file_name: Some("httpbingo-upload.txt".to_string()),
                        content_type: Some("text/plain".to_string()),
                    },
                },
            ])
        );
    });
}

#[gpui::test]
fn left_rail_new_request_is_a_real_command(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    click(cx, "rail-new-request").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.tab_count()),
        2
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_tab_index().unwrap()),
        1
    );
}

#[gpui::test]
fn history_search_filters_completed_requests(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let alpha = server
        .mock("GET", "/alpha-resource")
        .with_status(200)
        .with_body("alpha")
        .create();
    let beta = server
        .mock("GET", "/beta-resource")
        .with_status(200)
        .with_body("beta")
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    type_into(cx, "url-input", &format!("{}/alpha-resource", server.url())).unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();
    replace_text(cx, "url-input", &format!("{}/beta-resource", server.url())).unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.history_len()),
        2
    );
    assert!(cx.debug_bounds("history-item-0").is_some());
    assert!(cx.debug_bounds("history-item-1").is_some());

    type_into(cx, "history-search-input", "alpha-resource").unwrap();
    assert!(cx.debug_bounds("history-item-0").is_none());
    assert!(cx.debug_bounds("history-item-1").is_some());
    alpha.assert();
    beta.assert();
}

#[gpui::test]
fn bearer_authorization_editor_affects_the_real_request(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let secured = server
        .mock("GET", "/secured")
        .match_header("authorization", "Bearer ui-secret")
        .with_status(200)
        .with_body(r#"{"authorized":true}"#)
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    type_into(cx, "url-input", &format!("{}/secured", server.url())).unwrap();
    click(cx, "request-pane-authorization").unwrap();
    for selector in [
        "authorization-summary",
        "authorization-kind-selector",
        "authorization-input",
        "authorization-normalized-token",
        "authorization-header-preview",
        "authorization-ready-indicator",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "Bearer design contract element `{selector}` should be rendered"
        );
    }

    type_into(cx, "authorization-input", "Bearer ui-secret").unwrap();

    // Live input remains verbatim in the VM. The normalized request value is a projection, not a
    // second editable source of truth.
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .bearer_token()
            .to_string()),
        "Bearer ui-secret"
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .normalized_bearer_token()),
        "ui-secret"
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .authorization_header_preview()),
        Some("Authorization: Bearer ui-secret".to_string())
    );

    // Send directly while the Bearer input is still active: no Enter, Tab, blur, or Add.
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .bearer_token()
            .to_string()),
        "ui-secret"
    );
    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .response()
            .clone()),
        ResponseState::Success { status: 200, .. }
    ));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.history_len()),
        1
    );
    assert!(cx.debug_bounds("response-content").is_some());
    assert!(cx.debug_bounds("history-item-0").is_some());
    secured.assert();
}

#[gpui::test]
fn basic_authorization_editor_affects_the_real_request(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let secured = server
        .mock("GET", "/basic-auth")
        .match_header("authorization", "Basic dWktdXNlcjp1aS1wYXNz")
        .with_status(200)
        .with_body(r#"{"authenticated":true}"#)
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    type_into(cx, "url-input", &format!("{}/basic-auth", server.url())).unwrap();
    click(cx, "request-pane-authorization").unwrap();
    click(cx, "auth-kind-basic").unwrap();
    for selector in [
        "authorization-summary",
        "authorization-kind-selector",
        "basic-auth-credentials",
        "basic-auth-username-field",
        "basic-auth-username-input",
        "basic-auth-username-saved",
        "basic-auth-password-field",
        "basic-auth-password-input",
        "basic-auth-password-masked",
        "basic-auth-password-saved",
        "basic-auth-header-preview",
        "basic-auth-projection-note",
        "authorization-ready-indicator",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "Basic Auth design contract element `{selector}` should be rendered"
        );
    }

    type_into(cx, "basic-auth-username-input", "ui-user").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .basic_username()
            .to_string()),
        "ui-user"
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .basic_password()
            .to_string()),
        ""
    );

    type_into(cx, "basic-auth-password-input", "ui-pass").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .basic_password()
            .to_string()),
        "ui-pass"
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .authorization_header_preview()),
        Some("Authorization: Basic dWktdXNlcjp1aS1wYXNz".to_string())
    );

    // Send directly while the masked password field is still active: no Enter, Tab, or blur.
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .authorization_kind()),
        AuthorizationKind::Basic
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .basic_username()
            .to_string()),
        "ui-user"
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .basic_password()
            .to_string()),
        "ui-pass"
    );
    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .response()
            .clone()),
        ResponseState::Success { status: 200, .. }
    ));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.history_len()),
        1
    );
    let authorization_headers = workspace.read_with(cx, |workspace, _| {
        workspace.history()[0]
            .request
            .headers
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case("authorization"))
            .cloned()
            .collect::<Vec<_>>()
    });
    assert!(
        authorization_headers.is_empty(),
        "the request must send Basic Auth, but SQLite History must not persist it"
    );
    assert!(cx.debug_bounds("response-content").is_some());
    assert!(cx.debug_bounds("history-item-0").is_some());
    secured.assert();
}

#[gpui::test]
fn script_and_test_editors_are_saved_per_tab(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    click(cx, "request-pane-scripts").unwrap();
    type_into(cx, "script-editor", "const token = 'first';").unwrap();
    click(cx, "request-pane-tests").unwrap();
    type_into(cx, "tests-editor", "status === 200").unwrap();

    click(cx, "new-tab-button").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .pre_request_script()
            .to_string()),
        ""
    );
    click(cx, "request-tab-0").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .pre_request_script()
            .to_string()),
        "const token = 'first';"
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .tests_script()
            .to_string()),
        "status === 200"
    );
}

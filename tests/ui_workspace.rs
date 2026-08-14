//! End-to-end interaction coverage for workspace-level MVVM features.

#[path = "common/ui.rs"]
mod ui;

use gpui::{AppContext, TestAppContext};
use postman_gpui::app::{AuthorizationKind, PostmanApp, RequestPane, WorkspaceViewModel};
use ui::{click, replace_text, type_into};

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
        workspace.read_with(cx, |workspace, _| workspace.url().to_string()),
        ""
    );

    type_into(cx, "url-input", "https://second.example/orders").unwrap();
    click(cx, "request-tab-0").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.url().to_string()),
        "https://first.example/users"
    );

    click(cx, "url-input").unwrap();
    cx.simulate_keystrokes("end");
    cx.simulate_input("?projected=first");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.url().to_string()),
        "https://first.example/users?projected=first"
    );

    click(cx, "close-tab-0").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.tab_count()),
        1
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.url().to_string()),
        "https://second.example/orders"
    );
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
        workspace.row_draft(RequestPane::Params) == Some(("q", "first-params"))
    }));

    click(cx, "new-tab-button").unwrap();
    type_into(cx, "row-key-input", "q").unwrap();
    type_into(cx, "row-value-input", "second").unwrap();
    click(cx, "request-tab-0").unwrap();

    // Appending proves the first tab's VM value was projected back into the reused controls.
    click(cx, "row-value-input").unwrap();
    cx.simulate_input("-restored");
    assert!(workspace.read_with(cx, |workspace, _| {
        workspace.row_draft(RequestPane::Params) == Some(("q", "first-params-restored"))
    }));
    assert!(workspace.read_with(cx, |workspace, _| {
        workspace.row_draft(RequestPane::Headers) == Some(("X-Tab-Draft", "one"))
    }));
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
        workspace.read_with(cx, |workspace, _| workspace.active_tab_index()),
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

    click(cx, "request-pane-authorization").unwrap();
    type_into(cx, "authorization-input", "Bearer ui-secret").unwrap();
    type_into(cx, "url-input", &format!("{}/secured", server.url())).unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.bearer_token().to_string()),
        "ui-secret"
    );
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

    click(cx, "request-pane-authorization").unwrap();
    click(cx, "auth-kind-basic").unwrap();
    type_into(cx, "basic-auth-username-input", "ui-user").unwrap();
    type_into(cx, "basic-auth-password-input", "ui-pass").unwrap();
    type_into(cx, "url-input", &format!("{}/basic-auth", server.url())).unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.authorization_kind()),
        AuthorizationKind::Basic
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.basic_username().to_string()),
        "ui-user"
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.basic_password().to_string()),
        "ui-pass"
    );
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
            .pre_request_script()
            .to_string()),
        ""
    );
    click(cx, "request-tab-0").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .pre_request_script()
            .to_string()),
        "const token = 'first';"
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.tests_script().to_string()),
        "status === 200"
    );
}

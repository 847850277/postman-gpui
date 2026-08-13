//! End-to-end interaction coverage for workspace-level MVVM features.

use gpui::{Modifiers, TestAppContext};
use postman_gpui::app::PostmanApp;

#[gpui::test]
fn new_switch_and_close_tabs_preserve_independent_drafts(cx: &mut TestAppContext) {
    let (app, cx) = cx.add_window_view(|_window, cx| PostmanApp::new(cx));

    app.update(cx, |app, cx| {
        app.type_url("https://first.example/users", cx);
    });

    let new_tab = cx
        .debug_bounds("new-tab-button")
        .expect("new tab button should render");
    cx.simulate_click(new_tab.center(), Modifiers::none());
    assert_eq!(app.read_with(cx, |app, _| app.tab_count()), 2);
    assert_eq!(
        app.read_with(cx, |app, _| app.current_url().to_string()),
        ""
    );

    app.update(cx, |app, cx| {
        app.type_url("https://second.example/orders", cx);
    });
    let first_tab = cx
        .debug_bounds("request-tab-0")
        .expect("first request tab should render");
    cx.simulate_click(first_tab.center(), Modifiers::none());
    assert_eq!(
        app.read_with(cx, |app, _| app.current_url().to_string()),
        "https://first.example/users"
    );

    let close_first = cx
        .debug_bounds("close-tab-0")
        .expect("first tab close button should render");
    cx.simulate_click(close_first.center(), Modifiers::none());
    assert_eq!(app.read_with(cx, |app, _| app.tab_count()), 1);
    assert_eq!(
        app.read_with(cx, |app, _| app.current_url().to_string()),
        "https://second.example/orders"
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

    let (app, cx) = cx.add_window_view(|_window, cx| PostmanApp::new(cx));
    app.update(cx, |app, cx| {
        app.type_url(&format!("{}/alpha-resource", server.url()), cx);
        app.click_send(cx);
        app.type_url(&format!("{}/beta-resource", server.url()), cx);
        app.click_send(cx);
    });
    assert_eq!(app.read_with(cx, |app, _| app.history_len()), 2);
    assert_eq!(app.read_with(cx, |app, cx| app.visible_history_len(cx)), 2);

    let search = cx
        .debug_bounds("history-search-input")
        .expect("history search input should render");
    cx.simulate_click(search.center(), Modifiers::none());
    cx.simulate_input("alpha-resource");

    assert_eq!(app.read_with(cx, |app, cx| app.visible_history_len(cx)), 1);
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

    let (app, cx) = cx.add_window_view(|_window, cx| PostmanApp::new(cx));
    let authorization_tab = cx
        .debug_bounds("request-pane-authorization")
        .expect("authorization tab should render");
    cx.simulate_click(authorization_tab.center(), Modifiers::none());

    let authorization_input = cx
        .debug_bounds("authorization-input")
        .expect("authorization input should render");
    cx.simulate_click(authorization_input.center(), Modifiers::none());
    cx.simulate_input("ui-secret");

    app.update(cx, |app, cx| {
        app.type_url(&format!("{}/secured", server.url()), cx);
        app.click_send(cx);
    });
    assert_eq!(
        app.read_with(cx, |app, _| app.current_bearer_token().to_string()),
        "ui-secret"
    );
    secured.assert();
}

#[gpui::test]
fn script_and_test_editors_are_saved_per_tab(cx: &mut TestAppContext) {
    let (app, cx) = cx.add_window_view(|_window, cx| PostmanApp::new(cx));

    let scripts_tab = cx
        .debug_bounds("request-pane-scripts")
        .expect("scripts tab should render");
    cx.simulate_click(scripts_tab.center(), Modifiers::none());
    let script_editor = cx
        .debug_bounds("script-editor")
        .expect("script editor should render");
    cx.simulate_click(script_editor.center(), Modifiers::none());
    cx.simulate_input("const token = 'first';");

    let tests_tab = cx
        .debug_bounds("request-pane-tests")
        .expect("tests tab should render");
    cx.simulate_click(tests_tab.center(), Modifiers::none());
    let tests_editor = cx
        .debug_bounds("tests-editor")
        .expect("tests editor should render");
    cx.simulate_click(tests_editor.center(), Modifiers::none());
    cx.simulate_input("status === 200");

    let new_tab = cx
        .debug_bounds("new-tab-button")
        .expect("new tab button should render");
    cx.simulate_click(new_tab.center(), Modifiers::none());
    assert_eq!(
        app.read_with(cx, |app, _| app.current_pre_request_script().to_string()),
        ""
    );

    let first_tab = cx
        .debug_bounds("request-tab-0")
        .expect("first request tab should render");
    cx.simulate_click(first_tab.center(), Modifiers::none());
    assert_eq!(
        app.read_with(cx, |app, _| app.current_pre_request_script().to_string()),
        "const token = 'first';"
    );
    assert_eq!(
        app.read_with(cx, |app, _| app.current_tests_script().to_string()),
        "status === 200"
    );
}

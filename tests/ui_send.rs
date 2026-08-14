//! UI-driven acceptance tests for the send/response path.
//!
//! User flows mutate the application through rendered controls. The injected workspace entity is
//! used for assertions and deterministic async preconditions, not as a second View command API.

#[path = "common/ui.rs"]
mod ui;

use gpui::{AppContext, TestAppContext};
use mockito::Matcher;
use postman_gpui::{
    app::{BodyKind, PostmanApp, ResponseState, WorkspaceViewModel},
    models::{MultipartValue, RequestBody},
};
use ui::{choose_method, click, replace_text, type_into};

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
fn query_parameter_is_saved_before_add_or_focus_change(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let request = server
        .mock("GET", "/live-query")
        .match_query(Matcher::UrlEncoded("source".into(), "typed".into()))
        .with_status(200)
        .with_body("query-saved")
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    type_into(cx, "url-input", &format!("{}/live-query", server.url())).unwrap();
    click(cx, "request-pane-params").unwrap();
    type_into(cx, "row-key-input", "source").unwrap();
    type_into(cx, "row-value-input", "typed").unwrap();

    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.url().to_string()),
        format!("{}/live-query?source=typed", server.url())
    );
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

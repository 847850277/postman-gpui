//! UI-driven acceptance tests for the send/response path.
//!
//! User flows mutate the application through rendered controls. The injected workspace entity is
//! used for assertions and deterministic async preconditions, not as a second View command API.

#[path = "common/ui.rs"]
mod ui;

use flate2::{
    write::{GzEncoder, ZlibEncoder},
    Compression,
};
use gpui::{AppContext, ClipboardItem, TestAppContext};
use mockito::Matcher;
use postman_gpui::{
    app::{
        BodyKind, KeyValueRow, MultipartDraftValue, PostmanApp, RequestBodyDraft, ResponseState,
        WorkspaceViewModel,
    },
    models::{
        HttpMethod, MultipartEditorPart, MultipartPart, MultipartValue, RequestBody,
        RequestEditorIntent,
    },
};
use std::io::Write;
use ui::{choose_method, click, replace_text, scroll_down, scroll_up, type_into};

const DEFAULT_ACCEPT_ENCODING: &str = "gzip,deflate,br";

fn gzip(body: &str) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(body.as_bytes())
        .expect("gzip test payload should be writable");
    encoder
        .finish()
        .expect("gzip test payload should be encodable")
}

fn deflate(body: &str) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(body.as_bytes())
        .expect("deflate test payload should be writable");
    encoder
        .finish()
        .expect("deflate test payload should be encodable")
}

fn brotli(body: &str) -> Vec<u8> {
    let mut encoded = Vec::new();
    {
        let mut encoder = brotli::CompressorWriter::new(&mut encoded, 4_096, 5, 22);
        encoder
            .write_all(body.as_bytes())
            .expect("Brotli test payload should be writable");
    }
    encoded
}

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
fn get_418_is_a_completed_response_with_exact_view_and_history_status(cx: &mut TestAppContext) {
    let response_body = "I'm a teapot!";
    let mut server = mockito::Server::new();
    let request = server
        .mock("GET", "/status/418")
        .match_body(Matcher::Exact(String::new()))
        .with_status(418)
        .with_header("content-type", "text/plain")
        .with_body(response_body)
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    let url = format!("{}/status/418", server.url());
    type_into(cx, "url-input", &url).unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.url().to_string()),
        url,
        "the active URL input must be authoritative before Send"
    );

    // Send directly from the active URL field: no Enter, Tab, blur, or submit-time backfill.
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    match workspace.read_with(cx, |workspace, _| workspace.response().clone()) {
        ResponseState::Success {
            status,
            body,
            headers,
            ..
        } => {
            assert_eq!(status, 418);
            assert_eq!(body, response_body);
            assert!(headers.iter().any(|(name, value)| {
                name.eq_ignore_ascii_case("content-type") && value == "text/plain"
            }));
        }
        other => panic!("HTTP 418 must complete as an HTTP response: {other:?}"),
    }

    for selector in [
        "response-container",
        "response-content",
        "response-status",
        "response-status-418",
        "response-copy-button",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "Issue #61 view contract element `{selector}` should be rendered"
        );
    }
    assert!(
        cx.debug_bounds("response-transport-error").is_none(),
        "a completed HTTP 418 response must not use the transport-error surface"
    );

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.history_len(), 1);
        let entry = &workspace.history()[0];
        assert_eq!(entry.request.method, HttpMethod::GET);
        assert_eq!(entry.request.url, url);
        assert_eq!(entry.request.body, RequestBody::None);
        assert_eq!(entry.status, Some(418));
        assert_eq!(entry.response_size, Some(response_body.len()));
    });
    for selector in [
        "history-method-0",
        "history-response-detail-0",
        "history-status-418-0",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "Issue #61 History contract element `{selector}` should be rendered"
        );
    }

    request.assert();
}

#[gpui::test]
fn get_redirect_follows_to_final_response_and_history_keeps_original_url(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let final_url = format!("{}/anything/redirected", server.url());
    let final_body = serde_json::json!({
        "method": "GET",
        "url": final_url.clone(),
    })
    .to_string();
    let redirect = server
        .mock("GET", "/redirect-to")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("url".into(), "/anything/redirected".into()),
            Matcher::UrlEncoded("status_code".into(), "302".into()),
        ]))
        .match_body(Matcher::Exact(String::new()))
        .with_status(302)
        .with_header("location", "/anything/redirected")
        .create();
    let target = server
        .mock("GET", "/anything/redirected")
        .match_body(Matcher::Exact(String::new()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(final_body.clone())
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    let original_url = format!(
        "{}/redirect-to?url=%2Fanything%2Fredirected&status_code=302",
        server.url()
    );
    type_into(cx, "url-input", &original_url).unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.url().to_string()),
        original_url,
        "the focused redirect URL must already be authoritative before Send"
    );

    // Keep the URL field active and Send directly. The first mock proves that the initial
    // outgoing URL is the original draft; the second proves that the client followed Location.
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
            let echo: serde_json::Value =
                serde_json::from_str(&body).expect("the final target should return JSON");
            assert_eq!(echo["method"], "GET");
            assert_eq!(echo["url"], final_url);
            assert!(headers.iter().any(|(name, value)| {
                name.eq_ignore_ascii_case("content-type") && value == "application/json"
            }));
        }
        other => panic!("redirect should complete with the final HTTP response: {other:?}"),
    }

    for selector in [
        "response-container",
        "response-content",
        "response-status",
        "response-copy-button",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "Issue #62 view contract element `{selector}` should be rendered"
        );
    }

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.history_len(), 1);
        let entry = &workspace.history()[0];
        assert_eq!(entry.request.method, HttpMethod::GET);
        assert_eq!(entry.request.url, original_url);
        assert_eq!(entry.request.body, RequestBody::None);
        assert_eq!(entry.status, Some(200));
        assert_eq!(entry.response_size, Some(final_body.len()));
    });
    for selector in ["history-method-0", "history-response-detail-0"] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "Issue #62 History contract element `{selector}` should be rendered"
        );
    }

    redirect.assert();
    target.assert();
}

#[gpui::test]
fn get_json_renders_the_stable_subset_and_keeps_the_full_lifecycle_in_sync(
    cx: &mut TestAppContext,
) {
    let response_body = r#"{"slideshow":{"author":"Yours Truly","date":"date of publication","slides":[{"title":"Wake up to WonderWidgets!","type":"all"},{"items":["Why WonderWidgets are great","Who buys WonderWidgets"],"title":"Overview","type":"all"}],"title":"Sample Slide Show"}}"#;
    let mut server = mockito::Server::new();
    let request = server
        .mock("GET", "/json")
        .match_query(Matcher::Missing)
        .match_header("content-type", Matcher::Missing)
        .match_body(Matcher::Exact(String::new()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(response_body)
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    let url = format!("{}/json", server.url());
    type_into(cx, "url-input", &url).unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.url().to_string()),
        url,
        "the focused JSON endpoint must already be authoritative before Send"
    );

    // Send directly from the active URL field: no Enter, Tab, blur, Add, or submit-time backfill.
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    let response_before_copy = workspace.read_with(cx, |workspace, _| workspace.response().clone());
    match &response_before_copy {
        ResponseState::Success {
            status,
            body,
            headers,
            ..
        } => {
            assert_eq!(*status, 200);
            assert_eq!(body, response_body, "ResponseState must keep the raw body");
            let json: serde_json::Value =
                serde_json::from_str(body).expect("the response body should parse as JSON");
            assert_eq!(json["slideshow"]["title"], "Sample Slide Show");
            assert!(headers.iter().any(|(name, value)| {
                name.eq_ignore_ascii_case("content-type") && value == "application/json"
            }));
        }
        other => panic!("GET /json should complete with an HTTP response: {other:?}"),
    }

    for selector in [
        "response-container",
        "response-content",
        "response-status",
        "response-status-200",
        "response-copy-button",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "Issue #63 view contract element `{selector}` should be rendered"
        );
    }
    assert!(cx.debug_bounds("response-transport-error").is_none());

    // Selecting the rendered Body copies the pretty-printed view, proving the visible surface is
    // valid JSON without snapshotting fields such as author, date, or slides.
    cx.write_to_clipboard(ClipboardItem::new_string("rendered sentinel".to_string()));
    click(cx, "response-content").unwrap();
    cx.simulate_keystrokes("ctrl-a ctrl-c");
    let rendered_body = cx
        .read_from_clipboard()
        .and_then(|item| item.text())
        .expect("the rendered response should be selectable");
    assert_ne!(
        rendered_body, response_body,
        "the JSON view should be formatted"
    );
    let rendered_json: serde_json::Value =
        serde_json::from_str(&rendered_body).expect("the rendered response should remain JSON");
    assert_eq!(rendered_json["slideshow"]["title"], "Sample Slide Show");

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.history_len(), 1);
        let entry = &workspace.history()[0];
        assert_eq!(entry.request.method, HttpMethod::GET);
        assert_eq!(entry.request.url, url);
        assert!(entry.request.headers.is_empty());
        assert_eq!(entry.request.body, RequestBody::None);
        assert_eq!(entry.status, Some(200));
        assert_eq!(entry.response_size, Some(response_body.len()));
    });
    for selector in [
        "history-method-0",
        "history-response-detail-0",
        "history-status-200-0",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "Issue #63 History contract element `{selector}` should be rendered"
        );
    }

    // Quick Copy deliberately uses the complete raw ResponseState body, not the formatted view.
    cx.write_to_clipboard(ClipboardItem::new_string("raw sentinel".to_string()));
    click(cx, "response-copy-button").unwrap();
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some(response_body.to_string())
    );
    assert!(cx.debug_bounds("response-copy-feedback").is_some());
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        response_before_copy
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.history_len()),
        1
    );

    request.assert();
}

#[gpui::test]
fn cookie_jar_stores_sends_and_clears_through_one_real_ui_session(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let set_cookie = server
        .mock("GET", "/cookies/set")
        .match_query(Matcher::UrlEncoded(
            "session".into(),
            "cookie-e2e-demo".into(),
        ))
        .match_header("cookie", Matcher::Missing)
        .with_status(302)
        .with_header("location", "/cookies")
        .with_header("set-cookie", "session=cookie-e2e-demo; Path=/")
        .create();
    let cookie_echo = server
        .mock("GET", "/cookies")
        .match_header("cookie", "session=cookie-e2e-demo")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"cookies":{"session":"cookie-e2e-demo"}}"#)
        .expect(2)
        .create();
    let empty_echo = server
        .mock("GET", "/cookies")
        .match_header("cookie", Matcher::Missing)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"cookies":{}}"#)
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    let set_url = format!("{}/cookies/set?session=cookie-e2e-demo", server.url());
    type_into(cx, "url-input", &set_url).unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.url().to_string()),
        set_url,
        "the focused cookie-setting URL must be authoritative before Send"
    );
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.cookie_count(), 1);
        assert_eq!(workspace.cookies()[0].name, "session");
        assert_eq!(workspace.cookies()[0].origin, server.url());
        assert_eq!(workspace.history_len(), 1);
        assert_eq!(workspace.history()[0].request.url, set_url);
        assert!(workspace.history()[0].request.headers.is_empty());
        let ResponseState::Success { status, body, .. } = workspace.response() else {
            panic!("the cookie-setting redirect should finish as a response");
        };
        assert_eq!(*status, 200);
        let body: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(body["cookies"]["session"], "cookie-e2e-demo");
    });
    assert!(
        cx.debug_bounds("request-pane-cookies").is_none(),
        "the application Cookie Jar is no longer a request editor tab"
    );
    assert!(cx.debug_bounds("cookie-jar-trigger").is_some());
    click(cx, "response-pane-cookies").unwrap();
    for selector in [
        "response-cookies-panel",
        "response-cookie-list",
        "response-cookie-row-0",
        "response-cookie-name-0",
        "response-cookie-storage-0",
        "response-open-cookie-jar",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "response cookie contract element `{selector}` should be rendered"
        );
    }
    click(cx, "response-open-cookie-jar").unwrap();
    assert!(cx.debug_bounds("cookie-jar-workspace-overlay").is_some());
    assert!(cx.debug_bounds("cookie-jar-panel").is_some());
    click(cx, "cookie-jar-close").unwrap();

    // A rendered New Request keeps the application-level transport session while creating a
    // clean request tab. Send is clicked directly from the active URL input.
    click(cx, "rail-new-request").unwrap();
    let cookies_url = format!("{}/cookies", server.url());
    type_into(cx, "url-input", &cookies_url).unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.url().to_string()),
        cookies_url
    );
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    let response_before_clear =
        workspace.read_with(cx, |workspace, _| workspace.response().clone());
    match &response_before_clear {
        ResponseState::Success { status, body, .. } => {
            assert_eq!(*status, 200);
            let body: serde_json::Value = serde_json::from_str(body).unwrap();
            assert_eq!(body["cookies"]["session"], "cookie-e2e-demo");
        }
        other => panic!("the later request should receive the automatic cookie: {other:?}"),
    }
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.history_len(), 2);
        assert_eq!(workspace.history()[0].request.url, cookies_url);
        assert!(
            workspace.history()[0].request.headers.is_empty(),
            "History keeps authored headers and must not persist the sensitive automatic Cookie"
        );
    });

    assert!(
        cx.debug_bounds("response-cookies-empty").is_some(),
        "the later /cookies response should expose Cookies (0)"
    );
    click(cx, "cookie-jar-trigger").unwrap();
    for selector in [
        "cookie-jar-workspace-overlay",
        "cookie-jar-panel",
        "cookie-jar-scope",
        "cookie-jar-count",
        "cookie-jar-clear-all",
        "cookie-jar-list",
        "cookie-row-0",
        "cookie-name-0",
        "cookie-origin-0",
        "cookie-value-protected-0",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "Issue #65 cookie contract element `{selector}` should be rendered"
        );
    }
    assert!(cx.debug_bounds("cookie-jar-empty").is_none());

    click(cx, "cookie-jar-clear-all").unwrap();
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.cookie_count(), 0);
        assert_eq!(workspace.last_cookie_clear_count(), Some(1));
        assert_eq!(workspace.history_len(), 2);
        assert_eq!(workspace.response(), &response_before_clear);
    });
    for selector in ["cookie-jar-empty", "cookie-jar-clear-feedback"] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "cleared cookie surface `{selector}` should be rendered"
        );
    }
    assert!(cx.debug_bounds("cookie-row-0").is_none());

    click(cx, "cookie-jar-close").unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.cookie_count(), 0);
        assert_eq!(workspace.history_len(), 3);
        let ResponseState::Success { status, body, .. } = workspace.response() else {
            panic!("the after-clear verification should complete as a response");
        };
        assert_eq!(*status, 200);
        let body: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(body["cookies"], serde_json::json!({}));
    });
    for selector in [
        "response-status-200",
        "history-status-200-0",
        "history-status-200-1",
        "history-status-200-2",
    ] {
        assert!(cx.debug_bounds(selector).is_some());
    }

    set_cookie.assert();
    cookie_echo.assert();
    empty_echo.assert();
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
fn head_and_options_preserve_bodyless_transport_headers_actions_and_history_methods(
    cx: &mut TestAppContext,
) {
    const ALLOW_METHODS: &str = "GET, POST, HEAD, PUT, DELETE, PATCH, OPTIONS";
    let mut server = mockito::Server::new();
    let head_request = server
        .mock("HEAD", "/get")
        .match_body(Matcher::Exact(String::new()))
        .with_status(200)
        .with_header("content-type", "application/json; charset=utf-8")
        .with_header("access-control-allow-origin", "*")
        .with_body("a HEAD response body must never surface")
        .create();
    let options_request = server
        .mock("OPTIONS", "/anything/options")
        .match_body(Matcher::Exact(String::new()))
        .with_status(200)
        .with_header("access-control-allow-methods", ALLOW_METHODS)
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    choose_method(cx, "HEAD").unwrap();
    let head_url = format!("{}/get", server.url());
    type_into(cx, "url-input", &head_url).unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.method(), HttpMethod::HEAD);
        assert_eq!(workspace.request_body(), RequestBody::None);
        let ResponseState::Success {
            status,
            body,
            headers,
            ..
        } = workspace.response()
        else {
            panic!("HEAD should complete as an HTTP response");
        };
        assert_eq!(*status, 200);
        assert!(body.is_empty(), "HEAD must not expose a response body");
        assert!(headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("content-type") && value == "application/json; charset=utf-8"
        }));
        assert_eq!(workspace.history_len(), 1);
        assert_eq!(workspace.history()[0].request.method, HttpMethod::HEAD);
        assert_eq!(workspace.history()[0].request.body, RequestBody::None);
    });
    assert!(cx.debug_bounds("response-pane-headers").is_some());
    click(cx, "response-pane-headers").unwrap();
    assert!(cx.debug_bounds("response-content").is_some());
    assert!(cx.debug_bounds("response-copy-button").is_none());
    head_request.assert();

    click(cx, "new-tab-button").unwrap();
    choose_method(cx, "OPTIONS").unwrap();
    let options_url = format!("{}/anything/options", server.url());
    type_into(cx, "url-input", &options_url).unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.method(), HttpMethod::OPTIONS);
        assert_eq!(workspace.request_body(), RequestBody::None);
        let ResponseState::Success {
            status,
            body,
            headers,
            ..
        } = workspace.response()
        else {
            panic!("OPTIONS should complete as an HTTP response");
        };
        assert_eq!(*status, 200);
        assert!(body.is_empty());
        assert!(headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("access-control-allow-methods") && value == ALLOW_METHODS
        }));
        assert_eq!(workspace.history_len(), 2);
        assert_eq!(workspace.history()[0].request.method, HttpMethod::OPTIONS);
        assert_eq!(workspace.history()[0].request.body, RequestBody::None);
        assert_eq!(workspace.history()[1].request.method, HttpMethod::HEAD);
    });
    assert!(cx.debug_bounds("response-copy-button").is_none());
    for selector in ["history-method-0", "history-method-1"] {
        assert!(cx.debug_bounds(selector).is_some());
    }
    options_request.assert();

    click(cx, "history-item-1").unwrap();
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.method(), HttpMethod::HEAD);
        assert_eq!(workspace.url(), head_url);
        assert_eq!(workspace.request_body(), RequestBody::None);
        assert!(matches!(workspace.response(), ResponseState::NotSent));
    });
    assert!(cx.debug_bounds("method-dropdown-selected-value").is_some());
    assert!(cx.debug_bounds("request-tab-method-1").is_some());

    click(cx, "history-item-0").unwrap();
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.method(), HttpMethod::OPTIONS);
        assert_eq!(workspace.url(), options_url);
        assert_eq!(workspace.request_body(), RequestBody::None);
        assert!(matches!(workspace.response(), ResponseState::NotSent));
    });
}

#[gpui::test]
fn compressed_responses_decode_through_real_controls_and_use_decoded_history_sizes(
    cx: &mut TestAppContext,
) {
    let cases = [
        (
            "/gzip",
            "gzip",
            r#"{"method":"GET","gzipped":true}"#,
            gzip as fn(&str) -> Vec<u8>,
        ),
        (
            "/deflate",
            "deflate",
            r#"{"method":"GET","deflated":true}"#,
            deflate as fn(&str) -> Vec<u8>,
        ),
        (
            "/brotli",
            "br",
            r#"{"method":"GET","brotli":true}"#,
            brotli as fn(&str) -> Vec<u8>,
        ),
    ];
    let mut server = mockito::Server::new();
    let mut requests = Vec::new();
    for (path, encoding, body, encode) in cases {
        requests.push(
            server
                .mock("GET", path)
                .match_header("accept-encoding", DEFAULT_ACCEPT_ENCODING)
                .match_body(Matcher::Exact(String::new()))
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_header("content-encoding", encoding)
                .with_body(encode(body))
                .create(),
        );
    }
    let corrupt = server
        .mock("GET", "/corrupt-gzip")
        .match_header("accept-encoding", DEFAULT_ACCEPT_ENCODING)
        .match_body(Matcher::Exact(String::new()))
        .with_status(200)
        .with_header("content-encoding", "gzip")
        .with_body("not a gzip stream")
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    choose_method(cx, "GET").unwrap();
    let mut urls = Vec::new();
    for (index, (path, _, body, _)) in cases.into_iter().enumerate() {
        let url = format!("{}{path}", server.url());
        if index == 0 {
            type_into(cx, "url-input", &url).unwrap();
        } else {
            replace_text(cx, "url-input", &url).unwrap();
        }
        urls.push(url.clone());
        assert_eq!(
            workspace.read_with(cx, |workspace, _| workspace.url().to_string()),
            url,
            "the latest compressed endpoint edit must be authoritative before Send"
        );

        click(cx, "send-button").unwrap();
        cx.run_until_parked();

        workspace.read_with(cx, |workspace, _| {
            assert_eq!(workspace.method(), HttpMethod::GET);
            assert!(workspace.headers().is_empty());
            assert_eq!(workspace.request_body(), RequestBody::None);
            let ResponseState::Success {
                status,
                body: decoded,
                headers,
                ..
            } = workspace.response()
            else {
                panic!("compressed response should complete successfully");
            };
            assert_eq!(*status, 200);
            assert_eq!(decoded, body);
            serde_json::from_str::<serde_json::Value>(decoded)
                .expect("the decoded response should be readable JSON");
            assert!(headers.iter().any(|(name, value)| {
                name.eq_ignore_ascii_case("content-type") && value == "application/json"
            }));
            assert!(headers.iter().all(|(name, _)| {
                !name.eq_ignore_ascii_case("content-encoding")
                    && !name.eq_ignore_ascii_case("content-length")
            }));

            assert_eq!(workspace.history_len(), index + 1);
            let entry = &workspace.history()[0];
            assert_eq!(entry.request.method, HttpMethod::GET);
            assert_eq!(entry.request.url, url);
            assert!(entry.request.headers.is_empty());
            assert_eq!(entry.request.body, RequestBody::None);
            assert_eq!(entry.status, Some(200));
            assert_eq!(entry.response_size, Some(body.len()));
        });

        for selector in [
            "response-container",
            "response-content",
            "response-status-200",
            "response-copy-button",
            "history-method-0",
            "history-status-200-0",
        ] {
            assert!(
                cx.debug_bounds(selector).is_some(),
                "Issue #67 lifecycle element `{selector}` should be rendered"
            );
        }
        cx.write_to_clipboard(ClipboardItem::new_string("compressed sentinel".to_string()));
        click(cx, "response-copy-button").unwrap();
        assert_eq!(
            cx.read_from_clipboard().and_then(|item| item.text()),
            Some(body.to_string()),
            "Quick Copy must use the canonical decoded body"
        );
        click(cx, "response-pane-headers").unwrap();
        assert!(cx.debug_bounds("response-content").is_some());
        click(cx, "response-pane-body").unwrap();
    }

    replace_text(cx, "url-input", &format!("{}/corrupt-gzip", server.url())).unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();
    match workspace.read_with(cx, |workspace, _| workspace.response().clone()) {
        ResponseState::Error { message } => assert!(
            message.to_ascii_lowercase().contains("decod"),
            "decoder failure should be readable: {message}"
        ),
        other => panic!("corrupt compressed bytes must produce an error state: {other:?}"),
    }
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.history_len()),
        3,
        "a decoder failure must not fabricate a successful History entry"
    );
    assert!(cx.debug_bounds("response-transport-error").is_some());
    assert!(cx.debug_bounds("response-copy-button").is_none());

    click(cx, "history-item-2").unwrap();
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.method(), HttpMethod::GET);
        assert_eq!(workspace.url(), urls[0]);
        assert!(workspace.headers().is_empty());
        assert_eq!(workspace.request_body(), RequestBody::None);
        assert!(matches!(workspace.response(), ResponseState::NotSent));
    });

    for request in requests {
        request.assert();
    }
    corrupt.assert();
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
            RequestBody::Json(r#"{"patched":true}"#.to_string()),
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
fn post_json_merges_generated_headers_with_a_custom_row_and_sends_the_active_value(
    cx: &mut TestAppContext,
) {
    let body = r#"{"name":"Ada","active":true}"#;
    let mut server = mockito::Server::new();
    let request = server
        .mock("POST", "/anything/post-json")
        .match_header("content-type", "application/json")
        .match_header("accept", "application/json")
        .match_header("x-scenario", "httpbingo-json")
        .match_body(Matcher::Exact(body.to_string()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"method":"POST","headers":{"Content-Type":["application/json"],"X-Scenario":["httpbingo-json"]},"data":"{\"name\":\"Ada\",\"active\":true}","json":{"name":"Ada","active":true}}"#,
        )
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    // Add the custom row first. Generated JSON defaults must not depend on Headers being empty.
    click(cx, "request-pane-headers").unwrap();
    type_into(cx, "row-key-input", "X-Scenario").unwrap();
    type_into(cx, "row-value-input", "httpbingo-json").unwrap();
    click(cx, "add-row-button").unwrap();

    choose_method(cx, "POST").unwrap();
    type_into(
        cx,
        "url-input",
        &format!("{}/anything/post-json", server.url()),
    )
    .unwrap();
    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-json").unwrap();
    replace_text(cx, "body-input", body).unwrap();

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.body_kind(), BodyKind::Json);
        assert_eq!(
            workspace.request_body(),
            RequestBody::Json(body.to_string())
        );
    });
    for selector in [
        "body-live-saved",
        "body-effective-headers",
        "body-effective-header-content-type",
        "body-effective-header-accept",
        "body-effective-header-x-scenario",
        "body-source-of-truth",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "Issue #57 design contract element `{selector}` should be rendered"
        );
    }

    // Send while the JSON editor is still active: no Enter, Tab, blur, or submit-time backfill.
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    match workspace.read_with(cx, |workspace, _| workspace.response().clone()) {
        ResponseState::Success { status, body, .. } => {
            assert_eq!(status, 200);
            let echo: serde_json::Value =
                serde_json::from_str(&body).expect("the mock should return a JSON echo");
            assert_eq!(echo["data"], r#"{"name":"Ada","active":true}"#);
            assert_eq!(echo["json"]["name"], "Ada");
            assert_eq!(echo["json"]["active"], true);
        }
        other => panic!("POST JSON should complete as a response: {other:?}"),
    }

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.history_len(), 1);
        let recorded = &workspace.history()[0].request;
        assert_eq!(recorded.body, RequestBody::Json(body.to_string()));
        for (name, value) in [
            ("Content-Type", "application/json"),
            ("Accept", "application/json"),
            ("X-Scenario", "httpbingo-json"),
        ] {
            assert!(recorded.headers.iter().any(|(actual_name, actual_value)| {
                actual_name.eq_ignore_ascii_case(name) && actual_value == value
            }));
        }
    });
    request.assert();
}

#[gpui::test]
fn put_raw_sends_active_exact_body_without_generated_content_type_and_records_history(
    cx: &mut TestAppContext,
) {
    let body = "plain text body";
    let mut server = mockito::Server::new();
    let request = server
        .mock("PUT", "/anything/raw")
        .match_header("content-type", Matcher::Missing)
        .match_body(Matcher::Exact(body.to_string()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"method":"PUT","data":"data:application/octet-stream;base64,cGxhaW4gdGV4dCBib2R5"}"#,
        )
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    choose_method(cx, "PUT").unwrap();
    type_into(cx, "url-input", &format!("{}/anything/raw", server.url())).unwrap();
    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-raw").unwrap();
    replace_text(cx, "body-input", body).unwrap();

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.method(), HttpMethod::PUT);
        assert_eq!(workspace.body_kind(), BodyKind::Raw);
        assert_eq!(workspace.body(), body);
        assert_eq!(workspace.request_body(), RequestBody::Raw(body.to_string()));
        assert!(workspace.effective_headers().is_empty());
    });
    for selector in [
        "body-raw-live-saved",
        "body-editor-shell",
        "body-input",
        "body-raw-effective-request",
        "body-raw-generated-header-count",
        "body-raw-content-type-state",
        "body-raw-exact-bytes",
        "body-raw-effective-body",
        "body-raw-request-target",
        "body-raw-ready-indicator",
        "body-source-of-truth",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "Issue #60 design contract element `{selector}` should be rendered"
        );
    }
    assert!(
        cx.debug_bounds("body-sample-json").is_none(),
        "Raw must not expose the JSON sample action"
    );

    // Send while the Raw editor is still active: no Enter, Tab, blur, or submit-time backfill.
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    match workspace.read_with(cx, |workspace, _| workspace.response().clone()) {
        ResponseState::Success { status, body, .. } => {
            assert_eq!(status, 200, "unexpected response body: {body}");
            let echo: serde_json::Value =
                serde_json::from_str(&body).expect("the mock should return a JSON echo");
            assert_eq!(echo["method"], "PUT");
            assert_eq!(
                echo["data"],
                "data:application/octet-stream;base64,cGxhaW4gdGV4dCBib2R5"
            );
        }
        other => panic!("PUT Raw should complete as a response: {other:?}"),
    }
    assert!(cx.debug_bounds("response-container").is_some());
    assert!(cx.debug_bounds("response-content").is_some());

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.history_len(), 1);
        let entry = &workspace.history()[0];
        assert_eq!(entry.request.method, HttpMethod::PUT);
        assert_eq!(entry.request.url, format!("{}/anything/raw", server.url()));
        assert_eq!(entry.request.body, RequestBody::Raw(body.to_string()));
        assert!(entry.request.headers.is_empty());
        assert_eq!(entry.status, Some(200));
    });
    assert!(cx.debug_bounds("history-method-0").is_some());
    request.assert();
}

#[gpui::test]
fn post_urlencoded_sends_the_active_value_and_excludes_disabled_rows(cx: &mut TestAppContext) {
    const ROW_SELECTORS: [&str; 10] = [
        "body-form-row-0",
        "body-form-row-1",
        "body-form-row-2",
        "body-form-row-3",
        "body-form-row-4",
        "body-form-row-5",
        "body-form-row-6",
        "body-form-row-7",
        "body-form-row-8",
        "body-form-row-9",
    ];
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
    const TOGGLE_SELECTORS: [&str; 8] = [
        "body-form-toggle-0",
        "body-form-toggle-1",
        "body-form-toggle-2",
        "body-form-toggle-3",
        "body-form-toggle-4",
        "body-form-toggle-5",
        "body-form-toggle-6",
        "body-form-toggle-7",
    ];
    let encoded_body = concat!(
        "name=Ada+Lovelace&active=true&tag=rust&",
        "unicode=%E4%BD%A0%E5%A5%BD+%E4%B8%96%E7%95%8C&",
        "reserved=a%26b%3Dc%2F%25+done&tag=gpui"
    );
    let mut server = mockito::Server::new();
    let request = server
        .mock("POST", "/anything/form")
        .match_header("content-type", "application/x-www-form-urlencoded")
        .match_header("accept", "application/json")
        .match_body(Matcher::Exact(encoded_body.to_string()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"method":"POST","form":{"name":["Ada Lovelace"],"active":["true"],"tag":["rust","gpui"],"unicode":["你好 世界"],"reserved":["a&b=c/% done"]}}"#,
        )
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    choose_method(cx, "POST").unwrap();
    let url = format!("{}/anything/form", server.url());
    type_into(cx, "url-input", &url).unwrap();
    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-url-encoded").unwrap();

    // Precreate blank rows before entering any data. Every Add click must append exactly one row,
    // preserve all existing drafts, and remain reachable outside the scrolling viewport.
    for (index, row_selector) in ROW_SELECTORS.iter().copied().enumerate().skip(1) {
        assert!(
            cx.debug_bounds(row_selector).is_none(),
            "row {index} should not exist before its Add click"
        );
        click(cx, "body-form-add-row").unwrap();
        assert!(
            cx.debug_bounds(row_selector).is_some(),
            "Add click should create row {index}"
        );
    }
    click(cx, "body-form-delete-9").unwrap();
    assert!(
        cx.debug_bounds("body-form-row-9").is_none(),
        "deleting one row must remove only that row"
    );

    scroll_up(cx, "body-form-scroll", 1000.0).unwrap();
    let rows = [
        ("name", "Ada Lovelace", true),
        ("active", "true", true),
        ("tag", "rust", true),
        ("ignored", "not-sent", false),
        ("", "draft-only", true),
        ("unicode", "你好 世界", true),
        ("reserved", "a&b=c/% done", true),
        ("tag", "gpui", true),
    ];
    for (index, (key, value, enabled)) in rows.into_iter().enumerate() {
        if index > 0 {
            scroll_down(cx, "body-form-scroll", 52.0).unwrap();
        }
        if !key.is_empty() {
            type_into(cx, KEY_SELECTORS[index], key).unwrap();
        }
        type_into(cx, VALUE_SELECTORS[index], value).unwrap();
        if !enabled {
            click(cx, TOGGLE_SELECTORS[index]).unwrap();
        }
    }

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.body_kind(), BodyKind::UrlEncoded);
        assert_eq!(
            workspace.request_body(),
            RequestBody::UrlEncoded(encoded_body.to_string()),
            "the active URL-encoded Value must already be persisted before blur"
        );
        let effective_headers = workspace.effective_headers();
        for (name, value) in [
            ("Content-Type", "application/x-www-form-urlencoded"),
            ("Accept", "application/json"),
        ] {
            assert_eq!(
                effective_headers
                    .iter()
                    .filter(|header| {
                        header.name.eq_ignore_ascii_case(name) && header.value == value
                    })
                    .count(),
                1,
                "`{name}` should appear exactly once in the effective request"
            );
        }
    });
    for selector in [
        "body-url-encoded-editor",
        "body-url-encoded-row-count",
        "body-form-table-header",
        "body-form-row-0",
        "body-form-row-1",
        "body-form-row-2",
        "body-form-row-8",
        "body-form-scroll",
        "body-form-scrollbar",
        "body-form-scrollbar-thumb",
        "body-form-add-row",
        "body-form-add-row-hint",
        "body-url-encoded-effective-request",
        "body-url-encoded-effective-body",
        "body-effective-header-content-type",
        "body-effective-header-accept",
        "body-url-encoded-ready-indicator",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "Issue #95 design contract element `{selector}` should be rendered"
        );
    }

    // Send while the final Value cell is active: no Enter, Tab, blur, or extra Add action.
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    match workspace.read_with(cx, |workspace, _| workspace.response().clone()) {
        ResponseState::Success { status, body, .. } => {
            assert_eq!(status, 200);
            let echo: serde_json::Value =
                serde_json::from_str(&body).expect("the mock should return a JSON form echo");
            assert_eq!(echo["method"], "POST");
            assert_eq!(echo["form"]["name"][0], "Ada Lovelace");
            assert_eq!(echo["form"]["active"][0], "true");
            assert_eq!(echo["form"]["tag"], serde_json::json!(["rust", "gpui"]));
            assert_eq!(echo["form"]["unicode"][0], "你好 世界");
            assert_eq!(echo["form"]["reserved"][0], "a&b=c/% done");
            assert!(echo["form"].get("ignored").is_none());
        }
        other => panic!("URL-encoded POST should complete as a response: {other:?}"),
    }
    assert!(cx.debug_bounds("response-content").is_some());

    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.history_len(), 1);
        let recorded = &workspace.history()[0].request;
        assert_eq!(recorded.method, HttpMethod::POST);
        assert_eq!(recorded.url, url);
        assert_eq!(
            recorded.body,
            RequestBody::UrlEncoded(encoded_body.to_string())
        );
        assert!(!recorded.body.searchable_text().contains("ignored"));
        assert!(!recorded.body.searchable_text().contains("draft-only"));
        for (name, value) in [
            ("Content-Type", "application/x-www-form-urlencoded"),
            ("Accept", "application/json"),
        ] {
            assert_eq!(
                recorded
                    .headers
                    .iter()
                    .filter(|(actual_name, actual_value)| {
                        actual_name.eq_ignore_ascii_case(name) && actual_value == value
                    })
                    .count(),
                1
            );
        }
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
fn multipart_text_rows_are_typed_live_and_sent_without_committing_the_active_cell(
    cx: &mut TestAppContext,
) {
    let mut server = mockito::Server::new();
    let submitted = server
        .mock("POST", "/post")
        .match_header(
            "content-type",
            Matcher::Regex("^multipart/form-data; boundary=".to_string()),
        )
        .match_body(Matcher::AllOf(vec![
            Matcher::Regex("name=\\\"note\\\"".to_string()),
            Matcher::Regex("hello multipart".to_string()),
            Matcher::Regex("name=\\\"category\\\"".to_string()),
            Matcher::Regex("gpui".to_string()),
        ]))
        .with_status(200)
        .with_body(r#"{"form":{"note":["hello multipart"],"category":["gpui"]}}"#)
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    choose_method(cx, "POST").unwrap();
    type_into(cx, "url-input", &format!("{}/post", server.url())).unwrap();
    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-form-data").unwrap();
    for expected_rows in [2, 3] {
        click(cx, "body-form-add-row").unwrap();
        let actual_rows = workspace.read_with(cx, |workspace, _| match workspace.body_draft() {
            RequestBodyDraft::Multipart(parts) => parts.len(),
            other => panic!("form-data must retain a multipart draft, got {other:?}"),
        });
        assert_eq!(
            actual_rows, expected_rows,
            "each Add form field click must append exactly one row"
        );
    }
    type_into(cx, "body-form-key-0", "note").unwrap();
    type_into(cx, "body-form-value-0", "hello multipart").unwrap();
    type_into(cx, "body-form-key-1", "category").unwrap();
    type_into(cx, "body-form-value-1", "gpui").unwrap();

    for selector in [
        "body-multipart-live-saved",
        "body-multipart-row-count",
        "body-multipart-editor",
        "body-multipart-effective-request",
        "body-multipart-effective-parts",
        "body-multipart-part-count",
        "body-multipart-boundary",
        "body-multipart-ready-indicator",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "multipart contract element `{selector}` should be rendered"
        );
    }

    let body = workspace.read_with(cx, |workspace, _| workspace.request_body().clone());
    let expected_body = RequestBody::Multipart(vec![
        MultipartPart::text("note", "hello multipart"),
        MultipartPart::text("category", "gpui"),
    ]);
    assert_eq!(body, expected_body);
    workspace.read_with(cx, |workspace, _| {
        let RequestBodyDraft::Multipart(parts) = workspace.body_draft() else {
            panic!("form-data must retain a multipart draft");
        };
        assert_eq!(parts.len(), 3, "the extra blank draft row must be retained");
        assert!(matches!(
            &parts[1].value,
            MultipartDraftValue::Text(value) if value == "gpui"
        ));
        assert!(parts[2].name.is_empty());
        assert!(matches!(
            &parts[2].value,
            MultipartDraftValue::Text(value) if value.is_empty()
        ));
        assert!(workspace
            .effective_headers()
            .iter()
            .all(|header| !header.name.eq_ignore_ascii_case("content-type")));
    });

    // `gpui` remains the active value. Send must not perform a last-minute control backfill.
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Success { status: 200, .. }
    ));
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.history_len(), 1);
        let completed = &workspace.history()[0].request;
        assert_eq!(completed.body, expected_body);
        assert!(completed
            .headers
            .iter()
            .all(|(name, _)| !name.eq_ignore_ascii_case("content-type")));
    });
    submitted.assert();
}

#[gpui::test]
fn multipart_file_picker_sends_a_typed_file_part(cx: &mut TestAppContext) {
    let fixture_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/httpbingo-upload.txt");
    assert!(fixture_path.is_file(), "upload fixture should exist");
    let expected_body = RequestBody::Multipart(vec![
        MultipartPart::text("note", "hello multipart"),
        MultipartPart {
            name: "upload".to_string(),
            value: MultipartValue::File {
                path: fixture_path.clone(),
                file_name: Some("httpbingo-upload.txt".to_string()),
                content_type: Some("text/plain".to_string()),
            },
        },
    ]);
    let mut server = mockito::Server::new();
    let upload = server
        .mock("POST", "/upload")
        .match_header(
            "content-type",
            Matcher::Regex("^multipart/form-data; boundary=".to_string()),
        )
        .match_body(Matcher::AllOf(vec![
            Matcher::Regex(
                "(?s)name=\\\"note\\\".*hello multipart.*name=\\\"upload\\\"; filename=\\\"httpbingo-upload.txt\\\""
                    .to_string(),
            ),
            Matcher::Regex("(?i)content-type: text/plain".to_string()),
            Matcher::Regex("hello from postman-gpui fixture".to_string()),
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
    type_into(cx, "body-form-key-0", "note").unwrap();
    type_into(cx, "body-form-value-0", "hello multipart").unwrap();
    click(cx, "body-form-add-row").unwrap();
    assert!(cx.debug_bounds("body-form-row-1").is_some());
    assert!(cx.debug_bounds("body-form-row-2").is_none());
    type_into(cx, "body-form-key-1", "upload").unwrap();
    click(cx, "body-form-type-1").unwrap();
    click(cx, "body-form-file-1").unwrap();
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

    for selector in [
        "body-form-file-1",
        "body-form-file-name-1",
        "body-form-file-metadata-1",
        "body-multipart-effective-parts",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "selected File row should render `{selector}`"
        );
    }
    let body = workspace.read_with(cx, |workspace, _| workspace.request_body().clone());
    assert_eq!(body, expected_body);
    workspace.read_with(cx, |workspace, _| {
        let RequestBodyDraft::Multipart(parts) = workspace.body_draft() else {
            panic!("form-data editor should retain a typed multipart draft");
        };
        assert!(matches!(
            &parts[1].value,
            MultipartDraftValue::File { path, file_name, content_type }
                if path == &fixture_path
                    && file_name.as_deref() == Some("httpbingo-upload.txt")
                    && content_type.as_deref() == Some("text/plain")
        ));
    });

    type_into(cx, "url-input", &format!("{}/upload", server.url())).unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Success { status: 201, .. }
    ));
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.history_len(), 1);
        assert_eq!(workspace.history()[0].request.body, expected_body);
        assert!(workspace.history()[0]
            .request
            .headers
            .iter()
            .all(|(name, _)| !name.eq_ignore_ascii_case("content-type")));
    });
    assert!(cx.debug_bounds("response-container").is_some());
    assert!(cx.debug_bounds("response-content").is_some());
    assert!(cx.debug_bounds("body-form-file-name-1").is_some());
    assert!(cx.debug_bounds("body-form-file-metadata-1").is_some());
    upload.assert();
}

#[gpui::test]
fn disabled_multipart_rows_preserve_values_metadata_and_history_editor_intent(
    cx: &mut TestAppContext,
) {
    let fixture_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/httpbingo-upload.txt");
    let expected_body = RequestBody::Multipart(vec![
        MultipartPart::text("enabled_note", "sent"),
        MultipartPart {
            name: "enabled_upload".to_string(),
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
            name: "enabled_note".to_string(),
            value: MultipartValue::Text("sent".to_string()),
        },
        MultipartEditorPart {
            enabled: true,
            name: "enabled_upload".to_string(),
            value: MultipartValue::File {
                path: fixture_path.clone(),
                file_name: Some("httpbingo-upload.txt".to_string()),
                content_type: Some("text/plain".to_string()),
            },
        },
        MultipartEditorPart {
            enabled: false,
            name: "disabled_upload".to_string(),
            value: MultipartValue::File {
                path: fixture_path.clone(),
                file_name: Some("httpbingo-upload.txt".to_string()),
                content_type: Some("text/plain".to_string()),
            },
        },
        MultipartEditorPart {
            enabled: false,
            name: "disabled_note".to_string(),
            value: MultipartValue::Text("omit-me".to_string()),
        },
    ]);
    let mut server = mockito::Server::new();
    let submitted = server
        .mock("POST", "/disabled")
        .match_header(
            "content-type",
            Matcher::Regex("^multipart/form-data; boundary=".to_string()),
        )
        .match_body(Matcher::AllOf(vec![
            Matcher::Regex("(?s)name=\"enabled_note\".*sent.*name=\"enabled_upload\"".to_string()),
            Matcher::Regex("hello from postman-gpui fixture".to_string()),
        ]))
        .with_status(200)
        .with_body(r#"{"ok":true}"#)
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    choose_method(cx, "POST").unwrap();
    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-form-data").unwrap();
    type_into(cx, "body-form-key-0", "enabled_note").unwrap();
    type_into(cx, "body-form-value-0", "sent").unwrap();

    click(cx, "body-form-add-row").unwrap();
    type_into(cx, "body-form-key-1", "enabled_upload").unwrap();
    click(cx, "body-form-type-1").unwrap();
    click(cx, "body-form-file-1").unwrap();
    let selected = fixture_path.clone();
    cx.simulate_path_prompt_response(move |_| Some(vec![selected]));
    cx.run_until_parked();

    click(cx, "body-form-add-row").unwrap();
    type_into(cx, "body-form-key-2", "disabled_upload").unwrap();
    click(cx, "body-form-type-2").unwrap();
    click(cx, "body-form-file-2").unwrap();
    let selected = fixture_path.clone();
    cx.simulate_path_prompt_response(move |_| Some(vec![selected]));
    cx.run_until_parked();
    click(cx, "body-form-toggle-2").unwrap();

    click(cx, "body-form-add-row").unwrap();
    type_into(cx, "body-form-key-3", "disabled_note").unwrap();
    type_into(cx, "body-form-value-3", "omit-me").unwrap();
    click(cx, "body-form-toggle-3").unwrap();

    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.request_body()),
        expected_body
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.request_editor_intent()),
        Some(expected_intent.clone())
    );
    for selector in [
        "body-form-ready-0",
        "body-form-ready-1",
        "body-form-omitted-2",
        "body-form-omitted-3",
        "body-multipart-omitted-count",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "missing `{selector}`");
    }

    click(cx, "body-form-toggle-2").unwrap();
    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.request_body()),
        RequestBody::Multipart(parts)
            if parts.iter().any(|part| part.name == "disabled_upload"
                && matches!(&part.value, MultipartValue::File { path, .. } if path == &fixture_path))
    ));
    click(cx, "body-form-toggle-2").unwrap();
    click(cx, "body-form-toggle-3").unwrap();
    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.request_body()),
        RequestBody::Multipart(parts)
            if parts.iter().any(|part| part.name == "disabled_note"
                && matches!(&part.value, MultipartValue::Text(value) if value == "omit-me"))
    ));
    click(cx, "body-form-toggle-3").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.request_body()),
        expected_body
    );

    type_into(cx, "url-input", &format!("{}/disabled", server.url())).unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();
    workspace.read_with(cx, |workspace, _| {
        assert!(matches!(
            workspace.response(),
            ResponseState::Success { status: 200, .. }
        ));
        assert_eq!(workspace.history_len(), 1);
        assert_eq!(workspace.history()[0].request.body, expected_body);
        assert_eq!(
            workspace.history()[0].editor_intent,
            Some(expected_intent.clone())
        );
    });
    submitted.assert();

    click(cx, "body-kind-none").unwrap();
    click(cx, "history-item-0").unwrap();
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.body_kind(), BodyKind::Multipart);
        assert_eq!(workspace.request_body(), expected_body);
        assert_eq!(
            workspace.request_editor_intent(),
            Some(expected_intent.clone())
        );
    });
    assert!(cx.debug_bounds("body-form-omitted-2").is_some());
    assert!(cx.debug_bounds("body-form-file-metadata-2").is_some());
}

#[gpui::test]
fn missing_multipart_file_replaces_old_response_with_error_and_preserves_editor_state(
    cx: &mut TestAppContext,
) {
    let fixture_path = std::env::temp_dir().join(format!(
        "postman-gpui-missing-upload-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should follow the Unix epoch")
            .as_nanos()
    ));
    std::fs::write(&fixture_path, "removed before Send")
        .expect("temporary upload fixture should be writable");
    let mut server = mockito::Server::new();
    let previous = server
        .mock("GET", "/previous")
        .with_status(200)
        .with_body(r#"{"previous":true}"#)
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    let previous_url = format!("{}/previous", server.url());
    type_into(cx, "url-input", &previous_url).unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();
    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Success { status: 200, .. }
    ));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.history_len()),
        1
    );
    previous.assert();

    choose_method(cx, "POST").unwrap();
    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-form-data").unwrap();
    type_into(cx, "body-form-key-0", "upload").unwrap();
    cx.simulate_keystrokes("enter");
    click(cx, "body-form-type-0").unwrap();
    click(cx, "body-form-file-0").unwrap();
    assert!(cx.did_prompt_for_paths());

    let selected = fixture_path.clone();
    cx.simulate_path_prompt_response(move |options| {
        assert!(options.files);
        assert!(!options.directories);
        assert!(!options.multiple);
        Some(vec![selected])
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("body-form-file-name-0").is_some());
    assert!(cx.debug_bounds("body-form-file-metadata-0").is_some());
    std::fs::remove_file(&fixture_path).expect("selected file should be removable before Send");

    replace_text(cx, "url-input", &format!("{}/upload", server.url())).unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    match workspace.read_with(cx, |workspace, _| workspace.response().clone()) {
        ResponseState::Error { message } => {
            assert!(message.contains("failed to read multipart file"));
            assert!(message.contains("field `upload`"));
            assert!(message.contains("postman-gpui-missing-upload"));
        }
        other => panic!("missing multipart file should fail before transport, got {other:?}"),
    }
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.history_len()),
        1,
        "a file-read failure must leave the previous successful History unchanged"
    );
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.history()[0].request.url, previous_url);
        let RequestBodyDraft::Multipart(parts) = workspace.body_draft() else {
            panic!("the correctable multipart File row must remain selected");
        };
        assert!(matches!(
            &parts[0].value,
            MultipartDraftValue::File { path, file_name, content_type }
                if path == &fixture_path
                    && file_name.as_deref().is_some_and(|name| name.starts_with("postman-gpui-missing-upload"))
                    && content_type.as_deref() == Some("text/plain")
        ));
    });
    for selector in [
        "body-multipart-file-error",
        "body-multipart-file-error-message",
        "body-form-file-0",
        "body-form-file-name-0",
        "body-form-file-metadata-0",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "file-read failure should retain `{selector}`"
        );
    }
    assert!(
        cx.debug_bounds("response-copy-button").is_none(),
        "an error without a populated response body must not expose Copy"
    );
}

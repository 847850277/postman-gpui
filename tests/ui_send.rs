//! UI-driven acceptance tests for the send/response path.
//!
//! These drive `PostmanApp` the same way a user does: fill the URL field,
//! click Send, and assert what the response panel and history sidebar show.
//! HTTP is a local mockito server so status codes are real.

use gpui::{AppContext, Modifiers, TestAppContext};
use mockito::Matcher;
use postman_gpui::app::PostmanApp;
use postman_gpui::models::HttpMethod;
use postman_gpui::ui::components::ResponseState;
use std::time::Duration;

#[gpui::test]
fn empty_url_shows_error_in_response_panel(cx: &mut TestAppContext) {
    let app = cx.new(PostmanApp::new);

    app.update(cx, |app, cx| {
        app.click_send(cx);
    });
    assert!(matches!(
        app.read_with(cx, |app, cx| app.response_state(cx)),
        ResponseState::Loading
    ));
    cx.run_until_parked();

    let state = app.read_with(cx, |app, cx| app.response_state(cx));
    match state {
        ResponseState::Error { message } => {
            assert!(
                message.to_lowercase().contains("url"),
                "error should mention URL, got: {message}"
            );
        }
        other => panic!("expected Error in the response panel, got {other:?}"),
    }

    let history_len = app.read_with(cx, |app, cx| app.history_len(cx));
    assert_eq!(history_len, 0, "failed send must not appear in history");
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

    let app = cx.new(PostmanApp::new);

    app.update(cx, |app, cx| {
        app.type_url(&format!("{}/missing", server.url()), cx);
        app.click_send(cx);
    });
    cx.run_until_parked();

    let state = app.read_with(cx, |app, cx| app.response_state(cx));
    match state {
        ResponseState::Success { status, body, .. } => {
            assert_eq!(status, 404, "response panel must show the real HTTP status");
            assert!(
                body.contains("missing"),
                "response panel should show the body, got: {body}"
            );
        }
        other => panic!("404 is a response, not a send failure: {other:?}"),
    }

    let history_len = app.read_with(cx, |app, cx| app.history_len(cx));
    assert_eq!(
        history_len, 1,
        "a completed HTTP exchange should appear in history"
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

    let app = cx.new(PostmanApp::new);

    app.update(cx, |app, cx| {
        app.choose_method(HttpMethod::PUT, cx);
        app.type_url(&format!("{}/item", server.url()), cx);
        app.set_body(r#"{"a":1}"#, cx);
        app.click_send(cx);
    });
    cx.run_until_parked();

    let state = app.read_with(cx, |app, cx| app.response_state(cx));
    match state {
        ResponseState::Success { status, body, .. } => {
            assert_eq!(status, 201);
            assert!(body.contains("ok"), "got: {body}");
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

    let url = format!("{}/health?source=gpui", server.url());
    let (app, cx) = cx.add_window_view(|_window, cx| PostmanApp::new(cx));

    let url_input = cx
        .debug_bounds("url-input")
        .expect("URL input should be rendered");
    cx.simulate_click(url_input.center(), Modifiers::none());
    cx.simulate_input(&url);

    let send_button = cx
        .debug_bounds("send-button")
        .expect("Send button should be rendered");
    cx.simulate_click(send_button.center(), Modifiers::none());
    cx.run_until_parked();

    let state = app.read_with(cx, |app, cx| app.response_state(cx));
    match state {
        ResponseState::Success {
            status,
            body,
            headers,
            ..
        } => {
            assert_eq!(status, 200, "response panel must show the real HTTP status");
            assert!(
                body.contains("minimal-flow-ok"),
                "response panel should show the body, got: {body}"
            );
            assert!(
                headers.iter().any(|(name, value)| {
                    name.eq_ignore_ascii_case("x-test-server") && value == "postman-gpui"
                }),
                "response panel state should retain response headers"
            );
        }
        other => panic!("expected a completed response, got {other:?}"),
    }
    assert!(
        cx.debug_bounds("response-content").is_some(),
        "successful response body should be rendered in the window"
    );

    let history_len = app.read_with(cx, |app, cx| app.history_len(cx));
    assert_eq!(
        history_len, 1,
        "the completed HTTP exchange should appear in history"
    );

    mock.assert();
}

#[gpui::test]
fn clicking_send_again_cancels_an_in_flight_request(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let _slow_mock = server
        .mock("GET", "/slow")
        .with_status(200)
        .with_chunked_body(|writer| {
            std::thread::sleep(Duration::from_millis(500));
            writer.write_all(b"too late")
        })
        .create();
    let app = cx.new(PostmanApp::new);

    app.update(cx, |app, cx| {
        app.type_url(&format!("{}/slow", server.url()), cx);
        app.click_send(cx);
    });
    assert!(matches!(
        app.read_with(cx, |app, cx| app.response_state(cx)),
        ResponseState::Loading
    ));

    app.update(cx, |app, cx| app.click_send(cx));
    cx.run_until_parked();

    assert!(matches!(
        app.read_with(cx, |app, cx| app.response_state(cx)),
        ResponseState::Cancelled
    ));
    assert_eq!(app.read_with(cx, |app, cx| app.history_len(cx)), 0);
}

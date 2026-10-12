//! Prototype Body workflows use the same native controls and send path as the application.
#[path = "common/ui.rs"]
mod ui;
use gpui::{px, size, AppContext, TestAppContext};
use postman_gpui::{
    app::{BodyKind, PostmanApp, RequestBodyDraft, RequestPane, ResponseState, WorkspaceViewModel},
    models::{HttpMethod, RequestBody},
};
use ui::{choose_body_kind, click, replace_text, type_into};

#[gpui::test]
fn json_format_validation_and_independent_modes(cx: &mut TestAppContext) {
    let model = cx.new(|_| {
        let mut model = WorkspaceViewModel::new();
        model
            .active_request_mut()
            .unwrap()
            .set_method(HttpMethod::POST);
        model
    });
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);
    click(cx, "request-pane-body").unwrap();
    choose_body_kind(cx, "body-kind-none").unwrap();
    assert!(cx.debug_bounds("body-empty").is_some());
    for id in [
        "body-kind-none",
        "body-kind-json",
        "body-kind-raw",
        "body-kind-url-encoded",
        "body-kind-form-data",
        "body-kind-binary",
    ] {
        assert!(cx.debug_bounds(id).is_some());
    }
    choose_body_kind(cx, "body-kind-json").unwrap();
    replace_text(cx, "body-input", "{broken").unwrap();
    click(cx, "body-format-json").unwrap();
    assert!(cx.debug_bounds("body-validation-error").is_some());
    assert_eq!(
        model.read_with(cx, |m, _| m.active_request().unwrap().body()),
        "{broken"
    );
    replace_text(cx, "body-input", "{\"name\":\"你好\",\"active\":true}").unwrap();
    click(cx, "body-format-json").unwrap();
    assert!(cx.debug_bounds("body-validation-error").is_none());
    let json = model.read_with(cx, |m, _| m.active_request().unwrap().body());
    assert!(json.contains("\n  \"name\": \"你好\""));
    choose_body_kind(cx, "body-kind-raw").unwrap();
    assert_eq!(
        model.read_with(cx, |m, _| m.active_request().unwrap().body()),
        ""
    );
    replace_text(cx, "body-input", "plain 🦀 text").unwrap();
    choose_body_kind(cx, "body-kind-json").unwrap();
    assert_eq!(
        model.read_with(cx, |m, _| m.active_request().unwrap().body()),
        json
    );
    choose_body_kind(cx, "body-kind-none").unwrap();
    assert_eq!(
        model.read_with(cx, |m, _| m.active_request().unwrap().request_body()),
        RequestBody::None
    );
    choose_body_kind(cx, "body-kind-raw").unwrap();
    assert_eq!(
        model.read_with(cx, |m, _| m.active_request().unwrap().body()),
        "plain 🦀 text"
    );
    click(cx, "body-header-source").unwrap();
    assert_eq!(
        model.read_with(cx, |m, _| m.active_request().unwrap().request_pane()),
        RequestPane::Headers
    );
}

#[gpui::test]
fn encoded_fields_preview_and_retention_are_visible_without_details(cx: &mut TestAppContext) {
    let model = cx.new(|_| {
        let mut model = WorkspaceViewModel::new();
        model
            .active_request_mut()
            .unwrap()
            .set_method(HttpMethod::POST);
        model
    });
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    let handle = cx.update(|window, _| window.window_handle());
    cx.simulate_window_resize(handle, size(px(1600.), px(1000.)));
    ui::open_http(cx);
    click(cx, "request-pane-body").unwrap();
    choose_body_kind(cx, "body-kind-url-encoded").unwrap();
    type_into(cx, "body-form-key-0", "tag").unwrap();
    type_into(cx, "body-form-value-0", "hello 世界").unwrap();
    click(cx, "body-form-add-row").unwrap();
    type_into(cx, "body-form-key-1", "tag").unwrap();
    type_into(cx, "body-form-value-1", "rust&gpui").unwrap();
    assert!(cx.debug_bounds("body-encoded-preview").is_some());
    assert!(cx.debug_bounds("body-details").is_none());
    assert!(cx.debug_bounds("request-context").is_none());
    let expected = "tag=hello+%E4%B8%96%E7%95%8C&tag=rust%26gpui";
    assert_eq!(
        model.read_with(cx, |m, _| m.active_request().unwrap().body()),
        expected
    );
    click(cx, "body-form-toggle-0").unwrap();
    assert_eq!(
        model.read_with(cx, |m, _| m.active_request().unwrap().body()),
        "tag=rust%26gpui"
    );
    choose_body_kind(cx, "body-kind-form-data").unwrap();
    choose_body_kind(cx, "body-kind-url-encoded").unwrap();
    model.read_with(cx, |m, _| {
        let RequestBodyDraft::UrlEncoded(rows) = m.active_request().unwrap().body_draft() else {
            panic!()
        };
        assert_eq!(rows.len(), 2);
        assert!(!rows[0].enabled);
        assert_eq!(rows[0].value, "hello 世界");
    });
    click(cx, "body-form-toggle-0").unwrap();
    assert_eq!(
        model.read_with(cx, |m, _| m.active_request().unwrap().body()),
        expected
    );
}

#[gpui::test]
fn binary_picker_sends_exact_file_bytes_and_remove_requires_a_file(cx: &mut TestAppContext) {
    let file = tempfile::Builder::new().suffix(".bin").tempfile().unwrap();
    let bytes = vec![0, 255, 128, 1, 13, 10, 0, 42];
    std::fs::write(file.path(), &bytes).unwrap();
    let mut server = mockito::Server::new();
    let sent = server
        .mock("POST", "/binary")
        .match_header("content-type", "application/octet-stream")
        .match_body(bytes.clone())
        .with_status(201)
        .create();
    let model = cx.new(|_| {
        let mut model = WorkspaceViewModel::new();
        let request = model.active_request_mut().unwrap();
        request.set_method(HttpMethod::POST);
        request.set_url(format!("{}/binary", server.url()));
        model
    });
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);
    click(cx, "request-pane-body").unwrap();
    choose_body_kind(cx, "body-kind-binary").unwrap();
    click(cx, "body-choose-file").unwrap();
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
    assert!(cx.debug_bounds("body-remove-file").is_none());
    click(cx, "body-choose-file").unwrap();
    let path = file.path().to_path_buf();
    cx.simulate_path_prompt_response({
        let path = path.clone();
        move |options| {
            assert!(options.files && !options.multiple && !options.directories);
            Some(vec![path])
        }
    });
    cx.run_until_parked();
    assert_eq!(
        model.read_with(cx, |m, _| m.active_request().unwrap().request_body()),
        RequestBody::File(path.clone())
    );
    assert_eq!(
        model.read_with(cx, |m, _| m.active_request().unwrap().binary_size()),
        Some(bytes.len() as u64)
    );
    choose_body_kind(cx, "body-kind-json").unwrap();
    choose_body_kind(cx, "body-kind-binary").unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();
    assert!(model.read_with(cx, |m, _| matches!(
        m.active_request().unwrap().response(),
        ResponseState::Success { status: 201, .. }
    )));
    sent.assert();
    assert_eq!(
        model.read_with(cx, |m, _| m.history()[0].request.body.clone()),
        RequestBody::File(path)
    );
    click(cx, "body-remove-file").unwrap();
    click(cx, "send-button").unwrap();
    assert!(cx.debug_bounds("body-validation-error").is_some());
    assert_eq!(model.read_with(cx, |m, _| m.history_len()), 1);
}

#[gpui::test]
fn late_binary_picker_does_not_replace_a_newly_selected_body_mode(cx: &mut TestAppContext) {
    let file = tempfile::NamedTempFile::new().unwrap();
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);
    click(cx, "request-pane-body").unwrap();
    choose_body_kind(cx, "body-kind-binary").unwrap();
    click(cx, "body-choose-file").unwrap();
    choose_body_kind(cx, "body-kind-json").unwrap();
    replace_text(cx, "body-input", "{invalid").unwrap();
    click(cx, "body-format-json").unwrap();
    assert!(cx.debug_bounds("body-validation-error").is_some());
    let path = file.path().to_path_buf();
    cx.simulate_path_prompt_response(move |_| Some(vec![path]));
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("body-validation-error").is_some(),
        "a stale picker must not clear the JSON error"
    );
    model.read_with(cx, |m, _| {
        let request = m.active_request().unwrap();
        assert_eq!(request.body_kind(), BodyKind::Json);
        assert_eq!(request.body(), "{invalid");
    });
}

#[gpui::test]
fn late_binary_picker_does_not_overwrite_a_reloaded_request(cx: &mut TestAppContext) {
    let first = tempfile::NamedTempFile::new().unwrap();
    let second = tempfile::NamedTempFile::new().unwrap();
    let second_path = second.path().to_path_buf();
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);
    click(cx, "request-pane-body").unwrap();
    choose_body_kind(cx, "body-kind-binary").unwrap();
    // The picker belongs to the draft before reload, even though the tab and type stay the same.
    click(cx, "body-choose-file").unwrap();
    model.update(cx, |workspace, cx| {
        let mut replacement =
            postman_gpui::models::Request::new(HttpMethod::PUT, "https://example.test/reloaded");
        replacement.body = RequestBody::File(second_path.clone());
        assert!(workspace.load_request(&replacement));
        cx.notify();
    });
    let first_path = first.path().to_path_buf();
    cx.simulate_path_prompt_response(move |_| Some(vec![first_path]));
    cx.run_until_parked();
    assert!(cx.debug_bounds("body-validation-error").is_none());
    model.read_with(cx, |workspace, _| {
        let request = workspace.active_request().unwrap();
        assert_eq!(request.url(), "https://example.test/reloaded");
        assert_eq!(request.request_body(), RequestBody::File(second_path));
        assert_eq!(
            request.binary_size(),
            None,
            "stale metadata must be ignored"
        );
        assert!(!request.is_dirty(), "reload must remain unmodified");
    });
}

#[gpui::test]
fn raw_format_select_sets_the_header_used_by_send(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let sent = server
        .mock("PUT", "/xml")
        .match_header("content-type", "application/xml")
        .match_body("<name>你好</name>")
        .with_status(200)
        .create();
    let model = cx.new(|_| {
        let mut model = WorkspaceViewModel::new();
        let request = model.active_request_mut().unwrap();
        request.set_method(HttpMethod::PUT);
        request.set_url(format!("{}/xml", server.url()));
        model
    });
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    let handle = cx.update(|window, _| window.window_handle());
    cx.simulate_window_resize(handle, size(px(1600.), px(1000.)));
    ui::open_http(cx);
    click(cx, "request-pane-body").unwrap();
    choose_body_kind(cx, "body-kind-raw").unwrap();
    ui::choose_option(cx, "body-raw-format", 1);
    assert_eq!(
        model.read_with(cx, |m, _| m.active_request().unwrap().raw_body_format()),
        postman_gpui::models::request_draft::RawBodyFormat::Xml,
        "selection must commit immediately"
    );
    replace_text(cx, "body-input", "<name>你好</name>").unwrap();
    choose_body_kind(cx, "body-kind-json").unwrap();
    choose_body_kind(cx, "body-kind-raw").unwrap();
    assert_eq!(
        model.read_with(cx, |m, _| m.active_request().unwrap().raw_body_format()),
        postman_gpui::models::request_draft::RawBodyFormat::Xml
    );
    click(cx, "send-button").unwrap();
    cx.run_until_parked();
    assert!(model.read_with(cx, |m, _| matches!(
        m.active_request().unwrap().response(),
        ResponseState::Success { status: 200, .. }
    )));
    sent.assert();
}

#[gpui::test]
fn form_fields_wrap_inside_the_minimum_request_width(cx: &mut TestAppContext) {
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    let handle = cx.update(|window, _| window.window_handle());
    cx.simulate_window_resize(handle, size(px(1440.), px(960.)));
    click(cx, "nav-http").unwrap();
    click(cx, "request-pane-body").unwrap();
    choose_body_kind(cx, "body-kind-url-encoded").unwrap();
    type_into(cx, "body-form-key-0", "label").unwrap();
    type_into(cx, "body-form-value-0", "kept at any width").unwrap();
    click(cx, "response-resize-handle").unwrap();
    ui::press(cx, "home");
    for _ in 0..4 {
        let _ = cx.debug_bounds("body-form-row-0");
    }
    let panel = cx.debug_bounds("request-container").unwrap();
    let key = cx.debug_bounds("body-form-key-0").unwrap();
    let value = cx.debug_bounds("body-form-value-0").unwrap();
    assert!(
        value.top() >= key.bottom(),
        "narrow row separates key and value: {key:?} {value:?}"
    );
    for field in [key, value, cx.debug_bounds("body-form-delete-0").unwrap()] {
        assert!(field.left() >= panel.left() && field.right() <= panel.right());
    }
    assert!(cx.debug_bounds("body-form-table-header").is_none());
    ui::press(cx, "end");
    for _ in 0..4 {
        let _ = cx.debug_bounds("body-form-row-0");
    }
    assert!(cx.debug_bounds("body-form-table-header").is_some());
    assert_eq!(
        model.read_with(cx, |m, _| m.active_request().unwrap().body()),
        "label=kept+at+any+width"
    );
}

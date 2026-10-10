//! Prototype request panes share context while their native editors retain real draft values.
#[path = "common/ui.rs"]
mod ui;
use gpui::{px, size, AppContext, ClipboardItem, TestAppContext};
use postman_gpui::app::{BodyKind, PostmanApp, RequestPane, WorkspaceViewModel};

#[gpui::test]
fn request_context_and_compact_forms_follow_the_selected_request(cx: &mut TestAppContext) {
    let model = cx.new(|_| {
        let mut model = WorkspaceViewModel::new();
        let request = model.active_request_mut().unwrap();
        request.set_url("https://example.test/users?page=1&limit=10#results");
        request.upsert_header("Accept", "application/json");
        request.set_body_kind(BodyKind::Json);
        request.set_body("{\"name\":\"你好🦀\"}");
        request.set_bearer_token("test-only-token");
        model
    });
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    let handle = cx.update(|window, _| window.window_handle());
    cx.simulate_window_resize(handle, size(px(1920.), px(1080.)));
    ui::open_http(cx);
    ui::click(cx, "rail-history").unwrap();
    let context = cx.debug_bounds("request-context").unwrap();
    for pane in [
        "request-pane-params",
        "request-pane-headers",
        "request-pane-body",
        "request-pane-authorization",
    ] {
        ui::click(cx, pane).unwrap();
        if pane != "request-pane-body" {
            assert_eq!(cx.debug_bounds("request-context").unwrap(), context);
            assert!(cx.debug_bounds("effective-url-preview").unwrap().bottom() < context.bottom());
        } else {
            assert!(cx.debug_bounds("request-context").is_none());
        }
        if pane == "request-pane-body" {
            assert!(cx.debug_bounds("body-details").is_none());
            let editor = cx.debug_bounds("body-input").unwrap();
            assert!(editor.size.height > px(250.));
            assert!(cx.debug_bounds("body-kind-json").is_some());
            assert!(cx.debug_bounds("body-kind-binary").is_some());
            ui::choose_body_kind(cx, "body-kind-raw").unwrap();
            ui::choose_body_kind(cx, "body-kind-json").unwrap();
            assert_eq!(
                model.read_with(cx, |m, _| m.active_request().unwrap().body()),
                "{\"name\":\"你好🦀\"}"
            );
            ui::show_body_details(cx).unwrap();
            assert!(cx.debug_bounds("body-effective-headers").is_some());
            ui::click(cx, "body-details-toggle").unwrap();
        }
    }
    let kind = cx.debug_bounds("authorization-kind-selector").unwrap();
    let token = cx.debug_bounds("authorization-input").unwrap();
    assert!(kind.right() < token.left());
    assert_eq!(kind.bottom(), token.bottom());
    // Masking keeps the real credential in the draft and prevents clipboard disclosure.
    ui::click(cx, "authorization-input").unwrap();
    cx.write_to_clipboard(ClipboardItem::new_string("sentinel".into()));
    ui::press(cx, "cmd-a cmd-c cmd-x");
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().as_deref(),
        Some("sentinel")
    );
    assert_eq!(
        model.read_with(cx, |m, _| m
            .active_request()
            .unwrap()
            .bearer_token()
            .to_string()),
        "test-only-token"
    );
    ui::click(cx, "request-pane-headers").unwrap();
    ui::click(cx, "request-auth-summary").unwrap();
    assert_eq!(
        model.read_with(cx, |m, _| m.active_request().unwrap().request_pane()),
        RequestPane::Authorization
    );
    cx.simulate_window_resize(handle, size(px(960.), px(640.)));
    assert!(
        cx.debug_bounds("request-context").is_none(),
        "stacked mode gives space to the editor"
    );
    for pane in [
        "request-pane-params",
        "request-pane-headers",
        "request-pane-body",
        "request-pane-authorization",
    ] {
        ui::click(cx, pane).unwrap();
        let editor = cx.debug_bounds("request-panel").unwrap();
        assert!(editor.size.height >= px(100.));
        assert!(editor.right() <= px(960.));
    }
}

#[gpui::test]
fn descriptions_follow_rows_tabs_and_resize_without_entering_the_request(cx: &mut TestAppContext) {
    let model = cx.new(|_| {
        let mut model = WorkspaceViewModel::new();
        model
            .active_request_mut()
            .unwrap()
            .set_url("https://example.test/users?page=1&limit=10");
        model
    });
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    let handle = cx.update(|window, _| window.window_handle());
    cx.simulate_window_resize(handle, size(px(1920.), px(1080.)));
    ui::open_http(cx);
    ui::click(cx, "rail-history").unwrap();
    ui::replace_text(cx, "param-row-description-input-0", "Page number").unwrap();
    ui::replace_text(cx, "param-row-description-input-1", "Results per page").unwrap();
    ui::replace_text(cx, "param-row-description-input-2", "Draft note").unwrap();
    ui::click(cx, "add-row-button").unwrap();
    model.read_with(cx, |m, _| {
        let request = m.active_request().unwrap();
        assert_eq!(request.params()[0].description, "Page number");
        assert_eq!(request.params()[2].description, "Draft note");
        assert_eq!(request.request_draft().row_description(false, None), "");
        assert_eq!(
            request.effective_url(),
            "https://example.test/users?page=1&limit=10"
        );
    });
    ui::click(cx, "param-row-delete-0").unwrap();
    ui::click(cx, "request-pane-headers").unwrap();
    ui::replace_text(cx, "row-key-input", "X-Trace").unwrap();
    ui::replace_text(cx, "row-value-input", "example").unwrap();
    ui::replace_text(cx, "header-row-description-input-0", "Trace note").unwrap();
    ui::click(cx, "add-row-button").unwrap();
    ui::click(cx, "new-tab-button").unwrap();
    ui::click(cx, "request-tab-0").unwrap();
    ui::click(cx, "request-pane-params").unwrap();
    assert_eq!(
        model.read_with(cx, |m, _| m.active_request().unwrap().params()[0]
            .description
            .clone()),
        "Results per page"
    );
    cx.simulate_window_resize(handle, size(px(1024.), px(768.)));
    cx.simulate_window_resize(handle, size(px(1920.), px(1080.)));
    ui::replace_text(cx, "param-row-description-input-0", "Updated note").unwrap();
    model.read_with(cx, |m, _| {
        let request = m.active_request().unwrap();
        assert_eq!(request.params()[0].description, "Updated note");
        assert_eq!(request.headers()[0].description, "Trace note");
        assert_eq!(
            request.effective_url(),
            "https://example.test/users?limit=10"
        );
        assert_eq!(
            request.request_construction().request().headers,
            vec![("X-Trace".into(), "example".into())]
        );
    });
}

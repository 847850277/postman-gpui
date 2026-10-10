//! Clipboard acceptance coverage for Kit inputs and application text editors.

#[path = "common/ui.rs"]
mod ui;

use std::time::Duration;

use gpui::{AppContext, ClipboardItem, TestAppContext};
use gpui_kit::test::TestWindowExt;
use postman_gpui::app::{PostmanApp, ResponseState, WorkspaceViewModel};
use ui::{click, right_click};

fn clipboard_text(cx: &TestAppContext) -> String {
    cx.read_from_clipboard()
        .and_then(|item| item.text())
        .unwrap_or_default()
}

#[gpui::test]
fn platform_clipboard_shortcuts_cover_all_editable_input_types(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);

    cx.write_to_clipboard(ClipboardItem::new_string(
        "https://clipboard.example/items".to_string(),
    ));
    click(cx, "url-input").unwrap();
    cx.simulate_keystrokes("ctrl-v ctrl-a ctrl-c");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .url()
            .to_string()),
        "https://clipboard.example/items"
    );
    assert_eq!(clipboard_text(cx), "https://clipboard.example/items");
    cx.simulate_keystrokes("ctrl-x");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .url()
            .to_string()),
        ""
    );

    click(cx, "request-pane-authorization").unwrap();
    cx.write_to_clipboard(ClipboardItem::new_string("clipboard-token".to_string()));
    click(cx, "authorization-input").unwrap();
    cx.simulate_keystrokes("cmd-v cmd-a cmd-c");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .bearer_token()
            .to_string()),
        "clipboard-token"
    );
    assert_eq!(clipboard_text(cx), "clipboard-token");
    cx.simulate_keystrokes("cmd-x");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .bearer_token()
            .to_string()),
        "clipboard-token"
    );
    cx.simulate_keystrokes("cmd-v");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .bearer_token()
            .to_string()),
        "clipboard-token"
    );

    click(cx, "request-pane-body").unwrap();
    ui::choose_body_kind(cx, "body-kind-json").unwrap();
    cx.write_to_clipboard(ClipboardItem::new_string(
        "{\n  \"copied\": true\n}".to_string(),
    ));
    click(cx, "body-input").unwrap();
    cx.simulate_keystrokes("ctrl-v ctrl-a ctrl-c");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .body()
            .to_string()),
        "{\n  \"copied\": true\n}"
    );
    assert_eq!(clipboard_text(cx), "{\n  \"copied\": true\n}");

    ui::choose_body_kind(cx, "body-kind-raw").unwrap();
    click(cx, "body-input").unwrap();
    cx.simulate_keystrokes("ctrl-a ctrl-x");
    cx.write_to_clipboard(ClipboardItem::new_string("raw clipboard body".to_string()));
    cx.simulate_keystrokes("ctrl-v ctrl-a ctrl-c");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .body()
            .to_string()),
        "raw clipboard body"
    );
    assert_eq!(clipboard_text(cx), "raw clipboard body");

    ui::choose_body_kind(cx, "body-kind-url-encoded").unwrap();
    cx.write_to_clipboard(ClipboardItem::new_string("pizza".to_string()));
    click(cx, "body-form-key-0").unwrap();
    cx.simulate_keystrokes("ctrl-v ctrl-a ctrl-c tab");
    assert_eq!(clipboard_text(cx), "pizza");

    cx.write_to_clipboard(ClipboardItem::new_string("margherita".to_string()));
    cx.simulate_keystrokes("ctrl-v ctrl-a ctrl-c");
    assert_eq!(clipboard_text(cx), "margherita");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .body()
            .to_string()),
        "pizza=margherita"
    );
}

#[gpui::test]
fn right_click_menus_paste_into_editors_and_copy_the_response(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let response = server
        .mock("GET", "/clipboard")
        .with_status(200)
        .with_body("response copied from the menu")
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);

    cx.write_to_clipboard(ClipboardItem::new_string(format!(
        "{}/clipboard",
        server.url()
    )));
    right_click(cx, "url-input").unwrap();
    // Kit uses the OS menu; the headless GPUI platform cannot select NSMenu items.
    // Exercise the same public action here; native menu interaction is checked manually.
    cx.update(|window, cx| window.dispatch_action(Box::new(gpui_kit::component::input::Paste), cx));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .url()
            .to_string()),
        format!("{}/clipboard", server.url())
    );

    click(cx, "request-pane-authorization").unwrap();
    cx.write_to_clipboard(ClipboardItem::new_string("menu-token".to_string()));
    right_click(cx, "authorization-input").unwrap();
    cx.update(|window, cx| window.dispatch_action(Box::new(gpui_kit::component::input::Paste), cx));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .bearer_token()
            .to_string()),
        "menu-token"
    );

    click(cx, "send-button").unwrap();
    cx.run_until_parked();
    click(cx, "response-content").unwrap();
    cx.simulate_keystrokes("ctrl-a ctrl-c");
    assert_eq!(clipboard_text(cx), "response copied from the menu");
    cx.write_to_clipboard(ClipboardItem::new_string("menu start".to_string()));
    right_click(cx, "response-content").unwrap();
    cx.update(|window, app| {
        window.render_frame(app);
        let menu = window.within("popup-menu");
        assert_eq!(menu.find(0usize).label(), Some("Copy"));
        assert_eq!(menu.find(1usize).label(), Some("Select All"));
        assert!(menu.try_find(2usize).is_none());
    });
    // Select All is the second item in the real Kit popup. Keyboard activation also
    // verifies that the menu routes the action back to the response editor.
    cx.simulate_keystrokes("down down enter");
    cx.write_to_clipboard(ClipboardItem::new_string("menu sentinel".to_string()));
    right_click(cx, "response-content").unwrap();
    cx.simulate_keystrokes("down enter");
    assert_eq!(clipboard_text(cx), "response copied from the menu");
    response.assert();
}

#[gpui::test]
fn populated_response_quick_copy_uses_the_full_raw_body_without_mutating_state(
    cx: &mut TestAppContext,
) {
    let raw_body = r#"{"compact":true,"message":"copy me exactly"}"#;
    let mut server = mockito::Server::new();
    let response = server
        .mock("GET", "/quick-copy")
        .with_status(500)
        .with_header("content-type", "application/json")
        .with_body(raw_body)
        .create();
    let next_body = "the current response replaces the previous clipboard value";
    let next_response = server
        .mock("GET", "/quick-copy-next")
        .with_status(200)
        .with_body(next_body)
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);

    assert!(
        cx.debug_bounds("response-copy-button").is_none(),
        "Not sent must not expose an active response-copy action"
    );

    cx.write_to_clipboard(ClipboardItem::new_string(format!(
        "{}/quick-copy",
        server.url()
    )));
    click(cx, "url-input").unwrap();
    cx.simulate_keystrokes("ctrl-v");
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    let response_before_copy = workspace.read_with(cx, |workspace, _| {
        workspace.active_request().unwrap().response().clone()
    });
    assert!(matches!(
        response_before_copy,
        ResponseState::Success { status: 500, .. }
    ));
    let history_len_before_copy = workspace.read_with(cx, |workspace, _| workspace.history_len());
    assert!(cx.debug_bounds("response-copy-button").is_some());

    cx.write_to_clipboard(ClipboardItem::new_string("sentinel".to_string()));
    click(cx, "response-copy-button").unwrap();
    assert_eq!(clipboard_text(cx), raw_body);
    assert!(cx.debug_bounds("response-copy-feedback").is_some());
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .response()
            .clone()),
        response_before_copy
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.history_len()),
        history_len_before_copy
    );

    // The click focuses the action, so keyboard activation must copy the current body again.
    cx.write_to_clipboard(ClipboardItem::new_string("keyboard sentinel".to_string()));
    cx.simulate_keystrokes("enter");
    assert_eq!(clipboard_text(cx), raw_body);

    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert!(cx.debug_bounds("response-copy-feedback").is_none());
    assert!(cx.debug_bounds("response-copy-button").is_some());

    cx.write_to_clipboard(ClipboardItem::new_string(format!(
        "{}/quick-copy-next",
        server.url()
    )));
    click(cx, "url-input").unwrap();
    cx.simulate_keystrokes("ctrl-a ctrl-v");
    click(cx, "send-button").unwrap();
    cx.run_until_parked();
    cx.write_to_clipboard(ClipboardItem::new_string("old response".to_string()));
    click(cx, "response-copy-button").unwrap();
    assert_eq!(clipboard_text(cx), next_body);

    response.assert();
    next_response.assert();
}

#[gpui::test]
fn empty_response_body_does_not_render_the_quick_copy_action(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let response = server.mock("GET", "/no-content").with_status(204).create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);

    cx.write_to_clipboard(ClipboardItem::new_string(format!(
        "{}/no-content",
        server.url()
    )));
    click(cx, "url-input").unwrap();
    cx.simulate_keystrokes("ctrl-v");
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.active_request().unwrap().response().clone()),
        ResponseState::Success { status: 204, ref body, .. } if body.is_empty()
    ));
    assert!(cx.debug_bounds("response-copy-button").is_none());
    response.assert();
}

#[gpui::test]
fn form_cell_right_click_menu_preserves_single_line_values(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);

    click(cx, "request-pane-body").unwrap();
    ui::choose_body_kind(cx, "body-kind-url-encoded").unwrap();
    cx.write_to_clipboard(ClipboardItem::new_string("menu\nkey".to_string()));
    right_click(cx, "body-form-key-0").unwrap();
    // Single-line cells use Kit's OS menu, so invoke its public action on the headless platform.
    cx.update(|window, cx| window.dispatch_action(Box::new(gpui_kit::component::input::Paste), cx));
    cx.simulate_keystrokes("tab");

    cx.write_to_clipboard(ClipboardItem::new_string("menu\r\nvalue".to_string()));
    right_click(cx, "body-form-value-0").unwrap();
    cx.update(|window, cx| window.dispatch_action(Box::new(gpui_kit::component::input::Paste), cx));

    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .body()
            .to_string()),
        "menukey=menuvalue"
    );
}

#[gpui::test]
fn masked_password_allows_paste_and_history_without_copy_or_cut_disclosure(
    cx: &mut TestAppContext,
) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);

    click(cx, "request-pane-authorization").unwrap();
    ui::choose_auth_kind(cx, "auth-kind-basic").unwrap();
    cx.write_to_clipboard(ClipboardItem::new_string("pässword-🔐".to_string()));
    click(cx, "basic-auth-password-input").unwrap();
    cx.simulate_keystrokes("ctrl-v");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .basic_password()
            .to_string()),
        "pässword-🔐"
    );

    cx.write_to_clipboard(ClipboardItem::new_string("clipboard sentinel".to_string()));
    cx.simulate_keystrokes("ctrl-a ctrl-c ctrl-x");
    assert_eq!(clipboard_text(cx), "clipboard sentinel");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .basic_password()
            .to_string()),
        "pässword-🔐"
    );

    right_click(cx, "basic-auth-password-input").unwrap();
    cx.simulate_keystrokes("escape ctrl-z");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .basic_password()
            .to_string()),
        ""
    );
    cx.simulate_keystrokes("ctrl-y");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .basic_password()
            .to_string()),
        "pässword-🔐"
    );
}

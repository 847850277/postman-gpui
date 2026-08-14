//! Clipboard acceptance coverage for every custom text editor used by the application.

#[path = "common/ui.rs"]
mod ui;

use gpui::{AppContext, ClipboardItem, TestAppContext};
use postman_gpui::app::{PostmanApp, WorkspaceViewModel};
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
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    cx.write_to_clipboard(ClipboardItem::new_string(
        "https://clipboard.example/items".to_string(),
    ));
    click(cx, "url-input").unwrap();
    cx.simulate_keystrokes("ctrl-v ctrl-a ctrl-c");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.url().to_string()),
        "https://clipboard.example/items"
    );
    assert_eq!(clipboard_text(cx), "https://clipboard.example/items");
    cx.simulate_keystrokes("ctrl-x");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.url().to_string()),
        ""
    );

    click(cx, "request-pane-authorization").unwrap();
    cx.write_to_clipboard(ClipboardItem::new_string("clipboard-token".to_string()));
    click(cx, "authorization-input").unwrap();
    cx.simulate_keystrokes("cmd-v cmd-a cmd-c");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.bearer_token().to_string()),
        "clipboard-token"
    );
    assert_eq!(clipboard_text(cx), "clipboard-token");
    cx.simulate_keystrokes("cmd-x");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.bearer_token().to_string()),
        ""
    );
    cx.simulate_keystrokes("cmd-v");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.bearer_token().to_string()),
        "clipboard-token"
    );

    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-json").unwrap();
    cx.write_to_clipboard(ClipboardItem::new_string(
        "{\n  \"copied\": true\n}".to_string(),
    ));
    click(cx, "body-input").unwrap();
    cx.simulate_keystrokes("ctrl-v ctrl-a ctrl-c");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.body().to_string()),
        "{\n  \"copied\": true\n}"
    );
    assert_eq!(clipboard_text(cx), "{\n  \"copied\": true\n}");

    click(cx, "body-kind-raw").unwrap();
    click(cx, "body-input").unwrap();
    cx.simulate_keystrokes("ctrl-a ctrl-x");
    cx.write_to_clipboard(ClipboardItem::new_string("raw clipboard body".to_string()));
    cx.simulate_keystrokes("ctrl-v ctrl-a ctrl-c");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.body().to_string()),
        "raw clipboard body"
    );
    assert_eq!(clipboard_text(cx), "raw clipboard body");

    click(cx, "body-kind-url-encoded").unwrap();
    cx.write_to_clipboard(ClipboardItem::new_string("pizza".to_string()));
    click(cx, "body-form-key-0").unwrap();
    cx.simulate_keystrokes("ctrl-v ctrl-a ctrl-c tab");
    assert_eq!(clipboard_text(cx), "pizza");

    cx.write_to_clipboard(ClipboardItem::new_string("margherita".to_string()));
    cx.simulate_keystrokes("ctrl-v ctrl-a ctrl-c enter");
    assert_eq!(clipboard_text(cx), "margherita");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.body().to_string()),
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
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    cx.write_to_clipboard(ClipboardItem::new_string(format!(
        "{}/clipboard",
        server.url()
    )));
    right_click(cx, "url-input").unwrap();
    assert!(cx.debug_bounds("url-edit-menu").is_some());
    click(cx, "url-edit-menu-paste").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.url().to_string()),
        format!("{}/clipboard", server.url())
    );

    click(cx, "request-pane-authorization").unwrap();
    cx.write_to_clipboard(ClipboardItem::new_string("menu-token".to_string()));
    right_click(cx, "authorization-input").unwrap();
    assert!(cx.debug_bounds("header-edit-menu").is_some());
    click(cx, "header-edit-menu-paste").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.bearer_token().to_string()),
        "menu-token"
    );

    click(cx, "send-button").unwrap();
    cx.run_until_parked();
    click(cx, "response-content").unwrap();
    cx.simulate_keystrokes("ctrl-a ctrl-c");
    assert_eq!(clipboard_text(cx), "response copied from the menu");
    cx.write_to_clipboard(ClipboardItem::new_string("menu start".to_string()));
    right_click(cx, "response-content").unwrap();
    assert!(cx.debug_bounds("response-edit-menu").is_some());
    assert!(cx.debug_bounds("response-edit-menu-paste").is_none());
    click(cx, "response-edit-menu-select-all").unwrap();
    cx.write_to_clipboard(ClipboardItem::new_string("menu sentinel".to_string()));
    right_click(cx, "response-content").unwrap();
    click(cx, "response-edit-menu-copy").unwrap();
    assert_eq!(clipboard_text(cx), "response copied from the menu");
    response.assert();
}

#[gpui::test]
fn form_cell_right_click_menu_preserves_single_line_values(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-url-encoded").unwrap();
    cx.write_to_clipboard(ClipboardItem::new_string("menu\nkey".to_string()));
    right_click(cx, "body-form-key-0").unwrap();
    assert!(cx.debug_bounds("body-edit-menu").is_some());
    click(cx, "body-edit-menu-paste").unwrap();
    cx.simulate_keystrokes("tab");

    cx.write_to_clipboard(ClipboardItem::new_string("menu\r\nvalue".to_string()));
    right_click(cx, "body-form-value-0").unwrap();
    click(cx, "body-edit-menu-paste").unwrap();
    cx.simulate_keystrokes("enter");

    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.body().to_string()),
        "menukey=menuvalue"
    );
}

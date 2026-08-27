//! Application-wide keyboard and focus acceptance coverage.

#[path = "common/ui.rs"]
mod ui;

use gpui::{AppContext, ClipboardItem, TestAppContext};
use postman_gpui::{
    app::{
        AuthorizationKind, BodyKind, PostmanApp, RequestBodyDraft, RequestPane, ResponseState,
        WorkspaceViewModel,
    },
    models::{HttpMethod, RedirectPolicy},
};
use ui::click;

#[gpui::test]
fn application_shortcuts_manage_tabs_focus_send_history_and_help(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let response = server
        .mock("GET", "/keyboard")
        .with_status(200)
        .with_body("keyboard-ok")
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    cx.simulate_keystrokes("ctrl-t");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| (
            workspace.tab_count(),
            workspace.active_tab_index().unwrap()
        )),
        (2, 1)
    );

    let url = format!("{}/keyboard", server.url());
    cx.write_to_clipboard(ClipboardItem::new_string(url.clone()));
    cx.simulate_keystrokes("ctrl-l ctrl-a ctrl-v");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .url()
            .to_string()),
        url
    );

    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_tab_index().unwrap()),
        0
    );
    cx.simulate_keystrokes("ctrl-shift-tab");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_tab_index().unwrap()),
        1
    );

    click(cx, "request-tab-0").unwrap();
    cx.simulate_keystrokes("right");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_tab_index().unwrap()),
        1
    );

    cx.simulate_keystrokes("ctrl-enter");
    cx.run_until_parked();
    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.active_request().unwrap().response().clone()),
        ResponseState::Success { status: 200, ref body, .. } if body == "keyboard-ok"
    ));
    click(cx, "response-pane-body").unwrap();
    cx.simulate_keystrokes("right");
    assert!(cx.debug_bounds("response-pane-headers-active").is_some());
    cx.simulate_keystrokes("left");
    assert!(cx.debug_bounds("response-pane-body-active").is_some());

    cx.simulate_keystrokes("ctrl-shift-f");
    cx.simulate_input("no matching history entry");
    assert!(cx.debug_bounds("history-item-0").is_none());
    cx.simulate_keystrokes("ctrl-a backspace");
    assert!(cx.debug_bounds("history-item-0").is_some());

    cx.simulate_keystrokes("ctrl-/");
    assert!(cx.debug_bounds("shortcut-help-dialog").is_some());
    cx.simulate_keystrokes("escape");
    assert!(cx.debug_bounds("shortcut-help-dialog").is_none());

    cx.simulate_keystrokes("ctrl-w");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.tab_count()),
        1
    );
    response.assert();
}

#[gpui::test]
fn option_groups_and_dynamic_rows_are_fully_keyboard_operable(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    click(cx, "method-dropdown-button").unwrap();
    cx.simulate_keystrokes("down escape");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .method()),
        HttpMethod::POST
    );
    assert!(cx.debug_bounds("method-dropdown-menu").is_none());

    click(cx, "request-pane-params").unwrap();
    cx.simulate_keystrokes("right");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .request_pane()),
        RequestPane::Authorization
    );

    click(cx, "auth-kind-bearer").unwrap();
    cx.simulate_keystrokes("right");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .authorization_kind()),
        AuthorizationKind::Basic
    );

    click(cx, "request-pane-options").unwrap();
    click(cx, "redirect-policy-follow").unwrap();
    cx.simulate_keystrokes("right");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .redirect_policy()),
        RedirectPolicy::DoNotFollow
    );
    cx.simulate_keystrokes("left");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .redirect_policy()),
        RedirectPolicy::Follow
    );
    click(cx, "redirect-max-hops-decrease").unwrap();
    cx.simulate_keystrokes("enter");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .max_redirect_hops()),
        8,
        "the focused stepper should activate through Enter"
    );

    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-raw").unwrap();
    cx.simulate_keystrokes("right");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .body_kind()),
        BodyKind::Json
    );

    click(cx, "request-pane-params").unwrap();
    let initial_rows = workspace.read_with(cx, |workspace, _| {
        workspace.active_request().unwrap().params().len()
    });
    click(cx, "add-row-button").unwrap();
    cx.simulate_keystrokes("enter");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .params()
            .len()),
        initial_rows + 2
    );

    // From Add: draft value, draft key, then the final row's Delete control.
    cx.simulate_keystrokes("shift-tab shift-tab shift-tab enter");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .params()
            .len()),
        initial_rows + 1
    );
    // Deletion moves focus to a surviving row toggle, so Space remains a valid next command.
    cx.simulate_keystrokes("space");
    assert!(!workspace.read_with(cx, |workspace, _| workspace
        .active_request()
        .unwrap()
        .params()[0]
        .enabled));

    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-url-encoded").unwrap();
    let initial_form_rows = workspace.read_with(cx, |workspace, _| {
        match workspace.active_request().unwrap().body_draft() {
            RequestBodyDraft::UrlEncoded(rows) => rows.len(),
            _ => 0,
        }
    });
    click(cx, "body-form-add-row").unwrap();
    cx.simulate_keystrokes("enter");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| {
            match workspace.active_request().unwrap().body_draft() {
                RequestBodyDraft::UrlEncoded(rows) => rows.len(),
                _ => 0,
            }
        }),
        initial_form_rows + 2
    );
    cx.simulate_keystrokes("shift-tab enter");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| {
            match workspace.active_request().unwrap().body_draft() {
                RequestBodyDraft::UrlEncoded(rows) => rows.len(),
                _ => 0,
            }
        }),
        initial_form_rows + 1
    );
}

#[gpui::test]
fn cookie_overlay_enters_its_controls_and_escape_restores_the_trigger(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    // Global search is the first header tab stop and Cookie Jar is the next. Opening the overlay
    // moves focus into it; Escape returns focus to the trigger instead of orphaning the handle.
    cx.simulate_keystrokes("tab tab enter");
    assert!(cx.debug_bounds("cookie-jar-panel").is_some());
    cx.simulate_keystrokes("tab escape");
    assert!(cx.debug_bounds("cookie-jar-panel").is_none());
    cx.simulate_keystrokes("enter");
    assert!(cx.debug_bounds("cookie-jar-panel").is_some());
    cx.simulate_keystrokes("escape");
    assert!(cx.debug_bounds("cookie-jar-panel").is_none());
}

#[gpui::test]
fn text_editing_shortcuts_remain_local_and_projection_safe(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    click(cx, "url-input").unwrap();
    cx.simulate_input("https://unicode.example/中文/items");
    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .url()
            .to_string()),
        ""
    );
    cx.simulate_keystrokes("ctrl-y");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .url()
            .to_string()),
        "https://unicode.example/中文/items"
    );

    // A tab switch projects another request and clears editor-local history.
    cx.simulate_keystrokes("ctrl-t ctrl-z");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .url()
            .to_string()),
        ""
    );
}

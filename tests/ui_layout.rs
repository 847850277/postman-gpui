//! Visual-contract checks derived from the issue-linked Pencil artifacts in `design/`.

#[path = "common/ui.rs"]
mod ui;

use gpui::{px, AppContext, Modifiers, TestAppContext};
use postman_gpui::app::{AuthorizationKind, BodyKind, PostmanApp, RequestPane, WorkspaceViewModel};
use postman_gpui::http::executor::RequestResult;
use postman_gpui::models::HttpMethod;
use ui::{click, scroll_down};

#[gpui::test]
fn app_shell_uses_the_pencil_frame_dimensions(cx: &mut TestAppContext) {
    let (_app, cx) = cx.add_window_view(|_window, cx| PostmanApp::new(cx));

    let top_header = cx
        .debug_bounds("top-header")
        .expect("top header should render");
    let left_rail = cx
        .debug_bounds("left-rail")
        .expect("left rail should render");
    let history = cx
        .debug_bounds("history-panel")
        .expect("history panel should render");
    let request_tabs = cx
        .debug_bounds("request-tabs-bar")
        .expect("request tabs should render");
    let request_head = cx
        .debug_bounds("request-head")
        .expect("request head should render");
    let request_panel = cx
        .debug_bounds("request-panel")
        .expect("request panel should render");
    let new_tab = cx
        .debug_bounds("new-tab-button")
        .expect("new tab button should render");
    let send = cx
        .debug_bounds("send-button")
        .expect("send button should render");
    assert!(
        cx.debug_bounds("response-container").is_some(),
        "response panel should render"
    );

    assert_eq!(top_header.size.height, px(72.0));
    assert_eq!(left_rail.size.width, px(72.0));
    assert_eq!(history.size.width, px(320.0));
    assert_eq!(request_tabs.size.height, px(54.0));
    assert_eq!(request_head.size.height, px(46.0));
    assert_eq!(request_panel.size.height, px(360.0));
    assert_eq!(new_tab.size.width, px(32.0));
    assert_eq!(new_tab.size.height, px(32.0));
    assert_eq!(send.size.width, px(110.0));
    assert_eq!(send.size.height, px(46.0));
}

#[gpui::test]
fn method_menu_opens_directly_below_its_button(cx: &mut TestAppContext) {
    let (_app, cx) = cx.add_window_view(|_window, cx| PostmanApp::new(cx));

    let button = cx
        .debug_bounds("method-dropdown-button")
        .expect("method dropdown button should render");
    cx.simulate_click(button.center(), Modifiers::none());

    let menu = cx
        .debug_bounds("method-dropdown-menu")
        .expect("method dropdown menu should open");

    assert_eq!(menu.origin.x, button.origin.x);
    let first_option = cx
        .debug_bounds("method-option-get")
        .expect("first method option should render");
    let last_option = cx
        .debug_bounds("method-option-options")
        .expect("last method option should render");

    assert_eq!(button.size.width, px(120.0));
    assert_eq!(button.size.height, px(46.0));
    assert_eq!(menu.origin.y, button.bottom() + px(6.0));
    assert_eq!(menu.size.width, button.size.width);
    assert_eq!(first_option.size.height, px(36.0));
    assert_eq!(last_option.size.height, px(36.0));
}

#[gpui::test]
fn history_panel_uses_the_issue_51_card_hierarchy(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| {
        let mut workspace = WorkspaceViewModel::new();
        workspace.set_url("https://httpbingo.org/get?existing=1");
        let pending = workspace.begin_send();
        workspace.complete_send(
            pending,
            Ok(RequestResult {
                status: 200,
                headers: Vec::new(),
                body: r#"{"ok":true}"#.to_string(),
                elapsed_ms: 483,
            }),
        );
        workspace
    });
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    let panel = cx
        .debug_bounds("history-panel")
        .expect("history panel should render");
    let header = cx
        .debug_bounds("history-header")
        .expect("history header should render");
    let options = cx
        .debug_bounds("history-options")
        .expect("history options should render");
    let search = cx
        .debug_bounds("history-search-input")
        .expect("history search should render");
    let date = cx
        .debug_bounds("history-date")
        .expect("history date should render");
    let item = cx
        .debug_bounds("history-item-0")
        .expect("history item should render");
    let method = cx
        .debug_bounds("history-method-0")
        .expect("history method pill should render");

    assert_eq!(panel.size.width, px(320.0));
    assert_eq!(header.origin.x, panel.origin.x + px(16.0));
    assert_eq!(header.origin.y, panel.origin.y + px(18.0));
    assert_eq!(options.size.width, px(18.0));
    assert_eq!(options.size.height, px(18.0));
    assert_eq!(search.size.height, px(38.0));
    assert!(date.origin.y >= search.bottom());
    assert_eq!(item.size.height, px(58.0));
    assert_eq!(method.size.width, px(48.0));
    assert_eq!(method.size.height, px(24.0));
}

#[gpui::test]
fn issue_51_query_contract_sections_fit_inside_the_request_panel(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| {
        let mut workspace = WorkspaceViewModel::new();
        workspace.set_url("https://httpbingo.org/get?existing=1");
        workspace.upsert_param("q", "rust gpui");
        workspace.upsert_param("locale", "中文");
        workspace
    });
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    let panel = cx
        .debug_bounds("request-panel")
        .expect("request panel should render");
    let preview = cx
        .debug_bounds("effective-url-preview")
        .expect("effective URL preview should render");
    let ready = cx
        .debug_bounds("params-ready-indicator")
        .expect("ready indicator should render");

    assert!(cx.debug_bounds("url-query-count").is_some());
    assert!(cx.debug_bounds("params-enabled-count").is_some());
    assert!(cx.debug_bounds("param-row-toggle-0").is_some());
    assert!(cx.debug_bounds("param-row-toggle-1").is_some());
    assert!(cx.debug_bounds("param-row-toggle-2").is_some());
    assert!(preview.origin.y >= panel.origin.y);
    assert!(preview.bottom() <= ready.origin.y);
    assert!(ready.bottom() <= panel.bottom());
}

#[gpui::test]
fn issue_53_bearer_contract_sections_fit_inside_the_request_panel(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| {
        let mut workspace = WorkspaceViewModel::new();
        workspace.set_url("https://httpbingo.org/bearer");
        workspace.set_request_pane(RequestPane::Authorization);
        workspace.set_bearer_token("Bearer scenario-token");
        workspace
    });
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    let panel = cx
        .debug_bounds("request-panel")
        .expect("request panel should render");
    let summary = cx
        .debug_bounds("authorization-summary")
        .expect("Authorization summary should render");
    let kind = cx
        .debug_bounds("authorization-kind-selector")
        .expect("Authorization type selector should render");
    let input = cx
        .debug_bounds("authorization-input")
        .expect("Bearer input should render");
    let normalized = cx
        .debug_bounds("authorization-normalized-token")
        .expect("normalized token should render");
    let outgoing = cx
        .debug_bounds("authorization-header-preview")
        .expect("outgoing header should render");
    let ready = cx
        .debug_bounds("authorization-ready-indicator")
        .expect("Authorization ready state should render");

    assert_eq!(panel.size.height, px(360.0));
    assert!(summary.origin.y >= panel.origin.y);
    assert!(summary.bottom() <= kind.origin.y);
    assert!(kind.bottom() <= input.origin.y);
    assert!(normalized.origin.x < outgoing.origin.x);
    assert!(input.size.width > px(0.0));
    assert!(outgoing.size.width > px(0.0));
    assert!(outgoing.bottom() <= ready.origin.y);
    assert!(ready.bottom() <= panel.bottom());
}

#[gpui::test]
fn issue_54_basic_auth_contract_sections_fit_inside_the_request_panel(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| {
        let mut workspace = WorkspaceViewModel::new();
        workspace.set_url("https://httpbingo.org/basic-auth/scenario-user/scenario-pass");
        workspace.set_request_pane(RequestPane::Authorization);
        workspace.set_authorization_kind(AuthorizationKind::Basic);
        workspace.set_basic_username("scenario-user");
        workspace.set_basic_password("scenario-pass");
        workspace
    });
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    let panel = cx
        .debug_bounds("request-panel")
        .expect("request panel should render");
    let summary = cx
        .debug_bounds("authorization-summary")
        .expect("Authorization summary should render");
    let kind = cx
        .debug_bounds("authorization-kind-selector")
        .expect("Authorization type selector should render");
    let username = cx
        .debug_bounds("basic-auth-username-field")
        .expect("Basic username should render");
    let password = cx
        .debug_bounds("basic-auth-password-field")
        .expect("masked Basic password should render");
    let outgoing = cx
        .debug_bounds("basic-auth-header-preview")
        .expect("Basic outgoing header should render");
    let projection = cx
        .debug_bounds("basic-auth-projection-note")
        .expect("Basic ViewModel projection note should render");
    let ready = cx
        .debug_bounds("authorization-ready-indicator")
        .expect("Authorization ready state should render");

    assert_eq!(panel.size.height, px(360.0));
    assert!(summary.origin.y >= panel.origin.y);
    assert!(summary.bottom() <= kind.origin.y);
    assert!(kind.bottom() <= username.origin.y);
    assert_eq!(username.origin.y, password.origin.y);
    assert!(username.origin.x < password.origin.x);
    assert!(username.size.width > px(0.0));
    assert!(password.size.width > px(0.0));
    assert!(username.bottom() <= outgoing.origin.y);
    assert!(outgoing.bottom() <= projection.origin.y);
    assert!(
        projection.bottom() <= ready.origin.y,
        "Basic projection {projection:?} overlaps ready state {ready:?}"
    );
    assert!(ready.bottom() <= panel.bottom());
}

#[gpui::test]
fn issue_57_json_body_contract_projects_the_active_value_and_effective_headers(
    cx: &mut TestAppContext,
) {
    let workspace = cx.new(|_| {
        let mut workspace = WorkspaceViewModel::new();
        workspace.upsert_header("X-Scenario", "httpbingo-json");
        workspace.set_method(HttpMethod::POST);
        workspace.set_url("https://httpbingo.org/anything/post-json");
        workspace.set_body_kind(BodyKind::Json);
        workspace.set_body(r#"{"name":"Ada","active":true}"#);
        workspace.set_request_pane(RequestPane::Body);
        workspace
    });
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    let panel = cx
        .debug_bounds("request-panel")
        .expect("Body panel should render");
    let kinds = cx
        .debug_bounds("body-kind-selector")
        .expect("Body type selector should render");
    let editor = cx
        .debug_bounds("body-editor-shell")
        .expect("JSON editor shell should render");
    let source = cx
        .debug_bounds("body-source-of-truth")
        .expect("single-source projection should render");
    let headers = cx
        .debug_bounds("body-effective-headers")
        .expect("effective headers should render");

    for selector in [
        "body-kind-json",
        "body-live-saved",
        "body-input",
        "body-effective-header-content-type",
        "body-effective-header-accept",
        "body-effective-header-x-scenario",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "Issue #57 contract element `{selector}` should render"
        );
    }
    assert_eq!(panel.size.height, px(360.0));
    assert_eq!(kinds.size.height, px(44.0));
    assert!(kinds.origin.y >= panel.origin.y);
    assert!(editor.origin.y >= kinds.bottom());
    assert!(editor.origin.x < headers.origin.x);
    assert!(editor.bottom() <= source.origin.y);
    assert!(source.bottom() <= panel.bottom());
    assert!(headers.bottom() <= panel.bottom());
}

#[gpui::test]
fn issue_58_url_encoded_contract_fits_the_editor_and_effective_preview(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| {
        let mut workspace = WorkspaceViewModel::new();
        workspace.set_method(HttpMethod::POST);
        workspace.set_url("https://httpbingo.org/anything/form");
        workspace.set_body_kind(BodyKind::UrlEncoded);
        workspace.set_body("name=Ada+Lovelace&active=true");
        workspace.set_request_pane(RequestPane::Body);
        workspace
    });
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    let panel = cx
        .debug_bounds("request-panel")
        .expect("Body panel should render");
    let kinds = cx
        .debug_bounds("body-kind-selector")
        .expect("Body type selector should render");
    let editor = cx
        .debug_bounds("body-url-encoded-editor")
        .expect("URL-encoded editor should render");
    let table = cx
        .debug_bounds("body-form-table-header")
        .expect("URL-encoded table header should render");
    let first_row = cx
        .debug_bounds("body-form-row-0")
        .expect("first URL-encoded row should render");
    let second_row = cx
        .debug_bounds("body-form-row-1")
        .expect("second URL-encoded row should render");
    let effective = cx
        .debug_bounds("body-url-encoded-effective-request")
        .expect("effective request preview should render");
    let ready = cx
        .debug_bounds("body-url-encoded-ready-indicator")
        .expect("ready indicator should render");

    for selector in [
        "body-kind-url-encoded",
        "body-url-encoded-live-saved",
        "body-form-toggle-0",
        "body-form-key-0",
        "body-form-value-0",
        "body-form-delete-0",
        "body-form-add-row",
        "body-url-encoded-effective-body",
        "body-effective-header-content-type",
        "body-effective-header-accept",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "Issue #58 contract element `{selector}` should render"
        );
    }

    let content_type = cx
        .debug_bounds("body-effective-header-content-type")
        .expect("Content-Type preview should render");
    let accept = cx
        .debug_bounds("body-effective-header-accept")
        .expect("Accept preview should render");

    assert_eq!(panel.size.height, px(360.0));
    assert_eq!(kinds.size.height, px(44.0));
    assert!(editor.origin.y >= kinds.bottom());
    assert!(table.origin.y >= editor.origin.y);
    assert!(first_row.origin.y >= table.bottom());
    assert!(second_row.origin.y >= first_row.bottom());
    assert!(second_row.bottom() <= effective.origin.y);
    assert!(effective.bottom() <= ready.origin.y);
    assert!(ready.bottom() <= panel.bottom());
    assert!(content_type.origin.x >= effective.origin.x);
    assert!(accept.right() <= effective.right());
}

#[gpui::test]
fn issue_95_urlencoded_rows_grow_then_scroll_below_fixed_actions(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| {
        let mut workspace = WorkspaceViewModel::new();
        workspace.set_method(HttpMethod::POST);
        workspace.set_body_kind(BodyKind::UrlEncoded);
        workspace.set_body("");
        workspace.set_request_pane(RequestPane::Body);
        workspace
    });
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    let initial_panel = cx
        .debug_bounds("request-panel")
        .expect("URL-encoded request panel should render");
    let initial_rows = cx
        .debug_bounds("body-form-scroll")
        .expect("URL-encoded row viewport should render");
    assert_eq!(initial_panel.size.height, px(360.0));
    assert!(cx.debug_bounds("body-form-scrollbar").is_none());

    for _ in 0..4 {
        click(cx, "body-form-add-row").unwrap();
    }
    cx.run_until_parked();

    let grown_panel = cx
        .debug_bounds("request-panel")
        .expect("URL-encoded request panel should grow with rows");
    let grown_rows = cx
        .debug_bounds("body-form-scroll")
        .expect("URL-encoded row viewport should grow with rows");
    assert!(grown_panel.size.height > initial_panel.size.height);
    assert_eq!(
        grown_rows.size.height - initial_rows.size.height,
        grown_panel.size.height - initial_panel.size.height
    );
    assert!(cx.debug_bounds("body-form-scrollbar").is_none());

    for _ in 0..4 {
        click(cx, "body-form-add-row").unwrap();
    }
    cx.run_until_parked();

    let capped_panel = cx
        .debug_bounds("request-panel")
        .expect("URL-encoded request panel should remain visible");
    let response = cx
        .debug_bounds("response-container")
        .expect("response panel should retain space");
    let rows_viewport = cx
        .debug_bounds("body-form-scroll")
        .expect("overflowing URL-encoded rows should remain scrollable");
    let scrollbar = cx
        .debug_bounds("body-form-scrollbar")
        .expect("overflowing URL-encoded rows should expose a scrollbar");
    let thumb = cx
        .debug_bounds("body-form-scrollbar-thumb")
        .expect("the URL-encoded scrollbar should expose its thumb");
    let add_action = cx
        .debug_bounds("body-form-add-row")
        .expect("Add form field should remain outside the row viewport");
    let effective = cx
        .debug_bounds("body-url-encoded-effective-request")
        .expect("effective request preview should remain fixed");
    let ready = cx
        .debug_bounds("body-url-encoded-ready-indicator")
        .expect("ready state should remain fixed");

    assert_eq!(capped_panel.size.height, px(544.0));
    assert!(response.size.height > px(0.0));
    assert!(thumb.origin.y >= scrollbar.origin.y);
    assert!(thumb.bottom() <= scrollbar.bottom());
    assert!(thumb.size.height < scrollbar.size.height);
    assert!(add_action.origin.y >= rows_viewport.bottom());
    assert!(effective.origin.y >= add_action.bottom());
    assert!(ready.origin.y >= effective.bottom());
    assert!(cx.debug_bounds("body-form-add-row-hint").is_some());
    assert!(cx.debug_bounds("body-url-encoded-row-count").is_some());

    scroll_down(cx, "body-form-scroll", 90.0).unwrap();
    let add_after_scroll = cx
        .debug_bounds("body-form-add-row")
        .expect("Add form field should remain visible after scrolling");
    let effective_after_scroll = cx
        .debug_bounds("body-url-encoded-effective-request")
        .expect("effective preview should remain visible after scrolling");
    let ready_after_scroll = cx
        .debug_bounds("body-url-encoded-ready-indicator")
        .expect("ready state should remain visible after scrolling");
    assert_eq!(add_after_scroll.origin.y, add_action.origin.y);
    assert_eq!(effective_after_scroll.origin.y, effective.origin.y);
    assert_eq!(ready_after_scroll.origin.y, ready.origin.y);
}

#[gpui::test]
fn params_panel_grows_with_rows_then_preserves_response_space(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    let initial_panel = cx
        .debug_bounds("request-panel")
        .expect("request panel should render");
    let initial_rows = cx
        .debug_bounds("params-rows-scroll")
        .expect("Params rows should render");
    assert_eq!(initial_panel.size.height, px(360.0));
    assert!(cx.debug_bounds("params-scrollbar").is_none());

    workspace.update(cx, |workspace, cx| {
        for _ in 0..3 {
            workspace.append_param_row();
        }
        cx.notify();
    });
    cx.run_until_parked();

    let grown_panel = cx
        .debug_bounds("request-panel")
        .expect("request panel should grow");
    let grown_rows = cx
        .debug_bounds("params-rows-scroll")
        .expect("Params rows should grow");
    assert!(grown_panel.size.height > initial_panel.size.height);
    assert_eq!(
        grown_rows.size.height - initial_rows.size.height,
        grown_panel.size.height - initial_panel.size.height
    );

    workspace.update(cx, |workspace, cx| {
        for _ in 0..20 {
            workspace.append_param_row();
        }
        cx.notify();
    });
    cx.run_until_parked();

    let capped_panel = cx
        .debug_bounds("request-panel")
        .expect("request panel should remain visible");
    let response = cx
        .debug_bounds("response-container")
        .expect("response panel should retain space");
    let scrollbar = cx
        .debug_bounds("params-scrollbar")
        .expect("overflowing Params rows should expose a scrollbar");
    let thumb = cx
        .debug_bounds("params-scrollbar-thumb")
        .expect("the Params scrollbar should expose its thumb");
    assert!(capped_panel.size.height >= grown_panel.size.height);
    assert!(capped_panel.size.height <= px(544.0));
    assert!(response.size.height > px(0.0));
    assert!(thumb.origin.y >= scrollbar.origin.y);
    assert!(thumb.bottom() <= scrollbar.bottom());
    assert!(thumb.size.height < scrollbar.size.height);

    workspace.update(cx, |workspace, cx| {
        workspace.append_param_row();
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(
        cx.debug_bounds("request-panel")
            .expect("request panel should stay capped")
            .size
            .height,
        capped_panel.size.height
    );
}

#[gpui::test]
fn headers_panel_grows_with_rows_then_exposes_a_fixed_scroll_region(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| {
        let mut workspace = WorkspaceViewModel::new();
        workspace.set_request_pane(RequestPane::Headers);
        workspace
    });
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    let initial_panel = cx
        .debug_bounds("request-panel")
        .expect("Headers panel should render");
    let initial_rows = cx
        .debug_bounds("headers-rows-scroll")
        .expect("Headers rows should render");
    assert_eq!(initial_panel.size.height, px(360.0));
    assert!(cx.debug_bounds("headers-scrollbar").is_none());

    workspace.update(cx, |workspace, cx| {
        for _ in 0..3 {
            workspace.append_header_row();
        }
        cx.notify();
    });
    cx.run_until_parked();

    let grown_panel = cx
        .debug_bounds("request-panel")
        .expect("Headers panel should grow");
    let grown_rows = cx
        .debug_bounds("headers-rows-scroll")
        .expect("Headers rows should grow");
    assert!(grown_panel.size.height > initial_panel.size.height);
    assert_eq!(
        grown_rows.size.height - initial_rows.size.height,
        grown_panel.size.height - initial_panel.size.height
    );

    workspace.update(cx, |workspace, cx| {
        for _ in 0..20 {
            workspace.append_header_row();
        }
        cx.notify();
    });
    cx.run_until_parked();

    let capped_panel = cx
        .debug_bounds("request-panel")
        .expect("Headers panel should remain visible");
    let response = cx
        .debug_bounds("response-container")
        .expect("response panel should retain space");
    let scrollbar = cx
        .debug_bounds("headers-scrollbar")
        .expect("overflowing Header rows should expose a scrollbar");
    let thumb = cx
        .debug_bounds("headers-scrollbar-thumb")
        .expect("the Headers scrollbar should expose its thumb");
    let add_action = cx
        .debug_bounds("add-row-button")
        .expect("Add Header should remain outside the scroll region");
    let rows_viewport = cx
        .debug_bounds("headers-rows-scroll")
        .expect("Headers rows should remain scrollable");

    assert!(capped_panel.size.height >= grown_panel.size.height);
    assert!(capped_panel.size.height <= px(452.0));
    assert!(response.size.height > px(0.0));
    assert!(thumb.origin.y >= scrollbar.origin.y);
    assert!(thumb.bottom() <= scrollbar.bottom());
    assert!(thumb.size.height < scrollbar.size.height);
    assert!(add_action.origin.y >= rows_viewport.bottom());
}

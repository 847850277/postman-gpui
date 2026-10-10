//! Visual-contract and layout checks for the application UI.

#[path = "common/ui.rs"]
mod ui;

use gpui::{point, px, AppContext, Modifiers, MouseButton, TestAppContext};
use postman_gpui::app::{AuthorizationKind, BodyKind, PostmanApp, RequestPane, WorkspaceViewModel};
use postman_gpui::models::{HistoryEntry, HttpMethod, Request};
use postman_gpui::persistence::{
    HistoryRepository, SqliteHistoryRepository, VersionedHistorySnapshot,
    DEFAULT_HISTORY_RETENTION_LIMIT,
};
use ui::{click, scroll_down, scroll_up};

/// The wrapper follows the viewport; Kit owns the painted thumb and its hit testing.
fn assert_scrollbar_at_viewport(
    cx: &mut gpui::VisualTestContext,
    track: &'static str,
    scroll: &'static str,
) -> gpui::Bounds<gpui::Pixels> {
    let bar = cx
        .debug_bounds(track)
        .expect("overflow should expose a scrollbar");
    let viewport = cx.debug_bounds(scroll).unwrap();
    assert_eq!(bar.top(), viewport.top(), "{track}");
    assert_eq!(bar.bottom(), viewport.bottom(), "{track}");
    assert_eq!(bar.right(), viewport.right(), "{track}");
    bar
}

fn assert_scrollbar_pointer_moves_rows(
    cx: &mut gpui::VisualTestContext,
    track: &'static str,
    scroll: &'static str,
    first_row: &'static str,
) {
    scroll_up(cx, scroll, 10_000.).unwrap();
    let bar = assert_scrollbar_at_viewport(cx, track, scroll);
    let first = cx.debug_bounds(first_row).unwrap();
    // At the top, this point lies inside Kit's actual thumb, regardless of content ratio.
    let start = point(bar.center().x, bar.top() + px(8.));
    let end = point(start.x, bar.center().y);
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        point(start.x, start.y + px(6.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    assert!(
        cx.debug_bounds(first_row).unwrap().top() < first.top(),
        "{track}: dragging the thumb must move content"
    );
    assert_eq!(
        assert_scrollbar_at_viewport(cx, track, scroll),
        bar,
        "the overlay must not move with its content"
    );

    scroll_up(cx, scroll, 10_000.).unwrap();
    assert_eq!(cx.debug_bounds(first_row).unwrap().top(), first.top());
    // Click beyond the thumb, near the track end, to reach the final rows.
    let end = point(bar.center().x, bar.bottom() - px(2.));
    cx.simulate_mouse_down(end, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    assert!(
        cx.debug_bounds(first_row).unwrap().top() < first.top(),
        "{track}: clicking the track must move content"
    );
    assert_eq!(assert_scrollbar_at_viewport(cx, track, scroll), bar);
}

fn copy_visible_json_line(cx: &mut gpui::VisualTestContext) -> u64 {
    click(cx, "body-text-scroll").unwrap();
    cx.simulate_keystrokes("home shift-down cmd-c");
    let copied = cx
        .read_from_clipboard()
        .and_then(|item| item.text())
        .unwrap();
    let line: serde_json::Value = serde_json::from_str(copied.trim()).unwrap();
    line["line"].as_u64().unwrap()
}

#[gpui::test]
fn app_shell_uses_expected_frame_dimensions(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);
    let handle = cx.update(|window, _| window.window_handle());
    cx.simulate_window_resize(handle, gpui::size(px(1480.), px(980.)));

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

    assert_eq!(top_header.size.height, px(52.0));
    assert_eq!(left_rail.size.width, px(72.0));
    assert_eq!(history.size.width, px(260.0));
    assert_eq!(request_tabs.size.height, px(39.0));
    assert!(
        (request_head.size.height.as_f32() - 172.625).abs() <= 0.5,
        "{request_head:?}"
    );
    assert!(request_panel.size.height > px(180.));
    assert_eq!(new_tab.size.width, px(32.0));
    assert_eq!(new_tab.size.height, px(32.0));
    assert_eq!(send.size.width, px(128.0));
    assert_eq!(send.size.height, px(48.0));
}

#[gpui::test]
fn history_panel_can_be_dragged_wider_and_narrower(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);

    let resize_handle = cx
        .debug_bounds("history-resize-handle")
        .expect("History resize handle should render");
    let start = resize_handle.center();

    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        point(start.x + px(4.0), start.y),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(
        point(start.x + px(120.0), start.y),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_up(
        point(start.x + px(120.0), start.y),
        MouseButton::Left,
        Modifiers::none(),
    );

    let widened = cx
        .debug_bounds("history-panel")
        .expect("History panel should remain rendered after widening");
    assert_eq!(widened.size.width, px(380.0));

    let resize_handle = cx
        .debug_bounds("history-resize-handle")
        .expect("History resize handle should move with the panel");
    let start = resize_handle.center();
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        point(start.x - px(4.0), start.y),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(
        point(start.x - px(400.0), start.y),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_up(
        point(start.x - px(400.0), start.y),
        MouseButton::Left,
        Modifiers::none(),
    );

    let narrowed = cx
        .debug_bounds("history-panel")
        .expect("History panel should remain rendered after narrowing");
    assert_eq!(narrowed.size.width, px(240.0));
}

#[gpui::test]
fn response_panel_can_be_dragged_taller_and_shorter(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);
    click(cx, "response-layout-toggle").unwrap();

    let initial_request = cx
        .debug_bounds("request-panel")
        .expect("request panel should render");
    let initial_response = cx
        .debug_bounds("response-container")
        .expect("response panel should render");
    let resize_handle = cx
        .debug_bounds("response-resize-handle")
        .expect("Response resize handle should render");
    let start = resize_handle.center();

    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        point(start.x, start.y - px(6.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(
        point(start.x, start.y - px(60.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_up(
        point(start.x, start.y - px(60.0)),
        MouseButton::Left,
        Modifiers::none(),
    );

    let expanded_request = cx
        .debug_bounds("request-panel")
        .expect("request panel should remain rendered after expanding Response");
    let expanded_response = cx
        .debug_bounds("response-container")
        .expect("response panel should remain rendered after expanding");
    assert!(
        (expanded_request.size.height - (initial_request.size.height - px(60.))).abs() <= px(1.)
    );
    assert!(
        (expanded_response.size.height - initial_response.size.height - px(60.)).abs() <= px(1.)
    );

    let resize_handle = cx
        .debug_bounds("response-resize-handle")
        .expect("Response resize handle should move with the panel");
    let start = resize_handle.center();
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        point(start.x, start.y + px(6.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(
        point(start.x, start.y + px(100.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_up(
        point(start.x, start.y + px(100.0)),
        MouseButton::Left,
        Modifiers::none(),
    );

    let shortened_response = cx
        .debug_bounds("response-container")
        .expect("response panel should remain rendered after shrinking");
    assert!(
        (cx.debug_bounds("request-panel").unwrap().size.height
            - expanded_request.size.height
            - px(100.))
        .abs()
            <= px(1.)
    );
    assert!(
        (shortened_response.size.height - expanded_response.size.height + px(100.)).abs() <= px(1.)
    );
}

#[gpui::test]
fn kit_method_select_matches_composer_geometry_and_escape_restores_focus(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);

    use gpui_kit::test::TestWindowExt;
    let button = cx.update(|window, cx| {
        window.render_frame(cx);
        window.find("method-select").bounds()
    });
    cx.update(|window, cx| window.click("method-select", cx));
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(button.size.width, px(110.));
        assert_eq!(button.size.height, px(46.));
        assert_eq!(window.find("method-select").expanded(), Some(true));
        window.press("down", cx);
        window.press("escape", cx);
        window.render_frame(cx);
        assert_eq!(window.find("method-select").expanded(), Some(false));
        assert_eq!(window.find("method-select").value(), Some("GET"));
        assert_eq!(window.find("method-select").focused(), Some(true));
    });
}

#[gpui::test]
fn history_panel_uses_the_issue_51_card_hierarchy(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("history.sqlite3");
    let entry = HistoryEntry::completed(
        Request::new(HttpMethod::GET, "https://httpbingo.org/get?existing=1"),
        "https://httpbingo.org/get?existing=1".to_string(),
        200,
        483,
        r#"{"ok":true}"#.len(),
    );
    let snapshot = VersionedHistorySnapshot::try_from(&entry).unwrap();
    let mut repository = SqliteHistoryRepository::new(&database_path).unwrap();
    repository.initialize().unwrap();
    repository
        .append_and_trim(&snapshot, DEFAULT_HISTORY_RETENTION_LIMIT)
        .unwrap();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model_and_history_path(observed, database_path, window, cx)
        })
    });
    ui::open_http(cx);
    cx.run_until_parked();

    let panel = cx
        .debug_bounds("history-panel")
        .expect("history panel should render");
    let header = cx
        .debug_bounds("history-header")
        .expect("history header should render");
    let actions = cx
        .debug_bounds("history-actions")
        .expect("history actions should render");
    let refresh = cx
        .debug_bounds("history-refresh-button")
        .expect("history refresh should render");
    let clear = cx
        .debug_bounds("history-clear-button")
        .expect("history clear should render");
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

    assert_eq!(panel.size.width, px(260.0));
    assert_eq!(header.origin.x, panel.origin.x + px(16.0));
    assert_eq!(header.origin.y, panel.origin.y + px(18.0));
    assert!(actions.size.width > px(18.0));
    assert_eq!(actions.size.height, px(24.0));
    assert_eq!(refresh.size.height, px(24.0));
    assert_eq!(clear.size.height, px(24.0));
    assert!(cx.debug_bounds("history-storage-ready").is_some());
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
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://httpbingo.org/get?existing=1");
        workspace
            .active_request_mut()
            .unwrap()
            .upsert_param("q", "rust gpui");
        workspace
            .active_request_mut()
            .unwrap()
            .upsert_param("locale", "中文");
        workspace
    });
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);

    let panel = cx
        .debug_bounds("request-panel")
        .expect("request panel should render");
    let preview = cx
        .debug_bounds("effective-url-preview")
        .expect("effective URL preview should render");
    assert!(cx.debug_bounds("params-enabled-count").is_some());
    for selector in [
        "param-row-toggle-0",
        "param-row-toggle-1",
        "param-row-toggle-2",
    ] {
        assert!(cx.debug_bounds(selector).is_some());
    }
    assert!(preview.origin.y >= panel.origin.y);
    assert!(preview.bottom() <= panel.bottom());
}

#[gpui::test]
fn issue_53_bearer_contract_sections_fit_inside_the_request_panel(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://httpbingo.org/bearer");
        workspace
            .active_request_mut()
            .unwrap()
            .set_request_pane(RequestPane::Authorization);
        workspace
            .active_request_mut()
            .unwrap()
            .set_bearer_token("Bearer scenario-token");
        workspace
    });
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);

    let panel = cx
        .debug_bounds("request-panel")
        .expect("request panel should render");
    let kind = cx.debug_bounds("authorization-kind-selector").unwrap();
    let status = cx.debug_bounds("authorization-status").unwrap();
    assert!(kind.origin.y >= panel.origin.y);
    let input = cx.debug_bounds("authorization-input").unwrap();
    assert!(kind.right() < input.left());
    assert_eq!(kind.bottom(), input.bottom());
    assert!(input.size.width > px(0.));
    assert!(input.bottom() <= status.origin.y);
    assert!(status.bottom() <= panel.bottom());
}

#[gpui::test]
fn issue_54_basic_auth_contract_sections_fit_inside_the_request_panel(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://httpbingo.org/basic-auth/scenario-user/scenario-pass");
        workspace
            .active_request_mut()
            .unwrap()
            .set_request_pane(RequestPane::Authorization);
        workspace
            .active_request_mut()
            .unwrap()
            .set_authorization_kind(AuthorizationKind::Basic);
        workspace
            .active_request_mut()
            .unwrap()
            .set_basic_username("scenario-user");
        workspace
            .active_request_mut()
            .unwrap()
            .set_basic_password("scenario-pass");
        workspace
    });
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);

    let panel = cx
        .debug_bounds("request-panel")
        .expect("request panel should render");
    let kind = cx.debug_bounds("authorization-kind-selector").unwrap();
    let status = cx.debug_bounds("authorization-status").unwrap();
    assert!(kind.origin.y >= panel.origin.y);
    let username = cx.debug_bounds("basic-auth-username-input").unwrap();
    let password = cx.debug_bounds("basic-auth-password-input").unwrap();
    assert!(kind.right() < username.left());
    assert_eq!(kind.bottom(), username.bottom());
    assert!(username.bottom() < password.top());
    assert_eq!(username.left(), password.left());
    assert!(username.size.width > px(0.));
    assert!(password.size.width > px(0.));
    assert!(password.bottom() <= status.origin.y);
    assert!(
        cx.debug_bounds("basic-auth-password-masked")
            .unwrap()
            .bottom()
            <= panel.bottom()
    );
    assert!(status.bottom() <= panel.bottom());
}

#[gpui::test]
fn issue_57_json_body_contract_projects_the_active_value_and_effective_headers(
    cx: &mut TestAppContext,
) {
    let workspace = cx.new(|_| {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .upsert_header("X-Scenario", "httpbingo-json");
        workspace
            .active_request_mut()
            .unwrap()
            .set_method(HttpMethod::POST);
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://httpbingo.org/anything/post-json");
        workspace
            .active_request_mut()
            .unwrap()
            .set_body_kind(BodyKind::Json);
        workspace
            .active_request_mut()
            .unwrap()
            .set_body(r#"{"name":"Ada","active":true}"#);
        workspace
            .active_request_mut()
            .unwrap()
            .set_request_pane(RequestPane::Body);
        workspace
    });
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);
    ui::show_body_details(cx).unwrap();

    let panel = cx
        .debug_bounds("request-panel")
        .expect("Body panel should render");
    let kinds = cx
        .debug_bounds("body-kind-selector")
        .expect("Body type selector should render");
    let editor = cx
        .debug_bounds("body-editor-shell")
        .expect("JSON editor shell should render");
    let headers = cx
        .debug_bounds("body-effective-headers")
        .expect("effective headers should render");

    for selector in [
        "body-kind-json",
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
    assert!(panel.size.height > px(180.));
    assert_eq!(kinds.size.height, px(55.0));
    assert!(kinds.origin.y >= panel.origin.y);
    assert!(editor.origin.y >= kinds.bottom());
    assert!(editor.bottom() <= headers.top());
    assert!(editor.bottom() <= panel.bottom());
    assert!(headers.bottom() <= panel.bottom());
    let rows = cx.debug_bounds("body-effective-headers-scroll").unwrap();
    for selector in [
        "body-effective-header-content-type",
        "body-effective-header-accept",
        "body-effective-header-x-scenario",
    ] {
        let row = cx.debug_bounds(selector).unwrap();
        assert!(row.top() >= rows.top() && row.bottom() <= rows.bottom());
    }
    assert!(cx
        .debug_bounds("body-effective-headers-scrollbar")
        .is_none());
    assert!(cx.debug_bounds("body-text-scrollbar").is_none());
}

#[gpui::test]
fn json_body_and_effective_headers_expose_visible_scrollbars_when_content_overflows(
    cx: &mut TestAppContext,
) {
    let long_body = (0..80)
        .map(|line| format!(r#"{{"line":{line}}}"#))
        .collect::<Vec<_>>()
        .join("\n");
    let workspace = cx.new(move |_| {
        let mut workspace = WorkspaceViewModel::new();
        let request = workspace.active_request_mut().unwrap();
        request.set_method(HttpMethod::POST);
        request.set_body_kind(BodyKind::Json);
        request.set_body(long_body);
        for index in 0..8 {
            request.upsert_header(format!("X-Overflow-{index}"), format!("value-{index}"));
        }
        request.set_request_pane(RequestPane::Body);
        workspace
    });
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);
    click(cx, "response-layout-toggle").unwrap();
    cx.run_until_parked();

    scroll_up(cx, "body-text-scroll", 10_000.0).unwrap();
    let line_before = copy_visible_json_line(cx);
    let text_scrollbar =
        assert_scrollbar_at_viewport(cx, "body-text-scrollbar", "body-text-scroll");
    scroll_down(cx, "body-text-scroll", 90.0).unwrap();
    let line_after = copy_visible_json_line(cx);
    assert!(
        line_after > line_before,
        "wheel scrolling must reveal later JSON lines"
    );
    // The prototype's ancestor can scroll too; the overlay must stay pinned
    // to this editor's viewport, rather than follow its text content.
    assert_eq!(
        assert_scrollbar_at_viewport(cx, "body-text-scrollbar", "body-text-scroll").size,
        text_scrollbar.size
    );

    ui::show_body_details(cx).unwrap();
    let headers_scrollbar = assert_scrollbar_at_viewport(
        cx,
        "body-effective-headers-scrollbar",
        "body-effective-headers-scroll",
    );
    let first_header = cx
        .debug_bounds("body-effective-header-content-type")
        .unwrap();
    scroll_down(cx, "body-effective-headers-scroll", 90.0).unwrap();
    assert!(
        cx.debug_bounds("body-effective-header-content-type")
            .unwrap()
            .top()
            < first_header.top()
    );
    assert_eq!(
        assert_scrollbar_at_viewport(
            cx,
            "body-effective-headers-scrollbar",
            "body-effective-headers-scroll"
        ),
        headers_scrollbar
    );
    assert_scrollbar_pointer_moves_rows(
        cx,
        "body-effective-headers-scrollbar",
        "body-effective-headers-scroll",
        "body-effective-header-content-type",
    );
}

#[gpui::test]
fn issue_60_raw_body_contract_fits_editor_and_exact_request_semantics(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_method(HttpMethod::PUT);
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://httpbingo.org/anything/raw");
        workspace
            .active_request_mut()
            .unwrap()
            .set_body_kind(BodyKind::Raw);
        workspace
            .active_request_mut()
            .unwrap()
            .set_body("plain text body");
        workspace
            .active_request_mut()
            .unwrap()
            .set_request_pane(RequestPane::Body);
        workspace
    });
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);
    ui::show_body_details(cx).unwrap();

    let panel = cx
        .debug_bounds("request-panel")
        .expect("Body panel should render");
    let kinds = cx
        .debug_bounds("body-kind-selector")
        .expect("Body type selector should render");
    let editor = cx
        .debug_bounds("body-editor-shell")
        .expect("Raw editor shell should render");
    let semantics = cx
        .debug_bounds("body-raw-effective-request")
        .expect("Raw request semantics should render");
    let content_type = cx
        .debug_bounds("body-raw-content-type-state")
        .expect("Content-Type policy should render");
    let exact_body = cx
        .debug_bounds("body-raw-exact-bytes")
        .expect("exact raw body should render");
    let ready = cx
        .debug_bounds("body-raw-ready-indicator")
        .expect("Raw ready state should render");

    for selector in [
        "body-kind-raw",
        "body-input",
        "body-raw-generated-header-count",
        "body-raw-effective-body",
        "body-raw-request-target",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "Issue #60 contract element `{selector}` should render"
        );
    }
    assert!(cx.debug_bounds("body-sample-json").is_none());
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .effective_headers()),
        vec![postman_gpui::app::EffectiveHeader {
            name: "Content-Type".to_string(),
            value: "text/plain".to_string(),
            source: postman_gpui::app::EffectiveHeaderSource::Generated,
        }]
    );

    assert!(panel.size.height > px(180.));
    assert_eq!(kinds.size.height, px(55.0));
    assert!(editor.origin.y >= kinds.bottom());
    assert!(editor.bottom() <= semantics.top());
    assert!(editor.bottom() <= panel.bottom());
    assert!(content_type.origin.y >= semantics.origin.y);
    assert!(content_type.bottom() <= exact_body.origin.y);
    assert!(exact_body.bottom() <= ready.origin.y);
    assert!(
        ready.bottom() <= semantics.bottom(),
        "Raw ready state {ready:?} overflows semantics panel {semantics:?}"
    );
    assert!(semantics.bottom() <= panel.bottom());
    assert!(cx.debug_bounds("body-raw-scrollbar").is_none());
}

#[gpui::test]
fn text_body_scrollbar_disappears_when_empty_or_short_content_fits(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| {
        let mut workspace = WorkspaceViewModel::new();
        let request = workspace.active_request_mut().unwrap();
        request.set_method(HttpMethod::POST);
        request.set_body_kind(BodyKind::Json);
        request.set_request_pane(RequestPane::Body);
        workspace
    });
    let observed = workspace.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    let handle = cx.update(|window, _| window.window_handle());
    // Match the native preview where an empty editor previously showed a full track.
    cx.simulate_window_resize(handle, gpui::size(px(1480.), px(978.)));
    ui::open_http(cx);
    for (kind, selector) in [
        (BodyKind::Json, "body-kind-json"),
        (BodyKind::Raw, "body-kind-raw"),
    ] {
        ui::choose_body_kind(cx, selector).unwrap();
        for body in [
            String::new(),
            (0..80).map(|line| format!("line-{line}\n")).collect(),
            String::new(),
            "short body".to_string(),
        ] {
            click(cx, "body-text-scroll").unwrap();
            cx.simulate_keystrokes("cmd-a");
            if body.is_empty() {
                cx.simulate_keystrokes("backspace");
            } else {
                cx.simulate_input(&body);
            }
            cx.run_until_parked();
            assert_eq!(
                workspace.read_with(cx, |model, _| model
                    .active_request()
                    .unwrap()
                    .body()
                    .to_string()),
                body
            );
            let viewport = cx.debug_bounds("body-text-scroll").unwrap();
            assert!(viewport.size.height > px(100.));
            if body.lines().count() > 1 {
                assert_scrollbar_at_viewport(cx, "body-text-scrollbar", "body-text-scroll");
                scroll_down(cx, "body-text-scroll", 1000.).unwrap();
            } else {
                assert!(
                    cx.debug_bounds("body-text-scrollbar").is_none(),
                    "{kind:?}: fitting content {body:?} must not show a scrollbar in {viewport:?}"
                );
            }
        }
    }
}

#[gpui::test]
fn raw_semantics_scrolls_internally_when_the_request_panel_is_narrowed(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| {
        let mut workspace = WorkspaceViewModel::new();
        let request = workspace.active_request_mut().unwrap();
        request.set_method(HttpMethod::PUT);
        request.set_url("https://httpbingo.org/anything/raw");
        request.set_body_kind(BodyKind::Raw);
        request.set_body("plain text body");
        request.set_request_pane(RequestPane::Body);
        workspace
    });
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);
    ui::show_body_details(cx).unwrap();
    click(cx, "response-layout-toggle").unwrap();
    let initial_height = cx.debug_bounds("request-panel").unwrap().size.height;

    let resize_handle = cx
        .debug_bounds("response-resize-handle")
        .expect("Response resize handle should render");
    let start = resize_handle.center();
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        point(start.x, start.y - px(6.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(
        point(start.x, start.y - px(60.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_up(
        point(start.x, start.y - px(60.0)),
        MouseButton::Left,
        Modifiers::none(),
    );

    assert!(
        (cx.debug_bounds("request-panel").unwrap().size.height - initial_height + px(60.)).abs()
            <= px(1.)
    );
    let rows = cx
        .debug_bounds("body-raw-semantics-scroll")
        .expect("Raw semantics should render inside a scroll viewport");
    let scrollbar = cx
        .debug_bounds("body-raw-scrollbar")
        .expect("overflowing Raw semantics should expose a scrollbar");
    let footer = cx
        .debug_bounds("body-raw-semantics-footer")
        .expect("the Raw semantics footer should remain fixed");
    assert_eq!(
        assert_scrollbar_at_viewport(cx, "body-raw-scrollbar", "body-raw-semantics-scroll"),
        scrollbar
    );
    let ready_before = cx.debug_bounds("body-raw-ready-indicator").unwrap();
    assert!(footer.origin.y >= rows.bottom());

    scroll_down(cx, "body-raw-semantics-scroll", 60.0).unwrap();
    let ready_after = cx
        .debug_bounds("body-raw-ready-indicator")
        .expect("the final Raw semantics row should be reachable by scrolling");
    let footer_after = cx
        .debug_bounds("body-raw-semantics-footer")
        .expect("the Raw footer should remain visible after scrolling");
    assert!(ready_after.top() < ready_before.top());
    assert_eq!(
        assert_scrollbar_at_viewport(cx, "body-raw-scrollbar", "body-raw-semantics-scroll"),
        scrollbar
    );
    assert!(ready_after.bottom() <= rows.bottom());
    assert_eq!(footer_after.origin.y, footer.origin.y);
    assert_scrollbar_pointer_moves_rows(
        cx,
        "body-raw-scrollbar",
        "body-raw-semantics-scroll",
        "body-raw-content-type-state",
    );
    assert!(
        cx.debug_bounds("body-raw-ready-indicator")
            .unwrap()
            .bottom()
            <= rows.bottom()
    );
    assert_eq!(
        cx.debug_bounds("body-raw-semantics-footer").unwrap(),
        footer
    );
}

#[gpui::test]
fn issue_58_url_encoded_contract_fits_the_editor_and_effective_preview(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_method(HttpMethod::POST);
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://httpbingo.org/anything/form");
        workspace
            .active_request_mut()
            .unwrap()
            .set_body_kind(BodyKind::UrlEncoded);
        workspace
            .active_request_mut()
            .unwrap()
            .set_body("name=Ada+Lovelace&active=true");
        workspace
            .active_request_mut()
            .unwrap()
            .set_request_pane(RequestPane::Body);
        workspace
    });
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);
    ui::show_body_details(cx).unwrap();

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

    assert!(panel.size.height > px(180.));
    assert_eq!(kinds.size.height, px(55.0));
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
fn urlencoded_prototype_rows_grow_then_scroll_with_the_preview_below_the_table(
    cx: &mut TestAppContext,
) {
    let workspace = cx.new(|_| {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_method(HttpMethod::POST);
        workspace
            .active_request_mut()
            .unwrap()
            .set_body_kind(BodyKind::UrlEncoded);
        workspace.active_request_mut().unwrap().set_body("");
        workspace
            .active_request_mut()
            .unwrap()
            .set_request_pane(RequestPane::Body);
        workspace
    });
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);
    let initial_panel = cx
        .debug_bounds("request-panel")
        .expect("URL-encoded request panel should render");
    let initial_rows = cx
        .debug_bounds("body-form-scroll")
        .expect("URL-encoded row viewport should render");
    assert!(initial_panel.size.height > px(180.));
    assert!(cx.debug_bounds("body-form-scrollbar").is_none());
    assert!(cx.debug_bounds("body-encoded-preview").is_some());
    assert!(cx.debug_bounds("request-context").is_none());
    let types = cx.debug_bounds("body-types").unwrap();
    for selector in [
        "body-kind-none",
        "body-kind-json",
        "body-kind-raw",
        "body-kind-url-encoded",
        "body-kind-form-data",
        "body-kind-binary",
    ] {
        let button = cx
            .debug_bounds(selector)
            .expect("all six body types are visible");
        assert!(button.left() >= types.left() && button.right() <= types.right());
        assert!(button.top() >= types.top() && button.bottom() <= types.bottom());
    }

    for _ in 0..4 {
        click(cx, "body-form-add-row").unwrap();
    }
    cx.run_until_parked();

    let grown_panel = cx
        .debug_bounds("request-panel")
        .expect("URL-encoded request panel should retain its divider");
    let grown_rows = cx
        .debug_bounds("body-form-scroll")
        .expect("URL-encoded row viewport should grow with rows");
    assert_eq!(
        grown_panel.size.height, initial_panel.size.height,
        "rows must not move the user-owned divider"
    );
    assert!(
        grown_rows.size.height > initial_rows.size.height,
        "the table should grow with its content until it reaches the available viewport"
    );
    let first_row = cx.debug_bounds("body-form-row-0").unwrap();
    let fifth_row = cx.debug_bounds("body-form-row-4").unwrap();
    assert_eq!(
        cx.debug_bounds("body-form-scrollbar").is_some(),
        fifth_row.bottom() - first_row.top() > grown_rows.size.height,
        "a partially filled table needs a scrollbar only when its rows actually overflow"
    );

    for _ in 0..20 {
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
    let add_action = cx
        .debug_bounds("body-form-add-row")
        .expect("Add form field should remain outside the row viewport");
    let preview = cx
        .debug_bounds("body-encoded-preview")
        .expect("encoded preview should remain available without opening Details");

    assert_eq!(capped_panel.size.height, initial_panel.size.height);
    assert!(response.size.height > px(0.0));
    assert_eq!(scrollbar.top(), rows_viewport.top());
    assert_eq!(scrollbar.bottom(), rows_viewport.bottom());
    assert_eq!(scrollbar.right(), rows_viewport.right());
    assert!(add_action.origin.y >= rows_viewport.bottom());
    assert!(preview.origin.y >= add_action.bottom());
    assert!(preview.bottom() <= capped_panel.bottom());
    assert!(cx.debug_bounds("body-form-add-row-hint").is_none());

    // Add field reveals the new row; return to the top before exercising wheel scrolling.
    scroll_up(cx, "body-form-scroll", 10_000.0).unwrap();
    let first_before_scroll = cx.debug_bounds("body-form-row-0").unwrap();
    assert!(first_before_scroll.top() >= rows_viewport.top());
    scroll_down(cx, "body-form-scroll", 90.0).unwrap();
    let first_after_scroll = cx.debug_bounds("body-form-row-0").unwrap();
    assert!(first_after_scroll.top() < first_before_scroll.top());
    let add_after_scroll = cx
        .debug_bounds("body-form-add-row")
        .expect("Add form field should remain visible after scrolling");
    let preview_after_scroll = cx
        .debug_bounds("body-encoded-preview")
        .expect("encoded preview should remain visible after scrolling");
    assert_eq!(add_after_scroll.origin.y, add_action.origin.y);
    assert_eq!(preview_after_scroll.origin.y, preview.origin.y);
    assert_eq!(cx.debug_bounds("request-panel").unwrap(), capped_panel);
}

#[gpui::test]
fn form_panel_preserves_split_when_blank_or_disabled_rows_are_added_and_removed(
    cx: &mut TestAppContext,
) {
    for kind in [BodyKind::UrlEncoded, BodyKind::Multipart] {
        let workspace = cx.new(|_| {
            let mut workspace = WorkspaceViewModel::new();
            let request = workspace.active_request_mut().unwrap();
            request.set_method(HttpMethod::POST);
            request.set_body_kind(kind);
            request.set_request_pane(RequestPane::Body);
            workspace
        });
        let (_app, cx) = cx.add_window_view(move |window, cx| {
            ui::shell(window, cx, |window, cx| {
                PostmanApp::with_view_model(workspace, window, cx)
            })
        });
        ui::open_http(cx);
        let initial = cx.debug_bounds("request-panel").unwrap().size.height;
        for _ in 0..4 {
            click(cx, "body-form-add-row").unwrap();
        }
        let grown = cx.debug_bounds("request-panel").unwrap().size.height;
        assert_eq!(
            grown, initial,
            "blank rows must not move the {kind:?} divider"
        );

        click(cx, "body-form-toggle-0").unwrap();
        assert_eq!(cx.debug_bounds("request-panel").unwrap().size.height, grown);
        click(cx, "body-form-delete-4").unwrap();
        let shrunk = cx.debug_bounds("request-panel").unwrap().size.height;
        assert_eq!(
            shrunk, grown,
            "removing a row must preserve the {kind:?} split"
        );
        assert!(cx.debug_bounds("body-form-row-4").is_none());
        click(cx, "body-form-delete-3").unwrap();
        click(cx, "body-form-delete-2").unwrap();
        click(cx, "body-form-delete-1").unwrap();
        click(cx, "body-form-delete-0").unwrap();
        assert!(cx.debug_bounds("body-form-row-0").is_some());
        assert!(cx.debug_bounds("body-form-row-1").is_none());
        assert_eq!(
            cx.debug_bounds("request-panel").unwrap().size.height,
            initial
        );
    }
}

#[gpui::test]
fn params_rows_grow_within_the_split_then_scroll(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);

    let initial_panel = cx
        .debug_bounds("request-panel")
        .expect("request panel should render");
    let initial_rows = cx
        .debug_bounds("params-rows-scroll")
        .expect("Params rows should render");
    assert!(initial_panel.size.height > px(180.));
    assert!(cx.debug_bounds("params-scrollbar").is_none());

    workspace.update(cx, |workspace, cx| {
        for _ in 0..3 {
            workspace.active_request_mut().unwrap().append_param_row();
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
    assert_eq!(
        grown_panel.size.height, initial_panel.size.height,
        "rows must not move the user-owned divider"
    );
    assert_eq!(
        grown_rows.size.height - initial_rows.size.height,
        px(120.0) // Three additional prototype rows at 40px each.
    );

    workspace.update(cx, |workspace, cx| {
        for _ in 0..20 {
            workspace.active_request_mut().unwrap().append_param_row();
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

    assert!(capped_panel.size.height >= grown_panel.size.height);
    assert_eq!(capped_panel.size.height, initial_panel.size.height);
    assert!(response.size.height > px(0.0));
    assert_eq!(
        assert_scrollbar_at_viewport(cx, "params-scrollbar", "params-rows-scroll"),
        scrollbar
    );
    assert_scrollbar_pointer_moves_rows(
        cx,
        "params-scrollbar",
        "params-rows-scroll",
        "param-row-0",
    );

    workspace.update(cx, |workspace, cx| {
        workspace.active_request_mut().unwrap().append_param_row();
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
fn header_rows_grow_within_the_split_then_scroll(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_request_pane(RequestPane::Headers);
        workspace
    });
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);

    let initial_panel = cx
        .debug_bounds("request-panel")
        .expect("Headers panel should render");
    let initial_rows = cx
        .debug_bounds("headers-rows-scroll")
        .expect("Headers rows should render");
    assert!(initial_panel.size.height > px(180.));
    assert!(cx.debug_bounds("headers-scrollbar").is_none());

    workspace.update(cx, |workspace, cx| {
        for _ in 0..3 {
            workspace.active_request_mut().unwrap().append_header_row();
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
    assert_eq!(
        grown_panel.size.height, initial_panel.size.height,
        "rows must not move the user-owned divider"
    );
    assert_eq!(
        grown_rows.size.height - initial_rows.size.height,
        px(120.0) // Three additional prototype rows at 40px each.
    );

    workspace.update(cx, |workspace, cx| {
        for _ in 0..20 {
            workspace.active_request_mut().unwrap().append_header_row();
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

    let add_action = cx
        .debug_bounds("add-row-button")
        .expect("Add Header should remain outside the scroll region");
    let rows_viewport = cx
        .debug_bounds("headers-rows-scroll")
        .expect("Headers rows should remain scrollable");

    assert!(capped_panel.size.height >= grown_panel.size.height);
    assert_eq!(capped_panel.size.height, initial_panel.size.height);
    assert!(response.size.height > px(0.0));
    assert_eq!(
        assert_scrollbar_at_viewport(cx, "headers-scrollbar", "headers-rows-scroll"),
        scrollbar
    );
    assert_scrollbar_pointer_moves_rows(
        cx,
        "headers-scrollbar",
        "headers-rows-scroll",
        "header-row-0",
    );
    assert!(add_action.origin.y >= rows_viewport.bottom());
}

#[gpui::test]
fn row_scrollbars_cover_partial_rows_and_disappear_when_all_rows_fit(cx: &mut TestAppContext) {
    for (pane, first, last, scroll, track, short_height) in [
        (
            "request-pane-headers",
            "header-row-0",
            "header-row-5",
            "headers-rows-scroll",
            "headers-scrollbar",
            740.,
        ),
        (
            "request-pane-params",
            "param-row-0",
            "param-row-5",
            "params-rows-scroll",
            "params-scrollbar",
            810.,
        ),
    ] {
        let workspace = cx.new(|_| WorkspaceViewModel::new());
        let (_, cx) = cx.add_window_view(move |window, cx| {
            ui::shell(window, cx, |window, cx| {
                PostmanApp::with_view_model(workspace, window, cx)
            })
        });
        let handle = cx.update(|window, _| window.window_handle());
        cx.simulate_window_resize(handle, gpui::size(px(1440.), px(1080.)));
        ui::open_http(cx);
        click(cx, pane).unwrap();
        for _ in 0..5 {
            click(cx, "add-row-button").unwrap();
        }
        let row_height = cx.debug_bounds(first).unwrap().size.height;
        let content_height = row_height * 6.;
        assert!(cx.debug_bounds(scroll).unwrap().size.height >= content_height);
        assert!(cx.debug_bounds(track).is_none());

        cx.simulate_window_resize(handle, gpui::size(px(1440.), px(short_height)));
        let mut adjusted_height = px(short_height);
        // Crossing the compact section-heading breakpoint needs a second measured frame.
        for _ in 0..3 {
            let clipped = cx.debug_bounds(scroll).unwrap().size.height;
            adjusted_height += content_height - row_height / 2. - clipped;
            cx.simulate_window_resize(handle, gpui::size(px(1440.), adjusted_height));
        }
        let viewport = cx.debug_bounds(scroll).unwrap();
        assert!(viewport.size.height < content_height);
        assert!(
            viewport.size.height > content_height - row_height,
            "{pane}: partial row viewport={viewport:?}, content={content_height:?}, adjusted_height={adjusted_height:?}"
        );
        assert!(
            cx.debug_bounds(track).is_some(),
            "{pane}: a partially clipped row needs a scrollbar"
        );
        let bar = assert_scrollbar_at_viewport(cx, track, scroll);
        let first_before = cx.debug_bounds(first).unwrap();
        scroll_down(cx, scroll, 1000.).unwrap();
        assert!(cx.debug_bounds(first).unwrap().top() < first_before.top());
        assert_eq!(assert_scrollbar_at_viewport(cx, track, scroll), bar);
        assert!(cx.debug_bounds(last).unwrap().bottom() <= viewport.bottom() + px(0.5));
        assert!(cx.debug_bounds("add-row-button").unwrap().top() >= viewport.bottom());

        cx.simulate_window_resize(handle, gpui::size(px(1440.), px(1080.)));
        assert!(cx.debug_bounds(track).is_none());
        assert!(cx.debug_bounds(first).unwrap().top() >= cx.debug_bounds(scroll).unwrap().top());
    }
}

#[gpui::test]
fn kit_request_editor_fits_minimum_window_in_both_themes_and_scales(cx: &mut TestAppContext) {
    use gpui::{size, SharedString};
    use gpui_kit::{component::ThemeMode, test::TestWindowExt};
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    click(cx, "home-open-http").unwrap();
    for _ in 0..17 {
        click(cx, "new-tab-button").unwrap();
    }
    for scale in [1., 2.] {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            for (width, height) in [(1440., 960.), (1920., 1080.), (1024., 768.), (960., 640.)] {
                let handle = cx.update(|window, cx| {
                    postman_gpui::ui::theme::apply(mode, cx);
                    window.window_handle()
                });
                cx.simulate_window_scale_factor_change(handle, scale);
                cx.simulate_window_resize(handle, size(px(width), px(height)));
                for pane in [
                    "request-pane-params",
                    "request-pane-headers",
                    "request-pane-body",
                    "request-pane-authorization",
                ] {
                    click(cx, pane).unwrap();
                    if pane == "request-pane-body" {
                        ui::choose_body_kind(cx, "body-kind-json").unwrap();
                    }
                    let footer = cx.debug_bounds("status-bar").unwrap();
                    let panel = cx.debug_bounds("request-panel").unwrap();
                    let response = cx.debug_bounds("response-container").unwrap();
                    let tabs = cx.debug_bounds("request-tabs-bar").unwrap();
                    assert!(tabs.size.height > px(38.), "many requests should wrap");
                    assert!(panel.top() >= tabs.bottom());
                    assert!(
                        response.bottom() <= footer.top() + px(0.5),
                        "{width}x{height} {pane}: {response:?} exceeds {footer:?}"
                    );
                    assert!(
                        response.size.height >= px(100.),
                        "{width}x{height} {pane}: response height {:?}",
                        response.size.height
                    );
                    let send = cx.debug_bounds("send-button").unwrap();
                    let url = cx.debug_bounds("url-input").unwrap();
                    assert!(url.right() < send.left());
                    assert_eq!(send.size.height, px(48.));
                    assert!(
                        send.right() < px(width),
                        "{width}x{height} {pane}: send={send:?} split={:?} head={:?}",
                        cx.debug_bounds("http-split"),
                        cx.debug_bounds("request-head")
                    );
                }
                // End/Home reveal the selected stable tab even when the strip scrolls.
                click(cx, "request-tab-17").unwrap();
                ui::press(cx, "home");
                assert_eq!(model.read_with(cx, |m, _| m.active_tab_index()), Some(0));
                ui::press(cx, "end");
                assert_eq!(model.read_with(cx, |m, _| m.active_tab_index()), Some(17));
                cx.update(|window, cx| {
                    window.render_frame(cx);
                    assert_eq!(window.find(("request-tab", 18u64)).selected(), Some(true));
                    assert_eq!(
                        window.find(SharedString::from("method-select")).value(),
                        Some("GET")
                    );
                });
            }
        }
    }
}

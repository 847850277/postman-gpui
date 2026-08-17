//! Visual-contract checks derived from `design/issue-0051-query-parameter-encoding.pen`.

use gpui::{px, AppContext, Modifiers, TestAppContext};
use postman_gpui::app::{PostmanApp, WorkspaceViewModel};
use postman_gpui::http::executor::RequestResult;

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

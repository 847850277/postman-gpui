//! Visual-contract checks derived from `design.pen`.

use gpui::{px, Modifiers, TestAppContext};
use postman_gpui::app::PostmanApp;

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
    assert_eq!(menu.origin.y, button.bottom() + px(2.0));
    assert_eq!(menu.size.width, button.size.width);
}

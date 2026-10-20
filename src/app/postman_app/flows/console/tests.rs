use super::super::tests::{add, click, create, press, replace, settle, setup};
use super::*;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{ElementInputHandler, InputHandler, TestAppContext, VisualTestContext};

fn command(cx: &mut VisualTestContext, value: &str) {
    replace(cx, "console-command", value);
    press(cx, "enter");
    settle(cx);
}
fn text(console: &Entity<FlowConsole>, cx: &mut VisualTestContext) -> String {
    console.read_with(cx, |c, _| c.current().unwrap().copy_text())
}

#[gpui_kit::test]
fn short_console_empty_message_starts_inside_its_scroll_view(cx: &mut TestAppContext) {
    use gpui_kit::{point, px, size, Modifiers, ScrollDelta, ScrollWheelEvent, TouchPhase};
    let (_, cx) = setup(cx);
    click(cx, "nav-flows");
    create(cx, "Small window");
    let handle = cx.update(|w, _| w.window_handle());
    cx.simulate_window_resize(handle, size(px(768.), px(600.)));
    settle(cx);
    let content = cx.debug_bounds("console-content").unwrap();
    let title = cx.debug_bounds("console-empty-title").unwrap();
    assert!(
        title.top() >= content.top(),
        "centered text is clipped above the viewport: {title:?}, {content:?}"
    );
    assert!(title.bottom() <= content.bottom());
    let command = cx.debug_bounds("console-command").unwrap();
    cx.simulate_event(ScrollWheelEvent {
        position: content.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(-100.))),
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
    });
    settle(cx);
    assert!(cx.debug_bounds("console-empty-title").unwrap().top() < title.top());
    assert_eq!(cx.debug_bounds("console-command").unwrap(), command);
}
#[gpui_kit::test]
fn commands_history_completion_and_view_keys_use_real_kit_controls(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    let console = flows.read_with(cx, |v, _| v.console.clone());
    click(cx, "nav-flows");
    create(cx, "Console commands");
    cx.update(|w, _| {
        assert_eq!(w.find("console-maximize").label(), Some("Maximize console"));
        assert_eq!(
            w.find("console-execute").label(),
            Some("Execute Flow command")
        );
    });
    replace(cx, "console-command", "he");
    press(cx, "tab");
    console.read_with(cx, |v, cx| assert_eq!(v.input.read(cx).value(), "help"));
    press(cx, "secondary-enter");
    settle(cx);
    assert!(text(&console, cx).contains("Show available Flow commands"));
    console.read_with(cx, |v, _| assert_eq!(v.current().unwrap().logs.len(), 2));
    cx.update(|w, cx| assert!(console.read(cx).input.focus_handle(cx).is_focused(w)));
    replace(cx, "console-command", "draft");
    press(cx, "up");
    console.read_with(cx, |v, cx| assert_eq!(v.input.read(cx).value(), "help"));
    press(cx, "down");
    console.read_with(cx, |v, cx| assert_eq!(v.input.read(cx).value(), "draft"));
    press(cx, "escape");
    console.read_with(cx, |v, cx| assert_eq!(v.input.read(cx).value(), ""));
    replace(cx, "console-command", "s");
    press(cx, "tab");
    cx.update(|w, cx| assert!(!console.read(cx).input.focus_handle(cx).is_focused(w)));
    replace(cx, "console-command", "help");
    press(cx, "shift-tab");
    cx.update(|w, cx| assert!(!console.read(cx).input.focus_handle(cx).is_focused(w)));
    command(cx, "  RUN ");
    assert!(text(&console, cx).contains("No requests sent"));
    command(cx, "check");
    assert!(text(&console, cx).contains("EmptyFlow"));
    command(cx, "stop");
    command(cx, "echo hello");
    console.read_with(cx, |v, _| {
        assert_eq!(
            v.current().unwrap().logs.back().unwrap().level,
            Level::Error
        )
    });
    command(cx, "steps");
    console.read_with(cx, |v, _| {
        assert_eq!(v.current().unwrap().view, View::Steps)
    });
    cx.update(|_, cx| {
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("unchanged".into()))
    });
    click(cx, "console-copy");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "unchanged"
        )
    });
    click(cx, "console-tab-logs");
    press(cx, "end");
    console.read_with(cx, |v, _| {
        assert_eq!(v.current().unwrap().view, View::Outputs)
    });
    press(cx, "left");
    console.read_with(cx, |v, _| {
        assert_eq!(v.current().unwrap().view, View::Steps)
    });
    press(cx, "home");
    console.read_with(cx, |v, _| assert_eq!(v.current().unwrap().view, View::Logs));
    command(cx, "clear");
    console.read_with(cx, |v, _| {
        assert!(v.current().unwrap().logs.is_empty());
        assert!(!v.current().unwrap().history.is_empty());
    });
    replace(cx, "console-command", "pending");
    click(cx, "console-toggle");
    settle(cx);
    cx.update(|w, cx| {
        assert_eq!(console.read(cx).mode, ConsoleMode::Collapsed);
        assert!(!console.read(cx).input.focus_handle(cx).is_focused(w));
    });
    // Enter on the visible tab reopens the panel without executing its hidden draft.
    press(cx, "enter");
    settle(cx);
    console.read_with(cx, |v, cx| {
        assert_eq!(v.mode, ConsoleMode::Normal);
        assert!(v.current().unwrap().logs.is_empty());
        assert_eq!(v.input.read(cx).value(), "pending");
    });
}

#[gpui_kit::test]
fn flow_console_isolation_filter_copy_clear_and_documents(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    let console = flows.read_with(cx, |v, _| v.console.clone());
    click(cx, "nav-flows");
    create(cx, "First");
    add(cx);
    let before = flows.read_with(cx, |v, _| {
        (
            v.library.active().unwrap().document().clone(),
            v.library.active().unwrap().revision(),
        )
    });
    command(cx, "help");
    command(cx, "unknown");
    click(cx, "console-level");
    for _ in 0..5 {
        press(cx, "down");
    }
    press(cx, "enter");
    settle(cx);
    console.read_with(cx, |v, _| {
        assert_eq!(v.current().unwrap().filter, Some(Level::Error))
    });
    assert!(!text(&console, cx).contains("Show available"));
    assert!(text(&console, cx).contains("Unknown Flow command"));
    click(cx, "console-copy");
    cx.update(|_, cx| {
        assert!(cx
            .read_from_clipboard()
            .unwrap()
            .text()
            .unwrap()
            .contains("Unknown Flow command"))
    });
    replace(cx, "console-command", "chec");
    create(cx, "Second");
    console.read_with(cx, |v, cx| {
        assert_eq!(v.input.read(cx).value(), "");
        assert!(v.current().unwrap().logs.is_empty());
    });
    command(cx, "help");
    replace(cx, "console-command", "second draft");
    click(cx, "nav-http");
    click(cx, "nav-flows");
    console.read_with(cx, |v, cx| {
        assert_eq!(v.input.read(cx).value(), "second draft")
    });
    click(cx, "flow-row-1");
    settle(cx);
    console.read_with(cx, |v, cx| {
        assert_eq!(v.input.read(cx).value(), "chec");
        assert_eq!(v.current().unwrap().filter, Some(Level::Error));
    });
    click(cx, "console-clear");
    console.read_with(cx, |v, _| {
        assert!(v.current().unwrap().logs.is_empty());
        assert_eq!(v.current().unwrap().history.len(), 2);
    });
    flows.read_with(cx, |v, _| {
        assert_eq!(v.library.active().unwrap().revision(), before.1);
        assert_eq!(v.library.active().unwrap().document(), &before.0);
    });
    click(cx, "flow-row-2");
    settle(cx);
    console.read_with(cx, |v, cx| {
        assert_eq!(v.input.read(cx).value(), "second draft");
        assert_eq!(v.current().unwrap().logs.len(), 2);
    });
}

#[gpui_kit::test]
fn composition_and_shortcuts_cannot_submit_or_escape_to_http(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    let console = flows.read_with(cx, |v, _| v.console.clone());
    click(cx, "nav-flows");
    create(cx, "IME");
    click(cx, "console-command");
    cx.update(|w, cx| {
        let mut input = ElementInputHandler::new(
            w.find("console-command").bounds(),
            console.read(cx).input.clone(),
        );
        input.replace_and_mark_text_in_range(None, "ni", Some(2..2), w, cx);
        assert!(input.marked_text_range(w, cx).is_some());
    });
    for key in ["enter", "secondary-enter", "escape", "tab", "up"] {
        press(cx, key);
    }
    console.read_with(cx, |v, cx| {
        assert!(v.current().unwrap().logs.is_empty());
        assert_eq!(v.input.read(cx).value(), "ni");
    });
    cx.update(|w, cx| {
        let mut input = ElementInputHandler::new(
            w.find("console-command").bounds(),
            console.read(cx).input.clone(),
        );
        input.replace_text_in_range(None, "你好", w, cx);
    });
    press(cx, "enter");
    settle(cx);
    console.read_with(cx, |v, _| {
        assert_eq!(v.current().unwrap().history.len(), 1);
        assert_eq!(v.current().unwrap().history[0], "你好");
    });
    press(cx, "secondary-s");
    app.read_with(cx, |v, cx| {
        assert_eq!(v.view_model.read(cx).in_flight_count(), 0)
    });
    cx.update(|w, cx| assert!(!gpui_kit::component::WindowExt::has_active_dialog(w, cx)));
}

#[gpui_kit::test]
fn virtual_logs_follow_pause_and_step_links_restore_canvas(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    let console = flows.read_with(cx, |v, _| v.console.clone());
    click(cx, "nav-flows");
    create(cx, "Log fixture");
    add(cx);
    add(cx);
    // Feed test-only events into the real Console, never a production mock executor.
    console.update(cx, |v, cx| {
        for ix in 0..510 {
            v.current_mut()
                .unwrap()
                .push(Level::Info, format!("event {ix}"), None);
        }
        v.current_mut()
            .unwrap()
            .push(Level::Error, "A check failed", Some("request-1".into()));
        v.refresh_logs(false);
        cx.notify();
    });
    settle(cx);
    console.read_with(cx, |v, _| {
        assert_eq!(v.visible.len(), 500);
        assert!(v.scroll.is_following_tail());
    });
    let bounds = cx.debug_bounds("console-content").unwrap();
    cx.simulate_event(gpui_kit::ScrollWheelEvent {
        position: bounds.center(),
        delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(150.))),
        ..Default::default()
    });
    settle(cx);
    console.read_with(cx, |v, _| assert!(!v.current().unwrap().follow));
    click(cx, "console-follow");
    settle(cx);
    console.read_with(cx, |v, _| assert!(v.current().unwrap().follow));
    click(cx, "console-maximize");
    settle(cx);
    assert!(cx.debug_bounds("flow-canvas").is_none());
    cx.update(|w, cx| w.click("console-step-1-511", cx));
    settle(cx);
    assert!(cx.debug_bounds("flow-canvas").is_some());
    flows.read_with(cx, |v, _| {
        assert_eq!(
            v.library.active().unwrap().selected().unwrap().id,
            "request-1"
        )
    });
    console.read_with(cx, |v, _| assert_eq!(v.mode, ConsoleMode::Normal));
}

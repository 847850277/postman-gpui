#![allow(dead_code)]

use gpui::{
    point, px, InputEvent, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, ScrollDelta,
    ScrollWheelEvent, VisualTestContext,
};

pub fn click(cx: &mut VisualTestContext, selector: &'static str) -> Result<(), String> {
    let bounds = cx
        .debug_bounds(selector)
        .ok_or_else(|| format!("application control `{selector}` is not rendered"))?;
    cx.simulate_click(bounds.center(), Modifiers::none());
    // GPUI's visual test platform queues mouse-up work until the next window update. Touching the
    // rendered frame here makes each driver click one complete user action instead of allowing two
    // adjacent clicks to observe the same pre-click frame.
    let _ = cx.debug_bounds(selector);
    Ok(())
}

/// Dispatches a click without draining outstanding background work. This is reserved for testing
/// a rendered Cancel transition while the request task is deliberately still in flight.
pub fn click_without_wait(
    cx: &mut VisualTestContext,
    selector: &'static str,
) -> Result<(), String> {
    let bounds = cx
        .debug_bounds(selector)
        .ok_or_else(|| format!("application control `{selector}` is not rendered"))?;
    let position = bounds.center();
    cx.update(|window, app| {
        window.dispatch_event(
            MouseDownEvent {
                position,
                modifiers: Modifiers::none(),
                button: MouseButton::Left,
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            app,
        );
        window.dispatch_event(
            MouseUpEvent {
                position,
                modifiers: Modifiers::none(),
                button: MouseButton::Left,
                click_count: 1,
            }
            .to_platform_input(),
            app,
        );
    });
    Ok(())
}

pub fn right_click(cx: &mut VisualTestContext, selector: &'static str) -> Result<(), String> {
    let bounds = cx
        .debug_bounds(selector)
        .ok_or_else(|| format!("application control `{selector}` is not rendered"))?;
    let position = bounds.center();
    cx.simulate_mouse_down(position, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(position, MouseButton::Right, Modifiers::none());
    let _ = cx.debug_bounds(selector);
    Ok(())
}

pub fn type_into(
    cx: &mut VisualTestContext,
    selector: &'static str,
    value: &str,
) -> Result<(), String> {
    click(cx, selector)?;
    cx.simulate_input(value);
    Ok(())
}

pub fn replace_text(
    cx: &mut VisualTestContext,
    selector: &'static str,
    value: &str,
) -> Result<(), String> {
    click(cx, selector)?;
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input(value);
    Ok(())
}

pub fn scroll_down(
    cx: &mut VisualTestContext,
    selector: &'static str,
    pixels: f32,
) -> Result<(), String> {
    let bounds = cx
        .debug_bounds(selector)
        .ok_or_else(|| format!("application scroll area `{selector}` is not rendered"))?;
    cx.simulate_event(ScrollWheelEvent {
        position: bounds.center(),
        delta: ScrollDelta::Pixels(point(px(0.0), px(-pixels))),
        ..Default::default()
    });
    let _ = cx.debug_bounds(selector);
    Ok(())
}

pub fn scroll_up(
    cx: &mut VisualTestContext,
    selector: &'static str,
    pixels: f32,
) -> Result<(), String> {
    let bounds = cx
        .debug_bounds(selector)
        .ok_or_else(|| format!("application scroll area `{selector}` is not rendered"))?;
    cx.simulate_event(ScrollWheelEvent {
        position: bounds.center(),
        delta: ScrollDelta::Pixels(point(px(0.0), px(pixels))),
        ..Default::default()
    });
    let _ = cx.debug_bounds(selector);
    Ok(())
}

pub fn choose_method(cx: &mut VisualTestContext, method: &str) -> Result<(), String> {
    use gpui_kit::test::TestWindowExt;
    let method = method.to_ascii_uppercase();
    let index = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"]
        .iter()
        .position(|m| *m == method)
        .ok_or_else(|| format!("unsupported method `{method}`"))?;
    cx.update(|window, cx| window.click("method-select", cx));
    cx.update(|window, cx| {
        window.press("home", cx);
        for _ in 0..index {
            window.press("down", cx);
        }
        window.press("enter", cx);
    });
    Ok(())
}

/// Select a visible Body type without opening an unrelated popup.
pub fn choose_body_kind(cx: &mut VisualTestContext, kind: &'static str) -> Result<(), String> {
    if cx.debug_bounds("body-details").is_some() {
        click(cx, "body-details-toggle")?;
    }
    click(cx, kind)
}
pub fn choose_auth_kind(cx: &mut VisualTestContext, kind: &'static str) -> Result<(), String> {
    let index = ["auth-kind-bearer", "auth-kind-basic"]
        .iter()
        .position(|candidate| *candidate == kind)
        .ok_or_else(|| format!("unknown auth kind {kind}"))?;
    choose_option(cx, "authorization-kind-select", index);
    Ok(())
}
pub fn choose_option(cx: &mut VisualTestContext, selector: &'static str, index: usize) {
    use gpui_kit::test::TestWindowExt;
    let labels: &[&str] = if selector == "body-kind-select" {
        &["None", "Form-data", "URL encoded", "Raw", "JSON", "Binary"]
    } else if selector == "body-raw-format" {
        &["Text", "XML", "HTML", "JavaScript"]
    } else {
        &["Bearer token", "Basic auth"]
    };
    let current = cx.update(|window, _| {
        let observed = window.find(selector);
        labels
            .iter()
            .position(|label| Some(*label) == observed.value())
            .unwrap()
    });
    cx.update(|window, cx| window.click(selector, cx));
    cx.run_until_parked();
    for _ in 0..index.abs_diff(current) {
        press(cx, if index > current { "down" } else { "up" });
    }
    press(cx, "enter");
    cx.run_until_parked();
}
pub fn body_action(cx: &mut VisualTestContext, index: usize) -> Result<(), String> {
    click(cx, "body-actions")?;
    press(cx, "down");
    for _ in 0..index {
        press(cx, "down");
    }
    press(cx, "enter");
    Ok(())
}

/// Build the same Kit Root and Home-first shell as the executable; HTTP tests navigate
/// through visible controls explicitly rather than substituting a test-only start route.
pub fn shell(
    window: &mut gpui::Window,
    cx: &mut gpui::Context<gpui_kit::component::Root>,
    build: impl FnOnce(
        &mut gpui::Window,
        &mut gpui::Context<postman_gpui::app::PostmanApp>,
    ) -> postman_gpui::app::PostmanApp,
) -> gpui_kit::component::Root {
    use gpui::AppContext;
    if cx.try_global::<gpui_kit::component::Theme>().is_none() {
        postman_gpui::assets::fonts::load_embedded_fonts(cx).unwrap();
        postman_gpui::ui::kit::init(cx);
        cx.set_reduce_motion(true);
    }
    let app = cx.new(|cx| build(window, cx));
    gpui_kit::component::Root::new(app, window, cx)
}

pub fn open_http(cx: &mut VisualTestContext) {
    click(cx, "nav-http").unwrap();
    click(cx, "rail-history").unwrap();
}

/// Kit controls expose stable IDs through Kit's native test observations.
pub fn kit_control_exists(cx: &mut VisualTestContext, id: &'static str) -> bool {
    use gpui_kit::test::TestWindowExt;
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some()
    })
}

/// Complete native key-down/key-up pairs (Kit buttons activate on key-up).
pub fn press(cx: &mut VisualTestContext, keys: &str) {
    use gpui_kit::test::TestWindowExt;
    for key in keys.split_whitespace() {
        cx.update(|window, cx| window.press(key, cx));
    }
}

/// Diagnostics are a deliberate disclosure, not a permanent sibling of the editor.
pub fn show_body_details(cx: &mut VisualTestContext) -> Result<(), String> {
    if cx.debug_bounds("body-details").is_none() {
        body_action(cx, 2)?;
    }
    Ok(())
}

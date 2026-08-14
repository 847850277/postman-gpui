#![allow(dead_code)]

use gpui::{Modifiers, MouseButton, VisualTestContext};

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

pub fn choose_method(cx: &mut VisualTestContext, method: &str) -> Result<(), String> {
    click(cx, "method-dropdown-button")?;
    match method.to_ascii_lowercase().as_str() {
        "get" => click(cx, "method-option-get"),
        "post" => click(cx, "method-option-post"),
        "put" => click(cx, "method-option-put"),
        "delete" => click(cx, "method-option-delete"),
        "patch" => click(cx, "method-option-patch"),
        "head" => click(cx, "method-option-head"),
        "options" => click(cx, "method-option-options"),
        _ => Err(format!("unsupported method `{method}`")),
    }
}

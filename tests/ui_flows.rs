//! Responsive native Flow layout and pointer/keyboard splitter behavior.
#[path = "common/ui.rs"]
mod ui;
use gpui::{
    point, px, size, AppContext, Modifiers, MouseButton, TestAppContext, VisualTestContext,
};
use gpui_kit::test::TestWindowExt;
use postman_gpui::app::{PostmanApp, WorkspaceViewModel};
fn click(cx: &mut VisualTestContext, id: &'static str) -> Result<(), String> {
    if ui::kit_control_exists(cx, id) {
        cx.update(|w, cx| w.click(id, cx));
        Ok(())
    } else {
        ui::click(cx, id)
    }
}

fn settle(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    // Nested Kit groups measure their children before applying retained preferred sizes.
    for _ in 0..6 {
        let _ = cx.debug_bounds("flows-screen");
    }
}
fn resize(cx: &mut VisualTestContext, w: f32, h: f32) {
    let handle = cx.update(|w, _| w.window_handle());
    cx.simulate_window_resize(handle, size(px(w), px(h)));
    settle(cx);
}
fn create(cx: &mut VisualTestContext, name: &str) {
    click(cx, "new-flow").unwrap();
    cx.simulate_input(name);
    click(cx, "flow-create-submit").unwrap();
    settle(cx);
}
fn add(cx: &mut VisualTestContext) {
    click(cx, "add-step").unwrap();
    click(cx, "flow-add-http").unwrap();
    settle(cx);
}
fn drag(cx: &mut VisualTestContext, id: &'static str, dx: f32, dy: f32, cancel: bool) {
    let start = cx.debug_bounds(id).unwrap().center();
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        start + point(px(dx.signum() * 6.), px(dy.signum() * 6.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    cx.simulate_mouse_move(
        start + point(px(dx), px(dy)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    if cancel {
        ui::press(cx, "escape");
    }
    cx.simulate_mouse_up(
        start + point(px(dx), px(dy)),
        MouseButton::Left,
        Modifiers::none(),
    );
    settle(cx);
}

#[gpui_kit::test]
fn flow_layout_keeps_fields_actions_and_dividers_inside_each_panel(cx: &mut TestAppContext) {
    let model = cx.new(|_| WorkspaceViewModel::new());
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| PostmanApp::with_view_model(model, w, cx))
    });
    click(cx, "nav-flows").unwrap();
    resize(cx, 1440., 960.);
    create(
        cx,
        "An intentionally long flow title 用于验证缩放和紧凑布局",
    );
    add(cx);
    ui::replace_text(
        cx,
        "flow-step-name",
        "A long step name with 中文 and 🦀 that should not widen the canvas",
    )
    .unwrap();
    ui::replace_text(
        cx,
        "flow-step-url",
        &format!(
            "https://httpbingo.org/anything?value={}",
            "long".repeat(150)
        ),
    )
    .unwrap();
    for theme in [
        gpui_kit::component::ThemeMode::Light,
        gpui_kit::component::ThemeMode::Dark,
    ] {
        cx.update(|_, cx| postman_gpui::ui::theme::apply(theme, cx));
        for scale in [1., 2.] {
            let handle = cx.update(|w, _| w.window_handle());
            cx.simulate_window_scale_factor_change(handle, scale);
            for (w, h) in [
                (1440., 960.),
                (1920., 1080.),
                (1024., 768.),
                (768., 720.),
                (1024., 600.),
            ] {
                resize(cx, w, h);
                assert_eq!(cx.debug_bounds("flow-library").is_some(), w > 1150.);
                assert_eq!(cx.debug_bounds("flow-library-divider").is_some(), w > 1150.);
                assert_eq!(
                    cx.debug_bounds("flow-inspector-divider").is_some(),
                    w > 800.
                );
                for id in [
                    "flow-topbar",
                    "flow-toolbar",
                    "flow-step-name",
                    "flow-step-method",
                    "flow-step-url",
                    "flow-results",
                    "console-heading",
                    "console-tools",
                    "console-command-bar",
                    "console-command",
                ] {
                    let b = cx
                        .debug_bounds(id)
                        .unwrap_or_else(|| panic!("{id} absent at {w}x{h}"));
                    assert!(
                        b.left() >= px(0.)
                            && b.right() <= px(w + 1.)
                            && b.top() >= px(0.)
                            && b.bottom() <= px(h + 1.),
                        "{id} at {w}x{h}: {b:?}"
                    );
                    assert!(
                        b.size.width > px(0.) && b.size.height > px(0.),
                        "{id}: {b:?}"
                    );
                }
                let results = cx.debug_bounds("flow-results").unwrap();
                let command = cx.debug_bounds("console-command-bar").unwrap();
                let content = cx.debug_bounds("console-content").unwrap();
                assert!(command.bottom() <= results.bottom() + px(1.));
                assert!(content.bottom() <= command.top() + px(1.));
                assert!(content.size.height > px(10.));
                let inspector = cx.debug_bounds("flow-inspector").unwrap();
                assert!(inspector.bottom() <= results.top() + px(1.));
                if w <= 800. {
                    assert!(cx.debug_bounds("flow-canvas").is_none());
                    click(cx, "flow-toggle-details").unwrap();
                    settle(cx);
                    assert!(cx.debug_bounds("flow-canvas").is_some());
                    assert!(cx.debug_bounds("flow-inspector").is_none());
                    click(cx, "flow-toggle-details").unwrap();
                    settle(cx);
                }
            }
        }
    }
}

#[gpui_kit::test]
fn flow_splitters_drag_cancel_keyboard_and_restore_preferred_sizes(cx: &mut TestAppContext) {
    let model = cx.new(|_| WorkspaceViewModel::new());
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| PostmanApp::with_view_model(model, w, cx))
    });
    click(cx, "nav-flows").unwrap();
    resize(cx, 1440., 960.);
    create(cx, "Resize flow");
    add(cx);
    for (divider, panel, dx, dy) in [
        ("flow-library-divider", "flow-library", 64., 0.),
        ("flow-inspector-divider", "flow-inspector", -64., 0.),
        ("flow-results-divider", "flow-results", 0., -48.),
    ] {
        let extent = |cx: &mut VisualTestContext| {
            let b = cx.debug_bounds(panel).unwrap();
            if dy == 0. {
                b.size.width.as_f32()
            } else {
                b.size.height.as_f32()
            }
        };
        let before = extent(cx);
        drag(cx, divider, dx, dy, false);
        let after = extent(cx);
        assert!(after > before + 20., "{divider}: {before} -> {after}");
        drag(cx, divider, dx, dy, true);
        assert!(
            (extent(cx) - after).abs() < 2.,
            "Escape restores size before drag"
        );
        click(cx, divider).unwrap();
        ui::press(cx, if dy == 0. { "left" } else { "up" });
        settle(cx);
        let preferred = extent(cx);
        assert!((preferred - after).abs() > 4.);
        resize(cx, 768., 720.);
        resize(cx, 1440., 960.);
        assert!(
            (extent(cx) - preferred).abs() < 2.,
            "{divider} must restore preferred size after expand"
        );
        click(cx, divider).unwrap();
        ui::press(cx, "enter");
        assert!(cx.debug_bounds("flow-size-input").is_some());
        click(cx, "flow-size-reset").unwrap();
        click(cx, "flow-size-apply").unwrap();
        settle(cx);
        assert!(
            (extent(cx) - before).abs() < 2.,
            "{divider} reset {before} -> {}",
            extent(cx)
        );
    }
}

#[gpui_kit::test]
fn console_modes_and_wide_inspector_preserve_normal_sizes(cx: &mut TestAppContext) {
    let model = cx.new(|_| WorkspaceViewModel::new());
    let (_, cx) = cx.add_window_view(move |w, cx| {
        ui::shell(w, cx, |w, cx| PostmanApp::with_view_model(model, w, cx))
    });
    click(cx, "nav-flows").unwrap();
    resize(cx, 1440., 960.);
    // A new session must not fabricate flows or command results.
    click(cx, "console-execute").unwrap();
    click(cx, "console-help").unwrap();
    create(cx, "Console geometry");
    add(cx);
    drag(cx, "flow-inspector-divider", -400., 0., false);
    let width = cx.debug_bounds("flow-inspector").unwrap().size.width;
    assert!(
        width > px(600.),
        "inspector should expand past its old limit: {width:?}"
    );
    assert!(cx.debug_bounds("flow-canvas").unwrap().size.width >= px(299.));
    let normal = cx.debug_bounds("flow-results").unwrap();
    assert!((normal.size.height - px(240.)).abs() < px(2.));
    for _ in 0..2 {
        click(cx, "console-maximize").unwrap();
        settle(cx);
        assert!(cx.debug_bounds("flow-canvas").is_none());
        assert!(cx.debug_bounds("flow-inspector-divider").is_none());
        assert!(cx.debug_bounds("flow-results-divider").is_none());
        assert!(cx.debug_bounds("flow-results").unwrap().size.height > normal.size.height);
        click(cx, "console-maximize").unwrap();
        settle(cx);
        assert!(
            (cx.debug_bounds("flow-results").unwrap().size.height - normal.size.height).abs()
                < px(2.)
        );
        assert!((cx.debug_bounds("flow-inspector").unwrap().size.width - width).abs() < px(2.));
        click(cx, "console-toggle").unwrap();
        settle(cx);
        assert!(cx.debug_bounds("console-command-bar").is_none());
        assert!(cx.debug_bounds("flow-results-divider").is_none());
        assert!((cx.debug_bounds("flow-results").unwrap().size.height - px(44.)).abs() < px(2.));
        click(cx, "console-tab-steps").unwrap();
        settle(cx);
        assert!(cx.debug_bounds("console-command-bar").is_some());
        assert!(
            (cx.debug_bounds("flow-results").unwrap().size.height - normal.size.height).abs()
                < px(2.)
        );
    }
    resize(cx, 1024., 600.);
    resize(cx, 1440., 960.);
    assert!((cx.debug_bounds("flow-inspector").unwrap().size.width - width).abs() < px(2.));
    assert!(
        (cx.debug_bounds("flow-results").unwrap().size.height - normal.size.height).abs() < px(2.)
    );
}

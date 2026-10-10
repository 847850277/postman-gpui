//! Kit integration checks through the production application and HTTP editor.

#[path = "common/ui.rs"]
mod ui;

use gpui::{AppContext, ClipboardItem, TestAppContext};
use gpui_kit::{
    component::{Root, Theme, ThemeMode},
    test::TestWindowExt,
};
use postman_gpui::{
    app::{PostmanApp, ResponseState, WorkspaceViewModel},
    assets::fonts::load_embedded_fonts,
    ui::kit,
};

fn init(cx: &mut TestAppContext) {
    cx.update(|cx| {
        load_embedded_fonts(cx).unwrap();
        kit::init(cx);
        cx.set_reduce_motion(true);
        let theme = Theme::global(cx);
        assert_eq!(theme.mode, ThemeMode::Light);
        assert_eq!(theme.font_family, "Inter");
        assert_eq!(theme.mono_font_family, "JetBrains Mono");
    });
}

#[gpui::test]
fn http_url_clipboard_and_help_preserve_text_and_focus_across_theme_changes(
    cx: &mut TestAppContext,
) {
    init(cx);
    let model = cx.new(|_| WorkspaceViewModel::new());
    let observed = model.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let app = cx.new(|cx| PostmanApp::with_view_model(observed, window, cx));
        Root::new(app, window, cx)
    });
    ui::click(cx, "home-open-http").unwrap();
    let value = "https://example.test/世界/🦀";
    cx.update(|window, cx| {
        window.click("request-url-input", cx);
        window.input(value, cx);
        window.press("secondary-a", cx);
        window.press("secondary-c", cx);
        assert_eq!(
            cx.read_from_clipboard().and_then(|item| item.text()),
            Some(value.into())
        );
        window.press("backspace", cx);
        assert_eq!(window.find("request-url-input").value(), Some(""));
        window.press("secondary-v", cx);
        assert_eq!(window.find("request-url-input").value(), Some(value));
        window.press("tab", cx);
        assert_eq!(window.find("send-button").focused(), Some(true));
        window.press("shift-tab", cx);
        assert_eq!(window.find("request-url-input").focused(), Some(true));
        window.press("ctrl-/", cx);
    });
    assert!(cx.debug_bounds("shortcut-help-dialog").is_some());
    cx.update(|window, cx| {
        postman_gpui::ui::theme::apply(ThemeMode::Dark, cx);
        window.render_frame(cx);
        assert_eq!(Theme::global(cx).mode, ThemeMode::Dark);
        assert_eq!(window.find("request-url-input").value(), Some(value));
    });
    assert!(cx.debug_bounds("shortcut-help-dialog").is_some());
    ui::press(cx, "escape");
    assert!(cx.debug_bounds("shortcut-help-dialog").is_none());
    cx.update(|window, cx| {
        assert_eq!(window.find("request-url-input").focused(), Some(true));
        window.input("!", cx);
    });
    assert_eq!(
        model.read_with(cx, |m, _| m.active_request().unwrap().url().to_owned()),
        format!("{value}!")
    );
}

#[gpui::test]
fn kit_root_preserves_existing_http_shortcuts_focused_input_and_history(cx: &mut TestAppContext) {
    init(cx);
    let mut server = mockito::Server::new();
    let response = server
        .mock("GET", "/kit")
        .with_status(200)
        .with_body("kit-http-ok")
        .create();
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let app = cx.new(|cx| PostmanApp::with_view_model(observed, window, cx));
        Root::new(app, window, cx)
    });

    ui::open_http(cx);
    cx.write_to_clipboard(ClipboardItem::new_string(format!("{}/kit", server.url())));
    cx.simulate_keystrokes("ctrl-l ctrl-a ctrl-v ctrl-enter");
    cx.run_until_parked();
    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.active_request().unwrap().response().clone()),
        ResponseState::Success { status: 200, ref body, .. } if body == "kit-http-ok"
    ));
    assert!(cx.debug_bounds("history-item-0").is_some());
    ui::click(cx, "response-pane-body").unwrap();
    cx.simulate_keystrokes("right");
    assert!(cx.debug_bounds("response-pane-headers-active").is_some());
    cx.simulate_keystrokes("ctrl-/");
    assert!(cx.debug_bounds("shortcut-help-dialog").is_some());
    cx.simulate_keystrokes("escape");
    assert!(cx.debug_bounds("shortcut-help-dialog").is_none());
    response.assert();
}

// Component geometry is measured in logical px. Pixel comparisons intentionally use
// separate surface/border and glyph masks (see ui_visual_compare.py); font AA must
// never excuse overlapping bounds or clipped editor geometry.
const GEOMETRY_EPSILON: f32 = 0.5;

fn assert_one_focus_frame(window: &gpui::Window, cx: &gpui::App, group: &'static str) {
    let bounds = window.find(group).bounds().scale(window.scale_factor());
    let frames: Vec<_> = window
        .painted_quads()
        .into_iter()
        .filter(|quad| {
            quad.border_color == Theme::global(cx).ring
                && quad.border_widths.top.as_f32() > 0.
                && quad.border_widths.bottom.as_f32() > 0.
                && quad.bounds.left() >= bounds.left()
                && quad.bounds.right() <= bounds.right()
                && quad.bounds.top() >= bounds.top()
                && quad.bounds.bottom() <= bounds.bottom()
        })
        .collect();
    assert!(!frames.is_empty(), "input group must paint a focus frame");
    // GPUI may clip one border into several painted strips. Every strip must
    // belong to the outer group; a nested input frame has different bounds.
    for frame in frames {
        assert_eq!(frame.bounds, bounds, "input group has a nested focus frame");
        assert_eq!(frame.border_widths.top.as_f32(), window.scale_factor());
    }
}

#[gpui_kit::test]
fn http_url_group_keeps_one_focus_frame_and_long_text_inside_its_bounds(cx: &mut TestAppContext) {
    init(cx);
    for (width, height) in [(1440., 960.), (960., 640.)] {
        let handle = cx.open_window(
            gpui::size(gpui::px(width), gpui::px(height)),
            |window, cx| {
                let model = cx.new(|_| WorkspaceViewModel::new());
                let view = cx.new(|cx| PostmanApp::with_view_model(model, window, cx));
                Root::new(view, window, cx)
            },
        );
        cx.update_window(handle.into(), |_, window, cx| {
            window.click("home-open-http", cx);
            let group = window.find("request-url-group").bounds();
            let input = window.find("request-url-input").bounds();
            let method = window.find("method-select").bounds();
            assert!((f32::from(group.size.height) - 48.).abs() <= GEOMETRY_EPSILON);
            assert!(input.left() >= method.right());
            assert!(input.right() < group.right());
            assert!(input.top() >= group.top() && input.bottom() <= group.bottom());
            assert!(group.right() <= gpui::px(width));
            window.click("request-url-input", cx);
            assert_one_focus_frame(window, cx, "request-url-group");
            let long = format!("https://example.test/{}", "长路径🦀?q=x&".repeat(80));
            cx.write_to_clipboard(ClipboardItem::new_string(long.clone()));
            window.press("secondary-a", cx);
            window.press("secondary-v", cx);
            assert_eq!(
                window.find("request-url-input").value(),
                Some(long.as_str())
            );
            assert_eq!(window.find("request-url-input").bounds(), input);
            postman_gpui::ui::theme::apply(ThemeMode::Dark, cx);
            window.render_frame(cx);
            assert_eq!(window.find("request-url-input").focused(), Some(true));
            assert_eq!(
                window.find("request-url-input").value(),
                Some(long.as_str())
            );
            assert_eq!(window.find("request-url-input").bounds(), input);
            assert_one_focus_frame(window, cx, "request-url-group");
            postman_gpui::ui::theme::apply(ThemeMode::Light, cx);
        })
        .unwrap();
    }
}

#[test]
fn product_icons_are_embedded_alongside_kits_default_icons() {
    use gpui::AssetSource;
    for icon in [
        gpui_kit::assets::IconName::Lock,
        gpui_kit::assets::IconName::Moon,
        gpui_kit::assets::IconName::Loader,
    ] {
        let bytes = postman_gpui::assets::KitAssets
            .load(&icon.path())
            .unwrap()
            .unwrap();
        assert!(bytes.starts_with(b"<svg"), "bundled icon must be SVG");
    }
}

//! P0 compatibility checks through the same Kit root used by the native executable.

#[path = "common/ui.rs"]
mod ui;

use std::time::Duration;

use gpui::{AppContext, ClipboardItem, TestAppContext, WindowOptions};
use gpui_kit::{
    component::{Root, Theme, ThemeMode},
    test::{TestAppContextExt, TestWindowExt},
};
use postman_gpui::{
    app::{kit_smoke::KitSmokeView, PostmanApp, ResponseState, WorkspaceViewModel},
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
async fn kit_input_keyboard_clipboard_and_dialog_share_the_application_root(
    cx: &mut TestAppContext,
) {
    init(cx);
    let (handle, _) = cx
        .update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| KitSmokeView::new(window, cx))
            })
        })
        .unwrap();
    assert!(handle.downcast::<Root>().is_some());

    cx.update_window(handle, |_, window, cx| {
        window.click("kit-smoke-input", cx);
        window.input("Hello, 世界 🦀", cx);
        window.press(
            if cfg!(target_os = "macos") {
                "cmd-a"
            } else {
                "ctrl-a"
            },
            cx,
        );
        window.press(
            if cfg!(target_os = "macos") {
                "cmd-c"
            } else {
                "ctrl-c"
            },
            cx,
        );
        assert_eq!(
            cx.read_from_clipboard().and_then(|item| item.text()),
            Some("Hello, 世界 🦀".into())
        );
        window.press("backspace", cx);
        assert_eq!(window.find("kit-smoke-input").value(), Some(""));
        window.press(
            if cfg!(target_os = "macos") {
                "cmd-v"
            } else {
                "ctrl-v"
            },
            cx,
        );
        assert_eq!(
            window.find("kit-smoke-input").value(),
            Some("Hello, 世界 🦀")
        );
        window.press("tab", cx);
        assert_eq!(window.find("kit-smoke-open").focused(), Some(true));
        window.press("shift-tab", cx);
        assert_eq!(window.find("kit-smoke-input").focused(), Some(true));
        window.press("tab", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(1), |window, _| {
        window.try_find("dialog").is_some()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(
            window.within("dialog").find("kit-smoke-value").label(),
            Some("Hello, 世界 🦀")
        );
        window.press("escape", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(1), |window, _| {
        window.try_find("dialog").is_none()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(window.find("kit-smoke-open").focused(), Some(true));
        window.click("kit-smoke-input", cx);
        window.input("!", cx);
        window.click("kit-smoke-open", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(1), |window, _| {
        window.try_find("dialog").is_some()
    })
    .await;
    cx.update_window(handle, |_, window, _| {
        assert_eq!(
            window.within("dialog").find("kit-smoke-value").label(),
            Some("Hello, 世界 🦀!")
        );
    })
    .unwrap();
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
        let app = cx.new(|cx| PostmanApp::with_view_model(observed, cx));
        Root::new(app, window, cx)
    });

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
fn shared_groups_keep_text_inside_their_frames_at_reference_and_minimum_sizes(
    cx: &mut TestAppContext,
) {
    init(cx);
    for (width, height) in [(1440., 960.), (960., 640.)] {
        let handle = cx.open_window(
            gpui::size(gpui::px(width), gpui::px(height)),
            |window, cx| {
                let view = cx.new(|cx| KitSmokeView::new(window, cx));
                Root::new(view, window, cx)
            },
        );
        cx.update_window(handle.into(), |_, window, cx| {
            window.render_frame(cx);
            let group = window.find("kit-url-group").bounds();
            let input = window.find("kit-url").bounds();
            let method = window.find("method-select").bounds();
            assert!((f32::from(group.size.height) - 48.).abs() <= GEOMETRY_EPSILON);
            assert!(
                (f32::from(window.find("kit-search-group").bounds().size.height) - 36.).abs()
                    <= GEOMETRY_EPSILON
            );
            assert!(input.left() >= method.right());
            assert!(input.right() < group.right());
            assert!(input.top() >= group.top() && input.bottom() <= group.bottom());
            assert!(group.right() <= gpui::px(width));
            window.click("kit-search", cx);
            assert_one_focus_frame(window, cx, "kit-search-group");
            window.click("kit-url", cx);
            assert_one_focus_frame(window, cx, "kit-url-group");
            let long = format!("https://example.test/{}", "长路径🦀?q=x&".repeat(80));
            cx.write_to_clipboard(ClipboardItem::new_string(long.clone()));
            window.press("secondary-a", cx);
            window.press("secondary-v", cx);
            assert_eq!(window.find("kit-url").value(), Some(long.as_str()));
            assert_eq!(window.find("kit-url").bounds(), input);
            postman_gpui::ui::theme::apply(ThemeMode::Dark, cx);
            window.render_frame(cx);
            assert_eq!(window.find("kit-url").focused(), Some(true));
            assert_eq!(window.find("kit-url").value(), Some(long.as_str()));
            assert_eq!(window.find("kit-url").bounds(), input);
            assert_eq!(
                Theme::global(cx).background,
                postman_gpui::ui::theme::PANEL
                    .for_mode(ThemeMode::Dark)
                    .into()
            );
            window.press("secondary-a", cx);
            window.input("bad-url", cx);
            assert_eq!(
                window.find("kit-url-error").label(),
                Some("Enter a valid URL")
            );
            window.click("kit-disable-url", cx);
            window.click("kit-url", cx);
            window.input("must not edit", cx);
            assert_eq!(window.find("kit-url").value(), Some("bad-url"));
            let method = window.find("method-select").value().map(str::to_owned);
            window.click("method-select", cx);
            window.press("down", cx);
            window.press("enter", cx);
            assert_eq!(window.find("method-select").value(), method.as_deref());
            window.click("kit-row-enabled", cx);
            window.click("kit-row-value", cx);
            window.input("2", cx);
            assert_eq!(window.find("kit-row-value").value(), Some("1"));
            window.click("kit-disabled", cx);
            window.click("kit-loading", cx);
            assert_eq!(window.find("kit-action-count").label(), Some("0 actions"));
            window.click("kit-action", cx);
            assert_eq!(window.find("kit-action-count").label(), Some("1 actions"));
            postman_gpui::ui::theme::apply(ThemeMode::Light, cx);
        })
        .unwrap();
    }
}

#[gpui_kit::test]
async fn theme_switch_updates_an_open_dialog_without_losing_its_value_or_focus_return(
    cx: &mut TestAppContext,
) {
    init(cx);
    let (handle, _) = cx
        .update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| KitSmokeView::new(window, cx))
            })
        })
        .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.click("kit-smoke-input", cx);
        window.input("Draft 世界 🦀", cx);
        window.press("tab", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(1), |window, _| {
        window.try_find("dialog").is_some()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        window.click("kit-dialog-theme", cx);
        assert_eq!(Theme::global(cx).mode, ThemeMode::Dark);
        assert_eq!(
            window.within("dialog").find("kit-smoke-value").label(),
            Some("Draft 世界 🦀")
        );
        window.press("escape", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(1), |window, _| {
        window.try_find("dialog").is_none()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(window.find("kit-smoke-open").focused(), Some(true));
        assert_eq!(
            window.find("kit-smoke-input").value(),
            Some("Draft 世界 🦀")
        );
        window.click("kit-theme", cx);
        assert_eq!(Theme::global(cx).mode, ThemeMode::Light);
    })
    .unwrap();
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

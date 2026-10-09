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

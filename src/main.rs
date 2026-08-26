#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use gpui::{
    actions, px, size, App, AppContext, Bounds, KeyBinding, Menu, MenuItem, WindowBounds,
    WindowOptions,
};
use postman_gpui::{
    app::PostmanApp,
    assets::fonts::{
        load_embedded_fonts, runtime_asset_application, schedule_runtime_asset_exit,
        verify_embedded_fonts,
    },
};

// 定义退出动作
actions!(postman, [Quit]);

fn quit(_: &Quit, cx: &mut App) {
    tracing::info!("application exiting");
    cx.quit();
}

fn main() {
    let verify_runtime_assets = std::env::args_os()
        .any(|argument| argument == std::ffi::OsStr::new("--verify-runtime-assets"));

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("postman_gpui=info")),
        )
        .with_target(true)
        .init();

    let application = if verify_runtime_assets {
        runtime_asset_application()
    } else {
        gpui_platform::application()
    };

    application.run(move |cx: &mut App| {
        load_embedded_fonts(cx).expect("failed to load embedded application fonts");
        if verify_runtime_assets {
            verify_embedded_fonts(cx).expect("failed to verify embedded application fonts");
            tracing::info!("embedded runtime assets verified");
            schedule_runtime_asset_exit(cx);
            return;
        }

        // 激活应用（使菜单栏在前台显示）
        cx.activate(true);

        // 注册退出动作处理函数
        cx.on_action(quit);

        // 绑定快捷键 Cmd-Q (macOS) / Ctrl-Q (其他平台)
        #[cfg(target_os = "macos")]
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        #[cfg(not(target_os = "macos"))]
        cx.bind_keys([KeyBinding::new("ctrl-q", Quit, None)]);

        // 设置应用菜单
        cx.set_menus([Menu::new("Postman GPUI").items([
            MenuItem::action("About Postman GPUI", Quit), // 可以后续替换为 About 动作
            MenuItem::separator(),
            #[cfg(target_os = "macos")]
            MenuItem::action("Hide Postman GPUI", Quit), // 可以后续替换为 Hide 动作
            #[cfg(target_os = "macos")]
            MenuItem::separator(),
            MenuItem::action("Quit Postman GPUI", Quit),
        ])]);

        let bounds = Bounds::centered(None, size(px(1480.), px(980.0)), cx);
        let option = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        };

        cx.open_window(option, |_window, cx| cx.new(PostmanApp::new))
            .expect("failed to open window");
    });
}

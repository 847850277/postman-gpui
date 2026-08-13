use gpui::{
    actions, px, size, App, AppContext, Bounds, KeyBinding, Menu, MenuItem, WindowBounds,
    WindowOptions,
};
use postman_gpui::app::PostmanApp;

// 定义退出动作
actions!(postman, [Quit]);

/// 处理退出应用的函数
fn quit(_: &Quit, cx: &mut App) {
    tracing::info!("🚪 Postman GPUI - 应用正在退出...");
    cx.quit();
}

fn main() {
    // 初始化 tracing
    tracing_subscriber::fmt()
        .with_env_filter("postman_gpui=debug")
        .with_target(false)
        .with_thread_ids(true)
        .with_line_number(true)
        .init();

    gpui_platform::application().run(|cx: &mut App| {
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

        let bounds = Bounds::centered(None, size(px(1600.), px(1200.0)), cx);
        let option = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        };

        cx.open_window(option, |_window, cx| {
            // 创建视图
            let postman_app = PostmanApp::new(cx);
            cx.new(|_| postman_app)
        })
        .expect("failed to open window");
    });
}

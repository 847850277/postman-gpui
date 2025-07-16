use gpui::{px, size, App, AppContext, Application, Bounds, WindowBounds, WindowOptions};
use postman_gpui::app::PostmanApp;

fn main() {
    Application::new().run(|cx: &mut App| {
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

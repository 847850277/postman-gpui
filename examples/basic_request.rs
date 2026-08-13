use gpui::{px, size, App, AppContext, Bounds, WindowBounds, WindowOptions};
use postman_gpui::app::PostmanApp;

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1600.), px(1200.0)), cx);
        let option = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        };

        cx.open_window(option, |_window, cx| cx.new(PostmanApp::new))
            .expect("failed to open window");
    });
}

use gpui::prelude::FluentBuilder;
use gpui::{
    div, rgb, Context, EventEmitter, FocusHandle, Focusable, FontWeight, InteractiveElement,
    IntoElement, ParentElement, Render, Styled, Window,
};
use gpui::{px, size, App, AppContext, Application, Bounds, WindowBounds, WindowOptions};
use postman_gpui::ui::components::url_input::{KeyboardInput, UrlInput, UrlInputEvent};

struct TextInputDemo {
    url_input: gpui::Entity<UrlInput>,
    instructions: Vec<String>,
}

impl TextInputDemo {
    pub fn new(cx: &mut App) -> Self {
        let url_input = cx.new(|cx| UrlInput::new(cx).with_placeholder("点击这里开始输入URL..."));

        let instructions = vec![
            "🎯 文本输入功能演示".to_string(),
            "".to_string(),
            "✨ 支持的功能:".to_string(),
            "• 字符输入 - 直接输入字符".to_string(),
            "• 退格和删除 - Backspace/Delete".to_string(),
            "• 光标移动 - 方向键、Home、End".to_string(),
            "• 文本选择 - Shift + 方向键".to_string(),
            "• 全选 - Ctrl+A".to_string(),
            "• 复制/粘贴/剪切 - Ctrl+C/V/X".to_string(),
            "• 提交 - Enter键".to_string(),
            "• 取消 - Escape键".to_string(),
            "".to_string(),
            "🔥 点击输入框开始体验！".to_string(),
        ];

        Self {
            url_input,
            instructions,
        }
    }

    fn handle_url_event(
        &mut self,
        _url_input: gpui::Entity<UrlInput>,
        event: &UrlInputEvent,
        _cx: &mut Context<Self>,
    ) {
        match event {
            UrlInputEvent::UrlChanged(url) => {
                println!("📝 URL变更: {}", url);
            }
            UrlInputEvent::SubmitRequested => {
                println!("🚀 提交请求!");
            }
        }
    }
}

impl Render for TextInputDemo {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .bg(rgb(0xf5f5f5))
            .size_full()
            .p_8()
            .gap_6()
            .child(
                // 标题
                div()
                    .child("Postman GPUI - 文本输入演示")
                    .text_size(px(24.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(0x333333)),
            )
            .child(
                // 输入框区域
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .p_6()
                    .bg(rgb(0xffffff))
                    .border_1()
                    .border_color(rgb(0xdddddd))
                    .rounded_md()
                    .child(
                        div()
                            .child("URL输入框:")
                            .text_size(px(16.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(0x555555)),
                    )
                    .child(self.url_input.clone()),
            )
            .child(
                // 说明文档
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_6()
                    .bg(rgb(0xffffff))
                    .border_1()
                    .border_color(rgb(0xdddddd))
                    .rounded_md()
                    .children(self.instructions.iter().map(|instruction| {
                        div()
                            .child(instruction.clone())
                            .text_size(px(14.0))
                            .text_color(if instruction.starts_with("🎯") {
                                rgb(0x007acc)
                            } else if instruction.starts_with("✨") {
                                rgb(0x28a745)
                            } else if instruction.starts_with("🔥") {
                                rgb(0xdc3545)
                            } else {
                                rgb(0x666666)
                            })
                            .when(instruction.starts_with("🎯"), |this| {
                                this.font_weight(FontWeight::BOLD)
                            })
                    })),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(800.), px(600.0)), cx);
        let option = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        };

        cx.open_window(option, |_window, cx| cx.new(|cx| TextInputDemo::new(cx)))
            .expect("failed to open window");
    });
}

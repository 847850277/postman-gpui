use gpui::{
    anchored, deferred, div, prelude::FluentBuilder, px, rgb, size, App, AppContext, Application, ClickEvent, Context, Element, InteractiveElement,
    IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled, Window, WindowBounds,
    WindowOptions,
};

#[derive(Clone)]
pub enum TooltipEvent {
    Show,
    Hide,
}

pub struct TooltipButton {
    label: String,
    tooltip_text: String,
    show_tooltip: bool,
    button_bounds: gpui::Bounds<gpui::Pixels>,
}

impl TooltipButton {
    pub fn new(label: impl Into<String>, tooltip_text: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            tooltip_text: tooltip_text.into(),
            show_tooltip: false,
            button_bounds: gpui::Bounds::default(),
        }
    }

    fn toggle_tooltip(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.show_tooltip = !self.show_tooltip;
        cx.notify();
    }

    fn hide_tooltip(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.show_tooltip = false;
        cx.notify();
    }

    fn render_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().clone();

        div()
            .id("tooltip-button")
            .relative()
            .px_4()
            .py_2()
            .bg(rgb(0x007bff))
            .text_color(rgb(0xffffff))
            .rounded_md()
            .cursor_pointer()
            .hover(|style| style.bg(rgb(0x0056b3)))
            .on_click(cx.listener(Self::toggle_tooltip))
            .child(self.label.clone())
            .child(
                // 使用 canvas 获取按钮的精确位置
                gpui::canvas(
                    move |bounds, _, cx| {
                        view.update(cx, |tooltip, _| tooltip.button_bounds = bounds)
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
    }

    fn render_tooltip(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let bounds = self.button_bounds;

        // 关键：使用 deferred + anchored 组合
        deferred(
            anchored()
                .snap_to_window_with_margin(px(8.)) // 确保 tooltip 不会超出窗口边界
                .child(
                    div()
                        .absolute()
                        .top(bounds.bottom() + px(8.)) // 在按钮下方 8px 处显示
                        .left(bounds.left())
                        .min_w_24()
                        .max_w_64()
                        .px_3()
                        .py_2()
                        .bg(rgb(0x333333))
                        .text_color(rgb(0xffffff))
                        .text_size(px(12.))
                        .rounded_md()
                        .shadow_lg()
                        .child(self.tooltip_text.clone())
                        // 添加小三角形指向按钮
                        .child(
                            div()
                                .absolute()
                                .top(px(-4.))
                                .left_4()
                                .w_0()
                                .h_0()
                                .border_4()
                                .border_color(rgb(0x00000000)), // 透明边框
                        ),
                ),
        )
        .with_priority(100) // 设置高渲染优先级，确保显示在最顶层
    }
}

impl Render for TooltipButton {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .child(self.render_button(cx))
            .when(self.show_tooltip, |this| {
                this.child(self.render_tooltip(cx))
            })
    }
}

pub struct DeferredAnchoredExample {
    buttons: Vec<gpui::Entity<TooltipButton>>,
}

impl DeferredAnchoredExample {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let buttons = vec![
            cx.new(|_cx| TooltipButton::new("Button 1", "This is tooltip for button 1")),
            cx.new(|_cx| {
                TooltipButton::new(
                    "Button 2",
                    "This is a longer tooltip text for button 2 that might wrap to multiple lines",
                )
            }),
            cx.new(|_cx| TooltipButton::new("Button 3", "Short tip")),
        ];

        Self { buttons }
    }
}

impl Render for DeferredAnchoredExample {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .bg(rgb(0xf0f0f0))
            .size_full()
            .p_8()
            .gap_4()
            .child(
                div()
                    .child("Deferred + Anchored Example")
                    .text_size(px(24.))
                    .font_weight(gpui::FontWeight::BOLD)
                    .mb_4()
            )
            .child(
                div()
                    .child("Click buttons to show tooltips that are rendered using deferred + anchored:")
                    .text_size(px(14.))
                    .text_color(rgb(0x666666))
                    .mb_4()
            )
            .child(
                div()
                    .flex()
                    .gap_4()
                    .flex_wrap()
                    .children(self.buttons.clone())
            )
            .child(
                div()
                    .mt_8()
                    .p_4()
                    .bg(rgb(0xffffff))
                    .rounded_lg()
                    .shadow_sm()
                    .child(
                        div()
                            .child("Key Features:")
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .mb_2()
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child("• deferred(): 延迟渲染，确保在所有其他元素之后渲染")
                            .child("• anchored(): 创建一个锚定到窗口的定位容器")
                            .child("• snap_to_window_with_margin(): 确保元素不会超出窗口边界")
                            .child("• with_priority(): 设置渲染优先级，控制层级")
                            .child("• canvas(): 获取元素的精确位置信息")
                    )
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = gpui::Bounds::centered(None, size(px(800.), px(600.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        };

        cx.open_window(options, |_window, cx| cx.new(DeferredAnchoredExample::new))
            .expect("Failed to open window");
    });
}

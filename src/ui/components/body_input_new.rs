use gpui::{
    div, prelude::*, px, rgb, Context, FocusHandle, Focusable, 
    IntoElement, MouseButton, MouseDownEvent, ParentElement, Render, 
    SharedString, Styled, Window, EventEmitter, App,
};

pub struct BodyInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
}

impl BodyInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: "".into(),
            placeholder: "Enter request body (JSON, form data, etc.)...".into(),
        }
    }

    pub fn with_placeholder(mut self, placeholder: &str) -> Self {
        self.placeholder = placeholder.to_string().into();
        self
    }

    pub fn set_content(&mut self, content: String, cx: &mut Context<Self>) {
        self.content = content.into();
        cx.notify();
    }

    pub fn get_content(&self) -> &str {
        &self.content
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.content = "".into();
        cx.notify();
    }

    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    // 处理鼠标点击
    fn handle_click(&mut self, _event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
    }
}

impl Focusable for BodyInput {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<()> for BodyInput {}

impl Render for BodyInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_focused = self.focus_handle.is_focused(window);

        div()
            .id("body-input")
            .w_full()
            .min_h_32()
            .max_h_96()
            .px_3()
            .py_2()
            .bg(rgb(0xffffff))
            .border_1()
            .border_color(if is_focused {
                rgb(0x007acc)
            } else {
                rgb(0xcccccc)
            })
            .rounded_md()
            .font_family("monospace")
            .text_size(px(13.0))
            .overflow_y_scroll()
            .when(is_focused, |div| {
                div.shadow_md()
                    .border_color(rgb(0x007acc))
            })
            .child(
                div()
                    .relative()
                    .min_h_6()
                    .child(if self.content.is_empty() && !is_focused {
                        div()
                            .text_color(rgb(0x999999))
                            .child(self.placeholder.to_string())
                    } else {
                        div()
                            .text_color(rgb(0x333333))
                            .children(if self.content.is_empty() {
                                vec![div().child(" ")]
                            } else {
                                // 保持换行和空格的显示
                                self.content.lines().map(|line| {
                                    div().child(if line.is_empty() { " " } else { line })
                                }).collect::<Vec<_>>()
                            })
                    })
                    .when(is_focused, |div| {
                        div.child(
                            div()
                                .absolute()
                                .w_px()
                                .h_4()
                                .bg(rgb(0x333333))
                                .opacity(0.8)
                        )
                    })
            )
            .on_mouse_down(MouseButton::Left, cx.listener(Self::handle_click))
            .focusable()
    }
}

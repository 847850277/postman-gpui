use gpui::{
    div, px, rgb, Context, EventEmitter, FocusHandle, Focusable, 
    InteractiveElement, IntoElement, KeyBinding, ParentElement, 
    Render, SharedString, Styled, Window, App
};

pub struct TextInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
}

#[derive(Clone)]
pub enum TextInputEvent {
    Input(String),
    Blur,
    Focus,
}

impl TextInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: "".into(),
            placeholder: "".into(),
        }
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn with_text(mut self, text: impl Into<SharedString>) -> Self {
        self.content = text.into();
        self
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    pub fn set_text(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.content = text.into();
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.content = "".into();
        cx.notify();
    }
}

impl EventEmitter<TextInputEvent> for TextInput {}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TextInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = self.content.clone();
        let placeholder = self.placeholder.clone();
        
        div()
            .w_full()
            .px_3()
            .py_2()
            .bg(rgb(0x00ff_ffff))
            .border_1()
            .border_color(rgb(0x00cc_cccc))
            .rounded_md()
            .focus(|style| style.border_color(rgb(0x0000_7acc)))
            .hover(|style| style.border_color(rgb(0x0099_9999)))
            .focusable()
            .child(
                div()
                    .text_size(px(14.0))
                    .when(content.is_empty(), |div| {
                        div.text_color(rgb(0x006c_757d)).child(placeholder.clone())
                    })
                    .when(!content.is_empty(), |div| {
                        div.text_color(rgb(0x0021_2529)).child(content.clone())
                    })
            )
            .on_focus(cx.listener(|this, _event, _window, cx| {
                cx.emit(TextInputEvent::Focus);
            }))
            .on_blur(cx.listener(|this, _event, _window, cx| {
                cx.emit(TextInputEvent::Blur);
            }))
    }
}

use gpui::{div, App, Context, IntoElement, ParentElement, Render, RenderOnce, Window};

use crate::ui::components::{
    body_editor::BodyEditor, headers_editor::HeadersEditor, method_selector::MethodSelector,
    url_input::UrlInput,
};

#[derive(IntoElement)]
pub struct RequestPanel {
    //method_selector: MethodSelector,
    //url_input: UrlInput,
    headers_editor: HeadersEditor,
    body_editor: BodyEditor,
}

impl RequestPanel {
    pub fn new() -> Self {
        Self {
            //method_selector: MethodSelector::new("GET"),
            //url_input: UrlInput::new(),
            headers_editor: HeadersEditor::new(),
            body_editor: BodyEditor::new(),
        }
    }

    pub fn send_request(&self) {
        println!("Sending request...");
        // Handle request sending logic here
    }
}

impl Render for RequestPanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
        // .child(
        //     div()
        //         .child(self.method_selector.render(_window, _cx))
        //         .child(self.url_input.render(_window, _cx))
        // )
        //.child(self.headers_editor.render(cx))
        //.child(self.body_editor.render(cx))
        //.child(div().child("Text left"))
    }
}

impl RenderOnce for RequestPanel {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        div().child(div().child("Text left"))
    }
}

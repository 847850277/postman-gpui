use gpui::{div, AppContext, Context, IntoElement, ParentElement, Render, Window};

pub struct BodyEditor {
    body_content: String,
}

impl BodyEditor {
    pub fn new() -> Self {
        Self {
            body_content: String::new(),
        }
    }
}

impl Render for BodyEditor {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child(div().child("Request Body:").child(div().child(
            if self.body_content.is_empty() {
                "Enter request body here...".to_string()
            } else {
                self.body_content.clone()
            },
        )))
    }
}

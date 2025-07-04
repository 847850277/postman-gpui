use gpui::{div, AppContext, Context, IntoElement, ParentElement, Render, Window};

pub struct HeadersEditor {
    headers: Vec<(String, String)>,
}

impl HeadersEditor {
    pub fn new() -> Self {
        Self {
            headers: Vec::new(),
        }
    }
}

impl Render for HeadersEditor {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child(
            div()
                .child("Headers:")
                .children(self.headers.iter().enumerate().map(|(i, (key, value))| {
                    div()
                        .child(format!(
                            "Key: {}",
                            if key.is_empty() { "Enter key..." } else { key }
                        ))
                        .child(format!(
                            "Value: {}",
                            if value.is_empty() {
                                "Enter value..."
                            } else {
                                value
                            }
                        ))
                        .child(format!("Remove Header {}", i + 1))
                }))
                .child("Add Header"),
        )
    }
}

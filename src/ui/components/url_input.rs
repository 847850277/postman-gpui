use gpui::{div, AppContext, Context, IntoElement, Render, Window};

pub struct UrlInput {
    url: String,
}

impl UrlInput {
    pub fn new() -> Self {
        Self { url: String::new() }
    }

    pub fn get_url(&self) -> &str {
        &self.url
    }

    pub fn submit_url(&self) {
        // Handle the URL submission
        println!("Submitted URL: {}", self.url);
    }
}

impl Render for UrlInput {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
        // .child(
        //     div()
        //         .child(if self.url.is_empty() {
        //             "Enter request URL".to_string()
        //         } else {
        //             self.url.clone()
        //         })
        // )
    }
}

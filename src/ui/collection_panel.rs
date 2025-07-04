use gpui::{div, Context, IntoElement, ParentElement, Render, Window};

pub struct CollectionPanel {
    // Add fields as necessary for managing collections
    collections: Vec<String>,
}

impl CollectionPanel {
    pub fn new() -> Self {
        Self {
            collections: vec!["Collection 1".to_string(), "Collection 2".to_string()],
        }
    }

    // pub fn add_collection(&mut self, name: String, cx: &mut ViewContext<Self>) {
    //     self.collections.push(name);
    //     cx.notify();
    // }
}

impl Render for CollectionPanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child("Collections").children(
            self.collections
                .iter()
                .map(|collection| div().child(collection.clone())),
        )
    }
}

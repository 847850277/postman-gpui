use gpui::{div, Context, IntoElement, ParentElement, Render, Window};

pub struct MethodSelector {
    selected_method: String,
    methods: Vec<String>,
}

impl MethodSelector {
    pub fn new() -> Self {
        let methods = vec![
            "GET".to_string(),
            "POST".to_string(),
            "PUT".to_string(),
            "DELETE".to_string(),
        ];
        Self {
            selected_method: methods[0].clone(),
            methods,
        }
    }
}

impl Render for MethodSelector {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child(
            div().child(format!("Method: {}", self.selected_method)), // Note: GPUI doesn't have a built-in Dropdown component
        )
    }
}

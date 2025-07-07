use crate::{http::client::HttpClient, ui::components::method_selector::MethodSelector};
use gpui::{
    div, px, rgb, App, AppContext, Bounds, Context, Element, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement, Render, Styled, Window, WindowOptions
};

pub struct PostmanApp {
    // HTTP Method
    selected_method: String,
    methods: Vec<String>,

    my_method_selector: Entity<MethodSelector>,

    // URL Input
    url: String,

    // Headers
    headers: Vec<(String, String)>,

    // Body
    body_content: String,

    // HTTP Client
    http_client: HttpClient,

    // Response (optional)
    response_body: Option<String>,
    response_status: Option<u16>,

}

impl PostmanApp {
    pub fn new(cx: &mut App) -> Self {
        let methods = vec![
            "GET".to_string(),
            "POST".to_string(),
            "PUT".to_string(),
            "DELETE".to_string(),
        ];

        PostmanApp {
            selected_method: methods[0].clone(),
            methods,
            my_method_selector: cx.new(|cx| MethodSelector::new()),
            url: String::new(),
            headers: Vec::new(),
            body_content: String::new(),
            http_client: HttpClient::new(),
            response_body: None,
            response_status: None,
        }
    }

    fn render_method_selector(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .relative()
            .child(
                div()
                    .child(self.selected_method.clone())
                    .bg(rgb(0xe0e0e0))
                    .px_4()
                    .py_2()
                    .border_1()
                    .border_color(rgb(0xcccccc))
                    .cursor_pointer()
            )
            // .child(
            //     div()
            //         .absolute()
            //         .top_full()
            //         .left_0()
            //         .bg(rgb(0xffffff))
            //         .border_1()
            //         .border_color(rgb(0xcccccc))
            //         .children(
            //             self.methods.iter().map(|method| {
            //                 div()
            //                     .child(method.clone())
            //                     .px_4()
            //                     .py_2()
            //                     .cursor_pointer()
            //                     .hover(|style| style.bg(rgb(0xf0f0f0)))
            //             })
            //         )
            // )
    }

    fn render_url_input(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_1()
            .px_4()
            .py_2()
            .bg(rgb(0xffffff))
            .border_1()
            .border_color(rgb(0xcccccc))
            .child("Enter URL...")
    }

    fn render_headers_editor(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .child("Headers")
                    .text_size(px(16.0))
                    .font_weight(FontWeight::MEDIUM)
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .px_3()
                            .py_2()
                            .bg(rgb(0xffffff))
                            .border_1()
                            .border_color(rgb(0xcccccc))
                            .child("Key")
                    )
                    .child(
                        div()
                            .flex_1()
                            .px_3()
                            .py_2()
                            .bg(rgb(0xffffff))
                            .border_1()
                            .border_color(rgb(0xcccccc))
                            .child("Value")
                    )
                    .child(
                        div()
                            .child("Add")
                            .bg(rgb(0x28a745))
                            .text_color(rgb(0xffffff))
                            .px_3()
                            .py_2()
                    )
            )
    }

    fn render_body_editor(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .child("Request Body")
                    .text_size(px(16.0))
                    .font_weight(FontWeight::MEDIUM)
            )
            .child(
                div()
                    .w_full()
                    .h_32()
                    .px_3()
                    .py_2()
                    .bg(rgb(0xffffff))
                    .border_1()
                    .border_color(rgb(0xcccccc))
                    .child("Enter request body...")
            )
    }

    fn render_response_panel(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .child("Response")
                    .text_size(px(16.0))
                    .font_weight(FontWeight::MEDIUM)
            )
            .child(
                match (&self.response_status, &self.response_body) {
                    (Some(status), Some(body)) => {
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .child(format!("Status: {}", status))
                                    .text_color(if *status < 400 { rgb(0x28a745) } else { rgb(0xdc3545) })
                                    .font_weight(FontWeight::MEDIUM)
                            )
                            .child(
                                div()
                                    .w_full()
                                    .h_32()
                                    .px_3()
                                    .py_2()
                                    .bg(rgb(0xf8f9fa))
                                    .border_1()
                                    .border_color(rgb(0xcccccc))
                                    .child(body.clone())
                            )
                    }
                    _ => {
                        div()
                            .w_full()
                            .h_32()
                            .px_3()
                            .py_2()
                            .bg(rgb(0xf8f9fa))
                            .border_1()
                            .border_color(rgb(0xcccccc))
                            .child("No response yet...")
                    }
                }
            )
    }
}

impl Render for PostmanApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .bg(rgb(0xf0f0f0))
            .size_full()
            .p_4()
            .gap_4()
            .child(
                // Header
                div()
                    .child("Postman GPUI")
                    .text_size(px(24.0))
                    .font_weight(FontWeight::BOLD),
            )
            .child(
                // Request Panel
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .p_4()
                    .bg(rgb(0xffffff))
                    .border_1()
                    .border_color(rgb(0xcccccc))
                    .child(
                        // Method and URL row
                        div()
                            .flex()
                            .gap_4()
                            //.child(self.render_method_selector(cx))
                            .child(
                                self.my_method_selector.clone()
                            )
                            .child(self.render_url_input(cx))
                            .child(
                                div()
                                    .child("Send")
                                    .bg(rgb(0x007acc))
                                    .text_color(rgb(0xffffff))
                                    .px_4()
                                    .py_2(),
                            ),
                    ) 
                    .child(self.render_headers_editor(cx))
                    .child(self.render_body_editor(cx))
            )
            .child(
                // Response Panel
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .p_4()
                    .bg(rgb(0xffffff))
                    .border_1()
                    .border_color(rgb(0xcccccc)) 
                    .child(self.render_response_panel(cx))
            )
    }
}

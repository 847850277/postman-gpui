use crate::{
    http::client::HttpClient,
    ui::components::{
        method_selector::{MethodSelector, MethodSelectorEvent},
        url_input::{UrlInput, UrlInputEvent},
    },
};
use gpui::{
    div, px, rgb, App, AppContext, Bounds, Context, Element, Entity, FontWeight,
    InteractiveElement, IntoElement, ParentElement, Render, Styled, Subscription, Window,
    WindowOptions,
};

pub struct PostmanApp {
    method_selector: Entity<MethodSelector>,
    url_input: Entity<UrlInput>, // 添加 url_input 组件
    //_method_subscription: Subscription,
    //_url_subscription: Subscription, // 添加 URL 订阅

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
        let method_selector = cx.new(MethodSelector::new);
        //let url_input = cx.new(|cx| UrlInput::new(cx).with_placeholder("Enter request URL..."));

        let url_input = cx.new(|cx| UrlInput::new(cx).with_placeholder("Enter request URL..."));

        // 订阅事件
        //let method_subscription = cx.subscribe(&method_selector, Self::on_method_changed);
        //let url_subscription = cx.subscribe(&url_input, Self::on_url_changed);

        PostmanApp {
            method_selector,
            url_input,
            //_method_subscription: method_subscription,
            //_url_subscription: url_subscription,
            headers: Vec::new(),
            body_content: String::new(),
            http_client: HttpClient::new(),
            response_body: None,
            response_status: None,
        }
    }

    // 处理方法变更事件
    pub fn on_method_changed(
        &mut self,
        _method_selector: Entity<MethodSelector>,
        event: &MethodSelectorEvent,
        _cx: &mut Context<Self>,
    ) {
        match event {
            MethodSelectorEvent::MethodChanged(method) => {
                println!("🎯 PostmanApp - HTTP方法变更为: {}", method);
                // 可以根据方法类型调整UI
            }
        }
    }

    // 处理URL变更事件
    pub fn on_url_changed(
        &mut self,
        _url_input: Entity<UrlInput>,
        event: &UrlInputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            UrlInputEvent::UrlChanged(url) => {
                println!("🌐 PostmanApp - URL变更为: {}", url);
            }
            UrlInputEvent::SubmitRequested => {
                println!("🚀 PostmanApp - 请求提交");
                self.send_request(cx);
            }
        }
    }

    // 发送请求
    fn send_request(&mut self, cx: &mut Context<Self>) {
        // let method = self.method_selector.read(cx).selected_method();
        // let url = self.url_input.read(cx).get_url().to_string();

        //println!("发送请求: {} {}", method, url);
        println!("🚀 PostmanApp - 发送请求");

        // 这里添加实际的HTTP请求逻辑
        // self.http_client.send_request(method, url, headers, body)
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
                    .font_weight(FontWeight::MEDIUM),
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
                            .child("Key"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .px_3()
                            .py_2()
                            .bg(rgb(0xffffff))
                            .border_1()
                            .border_color(rgb(0xcccccc))
                            .child("Value"),
                    )
                    .child(
                        div()
                            .child("Add")
                            .bg(rgb(0x28a745))
                            .text_color(rgb(0xffffff))
                            .px_3()
                            .py_2(),
                    ),
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
                    .font_weight(FontWeight::MEDIUM),
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
                    .child("Enter request body..."),
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
                    .font_weight(FontWeight::MEDIUM),
            )
            .child(match (&self.response_status, &self.response_body) {
                (Some(status), Some(body)) => div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .child(format!("Status: {}", status))
                            .text_color(if *status < 400 {
                                rgb(0x28a745)
                            } else {
                                rgb(0xdc3545)
                            })
                            .font_weight(FontWeight::MEDIUM),
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
                            .child(body.clone()),
                    ),
                _ => div()
                    .w_full()
                    .h_32()
                    .px_3()
                    .py_2()
                    .bg(rgb(0xf8f9fa))
                    .border_1()
                    .border_color(rgb(0xcccccc))
                    .child("No response yet..."),
            })
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
                            .child(self.method_selector.clone())
                            .child(self.url_input.clone()) // 使用 UrlInput 组件替代 render_url_input
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
                    .child(self.render_body_editor(cx)),
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
                    .child(self.render_response_panel(cx)),
            )
    }
}

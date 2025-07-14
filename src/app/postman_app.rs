use crate::{
    http::client::HttpClient,
    ui::components::{
        method_selector::{MethodSelector, MethodSelectorEvent},
        url_input::{UrlInput, UrlInputEvent, setup_url_input_key_bindings},
    },
};
use gpui::{
    div, px, rgb, App, AppContext, Context, Entity, FontWeight,
    IntoElement, ParentElement, Render, Styled, Window,
    InteractiveElement,
};

pub struct PostmanApp {
    method_selector: Entity<MethodSelector>,
    url_input: Entity<UrlInput>,

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
        // 设置键盘绑定 - 在创建组件之前
        cx.bind_keys(setup_url_input_key_bindings());

        let method_selector = cx.new(MethodSelector::new);
        let url_input = cx.new(|cx| UrlInput::new(cx).with_placeholder("Enter request URL..."));

        PostmanApp {
            method_selector,
            url_input,
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
        event: &MethodSelectorEvent,
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
        event: &UrlInputEvent,
    ) {
        match event {
            UrlInputEvent::UrlChanged(url) => {
                println!("🌐 PostmanApp - URL变更为: {}", url);
            }
            UrlInputEvent::SubmitRequested => {
                println!("🚀 PostmanApp - 请求提交");
                // 注意：这里我们需要重新构造 Context，暂时简化处理
                println!("🚀 PostmanApp - 发送请求");
            }
        }
    }

    // 发送请求
    fn send_request(&mut self, cx: &mut Context<Self>) {
        let method = self.method_selector.update(cx, |selector, cx| selector.selected_method(cx));
        let url = self.url_input.read(cx).get_url().to_string();

        println!("🚀 PostmanApp - 发送请求: {} {}", method, url);

        // 这里添加实际的HTTP请求逻辑
        // self.http_client.send_request(method, url, headers, body)
        
        // 模拟响应
        self.response_status = Some(200);
        self.response_body = Some(format!("Response for {} request to {}", method, url));
        cx.notify();
    }

    // 处理 Send 按钮点击
    fn on_send_clicked(&mut self, _event: &gpui::MouseUpEvent, _window: &mut gpui::Window, cx: &mut Context<Self>) {
        self.send_request(cx);
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
                                    .py_2()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|style| style.bg(rgb(0x0056b3)))
                                    .on_mouse_up(gpui::MouseButton::Left, cx.listener(Self::on_send_clicked)),
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

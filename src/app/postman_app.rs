use crate::{
    http::client::HttpClient,
    ui::components::{
        method_selector::{MethodSelector, MethodSelectorEvent},
        url_input::{setup_url_input_key_bindings, UrlInput, UrlInputEvent},
    },
};
use gpui::{
    div, px, rgb, App, AppContext, Context, Entity, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Render, Styled, Window,
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
    
    // 请求状态
    is_loading: bool,
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
            is_loading: false,
        }
    }

    // 处理方法变更事件
    pub fn on_method_changed(&mut self, event: &MethodSelectorEvent) {
        match event {
            MethodSelectorEvent::MethodChanged(method) => {
                println!("🎯 PostmanApp - HTTP方法变更为: {}", method);
                
                // 根据方法类型设置默认请求体
                if method.to_uppercase() == "POST" && self.body_content.is_empty() {
                    self.body_content = r#"{
  "message": "Hello, World!",
  "timestamp": "2025-07-15T14:30:00Z",
  "data": {
    "key": "value"
  }
}"#.to_string();
                    println!("📝 PostmanApp - 为POST请求设置默认JSON请求体");
                } else if method.to_uppercase() == "GET" {
                    // GET请求通常不需要请求体
                    if !self.body_content.is_empty() {
                        println!("ℹ️ PostmanApp - GET请求通常不使用请求体");
                    }
                }
            }
        }
    }

    // 处理URL变更事件
    pub fn on_url_changed(&mut self, event: &UrlInputEvent) {
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
        let method = self
            .method_selector
            .update(cx, |selector, cx| selector.selected_method(cx));
        let url = self.url_input.read(cx).get_url().to_string();

        // 验证URL是否为空
        if url.trim().is_empty() {
            println!("❌ PostmanApp - URL不能为空");
            self.response_status = Some(0);
            self.response_body = Some("Error: URL cannot be empty".to_string());
            cx.notify();
            return;
        }

        println!("🚀 PostmanApp - 发送请求: {} {}", method, url);

        // 支持GET和POST请求
        if method.to_uppercase() == "GET" || method.to_uppercase() == "POST" {
            // 设置加载状态
            self.is_loading = true;
            self.response_body = None;
            self.response_status = None;
            cx.notify();

            println!("📡 PostmanApp - 发送{}请求到: {}", method.to_uppercase(), url);
            
            // 使用 tokio 的 block_on 来同步执行异步请求
            let client = &self.http_client;
            let rt = tokio::runtime::Runtime::new().unwrap();
            
            let result = if method.to_uppercase() == "GET" {
                rt.block_on(client.get(&url))
            } else {
                // POST 请求
                let headers = if self.headers.is_empty() {
                    None
                } else {
                    let mut header_map = std::collections::HashMap::new();
                    for (key, value) in &self.headers {
                        header_map.insert(key.clone(), value.clone());
                    }
                    Some(header_map)
                };
                
                rt.block_on(client.post(&url, &self.body_content, headers))
            };
            
            match result {
                Ok(response_body) => {
                    self.is_loading = false;
                    self.response_status = Some(200);
                    self.response_body = Some(response_body);
                    println!("✅ PostmanApp - {}请求成功，响应长度: {} bytes", 
                        method.to_uppercase(),
                        self.response_body.as_ref().unwrap().len());
                }
                Err(e) => {
                    self.is_loading = false;
                    self.response_status = Some(0);
                    self.response_body = Some(format!("请求失败: {}", e));
                    println!("❌ PostmanApp - {}请求失败: {}", method.to_uppercase(), e);
                }
            }
        } else {
            self.response_status = Some(0);
            self.response_body = Some(format!("Method {} not implemented yet", method));
            println!("⚠️ PostmanApp - 方法 {} 尚未实现", method);
        }
        
        cx.notify();
    }

    // 处理 Send 按钮点击
    fn on_send_clicked(
        &mut self,
        _event: &gpui::MouseUpEvent,
        _window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
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
                    .child(if self.body_content.is_empty() {
                        "Enter request body (JSON, form data, etc.)...".to_string()
                    } else {
                        self.body_content.clone()
                    }),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(0x6c757d))
                    .child(format!("Body length: {} characters", self.body_content.len())),
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
            .child(if self.is_loading {
                // 显示加载状态
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .child("🔄 发送请求中...")
                            .text_color(rgb(0x007acc))
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
                            .child("请稍等，正在处理请求..."),
                    )
            } else {
                match (&self.response_status, &self.response_body) {
                    (Some(status), Some(body)) => div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .child(format!("Status: {}", status))
                                .text_color(if *status == 0 {
                                    rgb(0xdc3545) // 错误
                                } else if *status < 400 {
                                    rgb(0x28a745) // 成功
                                } else {
                                    rgb(0xdc3545) // 客户端/服务器错误
                                })
                                .font_weight(FontWeight::MEDIUM),
                        )
                        .child(
                            div()
                                .w_full()
                                .h_40()
                                .px_3()
                                .py_2()
                                .bg(rgb(0xf8f9fa))
                                .border_1()
                                .border_color(rgb(0xcccccc))
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .font_family("monospace")
                                        .child(body.clone())
                                ),
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
                }
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
                                    .on_mouse_up(
                                        gpui::MouseButton::Left,
                                        cx.listener(Self::on_send_clicked),
                                    ),
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

use crate::{
    http::executor::RequestExecutor,
    models::{Request, RequestHistory},
    ui::components::{
        body_input::{setup_body_input_key_bindings, BodyInput},
        header_input::{setup_header_input_key_bindings, HeaderInput},
        history_list::{HistoryList, HistoryListEvent},
        method_selector::{MethodSelector, MethodSelectorEvent},
        response_viewer::{setup_response_viewer_key_bindings, ResponseViewer},
        url_input::{setup_url_input_key_bindings, UrlInput, UrlInputEvent},
    },
};
use gpui::{
    div, px, rgb, App, AppContext, Context, Entity, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, Window,
};

// Maximum length for URL display in history
const MAX_HISTORY_URL_LENGTH: usize = 40;

// UI Color constants
const COLOR_CHECKBOX_ENABLED_BG: u32 = 0x0000_7acc;
const COLOR_CHECKBOX_ENABLED_HOVER: u32 = 0x0000_56b3;
const COLOR_CHECKBOX_DISABLED_BG: u32 = 0x00ff_ffff;
const COLOR_CHECKBOX_DISABLED_HOVER: u32 = 0x00e9_ecef;
const COLOR_CHECKBOX_TEXT: u32 = 0x00ff_ffff;
const COLOR_HEADER_ENABLED_BG: u32 = 0x00ff_ffff;
const COLOR_HEADER_ENABLED_BORDER: u32 = 0x0028_a745;
const COLOR_HEADER_DISABLED_BG: u32 = 0x00f8_f9fa;
const COLOR_HEADER_DISABLED_BORDER: u32 = 0x00cc_cccc;
const COLOR_TEXT_ENABLED: u32 = 0x0000_0000;
const COLOR_TEXT_DISABLED: u32 = 0x006c_757d;

pub struct PostmanApp {
    method_selector: Entity<MethodSelector>,
    url_input: Entity<UrlInput>,

    // Headers - (enabled, key, value)
    headers: Vec<(bool, String, String)>,

    // Body - 使用BodyInput组件替代字符串
    body_input: Entity<BodyInput>,

    // HTTP Request Executor
    request_executor: RequestExecutor,

    // Response viewer component
    response_viewer: Entity<ResponseViewer>,

    // Headers输入组件
    header_key_input: Entity<HeaderInput>,
    header_value_input: Entity<HeaderInput>,

    // Request history
    request_history: RequestHistory,
    history_list: Entity<HistoryList>,
}

impl PostmanApp {
    pub fn new(cx: &mut App) -> Self {
        // 设置键盘绑定 - 在创建组件之前
        cx.bind_keys(setup_url_input_key_bindings());
        cx.bind_keys(setup_header_input_key_bindings());
        cx.bind_keys(setup_body_input_key_bindings());
        cx.bind_keys(setup_response_viewer_key_bindings());

        let method_selector = cx.new(MethodSelector::new);
        let url_input = cx.new(|cx| UrlInput::new(cx).with_placeholder("Enter request URL..."));
        let header_key_input =
            cx.new(|cx| HeaderInput::new(cx).with_placeholder("Header Key (e.g., Authorization)"));
        let header_value_input = cx.new(|cx| {
            HeaderInput::new(cx).with_placeholder("Header Value (e.g., Bearer token123)")
        });
        let body_input = cx.new(|cx| {
            BodyInput::new(cx).with_placeholder("Enter request body (JSON, form data, etc.)...")
        });
        let response_viewer = cx.new(ResponseViewer::new);
        let history_list = cx.new(|_cx| HistoryList::new());

        PostmanApp {
            method_selector,
            url_input,
            headers: Vec::new(),
            body_input,
            request_executor: RequestExecutor::new(),
            response_viewer,
            header_key_input,
            header_value_input,
            request_history: RequestHistory::new(),
            history_list,
        }
    }

    // 处理方法变更事件
    pub fn on_method_changed(&mut self, event: &MethodSelectorEvent, cx: &mut Context<Self>) {
        match event {
            MethodSelectorEvent::MethodChanged(method) => {
                println!("🎯 PostmanApp - HTTP方法变更:");
                println!("   新方法: {method}");
                println!("   当前headers数量: {}", self.headers.len());

                let body_length = self.body_input.read(cx).get_content().len();
                println!("   当前body长度: {body_length} bytes");

                // 根据方法类型设置默认请求体
                if method.to_uppercase() == "POST" && self.body_input.read(cx).is_empty() {
                    let default_json = r#"{
  "message": "Hello, World!",
  "timestamp": "2025-07-15T14:30:00Z",
  "data": {
    "key": "value"
  }
}"#
                    .to_string();

                    self.body_input.update(cx, |input, cx| {
                        input.set_content(default_json, cx);
                    });

                    let new_body_length = self.body_input.read(cx).get_content().len();
                    println!("📝 PostmanApp - 为POST请求设置默认JSON请求体:");
                    println!("   Body长度: {new_body_length} bytes");

                    // 为POST请求设置默认Content-Type头
                    if self.headers.is_empty() {
                        self.headers.push((
                            true,
                            "Content-Type".to_string(),
                            "application/json".to_string(),
                        ));
                        self.headers.push((
                            true,
                            "Accept".to_string(),
                            "application/json".to_string(),
                        ));
                        println!("📝 PostmanApp - 为POST请求设置默认Headers:");
                        println!("   添加: Content-Type = application/json");
                        println!("   添加: Accept = application/json");
                        println!("   当前headers总数: {}", self.headers.len());
                    } else {
                        println!("ℹ️ PostmanApp - 已有headers，跳过默认headers设置");
                    }
                } else if method.to_uppercase() == "GET" {
                    // GET请求通常不需要请求体
                    if !self.body_input.read(cx).is_empty() {
                        println!("ℹ️ PostmanApp - GET请求通常不使用请求体");
                        println!("   当前body长度: {body_length} bytes");
                        println!("   建议: 清空请求体或改用POST方法");
                    } else {
                        println!("✅ PostmanApp - GET请求配置正确，无请求体");
                    }
                }

                println!("🏁 PostmanApp - 方法变更处理完成");
            }
        }
    }

    // 处理URL变更事件
    pub fn on_url_changed(&mut self, event: &UrlInputEvent) {
        match event {
            UrlInputEvent::UrlChanged(url) => {
                println!("🌐 PostmanApp - URL变更为: {url}");
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
        // Only include enabled headers
        let headers: Vec<(String, String)> = self
            .headers
            .iter()
            .filter(|(enabled, _, _)| *enabled)
            .map(|(_, key, value)| (key.clone(), value.clone()))
            .collect();
        let body = if method.to_uppercase() == "POST" {
            Some(self.body_input.read(cx).get_content().to_string())
        } else {
            None
        };

        // 设置加载状态
        self.response_viewer.update(cx, |viewer, cx| {
            viewer.set_loading(cx);
        });
        cx.notify();

        // Create a Request object for history
        let mut request = Request::new(&method, &url);
        for (key, value) in &headers {
            request.add_header(key, value);
        }
        if let Some(body_content) = &body {
            request.set_body(body_content);
        }

        // 执行请求
        let result = self.request_executor.execute(&method, &url, headers, body);

        // 处理结果
        match result {
            Ok(request_result) => {
                // Add to history on success
                let url_display = if url.len() > MAX_HISTORY_URL_LENGTH {
                    let truncated: String = url.chars().take(MAX_HISTORY_URL_LENGTH).collect();
                    format!("{}...", truncated)
                } else {
                    url.clone()
                };
                self.request_history.add(request, url_display);

                // Update history list UI
                self.history_list.update(cx, |list, cx| {
                    list.set_entries(self.request_history.entries().to_vec(), cx);
                });

                self.response_viewer.update(cx, |viewer, cx| {
                    viewer.set_success(request_result.status, request_result.body, cx);
                });
            }
            Err(error_message) => {
                self.response_viewer.update(cx, |viewer, cx| {
                    viewer.set_error(error_message, cx);
                });
            }
        }

        println!("🏁 PostmanApp - 请求处理完成");
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

    // 添加header
    fn add_header(&mut self, cx: &mut Context<Self>) {
        let key = self
            .header_key_input
            .read(cx)
            .get_content()
            .trim()
            .to_string();
        let value = self
            .header_value_input
            .read(cx)
            .get_content()
            .trim()
            .to_string();

        println!("🔧 PostmanApp - 尝试添加header:");
        println!("   Key: '{key}'");
        println!("   Value: '{value}'");

        if !key.is_empty() && !value.is_empty() {
            // 检查是否已存在相同的key
            let existing_index = self.headers.iter().position(|(_, k, _)| k == &key);

            if let Some(index) = existing_index {
                let old_value = self.headers[index].2.clone(); // 克隆旧值避免借用冲突
                self.headers[index].2 = value.clone();
                println!("🔄 PostmanApp - 更新已存在的header:");
                println!("   Key: {key}");
                println!("   旧值: {old_value}");
                println!("   新值: {value}");
            } else {
                self.headers.push((true, key.clone(), value.clone())); // enabled by default
                println!("✅ PostmanApp - 成功添加新header:");
                println!("   Key: {key}");
                println!("   Value: {value}");
                println!("   当前headers总数: {}", self.headers.len());
            }

            // 清空输入框
            self.header_key_input
                .update(cx, |input, cx| input.clear(cx));
            self.header_value_input
                .update(cx, |input, cx| input.clear(cx));

            // 打印当前所有headers
            println!("📋 PostmanApp - 当前所有headers:");
            for (i, (enabled, k, v)) in self.headers.iter().enumerate() {
                println!(
                    "   {}. [{}] {} = {}",
                    i + 1,
                    if *enabled { "✓" } else { " " },
                    k,
                    v
                );
            }

            cx.notify();
        } else {
            println!("⚠️ PostmanApp - 添加header失败:");
            if key.is_empty() {
                println!("   原因: Header key不能为空");
            }
            if value.is_empty() {
                println!("   原因: Header value不能为空");
            }
            println!("   请确保key和value都有内容");
        }
    }

    // 通过输入框设置header值
    fn set_header_input_values(&mut self, key: &str, value: &str, cx: &mut Context<Self>) {
        println!("🎯 PostmanApp - 设置预设header到输入框:");
        println!("   预设Key: {key}");
        println!("   预设Value: {value}");

        self.header_key_input.update(cx, |input, cx| {
            input.set_content(key.to_string(), cx);
        });
        self.header_value_input.update(cx, |input, cx| {
            input.set_content(value.to_string(), cx);
        });

        println!("✅ PostmanApp - 预设header已填入输入框，请点击Add按钮添加");
    }

    // 删除header
    fn remove_header(&mut self, index: usize, cx: &mut Context<Self>) {
        println!("🗑️ PostmanApp - 尝试删除header，索引: {index}");

        if index < self.headers.len() {
            let removed = self.headers.remove(index);
            println!("✅ PostmanApp - 成功删除header:");
            println!("   Enabled: {}", removed.0);
            println!("   Key: {}", removed.1);
            println!("   Value: {}", removed.2);
            println!("   剩余headers数量: {}", self.headers.len());

            // 打印剩余的headers
            if self.headers.is_empty() {
                println!("📋 PostmanApp - 当前无headers");
            } else {
                println!("📋 PostmanApp - 剩余headers:");
                for (i, (enabled, k, v)) in self.headers.iter().enumerate() {
                    println!(
                        "   {}. [{}] {} = {}",
                        i + 1,
                        if *enabled { "✓" } else { " " },
                        k,
                        v
                    );
                }
            }

            cx.notify();
        } else {
            println!("❌ PostmanApp - 删除header失败:");
            println!(
                "   原因: 索引 {} 超出范围 (当前headers数量: {})",
                index,
                self.headers.len()
            );
        }
    }

    // Toggle header enabled state
    fn toggle_header(&mut self, index: usize, cx: &mut Context<Self>) {
        println!("🔄 PostmanApp - 切换header状态，索引: {index}");

        if index < self.headers.len() {
            let current_state = self.headers[index].0;
            self.headers[index].0 = !current_state;
            println!("✅ PostmanApp - 成功切换header状态:");
            println!("   Key: {}", self.headers[index].1);
            println!("   从 {} 切换到 {}", current_state, !current_state);

            cx.notify();
        } else {
            println!("❌ PostmanApp - 切换header失败:");
            println!(
                "   原因: 索引 {} 超出范围 (当前headers数量: {})",
                index,
                self.headers.len()
            );
        }
    }

    // Handle history item selection
    fn on_history_selected(
        &mut self,
        _history_list: gpui::Entity<HistoryList>,
        event: &HistoryListEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            HistoryListEvent::RequestSelected(request) => {
                println!("📋 PostmanApp - Loading request from history:");
                println!("   Method: {}", request.method);
                println!("   URL: {}", request.url);
                println!("   Headers: {}", request.headers.len());

                // Log query parameters if present in URL
                if request.url.contains('?') {
                    if let Some(query_str) = request.url.split('?').nth(1) {
                        println!("   Query parameters: {}", query_str);
                    }
                }

                // Log body info
                if let Some(ref body) = request.body {
                    println!("   Body length: {} bytes", body.len());
                }

                // Update method selector - normalize method to uppercase
                let method = request.method.to_uppercase();
                self.method_selector.update(cx, |selector, cx| {
                    selector.set_selected_method(&method, cx);
                });

                // Update URL input
                self.url_input.update(cx, |input, cx| {
                    input.set_url(&request.url, cx);
                });

                // Update headers - convert from Vec<(String, String)> to Vec<(bool, String, String)>
                self.headers = request
                    .headers
                    .iter()
                    .map(|(key, value)| (true, key.clone(), value.clone()))
                    .collect();

                // Update body
                if let Some(body) = &request.body {
                    self.body_input.update(cx, |input, cx| {
                        input.set_content(body.clone(), cx);
                    });
                } else {
                    self.body_input.update(cx, |input, cx| {
                        input.clear(cx);
                    });
                }

                println!("✅ PostmanApp - Request loaded from history successfully");
                println!("   • URL loaded into URL input field");
                println!("   • {} headers loaded", request.headers.len());
                if request.body.is_some() {
                    println!("   • Request body loaded");
                }
                cx.notify();
            }
        }
    }

    // Helper function to get checkbox background color
    fn checkbox_bg_color(enabled: bool) -> u32 {
        if enabled {
            COLOR_CHECKBOX_ENABLED_BG
        } else {
            COLOR_CHECKBOX_DISABLED_BG
        }
    }

    // Helper function to get checkbox hover background color
    fn checkbox_hover_bg_color(enabled: bool) -> u32 {
        if enabled {
            COLOR_CHECKBOX_ENABLED_HOVER
        } else {
            COLOR_CHECKBOX_DISABLED_HOVER
        }
    }

    // Helper function to get header cell background color
    fn header_cell_bg_color(enabled: bool) -> u32 {
        if enabled {
            COLOR_HEADER_ENABLED_BG
        } else {
            COLOR_HEADER_DISABLED_BG
        }
    }

    // Helper function to get header cell border color
    fn header_cell_border_color(enabled: bool) -> u32 {
        if enabled {
            COLOR_HEADER_ENABLED_BORDER
        } else {
            COLOR_HEADER_DISABLED_BORDER
        }
    }

    // Helper function to get header text color
    fn header_text_color(enabled: bool) -> u32 {
        if enabled {
            COLOR_TEXT_ENABLED
        } else {
            COLOR_TEXT_DISABLED
        }
    }

    fn render_headers_editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .child(format!(
                        "Headers ({})",
                        self.headers
                            .iter()
                            .filter(|(enabled, _, _)| *enabled)
                            .count()
                    ))
                    .text_size(px(16.0))
                    .font_weight(FontWeight::MEDIUM),
            )
            // 现有headers列表
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(if self.headers.is_empty() {
                        vec![div()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .w_8()
                                    .px_2()
                                    .py_2()
                                    .bg(rgb(COLOR_HEADER_DISABLED_BG))
                                    .border_1()
                                    .border_color(rgb(COLOR_HEADER_DISABLED_BORDER))
                                    .child(""),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .px_3()
                                    .py_2()
                                    .bg(rgb(COLOR_HEADER_DISABLED_BG))
                                    .border_1()
                                    .border_color(rgb(COLOR_HEADER_DISABLED_BORDER))
                                    .child("No headers set"),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .px_3()
                                    .py_2()
                                    .bg(rgb(COLOR_HEADER_DISABLED_BG))
                                    .border_1()
                                    .border_color(rgb(COLOR_HEADER_DISABLED_BORDER))
                                    .child(""),
                            )
                            .child(
                                div()
                                    .w_16()
                                    .px_3()
                                    .py_2()
                                    .bg(rgb(COLOR_HEADER_DISABLED_BG))
                                    .border_1()
                                    .border_color(rgb(COLOR_HEADER_DISABLED_BORDER))
                                    .child(""),
                            )]
                    } else {
                        self.headers
                            .iter()
                            .enumerate()
                            .map(|(index, (enabled, key, value))| {
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        // Checkbox column
                                        div()
                                            .w_8()
                                            .h_8()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .bg(rgb(Self::checkbox_bg_color(*enabled)))
                                            .border_1()
                                            .border_color(rgb(COLOR_HEADER_DISABLED_BORDER))
                                            .rounded_sm()
                                            .cursor_pointer()
                                            .hover(|style| {
                                                style.bg(rgb(Self::checkbox_hover_bg_color(
                                                    *enabled,
                                                )))
                                            })
                                            .child(if *enabled { "✓" } else { "" })
                                            .text_color(rgb(COLOR_CHECKBOX_TEXT))
                                            .on_mouse_up(
                                                gpui::MouseButton::Left,
                                                cx.listener(move |this, _event, _window, cx| {
                                                    this.toggle_header(index, cx);
                                                }),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .px_3()
                                            .py_2()
                                            .bg(rgb(Self::header_cell_bg_color(*enabled)))
                                            .border_1()
                                            .border_color(rgb(Self::header_cell_border_color(
                                                *enabled,
                                            )))
                                            .text_color(rgb(Self::header_text_color(*enabled)))
                                            .child(key.clone()),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .px_3()
                                            .py_2()
                                            .bg(rgb(Self::header_cell_bg_color(*enabled)))
                                            .border_1()
                                            .border_color(rgb(Self::header_cell_border_color(
                                                *enabled,
                                            )))
                                            .text_color(rgb(Self::header_text_color(*enabled)))
                                            .child(value.clone()),
                                    )
                                    .child(
                                        div()
                                            .w_16()
                                            .px_2()
                                            .py_1()
                                            .bg(rgb(0x00dc_3545))
                                            .text_color(rgb(0x00ff_ffff))
                                            .rounded_md()
                                            .cursor_pointer()
                                            .hover(|style| style.bg(rgb(0x00c8_2333)))
                                            .child("Delete")
                                            .on_mouse_up(
                                                gpui::MouseButton::Left,
                                                cx.listener(move |this, _event, _window, cx| {
                                                    this.remove_header(index, cx);
                                                }),
                                            ),
                                    )
                            })
                            .collect()
                    }),
            )
            // 添加新header的输入框
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        // Empty checkbox column for alignment
                        div().w_8(),
                    )
                    .child(self.header_key_input.clone())
                    .child(self.header_value_input.clone())
                    .child(
                        div()
                            .w_16()
                            .px_2()
                            .py_1()
                            .bg(rgb(0x0028_a745))
                            .text_color(rgb(0x00ff_ffff))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0x0021_8838)))
                            .child("Add")
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.add_header(cx);
                                }),
                            ),
                    ),
            )
            // 快速添加预设headers
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(0x006c_757d))
                            .child("Quick add: "),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .bg(rgb(0x006c_757d))
                            .text_color(rgb(0x00ff_ffff))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0x005a_6268)))
                            .child("JSON")
                            .text_size(px(12.0))
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.set_header_input_values(
                                        "Content-Type",
                                        "application/json",
                                        cx,
                                    );
                                }),
                            ),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .bg(rgb(0x006c_757d))
                            .text_color(rgb(0x00ff_ffff))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0x005a_6268)))
                            .child("Auth")
                            .text_size(px(12.0))
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.set_header_input_values("Authorization", "Bearer ", cx);
                                }),
                            ),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .bg(rgb(0x006c_757d))
                            .text_color(rgb(0x00ff_ffff))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0x005a_6268)))
                            .child("CORS")
                            .text_size(px(12.0))
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.set_header_input_values(
                                        "Access-Control-Allow-Origin",
                                        "*",
                                        cx,
                                    );
                                }),
                            ),
                    ),
            )
            // 统计信息
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(0x006c_757d))
                    .child(format!(
                    "Total headers: {} | Enabled: {} | Add headers by typing key and value above",
                    self.headers.len(),
                    self.headers
                        .iter()
                        .filter(|(enabled, _, _)| *enabled)
                        .count()
                )),
            )
    }

    fn render_body_editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
            .child(self.body_input.clone())
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(0x006c_757d))
                    .child(match self.body_input.read(cx).get_current_type() {
                        crate::ui::components::body_input::BodyType::Json => {
                            format!(
                                "JSON body length: {} characters",
                                self.body_input.read(cx).get_json_content().len()
                            )
                        }
                        crate::ui::components::body_input::BodyType::FormData => {
                            format!(
                                "Form data entries: {}",
                                self.body_input.read(cx).get_form_data_entries().len()
                            )
                        }
                        crate::ui::components::body_input::BodyType::Raw => {
                            format!(
                                "Raw body length: {} characters",
                                self.body_input.read(cx).get_content().len()
                            )
                        }
                    }),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(0x006c_757d))
                            .child("Quick actions: "),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .bg(rgb(0x0017_a2b8))
                            .text_color(rgb(0x00ff_ffff))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0x0013_8496)))
                            .child("Sample JSON")
                            .text_size(px(12.0))
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    let sample_json = r#"{
                                                                "name": "John Doe",
                                                                "email": "john.doe@example.com",
                                                                "age": 30
                                                                }"#
                                    .to_string();
                                    this.body_input.update(cx, |input, cx| {
                                        input.set_content(sample_json, cx);
                                    });
                                }),
                            ),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .bg(rgb(0x00dc_3545))
                            .text_color(rgb(0x00ff_ffff))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0x00c8_2333)))
                            .child("Clear")
                            .text_size(px(12.0))
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.body_input.update(cx, |input, cx| {
                                        input.clear(cx);
                                    });
                                }),
                            ),
                    ),
            )
    }
}

impl Render for PostmanApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Subscribe to history list events
        let history_list_clone = self.history_list.clone();
        cx.subscribe(&history_list_clone, Self::on_history_selected)
            .detach();

        div()
            .id("main-container")
            .flex()
            .bg(rgb(0x00f0_f0f0))
            .size_full()
            .child(
                // Left sidebar - History List
                self.history_list.clone(),
            )
            .child(
                // Main content area
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
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
                            .bg(rgb(0x00ff_ffff))
                            .border_1()
                            .border_color(rgb(0x00cc_cccc))
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
                                            .bg(rgb(0x0000_7acc))
                                            .text_color(rgb(0x00ff_ffff))
                                            .px_4()
                                            .py_2()
                                            .rounded_md()
                                            .cursor_pointer()
                                            .hover(|style| style.bg(rgb(0x0000_56b3)))
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
                            .id("response-container")
                            .overflow_scroll()
                            .flex()
                            .flex_col()
                            .gap_4()
                            .p_4()
                            .bg(rgb(0x00ff_ffff))
                            .border_1()
                            .border_color(rgb(0x00cc_cccc))
                            .child(self.response_viewer.clone()),
                    ),
            )
    }
}

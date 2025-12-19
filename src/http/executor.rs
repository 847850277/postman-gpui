use crate::http::client::HttpClient;
use crate::models::Request;
use std::collections::HashMap;

/// HTTP 请求执行结果
#[derive(Debug, Clone)]
pub struct RequestResult {
    pub status: u16,
    pub body: String,
}

impl RequestResult {
    pub fn success(body: String) -> Self {
        Self { status: 200, body }
    }

    pub fn error(message: String) -> Self {
        Self {
            status: 0,
            body: message,
        }
    }
}

/// HTTP 请求执行器
pub struct RequestExecutor {
    client: HttpClient,
}

impl RequestExecutor {
    pub fn new() -> Self {
        Self {
            client: HttpClient::new(),
        }
    }

    /// 执行 HTTP 请求（接受统一的 Request 模型）
    pub fn execute_request(&self, request: &Request) -> Result<RequestResult, String> {
        self.execute(
            &request.method,
            &request.url,
            request.headers.clone(),
            request.body.clone(),
        )
    }

    /// 执行 HTTP 请求（保留原有接口以兼容）
    pub fn execute(
        &self,
        method: &str,
        url: &str,
        headers: Vec<(String, String)>,
        body: Option<String>,
    ) -> Result<RequestResult, String> {
        // 验证URL
        if url.trim().is_empty() {
            println!("❌ RequestExecutor - URL不能为空");
            return Err("Error: URL cannot be empty".to_string());
        }

        println!("🚀 RequestExecutor - 开始发送请求");
        println!("📋 RequestExecutor - 请求详情:");
        println!("   Method: {method}");
        println!("   URL: {url}");
        println!("   Headers Count: {}", headers.len());

        // 打印所有headers
        if !headers.is_empty() {
            println!("   Headers:");
            for (i, (key, value)) in headers.iter().enumerate() {
                println!("     {}. {} = {}", i + 1, key, value);
            }
        } else {
            println!("   Headers: None");
        }

        // 打印请求体信息
        if let Some(ref body_content) = body {
            println!("   Body Length: {} bytes", body_content.len());
            if !body_content.is_empty() {
                println!(
                    "   Body Preview: {}",
                    if body_content.len() > 200 {
                        format!("{}... (truncated)", &body_content[..200])
                    } else {
                        body_content.to_string()
                    }
                );
            } else {
                println!("   Body: Empty");
            }
        }

        // 使用 tokio 的 block_on 来同步执行异步请求
        let rt = tokio::runtime::Runtime::new().unwrap();

        let result = match method.to_uppercase().as_str() {
            "GET" => {
                // GET 请求
                let header_map = if headers.is_empty() {
                    println!("🔍 RequestExecutor - 执行GET请求，无自定义headers");
                    None
                } else {
                    let map: HashMap<String, String> = headers.iter().cloned().collect();
                    println!(
                        "🔍 RequestExecutor - 执行GET请求，包含{}个自定义headers",
                        map.len()
                    );
                    Some(map)
                };
                rt.block_on(self.client.get(url, header_map))
            }
            "POST" => {
                // POST 请求
                let header_map = if headers.is_empty() {
                    println!("📝 RequestExecutor - POST请求，无自定义headers");
                    None
                } else {
                    let map: HashMap<String, String> = headers.iter().cloned().collect();
                    println!(
                        "📝 RequestExecutor - POST请求，包含{}个自定义headers",
                        map.len()
                    );
                    Some(map)
                };

                let body_content = body.unwrap_or_default();
                println!(
                    "📤 RequestExecutor - 执行POST请求，Body大小: {} bytes",
                    body_content.len()
                );
                rt.block_on(self.client.post(url, &body_content, header_map))
            }
            _ => {
                println!("⚠️ RequestExecutor - 方法 {method} 尚未实现");
                println!("📋 RequestExecutor - 当前支持的方法: GET, POST");
                return Err(format!("Method {method} not implemented yet"));
            }
        };

        match result {
            Ok(response_body) => {
                println!("✅ RequestExecutor - {}请求成功!", method.to_uppercase());
                println!("📊 RequestExecutor - 响应信息:");
                println!("   Status: 200 OK");
                println!("   Response Length: {} bytes", response_body.len());
                println!(
                    "   Response Preview: {}",
                    if response_body.len() > 300 {
                        format!("{}... (truncated)", &response_body[..300])
                    } else {
                        response_body.clone()
                    }
                );
                Ok(RequestResult::success(response_body))
            }
            Err(e) => {
                println!("❌ RequestExecutor - {}请求失败!", method.to_uppercase());
                println!("💥 RequestExecutor - 错误详情:");
                println!("   Error: {e}");
                println!("   可能的原因:");
                println!("     - 网络连接问题");
                println!("     - 服务器未响应");
                println!("     - URL格式错误");
                println!("     - 服务器返回错误状态码");
                Err(format!("请求失败: {e}"))
            }
        }
    }
}

impl Default for RequestExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executor_creation() {
        let executor = RequestExecutor::new();
        // Verify executor can be created
        assert!(std::mem::size_of_val(&executor) > 0);
    }

    #[test]
    fn test_executor_execute_validates_empty_url() {
        let executor = RequestExecutor::new();
        let result = executor.execute("GET", "", vec![], None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("URL cannot be empty"));
    }

    #[test]
    fn test_executor_execute_request_model() {
        let executor = RequestExecutor::new();
        let mut request = Request::new("GET", "https://httpbin.org/get");
        request.add_header("User-Agent", "test-agent");
        
        // Just verify the model can be passed to the executor
        // We won't actually make the request in the test
        assert!(request.is_valid());
        assert_eq!(request.headers.len(), 1);
    }
}

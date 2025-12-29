use crate::errors::AppError;
use crate::http::client::HttpClient;
use crate::models::{HttpMethod, Request};
use crate::utils::formatter::format_response_body;
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
    pub fn execute_request(&self, request: &Request) -> Result<RequestResult, AppError> {
        self.execute(
            request.method,
            &request.url,
            request.headers.clone(),
            request.body.clone(),
        )
    }

    /// 执行 HTTP 请求（保留原有接口以兼容）
    pub fn execute(
        &self,
        method: HttpMethod,
        url: &str,
        headers: Vec<(String, String)>,
        body: Option<String>,
    ) -> Result<RequestResult, AppError> {
        // 验证URL
        if url.trim().is_empty() {
            tracing::info!("❌ RequestExecutor - URL不能为空");
            return Err(AppError::UrlEmpty);
        }
        tracing::info!("🚀 RequestExecutor - 开始发送请求");
        tracing::info!("📋 RequestExecutor - 请求详情:");
        tracing::info!("   Method: {}", method);
        tracing::info!("   URL: {}", url);
        tracing::info!("   Headers Count: {}", headers.len());

        // 打印所有headers
        if !headers.is_empty() {
            tracing::info!("   Headers:");
            for (i, (key, value)) in headers.iter().enumerate() {
                tracing::info!("     {}. {} = {}", i + 1, key, value);
            }
        } else {
            tracing::info!("   Headers: None");
        }

        // 打印请求体信息
        if let Some(ref body_content) = body {
            tracing::info!("   Body Length: {} bytes", body_content.len());
            if !body_content.is_empty() {
                tracing::info!(
                    "   Body Preview: {}",
                    if body_content.len() > 200 {
                        format!("{}... (truncated)", &body_content[..200])
                    } else {
                        body_content.to_string()
                    }
                );
            } else {
                tracing::info!("   Body: Empty");
            }
        }

        // 使用 tokio 的 block_on 来同步执行异步请求
        let rt = tokio::runtime::Runtime::new().unwrap();

        let result = match method {
            HttpMethod::GET => {
                // GET 请求
                let header_map = if headers.is_empty() {
                    tracing::info!("🔍 RequestExecutor - 执行GET请求，无自定义headers");
                    None
                } else {
                    let map: HashMap<String, String> = headers.iter().cloned().collect();
                    tracing::info!(
                        "🔍 RequestExecutor - 执行GET请求，包含{}个自定义headers",
                        map.len()
                    );
                    Some(map)
                };
                rt.block_on(self.client.get_with_headers(url, header_map))
            }
            HttpMethod::POST => {
                // POST 请求
                let header_map = if headers.is_empty() {
                    tracing::info!("📝 RequestExecutor - 执行POST请求，无自定义headers");
                    None
                } else {
                    let map: HashMap<String, String> = headers.iter().cloned().collect();
                    tracing::info!(
                        "📝 RequestExecutor - POST请求，包含{}个自定义headers",
                        map.len()
                    );
                    Some(map)
                };

                let body_content = body.unwrap_or_default();
                tracing::info!(
                    "📝 RequestExecutor - 执行POST请求，Body大小: {} bytes",
                    body_content.len()
                );
                rt.block_on(self.client.post(url, &body_content, header_map))
            }
            HttpMethod::PUT | HttpMethod::DELETE | HttpMethod::PATCH | HttpMethod::HEAD | HttpMethod::OPTIONS => {
                tracing::info!("⚠️ RequestExecutor - 方法 {} 尚未实现", method);
                tracing::info!("📋 RequestExecutor - 当前支持的方法: GET, POST");
                return Err(AppError::ValidationError(format!(
                    "Unsupported HTTP method: {}. Supported methods are: GET, POST",
                    method
                )));
            }
        };

        match result {
            Ok(response_body) => {
                tracing::info!("✅ RequestExecutor - {}请求成功!", method);
                tracing::info!("📊 RequestExecutor - 响应信息:");
                tracing::info!("   Status: 200 OK");
                tracing::info!("   Response Length: {} bytes", response_body.len());
                tracing::info!(
                    "   Response Preview: {}",
                    if response_body.len() > 300 {
                        format!("{}... (truncated)", &response_body[..300])
                    } else {
                        response_body.clone()
                    }
                );
                // Format the response body (pretty-print JSON if applicable)
                let formatted_body = format_response_body(&response_body);

                Ok(RequestResult::success(formatted_body))
            }
            Err(e) => {
                tracing::info!("❌ RequestExecutor - {}请求失败!", method);
                tracing::info!("💥 RequestExecutor - 错误详情:");
                tracing::info!("   Error: {}", e);
                tracing::info!("   可能的原因:");
                tracing::info!("     - 网络连接问题");
                tracing::info!("     - 服务器未响应");
                tracing::info!("     - URL格式错误");
                tracing::info!("     - 服务器返回错误状态码");
                Err(e)
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
        let result = executor.execute(HttpMethod::GET, "", vec![], None);
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e, AppError::UrlEmpty));
        }
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

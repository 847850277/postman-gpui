use crate::errors::AppError;
use crate::http::client::HttpClient;
use crate::models::{HttpMethod, Request, RequestBody};
use crate::utils::formatter::format_response_body;
use std::sync::Arc;

/// HTTP 请求执行结果
#[derive(Debug, Clone)]
pub struct RequestResult {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub elapsed_ms: u128,
}

impl RequestResult {
    pub fn success(body: String) -> Self {
        Self {
            status: 200,
            headers: Vec::new(),
            body,
            elapsed_ms: 0,
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            status: 0,
            headers: Vec::new(),
            body: message,
            elapsed_ms: 0,
        }
    }
}

/// HTTP 请求执行器
#[derive(Clone)]
pub struct RequestExecutor {
    client: HttpClient,
    runtime: Arc<tokio::runtime::Runtime>,
}

impl RequestExecutor {
    pub fn new() -> Self {
        Self {
            client: HttpClient::new(),
            runtime: Arc::new(
                tokio::runtime::Runtime::new()
                    .expect("the built-in async HTTP runtime should be available"),
            ),
        }
    }

    /// 执行 HTTP 请求（接受统一的 Request 模型）
    pub fn execute_request(&self, request: &Request) -> Result<RequestResult, AppError> {
        self.execute_body(
            request.method,
            &request.url,
            request.headers.clone(),
            request.body.clone(),
        )
    }

    /// Starts a cancellable request on the shared Tokio runtime. Dropping or aborting the
    /// returned handle cancels the underlying reqwest future instead of merely ignoring its UI
    /// result.
    pub fn spawn_request(
        &self,
        request: Request,
    ) -> (
        tokio::task::AbortHandle,
        tokio::task::JoinHandle<Result<RequestResult, AppError>>,
    ) {
        let client = self.client.clone();
        let handle = self.runtime.spawn(Self::execute_with_client(
            client,
            request.method,
            request.url,
            request.headers,
            request.body,
        ));
        (handle.abort_handle(), handle)
    }

    /// Bridges a Tokio request task back through GPUI's background executor. Keeping the join
    /// wake-up off GPUI's foreground executor also preserves deterministic GPUI tests.
    pub fn wait_for_request(
        &self,
        handle: tokio::task::JoinHandle<Result<RequestResult, AppError>>,
    ) -> Result<RequestResult, AppError> {
        self.runtime.block_on(handle).unwrap_or_else(|error| {
            Err(AppError::NetworkError(format!(
                "request task stopped before completion: {error}"
            )))
        })
    }

    /// 执行 HTTP 请求（保留原有接口以兼容）
    pub fn execute(
        &self,
        method: HttpMethod,
        url: &str,
        headers: Vec<(String, String)>,
        body: Option<String>,
    ) -> Result<RequestResult, AppError> {
        self.execute_body(
            method,
            url,
            headers,
            body.map(RequestBody::Raw).unwrap_or(RequestBody::None),
        )
    }

    fn execute_body(
        &self,
        method: HttpMethod,
        url: &str,
        headers: Vec<(String, String)>,
        body: RequestBody,
    ) -> Result<RequestResult, AppError> {
        self.runtime.block_on(Self::execute_with_client(
            self.client.clone(),
            method,
            url.to_string(),
            headers,
            body,
        ))
    }

    async fn execute_with_client(
        client: HttpClient,
        method: HttpMethod,
        url: String,
        headers: Vec<(String, String)>,
        body: RequestBody,
    ) -> Result<RequestResult, AppError> {
        // 验证URL
        if url.trim().is_empty() {
            tracing::info!("❌ RequestExecutor - URL不能为空");
            return Err(AppError::UrlEmpty);
        }
        tracing::info!("🚀 RequestExecutor - 开始发送请求");
        tracing::info!("📋 RequestExecutor - 请求详情:");
        tracing::info!("   Method: {}", method);
        tracing::info!("   URL: {}", display_url_for_log(&url));
        tracing::info!("   Headers Count: {}", headers.len());

        if !headers.is_empty() {
            tracing::info!("   Headers:");
            for (i, (key, value)) in headers.iter().enumerate() {
                tracing::info!(
                    "     {}. {} = {}",
                    i + 1,
                    key,
                    display_header_value(key, value)
                );
            }
        } else {
            tracing::info!("   Headers: None");
        }

        if !body.is_none() {
            tracing::info!("   Body Length: {} bytes", body.payload_len());
        }

        let started = std::time::Instant::now();
        let result = client.request(method, &url, headers, body).await;
        let elapsed_ms = started.elapsed().as_millis();

        match result {
            Ok(response) => {
                tracing::info!("✅ RequestExecutor - {}请求完成!", method);
                tracing::info!("📊 RequestExecutor - 响应信息:");
                tracing::info!("   Status: {}", response.status());
                tracing::info!("   Elapsed: {} ms", elapsed_ms);
                tracing::info!("   Response Length: {} bytes", response.body().len());
                let formatted_body = format_response_body(response.body());

                Ok(RequestResult {
                    status: response.status(),
                    headers: response.headers().to_vec(),
                    body: formatted_body,
                    elapsed_ms,
                })
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

fn display_header_value<'a>(name: &str, value: &'a str) -> &'a str {
    if is_sensitive_header(name) {
        "[REDACTED]"
    } else {
        value
    }
}

fn is_sensitive_header(name: &str) -> bool {
    let compact_name: String = name
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    matches!(
        compact_name.as_str(),
        "authorization" | "proxyauthorization" | "cookie" | "setcookie" | "apikey"
    ) || compact_name.contains("token")
        || compact_name.contains("secret")
        || compact_name.contains("password")
        || compact_name.contains("credential")
}

fn display_url_for_log(value: &str) -> String {
    let Ok(url) = reqwest::Url::parse(value) else {
        return "[INVALID URL]".to_string();
    };
    let Some(host) = url.host_str() else {
        return "[URL WITHOUT HOST]".to_string();
    };
    let mut output = format!("{}://{host}", url.scheme());
    if let Some(port) = url.port() {
        output.push_str(&format!(":{port}"));
    }
    output.push_str(url.path());
    if url.query().is_some() {
        output.push_str("?[REDACTED]");
    }
    if url.fragment().is_some() {
        output.push_str("#[REDACTED]");
    }
    output
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
        let _executor = RequestExecutor::new();
        let mut request = Request::new("GET", "https://httpbin.org/get");
        request.add_header("User-Agent", "test-agent");

        // Just verify the model can be passed to the executor
        // We won't actually make the request in the test
        assert!(request.is_valid());
        assert_eq!(request.headers.len(), 1);
    }

    #[test]
    fn sensitive_header_values_are_redacted_before_logging() {
        assert_eq!(
            display_header_value("Authorization", "Bearer secret"),
            "[REDACTED]"
        );
        assert_eq!(
            display_header_value("cookie", "session=secret"),
            "[REDACTED]"
        );
        assert_eq!(display_header_value("X-Trace", "visible"), "visible");
        assert_eq!(display_header_value("X-Auth-Token", "secret"), "[REDACTED]");
    }

    #[test]
    fn urls_are_logged_without_credentials_or_query_values() {
        assert_eq!(
            display_url_for_log("https://user:pass@example.com/search?api_key=secret#token"),
            "https://example.com/search?[REDACTED]#[REDACTED]"
        );
    }
}

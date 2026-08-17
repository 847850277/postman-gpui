use crate::errors::AppError;
use crate::http::client::HttpClient;
use crate::models::{HttpMethod, Request, RequestBody};
use crate::utils::log::{format_http_request, format_http_response};
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
        if url.trim().is_empty() {
            tracing::debug!(method = %method, "skipping empty URL");
            return Err(AppError::UrlEmpty);
        }

        let url_for_log = crate::utils::log::display_url_for_log(&url);
        if tracing::enabled!(target: "postman_gpui::http", tracing::Level::INFO) {
            let request_log = format_http_request(method, &url, &headers, &body);
            tracing::info!(target: "postman_gpui::http", "\n{}", request_log);
        }

        let started = std::time::Instant::now();
        let result = client.request(method, &url, headers, body).await;
        let elapsed_ms = started.elapsed().as_millis();

        match result {
            Ok(response) => {
                if tracing::enabled!(target: "postman_gpui::http", tracing::Level::INFO) {
                    let response_log = format_http_response(
                        response.status(),
                        elapsed_ms,
                        response.headers(),
                        response.body(),
                    );
                    tracing::info!(target: "postman_gpui::http", "\n{}", response_log);
                }
                Ok(RequestResult {
                    status: response.status(),
                    headers: response.headers().to_vec(),
                    // ResponseState owns the exact server text. Formatting belongs to the
                    // response viewer so copy/export features never lose the original payload.
                    body: response.body().to_string(),
                    elapsed_ms,
                })
            }
            Err(error) => {
                tracing::warn!(
                    target: "postman_gpui::http",
                    method = %method,
                    url = %url_for_log,
                    elapsed_ms,
                    error = %error,
                    "HTTP RESPONSE: transport failed"
                );
                Err(error)
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
        let _executor = RequestExecutor::new();
        let mut request = Request::new("GET", "https://httpbingo.org/get");
        request.add_header("User-Agent", "test-agent");

        // Just verify the model can be passed to the executor
        // We won't actually make the request in the test
        assert!(request.is_valid());
        assert_eq!(request.headers.len(), 1);
    }
}

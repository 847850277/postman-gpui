use crate::errors::AppError;
use crate::http::client::HttpClient;
use crate::models::Request;
use crate::utils::log::{format_http_request, format_http_response};
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context as TaskContext, Poll},
};

/// HTTP 请求执行结果
#[derive(Debug, Clone)]
pub struct RequestResult {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub elapsed_ms: u128,
}

/// A cancellable request scheduled on the executor's Tokio runtime.
///
/// Public callers await the task directly. Callers that require blocking behavior (such as the
/// deterministic scenario harness) own that scheduling adapter themselves.
#[must_use = "request tasks must be awaited or explicitly aborted"]
pub struct RequestTask {
    handle: tokio::task::JoinHandle<Result<RequestResult, AppError>>,
    runtime: Arc<tokio::runtime::Runtime>,
}

impl RequestTask {
    pub fn abort_handle(&self) -> tokio::task::AbortHandle {
        self.handle.abort_handle()
    }

    /// GPUI's background executor does not reliably receive Tokio join wake-ups during its
    /// deterministic test loop. This crate-only adapter blocks that background thread on the
    /// same typed task; it does not create a second request construction or transport path.
    pub(crate) fn join_on_background_thread(self) -> Result<RequestResult, AppError> {
        let runtime = self.runtime.clone();
        runtime.block_on(self)
    }
}

impl Future for RequestTask {
    type Output = Result<RequestResult, AppError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Self::Output> {
        match Pin::new(&mut self.handle).poll(cx) {
            Poll::Ready(Ok(result)) => Poll::Ready(result),
            Poll::Ready(Err(error)) => Poll::Ready(Err(AppError::NetworkError(format!(
                "request task stopped before completion: {error}"
            )))),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Drop for RequestTask {
    fn drop(&mut self) {
        if !self.handle.is_finished() {
            self.handle.abort();
        }
    }
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

    /// Starts a cancellable request on the shared Tokio runtime. Dropping the returned task or
    /// using its abort handle cancels the underlying reqwest future instead of merely ignoring
    /// its UI result.
    pub fn spawn(&self, request: Request) -> RequestTask {
        let client = self.client.clone();
        let handle = self.runtime.spawn(Self::execute(client, request));
        RequestTask {
            handle,
            runtime: self.runtime.clone(),
        }
    }

    /// Canonical transport path. Every caller supplies the same typed command; only scheduling
    /// differs between the GPUI application and deterministic test adapters.
    async fn execute(client: HttpClient, request: Request) -> Result<RequestResult, AppError> {
        if request.url.trim().is_empty() {
            tracing::debug!(method = %request.method, "skipping empty URL");
            return Err(AppError::UrlEmpty);
        }

        let method = request.method;
        let url_for_log = crate::utils::log::display_url_for_log(&request.url);
        if tracing::enabled!(target: "postman_gpui::http", tracing::Level::INFO) {
            let request_log = format_http_request(
                request.method,
                &request.url,
                &request.headers,
                &request.body,
            );
            tracing::info!(target: "postman_gpui::http", "\n{}", request_log);
        }

        let started = std::time::Instant::now();
        let result = client.execute(request).await;
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
    use crate::models::{HttpMethod, RequestBody};
    use mockito::Server;
    use std::{
        sync::{mpsc, Condvar, Mutex},
        time::Duration,
    };

    fn wait(task: RequestTask) -> Result<RequestResult, AppError> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("the test scheduling adapter should be available")
            .block_on(task)
    }

    #[test]
    fn test_executor_creation() {
        let executor = RequestExecutor::new();
        // Verify executor can be created
        assert!(std::mem::size_of_val(&executor) > 0);
    }

    #[test]
    fn test_executor_execute_validates_empty_url() {
        let executor = RequestExecutor::new();
        let task = executor.spawn(Request::new(HttpMethod::GET, ""));
        let result = wait(task);
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e, AppError::UrlEmpty));
        }
    }

    #[test]
    fn typed_request_is_the_canonical_execution_command() {
        let mut server = Server::new();
        let received = server
            .mock("POST", "/typed")
            .match_header("content-type", "application/json")
            .match_body(r#"{"name":"Ada"}"#)
            .with_status(201)
            .with_header("x-contract", "typed-request")
            .with_body("created")
            .create();
        let mut request = Request::new(HttpMethod::POST, format!("{}/typed", server.url()));
        request.add_header("Content-Type", "application/json");
        request.body = RequestBody::Json(r#"{"name":"Ada"}"#.to_string());

        let executor = RequestExecutor::new();
        let result = wait(executor.spawn(request)).expect("typed request should succeed");

        assert_eq!(result.status, 201);
        assert_eq!(result.body, "created");
        assert!(result
            .headers
            .iter()
            .any(|header| header == &("x-contract".to_string(), "typed-request".to_string())));
        received.assert();
    }

    #[test]
    fn abort_handle_cancels_the_underlying_request_task() {
        let mut server = Server::new();
        let (response_started_tx, response_started_rx) = mpsc::channel();
        let release_response = std::sync::Arc::new((Mutex::new(false), Condvar::new()));
        let response_gate = release_response.clone();
        let slow_response = server
            .mock("GET", "/slow")
            .with_chunked_body(move |writer| {
                writer.write_all(b"started")?;
                let _ = response_started_tx.send(());
                let (released, wake) = &*response_gate;
                let released = released
                    .lock()
                    .expect("response gate should not be poisoned");
                let _ = wake
                    .wait_timeout_while(released, Duration::from_secs(2), |released| !*released)
                    .expect("response gate should remain available");
                Ok(())
            })
            .create();
        let executor = RequestExecutor::new();
        let task = executor.spawn(Request::new(
            HttpMethod::GET,
            format!("{}/slow", server.url()),
        ));
        let abort_handle = task.abort_handle();

        response_started_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("the slow response should start before cancellation");
        abort_handle.abort();
        let (released, wake) = &*release_response;
        *released
            .lock()
            .expect("response gate should not be poisoned") = true;
        wake.notify_all();

        let error = wait(task).expect_err("aborted request should not complete successfully");
        assert!(matches!(error, AppError::NetworkError(message) if message.contains("cancelled")));
        slow_response.assert();
    }
}

use crate::errors::AppError;
use crate::utils::log::{format_http_request, format_http_response};
use postman_http::request::{Request, RequestOptions};
use postman_http::{HttpError, HttpResponse, HttpTransport};
use postman_request::RequestClient;
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context as TaskContext, Poll},
};

/// A cancellable request scheduled on the executor's Tokio runtime.
///
/// Public callers await the task directly. Callers that require blocking behavior (such as the
/// deterministic scenario harness) own that scheduling adapter themselves.
#[must_use = "request tasks must be awaited or explicitly aborted"]
pub struct RequestTask {
    handle: tokio::task::JoinHandle<Result<HttpResponse, AppError>>,
    runtime: Arc<tokio::runtime::Runtime>,
}

impl RequestTask {
    pub fn abort_handle(&self) -> tokio::task::AbortHandle {
        self.handle.abort_handle()
    }

    /// GPUI's background executor does not reliably receive Tokio join wake-ups during its
    /// deterministic test loop. This crate-only adapter blocks that background thread on the
    /// same typed task; it does not create a second request construction or transport path.
    pub(crate) fn join_on_background_thread(self) -> Result<HttpResponse, AppError> {
        let runtime = self.runtime.clone();
        runtime.block_on(self)
    }
}

impl Future for RequestTask {
    type Output = Result<HttpResponse, AppError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Self::Output> {
        match Pin::new(&mut self.handle).poll(cx) {
            Poll::Ready(Ok(result)) => Poll::Ready(result),
            Poll::Ready(Err(error)) if error.is_cancelled() => {
                Poll::Ready(Err(AppError::Http(HttpError::Cancelled)))
            }
            Poll::Ready(Err(error)) => Poll::Ready(Err(AppError::RuntimeError(format!(
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

/// HTTP 请求执行器
pub struct RequestExecutor {
    client: RequestClient,
    runtime: Arc<tokio::runtime::Runtime>,
}

impl RequestExecutor {
    pub fn try_new() -> Result<Self, AppError> {
        Ok(Self {
            client: RequestClient::try_new().map_err(AppError::from)?,
            runtime: Arc::new(tokio::runtime::Runtime::new().map_err(|error| {
                AppError::RuntimeError(format!("failed to initialize HTTP runtime: {error}"))
            })?),
        })
    }

    /// Starts a cancellable request on the shared Tokio runtime. Dropping the returned task or
    /// using its abort handle cancels the underlying reqwest future instead of merely ignoring
    /// its UI result.
    pub fn spawn(&self, request: Request) -> RequestTask {
        self.spawn_with_options(request, RequestOptions::default())
    }

    /// Starts the canonical request path with an optional request-level deadline. A zero value is
    /// treated as disabled so callers cannot accidentally create an immediate timeout.
    pub fn spawn_with_timeout(&self, request: Request, timeout_ms: Option<u64>) -> RequestTask {
        self.spawn_with_options(
            request,
            RequestOptions {
                timeout_ms,
                ..RequestOptions::default()
            },
        )
    }

    /// Starts the canonical request path with the complete per-request transport policy.
    pub fn spawn_with_options(&self, request: Request, options: RequestOptions) -> RequestTask {
        let client = self.client.clone();
        let handle = self.runtime.spawn(Self::execute(client, request, options));
        RequestTask {
            handle,
            runtime: self.runtime.clone(),
        }
    }

    pub(crate) fn cookie_snapshot(&self) -> Vec<(String, String)> {
        self.client.cookie_snapshot()
    }

    pub(crate) fn clear_cookies(&self) -> usize {
        self.client.clear_cookies()
    }

    /// Canonical transport path. Every caller supplies the same typed command; only scheduling
    /// differs between the GPUI application and deterministic test adapters.
    async fn execute(
        client: RequestClient,
        request: Request,
        options: RequestOptions,
    ) -> Result<HttpResponse, AppError> {
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
        match client.execute(request, options).await {
            Ok(response) => {
                let elapsed_ms = response.elapsed_ms;
                if tracing::enabled!(target: "postman_gpui::http", tracing::Level::INFO) {
                    let response_log = format_http_response(
                        response.status(),
                        elapsed_ms,
                        response.headers(),
                        response.body(),
                    );
                    tracing::info!(target: "postman_gpui::http", "\n{}", response_log);
                }
                Ok(response)
            }
            Err(error) => {
                let elapsed_ms = started.elapsed().as_millis();
                tracing::warn!(
                    target: "postman_gpui::http",
                    method = %method,
                    url = %url_for_log,
                    elapsed_ms,
                    error = %error,
                    "HTTP RESPONSE: transport failed"
                );
                Err(error.into())
            }
        }
    }
}

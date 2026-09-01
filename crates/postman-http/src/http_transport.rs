//! Runtime-neutral boundary for executing typed HTTP requests.
//!
//! This module describes the capability required by callers such as the Flow engine. Concrete
//! adapters decide how bytes reach the network, while the application decides where the returned
//! future is polled. Consequently this crate does not expose reqwest or Tokio runtime types and
//! does not allocate a boxed future on its default execution path.

use std::future::Future;

use crate::error::HttpError;
use crate::request::{Request, RequestOptions};
use crate::response::HttpResponse;

/// Port implemented by a concrete HTTP transport.
///
/// Implementations translate [`Request`] into their wire representation and translate every
/// implementation-specific response or failure back into [`HttpResponse`] or [`HttpError`]. They
/// must not leak reqwest, Tokio, GPUI, or other adapter-specific types through this interface.
///
/// The opaque `impl Future` return type gives every implementation its own statically dispatched
/// future. This avoids the allocation and virtual `poll` call required by `Box<dyn Future>` and
/// allows the compiler to monomorphize transport calls. Callers should therefore use this trait as
/// a generic bound, for example `FlowRunner<T: HttpTransport>`.
///
/// This choice deliberately makes [`HttpTransport`] unsuitable for direct use as
/// `dyn HttpTransport`. If runtime-selected transports are needed later, that application boundary
/// can add a separate boxed adapter without charging the default request path for it.
///
/// Keeping scheduling outside this trait lets a GPUI application, CLI, Flow runner, and tests use
/// the same transport contract with different execution environments.
pub trait HttpTransport: Send + Sync {
    /// Creates the asynchronous operation for one request.
    ///
    /// Calling this method does not imply that a background task has been spawned. The caller must
    /// poll or await the returned future in its chosen runtime. `request` and `options` are owned so
    /// an implementation may safely retain them across suspension points. `Send` allows the host
    /// runtime to move that future between worker threads, while `'_` prevents it from outliving
    /// the borrowed transport.
    fn execute(
        &self,
        request: Request,
        options: RequestOptions,
    ) -> impl Future<Output = Result<HttpResponse, HttpError>> + Send + '_;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ReadyTransport;

    impl HttpTransport for ReadyTransport {
        fn execute(
            &self,
            _request: Request,
            _options: RequestOptions,
        ) -> impl Future<Output = Result<HttpResponse, HttpError>> + Send + '_ {
            std::future::ready(Ok(HttpResponse::new(204, Vec::new(), String::new())))
        }
    }

    #[test]
    fn concrete_transport_returns_a_statically_dispatched_send_future() {
        fn assert_send_http_future(
            _future: &(impl Future<Output = Result<HttpResponse, HttpError>> + Send),
        ) {
        }

        let transport = ReadyTransport;
        let future = transport.execute(
            Request::new(crate::request::HttpMethod::GET, "https://example.com"),
            RequestOptions::default(),
        );

        assert_send_http_future(&future);
    }
}

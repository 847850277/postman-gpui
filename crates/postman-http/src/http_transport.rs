use std::future::Future;
use std::pin::Pin;

use crate::error::HttpError;
use crate::request::{Request, RequestOptions};
use crate::response::HttpResponse;

pub type HttpFuture<'a> =
    Pin<Box<dyn Future<Output = Result<HttpResponse, HttpError>> + Send + 'a>>;

pub trait HttpTransport {
    fn execute(&self, request: Request, options: RequestOptions) -> HttpFuture<'_>;
}

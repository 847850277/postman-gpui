use std::sync::Arc;

use postman_http::{
    request::{HttpMethod, Request, RequestBody},
    HttpError,
};
use reqwest::{Client, ClientBuilder, Response};

use crate::{cookie_store::ApplicationCookieJar, multipart::build_multipart};

const DEFAULT_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

#[derive(Clone)]
pub struct RequestClient {
    client: Client,
    range_client: Client,
    cookie_jar: Arc<ApplicationCookieJar>,
}

impl RequestClient {
    pub fn try_new() -> Result<Self, HttpError> {
        let cookie_jar = Arc::new(ApplicationCookieJar::default());
        let client = base_client_builder(cookie_jar.clone())
            // Keep response negotiation explicit. Reqwest adds one Accept-Encoding value only
            // when the user did not supply one, then transparently decodes the response and
            // removes the stale wire encoding/length headers.
            .gzip(true)
            .deflate(true)
            .brotli(true)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| {
                HttpError::invalid_request(format!(
                    "failed to initialize the default HTTP client: {error}"
                ))
            })?;
        // Reqwest 0.12 currently adds Accept-Encoding even when Range is present. Keep a second
        // connection pool with the same cookie provider for that one negotiation-suppressed path.
        let range_client = base_client_builder(cookie_jar.clone())
            .no_gzip()
            .no_deflate()
            .no_brotli()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| {
                HttpError::invalid_request(format!(
                    "failed to initialize the Range HTTP client: {error}"
                ))
            })?;

        Ok(RequestClient {
            client,
            range_client,
            cookie_jar,
        })
    }

    pub fn cookie_snapshot(&self) -> Vec<(String, String)> {
        self.cookie_jar.snapshot()
    }

    pub fn clear_cookies(&self) -> usize {
        self.cookie_jar.clear()
    }

    /// Sends exactly one HTTP exchange.
    ///
    /// Redirect orchestration, request deadlines, cookie capture, and response conversion belong
    /// to the `HttpTransport` implementation. This method only translates the typed request into
    /// reqwest and returns its raw response.
    pub(crate) async fn send_once(&self, request: &Request) -> Result<Response, HttpError> {
        let client = self.select_client(request);

        let mut builder = match request.method {
            HttpMethod::GET => client.get(&request.url),
            HttpMethod::POST => client.post(&request.url),
            HttpMethod::PUT => client.put(&request.url),
            HttpMethod::DELETE => client.delete(&request.url),
            HttpMethod::PATCH => client.patch(&request.url),
            HttpMethod::HEAD => client.head(&request.url),
            HttpMethod::OPTIONS => client.request(reqwest::Method::OPTIONS, &request.url),
        };

        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }

        builder = match &request.body {
            RequestBody::None => builder,

            RequestBody::Json(body) | RequestBody::Raw(body) | RequestBody::UrlEncoded(body) => {
                builder.body(body.clone())
            }

            RequestBody::Multipart(parts) => builder.multipart(build_multipart(parts).await?),
        };

        builder.send().await.map_err(map_reqwest_error)
    }

    fn select_client(&self, request: &Request) -> &Client {
        let has_range = request
            .headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("range"));

        let has_accept_encoding = request
            .headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("accept-encoding"));

        if has_range && !has_accept_encoding {
            &self.range_client
        } else {
            &self.client
        }
    }
}

fn base_client_builder(cookie_jar: Arc<ApplicationCookieJar>) -> ClientBuilder {
    Client::builder()
        .user_agent(DEFAULT_USER_AGENT)
        // Both connection pools belong to one application session. The observable provider
        // retains cookies from intermediate redirects and adds them to later requests.
        .cookie_provider(cookie_jar)
}

pub(crate) fn map_reqwest_error(error: reqwest::Error) -> HttpError {
    if error.is_builder() {
        HttpError::invalid_request(error.to_string())
    } else if error.is_decode() {
        HttpError::invalid_response(error.to_string())
    } else {
        HttpError::network(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_configuration_builds_both_clients() {
        RequestClient::try_new().expect("the built-in client configuration should be valid");
    }
}

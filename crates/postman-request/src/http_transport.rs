use std::time::{Duration, Instant};

use encoding_rs::{Encoding, UTF_8};
use mime::Mime;
use postman_http::{
    request::{RedirectPolicy, Request, RequestOptions, MAX_REDIRECT_HOPS},
    response::{HttpResponse, RedirectHop},
    HttpError, HttpTransport,
};
use reqwest::{
    header::{CONTENT_TYPE, LOCATION},
    Response,
};

use crate::{
    client::{map_reqwest_error, RequestClient},
    cookie_store::capture_response_cookies,
    redirect::{apply_redirect_semantics, is_redirect_status},
};

impl HttpTransport for RequestClient {
    async fn execute(
        &self,
        request: Request,
        options: RequestOptions,
    ) -> Result<HttpResponse, HttpError> {
        if request.url.trim().is_empty() {
            return Err(HttpError::EmptyUrl);
        }

        let started = Instant::now();
        let timeout_ms = options.timeout_ms.filter(|value| *value > 0);

        let request_future =
            capture_response_cookies(send_following_redirects(self, request, options));
        let (result, stored_cookies) = match timeout_ms {
            Some(timeout_ms) => {
                match tokio::time::timeout(Duration::from_millis(timeout_ms), request_future).await
                {
                    Ok(result) => result,
                    Err(_) => {
                        return Err(HttpError::Timeout { timeout_ms });
                    }
                }
            }
            None => request_future.await,
        };

        let mut response = result?.with_stored_cookies(stored_cookies);
        response.elapsed_ms = started.elapsed().as_millis();

        Ok(response)
    }
}

async fn send_following_redirects(
    client: &RequestClient,
    mut request: Request,
    options: RequestOptions,
) -> Result<HttpResponse, HttpError> {
    let mut redirect_chain = Vec::new();
    let max_hops = options.max_redirect_hops.clamp(1, MAX_REDIRECT_HOPS);
    let max_response_body_bytes = client.max_response_body_bytes();

    loop {
        let response = client.send_once(&request).await?;
        let status = response.status();
        let response_url = response.url().clone();
        let location = response
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);

        if !is_redirect_status(status) {
            if !redirect_chain.is_empty() {
                redirect_chain.push(RedirectHop::terminal(
                    status.as_u16(),
                    response_url.to_string(),
                ));
            }

            return finish_response(response, redirect_chain, max_response_body_bytes).await;
        }

        redirect_chain.push(RedirectHop::new(
            status.as_u16(),
            response_url.to_string(),
            location.clone(),
        ));

        if options.redirect_policy == RedirectPolicy::DoNotFollow {
            return finish_response(response, redirect_chain, max_response_body_bytes).await;
        }

        let Some(location) = location else {
            return finish_response(response, redirect_chain, max_response_body_bytes).await;
        };

        let Ok(next_url) = response_url.join(&location) else {
            return finish_response(response, redirect_chain, max_response_body_bytes).await;
        };

        if !matches!(next_url.scheme(), "http" | "https") {
            return Err(HttpError::invalid_response(format!(
                "unsupported redirect scheme: {}",
                next_url.scheme()
            )));
        }

        if redirect_chain.len() >= max_hops as usize {
            return Err(HttpError::RedirectLimitExceeded {
                max_hops,
                chain: redirect_chain,
            });
        }

        apply_redirect_semantics(&mut request, status, &response_url, next_url);
    }
}

async fn finish_response(
    mut response: Response,
    redirect_chain: Vec<RedirectHop>,
    max_response_body_bytes: u64,
) -> Result<HttpResponse, HttpError> {
    let status = response.status().as_u16();
    let encoding = response_encoding(&response);
    let content_length = response.content_length();
    // Size hints provide an early rejection when available, but are absent for chunked and
    // transparently decoded responses. The streaming check below remains authoritative.
    if let Some(size_bytes) = content_length {
        ensure_response_body_size(max_response_body_bytes, size_bytes)?;
    }
    let headers = response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.to_string(), value.to_string()))
        })
        .collect();
    let initial_capacity = content_length
        .and_then(|size| usize::try_from(size).ok())
        .unwrap_or(0);
    let mut body_bytes = Vec::with_capacity(initial_capacity);
    // Reqwest yields bytes after gzip/deflate/Brotli decoding, so this also bounds compressed
    // responses whose wire-level Content-Length is small or removed during decoding.
    while let Some(chunk) = response.chunk().await.map_err(map_reqwest_error)? {
        let size_bytes = (body_bytes.len() as u64).saturating_add(chunk.len() as u64);
        ensure_response_body_size(max_response_body_bytes, size_bytes)?;
        body_bytes.extend_from_slice(&chunk);
    }
    let (body, _, _) = encoding.decode(&body_bytes);

    Ok(HttpResponse::new(status, headers, body.into_owned()).with_redirect_chain(redirect_chain))
}

fn ensure_response_body_size(limit_bytes: u64, size_bytes: u64) -> Result<(), HttpError> {
    if size_bytes > limit_bytes {
        Err(HttpError::ResponseTooLarge {
            limit_bytes,
            size_bytes,
        })
    } else {
        Ok(())
    }
}

fn response_encoding(response: &Response) -> &'static Encoding {
    response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<Mime>().ok())
        .and_then(|mime| {
            mime.get_param("charset")
                .and_then(|charset| Encoding::for_label(charset.as_str().as_bytes()))
        })
        .unwrap_or(UTF_8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::GzEncoder, Compression};
    use postman_http::request::HttpMethod;
    use std::io::Write;

    fn execute_get(client: &RequestClient, url: String) -> Result<HttpResponse, HttpError> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("the test runtime should be available")
            .block_on(client.execute(
                Request::new(HttpMethod::GET, url),
                RequestOptions::default(),
            ))
    }

    #[test]
    fn malformed_location_returns_the_observed_redirect_response() {
        let mut server = mockito::Server::new();
        let redirect = server
            .mock("GET", "/malformed-location")
            .with_status(302)
            .with_header("location", "http://[::1")
            .with_body("redirect target is malformed")
            .create();
        let client = RequestClient::try_new("postman-request-test/0.1.0")
            .expect("the test client configuration should be valid");
        let response = execute_get(&client, format!("{}/malformed-location", server.url()))
            .expect("a malformed Location should terminate redirect following, not the request");

        assert_eq!(response.status, 302);
        assert_eq!(response.body, "redirect target is malformed");
        assert_eq!(response.redirect_chain.len(), 1);
        assert_eq!(
            response.redirect_chain[0].location.as_deref(),
            Some("http://[::1")
        );
        redirect.assert();
    }

    #[test]
    fn declared_response_body_larger_than_the_limit_is_rejected() {
        let mut server = mockito::Server::new();
        let oversized = server
            .mock("GET", "/declared-oversized")
            .with_status(200)
            .with_body("12345")
            .create();
        let client = RequestClient::try_new("postman-request-test/0.1.0")
            .expect("the test client configuration should be valid")
            .with_max_response_body_bytes(4);

        let error = execute_get(&client, format!("{}/declared-oversized", server.url()))
            .expect_err("Content-Length above the configured limit must be rejected");

        assert_eq!(
            error,
            HttpError::ResponseTooLarge {
                limit_bytes: 4,
                size_bytes: 5,
            }
        );
        oversized.assert();
    }

    #[test]
    fn decoded_response_body_larger_than_the_limit_is_rejected_while_streaming() {
        let decoded = vec![b'x'; 1_024];
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(&decoded)
            .expect("the compressed fixture should be writable");
        let encoded = encoder
            .finish()
            .expect("the compressed fixture should be encodable");
        assert!(encoded.len() < 64, "the wire body must fit below the limit");

        let mut server = mockito::Server::new();
        let oversized = server
            .mock("GET", "/decoded-oversized")
            .with_status(200)
            .with_header("content-encoding", "gzip")
            .with_body(encoded)
            .create();
        let client = RequestClient::try_new("postman-request-test/0.1.0")
            .expect("the test client configuration should be valid")
            .with_max_response_body_bytes(64);

        let error = execute_get(&client, format!("{}/decoded-oversized", server.url()))
            .expect_err("the decoded stream must be measured instead of its compressed length");

        assert!(matches!(
            error,
            HttpError::ResponseTooLarge {
                limit_bytes: 64,
                size_bytes
            } if size_bytes > 64
        ));
        oversized.assert();
    }
}

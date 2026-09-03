use std::time::{Duration, Instant};

use postman_http::{
    request::{RedirectPolicy, Request, RequestOptions, MAX_REDIRECT_HOPS},
    response::{HttpResponse, RedirectHop},
    HttpError, HttpTransport,
};
use reqwest::{header::LOCATION, Response};

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

            return finish_response(response, redirect_chain).await;
        }

        redirect_chain.push(RedirectHop::new(
            status.as_u16(),
            response_url.to_string(),
            location.clone(),
        ));

        if options.redirect_policy == RedirectPolicy::DoNotFollow {
            return finish_response(response, redirect_chain).await;
        }

        let Some(location) = location else {
            return finish_response(response, redirect_chain).await;
        };

        let next_url = response_url.join(&location).map_err(|error| {
            HttpError::invalid_response(format!("invalid redirect location: {error}"))
        })?;

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
    response: Response,
    redirect_chain: Vec<RedirectHop>,
) -> Result<HttpResponse, HttpError> {
    let status = response.status().as_u16();
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
    let body = response.text().await.map_err(map_reqwest_error)?;

    Ok(HttpResponse::new(status, headers, body).with_redirect_chain(redirect_chain))
}

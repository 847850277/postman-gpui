use postman_http::request::{HttpMethod, Request, RequestBody};
use reqwest::{StatusCode, Url};

pub(crate) fn is_redirect_status(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::MOVED_PERMANENTLY
            | StatusCode::FOUND
            | StatusCode::SEE_OTHER
            | StatusCode::TEMPORARY_REDIRECT
            | StatusCode::PERMANENT_REDIRECT
    )
}

pub(crate) fn apply_redirect_semantics(
    request: &mut Request,
    status: StatusCode,
    previous_url: &Url,
    next_url: Url,
) {
    let drop_payload = match status {
        StatusCode::MOVED_PERMANENTLY | StatusCode::FOUND => request.method == HttpMethod::POST,
        StatusCode::SEE_OTHER => request.method != HttpMethod::HEAD,
        StatusCode::TEMPORARY_REDIRECT | StatusCode::PERMANENT_REDIRECT => false,
        _ => false,
    };

    if drop_payload {
        request.method = HttpMethod::GET;
        request.body = RequestBody::None;
        request.headers.retain(|(name, _)| {
            !matches!(
                name.to_ascii_lowercase().as_str(),
                "content-type" | "content-length" | "content-encoding" | "transfer-encoding"
            )
        });
    }

    let cross_origin = previous_url.host_str() != next_url.host_str()
        || previous_url.port_or_known_default() != next_url.port_or_known_default();
    if cross_origin {
        request.headers.retain(|(name, _)| {
            !matches!(
                name.to_ascii_lowercase().as_str(),
                "authorization" | "cookie" | "cookie2" | "proxy-authorization"
            )
        });
    }

    request.url = next_url.to_string();
}

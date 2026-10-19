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

    let cross_origin = previous_url.scheme() != next_url.scheme()
        || previous_url.host_str() != next_url.host_str()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn credentialed_request(url: &str) -> Request {
        let mut request = Request::new(HttpMethod::GET, url);
        request.headers = vec![
            ("Authorization".into(), "Bearer secret".into()),
            ("cOoKiE".into(), "sid=secret".into()),
            ("Cookie2".into(), "sid=secret".into()),
            ("Proxy-Authorization".into(), "Basic secret".into()),
            ("Accept".into(), "application/json".into()),
        ];
        request
    }

    #[test]
    fn cross_origin_redirects_strip_credentials_including_same_port_downgrades() {
        for (previous, next) in [
            ("https://example.com/", "http://example.com:443/"),
            ("https://example.com:8443/", "http://example.com:8443/"),
            ("http://example.com:8443/", "https://example.com:8443/"),
            ("https://example.com/", "https://other.example.com/"),
            ("https://example.com/", "https://example.com:8443/"),
        ] {
            let mut request = credentialed_request(previous);
            apply_redirect_semantics(
                &mut request,
                StatusCode::FOUND,
                &Url::parse(previous).unwrap(),
                Url::parse(next).unwrap(),
            );
            assert_eq!(
                request.headers,
                [("Accept".into(), "application/json".into())],
                "credentials leaked from {previous} to {next}"
            );
            assert_eq!(request.url, next);
        }
    }

    #[test]
    fn same_origin_redirects_preserve_credentials_with_explicit_default_ports() {
        for (previous, next) in [
            ("https://example.com/", "https://example.com:443/next"),
            ("http://example.com:80/", "http://example.com/next"),
        ] {
            let mut request = credentialed_request(previous);
            let headers = request.headers.clone();
            apply_redirect_semantics(
                &mut request,
                StatusCode::TEMPORARY_REDIRECT,
                &Url::parse(previous).unwrap(),
                Url::parse(next).unwrap(),
            );
            assert_eq!(request.headers, headers);
        }
    }
}

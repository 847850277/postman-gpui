use crate::error::HttpError;

/// One observed HTTP response in a redirect exchange.
///
/// Redirect responses retain the server's original `Location` value while the terminal response
/// has no location. URLs are the absolute request URLs used for each individual exchange.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedirectHop {
    pub status: u16,
    pub url: String,
    pub location: Option<String>,
}

impl RedirectHop {
    pub fn new(status: u16, url: impl Into<String>, location: Option<impl Into<String>>) -> Self {
        Self {
            status,
            url: url.into(),
            location: location.map(Into::into),
        }
    }

    pub fn terminal(status: u16, url: impl Into<String>) -> Self {
        Self::new(status, url, None::<String>)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub elapsed_ms: u128,
    pub stored_cookies: Vec<(String, String)>,
    pub redirect_chain: Vec<RedirectHop>,
}

impl HttpResponse {
    pub fn new(status: u16, headers: Vec<(String, String)>, body: String) -> Self {
        HttpResponse {
            status,
            headers,
            body,
            elapsed_ms: 0,
            stored_cookies: Vec::new(),
            redirect_chain: Vec::new(),
        }
    }

    /// Attaches cookies captured by the concrete transport while preserving the response value.
    pub fn with_stored_cookies(mut self, stored_cookies: Vec<(String, String)>) -> Self {
        self.stored_cookies = stored_cookies;
        self
    }

    /// Attaches the observed redirect exchange assembled by the concrete transport.
    pub fn with_redirect_chain(mut self, redirect_chain: Vec<RedirectHop>) -> Self {
        self.redirect_chain = redirect_chain;
        self
    }

    pub fn status(&self) -> u16 {
        self.status
    }

    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn stored_cookies(&self) -> &[(String, String)] {
        &self.stored_cookies
    }

    pub fn redirect_chain(&self) -> &[RedirectHop] {
        &self.redirect_chain
    }

    pub fn from_raw_response(raw_response: &str) -> Result<Self, HttpError> {
        let (header_part, body_part) = raw_response.split_once("\r\n\r\n").ok_or_else(|| {
            HttpError::invalid_response("headers and body are not separated by CRLF")
        })?;

        let status_line = header_part
            .lines()
            .next()
            .ok_or_else(|| HttpError::invalid_response("missing status line"))?;
        let status_code: u16 = status_line
            .split_whitespace()
            .nth(1)
            .ok_or_else(|| HttpError::invalid_response("missing status code"))?
            .parse()
            .map_err(|_| HttpError::invalid_response("invalid status code"))?;

        let headers = header_part
            .lines()
            .skip(1)
            .filter_map(|line| {
                let mut parts = line.splitn(2, ": ");
                if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
                    Some((key.to_string(), value.to_string()))
                } else {
                    None
                }
            })
            .collect();

        Ok(HttpResponse::new(
            status_code,
            headers,
            body_part.to_string(),
        ))
    }

    pub fn success(body: String) -> Self {
        Self {
            status: 200,
            headers: Vec::new(),
            body,
            elapsed_ms: 0,
            stored_cookies: Vec::new(),
            redirect_chain: Vec::new(),
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            status: 0,
            headers: Vec::new(),
            body: message,
            elapsed_ms: 0,
            stored_cookies: Vec::new(),
            redirect_chain: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_response_parser_returns_a_typed_response() {
        let response = HttpResponse::from_raw_response(
            "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\n\r\n{\"ok\":true}",
        )
        .expect("raw response should parse");

        assert_eq!(response.status, 201);
        assert_eq!(
            response.headers,
            vec![("Content-Type".to_owned(), "application/json".to_owned())]
        );
        assert_eq!(response.body, "{\"ok\":true}");
    }

    #[test]
    fn raw_response_parser_returns_http_error_for_malformed_input() {
        let error = HttpResponse::from_raw_response("not an HTTP response").unwrap_err();

        assert!(matches!(error, HttpError::InvalidResponse(_)));
    }

    #[test]
    fn raw_response_body_may_contain_additional_blank_lines() {
        let response = HttpResponse::from_raw_response(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\nfirst\r\n\r\nsecond",
        )
        .expect("body delimiters should not invalidate the response");

        assert_eq!(response.body, "first\r\n\r\nsecond");
    }
}

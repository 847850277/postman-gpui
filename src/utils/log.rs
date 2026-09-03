use postman_http::request::{HttpMethod, MultipartValue, RequestBody};

pub(crate) fn display_header_value<'a>(name: &str, value: &'a str) -> &'a str {
    if is_sensitive_name(name) {
        "[REDACTED]"
    } else {
        value
    }
}

fn is_sensitive_name(name: &str) -> bool {
    let compact_name: String = name
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    matches!(
        compact_name.as_str(),
        "authorization"
            | "proxyauthorization"
            | "cookie"
            | "cookies"
            | "setcookie"
            | "apikey"
            | "session"
            | "sessionid"
    ) || compact_name.contains("token")
        || compact_name.contains("secret")
        || compact_name.contains("password")
        || compact_name.contains("credential")
}

pub(crate) fn format_http_request(
    method: HttpMethod,
    url: &str,
    headers: &[(String, String)],
    body: &RequestBody,
) -> String {
    format!(
        "HTTP REQUEST\nmethod: {method}\nurl: {}\nheaders:\n{}\nbody:\n{}",
        display_detailed_url_for_log(url),
        format_headers(headers),
        format_request_body(body)
    )
}

pub(crate) fn format_http_response(
    status: u16,
    elapsed_ms: u128,
    headers: &[(String, String)],
    body: &str,
) -> String {
    format!(
        "HTTP RESPONSE\nstatus: {status}\nelapsed: {elapsed_ms} ms\nheaders:\n{}\nbody:\n{}",
        format_headers(headers),
        format_response_body(body)
    )
}

fn display_detailed_url_for_log(value: &str) -> String {
    let Ok(url) = reqwest::Url::parse(value) else {
        return "[INVALID URL]".to_string();
    };
    let Some(host) = url.host_str() else {
        return "[URL WITHOUT HOST]".to_string();
    };

    let mut output = format!("{}://{host}", url.scheme());
    if let Some(port) = url.port() {
        output.push_str(&format!(":{port}"));
    }
    output.push_str(url.path());

    let query = url
        .query_pairs()
        .map(|(name, value)| {
            let value = if is_sensitive_name(&name) {
                "[REDACTED]".to_string()
            } else {
                escape_inline(&value)
            };
            format!("{}={value}", escape_inline(&name))
        })
        .collect::<Vec<_>>();
    if !query.is_empty() {
        output.push('?');
        output.push_str(&query.join("&"));
    }
    output
}

fn format_headers(headers: &[(String, String)]) -> String {
    if headers.is_empty() {
        return "  (none)".to_string();
    }
    headers
        .iter()
        .map(|(name, value)| {
            format!(
                "  {name}: {}",
                escape_inline(display_header_value(name, value))
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_request_body(body: &RequestBody) -> String {
    match body {
        RequestBody::None => "  kind: none\n  content: (empty)".to_string(),
        RequestBody::Json(value) => format!(
            "  kind: application/json\n  content:\n{}",
            indent(&format_json_for_log(value), 4)
        ),
        RequestBody::Raw(value) => {
            format!("  kind: raw\n  content:\n{}", indent_or_empty(value, 4))
        }
        RequestBody::UrlEncoded(value) => {
            let fields = form_urlencoded::parse(value.as_bytes())
                .map(|(name, value)| {
                    let value = if is_sensitive_name(&name) {
                        "[REDACTED]".to_string()
                    } else {
                        value.into_owned()
                    };
                    format!("{} = {value:?}", name)
                })
                .collect::<Vec<_>>();
            let content = if fields.is_empty() {
                "(empty)".to_string()
            } else {
                fields.join("\n")
            };
            format!(
                "  kind: application/x-www-form-urlencoded\n  fields:\n{}",
                indent(&content, 4)
            )
        }
        RequestBody::Multipart(parts) => {
            let content = if parts.is_empty() {
                "(empty)".to_string()
            } else {
                parts
                    .iter()
                    .enumerate()
                    .map(|(index, part)| match &part.value {
                        MultipartValue::Text(value) => {
                            let value = if is_sensitive_name(&part.name) {
                                "[REDACTED]"
                            } else {
                                value
                            };
                            format!(
                                "part[{index}]:\n  name: {}\n  type: text\n  value: {value:?}",
                                part.name
                            )
                        }
                        MultipartValue::File {
                            path,
                            file_name,
                            content_type,
                        } => format!(
                            "part[{index}]:\n  name: {}\n  type: file\n  path: {}\n  file_name: {}\n  content_type: {}",
                            part.name,
                            path.display(),
                            file_name.as_deref().unwrap_or("(inferred)"),
                            content_type.as_deref().unwrap_or("(inferred)")
                        ),
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            format!(
                "  kind: multipart/form-data\n  parts:\n{}",
                indent(&content, 4)
            )
        }
    }
}

fn format_response_body(body: &str) -> String {
    if body.is_empty() {
        "  (empty)".to_string()
    } else {
        indent(&format_json_for_log(body), 2)
    }
}

fn format_json_for_log(value: &str) -> String {
    let Ok(mut json) = serde_json::from_str::<serde_json::Value>(value) else {
        return value.to_string();
    };
    redact_json_value(&mut json);
    serde_json::to_string_pretty(&json).unwrap_or_else(|_| value.to_string())
}

fn redact_json_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(values) => {
            for (name, value) in values {
                if is_sensitive_name(name) {
                    *value = serde_json::Value::String("[REDACTED]".to_string());
                } else {
                    redact_json_value(value);
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                redact_json_value(value);
            }
        }
        _ => {}
    }
}

fn escape_inline(value: &str) -> String {
    value.replace('\r', "\\r").replace('\n', "\\n")
}

fn indent_or_empty(value: &str, spaces: usize) -> String {
    if value.is_empty() {
        " ".repeat(spaces) + "(empty)"
    } else {
        indent(value, spaces)
    }
}

fn indent(value: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    value
        .lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn display_url_for_log(value: &str) -> String {
    let Ok(url) = reqwest::Url::parse(value) else {
        return "[INVALID URL]".to_string();
    };
    let Some(host) = url.host_str() else {
        return "[URL WITHOUT HOST]".to_string();
    };
    let mut output = format!("{}://{host}", url.scheme());
    if let Some(port) = url.port() {
        output.push_str(&format!(":{port}"));
    }
    output.push_str(url.path());
    if url.query().is_some() {
        output.push_str("?[REDACTED]");
    }
    if url.fragment().is_some() {
        output.push_str("#[REDACTED]");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use postman_http::request::MultipartPart;
    use std::path::PathBuf;

    #[test]
    fn sensitive_header_values_are_redacted_before_logging() {
        assert_eq!(
            display_header_value("Authorization", "Bearer secret"),
            "[REDACTED]"
        );
        assert_eq!(
            display_header_value("cookie", "session=secret"),
            "[REDACTED]"
        );
        assert_eq!(display_header_value("X-Trace", "visible"), "visible");
        assert_eq!(display_header_value("X-Auth-Token", "secret"), "[REDACTED]");
    }

    #[test]
    fn urls_are_logged_without_credentials_or_query_values() {
        assert_eq!(
            display_url_for_log("https://user:pass@example.com/search?api_key=secret#token"),
            "https://example.com/search?[REDACTED]#[REDACTED]"
        );
    }

    #[test]
    fn detailed_request_log_includes_parameters_and_redacts_credentials() {
        let body = RequestBody::Multipart(vec![
            MultipartPart::text("comments", "Ring the bell"),
            MultipartPart::text("password", "do not log me"),
            MultipartPart {
                name: "attachment".to_string(),
                value: MultipartValue::File {
                    path: PathBuf::from("/tmp/report.txt"),
                    file_name: Some("report.txt".to_string()),
                    content_type: Some("text/plain".to_string()),
                },
            },
        ]);
        let output = format_http_request(
            HttpMethod::POST,
            "https://user:pass@example.com/post?source=gpui&api_key=secret",
            &[
                ("X-Trace".to_string(), "visible".to_string()),
                ("Authorization".to_string(), "Bearer secret".to_string()),
            ],
            &body,
        );

        assert!(output.contains("method: POST"));
        assert!(output.contains("source=gpui"));
        assert!(output.contains("api_key=[REDACTED]"));
        assert!(!output.contains("user:pass"));
        assert!(output.contains("X-Trace: visible"));
        assert!(!output.contains("Bearer secret"));
        assert!(output.contains("value: \"Ring the bell\""));
        assert!(!output.contains("do not log me"));
        assert!(output.contains("path: /tmp/report.txt"));
    }

    #[test]
    fn detailed_response_log_includes_headers_and_redacted_json_body() {
        let output = format_http_response(
            200,
            42,
            &[
                ("Content-Type".to_string(), "application/json".to_string()),
                ("Set-Cookie".to_string(), "session=secret".to_string()),
            ],
            r#"{"form":{"comments":["1234"],"password":"secret"}}"#,
        );

        assert!(output.contains("status: 200"));
        assert!(output.contains("elapsed: 42 ms"));
        assert!(output.contains("Content-Type: application/json"));
        assert!(!output.contains("session=secret"));
        assert!(output.contains("1234"));
        assert!(!output.contains(r#""password": "secret""#));
        assert!(output.contains("[REDACTED]"));
    }

    #[test]
    fn cookie_setting_urls_headers_and_echo_bodies_never_log_session_values() {
        let request = format_http_request(
            HttpMethod::GET,
            "https://httpbingo.org/cookies/set?session=cookie-e2e-demo",
            &[("Cookie".to_string(), "session=cookie-e2e-demo".to_string())],
            &RequestBody::None,
        );
        assert!(request.contains("session=[REDACTED]"));
        assert!(request.contains("Cookie: [REDACTED]"));
        assert!(!request.contains("cookie-e2e-demo"));

        let response = format_http_response(
            200,
            12,
            &[(
                "Set-Cookie".to_string(),
                "session=cookie-e2e-demo; Path=/".to_string(),
            )],
            r#"{"cookies":{"session":"cookie-e2e-demo"}}"#,
        );
        assert!(response.contains("Set-Cookie: [REDACTED]"));
        assert!(response.contains(r#""cookies": "[REDACTED]""#));
        assert!(!response.contains("cookie-e2e-demo"));
    }
}

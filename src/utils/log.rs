pub(crate) fn display_header_value<'a>(name: &str, value: &'a str) -> &'a str {
    if is_sensitive_header(name) {
        "[REDACTED]"
    } else {
        value
    }
}

fn is_sensitive_header(name: &str) -> bool {
    let compact_name: String = name
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    matches!(
        compact_name.as_str(),
        "authorization" | "proxyauthorization" | "cookie" | "setcookie" | "apikey"
    ) || compact_name.contains("token")
        || compact_name.contains("secret")
        || compact_name.contains("password")
        || compact_name.contains("credential")
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
}

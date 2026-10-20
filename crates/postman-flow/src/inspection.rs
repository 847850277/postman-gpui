//! Bounded, redacted values for interactive inspection. Execution bindings remain unchanged.
use serde_json::Value;
use std::{collections::BTreeSet, sync::Arc};

mod http;

const MAX_BYTES: usize = 16 * 1024;
const MAX_NODES: usize = 2048;
const MAX_SECRET_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InspectionValue {
    value: Option<Arc<Value>>,
    redacted_paths: Arc<Vec<String>>,
    json: Option<Arc<str>>,
}
impl InspectionValue {
    pub fn value(&self) -> Option<&Value> {
        self.value.as_deref()
    }
    /// JSON pointers; the empty pointer means the whole value was redacted.
    pub fn redacted_paths(&self) -> &[String] {
        &self.redacted_paths
    }
    pub fn is_truncated(&self) -> bool {
        self.value.is_none()
    }
    pub fn kind(&self) -> &'static str {
        if self.redacted_paths.iter().any(String::is_empty) {
            return "redacted";
        }
        match self.value() {
            None => "unavailable (size limit)",
            Some(Value::Null) => "null",
            Some(Value::Bool(_)) => "boolean",
            Some(Value::Number(_)) => "number",
            Some(Value::String(_)) => "string",
            Some(Value::Array(_)) => "array",
            Some(Value::Object(_)) => "object",
        }
    }
    pub fn byte_len(&self) -> usize {
        self.json.as_ref().map_or(0, |s| s.len())
    }
    pub fn json(&self) -> Option<String> {
        self.json.as_ref().map(|v| v.to_string())
    }
}

#[derive(Clone, Default)]
pub(crate) struct Redactor {
    secrets: BTreeSet<String>,
    bytes: usize,
    exhausted: bool,
}
impl std::fmt::Debug for Redactor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Redactor")
            .field("exhausted", &self.exhausted)
            .finish_non_exhaustive()
    }
}
pub(crate) fn sensitive_name(name: &str) -> bool {
    // Recognize credential words, including snake/kebab/camel case, without
    // treating metadata such as token_count or unrelated words as credentials.
    let mut separated = String::with_capacity(name.len());
    let mut chars = name.chars().peekable();
    let mut previous = None::<char>;
    while let Some(ch) = chars.next() {
        if !ch.is_ascii_alphanumeric() {
            separated.push('_');
        } else {
            if ch.is_ascii_uppercase()
                && previous.is_some_and(|p| {
                    p.is_ascii_lowercase()
                        || p.is_ascii_uppercase()
                            && chars.peek().is_some_and(char::is_ascii_lowercase)
                })
            {
                separated.push('_');
            }
            separated.push(ch.to_ascii_lowercase());
        }
        previous = Some(ch);
    }
    let words: Vec<_> = separated.split('_').filter(|s| !s.is_empty()).collect();
    matches!(
        words.concat().as_str(),
        "authorization"
            | "proxyauthorization"
            | "cookie"
            | "cookies"
            | "setcookie"
            | "session"
            | "sessionid"
            | "apikey"
            | "secretkey"
    ) || matches!(
        words.last().copied(),
        Some(
            "token"
                | "tokens"
                | "secret"
                | "secrets"
                | "password"
                | "passwords"
                | "credential"
                | "credentials"
                | "apikey"
        )
    ) || words.ends_with(&["api", "key"])
        || words.ends_with(&["secret", "key"])
        || words.ends_with(&["private", "key"])
        || words.ends_with(&["secret", "access", "key"])
        || words.ends_with(&["password", "hash"])
        || words.ends_with(&["token", "value"])
        || words.ends_with(&["secret", "value"])
        || words.ends_with(&["session", "id"])
}
impl Redactor {
    fn remember_text(&mut self, text: &str) {
        if text.is_empty() || self.exhausted {
            return;
        }
        if text.len() > MAX_SECRET_BYTES {
            self.exhausted = true;
            return;
        }
        let encoded = serde_json::to_string(text).expect("string serialization");
        let url = url::form_urlencoded::byte_serialize(text.as_bytes()).collect::<String>();
        let percent = url.replace('+', "%20");
        for pattern in [text, &encoded[1..encoded.len() - 1], &url, &percent] {
            if self.secrets.contains(pattern) {
                continue;
            }
            if self.bytes.saturating_add(pattern.len()) > MAX_SECRET_BYTES
                || self.secrets.len() >= 2048
            {
                self.exhausted = true;
                return;
            }
            self.bytes += pattern.len();
            self.secrets.insert(pattern.to_owned());
        }
    }
    fn remember_url_component(&mut self, raw: &str) {
        self.remember_text(raw);
        if !self.exhausted {
            self.remember_text(&percent_encoding::percent_decode_str(raw).decode_utf8_lossy());
        }
    }
    pub(crate) fn body(&mut self, body: &postman_http::request::RequestBody) {
        use postman_http::request::{MultipartValue, RequestBody};
        match body {
            RequestBody::UrlEncoded(text) => {
                self.form_secrets(text);
            }
            RequestBody::Multipart(parts) => {
                for part in parts {
                    if sensitive_name(&part.name) {
                        if let MultipartValue::Text(text) = &part.value {
                            self.remember_text(text);
                        }
                    }
                }
            }
            RequestBody::Json(text) | RequestBody::Raw(text) => {
                if let Ok(value) = serde_json::from_str::<Value>(text) {
                    self.discover(&value);
                }
            }
            _ => {}
        }
    }
    fn form_secrets(&mut self, text: &str) {
        for (name, value) in url::form_urlencoded::parse(text.as_bytes()) {
            if sensitive_name(&name) {
                self.remember_text(&value);
            }
        }
    }
    pub(crate) fn remember(&mut self, value: &Value) {
        match value {
            Value::Object(v) => {
                for value in v.values() {
                    self.remember(value);
                }
            }
            Value::Array(v) => {
                for value in v {
                    self.remember(value);
                }
            }
            Value::String(v) => self.remember_text(v),
            v => self.remember_text(&v.to_string()),
        }
    }
    pub(crate) fn discover(&mut self, value: &Value) {
        match value {
            Value::Object(v) => {
                for (key, value) in v {
                    if sensitive_name(key) {
                        self.remember(value);
                        if let Value::String(text) = value {
                            self.headers(&[(key.clone(), text.clone())]);
                        }
                    } else {
                        self.discover(value);
                    }
                }
            }
            Value::Array(v) => {
                for value in v {
                    self.discover(value);
                }
            }
            _ => {}
        }
    }
    pub(crate) fn headers(&mut self, headers: &[(String, String)]) {
        for (name, value) in headers {
            if name.eq_ignore_ascii_case("location") || name.eq_ignore_ascii_case("referer") {
                // Location may be relative. Register its credentials before
                // projecting headers or any echoed values from the same response.
                if let Ok(url) =
                    url::Url::parse("https://inspection.invalid/").and_then(|base| base.join(value))
                {
                    self.url(url.as_str());
                }
            }
            if !sensitive_name(name) {
                continue;
            }
            self.remember_text(value);
            if name.eq_ignore_ascii_case("authorization")
                || name.eq_ignore_ascii_case("proxy-authorization")
            {
                if let Some((_, token)) = value.split_once(char::is_whitespace) {
                    self.remember_text(token.trim());
                }
            }
            // Only cookies contain name=value pairs. '=' in an opaque header
            // (notably Basic's Base64 padding) must not become its own secret.
            if !name.eq_ignore_ascii_case("cookie") && !name.eq_ignore_ascii_case("set-cookie") {
                continue;
            }
            for part in value
                .split(';')
                .take(if name.eq_ignore_ascii_case("set-cookie") {
                    1
                } else {
                    usize::MAX
                })
            {
                if let Some((_, value)) = part.split_once('=') {
                    self.remember_text(value.trim());
                }
            }
        }
    }
    pub(crate) fn url(&mut self, source: &str) {
        if let Ok(url) = url::Url::parse(source) {
            self.remember_url_component(url.username());
            if let Some(p) = url.password() {
                self.remember_url_component(p);
            }
            // Keep both the original wire spelling and form-decoded value.
            for pair in url.query().unwrap_or_default().split('&') {
                if let Some((name, value)) = url::form_urlencoded::parse(pair.as_bytes()).next() {
                    if sensitive_name(&name) {
                        self.remember_text(&value);
                        if let Some((_, raw)) = pair.split_once('=') {
                            self.remember_text(raw);
                        }
                    }
                }
            }
            if let Some(f) = url.fragment() {
                self.remember_url_component(f);
            }
        }
    }
    pub(crate) fn inspect(&self, name: &str, value: &Value, sensitive: bool) -> InspectionValue {
        let mut paths = Vec::new();
        let mut budget = Budget {
            bytes: MAX_BYTES,
            nodes: MAX_NODES,
        };
        let mut value = if sensitive || sensitive_name(name) || self.exhausted {
            paths.push(String::new());
            Some(Value::String("[REDACTED]".into()))
        } else {
            self.visit(value, "", 0, &mut budget, &mut paths)
        };
        if value
            .as_ref()
            .is_some_and(|v| serde_json::to_vec_pretty(v).map_or(true, |v| v.len() > MAX_BYTES))
        {
            value = None;
            paths.clear();
        }
        InspectionValue {
            json: value
                .as_ref()
                .and_then(|v| serde_json::to_string_pretty(v).ok())
                .map(Arc::from),
            value: value.map(Arc::new),
            redacted_paths: Arc::new(paths),
        }
    }
    fn visit(
        &self,
        value: &Value,
        path: &str,
        depth: usize,
        budget: &mut Budget,
        paths: &mut Vec<String>,
    ) -> Option<Value> {
        if depth > 32 || budget.nodes == 0 {
            return None;
        }
        budget.nodes -= 1;
        let redact = |paths: &mut Vec<String>| {
            paths.push(path.to_owned());
            Value::String("[REDACTED]".into())
        };
        match value {
            Value::Object(fields) => {
                let mut result = serde_json::Map::new();
                for (key, value) in fields {
                    budget.consume(key.len())?;
                    // A secret used as a key must not reach display, paths or copied JSON.
                    if self.contains_secret(key) {
                        return Some(redact(paths));
                    }
                    let child = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                    let value = if sensitive_name(key) {
                        paths.push(child);
                        Value::String("[REDACTED]".into())
                    } else {
                        self.visit(value, &child, depth + 1, budget, paths)?
                    };
                    result.insert(key.clone(), value);
                }
                Some(Value::Object(result))
            }
            Value::Array(values) => values
                .iter()
                .enumerate()
                .map(|(i, v)| self.visit(v, &format!("{path}/{i}"), depth + 1, budget, paths))
                .collect::<Option<Vec<_>>>()
                .map(Value::Array),
            Value::String(text) => {
                budget.consume(text.len())?;
                if self.contains_secret(text) {
                    return Some(redact(paths));
                }
                // Encoded JSON strings and URLs can contain credentials under otherwise benign keys.
                if self.encoded_sensitive(text, 0) {
                    return Some(redact(paths));
                }
                if url::Url::parse(text).is_ok_and(|u| {
                    !u.username().is_empty()
                        || u.password().is_some()
                        || u.query().is_some()
                        || u.fragment().is_some()
                }) {
                    return Some(redact(paths));
                }
                Some(value.clone())
            }
            value => {
                let text = value.to_string();
                if self.secrets.contains(&text) {
                    return Some(redact(paths));
                }
                budget.consume(text.len())?;
                Some(value.clone())
            }
        }
    }
    fn encoded_sensitive(&self, text: &str, depth: usize) -> bool {
        if depth >= 8 {
            return true;
        }
        fn protected(value: &Value) -> bool {
            match value {
                Value::Object(fields) => fields
                    .iter()
                    .any(|(k, v)| sensitive_name(k) || protected(v)),
                Value::Array(values) => values.iter().any(protected),
                _ => false,
            }
        }
        match serde_json::from_str::<Value>(text) {
            Ok(Value::String(decoded)) => {
                self.contains_secret(&decoded) || self.encoded_sensitive(&decoded, depth + 1)
            }
            Ok(value) => protected(&value),
            _ => false,
        }
    }
    fn contains_secret(&self, text: &str) -> bool {
        self.secrets.iter().any(|s| text.contains(s))
    }
}
struct Budget {
    bytes: usize,
    nodes: usize,
}
impl Budget {
    fn consume(&mut self, amount: usize) -> Option<()> {
        self.bytes = self.bytes.checked_sub(amount)?;
        Some(())
    }
}
#[cfg(test)]
mod tests;

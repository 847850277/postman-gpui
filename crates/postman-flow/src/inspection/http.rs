//! Safe snapshots of resolved HTTP parameters, never raw wire captures.
use super::{InspectionValue, Redactor};
use postman_http::{
    request::{MultipartValue, Request, RequestBody},
    HttpResponse,
};
use serde_json::{json, Value};

const BODY_BYTES: usize = 8 * 1024;
const PARAMETER_BYTES: usize = 4 * 1024;
const PARAMETER_COUNT: usize = 64;

fn omitted(reason: &str) -> Value {
    json!({"omitted": reason})
}

// Singleton objects preserve duplicate names/order and let the shared redactor
// recognize sensitive field names. Never collapse HTTP parameters into a map.
fn parameters<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Value {
    let mut values = Vec::new();
    let mut bytes = 0_usize;
    for (name, value) in pairs {
        bytes = bytes.saturating_add(name.len()).saturating_add(value.len());
        if bytes > PARAMETER_BYTES || values.len() >= PARAMETER_COUNT {
            return omitted("Parameter size limit exceeded");
        }
        values.push(json!({name: value}));
    }
    Value::Array(values)
}

fn form(text: &str) -> Value {
    if text.len() > BODY_BYTES {
        return omitted("Body size limit exceeded");
    }
    let pairs: Vec<_> = url::form_urlencoded::parse(text.as_bytes()).collect();
    parameters(pairs.iter().map(|(k, v)| (k.as_ref(), v.as_ref())))
}

fn text_body(text: &str) -> Value {
    if text.len() > BODY_BYTES {
        omitted("Body size limit exceeded")
    } else {
        serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.into()))
    }
}

fn form_content_type(headers: &[(String, String)]) -> bool {
    headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("content-type")
            && value.split(';').next().is_some_and(|v| {
                v.trim()
                    .eq_ignore_ascii_case("application/x-www-form-urlencoded")
            })
    })
}

fn request_body(body: &RequestBody) -> Value {
    match body {
        RequestBody::None => json!({"kind": "none"}),
        RequestBody::Json(text) | RequestBody::Raw(text) => json!({
            "kind": if matches!(body, RequestBody::Json(_)) { "json" } else { "raw" },
            "bytes": text.len(), "value": text_body(text),
        }),
        RequestBody::UrlEncoded(text) => json!({
            "kind": "form", "bytes": text.len(), "value": form(text),
        }),
        RequestBody::File(_) => json!({"kind": "file", "omitted": "File contents and local path"}),
        RequestBody::Multipart(parts) => {
            let mut bytes = 0_usize;
            let mut values = Vec::new();
            for part in parts {
                let value = match &part.value {
                    MultipartValue::Text(text) => text.as_str(),
                    MultipartValue::File { .. } => "[File contents and local path omitted]",
                };
                bytes = bytes
                    .saturating_add(part.name.len())
                    .saturating_add(value.len());
                if bytes > BODY_BYTES || values.len() >= PARAMETER_COUNT {
                    return json!({"kind": "multipart", "omitted": "Body size limit exceeded"});
                }
                values.push(json!({&part.name: value}));
            }
            json!({"kind": "multipart", "value": values})
        }
    }
}

impl Redactor {
    pub(crate) fn request(&mut self, request: &Request) -> InspectionValue {
        let body = if let RequestBody::Raw(text) = &request.body {
            if form_content_type(&request.headers) {
                self.form_secrets(text);
                json!({"kind":"form", "bytes":text.len(), "value": form(text)})
            } else {
                request_body(&request.body)
            }
        } else {
            request_body(&request.body)
        };
        let (base, query) = if request.url.len() > PARAMETER_BYTES {
            (
                omitted("URL size limit exceeded"),
                omitted("URL size limit exceeded"),
            )
        } else if let Ok(mut url) = url::Url::parse(&request.url) {
            let query: Vec<_> = url
                .query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
            // Credentials and fragments never enter the snapshot. Query is
            // shown separately so ordinary parameters survive URL redaction.
            let _ = url.set_username("");
            let _ = url.set_password(None);
            url.set_query(None);
            url.set_fragment(None);
            (
                Value::String(url.into()),
                parameters(query.iter().map(|(k, v)| (k.as_str(), v.as_str()))),
            )
        } else {
            (omitted("Invalid URL"), Value::Array(Vec::new()))
        };
        self.inspect("request", &json!({
            "method": request.method.to_string(),
            "url": base,
            "query": query,
            "headers": parameters(request.headers.iter().map(|(k, v)| (k.as_str(), v.as_str()))),
            "body": body,
        }), false)
    }

    pub(crate) fn response(&mut self, response: &HttpResponse) -> InspectionValue {
        let is_form = form_content_type(&response.headers);
        if is_form {
            self.form_secrets(&response.body);
        }
        let body = if response.is_binary() {
            json!({"kind": "binary", "bytes": response.body_bytes().len(), "omitted": "Binary contents"})
        } else {
            let value = if is_form {
                form(&response.body)
            } else {
                text_body(&response.body)
            };
            self.discover(&value);
            json!({"kind": if is_form { "form" } else { "text" }, "bytes": response.body_bytes().len(), "value": value})
        };
        self.inspect("response", &json!({
            "status": response.status,
            "elapsed_ms": u64::try_from(response.elapsed_ms).unwrap_or(u64::MAX),
            "headers": parameters(response.headers.iter().map(|(k, v)| (k.as_str(), v.as_str()))),
            "body": body,
        }), false)
    }
}

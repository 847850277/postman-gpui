//! A deliberately bounded, non-executing POSIX cURL interchange format.
//! See docs/curl.md for supported syntax and native-client differences.
mod export;
mod parse;
mod shell;

pub use export::{export, CurlCode};
pub use parse::parse;

use super::{Request, RequestDraft, RequestOptions};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurlError(String);
impl CurlError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}
impl fmt::Display for CurlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for CurlError {}
type Result<T> = std::result::Result<T, CurlError>;

/// Validated preview and editable intent. No transport or filesystem work is performed.
#[derive(Clone, Debug)]
pub struct CurlImport {
    draft: RequestDraft,
}
impl CurlImport {
    pub fn draft(&self) -> &RequestDraft {
        &self.draft
    }
    pub fn into_draft(self) -> RequestDraft {
        self.draft
    }
    fn new(request: Request, options: RequestOptions) -> Result<Self> {
        let mut draft = RequestDraft::from_request(&request);
        draft.set_request_options(options);
        // A parser must never accept data that the native editor silently changes on Send.
        let actual = draft.construct();
        let actual = actual.request();
        let ordered_fields = |headers: &[(String, String)]| {
            let mut fields = headers
                .iter()
                .map(|(n, v)| (n.to_ascii_lowercase(), v.clone()))
                .collect::<Vec<_>>();
            // Distinct field names may be reordered by the managed auth editor. Repeated
            // values for the same field must retain their original order.
            fields.sort_by(|a, b| a.0.cmp(&b.0));
            fields
        };
        if actual.method != request.method
            || actual.url != request.url
            || actual.body != request.body
            || ordered_fields(&actual.headers) != ordered_fields(&request.headers)
        {
            return Err(CurlError::new("This command cannot be represented without changing its request. Use inline raw data and complete headers."));
        }
        Ok(Self { draft })
    }
}

fn validate_url(value: &str) -> Result<()> {
    if !value.split_once("://").is_some_and(|(scheme, _)| {
        scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
    }) {
        return Err(CurlError::new(
            "Enter a complete URL beginning with http:// or https://.",
        ));
    }
    let url = reqwest::Url::parse(value)
        .map_err(|_| CurlError::new("Enter a complete HTTP or HTTPS URL."))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(CurlError::new(
            "Only absolute HTTP and HTTPS URLs are supported.",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(CurlError::new(
            "Use --user username:password instead of credentials inside the URL.",
        ));
    }
    if value.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(CurlError::new(
            "URL spaces and control characters must be percent encoded.",
        ));
    }
    Ok(())
}
fn validate_header(name: &str, value: &str) -> Result<()> {
    reqwest::header::HeaderName::from_bytes(name.as_bytes())
        .map_err(|_| CurlError::new("Enter a valid HTTP header name."))?;
    reqwest::header::HeaderValue::from_str(value)
        .map_err(|_| CurlError::new("Header values must be single-line HTTP values."))?;
    Ok(())
}

#[cfg(test)]
mod tests;

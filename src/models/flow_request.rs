//! One-way, literal conversion of the same normalized request used by Send.
//! No execution, file access, environment substitution or editor mutation happens here.
use super::{RequestBody, RequestDraft};
use postman_flow::{BodyTemplate, HttpRequestTemplate, RequestOptionOverrides, TextTemplate};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConversionError {
    Scripts,
    Url,
    Headers,
    Json,
    FilePath,
    Multipart,
}
impl ConversionError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Scripts => "Scripts and Tests cannot be copied to a flow yet. Remove them from a separate HTTP draft before adding it.",
            Self::Url => "Enter an absolute HTTP(S) request URL before adding it to a flow.",
            Self::Headers => "A request header is invalid. Fix its name or value in HTTP before adding it to a flow.",
            Self::Json => "The JSON body is invalid. Fix it in HTTP before adding it to a flow.",
            Self::FilePath => "The binary body needs an absolute Unicode file path. Choose the file again in HTTP.",
            Self::Multipart => "Form-data bodies are not supported by Flow yet. Keep this request in HTTP or use a supported body type.",
        }
    }
}

pub(crate) fn convert(
    draft: &RequestDraft,
    pre_request: &str,
    tests: &str,
) -> Result<HttpRequestTemplate, ConversionError> {
    if !pre_request.trim().is_empty() || !tests.trim().is_empty() {
        return Err(ConversionError::Scripts);
    }
    let construction = draft.construct();
    construction.validate().map_err(|_| ConversionError::Url)?;
    let request = construction.request();
    let url = reqwest::Url::parse(&request.url).map_err(|_| ConversionError::Url)?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(ConversionError::Url);
    }
    for (name, value) in &request.headers {
        reqwest::header::HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| ConversionError::Headers)?;
        reqwest::header::HeaderValue::from_str(value).map_err(|_| ConversionError::Headers)?;
    }
    let body = match &request.body {
        RequestBody::None => BodyTemplate::None,
        RequestBody::Json(value) => {
            // Flow validates JSON before execution. Do not change invalid JSON into Raw silently.
            serde_json::from_str::<serde_json::Value>(value).map_err(|_| ConversionError::Json)?;
            BodyTemplate::Json(TextTemplate::literal(value))
        }
        RequestBody::Raw(value) => BodyTemplate::Raw(TextTemplate::literal(value)),
        RequestBody::UrlEncoded(value) => BodyTemplate::UrlEncoded(TextTemplate::literal(value)),
        RequestBody::File(path) => {
            let path_text = path
                .to_str()
                .filter(|_| path.is_absolute())
                .ok_or(ConversionError::FilePath)?;
            BodyTemplate::File(TextTemplate::literal(path_text))
        }
        RequestBody::Multipart(_) => return Err(ConversionError::Multipart),
    };
    let options = construction.request_options();
    Ok(HttpRequestTemplate {
        method: request.method,
        url: TextTemplate::literal(&request.url),
        headers: request
            .headers
            .iter()
            .map(|(name, value)| (TextTemplate::literal(name), TextTemplate::literal(value)))
            .collect(),
        body,
        options: RequestOptionOverrides {
            // None means inheritance in Flow; zero explicitly disables the HTTP deadline.
            timeout_ms: Some(options.timeout_ms.unwrap_or(0)),
            redirect_policy: Some(options.redirect_policy),
            max_redirect_hops: Some(options.max_redirect_hops),
        },
        auth: None,
    })
}

#[cfg(test)]
mod tests;

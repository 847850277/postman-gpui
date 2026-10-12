use super::{shell, validate_header, validate_url, CurlError, Result};
use crate::models::{
    HttpMethod, MultipartValue, RedirectPolicy, RequestBody, RequestDraft, MAX_REDIRECT_HOPS,
};

/// Immutable, selectable Code dialog snapshot. Copy uses precisely this text.
#[derive(Clone, Debug)]
pub struct CurlCode {
    text: String,
    references_files: bool,
    requires_modern_curl: bool,
}
impl CurlCode {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn requires_modern_curl(&self) -> bool {
        self.requires_modern_curl
    }
    pub fn references_files(&self) -> bool {
        self.references_files
    }
}

pub fn export(draft: &RequestDraft) -> Result<CurlCode> {
    let mut draft = draft.clone();
    draft.normalize_for_send();
    if let Some(error) = draft.body_validation_error() {
        return Err(CurlError::new(error));
    }
    let construction = draft.build().map_err(|e| CurlError::new(e.to_string()))?;
    let request = construction.request();
    validate_url(&request.url)?;
    let options = construction.request_options();
    if options.timeout_ms.is_some_and(|v| v > 86_400_000)
        || !(1..=MAX_REDIRECT_HOPS).contains(&options.max_redirect_hops)
    {
        return Err(CurlError::new(
            "Export supports timeouts up to 86400 seconds and redirect limits from 1 to 100.",
        ));
    }
    // Data/form options already select POST. Let curl infer it so ordinary POST
    // redirects also work on older curl versions without a forced custom method.
    let inferred_post = request.method == HttpMethod::POST && !request.body.is_none();
    let requires_modern_curl = options.redirect_policy == RedirectPolicy::Follow
        && !inferred_post
        && !matches!(request.method, HttpMethod::GET | HttpMethod::HEAD);
    let mut lines = vec!["curl --globoff".to_string()];
    // HEAD must select curl's no-response-body behavior, not merely change the method token.
    if request.method == HttpMethod::HEAD {
        lines.push("  --head".into());
    } else if !inferred_post {
        lines.push(format!("  --request {}", request.method));
    }
    lines.push(format!("  --url {}", shell::quote(&request.url)?));
    if options.redirect_policy == RedirectPolicy::Follow {
        lines.push(
            if requires_modern_curl {
                "  --follow"
            } else {
                "  --location"
            }
            .into(),
        );
    }
    lines.push(format!("  --max-redirs {}", options.max_redirect_hops));
    if let Some(ms) = options.timeout_ms {
        lines.push(format!("  --max-time {}.{:03}", ms / 1000, ms % 1000));
    }
    if !request
        .headers
        .iter()
        .any(|(n, _)| n.eq_ignore_ascii_case("range"))
    {
        lines.push("  --compressed".into());
    }
    for (name, value) in &request.headers {
        validate_header(name, value)?;
        lines.push(format!(
            "  --header {}",
            shell::quote(&format!("{name}: {value}"))?
        ));
    }
    // Suppress curl defaults that are intentionally absent from the effective draft.
    if !request
        .headers
        .iter()
        .any(|(n, _)| n.eq_ignore_ascii_case("accept"))
    {
        lines.push("  --header 'Accept:'".into());
    }
    let mut references_files = false;
    match &request.body {
        RequestBody::None => {}
        RequestBody::Json(value) | RequestBody::Raw(value) | RequestBody::UrlEncoded(value) => {
            if !request
                .headers
                .iter()
                .any(|(n, _)| n.eq_ignore_ascii_case("content-type"))
            {
                lines.push("  --header 'Content-Type:'".into());
            }
            // data-raw preserves literal @, Unicode, all whitespace, and explicit empty bodies.
            lines.push(format!("  --data-raw {}", shell::quote(value)?));
        }
        RequestBody::File(path) => {
            references_files = true;
            if !request
                .headers
                .iter()
                .any(|(n, _)| n.eq_ignore_ascii_case("content-type"))
            {
                lines.push("  --header 'Content-Type:'".into());
            }
            lines.push(format!(
                "  --data-binary {}",
                shell::quote(&format!("@{}", file_path(path)?))?
            ));
        }
        RequestBody::Multipart(parts) => {
            if parts.is_empty() {
                return Err(CurlError::new(
                    "Add a form field before exporting a multipart request.",
                ));
            }
            for part in parts {
                if part.name.is_empty() || part.name.contains(['=', '\r', '\n', '\0', '"']) {
                    return Err(CurlError::new(
                        "This multipart field name cannot be expressed safely in cURL form syntax.",
                    ));
                }
                match &part.value {
                    MultipartValue::Text(value) => lines.push(format!(
                        "  --form-string {}",
                        shell::quote(&format!("{}={value}", part.name))?
                    )),
                    MultipartValue::File {
                        path,
                        file_name,
                        content_type,
                    } => {
                        references_files = true;
                        let mut value =
                            format!("{}=@{}", part.name, form_quote(&file_path(path)?)?);
                        // curl guesses a type for file parts; reqwest omits it when unset.
                        // Do not invent metadata when that absence cannot be expressed by curl.
                        let mime = content_type.as_deref().ok_or_else(|| CurlError::new("Set an explicit content type for every multipart file before exporting; cURL would otherwise infer a different type."))?;
                        reqwest::multipart::Part::text("")
                            .mime_str(mime)
                            .map_err(|_| {
                                CurlError::new("Enter a valid multipart file content type.")
                            })?;
                        if mime.contains([';', '"', '\\', '\r', '\n']) {
                            return Err(CurlError::new("Parameterized multipart file content types cannot be exported faithfully."));
                        }
                        value.push_str(&format!(";type={mime}"));
                        if let Some(name) = file_name {
                            value.push_str(&format!(";filename={}", form_quote(name)?));
                        }
                        lines.push(format!("  --form {}", shell::quote(&value)?));
                    }
                }
            }
        }
    }
    Ok(CurlCode {
        text: lines.join(" \\\n"),
        references_files,
        requires_modern_curl,
    })
}
fn file_path(path: &std::path::Path) -> Result<String> {
    let value = path
        .to_str()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| CurlError::new("cURL export needs a nonempty UTF-8 file path."))?;
    // '-' is curl's stdin sentinel, so force a relative filesystem path for relative files.
    Ok(if path.is_absolute() {
        value.to_string()
    } else {
        format!("./{value}")
    })
}
fn form_quote(value: &str) -> Result<String> {
    if value.contains(['\0', '\r', '\n']) {
        return Err(CurlError::new(
            "Multipart filenames and paths cannot contain newlines or null characters.",
        ));
    }
    Ok(format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

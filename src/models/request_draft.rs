use base64::{engine::general_purpose::STANDARD, Engine as _};
use postman_http::request::{
    HttpMethod, MultipartPart, MultipartValue, RedirectPolicy, Request, RequestBody,
    RequestOptions, DEFAULT_MAX_REDIRECT_HOPS, MAX_REDIRECT_HOPS,
};
use std::{fmt, path::PathBuf};

/// Editor-only state captured with a completed request. The effective [`Request`] remains the
/// transport truth; this snapshot preserves disabled and incomplete multipart rows for replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestEditorIntent {
    Multipart(Vec<MultipartEditorPart>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultipartEditorPart {
    pub enabled: bool,
    pub name: String,
    pub value: MultipartValue,
}

/// Authentication scheme managed by the request draft.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorizationKind {
    Bearer,
    Basic,
}

/// Body encoding selected in the editor. The editable payload and encoding are stored together
/// in [`RequestBodyDraft`]; this enum is only a compact value for rendering controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    None,
    Json,
    Raw,
    UrlEncoded,
    Multipart,
    Binary,
}

/// The media type of the independent Raw editor draft.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RawBodyFormat {
    #[default]
    Text,
    Xml,
    Html,
    JavaScript,
}

impl RawBodyFormat {
    pub fn content_type(self) -> &'static str {
        match self {
            Self::Text => "text/plain",
            Self::Xml => "application/xml",
            Self::Html => "text/html",
            Self::JavaScript => "application/javascript",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ManagedHeaderSource {
    Unset,
    Automatic,
    User,
}

/// Explains where one header in the normalized request came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectiveHeaderSource {
    Generated,
    User,
}

/// One enabled header exactly as it will participate in the normalized request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectiveHeader {
    pub name: String,
    pub value: String,
    pub source: EffectiveHeaderSource,
}

/// A row in the params/headers editor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyValueRow {
    pub enabled: bool,
    pub key: String,
    pub value: String,
    /// Optional editor note. Never participates in the outgoing request.
    pub description: String,
}

impl KeyValueRow {
    pub fn enabled(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            enabled: true,
            key: key.into(),
            value: value.into(),
            description: String::new(),
        }
    }
}

/// Editable value for one multipart row. Unlike the transport [`MultipartValue`], a file value
/// may intentionally have an empty path while the user is still completing the row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MultipartDraftValue {
    Text(String),
    File {
        path: PathBuf,
        file_name: Option<String>,
        content_type: Option<String>,
    },
}

/// One complete multipart editor row, including state that does not participate in the outgoing
/// request yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultipartDraftPart {
    pub enabled: bool,
    pub name: String,
    pub value: MultipartDraftValue,
}

impl MultipartDraftPart {
    pub fn text(name: impl Into<String>, value: impl Into<String>, enabled: bool) -> Self {
        Self {
            enabled,
            name: name.into(),
            value: MultipartDraftValue::Text(value.into()),
        }
    }

    pub fn file(
        name: impl Into<String>,
        path: impl Into<PathBuf>,
        file_name: Option<String>,
        content_type: Option<String>,
        enabled: bool,
    ) -> Self {
        Self {
            enabled,
            name: name.into(),
            value: MultipartDraftValue::File {
                path: path.into(),
                file_name,
                content_type,
            },
        }
    }
}

/// Authoritative editable body state for one request draft.
///
/// Form variants intentionally retain disabled, blank, duplicate, ordered, and incomplete rows.
/// [`RequestBody`] is derived only by the normalized request-construction path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum RequestBodyDraft {
    #[default]
    None,
    Json(String),
    Raw(String),
    UrlEncoded(Vec<KeyValueRow>),
    Multipart(Vec<MultipartDraftPart>),
    Binary(PathBuf),
}

impl RequestBodyDraft {
    fn kind(&self) -> BodyKind {
        match self {
            Self::None => BodyKind::None,
            Self::Json(_) => BodyKind::Json,
            Self::Raw(_) => BodyKind::Raw,
            Self::UrlEncoded(_) => BodyKind::UrlEncoded,
            Self::Multipart(_) => BodyKind::Multipart,
            Self::Binary(_) => BodyKind::Binary,
        }
    }

    fn empty_for(kind: BodyKind) -> Self {
        match kind {
            BodyKind::None => Self::None,
            BodyKind::Json => Self::Json(String::new()),
            BodyKind::Raw => Self::Raw(String::new()),
            BodyKind::UrlEncoded => Self::UrlEncoded(blank_url_encoded_rows()),
            BodyKind::Multipart => Self::Multipart(blank_multipart_parts()),
            BodyKind::Binary => Self::Binary(PathBuf::new()),
        }
    }

    fn from_request_body(body: &RequestBody) -> Self {
        match body {
            RequestBody::None => Self::None,
            RequestBody::Json(value) => Self::Json(value.clone()),
            RequestBody::Raw(value) => Self::Raw(value.clone()),
            RequestBody::UrlEncoded(value) => Self::UrlEncoded(parse_url_encoded_rows(value)),
            RequestBody::Multipart(parts) => Self::Multipart(nonempty_multipart_parts(
                parts
                    .iter()
                    .map(|part| MultipartDraftPart {
                        enabled: true,
                        name: part.name.clone(),
                        value: match &part.value {
                            MultipartValue::Text(value) => MultipartDraftValue::Text(value.clone()),
                            MultipartValue::File {
                                path,
                                file_name,
                                content_type,
                            } => MultipartDraftValue::File {
                                path: path.clone(),
                                file_name: file_name.clone(),
                                content_type: content_type.clone(),
                            },
                        },
                    })
                    .collect(),
            )),
            RequestBody::File(path) => Self::Binary(path.clone()),
        }
    }

    fn effective_body(&self) -> RequestBody {
        match self {
            Self::None => RequestBody::None,
            Self::Json(value) => RequestBody::Json(value.clone()),
            Self::Raw(value) => RequestBody::Raw(value.clone()),
            Self::Binary(path) => RequestBody::File(path.clone()),
            Self::UrlEncoded(rows) => RequestBody::UrlEncoded(serialize_url_encoded_rows(rows)),
            Self::Multipart(parts) => RequestBody::Multipart(
                parts
                    .iter()
                    .filter(|part| part.enabled && !part.name.trim().is_empty())
                    .filter_map(|part| {
                        let value = match &part.value {
                            MultipartDraftValue::Text(value) => MultipartValue::Text(value.clone()),
                            MultipartDraftValue::File {
                                path,
                                file_name,
                                content_type,
                            } if !path.as_os_str().is_empty() => MultipartValue::File {
                                path: path.clone(),
                                file_name: file_name.clone(),
                                content_type: content_type.clone(),
                            },
                            MultipartDraftValue::File { .. } => return None,
                        };
                        Some(MultipartPart {
                            name: part.name.clone(),
                            value,
                        })
                    })
                    .collect(),
            ),
        }
    }

    fn editor_intent(&self) -> Option<RequestEditorIntent> {
        match self {
            Self::Multipart(parts) => Some(RequestEditorIntent::Multipart(
                parts
                    .iter()
                    .map(|part| MultipartEditorPart {
                        enabled: part.enabled,
                        name: part.name.clone(),
                        value: match &part.value {
                            MultipartDraftValue::Text(value) => MultipartValue::Text(value.clone()),
                            MultipartDraftValue::File {
                                path,
                                file_name,
                                content_type,
                            } => MultipartValue::File {
                                path: path.clone(),
                                file_name: file_name.clone(),
                                content_type: content_type.clone(),
                            },
                        },
                    })
                    .collect(),
            )),
            Self::None | Self::Json(_) | Self::Raw(_) | Self::UrlEncoded(_) | Self::Binary(_) => {
                None
            }
        }
    }

    fn from_editor_intent(intent: &RequestEditorIntent) -> Self {
        match intent {
            RequestEditorIntent::Multipart(parts) => Self::Multipart(nonempty_multipart_parts(
                parts
                    .iter()
                    .map(|part| MultipartDraftPart {
                        enabled: part.enabled,
                        name: part.name.clone(),
                        value: match &part.value {
                            MultipartValue::Text(value) => MultipartDraftValue::Text(value.clone()),
                            MultipartValue::File {
                                path,
                                file_name,
                                content_type,
                            } => MultipartDraftValue::File {
                                path: path.clone(),
                                file_name: file_name.clone(),
                                content_type: content_type.clone(),
                            },
                        },
                    })
                    .collect(),
            )),
        }
    }

    fn editor_text(&self) -> String {
        match self {
            Self::Json(value) | Self::Raw(value) => value.clone(),
            Self::UrlEncoded(rows) => serialize_url_encoded_rows(rows),
            Self::None | Self::Multipart(_) | Self::Binary(_) => String::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct KeyValueDraft {
    key: String,
    value: String,
    description: String,
}

/// Immutable normalized output shared by request previews and Send.
///
/// The transport request is derived from `effective_headers`, so a preview cannot apply a second
/// auth or Body-header policy that differs from the bytes scheduled for execution.
#[derive(Clone, Debug, PartialEq)]
pub struct RequestConstruction {
    request: Request,
    effective_headers: Vec<EffectiveHeader>,
    editor_intent: Option<RequestEditorIntent>,
    request_options: RequestOptions,
}

impl RequestConstruction {
    pub fn request(&self) -> &Request {
        &self.request
    }

    pub fn effective_headers(&self) -> &[EffectiveHeader] {
        &self.effective_headers
    }

    pub fn editor_intent(&self) -> Option<&RequestEditorIntent> {
        self.editor_intent.as_ref()
    }

    pub fn request_options(&self) -> RequestOptions {
        self.request_options
    }

    pub fn validate(&self) -> Result<(), RequestDraftError> {
        if self.request.url.trim().is_empty() {
            Err(RequestDraftError::UrlEmpty)
        } else {
            Ok(())
        }
    }

    pub fn into_parts(
        self,
    ) -> (
        Request,
        Vec<EffectiveHeader>,
        Option<RequestEditorIntent>,
        RequestOptions,
    ) {
        (
            self.request,
            self.effective_headers,
            self.editor_intent,
            self.request_options,
        )
    }
}

/// Validation failures detected without constructing a workspace or starting the transport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestDraftError {
    UrlEmpty,
}

impl fmt::Display for RequestDraftError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UrlEmpty => formatter.write_str("request URL cannot be empty"),
        }
    }
}

impl std::error::Error for RequestDraftError {}

/// Pure source of truth for one editable request.
///
/// This type owns request data and normalization rules but no tab identity, GPUI state,
/// notifications, response lifecycle, or persistence coordination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestDraft {
    method: HttpMethod,
    url: String,
    params: Vec<KeyValueRow>,
    param_draft: KeyValueDraft,
    headers: Vec<KeyValueRow>,
    header_draft: KeyValueDraft,
    body: RequestBodyDraft,
    inactive_bodies: Vec<RequestBodyDraft>,
    raw_format: RawBodyFormat,
    binary_size: Option<u64>,
    content_type_source: ManagedHeaderSource,
    accept_source: ManagedHeaderSource,
    authorization_kind: AuthorizationKind,
    bearer_token: String,
    basic_username: String,
    basic_password: String,
    request_options: RequestOptions,
}

impl RequestDraft {
    pub fn new() -> Self {
        Self::default()
    }

    /// Rehydrates an exact effective request as an editable saved draft.
    pub fn from_request(request: &Request) -> Self {
        let mut draft = Self {
            method: request.method,
            url: request.url.clone(),
            params: parse_query_params(&request.url),
            body: RequestBodyDraft::from_request_body(&request.body),
            // Loading preserves both explicit managed headers and their intentional absence.
            content_type_source: ManagedHeaderSource::User,
            accept_source: ManagedHeaderSource::User,
            ..Self::default()
        };

        if matches!(draft.body, RequestBodyDraft::Raw(_)) {
            if let Some((_, value)) = request
                .headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
            {
                draft.raw_format = match value
                    .split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "application/xml" | "text/xml" => RawBodyFormat::Xml,
                    "text/html" => RawBodyFormat::Html,
                    "application/javascript" | "text/javascript" => RawBodyFormat::JavaScript,
                    _ => RawBodyFormat::Text,
                };
            }
        }

        let authorization = request
            .headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("authorization"))
            .map(|(_, value)| value.as_str());
        let manages_authorization = if let Some(value) = authorization {
            if let Some((username, password)) = decode_basic_credentials(value) {
                draft.authorization_kind = AuthorizationKind::Basic;
                draft.basic_username = username;
                draft.basic_password = password;
                true
            } else if let Some(token) = bearer_token_from_header(value) {
                draft.bearer_token = token;
                true
            } else {
                false
            }
        } else {
            false
        };
        draft.headers = request
            .headers
            .iter()
            .filter(|(key, _)| {
                !(manages_authorization && key.eq_ignore_ascii_case("authorization"))
            })
            .map(|(key, value)| KeyValueRow::enabled(key, value))
            .collect();
        draft
    }

    /// Builds and validates a normalized request without any workspace or UI state.
    pub fn build(&self) -> Result<RequestConstruction, RequestDraftError> {
        let construction = self.construct();
        construction.validate()?;
        Ok(construction)
    }

    /// Produces the immutable normalization result consumed by previews and Send.
    ///
    /// Validation is deliberately separate so the existing transport error lifecycle can still
    /// represent an empty URL as a completed failed send.
    pub fn construct(&self) -> RequestConstruction {
        let mut generated_content_type = self.content_type_source == ManagedHeaderSource::Automatic;
        let mut generated_accept = self.accept_source == ManagedHeaderSource::Automatic;
        let mut effective_headers = self
            .headers
            .iter()
            .filter(|row| row.enabled && header_row_is_complete(row))
            .map(|row| {
                let generated =
                    if generated_content_type && row.key.eq_ignore_ascii_case("content-type") {
                        generated_content_type = false;
                        true
                    } else if generated_accept && row.key.eq_ignore_ascii_case("accept") {
                        generated_accept = false;
                        true
                    } else {
                        false
                    };
                EffectiveHeader {
                    name: row.key.clone(),
                    value: row.value.clone(),
                    source: if generated {
                        EffectiveHeaderSource::Generated
                    } else {
                        EffectiveHeaderSource::User
                    },
                }
            })
            .collect::<Vec<_>>();

        if header_draft_is_complete(&self.header_draft) {
            effective_headers.push(EffectiveHeader {
                name: self.header_draft.key.trim().to_string(),
                value: self.header_draft.value.trim().to_string(),
                source: EffectiveHeaderSource::User,
            });
        }

        if let Some(value) = self.authorization_header_value() {
            effective_headers.retain(|header| !header.name.eq_ignore_ascii_case("authorization"));
            effective_headers.push(EffectiveHeader {
                name: "Authorization".to_string(),
                value,
                source: EffectiveHeaderSource::Generated,
            });
        }

        let body = if self.method.allows_body() {
            if self.content_type_source != ManagedHeaderSource::User
                && !effective_headers
                    .iter()
                    .any(|header| header.name.eq_ignore_ascii_case("content-type"))
            {
                if let Some(value) = self.automatic_content_type() {
                    effective_headers.push(EffectiveHeader {
                        name: "Content-Type".to_string(),
                        value: value.to_string(),
                        source: EffectiveHeaderSource::Generated,
                    });
                }
            }
            self.body.effective_body()
        } else {
            RequestBody::None
        };

        if self.method == HttpMethod::POST
            && self.accept_source != ManagedHeaderSource::User
            && !effective_headers
                .iter()
                .any(|header| header.name.eq_ignore_ascii_case("accept"))
        {
            effective_headers.push(EffectiveHeader {
                name: "Accept".to_string(),
                value: "application/json".to_string(),
                source: EffectiveHeaderSource::Generated,
            });
        }

        let request = Request {
            method: self.method,
            url: self.url.clone(),
            headers: effective_headers
                .iter()
                .map(|header| (header.name.clone(), header.value.clone()))
                .collect(),
            body,
        };
        RequestConstruction {
            request,
            effective_headers,
            editor_intent: self.body.editor_intent(),
            request_options: self.request_options,
        }
    }

    pub fn method(&self) -> HttpMethod {
        self.method
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn url_query_parameter_count(&self) -> usize {
        query_parameter_count(&self.url)
    }

    pub fn enabled_param_count(&self) -> usize {
        self.effective_params()
            .iter()
            .filter(|row| row.enabled && !row.key.trim().is_empty())
            .count()
    }

    pub fn params(&self) -> &[KeyValueRow] {
        &self.params
    }

    pub fn visible_param_row_count(&self) -> usize {
        self.params.len() + 1
    }

    pub fn headers(&self) -> &[KeyValueRow] {
        &self.headers
    }

    pub fn enabled_header_count(&self) -> usize {
        let saved = self
            .headers
            .iter()
            .filter(|row| row.enabled && header_row_is_complete(row))
            .count();
        saved + usize::from(header_draft_is_complete(&self.header_draft))
    }

    pub fn visible_header_row_count(&self) -> usize {
        self.headers.len() + 1
    }

    pub fn param_row_draft(&self) -> (&str, &str) {
        (&self.param_draft.key, &self.param_draft.value)
    }

    pub fn header_row_draft(&self) -> (&str, &str) {
        (&self.header_draft.key, &self.header_draft.value)
    }

    pub fn body_text(&self) -> String {
        self.body.editor_text()
    }

    pub fn body_draft(&self) -> &RequestBodyDraft {
        &self.body
    }

    /// Effective body before method gating. The normalized construction omits it for methods that
    /// do not carry a body while this projection preserves the editor's selected payload.
    pub fn effective_body(&self) -> RequestBody {
        self.body.effective_body()
    }

    pub fn body_kind(&self) -> BodyKind {
        self.body.kind()
    }

    pub fn bearer_token(&self) -> &str {
        &self.bearer_token
    }

    pub fn normalized_bearer_token(&self) -> String {
        normalize_bearer_token(&self.bearer_token)
    }

    pub fn authorization_header_preview(&self) -> Option<String> {
        self.authorization_header_value()
            .map(|value| format!("Authorization: {value}"))
    }

    pub fn authorization_kind(&self) -> AuthorizationKind {
        self.authorization_kind
    }

    pub fn basic_username(&self) -> &str {
        &self.basic_username
    }

    pub fn basic_password(&self) -> &str {
        &self.basic_password
    }

    pub fn timeout_ms(&self) -> u64 {
        self.request_options.timeout_ms.unwrap_or(0)
    }

    pub fn redirect_policy(&self) -> RedirectPolicy {
        self.request_options.redirect_policy
    }

    pub fn max_redirect_hops(&self) -> u32 {
        self.request_options.max_redirect_hops
    }

    pub fn request_options(&self) -> RequestOptions {
        self.request_options
    }

    pub fn editor_intent(&self) -> Option<RequestEditorIntent> {
        self.body.editor_intent()
    }

    pub fn set_method(&mut self, method: HttpMethod) -> bool {
        if self.method == method {
            return false;
        }
        self.method = method;
        if method == HttpMethod::POST && matches!(self.body, RequestBodyDraft::None) {
            let has_json_draft = self
                .inactive_bodies
                .iter()
                .any(|body| body.kind() == BodyKind::Json);
            self.set_body_kind(BodyKind::Json);
            if !has_json_draft {
                self.body = RequestBodyDraft::Json(default_json_body());
            }
        }
        self.sync_automatic_content_type();
        self.sync_automatic_accept();
        true
    }

    pub fn set_url(&mut self, url: impl Into<String>) -> bool {
        let url = url.into();
        if self.url == url {
            return false;
        }
        let mut previous = self.effective_params();
        self.params = parse_query_params(&url)
            .into_iter()
            .map(|mut row| {
                if let Some(index) = previous
                    .iter()
                    .position(|old| old.enabled && old.key == row.key)
                {
                    row.description = previous.remove(index).description;
                }
                row
            })
            .collect();
        self.param_draft = KeyValueDraft::default();
        self.url = url;
        true
    }

    pub fn set_body(&mut self, body: impl Into<String>) -> bool {
        let body = body.into();
        let kind_changed = self.body_kind() == BodyKind::None && self.set_body_kind(BodyKind::Raw);
        let next = match &self.body {
            RequestBodyDraft::Binary(_) => return false,
            RequestBodyDraft::None => RequestBodyDraft::Raw(body),
            RequestBodyDraft::Json(_) => RequestBodyDraft::Json(body),
            RequestBodyDraft::Raw(_) => RequestBodyDraft::Raw(body),
            RequestBodyDraft::UrlEncoded(_) => {
                RequestBodyDraft::UrlEncoded(parse_url_encoded_rows(&body))
            }
            RequestBodyDraft::Multipart(_) => {
                RequestBodyDraft::Multipart(parse_multipart_text_parts(&body))
            }
        };
        let changed = self.body != next;
        self.body = next;
        self.sync_automatic_content_type() || kind_changed || changed
    }

    pub fn clear_body(&mut self) -> bool {
        if self.body_kind() == BodyKind::Binary {
            self.binary_size = None;
        }
        let next = RequestBodyDraft::empty_for(self.body_kind());
        let mut changed = false;
        if self.body != next {
            self.body = next;
            changed = true;
        }
        self.sync_automatic_content_type() || changed
    }

    pub fn set_body_kind(&mut self, body_kind: BodyKind) -> bool {
        let mut changed = false;
        if self.body_kind() != body_kind {
            let next = self
                .inactive_bodies
                .iter()
                .position(|draft| draft.kind() == body_kind)
                .map(|index| self.inactive_bodies.remove(index))
                .unwrap_or_else(|| RequestBodyDraft::empty_for(body_kind));
            let previous = std::mem::replace(&mut self.body, next);
            self.inactive_bodies
                .retain(|draft| draft.kind() != previous.kind());
            self.inactive_bodies.push(previous);
            changed = true;
        }
        self.sync_automatic_content_type() || changed
    }

    /// Checks the selected editor draft before Send, without reading files or changing drafts.
    pub fn body_validation_error(&self) -> Option<String> {
        if !self.method.allows_body() {
            return None;
        }
        match &self.body {
            RequestBodyDraft::Json(body) => serde_json::from_str::<serde_json::Value>(body)
                .err()
                .map(|error| format!("Invalid JSON: {error}")),
            RequestBodyDraft::Binary(path) if path.as_os_str().is_empty() => {
                Some("Choose a file for the binary body.".into())
            }
            RequestBodyDraft::UrlEncoded(rows) => rows
                .iter()
                .enumerate()
                .find(|(_, row)| row.enabled && row.key.trim().is_empty() && !row.value.is_empty())
                .map(|(index, _)| format!("Enter a key for field {}.", index + 1)),
            RequestBodyDraft::Multipart(parts) => {
                parts.iter().enumerate().find_map(|(index, part)| {
                    if !part.enabled {
                        return None;
                    }
                    let has_value = match &part.value {
                        MultipartDraftValue::Text(value) => !value.is_empty(),
                        MultipartDraftValue::File { .. } => true,
                    };
                    if has_value && part.name.trim().is_empty() {
                        return Some(format!("Enter a key for field {}.", index + 1));
                    }
                    if let MultipartDraftValue::File { path, .. } = &part.value {
                        if path.as_os_str().is_empty() {
                            return Some(format!("Choose a file for field {}.", index + 1));
                        }
                    }
                    None
                })
            }
            _ => None,
        }
    }

    pub fn raw_body_format(&self) -> RawBodyFormat {
        self.raw_format
    }

    pub fn set_raw_body_format(&mut self, format: RawBodyFormat) -> bool {
        let changed = self.raw_format != format;
        self.raw_format = format;
        self.sync_automatic_content_type() || changed
    }

    pub fn binary_size(&self) -> Option<u64> {
        self.binary_size
    }

    pub fn set_binary_file(&mut self, path: PathBuf, size: Option<u64>) -> bool {
        let kind_changed = self.set_body_kind(BodyKind::Binary);
        let next = RequestBodyDraft::Binary(path);
        let changed = kind_changed || self.body != next || self.binary_size != size;
        self.body = next;
        self.binary_size = size;
        self.sync_automatic_content_type() || changed
    }

    fn automatic_content_type(&self) -> Option<&'static str> {
        match &self.body {
            RequestBodyDraft::None | RequestBodyDraft::Multipart(_) => None,
            RequestBodyDraft::Json(_) => Some("application/json"),
            RequestBodyDraft::Raw(_) => Some(self.raw_format.content_type()),
            RequestBodyDraft::UrlEncoded(_) => Some("application/x-www-form-urlencoded"),
            RequestBodyDraft::Binary(path) => Some(
                mime_guess::from_path(path)
                    .first_raw()
                    .unwrap_or("application/octet-stream"),
            ),
        }
    }

    pub fn set_url_encoded_rows(&mut self, rows: Vec<KeyValueRow>) -> bool {
        let kind_changed = self.set_body_kind(BodyKind::UrlEncoded);
        let body = RequestBodyDraft::UrlEncoded(nonempty_url_encoded_rows(rows));
        let mut changed = kind_changed;
        if self.body != body {
            self.body = body;
            changed = true;
        }
        self.sync_automatic_content_type() || changed
    }

    pub fn set_multipart_draft_parts(&mut self, parts: Vec<MultipartDraftPart>) -> bool {
        let kind_changed = self.set_body_kind(BodyKind::Multipart);
        let body = RequestBodyDraft::Multipart(nonempty_multipart_parts(parts));
        let mut changed = kind_changed;
        if self.body != body {
            self.body = body;
            changed = true;
        }
        self.sync_automatic_content_type() || changed
    }

    pub fn set_multipart_parts(&mut self, parts: Vec<MultipartPart>) -> bool {
        let body = RequestBodyDraft::from_request_body(&RequestBody::Multipart(parts));
        let RequestBodyDraft::Multipart(parts) = body else {
            unreachable!("multipart conversion must produce a multipart draft");
        };
        self.set_multipart_draft_parts(parts)
    }

    pub fn set_bearer_token(&mut self, token: impl Into<String>) -> bool {
        let token = token.into();
        if self.bearer_token == token {
            false
        } else {
            self.bearer_token = token;
            true
        }
    }

    pub fn set_authorization_kind(&mut self, kind: AuthorizationKind) -> bool {
        if self.authorization_kind == kind {
            false
        } else {
            self.authorization_kind = kind;
            true
        }
    }

    pub fn set_basic_username(&mut self, username: impl Into<String>) -> bool {
        let username = username.into();
        if self.basic_username == username {
            false
        } else {
            self.basic_username = username;
            true
        }
    }

    pub fn set_basic_password(&mut self, password: impl Into<String>) -> bool {
        let password = password.into();
        if self.basic_password == password {
            false
        } else {
            self.basic_password = password;
            true
        }
    }

    pub fn set_timeout_ms(&mut self, timeout_ms: u64) -> bool {
        let timeout_ms = (timeout_ms > 0).then_some(timeout_ms);
        if self.request_options.timeout_ms == timeout_ms {
            false
        } else {
            self.request_options.timeout_ms = timeout_ms;
            true
        }
    }

    pub fn set_redirect_policy(&mut self, redirect_policy: RedirectPolicy) -> bool {
        if self.request_options.redirect_policy == redirect_policy {
            false
        } else {
            self.request_options.redirect_policy = redirect_policy;
            true
        }
    }

    pub fn set_max_redirect_hops(&mut self, max_redirect_hops: u32) -> bool {
        let max_redirect_hops = max_redirect_hops.clamp(1, MAX_REDIRECT_HOPS);
        if self.request_options.max_redirect_hops == max_redirect_hops {
            false
        } else {
            self.request_options.max_redirect_hops = max_redirect_hops;
            true
        }
    }

    pub fn set_request_options(&mut self, request_options: RequestOptions) -> bool {
        if self.request_options == request_options {
            false
        } else {
            self.request_options = request_options;
            true
        }
    }

    pub fn set_param_draft_key(&mut self, key: impl Into<String>) -> bool {
        let key = key.into();
        if self.param_draft.key == key {
            return false;
        }
        self.param_draft.key = key;
        self.sync_url_from_params();
        true
    }

    pub fn set_header_draft_key(&mut self, key: impl Into<String>) -> bool {
        let key = key.into();
        if self.header_draft.key == key {
            false
        } else {
            self.header_draft.key = key;
            true
        }
    }

    pub fn set_param_draft_value(&mut self, value: impl Into<String>) -> bool {
        let value = value.into();
        if self.param_draft.value == value {
            return false;
        }
        self.param_draft.value = value;
        self.sync_url_from_params();
        true
    }

    pub fn set_header_draft_value(&mut self, value: impl Into<String>) -> bool {
        let value = value.into();
        if self.header_draft.value == value {
            false
        } else {
            self.header_draft.value = value;
            true
        }
    }

    /// Notes belong to the draft, independent of enabled flags and transport normalization.
    pub fn row_description(&self, headers: bool, index: Option<usize>) -> &str {
        match index {
            Some(index) => (if headers { &self.headers } else { &self.params })
                .get(index)
                .map_or("", |row| row.description.as_str()),
            None => {
                &(if headers {
                    &self.header_draft
                } else {
                    &self.param_draft
                })
                .description
            }
        }
    }

    pub fn set_row_description(
        &mut self,
        headers: bool,
        index: Option<usize>,
        value: String,
    ) -> bool {
        let description = match index {
            Some(index) => {
                let Some(row) = (if headers {
                    &mut self.headers
                } else {
                    &mut self.params
                })
                .get_mut(index) else {
                    return false;
                };
                &mut row.description
            }
            None => {
                &mut (if headers {
                    &mut self.header_draft
                } else {
                    &mut self.param_draft
                })
                .description
            }
        };
        if *description == value {
            return false;
        }
        *description = value;
        true
    }

    pub fn append_param_row(&mut self) -> bool {
        let draft = std::mem::take(&mut self.param_draft);
        self.params.push(KeyValueRow {
            enabled: true,
            key: draft.key,
            value: draft.value,
            description: draft.description,
        });
        self.sync_url_from_params();
        true
    }

    pub fn append_header_row(&mut self) -> bool {
        let draft = std::mem::take(&mut self.header_draft);
        if draft.key.eq_ignore_ascii_case("content-type") {
            self.content_type_source = ManagedHeaderSource::User;
        }
        if draft.key.eq_ignore_ascii_case("accept") {
            self.accept_source = ManagedHeaderSource::User;
        }
        self.headers.push(KeyValueRow {
            enabled: true,
            key: draft.key,
            value: draft.value,
            description: draft.description,
        });
        true
    }

    pub fn upsert_param(&mut self, key: impl Into<String>, value: impl Into<String>) -> bool {
        let key = key.into();
        if key.trim().is_empty() {
            return false;
        }
        let value = value.into();
        if let Some(row) = self.params.iter_mut().find(|row| row.key == key) {
            row.value = value;
            row.enabled = true;
        } else {
            self.params.push(KeyValueRow::enabled(key, value));
        }
        self.sync_url_from_params();
        true
    }

    pub fn set_param_key(&mut self, index: usize, key: impl Into<String>) -> bool {
        let key = key.into();
        let Some(row) = self.params.get_mut(index) else {
            return false;
        };
        if row.key == key {
            return false;
        }
        row.key = key;
        self.sync_url_from_params();
        true
    }

    pub fn set_param_value(&mut self, index: usize, value: impl Into<String>) -> bool {
        let value = value.into();
        let Some(row) = self.params.get_mut(index) else {
            return false;
        };
        if row.value == value {
            return false;
        }
        row.value = value;
        self.sync_url_from_params();
        true
    }

    pub fn toggle_param(&mut self, index: usize) -> bool {
        let Some(row) = self.params.get_mut(index) else {
            return false;
        };
        row.enabled = !row.enabled;
        self.sync_url_from_params();
        true
    }

    pub fn remove_param(&mut self, index: usize) -> bool {
        if index >= self.params.len() {
            return false;
        }
        self.params.remove(index);
        self.sync_url_from_params();
        true
    }

    pub fn upsert_header(&mut self, key: impl Into<String>, value: impl Into<String>) -> bool {
        let key = key.into();
        let value = value.into();
        if key.trim().is_empty() || value.trim().is_empty() {
            return false;
        }
        let is_content_type = key.eq_ignore_ascii_case("content-type");
        let is_accept = key.eq_ignore_ascii_case("accept");
        if let Some(row) = self
            .headers
            .iter_mut()
            .find(|row| row.key.eq_ignore_ascii_case(&key))
        {
            row.value = value;
            row.enabled = true;
        } else {
            self.headers.push(KeyValueRow::enabled(key, value));
        }
        if is_content_type {
            self.content_type_source = ManagedHeaderSource::User;
        }
        if is_accept {
            self.accept_source = ManagedHeaderSource::User;
        }
        true
    }

    pub fn set_header_key(&mut self, index: usize, key: impl Into<String>) -> bool {
        let key = key.into();
        let Some(row) = self.headers.get_mut(index) else {
            return false;
        };
        if row.key == key {
            return false;
        }
        if row.key.eq_ignore_ascii_case("content-type") || key.eq_ignore_ascii_case("content-type")
        {
            self.content_type_source = ManagedHeaderSource::User;
        }
        if row.key.eq_ignore_ascii_case("accept") || key.eq_ignore_ascii_case("accept") {
            self.accept_source = ManagedHeaderSource::User;
        }
        row.key = key;
        true
    }

    pub fn set_header_value(&mut self, index: usize, value: impl Into<String>) -> bool {
        let value = value.into();
        let Some(row) = self.headers.get_mut(index) else {
            return false;
        };
        if row.value == value {
            return false;
        }
        if row.key.eq_ignore_ascii_case("content-type") {
            self.content_type_source = ManagedHeaderSource::User;
        }
        if row.key.eq_ignore_ascii_case("accept") {
            self.accept_source = ManagedHeaderSource::User;
        }
        row.value = value;
        true
    }

    pub fn clear_header_draft(&mut self) -> bool {
        if self.header_draft == KeyValueDraft::default() {
            false
        } else {
            self.header_draft = KeyValueDraft::default();
            true
        }
    }

    pub fn toggle_header(&mut self, index: usize) -> bool {
        let Some(row) = self.headers.get_mut(index) else {
            return false;
        };
        if row.key.eq_ignore_ascii_case("content-type") {
            self.content_type_source = ManagedHeaderSource::User;
        }
        if row.key.eq_ignore_ascii_case("accept") {
            self.accept_source = ManagedHeaderSource::User;
        }
        row.enabled = !row.enabled;
        true
    }

    pub fn remove_header(&mut self, index: usize) -> bool {
        if index >= self.headers.len() {
            return false;
        }
        if self.headers[index].key.eq_ignore_ascii_case("content-type") {
            self.content_type_source = ManagedHeaderSource::User;
        }
        if self.headers[index].key.eq_ignore_ascii_case("accept") {
            self.accept_source = ManagedHeaderSource::User;
        }
        self.headers.remove(index);
        true
    }

    pub fn restore_editor_intent(&mut self, intent: &RequestEditorIntent) {
        self.body = RequestBodyDraft::from_editor_intent(intent);
    }

    /// Applies editor-only canonicalization performed when Send is pressed.
    pub fn normalize_for_send(&mut self) {
        if self.authorization_kind == AuthorizationKind::Bearer {
            self.bearer_token = normalize_bearer_token(&self.bearer_token);
        }
    }

    fn authorization_header_value(&self) -> Option<String> {
        match self.authorization_kind {
            AuthorizationKind::Bearer => {
                let token = self.normalized_bearer_token();
                (!token.is_empty()).then(|| format!("Bearer {token}"))
            }
            AuthorizationKind::Basic
                if !self.basic_username.is_empty() || !self.basic_password.is_empty() =>
            {
                Some(basic_authorization_value(
                    &self.basic_username,
                    &self.basic_password,
                ))
            }
            AuthorizationKind::Basic => None,
        }
    }

    fn sync_automatic_content_type(&mut self) -> bool {
        if self.content_type_source == ManagedHeaderSource::User {
            return false;
        }

        let desired = if self.method.allows_body() {
            self.automatic_content_type()
        } else {
            None
        };
        let content_type_index = self
            .headers
            .iter()
            .position(|row| row.key.eq_ignore_ascii_case("content-type"));

        match (self.content_type_source, content_type_index, desired) {
            (ManagedHeaderSource::User, _, _) => unreachable!("handled above"),
            (ManagedHeaderSource::Unset, Some(_), _) => {
                self.content_type_source = ManagedHeaderSource::User;
                false
            }
            (_, Some(index), Some(value)) => {
                let row = &mut self.headers[index];
                let changed = row.value != value || !row.enabled;
                row.value = value.to_string();
                row.enabled = true;
                self.content_type_source = ManagedHeaderSource::Automatic;
                changed
            }
            (_, None, Some(value)) => {
                self.headers
                    .push(KeyValueRow::enabled("Content-Type", value));
                self.content_type_source = ManagedHeaderSource::Automatic;
                true
            }
            (ManagedHeaderSource::Automatic, Some(index), None) => {
                self.headers.remove(index);
                self.content_type_source = ManagedHeaderSource::Unset;
                true
            }
            (_, None, None) => {
                self.content_type_source = ManagedHeaderSource::Unset;
                false
            }
        }
    }

    fn sync_automatic_accept(&mut self) -> bool {
        if self.accept_source == ManagedHeaderSource::User {
            return false;
        }

        let desired = (self.method == HttpMethod::POST).then_some("application/json");
        let accept_index = self
            .headers
            .iter()
            .position(|row| row.key.eq_ignore_ascii_case("accept"));

        match (self.accept_source, accept_index, desired) {
            (ManagedHeaderSource::User, _, _) => unreachable!("handled above"),
            (ManagedHeaderSource::Unset, Some(_), _) => {
                self.accept_source = ManagedHeaderSource::User;
                false
            }
            (_, Some(index), Some(value)) => {
                let row = &mut self.headers[index];
                let changed = row.value != value || !row.enabled;
                row.value = value.to_string();
                row.enabled = true;
                self.accept_source = ManagedHeaderSource::Automatic;
                changed
            }
            (_, None, Some(value)) => {
                self.headers.push(KeyValueRow::enabled("Accept", value));
                self.accept_source = ManagedHeaderSource::Automatic;
                true
            }
            (ManagedHeaderSource::Automatic, Some(index), None) => {
                self.headers.remove(index);
                self.accept_source = ManagedHeaderSource::Unset;
                true
            }
            (_, None, None) => {
                self.accept_source = ManagedHeaderSource::Unset;
                false
            }
        }
    }

    fn effective_params(&self) -> Vec<KeyValueRow> {
        let mut params = self.params.clone();
        if !self.param_draft.key.trim().is_empty() {
            params.push(KeyValueRow {
                enabled: true,
                key: self.param_draft.key.clone(),
                value: self.param_draft.value.clone(),
                description: self.param_draft.description.clone(),
            });
        }
        params
    }

    fn sync_url_from_params(&mut self) {
        self.url = apply_query_params(&self.url, &self.effective_params());
    }
}

impl Default for RequestDraft {
    fn default() -> Self {
        Self {
            method: HttpMethod::GET,
            url: String::new(),
            params: Vec::new(),
            param_draft: KeyValueDraft::default(),
            headers: Vec::new(),
            header_draft: KeyValueDraft::default(),
            body: RequestBodyDraft::None,
            inactive_bodies: Vec::new(),
            raw_format: RawBodyFormat::default(),
            binary_size: None,
            content_type_source: ManagedHeaderSource::Unset,
            accept_source: ManagedHeaderSource::Unset,
            authorization_kind: AuthorizationKind::Bearer,
            bearer_token: String::new(),
            basic_username: String::new(),
            basic_password: String::new(),
            request_options: RequestOptions {
                timeout_ms: None,
                redirect_policy: RedirectPolicy::Follow,
                max_redirect_hops: DEFAULT_MAX_REDIRECT_HOPS,
            },
        }
    }
}

fn query_parameter_count(url: &str) -> usize {
    let Some((_, query_and_fragment)) = url.split_once('?') else {
        return 0;
    };
    let query = query_and_fragment
        .split_once('#')
        .map(|(query, _)| query)
        .unwrap_or(query_and_fragment);
    form_urlencoded::parse(query.as_bytes()).count()
}

fn parse_query_params(url: &str) -> Vec<KeyValueRow> {
    let Some((_, query_and_fragment)) = url.split_once('?') else {
        return Vec::new();
    };
    let query = query_and_fragment
        .split_once('#')
        .map(|(query, _)| query)
        .unwrap_or(query_and_fragment);

    form_urlencoded::parse(query.as_bytes())
        .map(|(key, value)| KeyValueRow::enabled(key.into_owned(), value.into_owned()))
        .collect()
}

fn apply_query_params(url: &str, params: &[KeyValueRow]) -> String {
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for row in params {
        if row.enabled && !row.key.trim().is_empty() {
            serializer.append_pair(&row.key, &row.value);
        }
    }
    let (url_without_fragment, fragment) = url
        .split_once('#')
        .map(|(base, fragment)| (base, Some(fragment)))
        .unwrap_or((url, None));
    let base_url = url_without_fragment
        .split_once('?')
        .map(|(base, _)| base)
        .unwrap_or(url_without_fragment);
    let query = serializer.finish();
    let fragment = fragment
        .map(|fragment| format!("#{fragment}"))
        .unwrap_or_default();
    if query.is_empty() {
        format!("{base_url}{fragment}")
    } else {
        format!("{base_url}?{query}{fragment}")
    }
}

fn normalize_bearer_token(value: &str) -> String {
    let value = value.trim();
    let mut segments = value.splitn(2, char::is_whitespace);
    let first = segments.next().unwrap_or_default();
    if first.eq_ignore_ascii_case("bearer") {
        segments.next().unwrap_or_default().trim().to_string()
    } else {
        value.to_string()
    }
}

fn bearer_token_from_header(value: &str) -> Option<String> {
    let value = value.trim();
    let (scheme, token) = value.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| token.trim().to_string())
}

fn basic_authorization_value(username: &str, password: &str) -> String {
    let credentials = STANDARD.encode(format!("{username}:{password}"));
    format!("Basic {credentials}")
}

fn decode_basic_credentials(value: &str) -> Option<(String, String)> {
    let (scheme, credentials) = value.trim().split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("basic") {
        return None;
    }
    let decoded = String::from_utf8(STANDARD.decode(credentials.trim()).ok()?).ok()?;
    let (username, password) = decoded.split_once(':')?;
    Some((username.to_string(), password.to_string()))
}

fn blank_url_encoded_rows() -> Vec<KeyValueRow> {
    vec![KeyValueRow::enabled("", "")]
}

fn nonempty_url_encoded_rows(mut rows: Vec<KeyValueRow>) -> Vec<KeyValueRow> {
    if rows.is_empty() {
        rows = blank_url_encoded_rows();
    }
    rows
}

fn parse_url_encoded_rows(body: &str) -> Vec<KeyValueRow> {
    nonempty_url_encoded_rows(
        form_urlencoded::parse(body.as_bytes())
            .map(|(key, value)| KeyValueRow::enabled(key.into_owned(), value.into_owned()))
            .collect(),
    )
}

fn serialize_url_encoded_rows(rows: &[KeyValueRow]) -> String {
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for row in rows
        .iter()
        .filter(|row| row.enabled && !row.key.trim().is_empty())
    {
        serializer.append_pair(&row.key, &row.value);
    }
    serializer.finish()
}

fn blank_multipart_parts() -> Vec<MultipartDraftPart> {
    vec![MultipartDraftPart::text("", "", true)]
}

fn nonempty_multipart_parts(mut parts: Vec<MultipartDraftPart>) -> Vec<MultipartDraftPart> {
    if parts.is_empty() {
        parts = blank_multipart_parts();
    }
    parts
}

fn parse_multipart_text_parts(body: &str) -> Vec<MultipartDraftPart> {
    nonempty_multipart_parts(
        form_urlencoded::parse(body.as_bytes())
            .map(|(name, value)| {
                MultipartDraftPart::text(name.into_owned(), value.into_owned(), true)
            })
            .collect(),
    )
}

fn default_json_body() -> String {
    r#"{
  "message": "Hello, World!",
  "data": {
    "key": "value"
  }
}"#
    .to_string()
}

fn header_row_is_complete(row: &KeyValueRow) -> bool {
    !row.key.trim().is_empty() && !row.value.trim().is_empty()
}

fn header_draft_is_complete(draft: &KeyValueDraft) -> bool {
    !draft.key.trim().is_empty() && !draft.value.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_values<'a>(construction: &'a RequestConstruction, name: &str) -> Vec<&'a str> {
        construction
            .request()
            .headers
            .iter()
            .filter(|(actual, _)| actual.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
            .collect()
    }

    #[test]
    fn descriptions_are_draft_only_and_follow_duplicate_query_occurrences() {
        let mut draft = super::RequestDraft::new();
        draft.set_url("https://example.test/users?tag=first&tag=second");
        let before = draft.construct().request().clone();
        draft.set_row_description(false, Some(0), "First tag".into());
        draft.set_row_description(false, Some(1), "Second tag".into());
        assert_eq!(draft.construct().request(), &before);
        draft.set_url("https://example.test/other?tag=changed&tag=second#anchor");
        assert_eq!(draft.params()[0].description, "First tag");
        assert_eq!(draft.params()[1].description, "Second tag");
        draft.remove_param(0);
        assert_eq!(draft.params()[0].description, "Second tag");
        draft.set_param_draft_key("page");
        draft.set_param_draft_value("1");
        draft.set_row_description(false, None, "Page number".into());
        draft.append_param_row();
        assert_eq!(draft.params()[1].description, "Page number");
        assert_eq!(draft.row_description(false, None), "");
    }

    #[test]
    fn draft_builds_and_validates_without_a_workspace() {
        let mut draft = RequestDraft::new();
        assert_eq!(draft.build().unwrap_err(), RequestDraftError::UrlEmpty);

        draft.set_url("https://example.com/items");
        draft.set_method(HttpMethod::POST);
        draft.set_body(r#"{"name":"Ada"}"#);
        let construction = draft.build().expect("a URL makes the draft valid");

        assert_eq!(construction.request().method, HttpMethod::POST);
        assert_eq!(construction.request().url, "https://example.com/items");
        assert_eq!(
            construction.request().body,
            RequestBody::Json(r#"{"name":"Ada"}"#.to_string())
        );
        assert_eq!(
            header_values(&construction, "content-type"),
            vec!["application/json"]
        );
    }

    #[test]
    fn auth_and_explicit_header_precedence_is_table_driven() {
        enum AuthCase {
            Bearer,
            Basic,
            EmptyManagedAuth,
        }
        struct Case {
            name: &'static str,
            auth: AuthCase,
            expected_authorization: Vec<&'static str>,
        }
        let cases = [
            Case {
                name: "bearer replaces every explicit authorization variant",
                auth: AuthCase::Bearer,
                expected_authorization: vec!["Bearer scenario-token"],
            },
            Case {
                name: "basic replaces every explicit authorization variant",
                auth: AuthCase::Basic,
                expected_authorization: vec!["Basic c2NlbmFyaW8tdXNlcjpzY2VuYXJpby1wYXNz"],
            },
            Case {
                name: "empty managed auth preserves explicit authorization",
                auth: AuthCase::EmptyManagedAuth,
                expected_authorization: vec!["Custom first", "Custom second"],
            },
        ];

        for case in cases {
            let mut draft = RequestDraft::new();
            draft.set_url("https://example.com/auth");
            draft.set_header_draft_key("Authorization");
            draft.set_header_draft_value("Custom first");
            draft.append_header_row();
            draft.set_header_draft_key("authorization");
            draft.set_header_draft_value("Custom second");
            draft.append_header_row();
            draft.upsert_header("X-Trace", case.name);
            match case.auth {
                AuthCase::Bearer => {
                    draft.set_bearer_token("  BEARER    scenario-token  ");
                }
                AuthCase::Basic => {
                    draft.set_authorization_kind(AuthorizationKind::Basic);
                    draft.set_basic_username("scenario-user");
                    draft.set_basic_password("scenario-pass");
                }
                AuthCase::EmptyManagedAuth => {}
            }

            let construction = draft
                .build()
                .unwrap_or_else(|error| panic!("{}: {error}", case.name));
            assert_eq!(
                header_values(&construction, "authorization"),
                case.expected_authorization,
                "{}",
                case.name
            );
            assert_eq!(
                header_values(&construction, "x-trace"),
                vec![case.name],
                "{}",
                case.name
            );
        }
    }

    #[test]
    fn every_body_mode_uses_one_normalized_construction_table() {
        struct Case {
            name: &'static str,
            kind: BodyKind,
            expected_body: RequestBody,
            expected_content_type: Vec<&'static str>,
        }
        let cases = [
            Case {
                name: "none",
                kind: BodyKind::None,
                expected_body: RequestBody::None,
                expected_content_type: Vec::new(),
            },
            Case {
                name: "json",
                kind: BodyKind::Json,
                expected_body: RequestBody::Json(r#"{"exact":true}"#.to_string()),
                expected_content_type: vec!["application/json"],
            },
            Case {
                name: "raw",
                kind: BodyKind::Raw,
                expected_body: RequestBody::Raw("raw\0bytes\nkept".to_string()),
                expected_content_type: vec!["text/plain"],
            },
            Case {
                name: "url-encoded",
                kind: BodyKind::UrlEncoded,
                expected_body: RequestBody::UrlEncoded(
                    "name=Ada+Lovelace&locale=%E4%B8%AD%E6%96%87".to_string(),
                ),
                expected_content_type: vec!["application/x-www-form-urlencoded"],
            },
            Case {
                name: "multipart",
                kind: BodyKind::Multipart,
                expected_body: RequestBody::Multipart(vec![
                    MultipartPart::text("name", "Ada"),
                    MultipartPart {
                        name: "avatar".to_string(),
                        value: MultipartValue::File {
                            path: PathBuf::from("/tmp/avatar.png"),
                            file_name: Some("profile.png".to_string()),
                            content_type: Some("image/png".to_string()),
                        },
                    },
                ]),
                expected_content_type: Vec::new(),
            },
            Case {
                name: "binary",
                kind: BodyKind::Binary,
                expected_body: RequestBody::File(PathBuf::from("/tmp/body.bin")),
                expected_content_type: vec!["application/octet-stream"],
            },
        ];

        for case in cases {
            let mut draft = RequestDraft::new();
            draft.set_url(format!("https://example.com/body/{}", case.name));
            draft.set_method(HttpMethod::POST);
            draft.set_body_kind(case.kind);
            match case.kind {
                BodyKind::None => {}
                BodyKind::Json => {
                    draft.set_body(r#"{"exact":true}"#);
                }
                BodyKind::Raw => {
                    draft.set_body("raw\0bytes\nkept");
                }
                BodyKind::UrlEncoded => {
                    draft.set_url_encoded_rows(vec![
                        KeyValueRow::enabled("name", "Ada Lovelace"),
                        KeyValueRow {
                            description: String::new(),
                            enabled: false,
                            key: "disabled".to_string(),
                            value: "omitted".to_string(),
                        },
                        KeyValueRow::enabled("locale", "中文"),
                    ]);
                }
                BodyKind::Multipart => {
                    draft.set_multipart_draft_parts(vec![
                        MultipartDraftPart::text("name", "Ada", true),
                        MultipartDraftPart::text("disabled", "omitted", false),
                        MultipartDraftPart::file(
                            "avatar",
                            "/tmp/avatar.png",
                            Some("profile.png".to_string()),
                            Some("image/png".to_string()),
                            true,
                        ),
                        MultipartDraftPart::file("incomplete", "", None, None, true),
                    ]);
                }
                BodyKind::Binary => {
                    draft.set_binary_file(PathBuf::from("/tmp/body.bin"), Some(4));
                }
            }

            let construction = draft
                .build()
                .unwrap_or_else(|error| panic!("{}: {error}", case.name));
            assert_eq!(
                construction.request().body,
                case.expected_body,
                "{}",
                case.name
            );
            assert_eq!(
                header_values(&construction, "content-type"),
                case.expected_content_type,
                "{}",
                case.name
            );
            assert_eq!(
                header_values(&construction, "accept"),
                vec!["application/json"],
                "{}",
                case.name
            );
        }
    }

    fn independent_body_drafts() -> Vec<(BodyKind, RequestBodyDraft)> {
        vec![
            (BodyKind::None, RequestBodyDraft::None),
            (
                BodyKind::Json,
                RequestBodyDraft::Json("{\n  \"json\": true\n}".into()),
            ),
            (
                BodyKind::Raw,
                RequestBodyDraft::Raw("<raw>中文\0\r\n</raw>".into()),
            ),
            (
                BodyKind::UrlEncoded,
                RequestBodyDraft::UrlEncoded(vec![
                    KeyValueRow::enabled("tag", "first"),
                    KeyValueRow {
                        enabled: false,
                        key: "tag".into(),
                        value: "disabled duplicate".into(),
                        description: "Keep this editor note".into(),
                    },
                    KeyValueRow::enabled("", "unfinished"),
                    KeyValueRow::enabled("tag", "second"),
                ]),
            ),
            (
                BodyKind::Multipart,
                RequestBodyDraft::Multipart(vec![
                    MultipartDraftPart::text("part", "first", true),
                    MultipartDraftPart::file(
                        "part",
                        "fixtures/upload.bin",
                        Some("renamed.bin".into()),
                        Some("application/octet-stream".into()),
                        true,
                    ),
                    MultipartDraftPart::file("part", "missing.bin", None, None, false),
                    MultipartDraftPart::text("part", "disabled duplicate", false),
                    MultipartDraftPart::text("", "unfinished", true),
                    MultipartDraftPart::file("pending", "", None, None, true),
                ]),
            ),
            (
                BodyKind::Binary,
                RequestBodyDraft::Binary(PathBuf::from("fixtures/中文 payload.bin")),
            ),
        ]
    }

    fn select_body_draft(draft: &mut RequestDraft, kind: BodyKind, body: &RequestBodyDraft) {
        draft.set_body_kind(kind);
        match body {
            RequestBodyDraft::None => {}
            RequestBodyDraft::Json(text) | RequestBodyDraft::Raw(text) => {
                draft.set_body(text);
            }
            RequestBodyDraft::UrlEncoded(rows) => {
                draft.set_url_encoded_rows(rows.clone());
            }
            RequestBodyDraft::Multipart(parts) => {
                draft.set_multipart_draft_parts(parts.clone());
            }
            RequestBodyDraft::Binary(path) => {
                draft.set_binary_file(path.clone(), Some(257));
            }
        }
    }

    #[test]
    fn switching_body_modes_preserves_each_complete_independent_draft() {
        let mut draft = RequestDraft::new();
        draft.set_method(HttpMethod::PUT);
        draft.set_raw_body_format(RawBodyFormat::Xml);
        let cases = independent_body_drafts();
        let mut constructions = Vec::new();
        for (kind, body) in &cases {
            select_body_draft(&mut draft, *kind, body);
            constructions.push(draft.construct().request().clone());
        }

        // Repeat visits in both directions to catch stale or overwritten cached drafts.
        for index in (0..cases.len()).rev().chain(0..cases.len()) {
            let (kind, body) = &cases[index];
            draft.set_body_kind(*kind);
            assert_eq!(draft.body_draft(), body, "{kind:?}");
            assert_eq!(
                draft.construct().request(),
                &constructions[index],
                "{kind:?}"
            );
            assert!(
                !draft.set_body_kind(*kind),
                "reselecting {kind:?} is a no-op"
            );
        }
        assert_eq!(draft.raw_body_format(), RawBodyFormat::Xml);
        assert_eq!(draft.binary_size(), Some(257));

        draft.set_body_kind(BodyKind::Raw);
        draft.set_body("edited after restoration");
        draft.set_body_kind(BodyKind::Json);
        assert_eq!(draft.body_draft(), &cases[1].1);
        draft.set_body_kind(BodyKind::Raw);
        assert_eq!(draft.body_text(), "edited after restoration");
        assert_eq!(
            draft.construct().request().body,
            RequestBody::Raw("edited after restoration".into())
        );
    }

    #[test]
    fn selecting_a_new_body_mode_starts_empty_without_converting_the_previous_draft() {
        let mut draft = RequestDraft::new();
        draft.set_method(HttpMethod::PUT);
        let cases = independent_body_drafts();
        let empty_drafts = [
            RequestBodyDraft::None,
            RequestBodyDraft::Json(String::new()),
            RequestBodyDraft::Raw(String::new()),
            RequestBodyDraft::UrlEncoded(vec![KeyValueRow::enabled("", "")]),
            RequestBodyDraft::Multipart(vec![MultipartDraftPart::text("", "", true)]),
            RequestBodyDraft::Binary(PathBuf::new()),
        ];
        for ((kind, populated), empty) in cases.iter().zip(empty_drafts) {
            draft.set_body_kind(*kind);
            assert_eq!(draft.body_draft(), &empty, "{kind:?}");
            select_body_draft(&mut draft, *kind, populated);
        }
    }

    #[test]
    fn post_from_none_keeps_the_initial_json_default_without_losing_other_modes() {
        for raw_text in [None, Some("keep the earlier Raw draft")] {
            let mut draft = RequestDraft::new();
            if let Some(text) = raw_text {
                draft.set_body_kind(BodyKind::Raw);
                draft.set_body(text);
                draft.set_body_kind(BodyKind::None);
            }

            draft.set_method(HttpMethod::POST);
            assert_eq!(draft.body_kind(), BodyKind::Json);
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&draft.body_text()).unwrap(),
                serde_json::json!({
                    "message": "Hello, World!",
                    "data": { "key": "value" }
                })
            );
            assert_eq!(
                header_values(&draft.construct(), "content-type"),
                vec!["application/json"]
            );

            if let Some(text) = raw_text {
                draft.set_body_kind(BodyKind::Raw);
                assert_eq!(draft.body_text(), text);
            }
        }
    }

    #[test]
    fn post_from_none_restores_cached_json_even_when_it_was_intentionally_cleared() {
        for json_text in ["{\n  \"keep\": \"my JSON draft\"\n}", ""] {
            let mut draft = RequestDraft::new();
            draft.set_method(HttpMethod::POST);
            if json_text.is_empty() {
                draft.clear_body();
            } else {
                draft.set_body(json_text);
            }
            draft.set_body_kind(BodyKind::Raw);
            draft.set_body("independent Raw draft");
            draft.set_body_kind(BodyKind::None);

            for previous_method in [HttpMethod::GET, HttpMethod::PUT] {
                draft.set_method(previous_method);
                draft.set_method(HttpMethod::POST);
                assert_eq!(
                    draft.body_draft(),
                    &RequestBodyDraft::Json(json_text.into()),
                    "returning from {previous_method:?} must restore the cached JSON"
                );
                assert_eq!(
                    draft.construct().request().body,
                    RequestBody::Json(json_text.into())
                );
                assert_eq!(
                    header_values(&draft.construct(), "content-type"),
                    vec!["application/json"]
                );
                draft.set_body_kind(BodyKind::Raw);
                assert_eq!(draft.body_text(), "independent Raw draft");
                draft.set_body_kind(BodyKind::None);
                assert_eq!(draft.construct().request().body, RequestBody::None);
            }
        }
    }

    #[test]
    fn typed_body_setters_preserve_previous_modes_without_an_explicit_kind_change() {
        let mut draft = RequestDraft::new();
        draft.set_method(HttpMethod::PUT);
        draft.set_body_kind(BodyKind::Json);
        draft.set_body(r#"{"keep":"json"}"#);
        let rows = vec![KeyValueRow::enabled("keep", "form")];
        draft.set_url_encoded_rows(rows.clone());
        let parts = vec![MultipartDraftPart::file(
            "keep",
            "upload.bin",
            None,
            None,
            true,
        )];
        draft.set_multipart_draft_parts(parts.clone());
        let path = PathBuf::from("binary.bin");
        draft.set_binary_file(path.clone(), Some(42));
        draft.set_body_kind(BodyKind::None);
        draft.set_body("new Raw from None");
        assert_eq!(draft.body_kind(), BodyKind::Raw);
        assert_eq!(
            header_values(&draft.construct(), "content-type"),
            vec!["text/plain"]
        );
        assert_eq!(
            draft
                .headers()
                .iter()
                .filter(|row| row.key.eq_ignore_ascii_case("content-type"))
                .count(),
            1
        );

        for (kind, expected) in [
            (
                BodyKind::Json,
                RequestBodyDraft::Json(r#"{"keep":"json"}"#.into()),
            ),
            (BodyKind::UrlEncoded, RequestBodyDraft::UrlEncoded(rows)),
            (BodyKind::Multipart, RequestBodyDraft::Multipart(parts)),
            (BodyKind::Binary, RequestBodyDraft::Binary(path)),
            (BodyKind::None, RequestBodyDraft::None),
            (
                BodyKind::Raw,
                RequestBodyDraft::Raw("new Raw from None".into()),
            ),
        ] {
            draft.set_body_kind(kind);
            assert_eq!(draft.body_draft(), &expected, "{kind:?}");
        }
    }

    #[test]
    fn clearing_one_body_mode_preserves_every_other_cached_draft() {
        let cases = independent_body_drafts();
        let mut populated = RequestDraft::new();
        populated.set_method(HttpMethod::PUT);
        populated.set_raw_body_format(RawBodyFormat::Html);
        for (kind, body) in &cases {
            select_body_draft(&mut populated, *kind, body);
        }
        for (cleared_kind, _) in &cases {
            let mut draft = populated.clone();
            draft.set_body_kind(*cleared_kind);
            draft.clear_body();
            let cleared = draft.body_draft().clone();
            let mut empty = RequestDraft::new();
            empty.set_body_kind(*cleared_kind);
            assert_eq!(&cleared, empty.body_draft(), "{cleared_kind:?}");
            assert!(!draft.clear_body(), "clearing twice is a no-op");

            for (kind, original) in &cases {
                draft.set_body_kind(*kind);
                let expected = if kind == cleared_kind {
                    &cleared
                } else {
                    original
                };
                assert_eq!(
                    draft.body_draft(),
                    expected,
                    "cleared {cleared_kind:?}, visited {kind:?}"
                );
            }
            assert_eq!(draft.raw_body_format(), RawBodyFormat::Html);
            assert_eq!(
                draft.binary_size(),
                if *cleared_kind == BodyKind::Binary {
                    None
                } else {
                    Some(257)
                }
            );
        }
    }

    #[test]
    fn raw_formats_change_only_the_generated_content_type_and_preserve_exact_text() {
        let mut draft = RequestDraft::new();
        draft.set_method(HttpMethod::PUT);
        draft.set_body_kind(BodyKind::Raw);
        let text = "not parsed or reformatted\0\r\n中文";
        draft.set_body(text);

        for (format, content_type) in [
            (RawBodyFormat::Text, "text/plain"),
            (RawBodyFormat::Xml, "application/xml"),
            (RawBodyFormat::Html, "text/html"),
            (RawBodyFormat::JavaScript, "application/javascript"),
            (RawBodyFormat::Text, "text/plain"),
        ] {
            draft.set_raw_body_format(format);
            let construction = draft.construct();
            assert_eq!(construction.request().body, RequestBody::Raw(text.into()));
            assert_eq!(
                header_values(&construction, "content-type"),
                vec![content_type]
            );
            assert_eq!(
                construction.effective_headers(),
                &[EffectiveHeader {
                    name: "Content-Type".into(),
                    value: content_type.into(),
                    source: EffectiveHeaderSource::Generated,
                }]
            );

            draft.set_body_kind(BodyKind::Json);
            assert_eq!(
                header_values(&draft.construct(), "content-type"),
                vec!["application/json"]
            );
            draft.set_body_kind(BodyKind::None);
            assert!(header_values(&draft.construct(), "content-type").is_empty());
            draft.set_body_kind(BodyKind::Raw);
            assert_eq!(draft.raw_body_format(), format);
            assert_eq!(draft.construct(), construction);

            draft.set_method(HttpMethod::GET);
            assert_eq!(draft.construct().request().body, RequestBody::None);
            assert!(header_values(&draft.construct(), "content-type").is_empty());
            draft.set_method(HttpMethod::PUT);
            assert_eq!(draft.construct(), construction);
        }
    }

    #[test]
    fn every_raw_format_preserves_a_manual_content_type_override() {
        let mut draft = RequestDraft::new();
        draft.set_method(HttpMethod::PUT);
        draft.set_body_kind(BodyKind::Raw);
        draft.upsert_header("cOnTeNt-TyPe", "application/x-custom; charset=utf-8");
        for format in [
            RawBodyFormat::Text,
            RawBodyFormat::Xml,
            RawBodyFormat::Html,
            RawBodyFormat::JavaScript,
        ] {
            draft.set_raw_body_format(format);
            let construction = draft.construct();
            assert_eq!(
                header_values(&construction, "content-type"),
                vec!["application/x-custom; charset=utf-8"]
            );
            assert_eq!(
                construction.effective_headers()[0].source,
                EffectiveHeaderSource::User
            );
        }
    }

    #[test]
    fn raw_reconstruction_infers_format_without_rewriting_the_saved_request() {
        for (content_type, format) in [
            (None, RawBodyFormat::Text),
            (Some("text/plain; charset=utf-8"), RawBodyFormat::Text),
            (Some(" APPLICATION/XML ; charset=UTF-8"), RawBodyFormat::Xml),
            (Some("text/xml"), RawBodyFormat::Xml),
            (Some("text/html; charset=utf-8"), RawBodyFormat::Html),
            (Some("application/javascript"), RawBodyFormat::JavaScript),
            (
                Some("text/javascript; charset=utf-8"),
                RawBodyFormat::JavaScript,
            ),
            (Some("application/x-custom"), RawBodyFormat::Text),
        ] {
            let mut request = Request::new(HttpMethod::PUT, "https://example.test/raw");
            request.body = RequestBody::Raw("saved\0\r\ntext".into());
            if let Some(value) = content_type {
                request.headers.push(("cOnTeNt-TyPe".into(), value.into()));
            }
            let mut draft = RequestDraft::from_request(&request);
            assert_eq!(draft.body_kind(), BodyKind::Raw);
            assert_eq!(draft.raw_body_format(), format, "{content_type:?}");
            assert_eq!(draft.construct().request(), &request);
            draft.set_raw_body_format(RawBodyFormat::Html);
            assert_eq!(
                draft.construct().request(),
                &request,
                "loaded header policy is explicit"
            );
        }
    }

    #[test]
    fn binary_reconstruction_keeps_a_file_body_and_never_injects_path_text() {
        for content_type in [None, Some("application/x-upload")] {
            let path = PathBuf::from("fixtures/中文 payload.bin");
            let mut request = Request::new(HttpMethod::PUT, "https://example.test/binary");
            request.body = RequestBody::File(path.clone());
            if let Some(value) = content_type {
                request.headers.push(("Content-Type".into(), value.into()));
            }
            let mut draft = RequestDraft::from_request(&request);
            assert_eq!(draft.body_kind(), BodyKind::Binary);
            assert_eq!(draft.body_draft(), &RequestBodyDraft::Binary(path));
            assert!(draft.body_text().is_empty());
            assert!(!draft.set_body("must not overwrite a file with text"));
            assert_eq!(draft.construct().request(), &request);
            draft.set_body_kind(BodyKind::Raw);
            assert_eq!(draft.body_draft(), &RequestBodyDraft::Raw(String::new()));
            draft.set_body_kind(BodyKind::Binary);
            assert_eq!(draft.construct().request(), &request);
        }
    }

    #[test]
    fn body_validation_reports_only_meaningful_enabled_errors_without_mutating_drafts() {
        let cases = [
            (BodyKind::None, RequestBodyDraft::None, None),
            (
                BodyKind::Json,
                RequestBodyDraft::Json(String::new()),
                Some("Invalid JSON:"),
            ),
            (
                BodyKind::Json,
                RequestBodyDraft::Json("{unfinished".into()),
                Some("Invalid JSON:"),
            ),
            (
                BodyKind::Json,
                RequestBodyDraft::Json("[true, null, 42]".into()),
                None,
            ),
            (
                BodyKind::Raw,
                RequestBodyDraft::Raw("{unfinished".into()),
                None,
            ),
            (
                BodyKind::Binary,
                RequestBodyDraft::Binary(PathBuf::new()),
                Some("Choose a file for the binary body."),
            ),
            // Validation does not read the selected file; the transport owns filesystem errors.
            (
                BodyKind::Binary,
                RequestBodyDraft::Binary(PathBuf::from("missing-upload.bin")),
                None,
            ),
            (
                BodyKind::UrlEncoded,
                RequestBodyDraft::UrlEncoded(vec![
                    KeyValueRow::enabled("", ""),
                    KeyValueRow::enabled("  ", "needs a key"),
                ]),
                Some("Enter a key for field 2"),
            ),
            (
                BodyKind::UrlEncoded,
                RequestBodyDraft::UrlEncoded(vec![
                    KeyValueRow {
                        enabled: false,
                        ..KeyValueRow::enabled("", "disabled")
                    },
                    KeyValueRow::enabled("", ""),
                    KeyValueRow::enabled("tag", ""),
                    KeyValueRow::enabled("tag", "duplicate"),
                ]),
                None,
            ),
            (
                BodyKind::Multipart,
                RequestBodyDraft::Multipart(vec![
                    MultipartDraftPart::text("", "", true),
                    MultipartDraftPart::text("  ", "needs a key", true),
                ]),
                Some("Enter a key for field 2"),
            ),
            (
                BodyKind::Multipart,
                RequestBodyDraft::Multipart(vec![MultipartDraftPart::file(
                    "",
                    "selected.bin",
                    None,
                    None,
                    true,
                )]),
                Some("Enter a key for field 1"),
            ),
            (
                BodyKind::Multipart,
                RequestBodyDraft::Multipart(vec![
                    MultipartDraftPart::text("note", "", true),
                    MultipartDraftPart::file("upload", "", None, None, true),
                ]),
                Some("Choose a file for field 2"),
            ),
            (
                BodyKind::Multipart,
                RequestBodyDraft::Multipart(vec![
                    MultipartDraftPart::text("", "disabled", false),
                    MultipartDraftPart::file("", "", None, None, false),
                    MultipartDraftPart::text("", "", true),
                    MultipartDraftPart::text("part", "", true),
                    MultipartDraftPart::file("part", "missing-upload.bin", None, None, true),
                ]),
                None,
            ),
        ];

        for (kind, body, expected_error) in cases {
            let mut draft = RequestDraft::new();
            draft.set_method(HttpMethod::PUT);
            draft.set_body_kind(BodyKind::Raw);
            draft.set_body("other mode stays cached");
            select_body_draft(&mut draft, kind, &body);
            let before = draft.clone();
            let actual = draft.body_validation_error();
            match expected_error {
                Some(prefix) => assert!(
                    actual
                        .as_deref()
                        .is_some_and(|message| message.starts_with(prefix)),
                    "{body:?}: {actual:?}"
                ),
                None => assert_eq!(actual, None, "{body:?}"),
            }
            assert_eq!(draft, before, "validation must be a pure read");

            draft.set_method(HttpMethod::GET);
            assert_eq!(
                draft.body_validation_error(),
                None,
                "GET does not send {kind:?}"
            );
            assert_eq!(draft.body_draft(), &body);
            draft.set_method(HttpMethod::PUT);
            assert_eq!(draft.body_validation_error(), actual);
        }
    }

    #[test]
    fn explicit_body_headers_survive_body_normalization() {
        let mut draft = RequestDraft::new();
        draft.set_url("https://example.com/manual-headers");
        draft.set_method(HttpMethod::POST);
        draft.upsert_header("content-type", "application/vnd.example+json");
        draft.upsert_header("accept", "application/problem+json");
        draft.set_body_kind(BodyKind::UrlEncoded);

        let construction = draft.build().unwrap();
        assert_eq!(
            header_values(&construction, "content-type"),
            vec!["application/vnd.example+json"]
        );
        assert_eq!(
            header_values(&construction, "accept"),
            vec!["application/problem+json"]
        );
    }
}

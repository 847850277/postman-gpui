use super::request::{
    HttpMethod, MultipartPart, MultipartValue, RedirectPolicy, Request, RequestBody,
    RequestOptions, DEFAULT_MAX_REDIRECT_HOPS, MAX_REDIRECT_HOPS,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
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
}

impl KeyValueRow {
    pub fn enabled(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            enabled: true,
            key: key.into(),
            value: value.into(),
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
}

impl RequestBodyDraft {
    fn kind(&self) -> BodyKind {
        match self {
            Self::None => BodyKind::None,
            Self::Json(_) => BodyKind::Json,
            Self::Raw(_) => BodyKind::Raw,
            Self::UrlEncoded(_) => BodyKind::UrlEncoded,
            Self::Multipart(_) => BodyKind::Multipart,
        }
    }

    fn empty_for(kind: BodyKind) -> Self {
        match kind {
            BodyKind::None => Self::None,
            BodyKind::Json => Self::Json(String::new()),
            BodyKind::Raw => Self::Raw(String::new()),
            BodyKind::UrlEncoded => Self::UrlEncoded(blank_url_encoded_rows()),
            BodyKind::Multipart => Self::Multipart(blank_multipart_parts()),
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
        }
    }

    fn effective_body(&self) -> RequestBody {
        match self {
            Self::None => RequestBody::None,
            Self::Json(value) => RequestBody::Json(value.clone()),
            Self::Raw(value) => RequestBody::Raw(value.clone()),
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
            Self::None | Self::Json(_) | Self::Raw(_) | Self::UrlEncoded(_) => None,
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
            Self::None | Self::Multipart(_) => String::new(),
        }
    }

    fn converted_to(&self, kind: BodyKind) -> Self {
        if self.kind() == kind {
            return self.clone();
        }

        match kind {
            BodyKind::None => Self::None,
            BodyKind::Json => Self::Json(self.editor_text()),
            BodyKind::Raw => Self::Raw(self.editor_text()),
            BodyKind::UrlEncoded => match self {
                Self::Multipart(parts) => Self::UrlEncoded(nonempty_url_encoded_rows(
                    parts
                        .iter()
                        .map(|part| KeyValueRow {
                            enabled: part.enabled,
                            key: part.name.clone(),
                            value: match &part.value {
                                MultipartDraftValue::Text(value) => value.clone(),
                                MultipartDraftValue::File { path, .. } => {
                                    path.display().to_string()
                                }
                            },
                        })
                        .collect(),
                )),
                _ => Self::UrlEncoded(parse_url_encoded_rows(&self.editor_text())),
            },
            BodyKind::Multipart => match self {
                Self::UrlEncoded(rows) => Self::Multipart(nonempty_multipart_parts(
                    rows.iter()
                        .map(|row| {
                            MultipartDraftPart::text(
                                row.key.clone(),
                                row.value.clone(),
                                row.enabled,
                            )
                        })
                        .collect(),
                )),
                _ => Self::Multipart(parse_multipart_text_parts(&self.editor_text())),
            },
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct KeyValueDraft {
    key: String,
    value: String,
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
                if let Some(value) = content_type_for(self.body_kind()) {
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
            self.body = RequestBodyDraft::Json(default_json_body());
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
        self.params = parse_query_params(&url);
        self.param_draft = KeyValueDraft::default();
        self.url = url;
        true
    }

    pub fn set_body(&mut self, body: impl Into<String>) -> bool {
        let body = body.into();
        let next = match &self.body {
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
        if self.body == next {
            false
        } else {
            self.body = next;
            true
        }
    }

    pub fn clear_body(&mut self) -> bool {
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
            self.body = self.body.converted_to(body_kind);
            changed = true;
        }
        self.sync_automatic_content_type() || changed
    }

    pub fn set_url_encoded_rows(&mut self, rows: Vec<KeyValueRow>) -> bool {
        let body = RequestBodyDraft::UrlEncoded(nonempty_url_encoded_rows(rows));
        let mut changed = false;
        if self.body != body {
            self.body = body;
            changed = true;
        }
        self.sync_automatic_content_type() || changed
    }

    pub fn set_multipart_draft_parts(&mut self, parts: Vec<MultipartDraftPart>) -> bool {
        let body = RequestBodyDraft::Multipart(nonempty_multipart_parts(parts));
        let mut changed = false;
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

    pub fn append_param_row(&mut self) -> bool {
        let draft = std::mem::take(&mut self.param_draft);
        self.params
            .push(KeyValueRow::enabled(draft.key, draft.value));
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
        self.headers
            .push(KeyValueRow::enabled(draft.key, draft.value));
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
            content_type_for(self.body_kind())
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
            params.push(KeyValueRow::enabled(
                self.param_draft.key.clone(),
                self.param_draft.value.clone(),
            ));
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

fn content_type_for(body_kind: BodyKind) -> Option<&'static str> {
    match body_kind {
        BodyKind::None | BodyKind::Raw | BodyKind::Multipart => None,
        BodyKind::Json => Some("application/json"),
        BodyKind::UrlEncoded => Some("application/x-www-form-urlencoded"),
    }
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
                expected_content_type: Vec::new(),
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

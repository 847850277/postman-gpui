use crate::utils::log::display_url_for_log;
use crate::{
    errors::AppError,
    http::executor::RequestResult,
    models::{
        HistoricalResponse, HistoryEntry, HttpMethod, MultipartEditorPart, MultipartPart,
        MultipartValue, RedirectHop, RedirectPolicy, Request, RequestBody, RequestEditorIntent,
        RequestHistory, RequestOptions, DEFAULT_MAX_REDIRECT_HOPS, MAX_REDIRECT_HOPS,
    },
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::{
    collections::HashMap,
    fmt,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

const MAX_HISTORY_URL_LENGTH: usize = 40;

/// The request editor section selected by the user.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestPane {
    Params,
    Authorization,
    Headers,
    Body,
    Scripts,
    Tests,
    Options,
}

/// Authentication scheme managed by the Authorization editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorizationKind {
    Bearer,
    Basic,
}

/// Body encoding selected in the editor. The editable payload and encoding are stored together
/// in `RequestBodyDraft`; this enum is only a compact value for rendering controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    None,
    Json,
    Raw,
    UrlEncoded,
    Multipart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContentTypeSource {
    Unset,
    Automatic,
    User,
}

/// Explains where one header in the final request came from. The Body view consumes this
/// projection instead of recreating request-building rules in the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectiveHeaderSource {
    Generated,
    User,
}

/// One enabled header exactly as it will participate in the next Send.
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

/// Editable value for one multipart row. Unlike the transport `MultipartValue`, a file value may
/// intentionally have an empty path while the user is still completing the row.
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

/// Authoritative editable body state for one request tab.
///
/// Form variants intentionally retain disabled, blank, duplicate, ordered, and incomplete rows.
/// `RequestBody` is derived only when the ViewModel builds an effective request.
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

/// State consumed by the response view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResponseState {
    NotSent,
    Loading,
    Cancelled,
    Success {
        status: u16,
        body: String,
        headers: Vec<(String, String)>,
        elapsed_ms: u128,
    },
    /// Read-only, sanitized evidence loaded from one immutable persisted History row.
    Historical {
        entry_id: String,
        response: HistoricalResponse,
    },
    /// A request-only V1 row (or V2 row without response evidence) was selected.
    HistoricalUnavailable {
        entry_id: String,
    },
    Error {
        message: String,
    },
}

impl ResponseState {
    pub fn historical_entry_id(&self) -> Option<&str> {
        match self {
            Self::Historical { entry_id, .. } | Self::HistoricalUnavailable { entry_id } => {
                Some(entry_id)
            }
            Self::NotSent
            | Self::Loading
            | Self::Cancelled
            | Self::Success { .. }
            | Self::Error { .. } => None,
        }
    }
}

/// Stable identity for a request tab. Async completions target this identity rather than
/// whichever tab happens to be active when the server responds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RequestTabId(u64);

impl fmt::Display for RequestTabId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// One open-request match in the application-wide search projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlobalSearchRequestResult {
    pub tab_id: RequestTabId,
    pub display_name: String,
    pub method: HttpMethod,
    pub url: String,
}

/// One persisted History match in the application-wide search projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlobalSearchHistoryResult {
    pub entry_id: String,
    pub display_name: String,
    pub method: HttpMethod,
    pub url: String,
    pub status: Option<u16>,
    pub response_size: Option<usize>,
}

/// Deterministic, grouped search results derived from `WorkspaceViewModel` state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GlobalSearchResults {
    requests: Vec<GlobalSearchRequestResult>,
    history: Vec<GlobalSearchHistoryResult>,
}

impl GlobalSearchResults {
    pub fn requests(&self) -> &[GlobalSearchRequestResult] {
        &self.requests
    }

    pub fn history(&self) -> &[GlobalSearchHistoryResult] {
        &self.history
    }

    pub fn len(&self) -> usize {
        self.requests.len() + self.history.len()
    }

    pub fn is_empty(&self) -> bool {
        self.requests.is_empty() && self.history.is_empty()
    }
}

/// Monotonic identity for one send attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SendId(u64);

impl fmt::Display for SendId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl SendId {
    /// Human-readable identity rendered while one send attempt owns the active lifecycle.
    pub fn request_id(self) -> String {
        format!("req-{:02}", self.0)
    }
}

/// Immutable command emitted by the ViewModel for the application service to execute.
#[derive(Clone, Debug)]
pub struct PendingRequest {
    tab_id: RequestTabId,
    send_id: SendId,
    request: Request,
    editor_intent: Option<RequestEditorIntent>,
    request_options: RequestOptions,
    cancelled: Arc<AtomicBool>,
}

impl PendingRequest {
    pub fn tab_id(&self) -> RequestTabId {
        self.tab_id
    }

    pub fn send_id(&self) -> SendId {
        self.send_id
    }

    pub fn request(&self) -> &Request {
        &self.request
    }

    pub fn editor_intent(&self) -> Option<&RequestEditorIntent> {
        self.editor_intent.as_ref()
    }

    /// Per-request deadline captured at Send. `None` means the deadline is disabled.
    pub fn timeout_ms(&self) -> Option<u64> {
        self.request_options.timeout_ms
    }

    /// Complete wire policy captured when Send was pressed.
    pub fn request_options(&self) -> RequestOptions {
        self.request_options
    }

    fn was_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// Source of truth for one request draft.
///
/// GPUI entities live in the View. This type intentionally has no GPUI dependency, so
/// request construction and response transitions can be tested in isolation.
/// Completed sends are recorded on `WorkspaceViewModel`, not here.
pub struct RequestViewModel {
    tab_id: RequestTabId,
    method: HttpMethod,
    url: String,
    params: Vec<KeyValueRow>,
    param_draft: KeyValueDraft,
    headers: Vec<KeyValueRow>,
    header_draft: KeyValueDraft,
    body_draft: RequestBodyDraft,
    content_type_source: ContentTypeSource,
    accept_source: ContentTypeSource,
    authorization_kind: AuthorizationKind,
    bearer_token: String,
    basic_username: String,
    basic_password: String,
    pre_request_script: String,
    tests_script: String,
    timeout_ms: u64,
    redirect_policy: RedirectPolicy,
    max_redirect_hops: u32,
    request_pane: RequestPane,
    response: ResponseState,
    redirect_chain: Vec<RedirectHop>,
    response_stored_cookies: Vec<CookieJarEntry>,
    pending_send_id: Option<SendId>,
    pending_cancellation: Option<Arc<AtomicBool>>,
    dirty: bool,
}

impl RequestViewModel {
    pub fn new() -> Self {
        Self::for_tab(RequestTabId(0))
    }

    fn for_tab(tab_id: RequestTabId) -> Self {
        Self {
            tab_id,
            method: HttpMethod::GET,
            url: String::new(),
            params: Vec::new(),
            param_draft: KeyValueDraft::default(),
            headers: Vec::new(),
            header_draft: KeyValueDraft::default(),
            body_draft: RequestBodyDraft::None,
            content_type_source: ContentTypeSource::Unset,
            accept_source: ContentTypeSource::Unset,
            authorization_kind: AuthorizationKind::Bearer,
            bearer_token: String::new(),
            basic_username: String::new(),
            basic_password: String::new(),
            pre_request_script: String::new(),
            tests_script: String::new(),
            timeout_ms: 0,
            redirect_policy: RedirectPolicy::Follow,
            max_redirect_hops: DEFAULT_MAX_REDIRECT_HOPS,
            request_pane: RequestPane::Params,
            response: ResponseState::NotSent,
            redirect_chain: Vec::new(),
            response_stored_cookies: Vec::new(),
            pending_send_id: None,
            pending_cancellation: None,
            dirty: false,
        }
    }

    pub fn tab_id(&self) -> RequestTabId {
        self.tab_id
    }

    pub fn method(&self) -> HttpMethod {
        self.method
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// Returns the URL that will be sent. URL input and Params are kept synchronized, so request
    /// construction must not append the same query pairs a second time.
    pub fn effective_url(&self) -> String {
        self.url.clone()
    }

    /// Counts query pairs currently represented in the synchronized URL input.
    pub fn url_query_parameter_count(&self) -> usize {
        query_parameter_count(&self.url)
    }

    /// Counts enabled Params rows, including a valid active draft that already participates in
    /// Send before the user presses Add or changes focus.
    pub fn enabled_param_count(&self) -> usize {
        self.effective_params()
            .iter()
            .filter(|row| row.enabled && !row.key.trim().is_empty())
            .count()
    }

    pub fn params(&self) -> &[KeyValueRow] {
        &self.params
    }

    /// Number of rows rendered by the Params editor. The active row is always visible, even
    /// before it has been confirmed with Add, so each Add action increases this count by one.
    pub fn visible_param_row_count(&self) -> usize {
        self.params.len() + 1
    }

    pub fn headers(&self) -> &[KeyValueRow] {
        &self.headers
    }

    /// Returns the enabled headers produced by the same request-construction path used by Send.
    /// This is a read-only View projection; it never becomes a second header store.
    pub fn effective_headers(&self) -> Vec<EffectiveHeader> {
        let mut generated_content_type = self.content_type_source == ContentTypeSource::Automatic;
        let mut generated_accept = self.accept_source == ContentTypeSource::Automatic;

        self.build_request()
            .headers
            .into_iter()
            .map(|(name, value)| {
                let generated =
                    if generated_content_type && name.eq_ignore_ascii_case("content-type") {
                        generated_content_type = false;
                        true
                    } else if generated_accept && name.eq_ignore_ascii_case("accept") {
                        generated_accept = false;
                        true
                    } else {
                        false
                    };
                EffectiveHeader {
                    name,
                    value,
                    source: if generated {
                        EffectiveHeaderSource::Generated
                    } else {
                        EffectiveHeaderSource::User
                    },
                }
            })
            .collect()
    }

    /// Counts complete, enabled Header rows, including the active row that already participates
    /// in Send before the user presses Add or changes focus.
    pub fn enabled_header_count(&self) -> usize {
        let saved = self
            .headers
            .iter()
            .filter(|row| row.enabled && header_row_is_complete(row))
            .count();
        let active = usize::from(header_draft_is_complete(&self.header_draft));
        saved + active
    }

    /// Number of rows rendered by the Headers editor. As with Params, one active row is always
    /// visible, so every Add Header action increases this count by exactly one.
    pub fn visible_header_row_count(&self) -> usize {
        self.headers.len() + 1
    }

    /// Returns the in-progress row shown by the Params or Headers editor. The draft belongs to
    /// the request tab, not to the text controls, and participates in request construction as
    /// soon as it is valid.
    pub fn row_draft(&self, pane: RequestPane) -> Option<(&str, &str)> {
        let draft = match pane {
            RequestPane::Params => &self.param_draft,
            RequestPane::Headers => &self.header_draft,
            RequestPane::Authorization
            | RequestPane::Body
            | RequestPane::Scripts
            | RequestPane::Tests
            | RequestPane::Options => return None,
        };
        Some((&draft.key, &draft.value))
    }

    /// Returns the text projection used by previews and legacy scenario helpers. URL-encoded
    /// drafts are serialized from their effective rows; multipart drafts remain structured.
    pub fn body(&self) -> String {
        self.body_draft.editor_text()
    }

    pub fn body_draft(&self) -> &RequestBodyDraft {
        &self.body_draft
    }

    /// Derives the body that will be used by Send without mutating or normalizing the draft.
    pub fn request_body(&self) -> RequestBody {
        self.body_draft.effective_body()
    }

    pub fn body_kind(&self) -> BodyKind {
        self.body_draft.kind()
    }

    pub fn bearer_token(&self) -> &str {
        &self.bearer_token
    }

    /// Returns the canonical token that will participate in request construction without
    /// mutating the live editor value. The UI uses this projection to explain the same
    /// transformation that Send applies.
    pub fn normalized_bearer_token(&self) -> String {
        normalize_bearer_token(&self.bearer_token)
    }

    /// Returns the complete managed header exactly as it will be sent. Keeping this projection
    /// next to request construction prevents the UI preview from becoming a second auth policy.
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

    pub fn pre_request_script(&self) -> &str {
        &self.pre_request_script
    }

    pub fn tests_script(&self) -> &str {
        &self.tests_script
    }

    /// Request-level timeout in milliseconds. Zero explicitly disables the deadline.
    pub fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }

    pub fn redirect_policy(&self) -> RedirectPolicy {
        self.redirect_policy
    }

    pub fn max_redirect_hops(&self) -> u32 {
        self.max_redirect_hops
    }

    pub fn request_pane(&self) -> RequestPane {
        self.request_pane
    }

    pub fn response(&self) -> &ResponseState {
        &self.response
    }

    /// Redirect responses observed while producing the active response. Followed chains end in
    /// the terminal response; no-follow and limit failures contain only the redirects observed.
    pub fn redirect_chain(&self) -> &[RedirectHop] {
        &self.redirect_chain
    }

    /// Non-sensitive cookies first stored while producing the active response. This includes
    /// cookies captured on followed redirects, whose Set-Cookie header is absent from the final
    /// response headers.
    pub fn response_stored_cookies(&self) -> &[CookieJarEntry] {
        &self.response_stored_cookies
    }

    pub fn is_sending(&self) -> bool {
        self.pending_send_id.is_some()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn set_method(&mut self, method: HttpMethod) {
        if self.method == method {
            return;
        }
        self.method = method;
        self.dirty = true;

        if method == HttpMethod::POST && matches!(self.body_draft, RequestBodyDraft::None) {
            self.body_draft = RequestBodyDraft::Json(default_json_body());
            self.sync_automatic_content_type();
        } else {
            self.sync_automatic_content_type();
        }
        self.sync_automatic_accept();
    }

    pub fn set_url(&mut self, url: impl Into<String>) {
        let url = url.into();
        if self.url == url {
            return;
        }
        self.params = parse_query_params(&url);
        self.param_draft = KeyValueDraft::default();
        self.url = url;
        self.dirty = true;
    }

    pub fn set_body(&mut self, body: impl Into<String>) {
        let body = body.into();
        let next = match &self.body_draft {
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
        if self.body_draft != next {
            self.body_draft = next;
            self.dirty = true;
        }
    }

    /// Clears the payload without guessing or changing its selected encoding.
    pub fn clear_body(&mut self) {
        let next = RequestBodyDraft::empty_for(self.body_kind());
        if self.body_draft != next {
            self.body_draft = next;
            self.dirty = true;
        }
        self.sync_automatic_content_type();
    }

    pub fn set_body_kind(&mut self, body_kind: BodyKind) {
        if self.body_kind() != body_kind {
            self.body_draft = self.body_draft.converted_to(body_kind);
            self.dirty = true;
        }
        self.sync_automatic_content_type();
    }

    pub fn set_url_encoded_rows(&mut self, rows: Vec<KeyValueRow>) {
        let body_draft = RequestBodyDraft::UrlEncoded(nonempty_url_encoded_rows(rows));
        if self.body_draft != body_draft {
            self.body_draft = body_draft;
            self.dirty = true;
        }
        self.sync_automatic_content_type();
    }

    pub fn set_multipart_draft_parts(&mut self, parts: Vec<MultipartDraftPart>) {
        let body_draft = RequestBodyDraft::Multipart(nonempty_multipart_parts(parts));
        if self.body_draft != body_draft {
            self.body_draft = body_draft;
            self.dirty = true;
        }
        self.sync_automatic_content_type();
    }

    /// Loads an already-effective multipart body as enabled editor rows.
    pub fn set_multipart_parts(&mut self, parts: Vec<MultipartPart>) {
        let draft = RequestBodyDraft::from_request_body(&RequestBody::Multipart(parts));
        let RequestBodyDraft::Multipart(parts) = draft else {
            unreachable!("multipart conversion must produce a multipart draft");
        };
        self.set_multipart_draft_parts(parts);
    }

    pub fn set_bearer_token(&mut self, token: impl Into<String>) {
        // Keep the editor text verbatim while the user is typing. Normalizing on every
        // keystroke would project `Bearer ` back as `Bearer` and swallow its space.
        let token = token.into();
        if self.bearer_token != token {
            self.bearer_token = token;
            self.dirty = true;
        }
    }

    pub fn set_authorization_kind(&mut self, kind: AuthorizationKind) {
        if self.authorization_kind != kind {
            self.authorization_kind = kind;
            self.dirty = true;
        }
    }

    pub fn set_basic_username(&mut self, username: impl Into<String>) {
        let username = username.into();
        if self.basic_username != username {
            self.basic_username = username;
            self.dirty = true;
        }
    }

    pub fn set_basic_password(&mut self, password: impl Into<String>) {
        let password = password.into();
        if self.basic_password != password {
            self.basic_password = password;
            self.dirty = true;
        }
    }

    pub fn set_pre_request_script(&mut self, script: impl Into<String>) {
        let script = script.into();
        if self.pre_request_script != script {
            self.pre_request_script = script;
            self.dirty = true;
        }
    }

    pub fn set_tests_script(&mut self, script: impl Into<String>) {
        let script = script.into();
        if self.tests_script != script {
            self.tests_script = script;
            self.dirty = true;
        }
    }

    pub fn set_timeout_ms(&mut self, timeout_ms: u64) {
        if self.timeout_ms != timeout_ms {
            self.timeout_ms = timeout_ms;
            self.dirty = true;
        }
    }

    pub fn set_redirect_policy(&mut self, redirect_policy: RedirectPolicy) {
        if self.redirect_policy != redirect_policy {
            self.redirect_policy = redirect_policy;
            self.dirty = true;
        }
    }

    pub fn set_max_redirect_hops(&mut self, max_redirect_hops: u32) {
        let max_redirect_hops = max_redirect_hops.clamp(1, MAX_REDIRECT_HOPS);
        if self.max_redirect_hops != max_redirect_hops {
            self.max_redirect_hops = max_redirect_hops;
            self.dirty = true;
        }
    }

    pub fn set_request_pane(&mut self, pane: RequestPane) {
        self.request_pane = pane;
    }

    pub fn set_row_draft_key(&mut self, pane: RequestPane, key: impl Into<String>) {
        let key = key.into();
        let changed = match pane {
            RequestPane::Params if self.param_draft.key != key => {
                self.param_draft.key = key;
                true
            }
            RequestPane::Headers if self.header_draft.key != key => {
                self.header_draft.key = key;
                true
            }
            RequestPane::Params
            | RequestPane::Headers
            | RequestPane::Authorization
            | RequestPane::Body
            | RequestPane::Scripts
            | RequestPane::Tests
            | RequestPane::Options => false,
        };
        if changed {
            if pane == RequestPane::Params {
                self.sync_url_from_params();
            }
            self.dirty = true;
        }
    }

    pub fn set_row_draft_value(&mut self, pane: RequestPane, value: impl Into<String>) {
        let value = value.into();
        let changed = match pane {
            RequestPane::Params if self.param_draft.value != value => {
                self.param_draft.value = value;
                true
            }
            RequestPane::Headers if self.header_draft.value != value => {
                self.header_draft.value = value;
                true
            }
            RequestPane::Params
            | RequestPane::Headers
            | RequestPane::Authorization
            | RequestPane::Body
            | RequestPane::Scripts
            | RequestPane::Tests
            | RequestPane::Options => false,
        };
        if changed {
            if pane == RequestPane::Params {
                self.sync_url_from_params();
            }
            self.dirty = true;
        }
    }

    /// Preserves the current Params row and appends one fresh Key/Value row.
    ///
    /// Empty rows are intentional and there is no row limit: every call appends exactly one row,
    /// so users can click Add as often as needed before entering any values. The active row already
    /// participates in Send as soon as it has a key, so adding another row is never required merely
    /// to persist input.
    pub fn append_param_row(&mut self) {
        let draft = std::mem::take(&mut self.param_draft);
        self.params
            .push(KeyValueRow::enabled(draft.key, draft.value));
        self.sync_url_from_params();
        self.dirty = true;
    }

    /// Preserves the current Header row and appends one fresh Header name/value row.
    ///
    /// Empty rows are intentional and unlimited. Header controls are editing buffers only; once a
    /// preserved row is edited, every keystroke is written directly to that indexed ViewModel row.
    pub fn append_header_row(&mut self) {
        let draft = std::mem::take(&mut self.header_draft);
        if draft.key.eq_ignore_ascii_case("content-type") {
            self.content_type_source = ContentTypeSource::User;
        }
        if draft.key.eq_ignore_ascii_case("accept") {
            self.accept_source = ContentTypeSource::User;
        }
        self.headers
            .push(KeyValueRow::enabled(draft.key, draft.value));
        self.dirty = true;
    }

    /// Confirms the active row for the selected key/value editor.
    pub fn commit_row_draft(&mut self, pane: RequestPane) {
        match pane {
            RequestPane::Params => self.append_param_row(),
            RequestPane::Headers => self.append_header_row(),
            RequestPane::Authorization
            | RequestPane::Body
            | RequestPane::Scripts
            | RequestPane::Tests
            | RequestPane::Options => {}
        }
    }

    pub fn upsert_param(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        if key.trim().is_empty() {
            return;
        }
        let value = value.into();
        if let Some(row) = self.params.iter_mut().find(|row| row.key == key) {
            row.value = value;
            row.enabled = true;
        } else {
            self.params.push(KeyValueRow::enabled(key, value));
        }
        self.sync_url_from_params();
        self.dirty = true;
    }

    /// Updates one persistent Params row. Text controls are only editing buffers; every keystroke
    /// is written here so Send never needs to scrape or manually commit the rendered controls.
    pub fn set_param_key(&mut self, index: usize, key: impl Into<String>) {
        let key = key.into();
        let Some(row) = self.params.get_mut(index) else {
            return;
        };
        if row.key == key {
            return;
        }
        row.key = key;
        self.sync_url_from_params();
        self.dirty = true;
    }

    pub fn set_param_value(&mut self, index: usize, value: impl Into<String>) {
        let value = value.into();
        let Some(row) = self.params.get_mut(index) else {
            return;
        };
        if row.value == value {
            return;
        }
        row.value = value;
        self.sync_url_from_params();
        self.dirty = true;
    }

    pub fn toggle_param(&mut self, index: usize) {
        if let Some(row) = self.params.get_mut(index) {
            row.enabled = !row.enabled;
            self.sync_url_from_params();
            self.dirty = true;
        }
    }

    pub fn remove_param(&mut self, index: usize) {
        if index < self.params.len() {
            self.params.remove(index);
            self.sync_url_from_params();
            self.dirty = true;
        }
    }

    pub fn upsert_header(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let value = value.into();
        if key.trim().is_empty() || value.trim().is_empty() {
            return;
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
            self.content_type_source = ContentTypeSource::User;
        }
        if is_accept {
            self.accept_source = ContentTypeSource::User;
        }
        self.dirty = true;
    }

    /// Updates one persistent Header row. Duplicate names remain independent because Header rows
    /// model ordered request fields rather than a key-addressed map.
    pub fn set_header_key(&mut self, index: usize, key: impl Into<String>) {
        let key = key.into();
        let Some(row) = self.headers.get_mut(index) else {
            return;
        };
        if row.key == key {
            return;
        }
        if row.key.eq_ignore_ascii_case("content-type") || key.eq_ignore_ascii_case("content-type")
        {
            self.content_type_source = ContentTypeSource::User;
        }
        if row.key.eq_ignore_ascii_case("accept") || key.eq_ignore_ascii_case("accept") {
            self.accept_source = ContentTypeSource::User;
        }
        row.key = key;
        self.dirty = true;
    }

    pub fn set_header_value(&mut self, index: usize, value: impl Into<String>) {
        let value = value.into();
        let Some(row) = self.headers.get_mut(index) else {
            return;
        };
        if row.value == value {
            return;
        }
        if row.key.eq_ignore_ascii_case("content-type") {
            self.content_type_source = ContentTypeSource::User;
        }
        if row.key.eq_ignore_ascii_case("accept") {
            self.accept_source = ContentTypeSource::User;
        }
        row.value = value;
        self.dirty = true;
    }

    pub fn clear_header_draft(&mut self) {
        if self.header_draft != KeyValueDraft::default() {
            self.header_draft = KeyValueDraft::default();
            self.dirty = true;
        }
    }

    pub fn toggle_header(&mut self, index: usize) {
        if let Some(row) = self.headers.get_mut(index) {
            if row.key.eq_ignore_ascii_case("content-type") {
                self.content_type_source = ContentTypeSource::User;
            }
            if row.key.eq_ignore_ascii_case("accept") {
                self.accept_source = ContentTypeSource::User;
            }
            row.enabled = !row.enabled;
            self.dirty = true;
        }
    }

    pub fn remove_header(&mut self, index: usize) {
        if index < self.headers.len() {
            if self.headers[index].key.eq_ignore_ascii_case("content-type") {
                self.content_type_source = ContentTypeSource::User;
            }
            if self.headers[index].key.eq_ignore_ascii_case("accept") {
                self.accept_source = ContentTypeSource::User;
            }
            self.headers.remove(index);
            self.dirty = true;
        }
    }

    pub fn new_request(&mut self) {
        self.mark_pending_cancelled();
        self.method = HttpMethod::GET;
        self.url.clear();
        self.params.clear();
        self.param_draft = KeyValueDraft::default();
        self.headers.clear();
        self.header_draft = KeyValueDraft::default();
        self.body_draft = RequestBodyDraft::None;
        self.content_type_source = ContentTypeSource::Unset;
        self.accept_source = ContentTypeSource::Unset;
        self.authorization_kind = AuthorizationKind::Bearer;
        self.bearer_token.clear();
        self.basic_username.clear();
        self.basic_password.clear();
        self.timeout_ms = 0;
        self.redirect_policy = RedirectPolicy::Follow;
        self.max_redirect_hops = DEFAULT_MAX_REDIRECT_HOPS;
        self.request_pane = RequestPane::Params;
        self.response = ResponseState::NotSent;
        self.redirect_chain.clear();
        self.response_stored_cookies.clear();
        self.dirty = false;
    }

    pub fn load_request(&mut self, request: &Request) {
        self.mark_pending_cancelled();
        self.method = request.method;
        self.url = request.url.clone();
        self.params = parse_query_params(&request.url);
        self.param_draft = KeyValueDraft::default();
        self.header_draft = KeyValueDraft::default();
        self.authorization_kind = AuthorizationKind::Bearer;
        self.bearer_token.clear();
        self.basic_username.clear();
        self.basic_password.clear();
        self.pre_request_script.clear();
        self.tests_script.clear();
        self.timeout_ms = 0;
        self.redirect_policy = RedirectPolicy::Follow;
        self.max_redirect_hops = DEFAULT_MAX_REDIRECT_HOPS;

        let authorization = request
            .headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("authorization"))
            .map(|(_, value)| value.as_str());
        let manages_authorization = if let Some(value) = authorization {
            if let Some((username, password)) = decode_basic_credentials(value) {
                self.authorization_kind = AuthorizationKind::Basic;
                self.basic_username = username;
                self.basic_password = password;
                true
            } else if let Some(token) = bearer_token_from_header(value) {
                self.bearer_token = token;
                true
            } else {
                false
            }
        } else {
            false
        };
        self.headers = request
            .headers
            .iter()
            .filter(|(key, _)| {
                !(manages_authorization && key.eq_ignore_ascii_case("authorization"))
            })
            .map(|(key, value)| KeyValueRow::enabled(key, value))
            .collect();
        self.body_draft = RequestBodyDraft::from_request_body(&request.body);
        // A loaded request is an exact saved draft. Its managed headers, including an
        // intentional absence, must not be replaced by automatic defaults.
        self.content_type_source = ContentTypeSource::User;
        self.accept_source = ContentTypeSource::User;
        self.request_pane = if request.body.is_empty() {
            RequestPane::Headers
        } else {
            RequestPane::Body
        };
        self.response = ResponseState::NotSent;
        self.redirect_chain.clear();
        self.response_stored_cookies.clear();
        self.dirty = false;
    }

    fn load_history_entry(&mut self, entry: &HistoryEntry, replay_request: &Request) {
        self.load_request(replay_request);
        self.timeout_ms = entry.request_options.timeout_ms.unwrap_or(0);
        self.redirect_policy = entry.request_options.redirect_policy;
        self.max_redirect_hops = entry.request_options.max_redirect_hops;
        if let Some(intent) = &entry.editor_intent {
            self.body_draft = RequestBodyDraft::from_editor_intent(intent);
            self.request_pane = RequestPane::Body;
            self.dirty = false;
        }
        self.response = entry.historical_response.clone().map_or_else(
            || ResponseState::HistoricalUnavailable {
                entry_id: entry.id.clone(),
            },
            |response| ResponseState::Historical {
                entry_id: entry.id.clone(),
                response,
            },
        );
        self.redirect_chain.clear();
        // Persisted History deliberately contains no cookie-jar evidence.
        self.response_stored_cookies.clear();
    }

    fn begin_send(&mut self, send_id: SendId, cancelled: Arc<AtomicBool>) -> Request {
        if self.authorization_kind == AuthorizationKind::Bearer {
            self.bearer_token = normalize_bearer_token(&self.bearer_token);
        }
        let request = self.build_request();
        self.pending_send_id = Some(send_id);
        self.pending_cancellation = Some(cancelled);
        self.response = ResponseState::Loading;
        self.redirect_chain.clear();
        self.response_stored_cookies.clear();
        request
    }

    fn complete_send(
        &mut self,
        pending: &PendingRequest,
        result: Result<RequestResult, AppError>,
        stored_cookies: Vec<CookieJarEntry>,
    ) -> bool {
        if self.pending_send_id != Some(pending.send_id) {
            return false;
        }

        self.pending_send_id = None;
        self.pending_cancellation = None;
        match result {
            Ok(result) => {
                let draft_is_unchanged = self.build_request() == pending.request
                    && self.timeout_ms == pending.request_options.timeout_ms.unwrap_or(0)
                    && self.redirect_policy == pending.request_options.redirect_policy
                    && self.max_redirect_hops == pending.request_options.max_redirect_hops;
                self.redirect_chain = result.redirect_chain;
                self.response = ResponseState::Success {
                    status: result.status,
                    body: result.body,
                    headers: result.headers,
                    elapsed_ms: result.elapsed_ms,
                };
                self.response_stored_cookies = stored_cookies;
                if draft_is_unchanged {
                    self.dirty = false;
                }
            }
            Err(error) => {
                self.redirect_chain = error.redirect_chain().to_vec();
                self.response_stored_cookies.clear();
                self.response = ResponseState::Error {
                    message: error.to_string(),
                };
            }
        }
        true
    }

    fn cancel_send(&mut self, send_id: SendId) -> bool {
        if self.pending_send_id != Some(send_id) {
            return false;
        }
        self.pending_send_id = None;
        if let Some(cancelled) = self.pending_cancellation.take() {
            cancelled.store(true, Ordering::Release);
        }
        self.response = ResponseState::Cancelled;
        self.redirect_chain.clear();
        true
    }

    fn mark_pending_cancelled(&mut self) {
        self.pending_send_id = None;
        if let Some(cancelled) = self.pending_cancellation.take() {
            cancelled.store(true, Ordering::Release);
        }
    }

    pub fn tab_title(&self) -> String {
        if self.url.trim().is_empty() {
            return "Untitled request".to_string();
        }
        let without_scheme = self
            .url
            .split_once("://")
            .map(|(_, value)| value)
            .unwrap_or(&self.url);
        let title: String = without_scheme.chars().take(28).collect();
        if without_scheme.chars().count() > 28 {
            format!("{title}…")
        } else {
            title
        }
    }

    fn build_request(&self) -> Request {
        let mut request = Request::new(self.method, self.effective_url());
        request.headers = self
            .headers
            .iter()
            .filter(|row| row.enabled && header_row_is_complete(row))
            .map(|row| (row.key.clone(), row.value.clone()))
            .collect();

        let draft_key = self.header_draft.key.trim();
        let draft_value = self.header_draft.value.trim();
        if header_draft_is_complete(&self.header_draft) {
            request.add_header(draft_key, draft_value);
        }

        let authorization = self.authorization_header_value();
        if let Some(value) = authorization {
            // Authorization is managed by this editor mode. Remove every manually-entered
            // variant first so the transport cannot emit two competing credentials.
            request
                .headers
                .retain(|(key, _)| !key.eq_ignore_ascii_case("authorization"));
            request.add_header("Authorization", value);
        }

        if self.method.allows_body() {
            if self.content_type_source != ContentTypeSource::User
                && !request
                    .headers
                    .iter()
                    .any(|(key, _)| key.eq_ignore_ascii_case("content-type"))
            {
                if let Some(value) = content_type_for(self.body_kind()) {
                    request.add_header("Content-Type", value);
                }
            }
            request.body = self.request_body();
        }
        if self.method == HttpMethod::POST
            && self.accept_source != ContentTypeSource::User
            && !request
                .headers
                .iter()
                .any(|(key, _)| key.eq_ignore_ascii_case("accept"))
        {
            request.add_header("Accept", "application/json");
        }
        request
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

    fn sync_automatic_content_type(&mut self) {
        if self.content_type_source == ContentTypeSource::User {
            return;
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
            (ContentTypeSource::User, _, _) => unreachable!("handled above"),
            (ContentTypeSource::Unset, Some(_), _) => {
                // This can only come from a loaded or legacy draft; preserve it.
                self.content_type_source = ContentTypeSource::User;
            }
            (_, Some(index), Some(value)) => {
                let row = &mut self.headers[index];
                if row.value != value || !row.enabled {
                    row.value = value.to_string();
                    row.enabled = true;
                    self.dirty = true;
                }
                self.content_type_source = ContentTypeSource::Automatic;
            }
            (_, None, Some(value)) => {
                self.headers
                    .push(KeyValueRow::enabled("Content-Type", value));
                self.content_type_source = ContentTypeSource::Automatic;
                self.dirty = true;
            }
            (ContentTypeSource::Automatic, Some(index), None) => {
                self.headers.remove(index);
                self.content_type_source = ContentTypeSource::Unset;
                self.dirty = true;
            }
            (_, None, None) => {
                self.content_type_source = ContentTypeSource::Unset;
            }
        }
    }

    fn sync_automatic_accept(&mut self) {
        if self.accept_source == ContentTypeSource::User {
            return;
        }

        let desired = (self.method == HttpMethod::POST).then_some("application/json");
        let accept_index = self
            .headers
            .iter()
            .position(|row| row.key.eq_ignore_ascii_case("accept"));

        match (self.accept_source, accept_index, desired) {
            (ContentTypeSource::User, _, _) => unreachable!("handled above"),
            (ContentTypeSource::Unset, Some(_), _) => {
                // An existing value predates automatic management and is therefore user-owned.
                self.accept_source = ContentTypeSource::User;
            }
            (_, Some(index), Some(value)) => {
                let row = &mut self.headers[index];
                if row.value != value || !row.enabled {
                    row.value = value.to_string();
                    row.enabled = true;
                    self.dirty = true;
                }
                self.accept_source = ContentTypeSource::Automatic;
            }
            (_, None, Some(value)) => {
                self.headers.push(KeyValueRow::enabled("Accept", value));
                self.accept_source = ContentTypeSource::Automatic;
                self.dirty = true;
            }
            (ContentTypeSource::Automatic, Some(index), None) => {
                self.headers.remove(index);
                self.accept_source = ContentTypeSource::Unset;
                self.dirty = true;
            }
            (_, None, None) => {
                self.accept_source = ContentTypeSource::Unset;
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

/// Non-sensitive projection of one application-session cookie. Cookie values stay exclusively in
/// the transport jar; the ViewModel exposes only enough metadata to make storage and clearing
/// observable through rendered controls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CookieJarEntry {
    pub origin: String,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryStorageStage {
    Initialize,
    Load,
    Append,
    Clear,
}

impl fmt::Display for HistoryStorageStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Initialize => "initialization",
            Self::Load => "load",
            Self::Append => "append",
            Self::Clear => "clear",
        })
    }
}

/// Observable state of the SQLite-backed History feature. Entries remain only the latest
/// successful database query result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryStorageStatus {
    Loading {
        stage: HistoryStorageStage,
    },
    Ready {
        skipped_rows: usize,
    },
    Error {
        stage: HistoryStorageStage,
        message: String,
    },
}

/// Result of applying one HTTP completion to request/response state. A completed exchange yields
/// a History candidate, but the ViewModel never adds it to the visible database query result.
#[derive(Debug)]
pub struct SendCompletion {
    response_applied: bool,
    history_entry: Option<HistoryEntry>,
}

impl SendCompletion {
    pub fn response_applied(&self) -> bool {
        self.response_applied
    }

    pub fn history_entry(&self) -> Option<&HistoryEntry> {
        self.history_entry.as_ref()
    }

    pub fn into_parts(self) -> (bool, Option<HistoryEntry>) {
        (self.response_applied, self.history_entry)
    }
}

/// Application-level ViewModel. It owns request tabs and the latest SQLite History query result.
pub struct WorkspaceViewModel {
    tabs: Vec<RequestViewModel>,
    active_tab_id: Option<RequestTabId>,
    history: RequestHistory,
    /// Current-process complete Requests keyed by SQLite-confirmed History IDs. This is not a
    /// second History store: it has no ordering or metadata, is never rendered independently,
    /// and is pruned whenever the authoritative SQLite query result changes. It only preserves
    /// credentials stripped at the persistence boundary for same-session replay.
    runtime_replay_requests: HashMap<String, Request>,
    history_storage_status: HistoryStorageStatus,
    cookie_jar: Vec<CookieJarEntry>,
    last_cookie_clear_count: Option<usize>,
    next_tab_id: u64,
    next_send_id: u64,
}

impl WorkspaceViewModel {
    pub fn new() -> Self {
        Self::with_request(RequestViewModel::new())
    }

    pub fn with_request(mut request: RequestViewModel) -> Self {
        request.tab_id = RequestTabId(1);
        Self {
            tabs: vec![request],
            active_tab_id: Some(RequestTabId(1)),
            history: RequestHistory::new(),
            runtime_replay_requests: HashMap::new(),
            history_storage_status: HistoryStorageStatus::Loading {
                stage: HistoryStorageStage::Initialize,
            },
            cookie_jar: Vec::new(),
            last_cookie_clear_count: None,
            next_tab_id: 2,
            next_send_id: 1,
        }
    }

    pub fn tabs(&self) -> &[RequestViewModel] {
        &self.tabs
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Stable identity of the request tab currently targeted by synchronous editor actions.
    ///
    /// The public workspace lifecycle keeps at least one tab open, but returning `Option` makes
    /// an absent or stale active-tab selection explicit instead of turning it into an indexing
    /// panic or silently choosing another tab.
    pub fn active_tab_id(&self) -> Option<RequestTabId> {
        self.active_tab_id
            .filter(|tab_id| self.request_for_tab(*tab_id).is_some())
    }

    pub fn active_tab_index(&self) -> Option<usize> {
        self.active_tab_id()
            .and_then(|tab_id| self.tab_index(tab_id))
    }

    pub fn tab_index(&self, tab_id: RequestTabId) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.tab_id == tab_id)
    }

    /// The active request, if the selected stable identity still belongs to this workspace.
    pub fn active_request(&self) -> Option<&RequestViewModel> {
        self.active_tab_id
            .and_then(|tab_id| self.request_for_tab(tab_id))
    }

    /// Mutable access to the active request. Callers must choose this API explicitly instead of
    /// obtaining a mutable request through workspace deref coercion.
    pub fn active_request_mut(&mut self) -> Option<&mut RequestViewModel> {
        let tab_id = self.active_tab_id?;
        self.request_for_tab_mut(tab_id)
    }

    pub fn request_for_tab(&self, tab_id: RequestTabId) -> Option<&RequestViewModel> {
        self.tabs.iter().find(|tab| tab.tab_id == tab_id)
    }

    pub fn request_for_tab_mut(&mut self, tab_id: RequestTabId) -> Option<&mut RequestViewModel> {
        self.tabs.iter_mut().find(|tab| tab.tab_id == tab_id)
    }

    pub fn update_active_request<R>(
        &mut self,
        update: impl FnOnce(&mut RequestViewModel) -> R,
    ) -> Option<R> {
        self.active_request_mut().map(update)
    }

    pub fn update_request_for_tab<R>(
        &mut self,
        tab_id: RequestTabId,
        update: impl FnOnce(&mut RequestViewModel) -> R,
    ) -> Option<R> {
        self.request_for_tab_mut(tab_id).map(update)
    }

    pub fn select_tab(&mut self, index: usize) -> bool {
        let Some(tab_id) = self.tabs.get(index).map(RequestViewModel::tab_id) else {
            return false;
        };
        if self.active_tab_id == Some(tab_id) {
            return false;
        }
        self.active_tab_id = Some(tab_id);
        true
    }

    pub fn select_tab_by_id(&mut self, tab_id: RequestTabId) -> bool {
        self.tab_index(tab_id)
            .is_some_and(|index| self.select_tab(index))
    }

    /// Search open request tabs and the latest authoritative History query result.
    ///
    /// Ordering is inherited from the two source collections: request tabs stay in tab order and
    /// History stays newest-first. Matching here keeps the application shell from growing a
    /// second request/history data model.
    pub fn global_search_results(&self, query: &str) -> GlobalSearchResults {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return GlobalSearchResults::default();
        }

        let matches = |display_name: &str, method: HttpMethod, url: &str| {
            display_name.to_lowercase().contains(&query)
                || method.to_string().to_lowercase().contains(&query)
                || url.to_lowercase().contains(&query)
        };

        let requests = self
            .tabs
            .iter()
            .filter_map(|request| {
                let display_name = request.tab_title();
                matches(&display_name, request.method, &request.url).then(|| {
                    GlobalSearchRequestResult {
                        tab_id: request.tab_id,
                        display_name,
                        method: request.method,
                        url: request.url.clone(),
                    }
                })
            })
            .collect();
        let history = self
            .history
            .entries()
            .iter()
            .filter(|entry| matches(&entry.name, entry.request.method, &entry.request.url))
            .map(|entry| GlobalSearchHistoryResult {
                entry_id: entry.id.clone(),
                display_name: entry.name.clone(),
                method: entry.request.method,
                url: entry.request.url.clone(),
                status: entry.status,
                response_size: entry.response_size,
            })
            .collect();

        GlobalSearchResults { requests, history }
    }

    pub fn new_request(&mut self) {
        let tab_id = RequestTabId(self.next_tab_id);
        self.next_tab_id += 1;
        self.tabs.push(RequestViewModel::for_tab(tab_id));
        self.active_tab_id = Some(tab_id);
    }

    pub fn close_tab(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() {
            return false;
        }

        if self.tabs.len() == 1 {
            self.tabs[0].new_request();
            self.active_tab_id = Some(self.tabs[0].tab_id);
            return true;
        }

        let closing_tab_id = self.tabs[index].tab_id;
        self.tabs[index].mark_pending_cancelled();
        self.tabs.remove(index);
        if self.active_tab_id == Some(closing_tab_id) {
            let next_index = index.min(self.tabs.len() - 1);
            self.active_tab_id = Some(self.tabs[next_index].tab_id);
        } else if self
            .active_tab_id
            .is_some_and(|tab_id| self.tab_index(tab_id).is_none())
        {
            self.active_tab_id = None;
        }
        true
    }

    pub fn close_tab_by_id(&mut self, tab_id: RequestTabId) -> bool {
        self.tab_index(tab_id)
            .is_some_and(|index| self.close_tab(index))
    }

    pub fn begin_send(&mut self) -> Option<PendingRequest> {
        let tab_id = self.active_tab_id()?;
        let send_id = SendId(self.next_send_id);
        self.next_send_id += 1;
        let cancelled = Arc::new(AtomicBool::new(false));
        let tab = self.request_for_tab_mut(tab_id)?;
        let editor_intent = tab.body_draft.editor_intent();
        let request_options = RequestOptions {
            timeout_ms: (tab.timeout_ms > 0).then_some(tab.timeout_ms),
            redirect_policy: tab.redirect_policy,
            max_redirect_hops: tab.max_redirect_hops,
        };
        let request = tab.begin_send(send_id, cancelled.clone());
        let pending = PendingRequest {
            tab_id: tab.tab_id,
            send_id,
            request,
            editor_intent,
            request_options,
            cancelled,
        };
        tracing::info!(
            send_id = %pending.send_id,
            tab_id = %pending.tab_id,
            method = %pending.request.method,
            url = %display_url_for_log(&pending.request.url),
            "request started"
        );
        Some(pending)
    }

    pub fn active_send_id(&self) -> Option<SendId> {
        self.active_request().and_then(|tab| tab.pending_send_id)
    }

    pub fn active_request_id(&self) -> Option<String> {
        self.active_send_id().map(SendId::request_id)
    }

    /// Number of request tabs whose active send has not reached a terminal state.
    pub fn in_flight_count(&self) -> usize {
        self.tabs
            .iter()
            .filter(|tab| tab.pending_send_id.is_some())
            .count()
    }

    pub fn send_id_for_tab(&self, index: usize) -> Option<SendId> {
        self.tabs.get(index).and_then(|tab| tab.pending_send_id)
    }

    pub fn send_id_for_tab_id(&self, tab_id: RequestTabId) -> Option<SendId> {
        self.tab_index(tab_id)
            .and_then(|index| self.send_id_for_tab(index))
    }

    pub fn cancel_send(&mut self, send_id: SendId) -> bool {
        let cancelled = self
            .tabs
            .iter_mut()
            .find(|tab| tab.pending_send_id == Some(send_id))
            .is_some_and(|tab| tab.cancel_send(send_id));
        if cancelled {
            tracing::info!(send_id = %send_id, "request cancelled");
        }
        cancelled
    }

    /// Applies response state but deliberately does not mutate visible History. Application hosts
    /// that persist History must use `complete_send_for_persistence` and commit its candidate.
    pub fn complete_send(
        &mut self,
        pending: PendingRequest,
        result: Result<RequestResult, AppError>,
    ) -> bool {
        self.complete_send_for_persistence(pending, result)
            .response_applied()
    }

    pub fn complete_send_for_persistence(
        &mut self,
        pending: PendingRequest,
        result: Result<RequestResult, AppError>,
    ) -> SendCompletion {
        self.complete_send_with_stored_cookies(pending, result, Vec::new())
    }

    pub(crate) fn complete_send_with_stored_cookies(
        &mut self,
        pending: PendingRequest,
        result: Result<RequestResult, AppError>,
        stored_cookies: Vec<(String, String)>,
    ) -> SendCompletion {
        let was_cancelled = pending.was_cancelled();
        let completed_response = result.as_ref().ok().map(|response| {
            HistoricalResponse::completed(
                response.status,
                response.headers.clone(),
                response.body.clone(),
                response.elapsed_ms,
            )
        });
        match &result {
            Ok(response) if !was_cancelled => {
                tracing::info!(
                    send_id = %pending.send_id,
                    tab_id = %pending.tab_id,
                    status = response.status,
                    elapsed_ms = response.elapsed_ms,
                    "request completed"
                );
            }
            Err(error) if !was_cancelled => {
                tracing::warn!(
                    send_id = %pending.send_id,
                    tab_id = %pending.tab_id,
                    error = %error,
                    "request failed"
                );
            }
            _ => {
                tracing::debug!(
                    send_id = %pending.send_id,
                    tab_id = %pending.tab_id,
                    cancelled = was_cancelled,
                    "ignored stale request completion"
                );
            }
        }
        let stored_cookies = stored_cookies
            .into_iter()
            .map(|(origin, name)| CookieJarEntry { origin, name })
            .collect();
        let applied = self
            .tabs
            .iter_mut()
            .find(|tab| tab.tab_id == pending.tab_id)
            .is_some_and(|tab| tab.complete_send(&pending, result, stored_cookies));
        let history_entry = completed_response
            .filter(|_| !was_cancelled)
            .map(|response| {
                HistoryEntry::completed_with_intent_and_options(
                    pending.request.clone(),
                    history_label(&pending.request.url),
                    response.status,
                    response.elapsed_ms,
                    response.original_size,
                    pending.editor_intent.clone(),
                    pending.request_options,
                )
                .with_historical_response(response)
            });
        SendCompletion {
            response_applied: applied,
            history_entry,
        }
    }

    pub fn load_request(&mut self, request: &Request) -> bool {
        self.update_active_request(|tab| tab.load_request(request))
            .is_some()
    }

    pub fn load_history_entry(&mut self, entry: &HistoryEntry) -> bool {
        let replay_request = self
            .runtime_replay_requests
            .get(&entry.id)
            .unwrap_or(&entry.request)
            .clone();
        self.update_active_request(|tab| tab.load_history_entry(entry, &replay_request))
            .is_some()
    }

    pub fn request_editor_intent(&self) -> Option<RequestEditorIntent> {
        self.active_request()
            .and_then(|tab| tab.body_draft.editor_intent())
    }

    pub fn history(&self) -> &[HistoryEntry] {
        self.history.entries()
    }

    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    pub fn history_storage_status(&self) -> &HistoryStorageStatus {
        &self.history_storage_status
    }

    pub(crate) fn set_history_loading(&mut self, stage: HistoryStorageStage) {
        self.history_storage_status = HistoryStorageStatus::Loading { stage };
    }

    /// Apply only rows confirmed by a successful SQLite query.
    pub(crate) fn replace_history_query_result(
        &mut self,
        entries: Vec<HistoryEntry>,
        skipped_rows: usize,
    ) {
        self.history.replace(entries);
        let retained_ids = self
            .history
            .entries()
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<std::collections::HashSet<_>>();
        self.runtime_replay_requests
            .retain(|entry_id, _| retained_ids.contains(entry_id.as_str()));
        for tab in &mut self.tabs {
            if tab
                .response
                .historical_entry_id()
                .is_some_and(|entry_id| !retained_ids.contains(entry_id))
            {
                tab.response = ResponseState::NotSent;
                tab.redirect_chain.clear();
                tab.response_stored_cookies.clear();
            }
        }
        self.history_storage_status = HistoryStorageStatus::Ready { skipped_rows };
    }

    /// Attach a complete Request only after its History ID has been returned by SQLite.
    /// Recovered rows intentionally have no overlay and therefore replay their sanitized request.
    pub(crate) fn confirm_runtime_replay_request(&mut self, entry_id: String, request: Request) {
        if self
            .history
            .entries()
            .iter()
            .any(|entry| entry.id == entry_id)
        {
            self.runtime_replay_requests.insert(entry_id, request);
        }
    }

    pub(crate) fn set_history_storage_error(
        &mut self,
        stage: HistoryStorageStage,
        message: impl Into<String>,
    ) {
        self.history_storage_status = HistoryStorageStatus::Error {
            stage,
            message: message.into(),
        };
    }

    pub fn cookies(&self) -> &[CookieJarEntry] {
        &self.cookie_jar
    }

    pub fn cookie_count(&self) -> usize {
        self.cookie_jar.len()
    }

    pub fn last_cookie_clear_count(&self) -> Option<usize> {
        self.last_cookie_clear_count
    }

    pub(crate) fn sync_cookie_jar(&mut self, snapshot: Vec<(String, String)>) {
        let mut cookie_jar = snapshot
            .into_iter()
            .map(|(origin, name)| CookieJarEntry { origin, name })
            .collect::<Vec<_>>();
        cookie_jar.sort_by(|left, right| {
            left.origin
                .cmp(&right.origin)
                .then_with(|| left.name.cmp(&right.name))
        });
        cookie_jar.dedup();
        if !cookie_jar.is_empty() {
            self.last_cookie_clear_count = None;
        }
        self.cookie_jar = cookie_jar;
    }

    pub(crate) fn record_cookies_cleared(&mut self, cleared: usize) {
        self.cookie_jar.clear();
        self.last_cookie_clear_count = Some(cleared);
    }
}

impl Default for WorkspaceViewModel {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for RequestViewModel {
    fn default() -> Self {
        Self::new()
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

fn history_label(url: &str) -> String {
    if url.chars().count() > MAX_HISTORY_URL_LENGTH {
        format!(
            "{}…",
            url.chars().take(MAX_HISTORY_URL_LENGTH).collect::<String>()
        )
    } else {
        url.to_string()
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
    use crate::persistence::VersionedHistorySnapshot;

    /// Unit-test stand-in for the application lifecycle: complete the response, cross the
    /// sanitized snapshot boundary, then replace History as if SQLite had returned the rows.
    fn complete_and_confirm_history(
        workspace: &mut WorkspaceViewModel,
        pending: PendingRequest,
        result: Result<RequestResult, AppError>,
    ) -> bool {
        let (response_applied, candidate) = workspace
            .complete_send_for_persistence(pending, result)
            .into_parts();
        if let Some(candidate) = candidate {
            let snapshot = VersionedHistorySnapshot::try_from(&candidate).unwrap();
            let confirmed = HistoryEntry::try_from(snapshot).unwrap();
            let mut entries = workspace.history().to_vec();
            entries.insert(0, confirmed);
            workspace.replace_history_query_result(entries, 0);
        }
        response_applied
    }

    #[test]
    fn global_search_projects_grouped_case_insensitive_results_in_source_order() {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://alpha.example/users");
        workspace.new_request();
        workspace
            .active_request_mut()
            .unwrap()
            .set_method(HttpMethod::POST);
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://beta.example/orders");

        let newest = HistoryEntry::completed(
            Request::new(HttpMethod::DELETE, "https://archive.example/shared/newest"),
            "Shared audit request".into(),
            204,
            3,
            0,
        );
        let older = HistoryEntry::completed(
            Request::new(HttpMethod::GET, "https://archive.example/shared/older"),
            "Shared lookup request".into(),
            200,
            5,
            12,
        );
        workspace.replace_history_query_result(vec![newest.clone(), older.clone()], 0);

        let method_matches = workspace.global_search_results("  pOsT ");
        assert_eq!(method_matches.requests().len(), 1);
        assert_eq!(
            method_matches.requests()[0].tab_id,
            workspace.tabs()[1].tab_id()
        );
        assert!(method_matches.history().is_empty());

        let name_matches = workspace.global_search_results("SHARED");
        assert!(name_matches.requests().is_empty());
        assert_eq!(
            name_matches
                .history()
                .iter()
                .map(|result| result.entry_id.as_str())
                .collect::<Vec<_>>(),
            vec![newest.id.as_str(), older.id.as_str()]
        );

        let url_matches = workspace.global_search_results("alpha.EXAMPLE");
        assert_eq!(url_matches.requests().len(), 1);
        assert_eq!(url_matches.requests()[0].url, "https://alpha.example/users");
        assert!(url_matches.history().is_empty());

        assert!(workspace.global_search_results("  ").is_empty());
        workspace.close_tab(0);
        assert!(workspace.global_search_results("alpha.example").is_empty());
    }

    #[test]
    fn pasted_url_query_is_projected_into_params_and_stays_synchronized() {
        let mut vm = RequestViewModel::new();
        vm.set_url("https://httpbingo.org/get?existing=1&q=rust+gpui&locale=%E4%B8%AD%E6%96%87");
        assert_eq!(
            vm.params(),
            &[
                KeyValueRow::enabled("existing", "1"),
                KeyValueRow::enabled("q", "rust gpui"),
                KeyValueRow::enabled("locale", "中文"),
            ]
        );
        assert_eq!(vm.url_query_parameter_count(), 3);
        assert_eq!(vm.enabled_param_count(), 3);

        vm.upsert_param("limit", "20");
        assert_eq!(
            vm.url(),
            "https://httpbingo.org/get?existing=1&q=rust+gpui&locale=%E4%B8%AD%E6%96%87&limit=20"
        );

        vm.toggle_param(0);
        assert_eq!(
            vm.effective_url(),
            "https://httpbingo.org/get?q=rust+gpui&locale=%E4%B8%AD%E6%96%87&limit=20"
        );
    }

    #[test]
    fn repeated_add_keeps_multiple_blank_param_rows_independent() {
        let mut vm = RequestViewModel::new();
        vm.set_url("https://httpbingo.org/get");

        assert_eq!(vm.visible_param_row_count(), 1);

        const ADD_CLICKS: usize = 32;
        for _ in 0..ADD_CLICKS {
            let previous_count = vm.visible_param_row_count();
            vm.append_param_row();
            assert_eq!(vm.visible_param_row_count(), previous_count + 1);
        }
        assert_eq!(vm.params().len(), ADD_CLICKS);
        assert!(vm.params().iter().all(|row| row.key.is_empty()));

        vm.set_param_key(0, "q");
        vm.set_param_value(0, "rust gpui");
        vm.set_param_key(1, "locale");
        vm.set_param_value(1, "中文");
        vm.set_param_key(2, "limit");
        vm.set_param_value(2, "20");
        vm.toggle_param(2);

        assert_eq!(
            vm.effective_url(),
            "https://httpbingo.org/get?q=rust+gpui&locale=%E4%B8%AD%E6%96%87"
        );
        assert_eq!(vm.enabled_param_count(), 2);
        assert_eq!(vm.params()[0].key, "q");
        assert_eq!(vm.params()[1].key, "locale");
        assert_eq!(vm.params()[2].key, "limit");
        assert!(!vm.params()[2].enabled);

        vm.remove_param(0);
        assert_eq!(vm.params()[0].key, "locale");
        assert_eq!(vm.params()[1].key, "limit");
        assert_eq!(
            vm.effective_url(),
            "https://httpbingo.org/get?locale=%E4%B8%AD%E6%96%87"
        );
    }

    #[test]
    fn repeated_add_keeps_multiple_blank_header_rows_independent() {
        let mut vm = RequestViewModel::new();
        vm.set_url("https://httpbingo.org/headers");

        assert_eq!(vm.visible_header_row_count(), 1);

        const ADD_CLICKS: usize = 32;
        for _ in 0..ADD_CLICKS {
            let previous_count = vm.visible_header_row_count();
            vm.append_header_row();
            assert_eq!(vm.visible_header_row_count(), previous_count + 1);
        }
        assert_eq!(vm.headers().len(), ADD_CLICKS);
        assert!(vm
            .headers()
            .iter()
            .all(|row| row.key.is_empty() && row.value.is_empty()));

        vm.set_header_key(0, "X-Scenario");
        vm.set_header_value(0, "multiple-header-rows");
        vm.set_header_key(1, "X-Locale");
        vm.set_header_value(1, "zh-CN");
        vm.set_header_key(2, "X-Disabled");
        vm.set_header_value(2, "must-not-be-sent");
        vm.toggle_header(2);

        let request = vm.build_request();
        assert_eq!(
            request.headers,
            vec![
                ("X-Scenario".to_string(), "multiple-header-rows".to_string()),
                ("X-Locale".to_string(), "zh-CN".to_string()),
            ]
        );
        assert_eq!(vm.enabled_header_count(), 2);
        assert!(!vm.headers()[2].enabled);

        vm.remove_header(0);
        assert_eq!(vm.headers()[0].key, "X-Locale");
        assert_eq!(vm.headers()[1].key, "X-Disabled");
    }

    #[test]
    fn active_and_saved_header_rows_with_the_same_name_remain_independent() {
        let mut vm = RequestViewModel::new();
        vm.set_url("https://httpbingo.org/headers");
        vm.append_header_row();
        vm.set_header_key(0, "X-Repeated");
        vm.set_header_value(0, "saved");
        vm.set_row_draft_key(RequestPane::Headers, "X-Repeated");
        vm.set_row_draft_value(RequestPane::Headers, "active");

        assert_eq!(
            vm.build_request().headers,
            vec![
                ("X-Repeated".to_string(), "saved".to_string()),
                ("X-Repeated".to_string(), "active".to_string()),
            ]
        );
    }

    #[test]
    fn query_edits_rewrite_the_url_before_its_fragment() {
        let mut vm = RequestViewModel::new();
        vm.set_url("https://example.com/get?existing=hello%20world#result");
        assert_eq!(
            vm.effective_url(),
            "https://example.com/get?existing=hello%20world#result"
        );
        vm.upsert_param("q", "rust gpui");
        vm.set_row_draft_key(RequestPane::Params, "locale");
        vm.set_row_draft_value(RequestPane::Params, "中文");

        assert_eq!(vm.url_query_parameter_count(), 3);
        assert_eq!(vm.enabled_param_count(), 3);
        assert_eq!(
            vm.effective_url(),
            "https://example.com/get?existing=hello+world&q=rust+gpui&locale=%E4%B8%AD%E6%96%87#result"
        );
        assert_eq!(
            vm.build_request().url,
            "https://example.com/get?existing=hello+world&q=rust+gpui&locale=%E4%B8%AD%E6%96%87#result"
        );
    }

    #[test]
    fn active_param_and_header_rows_participate_before_commit() {
        let mut vm = RequestViewModel::new();
        vm.set_url("https://example.com/live");
        vm.set_row_draft_key(RequestPane::Params, "source");
        vm.set_row_draft_value(RequestPane::Params, "typed");
        vm.set_row_draft_key(RequestPane::Headers, "X-Live-Input");
        vm.set_row_draft_value(RequestPane::Headers, "saved-before-add");

        assert_eq!(vm.url(), "https://example.com/live?source=typed");
        assert_eq!(vm.effective_url(), "https://example.com/live?source=typed");
        assert_eq!(vm.enabled_param_count(), 1);
        assert_eq!(vm.row_draft(RequestPane::Params), Some(("source", "typed")));
        let request = vm.build_request();
        assert!(request
            .headers
            .iter()
            .any(|(key, value)| { key == "X-Live-Input" && value == "saved-before-add" }));

        vm.commit_row_draft(RequestPane::Params);
        vm.commit_row_draft(RequestPane::Headers);
        assert_eq!(vm.params(), &[KeyValueRow::enabled("source", "typed")]);
        assert!(vm
            .headers()
            .iter()
            .any(|row| row.key == "X-Live-Input" && row.value == "saved-before-add"));
        assert_eq!(vm.row_draft(RequestPane::Params), Some(("", "")));
        assert_eq!(vm.row_draft(RequestPane::Headers), Some(("", "")));
    }

    #[test]
    fn send_builds_request_and_transitions_response() {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_method(HttpMethod::POST);
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://example.com/users");
        workspace
            .active_request_mut()
            .unwrap()
            .set_body(r#"{"name":"Ada"}"#);
        workspace
            .active_request_mut()
            .unwrap()
            .upsert_header("X-Trace", "abc");

        let pending = workspace.begin_send().unwrap();
        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::Loading
        ));
        assert_eq!(pending.request().method, HttpMethod::POST);
        assert_eq!(
            pending.request().body,
            RequestBody::Json(r#"{"name":"Ada"}"#.to_string())
        );
        assert!(pending
            .request()
            .headers
            .iter()
            .any(|(key, value)| key == "X-Trace" && value == "abc"));
        assert!(workspace.complete_send(
            pending,
            Ok(RequestResult {
                status: 201,
                headers: vec![("x-test".into(), "yes".into())],
                body: r#"{"ok":true}"#.into(),
                elapsed_ms: 7,
                stored_cookies: Vec::new(),
                redirect_chain: Vec::new(),
            })
        ));
        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::Success { status: 201, .. }
        ));
        assert!(!workspace.active_request().unwrap().is_dirty());
    }

    #[test]
    fn failed_send_does_not_enter_history() {
        let mut workspace = WorkspaceViewModel::new();
        let pending = workspace.begin_send().unwrap();
        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::Loading
        ));
        assert!(workspace.complete_send(pending, Err(AppError::UrlEmpty)));

        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::Error { .. }
        ));
        assert_eq!(workspace.history_len(), 0);
    }

    #[test]
    fn switching_post_body_to_form_data_replaces_json_content_type() {
        let mut vm = RequestViewModel::new();
        vm.set_method(HttpMethod::POST);
        assert!(vm
            .headers()
            .iter()
            .any(|row| { row.key == "Content-Type" && row.value == "application/json" }));

        vm.set_body("name=Ada&active=true");
        vm.set_body_kind(BodyKind::UrlEncoded);

        assert_eq!(
            vm.headers()
                .iter()
                .find(|row| row.key == "Content-Type")
                .map(|row| row.value.as_str()),
            Some("application/x-www-form-urlencoded")
        );
        assert!(vm
            .headers()
            .iter()
            .any(|row| row.key == "Accept" && row.value == "application/json"));
    }

    #[test]
    fn post_json_defaults_do_not_depend_on_custom_headers_being_absent() {
        let mut vm = RequestViewModel::new();
        vm.upsert_header("X-Scenario", "httpbingo-json");

        vm.set_method(HttpMethod::POST);
        vm.set_body_kind(BodyKind::Json);

        let request = vm.build_request();
        for (name, value) in [
            ("Content-Type", "application/json"),
            ("Accept", "application/json"),
            ("X-Scenario", "httpbingo-json"),
        ] {
            assert!(request.headers.iter().any(|(actual_name, actual_value)| {
                actual_name.eq_ignore_ascii_case(name) && actual_value == value
            }));
        }

        let effective = vm.effective_headers();
        assert_eq!(effective.len(), 3);
        for name in ["Content-Type", "Accept"] {
            assert_eq!(
                effective
                    .iter()
                    .find(|header| header.name.eq_ignore_ascii_case(name))
                    .map(|header| header.source),
                Some(EffectiveHeaderSource::Generated)
            );
        }
        assert_eq!(
            effective
                .iter()
                .find(|header| header.name.eq_ignore_ascii_case("X-Scenario"))
                .map(|header| header.source),
            Some(EffectiveHeaderSource::User)
        );
    }

    #[test]
    fn a_user_accept_header_is_preserved_instead_of_duplicated_by_post_defaults() {
        let mut vm = RequestViewModel::new();
        vm.upsert_header("Accept", "application/problem+json");

        vm.set_method(HttpMethod::POST);

        let accepts: Vec<_> = vm
            .effective_headers()
            .into_iter()
            .filter(|header| header.name.eq_ignore_ascii_case("accept"))
            .collect();
        assert_eq!(accepts.len(), 1);
        assert_eq!(accepts[0].value, "application/problem+json");
        assert_eq!(accepts[0].source, EffectiveHeaderSource::User);
    }

    #[test]
    fn leaving_post_removes_only_the_automatic_accept_header() {
        let mut vm = RequestViewModel::new();
        vm.set_method(HttpMethod::POST);

        vm.set_method(HttpMethod::PUT);

        assert!(!vm
            .effective_headers()
            .iter()
            .any(|header| header.name.eq_ignore_ascii_case("accept")));
    }

    #[test]
    fn put_with_default_json_kind_gets_an_automatic_content_type() {
        let mut vm = RequestViewModel::new();

        vm.set_method(HttpMethod::PUT);
        vm.set_body_kind(BodyKind::Json);

        assert!(vm.headers().iter().any(|row| {
            row.key.eq_ignore_ascii_case("content-type") && row.value == "application/json"
        }));
    }

    #[test]
    fn switching_to_raw_removes_only_an_automatic_content_type() {
        let mut vm = RequestViewModel::new();
        vm.set_method(HttpMethod::POST);

        vm.set_body_kind(BodyKind::Raw);

        assert!(!vm
            .headers()
            .iter()
            .any(|row| row.key.eq_ignore_ascii_case("content-type")));
        assert!(vm
            .headers()
            .iter()
            .any(|row| row.key.eq_ignore_ascii_case("accept")));
    }

    #[test]
    fn put_raw_builds_an_exact_typed_body_without_generated_headers() {
        let mut vm = RequestViewModel::new();
        vm.set_method(HttpMethod::PUT);
        vm.set_url("https://httpbingo.org/anything/raw");
        vm.set_body_kind(BodyKind::Raw);
        vm.set_body("plain text body");

        assert_eq!(vm.body(), "plain text body");
        assert_eq!(
            vm.request_body(),
            RequestBody::Raw("plain text body".to_string())
        );
        assert!(vm.effective_headers().is_empty());

        let request = vm.build_request();
        assert_eq!(request.method, HttpMethod::PUT);
        assert_eq!(request.url, "https://httpbingo.org/anything/raw");
        assert_eq!(
            request.body,
            RequestBody::Raw("plain text body".to_string())
        );
        assert!(request.headers.is_empty());
    }

    #[test]
    fn manual_content_type_is_case_insensitive_and_survives_body_kind_changes() {
        let mut vm = RequestViewModel::new();
        vm.set_method(HttpMethod::POST);

        vm.upsert_header("content-type", "application/vnd.example+json");
        vm.set_body_kind(BodyKind::UrlEncoded);
        vm.set_body_kind(BodyKind::Raw);

        let content_types: Vec<_> = vm
            .headers()
            .iter()
            .filter(|row| row.key.eq_ignore_ascii_case("content-type"))
            .collect();
        assert_eq!(content_types.len(), 1);
        assert_eq!(content_types[0].value, "application/vnd.example+json");
    }

    #[test]
    fn removing_an_automatic_content_type_is_a_user_override() {
        let mut vm = RequestViewModel::new();
        vm.set_method(HttpMethod::POST);
        vm.set_url("https://example.com/manual-content-type");
        let content_type_index = vm
            .headers()
            .iter()
            .position(|row| row.key.eq_ignore_ascii_case("content-type"))
            .expect("POST should add an automatic Content-Type");

        vm.remove_header(content_type_index);
        vm.set_body_kind(BodyKind::UrlEncoded);
        let request = vm.begin_send(SendId(1), Arc::new(AtomicBool::new(false)));

        assert!(!vm
            .headers()
            .iter()
            .any(|row| row.key.eq_ignore_ascii_case("content-type")));
        assert!(!request
            .headers
            .iter()
            .any(|(key, _)| key.eq_ignore_ascii_case("content-type")));
    }

    #[test]
    fn bearer_auth_is_normalized_and_sent_as_authorization_header() {
        let mut vm = RequestViewModel::new();
        vm.set_url("https://example.com/me");
        vm.set_bearer_token("Bearer secret-token");

        assert_eq!(vm.bearer_token(), "Bearer secret-token");
        assert_eq!(vm.normalized_bearer_token(), "secret-token");
        assert_eq!(
            vm.authorization_header_preview(),
            Some("Authorization: Bearer secret-token".to_string())
        );

        let request = vm.begin_send(SendId(1), Arc::new(AtomicBool::new(false)));

        assert_eq!(vm.bearer_token(), "secret-token");
        assert!(request
            .headers
            .iter()
            .any(|(key, value)| key.eq_ignore_ascii_case("authorization")
                && value == "Bearer secret-token"));
    }

    #[test]
    fn bearer_normalization_removes_one_case_insensitive_scheme_and_extra_spacing() {
        let mut vm = RequestViewModel::new();

        vm.set_bearer_token("  BEARER    scenario-token  ");
        assert_eq!(vm.bearer_token(), "  BEARER    scenario-token  ");
        assert_eq!(vm.normalized_bearer_token(), "scenario-token");
        assert_eq!(
            vm.authorization_header_preview(),
            Some("Authorization: Bearer scenario-token".to_string())
        );

        vm.set_bearer_token("Bearer Bearer scenario-token");
        assert_eq!(vm.normalized_bearer_token(), "Bearer scenario-token");

        vm.set_bearer_token("Bearer");
        assert_eq!(vm.normalized_bearer_token(), "");
        assert_eq!(vm.authorization_header_preview(), None);
    }

    #[test]
    fn basic_auth_is_encoded_and_sent_as_authorization_header() {
        let mut vm = RequestViewModel::new();
        vm.set_url("https://example.com/basic-auth");
        vm.set_row_draft_key(RequestPane::Headers, "Authorization");
        vm.set_row_draft_value(RequestPane::Headers, "Basic stale-one");
        vm.commit_row_draft(RequestPane::Headers);
        vm.set_row_draft_key(RequestPane::Headers, "authorization");
        vm.set_row_draft_value(RequestPane::Headers, "Basic stale-two");
        vm.commit_row_draft(RequestPane::Headers);
        vm.set_authorization_kind(AuthorizationKind::Basic);
        vm.set_basic_username("scenario-user");
        vm.set_basic_password("scenario-pass");
        assert_eq!(
            vm.authorization_header_preview(),
            Some("Authorization: Basic c2NlbmFyaW8tdXNlcjpzY2VuYXJpby1wYXNz".to_string())
        );
        let request = vm.begin_send(SendId(1), Arc::new(AtomicBool::new(false)));

        let authorization_headers = request
            .headers
            .iter()
            .filter(|(key, _)| key.eq_ignore_ascii_case("authorization"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            authorization_headers,
            vec![(
                "Authorization".to_string(),
                "Basic c2NlbmFyaW8tdXNlcjpzY2VuYXJpby1wYXNz".to_string()
            )]
        );
    }

    #[test]
    fn loading_basic_auth_projects_credentials_back_into_the_editor_state() {
        let mut request = Request::new(HttpMethod::GET, "https://example.com/basic-auth");
        request.add_header(
            "Authorization",
            "Basic c2NlbmFyaW8tdXNlcjpzY2VuYXJpby1wYXNz",
        );
        request.add_header("X-Trace", "kept");
        let mut vm = RequestViewModel::new();

        vm.load_request(&request);

        assert_eq!(vm.authorization_kind(), AuthorizationKind::Basic);
        assert_eq!(vm.basic_username(), "scenario-user");
        assert_eq!(vm.basic_password(), "scenario-pass");
        assert_eq!(vm.headers(), &[KeyValueRow::enabled("X-Trace", "kept")]);
    }

    #[test]
    fn tabs_preserve_independent_request_and_editor_state() {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://first.example");
        workspace
            .active_request_mut()
            .unwrap()
            .set_body(r#"{"tab":1}"#);
        workspace
            .active_request_mut()
            .unwrap()
            .set_bearer_token("first-token");
        workspace
            .active_request_mut()
            .unwrap()
            .set_pre_request_script("const first = true;");
        workspace
            .active_request_mut()
            .unwrap()
            .set_tests_script("status == 200");
        workspace
            .active_request_mut()
            .unwrap()
            .set_row_draft_key(RequestPane::Headers, "X-First-Draft");
        workspace
            .active_request_mut()
            .unwrap()
            .set_row_draft_value(RequestPane::Headers, "one");

        workspace.new_request();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://second.example");
        workspace
            .active_request_mut()
            .unwrap()
            .set_body(r#"{"tab":2}"#);
        workspace
            .active_request_mut()
            .unwrap()
            .set_bearer_token("second-token");
        workspace
            .active_request_mut()
            .unwrap()
            .set_row_draft_key(RequestPane::Headers, "X-Second-Draft");
        workspace
            .active_request_mut()
            .unwrap()
            .set_row_draft_value(RequestPane::Headers, "two");

        assert_eq!(workspace.tab_count(), 2);
        assert_eq!(workspace.active_tab_index().unwrap(), 1);
        assert!(workspace.select_tab(0));
        assert_eq!(
            workspace.active_request().unwrap().url(),
            "https://first.example"
        );
        assert_eq!(workspace.active_request().unwrap().body(), r#"{"tab":1}"#);
        assert_eq!(
            workspace.active_request().unwrap().bearer_token(),
            "first-token"
        );
        assert_eq!(
            workspace.active_request().unwrap().pre_request_script(),
            "const first = true;"
        );
        assert_eq!(
            workspace.active_request().unwrap().tests_script(),
            "status == 200"
        );
        assert_eq!(
            workspace
                .active_request()
                .unwrap()
                .row_draft(RequestPane::Headers),
            Some(("X-First-Draft", "one"))
        );

        assert!(workspace.select_tab(1));
        assert_eq!(
            workspace.active_request().unwrap().url(),
            "https://second.example"
        );
        assert_eq!(
            workspace.active_request().unwrap().bearer_token(),
            "second-token"
        );
        assert_eq!(
            workspace
                .active_request()
                .unwrap()
                .row_draft(RequestPane::Headers),
            Some(("X-Second-Draft", "two"))
        );
    }

    #[test]
    fn tabs_preserve_complete_url_encoded_body_drafts() {
        let rows = vec![
            KeyValueRow::enabled("tag", "rust"),
            KeyValueRow {
                enabled: false,
                key: "ignored".to_string(),
                value: "draft-only".to_string(),
            },
            KeyValueRow::enabled("", "blank-key-draft"),
            KeyValueRow::enabled("tag", "gpui"),
        ];
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_body_kind(BodyKind::UrlEncoded);
        workspace
            .active_request_mut()
            .unwrap()
            .set_url_encoded_rows(rows.clone());

        workspace.new_request();
        assert!(workspace.select_tab(0));

        assert_eq!(
            workspace.active_request().unwrap().body_draft(),
            &RequestBodyDraft::UrlEncoded(rows)
        );
        assert_eq!(
            workspace.active_request().unwrap().request_body(),
            RequestBody::UrlEncoded("tag=rust&tag=gpui".to_string())
        );

        workspace
            .active_request_mut()
            .unwrap()
            .set_method(HttpMethod::POST);
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://example.test/form");
        let pending = workspace.begin_send().unwrap();
        assert_eq!(
            pending.request().body,
            RequestBody::UrlEncoded("tag=rust&tag=gpui".to_string())
        );
    }

    #[test]
    fn tabs_preserve_complete_multipart_body_drafts() {
        let fixture_path = std::path::PathBuf::from("tests/fixtures/httpbingo-upload.txt");
        let parts = vec![
            MultipartDraftPart::text("note", "hello", true),
            MultipartDraftPart::text("ignored", "draft-only", false),
            MultipartDraftPart::text("", "blank-key-draft", true),
            MultipartDraftPart::file(
                "upload",
                fixture_path.clone(),
                Some("renamed.txt".to_string()),
                Some("text/plain".to_string()),
                true,
            ),
            MultipartDraftPart::file("pending", std::path::PathBuf::new(), None, None, true),
        ];
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_body_kind(BodyKind::Multipart);
        workspace
            .active_request_mut()
            .unwrap()
            .set_multipart_draft_parts(parts.clone());

        workspace.new_request();
        assert!(workspace.select_tab(0));

        assert_eq!(
            workspace.active_request().unwrap().body_draft(),
            &RequestBodyDraft::Multipart(parts)
        );
        let expected_body = RequestBody::Multipart(vec![
            MultipartPart::text("note", "hello"),
            MultipartPart {
                name: "upload".to_string(),
                value: MultipartValue::File {
                    path: fixture_path,
                    file_name: Some("renamed.txt".to_string()),
                    content_type: Some("text/plain".to_string()),
                },
            },
        ]);
        assert_eq!(
            workspace.active_request().unwrap().request_body(),
            expected_body
        );

        workspace
            .active_request_mut()
            .unwrap()
            .set_method(HttpMethod::POST);
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://example.test/upload");
        let pending = workspace.begin_send().unwrap();
        assert_eq!(pending.request().body, expected_body);
    }

    #[test]
    fn multipart_history_keeps_disabled_editor_intent_separate_from_the_sent_request() {
        let selected = std::path::PathBuf::from("tests/fixtures/httpbingo-upload.txt");
        let disabled_missing = std::path::PathBuf::from("tests/fixtures/missing-upload.txt");
        let parts = vec![
            MultipartDraftPart::text("enabled_note", "sent", true),
            MultipartDraftPart::text("disabled_note", "omit-me", false),
            MultipartDraftPart::file(
                "disabled_upload",
                disabled_missing.clone(),
                Some("missing-upload.txt".to_string()),
                Some("text/plain".to_string()),
                false,
            ),
            MultipartDraftPart::file(
                "enabled_upload",
                selected.clone(),
                Some("httpbingo-upload.txt".to_string()),
                Some("text/plain".to_string()),
                true,
            ),
        ];
        let expected_request_body = RequestBody::Multipart(vec![
            MultipartPart::text("enabled_note", "sent"),
            MultipartPart {
                name: "enabled_upload".to_string(),
                value: MultipartValue::File {
                    path: selected.clone(),
                    file_name: Some("httpbingo-upload.txt".to_string()),
                    content_type: Some("text/plain".to_string()),
                },
            },
        ]);
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_method(HttpMethod::POST);
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://example.test/post");
        workspace
            .active_request_mut()
            .unwrap()
            .set_body_kind(BodyKind::Multipart);
        workspace
            .active_request_mut()
            .unwrap()
            .set_multipart_draft_parts(parts.clone());

        let pending = workspace.begin_send().unwrap();
        assert_eq!(pending.request().body, expected_request_body);
        assert!(complete_and_confirm_history(
            &mut workspace,
            pending,
            Ok(RequestResult::success("ok".to_string()))
        ));

        let entry = workspace.history()[0].clone();
        assert_eq!(entry.request.body, expected_request_body);
        assert_eq!(
            entry.editor_intent,
            Some(RequestEditorIntent::Multipart(vec![
                MultipartEditorPart {
                    enabled: true,
                    name: "enabled_note".to_string(),
                    value: MultipartValue::Text("sent".to_string()),
                },
                MultipartEditorPart {
                    enabled: false,
                    name: "disabled_note".to_string(),
                    value: MultipartValue::Text("omit-me".to_string()),
                },
                MultipartEditorPart {
                    enabled: false,
                    name: "disabled_upload".to_string(),
                    value: MultipartValue::File {
                        path: disabled_missing,
                        file_name: Some("missing-upload.txt".to_string()),
                        content_type: Some("text/plain".to_string()),
                    },
                },
                MultipartEditorPart {
                    enabled: true,
                    name: "enabled_upload".to_string(),
                    value: MultipartValue::File {
                        path: selected,
                        file_name: Some("httpbingo-upload.txt".to_string()),
                        content_type: Some("text/plain".to_string()),
                    },
                },
            ]))
        );

        workspace.new_request();
        workspace.load_history_entry(&entry);
        assert_eq!(
            workspace.active_request().unwrap().body_draft(),
            &RequestBodyDraft::Multipart(parts)
        );
        assert_eq!(
            workspace.active_request().unwrap().request_body(),
            expected_request_body
        );
    }

    #[test]
    fn closing_tabs_keeps_a_valid_active_request() {
        let mut workspace = WorkspaceViewModel::new();
        workspace.active_request_mut().unwrap().set_url("one");
        workspace.new_request();
        workspace.active_request_mut().unwrap().set_url("two");
        workspace.new_request();
        workspace.active_request_mut().unwrap().set_url("three");

        assert!(workspace.close_tab(1));
        assert_eq!(workspace.tab_count(), 2);
        assert_eq!(workspace.active_request().unwrap().url(), "three");

        assert!(workspace.close_tab(1));
        assert_eq!(workspace.tab_count(), 1);
        assert_eq!(workspace.active_request().unwrap().url(), "one");

        assert!(workspace.close_tab(0));
        assert_eq!(workspace.tab_count(), 1);
        assert_eq!(workspace.active_request().unwrap().url(), "");
    }

    #[test]
    fn explicit_active_request_apis_report_absence_without_mutating_a_tab() {
        let mut workspace = WorkspaceViewModel::new();
        let tab_id = workspace
            .active_tab_id()
            .expect("a new workspace starts with one active request");
        workspace
            .update_request_for_tab(tab_id, |request| request.set_url("https://kept.example"))
            .expect("the stable tab id must resolve");

        workspace.active_tab_id = None;

        assert_eq!(workspace.active_tab_id(), None);
        assert_eq!(workspace.active_tab_index(), None);
        assert!(workspace.active_request().is_none());
        assert!(workspace.active_request_mut().is_none());
        assert!(workspace.begin_send().is_none());
        assert!(workspace
            .update_active_request(|request| request.set_url("https://wrong.example"))
            .is_none());
        assert!(
            !workspace.load_request(&Request::new(HttpMethod::GET, "https://also-wrong.example",))
        );
        assert_eq!(
            workspace
                .request_for_tab(tab_id)
                .expect("losing the active selection must not remove the tab")
                .url(),
            "https://kept.example"
        );

        assert!(workspace
            .update_request_for_tab(tab_id, |request| request
                .set_url("https://explicit.example"))
            .is_some());
        assert_eq!(
            workspace
                .request_for_tab(tab_id)
                .expect("the explicit stable id still resolves")
                .url(),
            "https://explicit.example"
        );
        assert!(workspace.request_for_tab(RequestTabId(u64::MAX)).is_none());
        assert!(workspace
            .update_request_for_tab(RequestTabId(u64::MAX), RequestViewModel::clear_body)
            .is_none());
    }

    #[test]
    fn workspace_collects_completed_requests_in_shared_history() {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://example.com/shared-history");
        let pending = workspace.begin_send().unwrap();
        assert!(complete_and_confirm_history(
            &mut workspace,
            pending,
            Ok(RequestResult {
                status: 204,
                headers: Vec::new(),
                body: String::new(),
                elapsed_ms: 2,
                stored_cookies: Vec::new(),
                redirect_chain: Vec::new(),
            })
        ));

        assert_eq!(workspace.history_len(), 1);
        assert_eq!(
            workspace.history()[0].request.url,
            "https://example.com/shared-history"
        );
        assert_eq!(workspace.history()[0].status, Some(204));
        assert_eq!(workspace.history()[0].elapsed_ms, Some(2));
        assert_eq!(workspace.history()[0].response_size, Some(0));
    }

    #[test]
    fn non_2xx_completion_remains_a_successful_response_and_enters_history() {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://httpbingo.org/status/418");
        let pending = workspace.begin_send().unwrap();

        assert!(complete_and_confirm_history(
            &mut workspace,
            pending,
            Ok(RequestResult {
                status: 418,
                headers: vec![("content-type".into(), "text/plain".into())],
                body: "I'm a teapot!".into(),
                elapsed_ms: 7,
                stored_cookies: Vec::new(),
                redirect_chain: Vec::new(),
            })
        ));

        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::Success {
                status: 418,
                body,
                ..
            } if body == "I'm a teapot!"
        ));
        assert_eq!(workspace.history_len(), 1);
        assert_eq!(workspace.history()[0].status, Some(418));
        assert_eq!(
            workspace.history()[0].request.url,
            "https://httpbingo.org/status/418"
        );
    }

    #[test]
    fn redirect_completion_uses_final_response_and_keeps_original_request_in_history() {
        let original_url =
            "https://httpbingo.org/redirect-to?url=%2Fanything%2Fredirected&status_code=302";
        let final_body = r#"{"method":"GET","url":"https://httpbingo.org/anything/redirected"}"#;
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url(original_url);
        let pending = workspace.begin_send().unwrap();

        assert!(complete_and_confirm_history(
            &mut workspace,
            pending,
            Ok(RequestResult {
                status: 200,
                headers: vec![("content-type".into(), "application/json".into())],
                body: final_body.into(),
                elapsed_ms: 11,
                stored_cookies: Vec::new(),
                redirect_chain: Vec::new(),
            })
        ));

        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::Success {
                status: 200,
                body,
                ..
            } if body == final_body
        ));
        assert_eq!(workspace.history_len(), 1);
        let entry = &workspace.history()[0];
        assert_eq!(entry.request.method, HttpMethod::GET);
        assert_eq!(entry.request.url, original_url);
        assert_eq!(entry.status, Some(200));
        assert_eq!(entry.elapsed_ms, Some(11));
        assert_eq!(entry.response_size, Some(final_body.len()));
    }

    #[test]
    fn json_completion_preserves_the_complete_body_and_request_lifecycle() {
        let url = "https://httpbingo.org/json";
        let body = r#"{"slideshow":{"author":"Yours Truly","date":"date of publication","slides":[{"title":"Wake up to WonderWidgets!","type":"all"}],"title":"Sample Slide Show"}}"#;
        let mut workspace = WorkspaceViewModel::new();
        workspace.active_request_mut().unwrap().set_url(url);
        let pending = workspace.begin_send().unwrap();

        assert!(complete_and_confirm_history(
            &mut workspace,
            pending,
            Ok(RequestResult {
                status: 200,
                headers: vec![("content-type".into(), "application/json".into())],
                body: body.into(),
                elapsed_ms: 13,
                stored_cookies: Vec::new(),
                redirect_chain: Vec::new(),
            })
        ));

        let ResponseState::Success {
            status,
            headers,
            body: actual_body,
            elapsed_ms,
        } = workspace.active_request().unwrap().response()
        else {
            panic!("JSON request should complete as an HTTP response");
        };
        assert_eq!(*status, 200);
        assert_eq!(*elapsed_ms, 13);
        assert_eq!(
            actual_body, body,
            "ResponseState must retain the full raw body"
        );
        assert_eq!(
            headers,
            &[("content-type".to_string(), "application/json".to_string())]
        );
        let parsed: serde_json::Value =
            serde_json::from_str(actual_body).expect("the completed body should be valid JSON");
        assert_eq!(parsed["slideshow"]["title"], "Sample Slide Show");

        assert_eq!(workspace.history_len(), 1);
        let entry = &workspace.history()[0];
        assert_eq!(entry.request.method, HttpMethod::GET);
        assert_eq!(entry.request.url, url);
        assert_eq!(entry.request.body, RequestBody::None);
        assert_eq!(entry.status, Some(200));
        assert_eq!(entry.elapsed_ms, Some(13));
        assert_eq!(entry.response_size, Some(body.len()));
    }

    #[test]
    fn cookie_jar_projection_is_application_scoped_non_sensitive_and_clearable() {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://httpbingo.org/cookies");
        let pending = workspace.begin_send().unwrap();
        assert!(complete_and_confirm_history(
            &mut workspace,
            pending,
            Ok(RequestResult {
                status: 200,
                headers: vec![("content-type".into(), "application/json".into())],
                body: r#"{"cookies":{"session":"cookie-e2e-demo"}}"#.into(),
                elapsed_ms: 8,
                stored_cookies: Vec::new(),
                redirect_chain: Vec::new(),
            })
        ));
        let response_before_clear = workspace.active_request().unwrap().response().clone();

        workspace.sync_cookie_jar(vec![
            ("https://httpbingo.org".into(), "session".into()),
            ("https://httpbingo.org".into(), "session".into()),
        ]);
        assert_eq!(
            workspace.cookies(),
            &[CookieJarEntry {
                origin: "https://httpbingo.org".into(),
                name: "session".into(),
            }]
        );
        assert_eq!(workspace.cookie_count(), 1);
        assert_eq!(workspace.last_cookie_clear_count(), None);

        workspace.new_request();
        assert_eq!(workspace.cookie_count(), 1, "the jar is shared across tabs");
        workspace.record_cookies_cleared(1);
        assert!(workspace.cookies().is_empty());
        assert_eq!(workspace.last_cookie_clear_count(), Some(1));
        assert_eq!(workspace.history_len(), 1);
        assert_eq!(&response_before_clear, &workspace.tabs()[0].response);
    }

    #[test]
    fn completion_targets_the_originating_tab_after_the_user_switches_tabs() {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://first.example/slow");
        let first = workspace.begin_send().unwrap();

        workspace.new_request();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://second.example/draft");
        assert!(complete_and_confirm_history(
            &mut workspace,
            first,
            Ok(RequestResult {
                status: 200,
                headers: Vec::new(),
                body: "first response".to_string(),
                elapsed_ms: 10,
                stored_cookies: Vec::new(),
                redirect_chain: Vec::new(),
            })
        ));

        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::NotSent
        ));
        assert!(workspace.select_tab(0));
        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::Success { body, .. } if body == "first response"
        ));
        assert_eq!(workspace.history_len(), 1);
    }

    #[test]
    fn delayed_completion_for_a_closed_tab_cannot_touch_the_active_request() {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://first.example/slow");
        let first_tab_id = workspace.active_tab_id().expect("first tab must be active");
        let pending = workspace
            .begin_send()
            .expect("the active request must produce a send command");

        workspace.new_request();
        workspace
            .active_request_mut()
            .expect("the new request must be active")
            .set_url("https://second.example/draft");
        let second_tab_id = workspace
            .active_tab_id()
            .expect("second tab must be active");
        assert!(workspace.close_tab_by_id(first_tab_id));

        assert!(
            !workspace.complete_send(pending, Ok(RequestResult::success("too late".to_string())))
        );
        assert!(workspace.request_for_tab(first_tab_id).is_none());
        assert_eq!(workspace.active_tab_id(), Some(second_tab_id));
        let active = workspace
            .active_request()
            .expect("closing another tab must preserve the active request");
        assert_eq!(active.url(), "https://second.example/draft");
        assert!(matches!(active.response(), ResponseState::NotSent));
        assert_eq!(workspace.history_len(), 0);
    }

    #[test]
    fn stale_completion_cannot_replace_a_newer_send() {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://example.com/race");
        let older = workspace.begin_send().unwrap();
        let newer = workspace.begin_send().unwrap();

        assert!(!complete_and_confirm_history(
            &mut workspace,
            older,
            Ok(RequestResult::success("stale".to_string()))
        ));
        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::Loading
        ));
        assert!(complete_and_confirm_history(
            &mut workspace,
            newer,
            Ok(RequestResult::success("current".to_string()))
        ));
        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::Success { body, .. } if body == "current"
        ));
        assert_eq!(workspace.history_len(), 2);
    }

    #[test]
    fn editing_while_a_request_is_in_flight_keeps_the_draft_dirty() {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://example.com/original");
        let pending = workspace.begin_send().unwrap();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://example.com/edited");

        assert!(complete_and_confirm_history(
            &mut workspace,
            pending,
            Ok(RequestResult::success("done".to_string()))
        ));

        assert_eq!(
            workspace.active_request().unwrap().url(),
            "https://example.com/edited"
        );
        assert!(workspace.active_request().unwrap().is_dirty());
        assert_eq!(
            workspace.history()[0].request.url,
            "https://example.com/original"
        );
    }

    #[test]
    fn cancelling_a_send_ignores_its_late_completion() {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://example.com/slow");
        let pending = workspace.begin_send().unwrap();

        assert_eq!(workspace.active_send_id(), Some(pending.send_id()));
        assert_eq!(workspace.active_request_id().as_deref(), Some("req-01"));
        assert_eq!(workspace.in_flight_count(), 1);
        assert!(workspace.cancel_send(pending.send_id()));
        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::Cancelled
        ));
        assert_eq!(workspace.active_request_id(), None);
        assert_eq!(workspace.in_flight_count(), 0);
        assert!(
            !workspace.complete_send(pending, Ok(RequestResult::success("too late".to_string())))
        );
        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::Cancelled
        ));
        assert_eq!(workspace.history_len(), 0);
    }

    #[test]
    fn request_timeout_is_captured_and_finishes_without_fabricating_history() {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://example.com/slow");
        workspace
            .active_request_mut()
            .unwrap()
            .set_timeout_ms(1_000);
        let pending = workspace.begin_send().unwrap();

        assert_eq!(pending.timeout_ms(), Some(1_000));
        assert_eq!(workspace.in_flight_count(), 1);
        assert!(workspace.complete_send(pending, Err(AppError::Timeout { timeout_ms: 1_000 })));
        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::Error { message } if message == "Request timed out after 1,000 ms"
        ));
        assert_eq!(workspace.active_request_id(), None);
        assert_eq!(workspace.in_flight_count(), 0);
        assert_eq!(workspace.history_len(), 0);
    }

    #[test]
    fn completed_history_captures_and_replays_request_timeout() {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://example.com/slow");
        workspace
            .active_request_mut()
            .unwrap()
            .set_timeout_ms(1_250);
        workspace
            .active_request_mut()
            .unwrap()
            .set_redirect_policy(RedirectPolicy::DoNotFollow);
        workspace
            .active_request_mut()
            .unwrap()
            .set_max_redirect_hops(7);
        let pending = workspace.begin_send().unwrap();

        assert!(complete_and_confirm_history(
            &mut workspace,
            pending,
            Ok(RequestResult::success("done".into()))
        ));
        let entry = workspace.history()[0].clone();
        assert_eq!(entry.request_options.timeout_ms, Some(1_250));
        assert_eq!(
            entry.request_options.redirect_policy,
            RedirectPolicy::DoNotFollow
        );
        assert_eq!(entry.request_options.max_redirect_hops, 7);

        workspace.new_request();
        assert_eq!(workspace.active_request().unwrap().timeout_ms(), 0);
        assert_eq!(
            workspace.active_request().unwrap().redirect_policy(),
            RedirectPolicy::Follow
        );
        assert_eq!(
            workspace.active_request().unwrap().max_redirect_hops(),
            DEFAULT_MAX_REDIRECT_HOPS
        );
        workspace.load_history_entry(&entry);
        assert_eq!(workspace.active_request().unwrap().timeout_ms(), 1_250);
        assert_eq!(
            workspace.active_request().unwrap().redirect_policy(),
            RedirectPolicy::DoNotFollow
        );
        assert_eq!(workspace.active_request().unwrap().max_redirect_hops(), 7);
    }

    #[test]
    fn redirect_policy_and_chain_are_captured_by_send_identity() {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://example.com/redirect/1");
        workspace
            .active_request_mut()
            .unwrap()
            .set_max_redirect_hops(3);
        let pending = workspace.begin_send().unwrap();
        assert_eq!(
            pending.request_options(),
            RequestOptions {
                timeout_ms: None,
                redirect_policy: RedirectPolicy::Follow,
                max_redirect_hops: 3,
            }
        );

        workspace
            .active_request_mut()
            .unwrap()
            .set_redirect_policy(RedirectPolicy::DoNotFollow);
        workspace
            .active_request_mut()
            .unwrap()
            .set_max_redirect_hops(9);
        let chain = vec![
            RedirectHop::new(302, "https://example.com/redirect/1", Some("/terminal")),
            RedirectHop::terminal(200, "https://example.com/terminal"),
        ];
        let mut result = RequestResult::success("done".into());
        result.redirect_chain = chain.clone();
        assert!(complete_and_confirm_history(
            &mut workspace,
            pending,
            Ok(result)
        ));

        assert_eq!(workspace.active_request().unwrap().redirect_chain(), chain);
        assert!(
            workspace.active_request().unwrap().is_dirty(),
            "edits made during Send must remain dirty"
        );
        assert_eq!(workspace.history()[0].request_options.max_redirect_hops, 3);
        assert_eq!(
            workspace.history()[0].request_options.redirect_policy,
            RedirectPolicy::Follow
        );
    }

    #[test]
    fn redirect_limit_failure_exposes_partial_chain_without_history() {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://example.com/redirect/3");
        workspace
            .active_request_mut()
            .unwrap()
            .set_max_redirect_hops(2);
        let pending = workspace.begin_send().unwrap();
        let chain = vec![
            RedirectHop::new(302, "https://example.com/redirect/3", Some("/redirect/2")),
            RedirectHop::new(302, "https://example.com/redirect/2", Some("/redirect/1")),
        ];

        assert!(workspace.complete_send(
            pending,
            Err(AppError::RedirectLimitExceeded {
                max_hops: 2,
                chain: chain.clone(),
            })
        ));
        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::Error { message }
                if message == "Redirect limit exceeded after 2 hops."
        ));
        assert_eq!(workspace.active_request().unwrap().redirect_chain(), chain);
        assert_eq!(workspace.history_len(), 0);

        workspace.new_request();
        assert!(workspace
            .active_request()
            .unwrap()
            .redirect_chain()
            .is_empty());
    }

    #[test]
    fn runtime_replay_overlay_requires_a_confirmed_row_and_does_not_survive_recovery() {
        let raw_url = "https://example.com/replay?tag=rust&api_key=runtime-secret";
        let mut raw_request = Request::new(HttpMethod::POST, raw_url);
        raw_request.add_header("X-Replay", "original");
        raw_request.add_header("Authorization", "Bearer runtime-token");
        raw_request.body = RequestBody::Json(r#"{"replay":true}"#.to_string());
        let candidate =
            HistoryEntry::completed(raw_request.clone(), "runtime replay".into(), 200, 4, 2)
                .with_historical_response(HistoricalResponse::completed(
                    200,
                    Vec::new(),
                    "ok".into(),
                    4,
                ));
        let confirmed = HistoryEntry::try_from(
            VersionedHistorySnapshot::try_from(&candidate).expect("candidate should serialize"),
        )
        .expect("snapshot should return to a History entry");
        assert_eq!(confirmed.id, candidate.id);
        assert_eq!(confirmed.request.url, "https://example.com/replay?tag=rust");
        assert!(confirmed
            .request
            .headers
            .iter()
            .all(|(name, _)| !name.eq_ignore_ascii_case("authorization")));

        let mut current_session = WorkspaceViewModel::new();
        current_session.confirm_runtime_replay_request(candidate.id.clone(), raw_request.clone());
        assert!(current_session.runtime_replay_requests.is_empty());
        current_session.replace_history_query_result(vec![confirmed.clone()], 0);
        current_session.confirm_runtime_replay_request(candidate.id.clone(), raw_request);
        current_session.load_history_entry(&confirmed);
        assert_eq!(current_session.active_request().unwrap().url(), raw_url);
        assert_eq!(
            current_session.active_request().unwrap().bearer_token(),
            "runtime-token"
        );

        // An explicit refresh retains overlays only for IDs still returned by SQLite.
        current_session.replace_history_query_result(vec![confirmed.clone()], 0);
        assert_eq!(current_session.runtime_replay_requests.len(), 1);
        current_session.replace_history_query_result(Vec::new(), 0);
        assert!(current_session.runtime_replay_requests.is_empty());

        let mut recovered_session = WorkspaceViewModel::new();
        recovered_session.replace_history_query_result(vec![confirmed.clone()], 0);
        recovered_session.load_history_entry(&confirmed);
        assert_eq!(
            recovered_session.active_request().unwrap().url(),
            "https://example.com/replay?tag=rust"
        );
        assert!(recovered_session
            .active_request()
            .unwrap()
            .bearer_token()
            .is_empty());
    }

    #[test]
    fn changing_timeout_during_send_keeps_the_request_draft_dirty() {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://example.com/slow");
        workspace
            .active_request_mut()
            .unwrap()
            .set_timeout_ms(1_000);
        let pending = workspace.begin_send().unwrap();
        workspace
            .active_request_mut()
            .unwrap()
            .set_timeout_ms(2_000);

        assert!(workspace.complete_send(pending, Ok(RequestResult::success("done".into()))));
        assert_eq!(workspace.active_request().unwrap().timeout_ms(), 2_000);
        assert!(workspace.active_request().unwrap().is_dirty());
    }

    #[test]
    fn selecting_history_replaces_response_evidence_without_sending_or_mutating_cookies() {
        let response_entry = |suffix: &str, body: &str| {
            let response = HistoricalResponse::completed(
                200,
                vec![("Content-Type".into(), "text/plain".into())],
                body.to_string(),
                9,
            );
            let mut entry = HistoryEntry::completed(
                Request::new(HttpMethod::GET, format!("https://example.com/{suffix}")),
                suffix.to_string(),
                200,
                9,
                body.len(),
            )
            .with_historical_response(response);
            entry.id = format!("00000000-0000-4000-8000-0000000000{suffix}");
            entry
        };
        let first = response_entry("41", "first historical body");
        let second = response_entry("42", "second historical body");
        let mut workspace = WorkspaceViewModel::new();
        workspace.replace_history_query_result(vec![first.clone(), second.clone()], 0);
        workspace.sync_cookie_jar(vec![("https://example.com".into(), "existing".into())]);

        workspace.load_history_entry(&first);
        assert_eq!(workspace.in_flight_count(), 0);
        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::Historical { entry_id, response }
                if entry_id == &first.id
                    && matches!(&response.body, crate::models::HistoricalResponseBody::Text(body) if body == "first historical body")
        ));
        assert_eq!(workspace.cookie_count(), 1);
        assert!(workspace
            .active_request()
            .unwrap()
            .response_stored_cookies()
            .is_empty());

        workspace.load_history_entry(&second);
        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::Historical { entry_id, response }
                if entry_id == &second.id
                    && matches!(&response.body, crate::models::HistoricalResponseBody::Text(body) if body == "second historical body")
        ));
        assert_eq!(workspace.history_len(), 2);
    }

    #[test]
    fn sending_from_historical_creates_an_independent_row_and_clear_removes_selection() {
        let response = HistoricalResponse::completed(202, Vec::new(), "stored".to_string(), 4);
        let original = HistoryEntry::completed(
            Request::new(HttpMethod::GET, "https://example.com/replay"),
            "replay".into(),
            202,
            4,
            6,
        )
        .with_historical_response(response);
        let original_id = original.id.clone();
        let mut workspace = WorkspaceViewModel::new();
        workspace.replace_history_query_result(vec![original.clone()], 0);
        workspace.load_history_entry(&original);

        let pending = workspace.begin_send().unwrap();
        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::Loading
        ));
        assert!(complete_and_confirm_history(
            &mut workspace,
            pending,
            Ok(RequestResult {
                status: 200,
                headers: Vec::new(),
                body: "new response".into(),
                elapsed_ms: 5,
                stored_cookies: Vec::new(),
                redirect_chain: Vec::new(),
            })
        ));
        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::Success { body, .. } if body == "new response"
        ));
        assert_eq!(workspace.history_len(), 2);
        assert_ne!(workspace.history()[0].id, original_id);
        assert_eq!(workspace.history()[1].id, original_id);

        workspace.load_history_entry(&original);
        workspace.replace_history_query_result(Vec::new(), 0);
        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::NotSent
        ));
        assert!(workspace.history().is_empty());
    }

    #[test]
    fn request_only_history_uses_the_explicit_unavailable_state() {
        let entry = HistoryEntry::completed(
            Request::new(HttpMethod::GET, "https://example.com/v1"),
            "legacy".into(),
            204,
            1,
            0,
        );
        let entry_id = entry.id.clone();
        let mut workspace = WorkspaceViewModel::new();

        workspace.load_history_entry(&entry);

        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::HistoricalUnavailable { entry_id: selected }
                if selected == &entry_id
        ));
    }
}

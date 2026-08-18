use crate::utils::log::display_url_for_log;
use crate::{
    errors::AppError,
    http::executor::RequestResult,
    models::{HistoryEntry, HttpMethod, MultipartPart, Request, RequestBody, RequestHistory},
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::{
    fmt,
    ops::{Deref, DerefMut},
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
}

/// Authentication scheme managed by the Authorization editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorizationKind {
    Bearer,
    Basic,
}

/// Body encoding selected in the editor. The payload and encoding are stored together in
/// `RequestBody`; this enum is only a compact value for rendering controls.
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
    Error {
        message: String,
    },
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

/// Monotonic identity for one send attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SendId(u64);

impl fmt::Display for SendId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Immutable command emitted by the ViewModel for the application service to execute.
#[derive(Clone, Debug)]
pub struct PendingRequest {
    tab_id: RequestTabId,
    send_id: SendId,
    request: Request,
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
    body: RequestBody,
    content_type_source: ContentTypeSource,
    authorization_kind: AuthorizationKind,
    bearer_token: String,
    basic_username: String,
    basic_password: String,
    pre_request_script: String,
    tests_script: String,
    request_pane: RequestPane,
    response: ResponseState,
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
            body: RequestBody::None,
            content_type_source: ContentTypeSource::Unset,
            authorization_kind: AuthorizationKind::Bearer,
            bearer_token: String::new(),
            basic_username: String::new(),
            basic_password: String::new(),
            pre_request_script: String::new(),
            tests_script: String::new(),
            request_pane: RequestPane::Params,
            response: ResponseState::NotSent,
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
            | RequestPane::Tests => return None,
        };
        Some((&draft.key, &draft.value))
    }

    pub fn body(&self) -> &str {
        self.body.as_text().unwrap_or_default()
    }

    pub fn request_body(&self) -> &RequestBody {
        &self.body
    }

    pub fn body_kind(&self) -> BodyKind {
        match self.body {
            RequestBody::None => BodyKind::None,
            RequestBody::Json(_) => BodyKind::Json,
            RequestBody::Raw(_) => BodyKind::Raw,
            RequestBody::UrlEncoded(_) => BodyKind::UrlEncoded,
            RequestBody::Multipart(_) => BodyKind::Multipart,
        }
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

    pub fn request_pane(&self) -> RequestPane {
        self.request_pane
    }

    pub fn response(&self) -> &ResponseState {
        &self.response
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

        if method == HttpMethod::POST && self.body.is_empty() {
            let add_default_accept = self.headers.is_empty();
            self.body = RequestBody::Json(default_json_body());
            self.sync_automatic_content_type();
            if add_default_accept {
                self.headers
                    .push(KeyValueRow::enabled("Accept", "application/json"));
            }
        } else {
            self.sync_automatic_content_type();
        }
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
        let next = match &self.body {
            RequestBody::None => RequestBody::Raw(body),
            RequestBody::Json(_) => RequestBody::Json(body),
            RequestBody::Raw(_) => RequestBody::Raw(body),
            RequestBody::UrlEncoded(_) => RequestBody::UrlEncoded(body),
            RequestBody::Multipart(_) => RequestBody::Multipart(parse_multipart_text_parts(&body)),
        };
        if self.body != next {
            self.body = next;
            self.dirty = true;
        }
    }

    /// Clears the payload without guessing or changing its selected encoding.
    pub fn clear_body(&mut self) {
        let next = match self.body {
            RequestBody::None => RequestBody::None,
            RequestBody::Json(_) => RequestBody::Json(String::new()),
            RequestBody::Raw(_) => RequestBody::Raw(String::new()),
            RequestBody::UrlEncoded(_) => RequestBody::UrlEncoded(String::new()),
            RequestBody::Multipart(_) => RequestBody::Multipart(Vec::new()),
        };
        if self.body != next {
            self.body = next;
            self.dirty = true;
        }
        self.sync_automatic_content_type();
    }

    pub fn set_body_kind(&mut self, body_kind: BodyKind) {
        if self.body_kind() != body_kind {
            let text = self.body.as_text().unwrap_or_default().to_string();
            self.body = match body_kind {
                BodyKind::None => RequestBody::None,
                BodyKind::Json => RequestBody::Json(text),
                BodyKind::Raw => RequestBody::Raw(text),
                BodyKind::UrlEncoded => RequestBody::UrlEncoded(text),
                BodyKind::Multipart => RequestBody::Multipart(parse_multipart_text_parts(&text)),
            };
            self.dirty = true;
        }
        self.sync_automatic_content_type();
    }

    pub fn set_multipart_parts(&mut self, parts: Vec<MultipartPart>) {
        let body = RequestBody::Multipart(parts);
        if self.body != body {
            self.body = body;
            self.dirty = true;
        }
        self.sync_automatic_content_type();
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
            | RequestPane::Tests => false,
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
            | RequestPane::Tests => false,
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
            | RequestPane::Tests => {}
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
            row.enabled = !row.enabled;
            self.dirty = true;
        }
    }

    pub fn remove_header(&mut self, index: usize) {
        if index < self.headers.len() {
            if self.headers[index].key.eq_ignore_ascii_case("content-type") {
                self.content_type_source = ContentTypeSource::User;
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
        self.body = RequestBody::None;
        self.content_type_source = ContentTypeSource::Unset;
        self.authorization_kind = AuthorizationKind::Bearer;
        self.bearer_token.clear();
        self.basic_username.clear();
        self.basic_password.clear();
        self.pre_request_script.clear();
        self.tests_script.clear();
        self.request_pane = RequestPane::Params;
        self.response = ResponseState::NotSent;
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
        self.body = request.body.clone();
        // A loaded request is an exact saved draft. Its Content-Type, including
        // an intentional absence, must not be replaced by automatic defaults.
        self.content_type_source = ContentTypeSource::User;
        self.request_pane = if self.body.is_empty() {
            RequestPane::Headers
        } else {
            RequestPane::Body
        };
        self.response = ResponseState::NotSent;
        self.dirty = false;
    }

    fn begin_send(&mut self, send_id: SendId, cancelled: Arc<AtomicBool>) -> Request {
        if self.authorization_kind == AuthorizationKind::Bearer {
            self.bearer_token = normalize_bearer_token(&self.bearer_token);
        }
        let request = self.build_request();
        self.pending_send_id = Some(send_id);
        self.pending_cancellation = Some(cancelled);
        self.response = ResponseState::Loading;
        request
    }

    fn complete_send(
        &mut self,
        pending: &PendingRequest,
        result: Result<RequestResult, AppError>,
    ) -> bool {
        if self.pending_send_id != Some(pending.send_id) {
            return false;
        }

        self.pending_send_id = None;
        self.pending_cancellation = None;
        match result {
            Ok(result) => {
                let draft_is_unchanged = self.build_request() == pending.request;
                self.response = ResponseState::Success {
                    status: result.status,
                    body: result.body,
                    headers: result.headers,
                    elapsed_ms: result.elapsed_ms,
                };
                if draft_is_unchanged {
                    self.dirty = false;
                }
            }
            Err(error) => {
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
            request.body = self.body.clone();
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

/// Application-level ViewModel. It owns independent request tabs and shared history.
pub struct WorkspaceViewModel {
    tabs: Vec<RequestViewModel>,
    active_tab: usize,
    history: RequestHistory,
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
            active_tab: 0,
            history: RequestHistory::new(),
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

    pub fn active_tab_index(&self) -> usize {
        self.active_tab
    }

    pub fn select_tab(&mut self, index: usize) -> bool {
        if index < self.tabs.len() && index != self.active_tab {
            self.active_tab = index;
            true
        } else {
            false
        }
    }

    pub fn new_request(&mut self) {
        let tab_id = RequestTabId(self.next_tab_id);
        self.next_tab_id += 1;
        self.tabs.push(RequestViewModel::for_tab(tab_id));
        self.active_tab = self.tabs.len() - 1;
    }

    pub fn close_tab(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() {
            return false;
        }

        if self.tabs.len() == 1 {
            self.tabs[0].new_request();
            self.active_tab = 0;
            return true;
        }

        self.tabs[index].mark_pending_cancelled();
        self.tabs.remove(index);
        if index < self.active_tab {
            self.active_tab -= 1;
        } else if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        true
    }

    pub fn begin_send(&mut self) -> PendingRequest {
        let send_id = SendId(self.next_send_id);
        self.next_send_id += 1;
        let cancelled = Arc::new(AtomicBool::new(false));
        let tab = &mut self.tabs[self.active_tab];
        let request = tab.begin_send(send_id, cancelled.clone());
        let pending = PendingRequest {
            tab_id: tab.tab_id,
            send_id,
            request,
            cancelled,
        };
        tracing::info!(
            send_id = %pending.send_id,
            tab_id = %pending.tab_id,
            method = %pending.request.method,
            url = %display_url_for_log(&pending.request.url),
            "request started"
        );
        pending
    }

    pub fn active_send_id(&self) -> Option<SendId> {
        self.tabs[self.active_tab].pending_send_id
    }

    pub fn send_id_for_tab(&self, index: usize) -> Option<SendId> {
        self.tabs.get(index).and_then(|tab| tab.pending_send_id)
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

    /// Applies a response only when both the tab and send attempt still exist. Successful stale
    /// completions still enter shared history because the HTTP exchange did occur.
    pub fn complete_send(
        &mut self,
        pending: PendingRequest,
        result: Result<RequestResult, AppError>,
    ) -> bool {
        let was_cancelled = pending.was_cancelled();
        let completed_response = result
            .as_ref()
            .ok()
            .map(|response| (response.status, response.elapsed_ms, response.body.len()));
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
        let applied = self
            .tabs
            .iter_mut()
            .find(|tab| tab.tab_id == pending.tab_id)
            .is_some_and(|tab| tab.complete_send(&pending, result));
        if let Some((status, elapsed_ms, response_size)) =
            completed_response.filter(|_| !was_cancelled)
        {
            self.history.add_completed(
                pending.request.clone(),
                history_label(&pending.request.url),
                status,
                elapsed_ms,
                response_size,
            );
        }
        applied
    }

    pub fn load_request(&mut self, request: &Request) {
        self.tabs[self.active_tab].load_request(request);
    }

    pub fn history(&self) -> &[HistoryEntry] {
        self.history.entries()
    }

    pub fn history_len(&self) -> usize {
        self.history.len()
    }
}

impl Default for WorkspaceViewModel {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for WorkspaceViewModel {
    type Target = RequestViewModel;

    fn deref(&self) -> &Self::Target {
        &self.tabs[self.active_tab]
    }
}

impl DerefMut for WorkspaceViewModel {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.tabs[self.active_tab]
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

fn parse_multipart_text_parts(body: &str) -> Vec<MultipartPart> {
    if body.is_empty() {
        return Vec::new();
    }

    form_urlencoded::parse(body.as_bytes())
        .map(|(name, value)| MultipartPart::text(name.into_owned(), value.into_owned()))
        .collect()
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
        workspace.set_method(HttpMethod::POST);
        workspace.set_url("https://example.com/users");
        workspace.set_body(r#"{"name":"Ada"}"#);
        workspace.upsert_header("X-Trace", "abc");

        let pending = workspace.begin_send();
        assert!(matches!(workspace.response(), ResponseState::Loading));
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
            })
        ));
        assert!(matches!(
            workspace.response(),
            ResponseState::Success { status: 201, .. }
        ));
        assert!(!workspace.is_dirty());
    }

    #[test]
    fn failed_send_does_not_enter_history() {
        let mut workspace = WorkspaceViewModel::new();
        let pending = workspace.begin_send();
        assert!(matches!(workspace.response(), ResponseState::Loading));
        assert!(workspace.complete_send(pending, Err(AppError::UrlEmpty)));

        assert!(matches!(workspace.response(), ResponseState::Error { .. }));
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
        workspace.set_url("https://first.example");
        workspace.set_body(r#"{"tab":1}"#);
        workspace.set_bearer_token("first-token");
        workspace.set_pre_request_script("const first = true;");
        workspace.set_tests_script("status == 200");
        workspace.set_row_draft_key(RequestPane::Headers, "X-First-Draft");
        workspace.set_row_draft_value(RequestPane::Headers, "one");

        workspace.new_request();
        workspace.set_url("https://second.example");
        workspace.set_body(r#"{"tab":2}"#);
        workspace.set_bearer_token("second-token");
        workspace.set_row_draft_key(RequestPane::Headers, "X-Second-Draft");
        workspace.set_row_draft_value(RequestPane::Headers, "two");

        assert_eq!(workspace.tab_count(), 2);
        assert_eq!(workspace.active_tab_index(), 1);
        assert!(workspace.select_tab(0));
        assert_eq!(workspace.url(), "https://first.example");
        assert_eq!(workspace.body(), r#"{"tab":1}"#);
        assert_eq!(workspace.bearer_token(), "first-token");
        assert_eq!(workspace.pre_request_script(), "const first = true;");
        assert_eq!(workspace.tests_script(), "status == 200");
        assert_eq!(
            workspace.row_draft(RequestPane::Headers),
            Some(("X-First-Draft", "one"))
        );

        assert!(workspace.select_tab(1));
        assert_eq!(workspace.url(), "https://second.example");
        assert_eq!(workspace.bearer_token(), "second-token");
        assert_eq!(
            workspace.row_draft(RequestPane::Headers),
            Some(("X-Second-Draft", "two"))
        );
    }

    #[test]
    fn closing_tabs_keeps_a_valid_active_request() {
        let mut workspace = WorkspaceViewModel::new();
        workspace.set_url("one");
        workspace.new_request();
        workspace.set_url("two");
        workspace.new_request();
        workspace.set_url("three");

        assert!(workspace.close_tab(1));
        assert_eq!(workspace.tab_count(), 2);
        assert_eq!(workspace.url(), "three");

        assert!(workspace.close_tab(1));
        assert_eq!(workspace.tab_count(), 1);
        assert_eq!(workspace.url(), "one");

        assert!(workspace.close_tab(0));
        assert_eq!(workspace.tab_count(), 1);
        assert_eq!(workspace.url(), "");
    }

    #[test]
    fn workspace_collects_completed_requests_in_shared_history() {
        let mut workspace = WorkspaceViewModel::new();
        workspace.set_url("https://example.com/shared-history");
        let pending = workspace.begin_send();
        assert!(workspace.complete_send(
            pending,
            Ok(RequestResult {
                status: 204,
                headers: Vec::new(),
                body: String::new(),
                elapsed_ms: 2,
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
    fn completion_targets_the_originating_tab_after_the_user_switches_tabs() {
        let mut workspace = WorkspaceViewModel::new();
        workspace.set_url("https://first.example/slow");
        let first = workspace.begin_send();

        workspace.new_request();
        workspace.set_url("https://second.example/draft");
        assert!(workspace.complete_send(
            first,
            Ok(RequestResult {
                status: 200,
                headers: Vec::new(),
                body: "first response".to_string(),
                elapsed_ms: 10,
            })
        ));

        assert!(matches!(workspace.response(), ResponseState::NotSent));
        assert!(workspace.select_tab(0));
        assert!(matches!(
            workspace.response(),
            ResponseState::Success { body, .. } if body == "first response"
        ));
        assert_eq!(workspace.history_len(), 1);
    }

    #[test]
    fn stale_completion_cannot_replace_a_newer_send() {
        let mut workspace = WorkspaceViewModel::new();
        workspace.set_url("https://example.com/race");
        let older = workspace.begin_send();
        let newer = workspace.begin_send();

        assert!(!workspace.complete_send(older, Ok(RequestResult::success("stale".to_string()))));
        assert!(matches!(workspace.response(), ResponseState::Loading));
        assert!(workspace.complete_send(newer, Ok(RequestResult::success("current".to_string()))));
        assert!(matches!(
            workspace.response(),
            ResponseState::Success { body, .. } if body == "current"
        ));
        assert_eq!(workspace.history_len(), 2);
    }

    #[test]
    fn editing_while_a_request_is_in_flight_keeps_the_draft_dirty() {
        let mut workspace = WorkspaceViewModel::new();
        workspace.set_url("https://example.com/original");
        let pending = workspace.begin_send();
        workspace.set_url("https://example.com/edited");

        assert!(workspace.complete_send(pending, Ok(RequestResult::success("done".to_string()))));

        assert_eq!(workspace.url(), "https://example.com/edited");
        assert!(workspace.is_dirty());
        assert_eq!(
            workspace.history()[0].request.url,
            "https://example.com/original"
        );
    }

    #[test]
    fn cancelling_a_send_ignores_its_late_completion() {
        let mut workspace = WorkspaceViewModel::new();
        workspace.set_url("https://example.com/slow");
        let pending = workspace.begin_send();

        assert_eq!(workspace.active_send_id(), Some(pending.send_id()));
        assert!(workspace.cancel_send(pending.send_id()));
        assert!(matches!(workspace.response(), ResponseState::Cancelled));
        assert!(
            !workspace.complete_send(pending, Ok(RequestResult::success("too late".to_string())))
        );
        assert!(matches!(workspace.response(), ResponseState::Cancelled));
        assert_eq!(workspace.history_len(), 0);
    }
}

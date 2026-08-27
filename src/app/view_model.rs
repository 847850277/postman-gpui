use super::request_lifecycle::{
    BeginSendTransition, RequestSendLifecycle, RequestTabValue, RequestTabs,
};
pub use super::request_lifecycle::{
    PendingRequest, RequestTabId, SendId, SendProgress, SendRejection, SendStart, SendTerminal,
    SendTerminalOutcome, SendTransition,
};
pub use crate::models::{
    AuthorizationKind, BodyKind, EffectiveHeader, EffectiveHeaderSource, KeyValueRow,
    MultipartDraftPart, MultipartDraftValue, RequestBodyDraft, RequestConstruction, RequestDraft,
    RequestDraftError,
};
use crate::utils::log::display_url_for_log;
use crate::{
    errors::AppError,
    http::executor::RequestResult,
    models::{
        HistoricalResponse, HistoryEntry, HttpMethod, MultipartPart, RedirectHop, RedirectPolicy,
        Request, RequestBody, RequestEditorIntent, RequestHistory,
    },
};
use std::{collections::HashMap, fmt};

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

/// UI and request-lifecycle state for one tab.
///
/// Editable request data and normalization live in [`RequestDraft`]. This adapter keeps tab,
/// response, Send, and dirty/saved behavior separate from the pure construction layer.
pub struct RequestViewModel {
    tab_id: RequestTabId,
    draft: RequestDraft,
    pre_request_script: String,
    tests_script: String,
    request_pane: RequestPane,
    response: ResponseState,
    redirect_chain: Vec<RedirectHop>,
    response_stored_cookies: Vec<CookieJarEntry>,
    send_lifecycle: RequestSendLifecycle,
    dirty: bool,
}

impl RequestViewModel {
    pub fn new() -> Self {
        Self::for_tab(RequestTabId(0))
    }

    fn for_tab(tab_id: RequestTabId) -> Self {
        Self {
            tab_id,
            draft: RequestDraft::new(),
            pre_request_script: String::new(),
            tests_script: String::new(),
            request_pane: RequestPane::Params,
            response: ResponseState::NotSent,
            redirect_chain: Vec::new(),
            response_stored_cookies: Vec::new(),
            send_lifecycle: RequestSendLifecycle::default(),
            dirty: false,
        }
    }

    pub fn tab_id(&self) -> RequestTabId {
        self.tab_id
    }

    pub fn request_draft(&self) -> &RequestDraft {
        &self.draft
    }

    /// The one normalized result shared by request previews and Send.
    pub fn request_construction(&self) -> RequestConstruction {
        self.draft.construct()
    }

    pub fn method(&self) -> HttpMethod {
        self.draft.method()
    }

    pub fn url(&self) -> &str {
        self.draft.url()
    }

    /// Returns the URL that will be sent. URL input and Params are kept synchronized, so request
    /// construction must not append the same query pairs a second time.
    pub fn effective_url(&self) -> String {
        self.request_construction().request().url.clone()
    }

    /// Counts query pairs currently represented in the synchronized URL input.
    pub fn url_query_parameter_count(&self) -> usize {
        self.draft.url_query_parameter_count()
    }

    /// Counts enabled Params rows, including a valid active draft that already participates in
    /// Send before the user presses Add or changes focus.
    pub fn enabled_param_count(&self) -> usize {
        self.draft.enabled_param_count()
    }

    pub fn params(&self) -> &[KeyValueRow] {
        self.draft.params()
    }

    /// Number of rows rendered by the Params editor. The active row is always visible, even
    /// before it has been confirmed with Add, so each Add action increases this count by one.
    pub fn visible_param_row_count(&self) -> usize {
        self.draft.visible_param_row_count()
    }

    pub fn headers(&self) -> &[KeyValueRow] {
        self.draft.headers()
    }

    /// Returns the enabled headers produced by the same request-construction path used by Send.
    /// This is a read-only View projection; it never becomes a second header store.
    pub fn effective_headers(&self) -> Vec<EffectiveHeader> {
        self.request_construction().effective_headers().to_vec()
    }

    /// Counts complete, enabled Header rows, including the active row that already participates
    /// in Send before the user presses Add or changes focus.
    pub fn enabled_header_count(&self) -> usize {
        self.draft.enabled_header_count()
    }

    /// Number of rows rendered by the Headers editor. As with Params, one active row is always
    /// visible, so every Add Header action increases this count by exactly one.
    pub fn visible_header_row_count(&self) -> usize {
        self.draft.visible_header_row_count()
    }

    /// Returns the in-progress row shown by the Params or Headers editor. The draft belongs to
    /// the request tab, not to the text controls, and participates in request construction as
    /// soon as it is valid.
    pub fn row_draft(&self, pane: RequestPane) -> Option<(&str, &str)> {
        match pane {
            RequestPane::Params => Some(self.draft.param_row_draft()),
            RequestPane::Headers => Some(self.draft.header_row_draft()),
            RequestPane::Authorization
            | RequestPane::Body
            | RequestPane::Scripts
            | RequestPane::Tests
            | RequestPane::Options => None,
        }
    }

    /// Returns the text projection used by previews and legacy scenario helpers. URL-encoded
    /// drafts are serialized from their effective rows; multipart drafts remain structured.
    pub fn body(&self) -> String {
        self.draft.body_text()
    }

    pub fn body_draft(&self) -> &RequestBodyDraft {
        self.draft.body_draft()
    }

    /// Derives the editor's effective body without applying HTTP-method gating.
    pub fn request_body(&self) -> RequestBody {
        self.draft.effective_body()
    }

    pub fn body_kind(&self) -> BodyKind {
        self.draft.body_kind()
    }

    pub fn bearer_token(&self) -> &str {
        self.draft.bearer_token()
    }

    /// Returns the canonical token that will participate in request construction without
    /// mutating the live editor value. The UI uses this projection to explain the same
    /// transformation that Send applies.
    pub fn normalized_bearer_token(&self) -> String {
        self.draft.normalized_bearer_token()
    }

    /// Returns the complete managed header exactly as it will be sent. Keeping this projection
    /// next to request construction prevents the UI preview from becoming a second auth policy.
    pub fn authorization_header_preview(&self) -> Option<String> {
        self.draft.authorization_header_preview()
    }

    pub fn authorization_kind(&self) -> AuthorizationKind {
        self.draft.authorization_kind()
    }

    pub fn basic_username(&self) -> &str {
        self.draft.basic_username()
    }

    pub fn basic_password(&self) -> &str {
        self.draft.basic_password()
    }

    pub fn pre_request_script(&self) -> &str {
        &self.pre_request_script
    }

    pub fn tests_script(&self) -> &str {
        &self.tests_script
    }

    /// Request-level timeout in milliseconds. Zero explicitly disables the deadline.
    pub fn timeout_ms(&self) -> u64 {
        self.draft.timeout_ms()
    }

    pub fn redirect_policy(&self) -> RedirectPolicy {
        self.draft.redirect_policy()
    }

    pub fn max_redirect_hops(&self) -> u32 {
        self.draft.max_redirect_hops()
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
        self.send_lifecycle.active_send_id().is_some()
    }

    pub fn send_progress(&self) -> Option<SendProgress> {
        self.send_lifecycle.progress_state()
    }

    pub fn last_send_terminal(&self) -> Option<SendTerminal> {
        self.send_lifecycle.last_terminal()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn edit_draft(&mut self, edit: impl FnOnce(&mut RequestDraft) -> bool) {
        if edit(&mut self.draft) {
            self.dirty = true;
        }
    }

    pub fn set_method(&mut self, method: HttpMethod) {
        self.edit_draft(|draft| draft.set_method(method));
    }

    pub fn set_url(&mut self, url: impl Into<String>) {
        self.edit_draft(|draft| draft.set_url(url));
    }

    pub fn set_body(&mut self, body: impl Into<String>) {
        self.edit_draft(|draft| draft.set_body(body));
    }

    /// Clears the payload without guessing or changing its selected encoding.
    pub fn clear_body(&mut self) {
        self.edit_draft(RequestDraft::clear_body);
    }

    pub fn set_body_kind(&mut self, body_kind: BodyKind) {
        self.edit_draft(|draft| draft.set_body_kind(body_kind));
    }

    pub fn set_url_encoded_rows(&mut self, rows: Vec<KeyValueRow>) {
        self.edit_draft(|draft| draft.set_url_encoded_rows(rows));
    }

    pub fn set_multipart_draft_parts(&mut self, parts: Vec<MultipartDraftPart>) {
        self.edit_draft(|draft| draft.set_multipart_draft_parts(parts));
    }

    /// Loads an already-effective multipart body as enabled editor rows.
    pub fn set_multipart_parts(&mut self, parts: Vec<MultipartPart>) {
        self.edit_draft(|draft| draft.set_multipart_parts(parts));
    }

    pub fn set_bearer_token(&mut self, token: impl Into<String>) {
        // Keep the editor text verbatim while the user is typing. Normalizing on every
        // keystroke would project `Bearer ` back as `Bearer` and swallow its space.
        self.edit_draft(|draft| draft.set_bearer_token(token));
    }

    pub fn set_authorization_kind(&mut self, kind: AuthorizationKind) {
        self.edit_draft(|draft| draft.set_authorization_kind(kind));
    }

    pub fn set_basic_username(&mut self, username: impl Into<String>) {
        self.edit_draft(|draft| draft.set_basic_username(username));
    }

    pub fn set_basic_password(&mut self, password: impl Into<String>) {
        self.edit_draft(|draft| draft.set_basic_password(password));
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
        self.edit_draft(|draft| draft.set_timeout_ms(timeout_ms));
    }

    pub fn set_redirect_policy(&mut self, redirect_policy: RedirectPolicy) {
        self.edit_draft(|draft| draft.set_redirect_policy(redirect_policy));
    }

    pub fn set_max_redirect_hops(&mut self, max_redirect_hops: u32) {
        self.edit_draft(|draft| draft.set_max_redirect_hops(max_redirect_hops));
    }

    pub fn set_request_pane(&mut self, pane: RequestPane) {
        self.request_pane = pane;
    }

    pub fn set_row_draft_key(&mut self, pane: RequestPane, key: impl Into<String>) {
        let changed = match pane {
            RequestPane::Params => self.draft.set_param_draft_key(key),
            RequestPane::Headers => self.draft.set_header_draft_key(key),
            RequestPane::Authorization
            | RequestPane::Body
            | RequestPane::Scripts
            | RequestPane::Tests
            | RequestPane::Options => false,
        };
        if changed {
            self.dirty = true;
        }
    }

    pub fn set_row_draft_value(&mut self, pane: RequestPane, value: impl Into<String>) {
        let changed = match pane {
            RequestPane::Params => self.draft.set_param_draft_value(value),
            RequestPane::Headers => self.draft.set_header_draft_value(value),
            RequestPane::Authorization
            | RequestPane::Body
            | RequestPane::Scripts
            | RequestPane::Tests
            | RequestPane::Options => false,
        };
        if changed {
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
        self.edit_draft(RequestDraft::append_param_row);
    }

    /// Preserves the current Header row and appends one fresh Header name/value row.
    ///
    /// Empty rows are intentional and unlimited. Header controls are editing buffers only; once a
    /// preserved row is edited, every keystroke is written directly to that indexed ViewModel row.
    pub fn append_header_row(&mut self) {
        self.edit_draft(RequestDraft::append_header_row);
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
        self.edit_draft(|draft| draft.upsert_param(key, value));
    }

    /// Updates one persistent Params row. Text controls are only editing buffers; every keystroke
    /// is written here so Send never needs to scrape or manually commit the rendered controls.
    pub fn set_param_key(&mut self, index: usize, key: impl Into<String>) {
        self.edit_draft(|draft| draft.set_param_key(index, key));
    }

    pub fn set_param_value(&mut self, index: usize, value: impl Into<String>) {
        self.edit_draft(|draft| draft.set_param_value(index, value));
    }

    pub fn toggle_param(&mut self, index: usize) {
        self.edit_draft(|draft| draft.toggle_param(index));
    }

    pub fn remove_param(&mut self, index: usize) {
        self.edit_draft(|draft| draft.remove_param(index));
    }

    pub fn upsert_header(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.edit_draft(|draft| draft.upsert_header(key, value));
    }

    /// Updates one persistent Header row. Duplicate names remain independent because Header rows
    /// model ordered request fields rather than a key-addressed map.
    pub fn set_header_key(&mut self, index: usize, key: impl Into<String>) {
        self.edit_draft(|draft| draft.set_header_key(index, key));
    }

    pub fn set_header_value(&mut self, index: usize, value: impl Into<String>) {
        self.edit_draft(|draft| draft.set_header_value(index, value));
    }

    pub fn clear_header_draft(&mut self) {
        self.edit_draft(RequestDraft::clear_header_draft);
    }

    pub fn toggle_header(&mut self, index: usize) {
        self.edit_draft(|draft| draft.toggle_header(index));
    }

    pub fn remove_header(&mut self, index: usize) {
        self.edit_draft(|draft| draft.remove_header(index));
    }

    pub fn new_request(&mut self) {
        self.reset_send_lifecycle();
        self.draft = RequestDraft::new();
        self.request_pane = RequestPane::Params;
        self.response = ResponseState::NotSent;
        self.redirect_chain.clear();
        self.response_stored_cookies.clear();
        self.dirty = false;
    }

    pub fn load_request(&mut self, request: &Request) {
        self.reset_send_lifecycle();
        self.draft = RequestDraft::from_request(request);
        self.pre_request_script.clear();
        self.tests_script.clear();
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
        self.draft.set_request_options(entry.request_options);
        if let Some(intent) = &entry.editor_intent {
            self.draft.restore_editor_intent(intent);
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

    fn begin_send(&mut self, send_id: SendId) -> (RequestConstruction, BeginSendTransition) {
        self.draft.normalize_for_send();
        let construction = self.draft.construct();
        let transition = self.send_lifecycle.begin(send_id);
        self.project_send_started();
        (construction, transition)
    }

    fn retry_send(
        &mut self,
        send_id: SendId,
    ) -> Result<(RequestConstruction, BeginSendTransition), SendRejection> {
        let transition = self.send_lifecycle.retry(send_id)?;
        self.draft.normalize_for_send();
        let construction = self.draft.construct();
        self.project_send_started();
        Ok((construction, transition))
    }

    fn project_send_started(&mut self) {
        self.response = ResponseState::Loading;
        self.redirect_chain.clear();
        self.response_stored_cookies.clear();
    }

    fn complete_send(
        &mut self,
        pending: &PendingRequest,
        result: Result<RequestResult, AppError>,
        stored_cookies: Vec<CookieJarEntry>,
    ) -> SendTransition {
        let outcome = match &result {
            Ok(_) => SendTerminalOutcome::Completed,
            Err(AppError::Timeout { .. }) => SendTerminalOutcome::TimedOut,
            Err(_) => SendTerminalOutcome::Failed,
        };
        let transition = self.send_lifecycle.complete(pending.send_id(), outcome);
        if !transition.is_applied() {
            return transition;
        }
        match result {
            Ok(result) => {
                let construction = self.draft.construct();
                let draft_is_unchanged = construction.request() == pending.request()
                    && construction.request_options() == pending.request_options();
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
        transition
    }

    fn record_send_progress(&mut self, send_id: SendId, progress: SendProgress) -> SendTransition {
        self.send_lifecycle.progress(send_id, progress)
    }

    fn cancel_send(&mut self, send_id: SendId) -> SendTransition {
        let transition = self.send_lifecycle.cancel(send_id);
        if transition.is_applied() {
            self.response = ResponseState::Cancelled;
            self.redirect_chain.clear();
        }
        transition
    }

    fn abandon_pending_send(&mut self) -> Option<SendId> {
        self.send_lifecycle.abandon()
    }

    fn reset_send_lifecycle(&mut self) -> Option<SendId> {
        self.send_lifecycle.reset()
    }

    pub fn tab_title(&self) -> String {
        if self.url().trim().is_empty() {
            return "Untitled request".to_string();
        }
        let without_scheme = self
            .url()
            .split_once("://")
            .map(|(_, value)| value)
            .unwrap_or(self.url());
        let title: String = without_scheme.chars().take(28).collect();
        if without_scheme.chars().count() > 28 {
            format!("{title}…")
        } else {
            title
        }
    }

    #[cfg(test)]
    fn build_request(&self) -> Request {
        self.request_construction().request().clone()
    }
}

impl RequestTabValue for RequestViewModel {
    fn tab_id(&self) -> RequestTabId {
        self.tab_id
    }

    fn assign_tab_id(&mut self, tab_id: RequestTabId) {
        self.tab_id = tab_id;
    }

    fn reset_for_replacement(&mut self) -> Option<SendId> {
        let abandoned_send_id = self.send_lifecycle.active_send_id();
        self.new_request();
        abandoned_send_id
    }

    fn prepare_for_close(&mut self) -> Option<SendId> {
        self.abandon_pending_send()
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

/// Result of routing one HTTP completion to request/response state. Only an accepted completion
/// yields a History candidate; the ViewModel never adds it to the visible database query result.
#[derive(Debug)]
pub struct SendCompletion {
    transition: SendTransition,
    history_entry: Option<HistoryEntry>,
}

impl SendCompletion {
    pub fn response_applied(&self) -> bool {
        self.transition.is_applied()
    }

    pub fn transition(&self) -> SendTransition {
        self.transition
    }

    pub fn history_entry(&self) -> Option<&HistoryEntry> {
        self.history_entry.as_ref()
    }

    pub fn into_parts(self) -> (bool, Option<HistoryEntry>) {
        (self.transition.is_applied(), self.history_entry)
    }
}

/// Application-level ViewModel. It owns request tabs and the latest SQLite History query result.
pub struct WorkspaceViewModel {
    tabs: RequestTabs<RequestViewModel>,
    history: RequestHistory,
    /// Current-process complete Requests keyed by SQLite-confirmed History IDs. This is not a
    /// second History store: it has no ordering or metadata, is never rendered independently,
    /// and is pruned whenever the authoritative SQLite query result changes. It only preserves
    /// credentials stripped at the persistence boundary for same-session replay.
    runtime_replay_requests: HashMap<String, Request>,
    history_storage_status: HistoryStorageStatus,
    cookie_jar: Vec<CookieJarEntry>,
    last_cookie_clear_count: Option<usize>,
    next_send_id: u64,
}

impl WorkspaceViewModel {
    pub fn new() -> Self {
        Self::with_request(RequestViewModel::new())
    }

    pub fn with_request(request: RequestViewModel) -> Self {
        Self {
            tabs: RequestTabs::with_initial(request),
            history: RequestHistory::new(),
            runtime_replay_requests: HashMap::new(),
            history_storage_status: HistoryStorageStatus::Loading {
                stage: HistoryStorageStage::Initialize,
            },
            cookie_jar: Vec::new(),
            last_cookie_clear_count: None,
            next_send_id: 1,
        }
    }

    pub fn tabs(&self) -> &[RequestViewModel] {
        self.tabs.values()
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
        self.tabs.active_tab_id()
    }

    pub fn active_tab_index(&self) -> Option<usize> {
        self.active_tab_id()
            .and_then(|tab_id| self.tab_index(tab_id))
    }

    pub fn tab_index(&self, tab_id: RequestTabId) -> Option<usize> {
        self.tabs.index_of(tab_id)
    }

    /// The active request, if the selected stable identity still belongs to this workspace.
    pub fn active_request(&self) -> Option<&RequestViewModel> {
        self.tabs.active()
    }

    /// Mutable access to the active request. Callers must choose this API explicitly instead of
    /// obtaining a mutable request through workspace deref coercion.
    pub fn active_request_mut(&mut self) -> Option<&mut RequestViewModel> {
        self.tabs.active_mut()
    }

    pub fn request_for_tab(&self, tab_id: RequestTabId) -> Option<&RequestViewModel> {
        self.tabs.get(tab_id)
    }

    pub fn request_for_tab_mut(&mut self, tab_id: RequestTabId) -> Option<&mut RequestViewModel> {
        self.tabs.get_mut(tab_id)
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
        self.tabs.select_index(index)
    }

    pub fn select_tab_by_id(&mut self, tab_id: RequestTabId) -> bool {
        self.tabs.select_id(tab_id)
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
            .values()
            .iter()
            .filter_map(|request| {
                let display_name = request.tab_title();
                matches(&display_name, request.method(), request.url()).then(|| {
                    GlobalSearchRequestResult {
                        tab_id: request.tab_id,
                        display_name,
                        method: request.method(),
                        url: request.url().to_string(),
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
        self.tabs.push(RequestViewModel::new());
    }

    pub fn close_tab(&mut self, index: usize) -> bool {
        self.tabs.close(index).changed()
    }

    pub fn close_tab_by_id(&mut self, tab_id: RequestTabId) -> bool {
        self.tab_index(tab_id)
            .is_some_and(|index| self.close_tab(index))
    }

    pub fn begin_send(&mut self) -> Option<PendingRequest> {
        let tab_id = self.active_tab_id()?;
        let send_id = SendId(self.next_send_id);
        self.next_send_id += 1;
        let tab = self.request_for_tab_mut(tab_id)?;
        let (construction, transition) = tab.begin_send(send_id);
        Some(Self::pending_from_send_start(
            tab_id,
            send_id,
            construction,
            transition,
        ))
    }

    /// Starts an explicit retry for the latest terminal attempt on `tab_id`.
    ///
    /// This only creates a lifecycle command. Transport execution and retry policy remain owned
    /// by the application service that receives the returned command.
    pub fn retry_send_for_tab(
        &mut self,
        tab_id: RequestTabId,
    ) -> Result<PendingRequest, SendRejection> {
        let send_id = SendId(self.next_send_id);
        self.next_send_id += 1;
        let tab = self
            .request_for_tab_mut(tab_id)
            .ok_or(SendRejection::TabNotFound { tab_id, send_id })?;
        let (construction, transition) = tab.retry_send(send_id)?;
        Ok(Self::pending_from_send_start(
            tab_id,
            send_id,
            construction,
            transition,
        ))
    }

    fn pending_from_send_start(
        tab_id: RequestTabId,
        send_id: SendId,
        construction: RequestConstruction,
        transition: BeginSendTransition,
    ) -> PendingRequest {
        let (request, _, editor_intent, request_options) = construction.into_parts();
        let pending = PendingRequest::new(
            tab_id,
            send_id,
            transition.start,
            request,
            editor_intent,
            request_options,
            transition.cancellation,
        );
        if let Some(superseded) = transition.superseded {
            tracing::debug!(
                send_id = %send_id,
                superseded_send_id = %superseded,
                tab_id = %tab_id,
                "request send superseded an earlier attempt"
            );
        }
        tracing::info!(
            send_id = %pending.send_id(),
            tab_id = %pending.tab_id(),
            method = %pending.request().method,
            url = %display_url_for_log(&pending.request().url),
            "request started"
        );
        pending
    }

    pub fn active_send_id(&self) -> Option<SendId> {
        self.active_request()
            .and_then(|tab| tab.send_lifecycle.active_send_id())
    }

    pub fn active_request_id(&self) -> Option<String> {
        self.active_send_id().map(SendId::request_id)
    }

    /// Number of request tabs whose active send has not reached a terminal state.
    pub fn in_flight_count(&self) -> usize {
        self.tabs
            .values()
            .iter()
            .filter(|tab| tab.is_sending())
            .count()
    }

    pub fn send_id_for_tab(&self, index: usize) -> Option<SendId> {
        self.tabs
            .get_at(index)
            .and_then(|tab| tab.send_lifecycle.active_send_id())
    }

    pub fn send_id_for_tab_id(&self, tab_id: RequestTabId) -> Option<SendId> {
        self.tab_index(tab_id)
            .and_then(|index| self.send_id_for_tab(index))
    }

    pub fn cancel_send(&mut self, send_id: SendId) -> bool {
        let transition = self
            .tabs
            .values_mut()
            .iter_mut()
            .find(|tab| tab.send_lifecycle.active_send_id() == Some(send_id))
            .map_or(
                SendTransition::Rejected(SendRejection::NoActiveSend { send_id }),
                |tab| tab.cancel_send(send_id),
            );
        if transition.is_applied() {
            tracing::info!(send_id = %send_id, "request cancelled");
        }
        transition.is_applied()
    }

    /// Routes progress by both stable identities. Stale events never fall back to the active tab.
    pub fn record_send_progress(
        &mut self,
        tab_id: RequestTabId,
        send_id: SendId,
        progress: SendProgress,
    ) -> SendTransition {
        self.request_for_tab_mut(tab_id).map_or(
            SendTransition::Rejected(SendRejection::TabNotFound { tab_id, send_id }),
            |tab| tab.record_send_progress(send_id, progress),
        )
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
        let successful_log = result
            .as_ref()
            .ok()
            .map(|response| (response.status, response.elapsed_ms));
        let failure_log = result.as_ref().err().map(ToString::to_string);
        let stored_cookies = stored_cookies
            .into_iter()
            .map(|(origin, name)| CookieJarEntry { origin, name })
            .collect();
        let transition = self.request_for_tab_mut(pending.tab_id()).map_or(
            SendTransition::Rejected(SendRejection::TabNotFound {
                tab_id: pending.tab_id(),
                send_id: pending.send_id(),
            }),
            |tab| tab.complete_send(&pending, result, stored_cookies),
        );
        match (transition.is_applied(), successful_log, failure_log) {
            (true, Some((status, elapsed_ms)), _) => {
                tracing::info!(
                    send_id = %pending.send_id(),
                    tab_id = %pending.tab_id(),
                    status,
                    elapsed_ms,
                    "request completed"
                );
            }
            (true, _, Some(error)) => {
                tracing::warn!(
                    send_id = %pending.send_id(),
                    tab_id = %pending.tab_id(),
                    error,
                    "request failed"
                );
            }
            _ => {
                tracing::debug!(
                    send_id = %pending.send_id(),
                    tab_id = %pending.tab_id(),
                    cancelled = was_cancelled,
                    rejection = ?transition.rejection(),
                    "ignored stale request completion"
                );
            }
        }
        let history_entry = completed_response
            .filter(|_| transition.is_applied() && !was_cancelled)
            .map(|response| {
                HistoryEntry::completed_with_intent_and_options(
                    pending.request().clone(),
                    history_label(&pending.request().url),
                    response.status,
                    response.elapsed_ms,
                    response.original_size,
                    pending.editor_intent().cloned(),
                    pending.request_options(),
                )
                .with_historical_response(response)
            });
        SendCompletion {
            transition,
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
            .and_then(|tab| tab.request_draft().editor_intent())
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
        for tab in self.tabs.values_mut() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::{MultipartEditorPart, MultipartValue, RequestOptions, DEFAULT_MAX_REDIRECT_HOPS},
        persistence::VersionedHistorySnapshot,
    };

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
        let (request, _) = vm.begin_send(SendId(1));

        assert!(!vm
            .headers()
            .iter()
            .any(|row| row.key.eq_ignore_ascii_case("content-type")));
        assert!(!request
            .request()
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

        let (request, _) = vm.begin_send(SendId(1));

        assert_eq!(vm.bearer_token(), "secret-token");
        assert!(request
            .request()
            .headers
            .iter()
            .any(|(key, value)| key.eq_ignore_ascii_case("authorization")
                && value == "Bearer secret-token"));
    }

    #[test]
    fn preview_and_send_consume_the_same_normalized_construction() {
        let mut vm = RequestViewModel::new();
        vm.set_url("https://example.com/preview-send");
        vm.set_method(HttpMethod::POST);
        vm.set_body(r#"{"same":true}"#);
        vm.upsert_header("X-Trace", "kept");
        vm.set_bearer_token("  BEARER   shared-token  ");
        vm.set_timeout_ms(2_500);

        let preview = vm.request_construction();
        let (sent, _) = vm.begin_send(SendId(1));

        assert_eq!(sent, preview);
        assert_eq!(vm.bearer_token(), "shared-token");
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
        let (request, _) = vm.begin_send(SendId(1));

        let authorization_headers = request
            .request()
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

        workspace.tabs.clear_active_selection();

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
    fn replacing_the_only_tab_rejects_its_in_flight_completion() {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://first.example/slow");
        let tab_id = workspace.active_tab_id().unwrap();
        let pending = workspace.begin_send().unwrap();

        assert!(workspace.close_tab(0));
        assert_eq!(workspace.active_tab_id(), Some(tab_id));
        assert_eq!(workspace.tab_count(), 1);
        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::NotSent
        ));

        let completion = workspace.complete_send_for_persistence(
            pending,
            Ok(RequestResult::success("too late".to_string())),
        );
        assert_eq!(
            completion.transition(),
            SendTransition::Rejected(SendRejection::NoActiveSend { send_id: SendId(1) })
        );
        assert!(completion.history_entry().is_none());
        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::NotSent
        ));
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
        assert_eq!(
            workspace.history_len(),
            1,
            "the rejected older completion must not emit a History candidate"
        );
    }

    #[test]
    fn duplicate_completion_emits_neither_a_second_response_nor_history_candidate() {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://example.com/duplicate");
        let pending = workspace.begin_send().unwrap();
        let duplicate = pending.clone();

        let first = workspace.complete_send_for_persistence(
            pending,
            Ok(RequestResult::success("accepted".to_string())),
        );
        assert_eq!(first.transition(), SendTransition::Applied);
        assert!(first.history_entry().is_some());

        let duplicate = workspace.complete_send_for_persistence(
            duplicate,
            Ok(RequestResult::success("duplicate".to_string())),
        );
        assert_eq!(
            duplicate.transition(),
            SendTransition::Rejected(SendRejection::DuplicateTerminal {
                send_id: SendId(1),
                outcome: SendTerminalOutcome::Completed,
            })
        );
        assert!(duplicate.history_entry().is_none());
        assert!(matches!(
            workspace.active_request().unwrap().response(),
            ResponseState::Success { body, .. } if body == "accepted"
        ));
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
        assert_eq!(
            workspace
                .active_request()
                .unwrap()
                .last_send_terminal()
                .unwrap()
                .outcome(),
            SendTerminalOutcome::Cancelled
        );
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
        assert_eq!(
            workspace
                .active_request()
                .unwrap()
                .last_send_terminal()
                .unwrap()
                .outcome(),
            SendTerminalOutcome::TimedOut
        );
        assert_eq!(workspace.history_len(), 0);
    }

    #[test]
    fn retry_carries_its_predecessor_and_rejects_stale_progress() {
        let mut workspace = WorkspaceViewModel::new();
        workspace
            .active_request_mut()
            .unwrap()
            .set_url("https://example.com/retry");
        let tab_id = workspace.active_tab_id().unwrap();
        let first = workspace.begin_send().unwrap();
        let first_send_id = first.send_id();
        assert!(workspace.complete_send(first, Err(AppError::Timeout { timeout_ms: 50 })));

        let retry = workspace.retry_send_for_tab(tab_id).unwrap();
        assert_eq!(
            retry.start(),
            SendStart::Retry {
                previous_send_id: first_send_id,
            }
        );
        assert_eq!(retry.retry_of(), Some(first_send_id));
        assert_eq!(
            workspace.record_send_progress(
                tab_id,
                first_send_id,
                SendProgress::Downloading { bytes_received: 12 },
            ),
            SendTransition::Rejected(SendRejection::StaleSend {
                send_id: first_send_id,
                active_send_id: retry.send_id(),
            })
        );
        assert_eq!(
            workspace.active_request().unwrap().send_progress(),
            Some(SendProgress::Started)
        );
        assert_eq!(
            workspace.record_send_progress(
                tab_id,
                retry.send_id(),
                SendProgress::WaitingForResponse,
            ),
            SendTransition::Applied
        );
        assert_eq!(
            workspace.active_request().unwrap().send_progress(),
            Some(SendProgress::WaitingForResponse)
        );
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

use crate::{
    errors::AppError,
    http::executor::{RequestExecutor, RequestResult},
    models::{HistoryEntry, HttpMethod, Request, RequestHistory},
};
use std::ops::{Deref, DerefMut};

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

/// Body encoding is presentation state, but it also affects the outgoing request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    Json,
    FormData,
    Raw,
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

/// State consumed by the response view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResponseState {
    NotSent,
    Loading,
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

/// Infrastructure boundary used by the ViewModel. Tests can replace it without a UI.
pub trait RequestService {
    fn execute(&self, request: &Request) -> Result<RequestResult, AppError>;
}

impl RequestService for RequestExecutor {
    fn execute(&self, request: &Request) -> Result<RequestResult, AppError> {
        self.execute_request(request)
    }
}

/// Source of truth for one request draft.
///
/// GPUI entities live in the View. This type intentionally has no GPUI dependency, so
/// request construction and response transitions can be tested in isolation.
/// Completed sends are recorded on `WorkspaceViewModel`, not here.
pub struct RequestViewModel {
    method: HttpMethod,
    url: String,
    params: Vec<KeyValueRow>,
    headers: Vec<KeyValueRow>,
    body: String,
    body_kind: BodyKind,
    bearer_token: String,
    pre_request_script: String,
    tests_script: String,
    request_pane: RequestPane,
    response: ResponseState,
    dirty: bool,
    service: Box<dyn RequestService>,
}

impl RequestViewModel {
    pub fn new() -> Self {
        Self::with_service(Box::new(RequestExecutor::new()))
    }

    pub fn with_service(service: Box<dyn RequestService>) -> Self {
        Self {
            method: HttpMethod::GET,
            url: String::new(),
            params: Vec::new(),
            headers: Vec::new(),
            body: String::new(),
            body_kind: BodyKind::Json,
            bearer_token: String::new(),
            pre_request_script: String::new(),
            tests_script: String::new(),
            request_pane: RequestPane::Params,
            response: ResponseState::NotSent,
            dirty: false,
            service,
        }
    }

    pub fn method(&self) -> HttpMethod {
        self.method
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn params(&self) -> &[KeyValueRow] {
        &self.params
    }

    pub fn headers(&self) -> &[KeyValueRow] {
        &self.headers
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn body_kind(&self) -> BodyKind {
        self.body_kind
    }

    pub fn bearer_token(&self) -> &str {
        &self.bearer_token
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

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn set_method(&mut self, method: HttpMethod) {
        if self.method == method {
            return;
        }
        self.method = method;
        self.dirty = true;

        if method == HttpMethod::POST && self.body.trim().is_empty() {
            self.body = default_json_body();
            self.body_kind = BodyKind::Json;
            if self.headers.is_empty() {
                self.headers = vec![
                    KeyValueRow::enabled("Content-Type", "application/json"),
                    KeyValueRow::enabled("Accept", "application/json"),
                ];
            }
        }
    }

    pub fn set_url(&mut self, url: impl Into<String>) {
        let url = url.into();
        if self.url == url {
            return;
        }
        self.params = parse_query_params(&url);
        self.url = url;
        self.dirty = true;
    }

    pub fn set_body(&mut self, body: impl Into<String>) {
        let body = body.into();
        if self.body != body {
            self.body = body;
            self.dirty = true;
        }
    }

    pub fn set_body_kind(&mut self, body_kind: BodyKind) {
        if self.body_kind != body_kind {
            self.body_kind = body_kind;
            self.dirty = true;
        }
    }

    pub fn set_bearer_token(&mut self, token: impl Into<String>) {
        let token = normalize_bearer_token(&token.into());
        if self.bearer_token != token {
            self.bearer_token = token;
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
        if let Some(row) = self.headers.iter_mut().find(|row| row.key == key) {
            row.value = value;
            row.enabled = true;
        } else {
            self.headers.push(KeyValueRow::enabled(key, value));
        }
        self.dirty = true;
    }

    pub fn toggle_header(&mut self, index: usize) {
        if let Some(row) = self.headers.get_mut(index) {
            row.enabled = !row.enabled;
            self.dirty = true;
        }
    }

    pub fn remove_header(&mut self, index: usize) {
        if index < self.headers.len() {
            self.headers.remove(index);
            self.dirty = true;
        }
    }

    pub fn new_request(&mut self) {
        self.method = HttpMethod::GET;
        self.url.clear();
        self.params.clear();
        self.headers.clear();
        self.body.clear();
        self.body_kind = BodyKind::Json;
        self.bearer_token.clear();
        self.pre_request_script.clear();
        self.tests_script.clear();
        self.request_pane = RequestPane::Params;
        self.response = ResponseState::NotSent;
        self.dirty = false;
    }

    pub fn load_request(&mut self, request: &Request) {
        self.method = request.method;
        self.url = request.url.clone();
        self.params = parse_query_params(&request.url);
        self.bearer_token = request
            .headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("authorization"))
            .map(|(_, value)| normalize_bearer_token(value))
            .unwrap_or_default();
        self.headers = request
            .headers
            .iter()
            .filter(|(key, _)| !key.eq_ignore_ascii_case("authorization"))
            .map(|(key, value)| KeyValueRow::enabled(key, value))
            .collect();
        self.body = request.body.clone().unwrap_or_default();
        self.body_kind = detect_body_kind(&self.body);
        self.request_pane = if self.body.is_empty() {
            RequestPane::Headers
        } else {
            RequestPane::Body
        };
        self.response = ResponseState::NotSent;
        self.dirty = false;
    }

    pub fn send(&mut self) {
        let _ = self.send_and_capture_request();
    }

    fn send_and_capture_request(&mut self) -> Option<Request> {
        self.response = ResponseState::Loading;
        let request = self.build_request();

        match self.service.execute(&request) {
            Ok(result) => {
                self.response = ResponseState::Success {
                    status: result.status,
                    body: result.body,
                    headers: result.headers,
                    elapsed_ms: result.elapsed_ms,
                };
                self.dirty = false;
                Some(request)
            }
            Err(error) => {
                self.response = ResponseState::Error {
                    message: error.to_string(),
                };
                None
            }
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
        let mut request = Request::new(self.method, &self.url);
        request.headers = self
            .headers
            .iter()
            .filter(|row| row.enabled && !row.key.trim().is_empty())
            .map(|row| (row.key.clone(), row.value.clone()))
            .collect();

        if !self.bearer_token.is_empty() {
            let value = format!("Bearer {}", self.bearer_token);
            if let Some((_, existing_value)) = request
                .headers
                .iter_mut()
                .find(|(key, _)| key.eq_ignore_ascii_case("authorization"))
            {
                *existing_value = value;
            } else {
                request.add_header("Authorization", value);
            }
        }

        if self.method.allows_body() {
            if self.body_kind == BodyKind::FormData
                && !request
                    .headers
                    .iter()
                    .any(|(key, _)| key.eq_ignore_ascii_case("content-type"))
            {
                request.add_header("Content-Type", "application/x-www-form-urlencoded");
            }
            request.body = Some(self.body.clone());
        }
        request
    }

    fn sync_url_from_params(&mut self) {
        self.url = apply_query_params(&self.url, &self.params);
    }
}

/// Application-level ViewModel. It owns independent request tabs and shared history.
pub struct WorkspaceViewModel {
    tabs: Vec<RequestViewModel>,
    active_tab: usize,
    history: RequestHistory,
}

impl WorkspaceViewModel {
    pub fn new() -> Self {
        Self::with_request(RequestViewModel::new())
    }

    pub fn with_request(request: RequestViewModel) -> Self {
        Self {
            tabs: vec![request],
            active_tab: 0,
            history: RequestHistory::new(),
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
        self.tabs.push(RequestViewModel::new());
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

        self.tabs.remove(index);
        if index < self.active_tab {
            self.active_tab -= 1;
        } else if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        true
    }

    pub fn send(&mut self) {
        if let Some(request) = self.tabs[self.active_tab].send_and_capture_request() {
            self.history
                .add(request.clone(), history_label(&request.url));
        }
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

pub fn detect_body_kind(body: &str) -> BodyKind {
    let trimmed = body.trim_start();
    if (trimmed.starts_with('{') || trimmed.starts_with('['))
        && serde_json::from_str::<serde_json::Value>(body).is_ok()
    {
        BodyKind::Json
    } else if body.contains('=') && (body.contains('&') || !body.contains('\n')) {
        BodyKind::FormData
    } else {
        BodyKind::Raw
    }
}

fn parse_query_params(url: &str) -> Vec<KeyValueRow> {
    let Some((_, query)) = url.split_once('?') else {
        return Vec::new();
    };
    if query.is_empty() {
        return Vec::new();
    }
    form_urlencoded::parse(query.as_bytes())
        .map(|(key, value)| KeyValueRow::enabled(key.into_owned(), value.into_owned()))
        .collect()
}

fn apply_query_params(url: &str, params: &[KeyValueRow]) -> String {
    let base = url.split('?').next().unwrap_or(url);
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for row in params {
        if row.enabled && !row.key.is_empty() {
            serializer.append_pair(&row.key, &row.value);
        }
    }
    let query = serializer.finish();
    if query.is_empty() {
        base.to_string()
    } else {
        format!("{base}?{query}")
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
    value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
        .unwrap_or(value)
        .trim()
        .to_string()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct FakeService {
        seen: Arc<Mutex<Vec<Request>>>,
        result: Result<RequestResult, AppError>,
    }

    impl RequestService for FakeService {
        fn execute(&self, request: &Request) -> Result<RequestResult, AppError> {
            self.seen.lock().unwrap().push(request.clone());
            self.result.clone()
        }
    }

    #[test]
    fn url_and_params_remain_one_consistent_draft() {
        let mut vm = RequestViewModel::with_service(Box::new(FakeService {
            seen: Arc::new(Mutex::new(Vec::new())),
            result: Err(AppError::UrlEmpty),
        }));
        vm.set_url("https://example.com/users?page=1");
        assert_eq!(vm.params(), &[KeyValueRow::enabled("page", "1")]);

        vm.upsert_param("limit", "20");
        assert_eq!(vm.url(), "https://example.com/users?page=1&limit=20");

        vm.toggle_param(0);
        assert_eq!(vm.url(), "https://example.com/users?limit=20");
    }

    #[test]
    fn send_builds_request_and_transitions_response() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut vm = RequestViewModel::with_service(Box::new(FakeService {
            seen: seen.clone(),
            result: Ok(RequestResult {
                status: 201,
                headers: vec![("x-test".into(), "yes".into())],
                body: r#"{"ok":true}"#.into(),
                elapsed_ms: 7,
            }),
        }));
        vm.set_method(HttpMethod::POST);
        vm.set_url("https://example.com/users");
        vm.set_body(r#"{"name":"Ada"}"#);
        vm.upsert_header("X-Trace", "abc");
        vm.send();

        let requests = seen.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, HttpMethod::POST);
        assert_eq!(requests[0].body.as_deref(), Some(r#"{"name":"Ada"}"#));
        assert!(requests[0]
            .headers
            .iter()
            .any(|(key, value)| key == "X-Trace" && value == "abc"));
        assert!(matches!(
            vm.response(),
            ResponseState::Success { status: 201, .. }
        ));
        assert!(!vm.is_dirty());
    }

    #[test]
    fn failed_send_does_not_enter_history() {
        let request = RequestViewModel::with_service(Box::new(FakeService {
            seen: Arc::new(Mutex::new(Vec::new())),
            result: Err(AppError::UrlEmpty),
        }));
        let mut workspace = WorkspaceViewModel::with_request(request);
        workspace.send();

        assert!(matches!(workspace.response(), ResponseState::Error { .. }));
        assert_eq!(workspace.history_len(), 0);
    }

    #[test]
    fn bearer_auth_is_normalized_and_sent_as_authorization_header() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut vm = RequestViewModel::with_service(Box::new(FakeService {
            seen: seen.clone(),
            result: Ok(RequestResult {
                status: 200,
                headers: Vec::new(),
                body: String::new(),
                elapsed_ms: 1,
            }),
        }));
        vm.set_url("https://example.com/me");
        vm.set_bearer_token("Bearer secret-token");
        vm.send();

        assert_eq!(vm.bearer_token(), "secret-token");
        assert!(seen.lock().unwrap()[0]
            .headers
            .iter()
            .any(|(key, value)| key.eq_ignore_ascii_case("authorization")
                && value == "Bearer secret-token"));
    }

    #[test]
    fn tabs_preserve_independent_request_and_editor_state() {
        let mut workspace = WorkspaceViewModel::new();
        workspace.set_url("https://first.example");
        workspace.set_body(r#"{"tab":1}"#);
        workspace.set_bearer_token("first-token");
        workspace.set_pre_request_script("const first = true;");
        workspace.set_tests_script("status == 200");

        workspace.new_request();
        workspace.set_url("https://second.example");
        workspace.set_body(r#"{"tab":2}"#);
        workspace.set_bearer_token("second-token");

        assert_eq!(workspace.tab_count(), 2);
        assert_eq!(workspace.active_tab_index(), 1);
        assert!(workspace.select_tab(0));
        assert_eq!(workspace.url(), "https://first.example");
        assert_eq!(workspace.body(), r#"{"tab":1}"#);
        assert_eq!(workspace.bearer_token(), "first-token");
        assert_eq!(workspace.pre_request_script(), "const first = true;");
        assert_eq!(workspace.tests_script(), "status == 200");

        assert!(workspace.select_tab(1));
        assert_eq!(workspace.url(), "https://second.example");
        assert_eq!(workspace.bearer_token(), "second-token");
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
        let seen = Arc::new(Mutex::new(Vec::new()));
        let request = RequestViewModel::with_service(Box::new(FakeService {
            seen,
            result: Ok(RequestResult {
                status: 204,
                headers: Vec::new(),
                body: String::new(),
                elapsed_ms: 2,
            }),
        }));
        let mut workspace = WorkspaceViewModel::with_request(request);
        workspace.set_url("https://example.com/shared-history");
        workspace.send();

        assert_eq!(workspace.history_len(), 1);
        assert_eq!(
            workspace.history()[0].request.url,
            "https://example.com/shared-history"
        );
    }
}

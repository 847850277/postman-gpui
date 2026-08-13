use mockito::{Matcher, Mock, Server};
use postman_gpui::{
    app::{BodyKind, RequestService, RequestViewModel, ResponseState, WorkspaceViewModel},
    errors::AppError,
    http::executor::{RequestExecutor, RequestResult},
    models::{HttpMethod, Request},
};
use serde::Deserialize;
use std::{
    str::FromStr,
    sync::{Arc, Mutex},
};

const SCENARIO_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioSuite {
    pub schema_version: u32,
    pub cases: Vec<RequestScenario>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestScenario {
    pub name: String,
    pub draft: DraftSpec,
    #[serde(default)]
    pub mock: Option<MockSpec>,
    pub expect: ExpectSpec,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftSpec {
    pub method: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub params: Vec<KeyValueSpec>,
    #[serde(default)]
    pub headers: Vec<KeyValueSpec>,
    pub body: Option<String>,
    pub body_kind: Option<String>,
    pub bearer_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyValueSpec {
    pub key: String,
    pub value: String,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MockSpec {
    pub status: u16,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub body: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectSpec {
    pub request: RequestSpec,
    pub response: ResponseSpec,
    pub history_len: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestSpec {
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResponseSpec {
    Success {
        status: u16,
        #[serde(default)]
        body_contains: Option<String>,
        #[serde(default)]
        headers_contain: Vec<(String, String)>,
    },
    Error {
        contains: String,
    },
}

struct RecordingExecutor {
    inner: RequestExecutor,
    seen: Arc<Mutex<Vec<Request>>>,
}

impl RequestService for RecordingExecutor {
    fn execute(&self, request: &Request) -> Result<RequestResult, AppError> {
        self.seen.lock().unwrap().push(request.clone());
        self.inner.execute_request(request)
    }
}

pub fn load_suite(json: &str) -> Result<ScenarioSuite, String> {
    let suite: ScenarioSuite = serde_json::from_str(json)
        .map_err(|error| format!("request scenario JSON is invalid: {error}"))?;
    if suite.schema_version != SCENARIO_SCHEMA_VERSION {
        return Err(format!(
            "unsupported request scenario schema version: {} (expected {SCENARIO_SCHEMA_VERSION})",
            suite.schema_version
        ));
    }
    if suite.cases.is_empty() {
        return Err("request scenario suite must contain at least one case".to_string());
    }
    Ok(suite)
}

pub fn run_scenario(scenario: &RequestScenario) -> Result<(), String> {
    let mut server = scenario.mock.is_some().then(Server::new);
    let server_url = server.as_ref().map(|server| server.url());
    let mock = match (server.as_mut(), &scenario.mock) {
        (Some(server), Some(spec)) => Some(install_mock(server, &scenario.expect.request, spec)?),
        (None, None) => None,
        _ => {
            return Err("a mock server is required whenever `mock` is present".to_string());
        }
    };

    let seen = Arc::new(Mutex::new(Vec::new()));
    let request = RequestViewModel::with_service(Box::new(RecordingExecutor {
        inner: RequestExecutor::new(),
        seen: seen.clone(),
    }));
    let mut workspace = WorkspaceViewModel::with_request(request);

    apply_draft(&mut workspace, &scenario.draft, server_url.as_deref())?;
    workspace.send();

    assert_outgoing_request(
        seen.lock().unwrap().as_slice(),
        &scenario.expect.request,
        server_url.as_deref(),
    )?;
    if let Some(mock) = &mock {
        if !mock.matched() {
            return Err(
                "mock server did not receive a request matching the transport contract".to_string(),
            );
        }
    }
    assert_response(workspace.response(), &scenario.expect.response)?;

    if workspace.history_len() != scenario.expect.history_len {
        return Err(format!(
            "history length mismatch: expected {}, actual {}",
            scenario.expect.history_len,
            workspace.history_len()
        ));
    }
    if scenario.expect.history_len > 0 {
        let expected = expected_request(&scenario.expect.request, server_url.as_deref())?;
        if workspace.history().first().map(|entry| &entry.request) != Some(&expected) {
            return Err("latest history entry does not contain the outgoing request".to_string());
        }
    }

    Ok(())
}

fn install_mock(
    server: &mut Server,
    request: &RequestSpec,
    spec: &MockSpec,
) -> Result<Mock, String> {
    let method = parse_method(&request.method)?;
    let (path, query) = split_path_query(&request.path);
    if path.is_empty() {
        return Err("mocked scenarios must expect a request path".to_string());
    }

    let mut mock = server.mock(method.to_string().as_str(), path);
    if let Some(query) = query {
        mock = mock.match_query(Matcher::Exact(query.to_string()));
    }
    if let Some(body) = &request.body {
        mock = mock.match_body(Matcher::Exact(body.clone()));
    }
    for (key, value) in &request.headers {
        mock = mock.match_header(key.as_str(), value.as_str());
    }
    mock = mock
        .with_status(spec.status as usize)
        .with_body(spec.body.clone());
    for (key, value) in &spec.headers {
        mock = mock.with_header(key, value);
    }
    Ok(mock.create())
}

fn apply_draft(
    workspace: &mut WorkspaceViewModel,
    draft: &DraftSpec,
    server_url: Option<&str>,
) -> Result<(), String> {
    workspace.set_method(parse_method(&draft.method)?);
    workspace.set_url(absolute_url(server_url, &draft.path)?);

    for param in &draft.params {
        workspace.upsert_param(&param.key, &param.value);
        if !param.enabled {
            let index = workspace
                .params()
                .iter()
                .position(|row| row.key == param.key)
                .ok_or_else(|| format!("parameter `{}` was not added", param.key))?;
            workspace.toggle_param(index);
        }
    }
    for header in &draft.headers {
        workspace.upsert_header(&header.key, &header.value);
        if !header.enabled {
            let index = workspace
                .headers()
                .iter()
                .position(|row| row.key.eq_ignore_ascii_case(&header.key))
                .ok_or_else(|| format!("header `{}` was not added", header.key))?;
            workspace.toggle_header(index);
        }
    }
    if let Some(body) = &draft.body {
        workspace.set_body(body);
    }
    if let Some(body_kind) = &draft.body_kind {
        workspace.set_body_kind(parse_body_kind(body_kind)?);
    }
    if let Some(token) = &draft.bearer_token {
        workspace.set_bearer_token(token);
    }
    Ok(())
}

fn assert_outgoing_request(
    sent: &[Request],
    spec: &RequestSpec,
    server_url: Option<&str>,
) -> Result<(), String> {
    let expected = expected_request(spec, server_url)?;
    if sent == [expected.clone()] {
        return Ok(());
    }
    Err(format!(
        "outgoing request mismatch\n  expected: {expected:#?}\n  actual:   {sent:#?}"
    ))
}

fn expected_request(spec: &RequestSpec, server_url: Option<&str>) -> Result<Request, String> {
    Ok(Request {
        method: parse_method(&spec.method)?,
        url: absolute_url(server_url, &spec.path)?,
        headers: spec.headers.clone(),
        body: spec.body.clone(),
    })
}

fn absolute_url(server_url: Option<&str>, path: &str) -> Result<String, String> {
    match (server_url, path) {
        (_, "") => Ok(String::new()),
        (Some(base), path) => Ok(format!("{base}{path}")),
        (None, path) => Err(format!(
            "scenario path `{path}` requires a mock server so the runner can assign a host"
        )),
    }
}

fn split_path_query(path: &str) -> (&str, Option<&str>) {
    match path.split_once('?') {
        Some((path, query)) if !query.is_empty() => (path, Some(query)),
        Some((path, _)) => (path, None),
        None => (path, None),
    }
}

fn parse_method(value: &str) -> Result<HttpMethod, String> {
    HttpMethod::from_str(value).map_err(|error| format!("invalid method `{value}`: {error}"))
}

fn parse_body_kind(value: &str) -> Result<BodyKind, String> {
    match value.to_ascii_lowercase().as_str() {
        "json" => Ok(BodyKind::Json),
        "form_data" => Ok(BodyKind::FormData),
        "raw" => Ok(BodyKind::Raw),
        _ => Err(format!("invalid body kind `{value}`")),
    }
}

fn assert_response(actual: &ResponseState, expected: &ResponseSpec) -> Result<(), String> {
    match (actual, expected) {
        (
            ResponseState::Success {
                status: actual_status,
                body: actual_body,
                headers: actual_headers,
                ..
            },
            ResponseSpec::Success {
                status,
                body_contains,
                headers_contain,
            },
        ) if actual_status == status
            && body_contains
                .as_ref()
                .map(|needle| actual_body.contains(needle))
                .unwrap_or(true)
            && headers_contain
                .iter()
                .all(|(expected_name, expected_value)| {
                    actual_headers.iter().any(|(actual_name, actual_value)| {
                        actual_name.eq_ignore_ascii_case(expected_name)
                            && actual_value == expected_value
                    })
                }) =>
        {
            Ok(())
        }
        (ResponseState::Error { message }, ResponseSpec::Error { contains })
            if message.contains(contains) =>
        {
            Ok(())
        }
        _ => Err(format!(
            "response mismatch\n  expected: {expected:#?}\n  actual:   {actual:#?}"
        )),
    }
}

fn enabled_by_default() -> bool {
    true
}

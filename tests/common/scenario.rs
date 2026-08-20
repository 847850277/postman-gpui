use mockito::{Matcher, Mock, Server};
use postman_gpui::{
    app::{
        AuthorizationKind, BodyKind, MultipartDraftPart, RequestPane, ResponseState,
        WorkspaceViewModel,
    },
    http::executor::RequestExecutor,
    models::{
        HttpMethod, MultipartEditorPart, MultipartPart, MultipartValue, Request, RequestBody,
        RequestEditorIntent,
    },
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    fs,
    path::{Component, Path, PathBuf},
    str::FromStr,
};

const SCENARIO_SCHEMA_VERSION: u32 = 5;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioSuite {
    pub schema_version: u32,
    pub target: ScenarioTarget,
    pub cases: Vec<RequestScenario>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioTarget {
    Local,
    Httpbingo,
}

#[derive(Debug)]
pub struct ScenarioFile {
    pub path: PathBuf,
    pub suite: ScenarioSuite,
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
    pub precreate_param_rows: usize,
    #[serde(default)]
    pub headers: Vec<KeyValueSpec>,
    #[serde(default)]
    pub precreate_header_rows: usize,
    pub body: Option<String>,
    pub body_kind: Option<String>,
    #[serde(default)]
    pub body_rows: Vec<KeyValueSpec>,
    #[serde(default)]
    pub multipart_parts: Vec<MultipartPartSpec>,
    #[serde(default)]
    pub precreate_body_rows: usize,
    pub bearer_token: Option<String>,
    pub basic_auth: Option<BasicAuthSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BasicAuthSpec {
    pub username: String,
    pub password: String,
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
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MultipartPartSpec {
    Text {
        name: String,
        value: String,
        #[serde(default = "enabled_by_default")]
        enabled: bool,
    },
    File {
        name: String,
        path: PathBuf,
        #[serde(default)]
        file_name: Option<String>,
        #[serde(default)]
        content_type: Option<String>,
        #[serde(default = "enabled_by_default")]
        enabled: bool,
    },
}

impl MultipartPartSpec {
    pub fn name(&self) -> &str {
        match self {
            Self::Text { name, .. } | Self::File { name, .. } => name,
        }
    }

    pub fn enabled(&self) -> bool {
        match self {
            Self::Text { enabled, .. } | Self::File { enabled, .. } => *enabled,
        }
    }

    fn to_draft_part(&self) -> Result<MultipartDraftPart, String> {
        Ok(match self {
            Self::Text {
                name,
                value,
                enabled,
            } => MultipartDraftPart::text(name, value, *enabled),
            Self::File {
                name,
                path,
                file_name,
                content_type,
                enabled,
            } => MultipartDraftPart::file(
                name,
                resolve_scenario_fixture_path(path)?,
                file_name.clone(),
                content_type.clone(),
                *enabled,
            ),
        })
    }

    fn to_request_part(&self) -> Result<MultipartPart, String> {
        Ok(match self {
            Self::Text { name, value, .. } => MultipartPart::text(name, value),
            Self::File {
                name,
                path,
                file_name,
                content_type,
                ..
            } => MultipartPart {
                name: name.clone(),
                value: MultipartValue::File {
                    path: resolve_scenario_fixture_path(path)?,
                    file_name: file_name.clone(),
                    content_type: content_type.clone(),
                },
            },
        })
    }

    fn to_editor_part(&self) -> Result<MultipartEditorPart, String> {
        Ok(match self {
            Self::Text {
                name,
                value,
                enabled,
            } => MultipartEditorPart {
                enabled: *enabled,
                name: name.clone(),
                value: MultipartValue::Text(value.clone()),
            },
            Self::File {
                name,
                path,
                file_name,
                content_type,
                enabled,
            } => MultipartEditorPart {
                enabled: *enabled,
                name: name.clone(),
                value: MultipartValue::File {
                    path: resolve_scenario_fixture_path(path)?,
                    file_name: file_name.clone(),
                    content_type: content_type.clone(),
                },
            },
        })
    }
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
    #[serde(default)]
    pub body_kind: Option<String>,
    #[serde(default)]
    pub multipart_parts: Vec<MultipartPartSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResponseSpec {
    Success {
        status: u16,
        #[serde(default)]
        body_contains: Option<String>,
        #[serde(default)]
        body_json_contains: Option<Value>,
        #[serde(default)]
        headers_contain: Vec<(String, String)>,
    },
    Error {
        contains: String,
    },
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
    for scenario in &suite.cases {
        validate_body_row_contract(&scenario.draft)
            .map_err(|error| format!("scenario `{}` draft is invalid: {error}", scenario.name))?;
        expected_body(&scenario.expect.request).map_err(|error| {
            format!(
                "scenario `{}` expected request is invalid: {error}",
                scenario.name
            )
        })?;
    }
    Ok(suite)
}

pub fn validate_body_row_contract(draft: &DraftSpec) -> Result<(), String> {
    if draft.body_rows.is_empty()
        && draft.multipart_parts.is_empty()
        && draft.precreate_body_rows == 0
    {
        return Ok(());
    }
    let body_kind = draft.body_kind.as_deref().unwrap_or_default();
    if !draft.multipart_parts.is_empty() {
        if !body_kind.eq_ignore_ascii_case("multipart") {
            return Err("`multipart_parts` requires `body_kind: multipart`".to_string());
        }
        if !draft.body_rows.is_empty() {
            return Err(
                "a multipart scenario cannot mix `body_rows` and `multipart_parts`".to_string(),
            );
        }
        if draft.body.is_some() {
            return Err(
                "a typed `multipart_parts` scenario must not duplicate its payload in `body`"
                    .to_string(),
            );
        }
        if draft.precreate_body_rows > 0 && draft.multipart_parts.len() > draft.precreate_body_rows
        {
            return Err(format!(
                "scenario defines {} multipart parts but precreates only {} rows",
                draft.multipart_parts.len(),
                draft.precreate_body_rows
            ));
        }
        for part in &draft.multipart_parts {
            if part.name().trim().is_empty() {
                return Err("a typed multipart part must declare a nonblank `name`".to_string());
            }
            if let MultipartPartSpec::File { path, .. } = part {
                resolve_scenario_fixture_path(path)?;
            }
        }
        return Ok(());
    }
    if !body_kind.eq_ignore_ascii_case("url_encoded")
        && !body_kind.eq_ignore_ascii_case("multipart")
    {
        return Err("`body_rows` and `precreate_body_rows` require a form body kind".to_string());
    }
    if draft.body.is_none() {
        return Err("a form-row scenario must declare its effective `body`".to_string());
    }
    if draft.precreate_body_rows > 0 && draft.body_rows.len() > draft.precreate_body_rows {
        return Err(format!(
            "scenario defines {} form rows but precreates only {}",
            draft.body_rows.len(),
            draft.precreate_body_rows
        ));
    }

    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for row in draft
        .body_rows
        .iter()
        .filter(|row| row.enabled && !row.key.trim().is_empty())
    {
        serializer.append_pair(&row.key, &row.value);
    }
    let effective_body = serializer.finish();
    if draft.body.as_deref() != Some(effective_body.as_str()) {
        return Err(format!(
            "form rows do not match the declared effective body\n  expected: {effective_body:?}\n  actual:   {:?}",
            draft.body
        ));
    }
    Ok(())
}

pub fn resolve_scenario_fixture_path(path: &Path) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(format!(
            "scenario fixture path must be a nonempty repository-relative path: {}",
            path.display()
        ));
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(format!(
            "scenario fixture path traversal is not allowed: {}",
            path.display()
        ));
    }

    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .map_err(|error| format!("failed to resolve repository root: {error}"))?;
    let resolved = repository.join(path).canonicalize().map_err(|error| {
        format!(
            "failed to resolve scenario fixture {}: {error}",
            path.display()
        )
    })?;
    if !resolved.starts_with(&repository) {
        return Err(format!(
            "scenario fixture resolves outside the repository: {}",
            path.display()
        ));
    }
    if !resolved.is_file() {
        return Err(format!(
            "scenario fixture is not a file: {}",
            path.display()
        ));
    }
    Ok(resolved)
}

pub fn expected_editor_intent(draft: &DraftSpec) -> Result<Option<RequestEditorIntent>, String> {
    if !draft
        .body_kind
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("multipart"))
    {
        return Ok(None);
    }
    validate_body_row_contract(draft)?;

    let mut parts = if !draft.multipart_parts.is_empty() {
        draft
            .multipart_parts
            .iter()
            .map(MultipartPartSpec::to_editor_part)
            .collect::<Result<Vec<_>, _>>()?
    } else if !draft.body_rows.is_empty() {
        draft
            .body_rows
            .iter()
            .map(|row| MultipartEditorPart {
                enabled: row.enabled,
                name: row.key.clone(),
                value: MultipartValue::Text(row.value.clone()),
            })
            .collect()
    } else {
        form_urlencoded::parse(draft.body.as_deref().unwrap_or_default().as_bytes())
            .map(|(name, value)| MultipartEditorPart {
                enabled: true,
                name: name.into_owned(),
                value: MultipartValue::Text(value.into_owned()),
            })
            .collect()
    };
    let row_count = draft.precreate_body_rows.max(parts.len()).max(1);
    parts.resize_with(row_count, || MultipartEditorPart {
        enabled: true,
        name: String::new(),
        value: MultipartValue::Text(String::new()),
    });
    Ok(Some(RequestEditorIntent::Multipart(parts)))
}

pub fn load_suites(root: &Path) -> Result<Vec<ScenarioFile>, String> {
    let mut paths = Vec::new();
    collect_json_files(root, &mut paths)?;
    paths.sort();

    if paths.is_empty() {
        return Err(format!(
            "request scenario directory contains no JSON files: {}",
            root.display()
        ));
    }

    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        let json = fs::read_to_string(&path).map_err(|error| {
            format!(
                "failed to read request scenario file {}: {error}",
                path.display()
            )
        })?;
        let suite = load_suite(&json).map_err(|error| format!("{}: {error}", path.display()))?;
        files.push(ScenarioFile { path, suite });
    }

    if files.iter().all(|file| file.suite.cases.is_empty()) {
        return Err("request scenario files must define at least one case".to_string());
    }

    Ok(files)
}

fn collect_json_files(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(directory).map_err(|error| {
        format!(
            "failed to read request scenario directory {}: {error}",
            directory.display()
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "failed to inspect request scenario directory {}: {error}",
                directory.display()
            )
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| {
            format!(
                "failed to inspect request scenario path {}: {error}",
                path.display()
            )
        })?;
        if file_type.is_dir() {
            collect_json_files(&path, paths)?;
        } else if file_type.is_file() && path.extension().is_some_and(|value| value == "json") {
            paths.push(path);
        }
    }

    Ok(())
}

#[allow(dead_code)]
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

    execute_scenario(scenario, server_url.as_deref(), mock.as_ref())
}

fn execute_scenario(
    scenario: &RequestScenario,
    server_url: Option<&str>,
    mock: Option<&Mock>,
) -> Result<(), String> {
    let mut workspace = WorkspaceViewModel::new();

    apply_draft(&mut workspace, &scenario.draft, server_url)?;
    let pending = workspace.begin_send();
    let sent = vec![pending.request().clone()];
    let executor = RequestExecutor::new();
    let task = executor.spawn(pending.request().clone());
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed to create scenario scheduling adapter: {error}"))?
        .block_on(task);
    workspace.complete_send(pending, result);

    assert_outgoing_request(&sent, &scenario.expect.request, server_url)?;
    if let Some(mock) = mock {
        if !mock.matched() {
            return Err(
                "mock server did not receive a request matching the transport contract".to_string(),
            );
        }
    }
    assert_response_state(workspace.response(), &scenario.expect.response)?;

    if workspace.history_len() != scenario.expect.history_len {
        return Err(format!(
            "history length mismatch: expected {}, actual {}",
            scenario.expect.history_len,
            workspace.history_len()
        ));
    }
    if scenario.expect.history_len > 0 {
        let expected = expected_request(&scenario.expect.request, server_url)?;
        let actual = workspace
            .history()
            .first()
            .map(|entry| &entry.request)
            .ok_or_else(|| "latest history entry is missing".to_string())?;
        assert_requests_equivalent(actual, &expected)
            .map_err(|error| format!("latest history entry is incorrect: {error}"))?;
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
    validate_body_row_contract(draft)?;
    if draft.bearer_token.is_some() && draft.basic_auth.is_some() {
        return Err("`bearer_token` and `basic_auth` are mutually exclusive".to_string());
    }

    workspace.set_method(parse_method(&draft.method)?);
    workspace.set_url(absolute_url(server_url, &draft.path)?);

    for _ in 0..draft.precreate_param_rows {
        workspace.commit_row_draft(RequestPane::Params);
    }

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
    if draft.precreate_header_rows == 0 {
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
    } else {
        if draft.headers.len() > draft.precreate_header_rows {
            return Err(format!(
                "scenario defines {} Headers rows but precreates only {}",
                draft.headers.len(),
                draft.precreate_header_rows
            ));
        }
        for _ in 0..draft.precreate_header_rows {
            workspace.append_header_row();
        }
        for (index, header) in draft.headers.iter().enumerate() {
            workspace.set_header_key(index, &header.key);
            workspace.set_header_value(index, &header.value);
            if !header.enabled {
                workspace.toggle_header(index);
            }
        }
    }
    if let Some(body) = &draft.body {
        workspace.set_body(body);
    }
    if let Some(body_kind) = &draft.body_kind {
        workspace.set_body_kind(parse_body_kind(body_kind)?);
    }
    if !draft.body_rows.is_empty()
        || !draft.multipart_parts.is_empty()
        || draft.precreate_body_rows > 0
    {
        let row_count = draft
            .precreate_body_rows
            .max(draft.body_rows.len())
            .max(draft.multipart_parts.len())
            .max(1);
        match draft
            .body_kind
            .as_deref()
            .map(parse_body_kind)
            .transpose()?
        {
            Some(BodyKind::UrlEncoded) => {
                let mut rows = draft
                    .body_rows
                    .iter()
                    .map(|row| postman_gpui::app::KeyValueRow {
                        enabled: row.enabled,
                        key: row.key.clone(),
                        value: row.value.clone(),
                    })
                    .collect::<Vec<_>>();
                rows.resize_with(row_count, || {
                    postman_gpui::app::KeyValueRow::enabled("", "")
                });
                workspace.set_url_encoded_rows(rows);
            }
            Some(BodyKind::Multipart) => {
                let mut parts = if draft.multipart_parts.is_empty() {
                    draft
                        .body_rows
                        .iter()
                        .map(|row| MultipartDraftPart::text(&row.key, &row.value, row.enabled))
                        .collect::<Vec<_>>()
                } else {
                    draft
                        .multipart_parts
                        .iter()
                        .map(MultipartPartSpec::to_draft_part)
                        .collect::<Result<Vec<_>, _>>()?
                };
                parts.resize_with(row_count, || MultipartDraftPart::text("", "", true));
                workspace.set_multipart_draft_parts(parts);
            }
            _ => unreachable!("form row contract validates the body kind"),
        }
    }
    if let Some(token) = &draft.bearer_token {
        workspace.set_authorization_kind(AuthorizationKind::Bearer);
        workspace.set_bearer_token(token);
    }
    if let Some(credentials) = &draft.basic_auth {
        workspace.set_authorization_kind(AuthorizationKind::Basic);
        workspace.set_basic_username(&credentials.username);
        workspace.set_basic_password(&credentials.password);
    }
    Ok(())
}

fn assert_outgoing_request(
    sent: &[Request],
    spec: &RequestSpec,
    server_url: Option<&str>,
) -> Result<(), String> {
    let expected = expected_request(spec, server_url)?;
    if let [actual] = sent {
        return assert_requests_equivalent(actual, &expected);
    }
    Err(format!(
        "outgoing request mismatch\n  expected: {expected:#?}\n  actual:   {sent:#?}"
    ))
}

pub fn assert_requests_equivalent(actual: &Request, expected: &Request) -> Result<(), String> {
    let mut actual_headers: Vec<_> = actual
        .headers
        .iter()
        .map(|(name, value)| (name.to_ascii_lowercase(), value.as_str()))
        .collect();
    let mut expected_headers: Vec<_> = expected
        .headers
        .iter()
        .map(|(name, value)| (name.to_ascii_lowercase(), value.as_str()))
        .collect();
    actual_headers.sort_unstable();
    expected_headers.sort_unstable();

    if actual.method == expected.method
        && actual.url == expected.url
        && actual_headers == expected_headers
        && actual.body == expected.body
    {
        return Ok(());
    }

    Err(format!(
        "request mismatch\n  expected: {expected:#?}\n  actual:   {actual:#?}"
    ))
}

pub fn expected_request(spec: &RequestSpec, server_url: Option<&str>) -> Result<Request, String> {
    Ok(Request {
        method: parse_method(&spec.method)?,
        url: absolute_url(server_url, &spec.path)?,
        headers: spec.headers.clone(),
        body: expected_body(spec)?,
    })
}

fn expected_body(spec: &RequestSpec) -> Result<RequestBody, String> {
    if !spec.multipart_parts.is_empty() {
        if !spec
            .body_kind
            .as_deref()
            .is_some_and(|kind| kind.eq_ignore_ascii_case("multipart"))
        {
            return Err("expected `multipart_parts` requires `body_kind: multipart`".to_string());
        }
        if spec.body.is_some() {
            return Err(
                "expected typed `multipart_parts` must not duplicate its payload in `body`"
                    .to_string(),
            );
        }
        return Ok(RequestBody::Multipart(
            spec.multipart_parts
                .iter()
                .filter(|part| part.enabled() && !part.name().trim().is_empty())
                .map(MultipartPartSpec::to_request_part)
                .collect::<Result<Vec<_>, _>>()?,
        ));
    }
    if let Some(body_kind) = spec.body_kind.as_deref() {
        let body_kind = parse_body_kind(body_kind)?;
        let body = spec.body.as_deref().unwrap_or_default();
        return Ok(match body_kind {
            BodyKind::None => {
                if !body.is_empty() {
                    return Err("a `none` expected body cannot contain a payload".to_string());
                }
                RequestBody::None
            }
            BodyKind::Json => RequestBody::Json(body.to_string()),
            BodyKind::Raw => RequestBody::Raw(body.to_string()),
            BodyKind::UrlEncoded => RequestBody::UrlEncoded(body.to_string()),
            BodyKind::Multipart => RequestBody::Multipart(
                form_urlencoded::parse(body.as_bytes())
                    .map(|(name, value)| MultipartPart::text(name.into_owned(), value.into_owned()))
                    .collect(),
            ),
        });
    }

    let Some(body) = &spec.body else {
        return Ok(RequestBody::None);
    };
    let content_type = spec
        .headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("content-type"))
        .map(|(_, value)| value.as_str());
    Ok(match content_type {
        Some(value) if value.starts_with("application/json") => RequestBody::Json(body.clone()),
        Some(value) if value.starts_with("application/x-www-form-urlencoded") => {
            RequestBody::UrlEncoded(body.clone())
        }
        _ => RequestBody::Raw(body.clone()),
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
        "url_encoded" => Ok(BodyKind::UrlEncoded),
        "multipart" => Ok(BodyKind::Multipart),
        "none" => Ok(BodyKind::None),
        "raw" => Ok(BodyKind::Raw),
        _ => Err(format!("invalid body kind `{value}`")),
    }
}

pub fn assert_response_state(
    actual: &ResponseState,
    expected: &ResponseSpec,
) -> Result<(), String> {
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
                body_json_contains,
                headers_contain,
            },
        ) => {
            if actual_status != status {
                return Err(format!(
                    "response status mismatch: expected {status}, actual {actual_status}"
                ));
            }
            if let Some(needle) = body_contains {
                if !actual_body.contains(needle) {
                    return Err(format!(
                        "response body does not contain {needle:?}\n  actual: {actual_body}"
                    ));
                }
            }
            for (expected_name, expected_value) in headers_contain {
                if !actual_headers.iter().any(|(actual_name, actual_value)| {
                    actual_name.eq_ignore_ascii_case(expected_name)
                        && actual_value == expected_value
                }) {
                    return Err(format!(
                        "response header missing: {expected_name}: {expected_value}\n  actual: {actual_headers:#?}"
                    ));
                }
            }
            if let Some(expected_json) = body_json_contains {
                let actual_json: Value = serde_json::from_str(actual_body).map_err(|error| {
                    format!("response body is not valid JSON: {error}\n  actual: {actual_body}")
                })?;
                assert_json_contains(&actual_json, expected_json, "$")?;
            }
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

fn assert_json_contains(actual: &Value, expected: &Value, path: &str) -> Result<(), String> {
    match expected {
        Value::Object(expected_fields) => {
            let actual_fields = actual.as_object().ok_or_else(|| {
                format!("JSON mismatch at {path}: expected object subset, actual {actual}")
            })?;
            for (key, expected_value) in expected_fields {
                let actual_value = actual_fields.get(key).ok_or_else(|| {
                    format!("JSON mismatch at {path}: missing field {key:?} in {actual}")
                })?;
                assert_json_contains(actual_value, expected_value, &format!("{path}.{key}"))?;
            }
            Ok(())
        }
        _ if actual == expected => Ok(()),
        _ => Err(format!(
            "JSON mismatch at {path}: expected {expected}, actual {actual}"
        )),
    }
}

fn enabled_by_default() -> bool {
    true
}

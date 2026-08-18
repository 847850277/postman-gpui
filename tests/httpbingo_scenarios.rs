//! Opt-in application scenarios that drive a real GPUI window against HTTPBingo.

mod common;
#[path = "common/ui.rs"]
mod ui;

use common::scenario::{
    assert_requests_equivalent, assert_response_state, expected_request, load_suites, DraftSpec,
    KeyValueSpec, RequestScenario, ResponseSpec, ScenarioTarget,
};
use gpui::{AppContext, ClipboardItem, Entity, TestAppContext, VisualTestContext};
use postman_gpui::app::{KeyValueRow, PostmanApp, ResponseState, WorkspaceViewModel};
use std::path::{Path, PathBuf};
use ui::{choose_method, click, scroll_down, scroll_up, type_into};

const HTTPBINGO_BASE_URL: &str = "https://httpbingo.org";
const PARAM_TOGGLE_SELECTORS: [&str; 16] = [
    "param-row-toggle-0",
    "param-row-toggle-1",
    "param-row-toggle-2",
    "param-row-toggle-3",
    "param-row-toggle-4",
    "param-row-toggle-5",
    "param-row-toggle-6",
    "param-row-toggle-7",
    "param-row-toggle-8",
    "param-row-toggle-9",
    "param-row-toggle-10",
    "param-row-toggle-11",
    "param-row-toggle-12",
    "param-row-toggle-13",
    "param-row-toggle-14",
    "param-row-toggle-15",
];
const PARAM_KEY_SELECTORS: [&str; 16] = [
    "param-row-key-input-0",
    "param-row-key-input-1",
    "param-row-key-input-2",
    "param-row-key-input-3",
    "param-row-key-input-4",
    "param-row-key-input-5",
    "param-row-key-input-6",
    "param-row-key-input-7",
    "param-row-key-input-8",
    "param-row-key-input-9",
    "param-row-key-input-10",
    "param-row-key-input-11",
    "param-row-key-input-12",
    "param-row-key-input-13",
    "param-row-key-input-14",
    "param-row-key-input-15",
];
const PARAM_VALUE_SELECTORS: [&str; 16] = [
    "param-row-value-input-0",
    "param-row-value-input-1",
    "param-row-value-input-2",
    "param-row-value-input-3",
    "param-row-value-input-4",
    "param-row-value-input-5",
    "param-row-value-input-6",
    "param-row-value-input-7",
    "param-row-value-input-8",
    "param-row-value-input-9",
    "param-row-value-input-10",
    "param-row-value-input-11",
    "param-row-value-input-12",
    "param-row-value-input-13",
    "param-row-value-input-14",
    "param-row-value-input-15",
];
const HEADER_TOGGLE_SELECTORS: [&str; 16] = [
    "header-row-toggle-0",
    "header-row-toggle-1",
    "header-row-toggle-2",
    "header-row-toggle-3",
    "header-row-toggle-4",
    "header-row-toggle-5",
    "header-row-toggle-6",
    "header-row-toggle-7",
    "header-row-toggle-8",
    "header-row-toggle-9",
    "header-row-toggle-10",
    "header-row-toggle-11",
    "header-row-toggle-12",
    "header-row-toggle-13",
    "header-row-toggle-14",
    "header-row-toggle-15",
];
const HEADER_KEY_SELECTORS: [&str; 16] = [
    "header-row-key-input-0",
    "header-row-key-input-1",
    "header-row-key-input-2",
    "header-row-key-input-3",
    "header-row-key-input-4",
    "header-row-key-input-5",
    "header-row-key-input-6",
    "header-row-key-input-7",
    "header-row-key-input-8",
    "header-row-key-input-9",
    "header-row-key-input-10",
    "header-row-key-input-11",
    "header-row-key-input-12",
    "header-row-key-input-13",
    "header-row-key-input-14",
    "header-row-key-input-15",
];
const HEADER_VALUE_SELECTORS: [&str; 16] = [
    "header-row-value-input-0",
    "header-row-value-input-1",
    "header-row-value-input-2",
    "header-row-value-input-3",
    "header-row-value-input-4",
    "header-row-value-input-5",
    "header-row-value-input-6",
    "header-row-value-input-7",
    "header-row-value-input-8",
    "header-row-value-input-9",
    "header-row-value-input-10",
    "header-row-value-input-11",
    "header-row-value-input-12",
    "header-row-value-input-13",
    "header-row-value-input-14",
    "header-row-value-input-15",
];
const HEADER_ROW_CONTRACT_SELECTORS: [[&str; 4]; 16] = [
    [
        "header-row-key-0",
        "header-row-value-0",
        "header-row-status-0",
        "header-row-delete-0",
    ],
    [
        "header-row-key-1",
        "header-row-value-1",
        "header-row-status-1",
        "header-row-delete-1",
    ],
    [
        "header-row-key-2",
        "header-row-value-2",
        "header-row-status-2",
        "header-row-delete-2",
    ],
    [
        "header-row-key-3",
        "header-row-value-3",
        "header-row-status-3",
        "header-row-delete-3",
    ],
    [
        "header-row-key-4",
        "header-row-value-4",
        "header-row-status-4",
        "header-row-delete-4",
    ],
    [
        "header-row-key-5",
        "header-row-value-5",
        "header-row-status-5",
        "header-row-delete-5",
    ],
    [
        "header-row-key-6",
        "header-row-value-6",
        "header-row-status-6",
        "header-row-delete-6",
    ],
    [
        "header-row-key-7",
        "header-row-value-7",
        "header-row-status-7",
        "header-row-delete-7",
    ],
    [
        "header-row-key-8",
        "header-row-value-8",
        "header-row-status-8",
        "header-row-delete-8",
    ],
    [
        "header-row-key-9",
        "header-row-value-9",
        "header-row-status-9",
        "header-row-delete-9",
    ],
    [
        "header-row-key-10",
        "header-row-value-10",
        "header-row-status-10",
        "header-row-delete-10",
    ],
    [
        "header-row-key-11",
        "header-row-value-11",
        "header-row-status-11",
        "header-row-delete-11",
    ],
    [
        "header-row-key-12",
        "header-row-value-12",
        "header-row-status-12",
        "header-row-delete-12",
    ],
    [
        "header-row-key-13",
        "header-row-value-13",
        "header-row-status-13",
        "header-row-delete-13",
    ],
    [
        "header-row-key-14",
        "header-row-value-14",
        "header-row-status-14",
        "header-row-delete-14",
    ],
    [
        "header-row-key-15",
        "header-row-value-15",
        "header-row-status-15",
        "header-row-delete-15",
    ],
];
const BODY_FORM_KEY_SELECTORS: [&str; 16] = [
    "body-form-key-0",
    "body-form-key-1",
    "body-form-key-2",
    "body-form-key-3",
    "body-form-key-4",
    "body-form-key-5",
    "body-form-key-6",
    "body-form-key-7",
    "body-form-key-8",
    "body-form-key-9",
    "body-form-key-10",
    "body-form-key-11",
    "body-form-key-12",
    "body-form-key-13",
    "body-form-key-14",
    "body-form-key-15",
];
const BODY_FORM_VALUE_SELECTORS: [&str; 16] = [
    "body-form-value-0",
    "body-form-value-1",
    "body-form-value-2",
    "body-form-value-3",
    "body-form-value-4",
    "body-form-value-5",
    "body-form-value-6",
    "body-form-value-7",
    "body-form-value-8",
    "body-form-value-9",
    "body-form-value-10",
    "body-form-value-11",
    "body-form-value-12",
    "body-form-value-13",
    "body-form-value-14",
    "body-form-value-15",
];

#[derive(Clone, Copy)]
enum RowEditor {
    Params,
    Headers,
}

fn scenario_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases")
}

fn source_name(path: &Path) -> String {
    path.strip_prefix(env!("CARGO_MANIFEST_DIR"))
        .unwrap_or(path)
        .display()
        .to_string()
}

fn query_rows_from_path(path: &str) -> Vec<KeyValueRow> {
    let Some((_, query_and_fragment)) = path.split_once('?') else {
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

#[gpui::test]
#[ignore = "requires public HTTPBingo network access"]
fn httpbingo_scenarios_drive_the_real_application_window(cx: &mut TestAppContext) {
    let files = load_suites(&scenario_root()).expect("scenario files should parse");
    let mut scenario_count = 0;
    let mut failures = Vec::new();

    for file in files
        .iter()
        .filter(|file| file.suite.target == ScenarioTarget::Httpbingo)
    {
        for scenario in &file.suite.cases {
            scenario_count += 1;
            if let Err(failure) = run_application_scenario(cx, scenario) {
                failures.push(format!(
                    "- {} :: {}\n{failure}",
                    source_name(&file.path),
                    scenario.name
                ));
            }
        }
    }

    assert!(scenario_count > 0, "no HTTPBingo scenarios were discovered");
    assert!(
        failures.is_empty(),
        "HTTPBingo application scenario failures:\n\n{}",
        failures.join("\n\n")
    );
}

fn run_application_scenario(
    test_cx: &mut TestAppContext,
    scenario: &RequestScenario,
) -> Result<(), String> {
    if scenario.mock.is_some() {
        return Err("HTTPBingo scenarios must not define a local `mock`".to_string());
    }

    let expected = expected_request(&scenario.expect.request, Some(HTTPBINGO_BASE_URL))?;
    let workspace = test_cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        test_cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    choose_method(cx, &scenario.draft.method)?;
    type_into(
        cx,
        "url-input",
        &format!("{HTTPBINGO_BASE_URL}{}", scenario.draft.path),
    )?;
    let url_rows = query_rows_from_path(&scenario.draft.path);
    let projected_url_rows = workspace.read_with(cx, |workspace, _| workspace.params().to_vec());
    if projected_url_rows != url_rows {
        return Err(format!(
            "URL query was not projected into Params\n  expected: {url_rows:#?}\n  actual:   {projected_url_rows:#?}"
        ));
    }
    for index in 0..url_rows.len() {
        let selector = row_toggle_selector(RowEditor::Params, index)?;
        if cx.debug_bounds(selector).is_none() {
            return Err(format!(
                "URL query parameter row `{selector}` is not rendered"
            ));
        }
    }
    if scenario.draft.precreate_param_rows == 0 {
        apply_rows(cx, &workspace, RowEditor::Params, &scenario.draft.params)?;
    } else {
        apply_precreated_param_rows(
            cx,
            &workspace,
            url_rows.len(),
            scenario.draft.precreate_param_rows,
            &scenario.draft.params,
        )?;
    }
    if scenario.draft.precreate_header_rows == 0 {
        apply_rows(cx, &workspace, RowEditor::Headers, &scenario.draft.headers)?;
    } else {
        apply_precreated_header_rows(
            cx,
            &workspace,
            scenario.draft.precreate_header_rows,
            &scenario.draft.headers,
        )?;
    }
    assert_headers_editor_contract(cx, &scenario.draft.headers)?;

    if !url_rows.is_empty() || !scenario.draft.params.is_empty() {
        for selector in [
            "params-enabled-count",
            "effective-url-preview",
            "params-ready-indicator",
        ] {
            if cx.debug_bounds(selector).is_none() {
                return Err(format!(
                    "query parameter contract element `{selector}` is not rendered"
                ));
            }
        }
        if !url_rows.is_empty() && cx.debug_bounds("url-query-count").is_none() {
            return Err("URL query count badge is not rendered".to_string());
        }
    }

    if scenario.draft.bearer_token.is_some() && scenario.draft.basic_auth.is_some() {
        return Err("`bearer_token` and `basic_auth` are mutually exclusive".to_string());
    }
    if let Some(token) = &scenario.draft.bearer_token {
        click(cx, "request-pane-authorization")?;
        assert_bearer_editor_contract(cx)?;
        type_into(cx, "authorization-input", token)?;
        let live_token =
            workspace.read_with(cx, |workspace, _| workspace.bearer_token().to_string());
        if live_token != *token {
            return Err(format!(
                "active Bearer input was not saved to the ViewModel\n  expected: {token:?}\n  actual:   {live_token:?}"
            ));
        }
    }
    if let Some(credentials) = &scenario.draft.basic_auth {
        click(cx, "request-pane-authorization")?;
        click(cx, "auth-kind-basic")?;
        type_into(cx, "basic-auth-username-input", &credentials.username)?;
        type_into(cx, "basic-auth-password-input", &credentials.password)?;
    }

    apply_body(cx, &scenario.draft)?;

    let assembled_url = workspace.read_with(cx, |workspace, _| workspace.effective_url());
    if assembled_url != expected.url {
        return Err(format!(
            "application URL mismatch before Send\n  expected: {:?}\n  actual:   {:?}",
            expected.url, assembled_url
        ));
    }

    click(cx, "send-button")?;
    cx.run_until_parked();

    let response = workspace.read_with(cx, |workspace, _| workspace.response().clone());
    assert_response_state(&response, &scenario.expect.response)?;
    assert_disabled_headers_absent_from_echo(&response, &scenario.draft.headers)?;
    assert_response_quick_copy(cx, &workspace, &response)?;

    if cx.debug_bounds("response-container").is_none() {
        return Err("response panel is not rendered in the application window".to_string());
    }
    if matches!(
        scenario.expect.response,
        ResponseSpec::Success { .. } | ResponseSpec::Error { .. }
    ) && cx.debug_bounds("response-content").is_none()
    {
        return Err("response content is not rendered in the application window".to_string());
    }

    let history_len = workspace.read_with(cx, |workspace, _| workspace.history_len());
    if history_len != scenario.expect.history_len {
        return Err(format!(
            "history length mismatch: expected {}, actual {history_len}",
            scenario.expect.history_len
        ));
    }

    let recorded_request = workspace.read_with(cx, |workspace, _| {
        workspace
            .history()
            .first()
            .map(|entry| entry.request.clone())
    });
    match (scenario.expect.history_len > 0, recorded_request) {
        (true, Some(actual)) => {
            assert_requests_equivalent(&actual, &expected).map_err(|error| {
                format!("request recorded by the real application is incorrect: {error}")
            })?
        }
        (true, None) => return Err("request history is missing the completed request".to_string()),
        (false, Some(actual)) => {
            return Err(format!(
                "request history unexpectedly contains a request: {actual:#?}"
            ));
        }
        (false, None) => {}
    }

    Ok(())
}

fn assert_bearer_editor_contract(cx: &mut VisualTestContext) -> Result<(), String> {
    for selector in [
        "authorization-summary",
        "authorization-kind-selector",
        "authorization-input",
        "authorization-normalized-token",
        "authorization-header-preview",
        "authorization-ready-indicator",
    ] {
        if cx.debug_bounds(selector).is_none() {
            return Err(format!(
                "Bearer design contract element `{selector}` is not rendered"
            ));
        }
    }
    Ok(())
}

fn assert_headers_editor_contract(
    cx: &mut VisualTestContext,
    headers: &[KeyValueSpec],
) -> Result<(), String> {
    if headers.is_empty() {
        return Ok(());
    }

    for selector in [
        "headers-summary",
        "headers-enabled-count",
        "headers-table-header",
        "headers-ready-indicator",
    ] {
        if cx.debug_bounds(selector).is_none() {
            return Err(format!(
                "Headers contract element `{selector}` is not rendered"
            ));
        }
    }

    for (index, _) in headers.iter().enumerate() {
        let selectors = HEADER_ROW_CONTRACT_SELECTORS.get(index).ok_or_else(|| {
            format!(
                "the UI scenario driver supports at most {} Header rows",
                HEADER_ROW_CONTRACT_SELECTORS.len()
            )
        })?;
        for selector in selectors {
            if cx.debug_bounds(selector).is_none() {
                return Err(format!(
                    "Headers row contract element `{selector}` is not rendered"
                ));
            }
        }
    }

    Ok(())
}

fn assert_disabled_headers_absent_from_echo(
    response: &ResponseState,
    headers: &[KeyValueSpec],
) -> Result<(), String> {
    let disabled_headers: Vec<&str> = headers
        .iter()
        .filter(|header| !header.enabled)
        .map(|header| header.key.as_str())
        .collect();
    if disabled_headers.is_empty() {
        return Ok(());
    }

    let ResponseState::Success { body, .. } = response else {
        return Err(
            "cannot verify disabled Headers because the request did not succeed".to_string(),
        );
    };
    let payload: serde_json::Value = serde_json::from_str(body).map_err(|error| {
        format!("cannot verify disabled Headers in the HTTPBingo JSON response: {error}")
    })?;
    let echoed_headers = payload
        .get("headers")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "HTTPBingo response does not contain a `headers` object".to_string())?;

    for disabled_header in disabled_headers {
        if let Some(echoed_name) = echoed_headers
            .keys()
            .find(|name| name.eq_ignore_ascii_case(disabled_header))
        {
            return Err(format!(
                "disabled Header `{disabled_header}` was unexpectedly echoed as `{echoed_name}`"
            ));
        }
    }

    Ok(())
}

fn assert_response_quick_copy(
    cx: &mut VisualTestContext,
    workspace: &Entity<WorkspaceViewModel>,
    response: &ResponseState,
) -> Result<(), String> {
    let ResponseState::Success { body, .. } = response else {
        if cx.debug_bounds("response-copy-button").is_some() {
            return Err("a response without a body exposes the quick-copy action".to_string());
        }
        return Ok(());
    };

    if body.is_empty() {
        if cx.debug_bounds("response-copy-button").is_some() {
            return Err("an empty response body exposes the quick-copy action".to_string());
        }
        return Ok(());
    }
    if cx.debug_bounds("response-copy-button").is_none() {
        return Err("a populated response does not expose the quick-copy action".to_string());
    }

    let state_before_copy = workspace.read_with(cx, |workspace, _| {
        (
            workspace.method(),
            workspace.url().to_string(),
            workspace.params().to_vec(),
            workspace.headers().to_vec(),
            workspace.request_body().clone(),
            workspace.request_pane(),
            workspace.response().clone(),
            workspace.history_len(),
            workspace.active_tab_index(),
            workspace.tab_count(),
        )
    });

    cx.write_to_clipboard(ClipboardItem::new_string(
        "HTTPBingo quick-copy sentinel".to_string(),
    ));
    click(cx, "response-copy-button")?;
    let copied = cx
        .read_from_clipboard()
        .and_then(|item| item.text())
        .unwrap_or_default();
    if copied != *body {
        return Err(format!(
            "quick-copy clipboard mismatch\n  expected: {body:?}\n  actual:   {copied:?}"
        ));
    }
    if cx.debug_bounds("response-copy-feedback").is_none() {
        return Err("quick-copy did not render transient Copied feedback".to_string());
    }

    let state_after_copy = workspace.read_with(cx, |workspace, _| {
        (
            workspace.method(),
            workspace.url().to_string(),
            workspace.params().to_vec(),
            workspace.headers().to_vec(),
            workspace.request_body().clone(),
            workspace.request_pane(),
            workspace.response().clone(),
            workspace.history_len(),
            workspace.active_tab_index(),
            workspace.tab_count(),
        )
    });
    if state_after_copy != state_before_copy {
        return Err("quick-copy mutated request, response, history, or active tabs".to_string());
    }

    Ok(())
}

fn apply_rows(
    cx: &mut VisualTestContext,
    workspace: &Entity<WorkspaceViewModel>,
    editor: RowEditor,
    rows: &[KeyValueSpec],
) -> Result<(), String> {
    if rows.is_empty() {
        return Ok(());
    }

    click(
        cx,
        match editor {
            RowEditor::Params => "request-pane-params",
            RowEditor::Headers => "request-pane-headers",
        },
    )?;

    for (index, row) in rows.iter().enumerate() {
        type_into(cx, "row-key-input", &row.key)?;
        type_into(cx, "row-value-input", &row.value)?;

        // Keep the final enabled row active so the scenario verifies that Send consumes the
        // live ViewModel draft without relying on Add or focus loss. Earlier rows still use Add
        // to open the next editor row; disabled rows must be committed before they can be toggled.
        let must_commit = index + 1 < rows.len() || !row.enabled;
        if must_commit {
            click(cx, "add-row-button")?;
        }

        if !row.enabled {
            let index = workspace
                .read_with(cx, |workspace, _| rows_for(workspace, editor))
                .iter()
                .position(|actual| row_matches(editor, actual, &row.key))
                .ok_or_else(|| format!("row `{}` was not added to the application", row.key))?;
            click(cx, row_toggle_selector(editor, index)?)?;
        }
    }

    Ok(())
}

fn apply_precreated_param_rows(
    cx: &mut VisualTestContext,
    workspace: &Entity<WorkspaceViewModel>,
    url_row_count: usize,
    row_count: usize,
    rows: &[KeyValueSpec],
) -> Result<(), String> {
    if rows.len() > row_count {
        return Err(format!(
            "scenario defines {} Params rows but precreates only {row_count}",
            rows.len()
        ));
    }

    click(cx, "request-pane-params")?;
    for _ in 0..row_count {
        click(cx, "add-row-button")?;
        cx.run_until_parked();
    }
    scroll_up(cx, "params-rows-scroll", 1000.0)?;

    for (offset, row) in rows.iter().enumerate() {
        if offset > 0 {
            scroll_down(cx, "params-rows-scroll", 90.0)?;
        }
        let index = url_row_count + offset;
        let key_selector = PARAM_KEY_SELECTORS.get(index).copied().ok_or_else(|| {
            format!(
                "the UI scenario driver supports at most {} Params rows",
                PARAM_KEY_SELECTORS.len()
            )
        })?;
        let value_selector = PARAM_VALUE_SELECTORS.get(index).copied().ok_or_else(|| {
            format!(
                "the UI scenario driver supports at most {} Params rows",
                PARAM_VALUE_SELECTORS.len()
            )
        })?;
        type_into(cx, key_selector, &row.key)?;
        type_into(cx, value_selector, &row.value)?;
        if !row.enabled {
            click(cx, row_toggle_selector(RowEditor::Params, index)?)?;
        }
    }

    let actual = workspace.read_with(cx, |workspace, _| workspace.params().to_vec());
    for (offset, expected) in rows.iter().enumerate() {
        let index = url_row_count + offset;
        let Some(row) = actual.get(index) else {
            return Err(format!("precreated Params row {index} disappeared"));
        };
        if row.key != expected.key || row.value != expected.value || row.enabled != expected.enabled
        {
            return Err(format!(
                "precreated Params row {index} mismatch\n  expected: {expected:#?}\n  actual:   {row:#?}"
            ));
        }
    }
    Ok(())
}

fn apply_precreated_header_rows(
    cx: &mut VisualTestContext,
    workspace: &Entity<WorkspaceViewModel>,
    row_count: usize,
    rows: &[KeyValueSpec],
) -> Result<(), String> {
    if rows.len() > row_count {
        return Err(format!(
            "scenario defines {} Headers rows but precreates only {row_count}",
            rows.len()
        ));
    }

    click(cx, "request-pane-headers")?;
    for _ in 0..row_count {
        click(cx, "add-row-button")?;
        cx.run_until_parked();
    }
    let expected_visible_rows = row_count + 1;
    let actual_visible_rows =
        workspace.read_with(cx, |workspace, _| workspace.visible_header_row_count());
    if actual_visible_rows != expected_visible_rows {
        return Err(format!(
            "Add Header did not append one row per click: expected {expected_visible_rows}, got {actual_visible_rows}"
        ));
    }
    for index in 0..expected_visible_rows {
        let selectors = HEADER_ROW_CONTRACT_SELECTORS.get(index).ok_or_else(|| {
            format!(
                "the UI scenario driver supports at most {} Header rows",
                HEADER_ROW_CONTRACT_SELECTORS.len()
            )
        })?;
        for selector in selectors {
            if cx.debug_bounds(selector).is_none() {
                return Err(format!(
                    "Header row {index} created by Add is missing `{selector}`"
                ));
            }
        }
    }
    if expected_visible_rows > 4 {
        for selector in [
            "headers-scrollbar",
            "headers-scrollbar-thumb",
            "add-row-button",
        ] {
            if cx.debug_bounds(selector).is_none() {
                return Err(format!(
                    "overflowing Header rows do not render `{selector}`"
                ));
            }
        }
    }

    scroll_up(cx, "headers-rows-scroll", 1000.0)?;
    for (index, row) in rows.iter().enumerate() {
        if index > 0 {
            scroll_down(cx, "headers-rows-scroll", 54.0)?;
        }
        let key_selector = HEADER_KEY_SELECTORS.get(index).copied().ok_or_else(|| {
            format!(
                "the UI scenario driver supports at most {} Header rows",
                HEADER_KEY_SELECTORS.len()
            )
        })?;
        let value_selector = HEADER_VALUE_SELECTORS.get(index).copied().ok_or_else(|| {
            format!(
                "the UI scenario driver supports at most {} Header rows",
                HEADER_VALUE_SELECTORS.len()
            )
        })?;
        type_into(cx, key_selector, &row.key)?;
        type_into(cx, value_selector, &row.value)?;
        if !row.enabled {
            click(cx, row_toggle_selector(RowEditor::Headers, index)?)?;
        }
    }

    let actual = workspace.read_with(cx, |workspace, _| workspace.headers().to_vec());
    for (index, expected) in rows.iter().enumerate() {
        let Some(row) = actual.get(index) else {
            return Err(format!("precreated Header row {index} disappeared"));
        };
        if row.key != expected.key || row.value != expected.value || row.enabled != expected.enabled
        {
            return Err(format!(
                "precreated Header row {index} mismatch\n  expected: {expected:#?}\n  actual:   {row:#?}"
            ));
        }
    }

    // Issue #81 explicitly sends while X-Locale remains active to prove live ViewModel writes.
    if rows.len() > 1 {
        scroll_up(cx, "headers-rows-scroll", 1000.0)?;
        click(cx, HEADER_VALUE_SELECTORS[1])?;
    }
    Ok(())
}

fn rows_for(workspace: &WorkspaceViewModel, editor: RowEditor) -> Vec<KeyValueRow> {
    match editor {
        RowEditor::Params => workspace.params().to_vec(),
        RowEditor::Headers => workspace.headers().to_vec(),
    }
}

fn row_matches(editor: RowEditor, row: &KeyValueRow, expected_key: &str) -> bool {
    match editor {
        RowEditor::Params => row.key == expected_key,
        RowEditor::Headers => row.key.eq_ignore_ascii_case(expected_key),
    }
}

fn row_toggle_selector(editor: RowEditor, index: usize) -> Result<&'static str, String> {
    let selectors = match editor {
        RowEditor::Params => &PARAM_TOGGLE_SELECTORS,
        RowEditor::Headers => &HEADER_TOGGLE_SELECTORS,
    };
    selectors.get(index).copied().ok_or_else(|| {
        format!(
            "the UI scenario driver supports at most {} rows",
            selectors.len()
        )
    })
}

fn body_kind_selector(value: &str) -> Result<&'static str, String> {
    match value.to_ascii_lowercase().as_str() {
        "json" => Ok("body-kind-json"),
        "url_encoded" => Ok("body-kind-url-encoded"),
        "multipart" => Ok("body-kind-form-data"),
        "none" => Ok("body-kind-none"),
        "raw" => Ok("body-kind-raw"),
        _ => Err(format!("invalid body kind `{value}`")),
    }
}

fn apply_body(cx: &mut VisualTestContext, draft: &DraftSpec) -> Result<(), String> {
    let Some(kind) = draft.body_kind.as_deref() else {
        if draft.body.is_some() {
            return Err("a UI body scenario must declare `body_kind`".to_string());
        }
        return Ok(());
    };

    click(cx, "request-pane-body")?;

    match kind.to_ascii_lowercase().as_str() {
        "none" => {
            click(cx, body_kind_selector(kind)?)?;
            if draft.body.as_deref().is_some_and(|body| !body.is_empty()) {
                return Err("a `none` body cannot contain a payload".to_string());
            }
        }
        "json" | "raw" => {
            click(cx, body_kind_selector(kind)?)?;
            let body = draft
                .body
                .as_deref()
                .ok_or_else(|| format!("`{kind}` body scenario is missing `body`"))?;
            click(cx, "body-input")?;
            cx.simulate_keystrokes("cmd-a");
            cx.simulate_input(body);
        }
        "url_encoded" | "multipart" => {
            // POST starts with a sample JSON body. Clear it through the same body-kind controls a
            // user sees, then select the key/value editor.
            click(cx, "body-kind-none")?;
            click(cx, body_kind_selector(kind)?)?;
            let body = draft
                .body
                .as_deref()
                .ok_or_else(|| format!("`{kind}` body scenario is missing `body`"))?;
            type_form_rows(cx, body)?;
        }
        _ => return Err(format!("invalid body kind `{kind}`")),
    }

    Ok(())
}

fn type_form_rows(cx: &mut VisualTestContext, encoded: &str) -> Result<(), String> {
    let rows: Vec<_> = form_urlencoded::parse(encoded.as_bytes()).collect();
    for (index, (key, value)) in rows.iter().enumerate() {
        if index > 0 {
            click(cx, "body-form-add-row")?;
        }
        let key_selector = BODY_FORM_KEY_SELECTORS
            .get(index)
            .copied()
            .ok_or_else(|| "the UI body driver supports at most 16 fields".to_string())?;
        let value_selector = BODY_FORM_VALUE_SELECTORS[index];
        type_into(cx, key_selector, key)?;
        type_into(cx, value_selector, value)?;
    }
    Ok(())
}

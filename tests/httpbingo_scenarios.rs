//! Opt-in application scenarios that drive a real GPUI window against HTTPBingo.

mod common;
#[path = "common/ui.rs"]
mod ui;

use common::scenario::{
    assert_requests_equivalent, assert_response_state, expected_request, load_suites, DraftSpec,
    KeyValueSpec, RequestScenario, ResponseSpec, ScenarioTarget,
};
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
use postman_gpui::app::{KeyValueRow, PostmanApp, WorkspaceViewModel};
use std::path::{Path, PathBuf};
use ui::{choose_method, click, type_into};

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
    apply_rows(cx, &workspace, RowEditor::Params, &scenario.draft.params)?;
    apply_rows(cx, &workspace, RowEditor::Headers, &scenario.draft.headers)?;

    if scenario.draft.bearer_token.is_some() && scenario.draft.basic_auth.is_some() {
        return Err("`bearer_token` and `basic_auth` are mutually exclusive".to_string());
    }
    if let Some(token) = &scenario.draft.bearer_token {
        click(cx, "request-pane-authorization")?;
        type_into(cx, "authorization-input", token)?;
    }
    if let Some(credentials) = &scenario.draft.basic_auth {
        click(cx, "request-pane-authorization")?;
        click(cx, "auth-kind-basic")?;
        type_into(cx, "basic-auth-username-input", &credentials.username)?;
        type_into(cx, "basic-auth-password-input", &credentials.password)?;
    }

    apply_body(cx, &scenario.draft)?;

    let assembled_url = workspace.read_with(cx, |workspace, _| workspace.url().to_string());
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

    for row in rows {
        type_into(cx, "row-key-input", &row.key)?;
        type_into(cx, "row-value-input", &row.value)?;
        click(cx, "add-row-button")?;

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
        cx.simulate_keystrokes("enter");
    }
    Ok(())
}

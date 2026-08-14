//! Opt-in application scenarios that drive a real GPUI window against HTTPBingo.

mod common;

use common::scenario::{
    assert_response_state, expected_request, load_suites, KeyValueSpec, RequestScenario,
    ResponseSpec, ScenarioTarget,
};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use postman_gpui::{
    app::{KeyValueRow, PostmanApp},
    models::HttpMethod,
};
use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

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
    let (app, cx) = test_cx.add_window_view(|_window, cx| PostmanApp::new(cx));

    choose_method(cx, &scenario.draft.method)?;
    type_into(
        cx,
        "url-input",
        &format!("{HTTPBINGO_BASE_URL}{}", scenario.draft.path),
    )?;
    apply_rows(cx, &app, RowEditor::Params, &scenario.draft.params)?;
    apply_rows(cx, &app, RowEditor::Headers, &scenario.draft.headers)?;

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

    if let Some(body) = &scenario.draft.body {
        click(cx, "request-pane-body")?;
        app.update(cx, |app, cx| app.set_body(body, cx));
    }
    if let Some(kind) = &scenario.draft.body_kind {
        click(cx, body_kind_selector(kind)?)?;
    }

    let assembled_url = app.read_with(cx, |app, cx| app.current_url(cx));
    if assembled_url != expected.url {
        return Err(format!(
            "application URL mismatch before Send\n  expected: {:?}\n  actual:   {:?}",
            expected.url, assembled_url
        ));
    }

    click(cx, "send-button")?;

    let response = app.read_with(cx, |app, cx| app.response_state(cx));
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

    let history_len = app.read_with(cx, |app, cx| app.history_len(cx));
    if history_len != scenario.expect.history_len {
        return Err(format!(
            "history length mismatch: expected {}, actual {history_len}",
            scenario.expect.history_len
        ));
    }

    let recorded_request = app.read_with(cx, |app, cx| app.latest_history_request(cx));
    let expected_recorded_request = (scenario.expect.history_len > 0).then_some(expected);
    if recorded_request != expected_recorded_request {
        return Err(format!(
            "request recorded by the real application does not match the scenario\n  expected: {expected_recorded_request:#?}\n  actual:   {recorded_request:#?}"
        ));
    }

    Ok(())
}

fn choose_method(cx: &mut VisualTestContext, value: &str) -> Result<(), String> {
    let method = HttpMethod::from_str(value)
        .map_err(|error| format!("invalid scenario method `{value}`: {error}"))?;
    let selector = match method {
        HttpMethod::GET => "method-option-get",
        HttpMethod::POST => "method-option-post",
        HttpMethod::PUT => "method-option-put",
        HttpMethod::DELETE => "method-option-delete",
        HttpMethod::PATCH => "method-option-patch",
        HttpMethod::HEAD => "method-option-head",
        HttpMethod::OPTIONS => "method-option-options",
    };
    click(cx, "method-dropdown-button")?;
    click(cx, selector)
}

fn apply_rows(
    cx: &mut VisualTestContext,
    app: &Entity<PostmanApp>,
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
            let index = app
                .read_with(cx, |app, cx| rows_for(app, editor, cx))
                .iter()
                .position(|actual| row_matches(editor, actual, &row.key))
                .ok_or_else(|| format!("row `{}` was not added to the application", row.key))?;
            click(cx, row_toggle_selector(editor, index)?)?;
        }
    }

    Ok(())
}

fn rows_for(app: &PostmanApp, editor: RowEditor, cx: &gpui::App) -> Vec<KeyValueRow> {
    match editor {
        RowEditor::Params => app.current_params(cx),
        RowEditor::Headers => app.current_headers(cx),
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
        "form_data" => Ok("body-kind-form-data"),
        "raw" => Ok("body-kind-raw"),
        _ => Err(format!("invalid body kind `{value}`")),
    }
}

fn type_into(
    cx: &mut VisualTestContext,
    selector: &'static str,
    value: &str,
) -> Result<(), String> {
    click(cx, selector)?;
    cx.simulate_input(value);
    Ok(())
}

fn click(cx: &mut VisualTestContext, selector: &'static str) -> Result<(), String> {
    let bounds = cx
        .debug_bounds(selector)
        .ok_or_else(|| format!("application control `{selector}` is not rendered"))?;
    cx.simulate_click(bounds.center(), Modifiers::none());
    Ok(())
}

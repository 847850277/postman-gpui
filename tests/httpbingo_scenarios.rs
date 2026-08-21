//! Opt-in application scenarios that drive a real GPUI window against HTTPBingo.

mod common;
#[path = "common/ui.rs"]
mod ui;

use common::scenario::{
    assert_requests_equivalent, assert_response_state, expected_editor_intent, expected_request,
    load_suites, resolve_scenario_fixture_path, validate_body_row_contract, DraftSpec,
    KeyValueSpec, MultipartPartSpec, RequestScenario, ResponseSpec, ScenarioFile, ScenarioTarget,
};
use gpui::{AppContext, ClipboardItem, Entity, TestAppContext, VisualTestContext};
use postman_gpui::app::{
    AuthorizationKind, BodyKind, KeyValueRow, PostmanApp, ResponseState, WorkspaceViewModel,
};
use postman_gpui::models::RequestBody;
use std::path::{Path, PathBuf};
use ui::{choose_method, click, click_without_wait, scroll_down, scroll_up, type_into};

const HTTPBINGO_BASE_URL: &str = "https://httpbingo.org";
const HTML_FORM_DISCOVERY_SCENARIO: &str =
    "HTTPBingo serves the HTML form that submits to POST /post";
const HTML_FORM_SUBMISSION_SCENARIO: &str =
    "HTTPBingo receives the HTML form submission at POST /post";
const COOKIE_SET_SCENARIO: &str = "HTTPBingo stores a session cookie through the followed redirect";
const COOKIE_CLEARED_SCENARIO: &str =
    "HTTPBingo returns an empty cookie echo after the application jar is cleared";
const DELAY_COMPLETED_SCENARIO: &str = "HTTPBingo completes a delayed request before any deadline";
const DELAY_CANCELLED_SCENARIO: &str = "HTTPBingo delayed request is cancelled by the user";
const DELAY_TIMEOUT_SCENARIO: &str = "HTTPBingo delayed request reaches its configured timeout";
const BODY_FORM_MAX_VISIBLE_ROWS: usize = 6;
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
const BODY_FORM_ROW_SELECTORS: [&str; 16] = [
    "body-form-row-0",
    "body-form-row-1",
    "body-form-row-2",
    "body-form-row-3",
    "body-form-row-4",
    "body-form-row-5",
    "body-form-row-6",
    "body-form-row-7",
    "body-form-row-8",
    "body-form-row-9",
    "body-form-row-10",
    "body-form-row-11",
    "body-form-row-12",
    "body-form-row-13",
    "body-form-row-14",
    "body-form-row-15",
];
const BODY_FORM_TOGGLE_SELECTORS: [&str; 16] = [
    "body-form-toggle-0",
    "body-form-toggle-1",
    "body-form-toggle-2",
    "body-form-toggle-3",
    "body-form-toggle-4",
    "body-form-toggle-5",
    "body-form-toggle-6",
    "body-form-toggle-7",
    "body-form-toggle-8",
    "body-form-toggle-9",
    "body-form-toggle-10",
    "body-form-toggle-11",
    "body-form-toggle-12",
    "body-form-toggle-13",
    "body-form-toggle-14",
    "body-form-toggle-15",
];
const BODY_FORM_DELETE_SELECTORS: [&str; 16] = [
    "body-form-delete-0",
    "body-form-delete-1",
    "body-form-delete-2",
    "body-form-delete-3",
    "body-form-delete-4",
    "body-form-delete-5",
    "body-form-delete-6",
    "body-form-delete-7",
    "body-form-delete-8",
    "body-form-delete-9",
    "body-form-delete-10",
    "body-form-delete-11",
    "body-form-delete-12",
    "body-form-delete-13",
    "body-form-delete-14",
    "body-form-delete-15",
];
const BODY_FORM_TYPE_SELECTORS: [&str; 16] = [
    "body-form-type-0",
    "body-form-type-1",
    "body-form-type-2",
    "body-form-type-3",
    "body-form-type-4",
    "body-form-type-5",
    "body-form-type-6",
    "body-form-type-7",
    "body-form-type-8",
    "body-form-type-9",
    "body-form-type-10",
    "body-form-type-11",
    "body-form-type-12",
    "body-form-type-13",
    "body-form-type-14",
    "body-form-type-15",
];
const BODY_FORM_FILE_SELECTORS: [&str; 16] = [
    "body-form-file-0",
    "body-form-file-1",
    "body-form-file-2",
    "body-form-file-3",
    "body-form-file-4",
    "body-form-file-5",
    "body-form-file-6",
    "body-form-file-7",
    "body-form-file-8",
    "body-form-file-9",
    "body-form-file-10",
    "body-form-file-11",
    "body-form-file-12",
    "body-form-file-13",
    "body-form-file-14",
    "body-form-file-15",
];
const BODY_FORM_FILE_NAME_SELECTORS: [&str; 16] = [
    "body-form-file-name-0",
    "body-form-file-name-1",
    "body-form-file-name-2",
    "body-form-file-name-3",
    "body-form-file-name-4",
    "body-form-file-name-5",
    "body-form-file-name-6",
    "body-form-file-name-7",
    "body-form-file-name-8",
    "body-form-file-name-9",
    "body-form-file-name-10",
    "body-form-file-name-11",
    "body-form-file-name-12",
    "body-form-file-name-13",
    "body-form-file-name-14",
    "body-form-file-name-15",
];
const BODY_FORM_FILE_METADATA_SELECTORS: [&str; 16] = [
    "body-form-file-metadata-0",
    "body-form-file-metadata-1",
    "body-form-file-metadata-2",
    "body-form-file-metadata-3",
    "body-form-file-metadata-4",
    "body-form-file-metadata-5",
    "body-form-file-metadata-6",
    "body-form-file-metadata-7",
    "body-form-file-metadata-8",
    "body-form-file-metadata-9",
    "body-form-file-metadata-10",
    "body-form-file-metadata-11",
    "body-form-file-metadata-12",
    "body-form-file-metadata-13",
    "body-form-file-metadata-14",
    "body-form-file-metadata-15",
];
const BODY_FORM_STATE_SELECTORS: [&str; 16] = [
    "body-form-state-0",
    "body-form-state-1",
    "body-form-state-2",
    "body-form-state-3",
    "body-form-state-4",
    "body-form-state-5",
    "body-form-state-6",
    "body-form-state-7",
    "body-form-state-8",
    "body-form-state-9",
    "body-form-state-10",
    "body-form-state-11",
    "body-form-state-12",
    "body-form-state-13",
    "body-form-state-14",
    "body-form-state-15",
];
const BODY_FORM_READY_SELECTORS: [&str; 16] = [
    "body-form-ready-0",
    "body-form-ready-1",
    "body-form-ready-2",
    "body-form-ready-3",
    "body-form-ready-4",
    "body-form-ready-5",
    "body-form-ready-6",
    "body-form-ready-7",
    "body-form-ready-8",
    "body-form-ready-9",
    "body-form-ready-10",
    "body-form-ready-11",
    "body-form-ready-12",
    "body-form-ready-13",
    "body-form-ready-14",
    "body-form-ready-15",
];
const BODY_FORM_OMITTED_SELECTORS: [&str; 16] = [
    "body-form-omitted-0",
    "body-form-omitted-1",
    "body-form-omitted-2",
    "body-form-omitted-3",
    "body-form-omitted-4",
    "body-form-omitted-5",
    "body-form-omitted-6",
    "body-form-omitted-7",
    "body-form-omitted-8",
    "body-form-omitted-9",
    "body-form-omitted-10",
    "body-form-omitted-11",
    "body-form-omitted-12",
    "body-form-omitted-13",
    "body-form-omitted-14",
    "body-form-omitted-15",
];

#[derive(Clone, Copy)]
enum RowEditor {
    Params,
    Headers,
}

struct HtmlFormWorkflow<'a> {
    discovery: &'a RequestScenario,
    submission: &'a RequestScenario,
}

struct CookieWorkflow<'a> {
    set: &'a RequestScenario,
    cleared: &'a RequestScenario,
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

fn find_httpbingo_scenario<'a>(
    files: &'a [ScenarioFile],
    name: &str,
) -> Result<&'a RequestScenario, String> {
    let mut matches = files
        .iter()
        .filter(|file| file.suite.target == ScenarioTarget::Httpbingo)
        .flat_map(|file| &file.suite.cases)
        .filter(|scenario| scenario.name == name);
    let scenario = matches
        .next()
        .ok_or_else(|| format!("HTTPBingo workflow scenario `{name}` is missing"))?;
    if matches.next().is_some() {
        return Err(format!(
            "HTTPBingo workflow scenario `{name}` is defined more than once"
        ));
    }
    Ok(scenario)
}

fn html_form_workflow(files: &[ScenarioFile]) -> Result<HtmlFormWorkflow<'_>, String> {
    let workflow = HtmlFormWorkflow {
        discovery: find_httpbingo_scenario(files, HTML_FORM_DISCOVERY_SCENARIO)?,
        submission: find_httpbingo_scenario(files, HTML_FORM_SUBMISSION_SCENARIO)?,
    };
    validate_html_form_workflow_contract(&workflow)?;
    Ok(workflow)
}

fn validate_html_form_workflow_contract(workflow: &HtmlFormWorkflow<'_>) -> Result<(), String> {
    let discovery = workflow.discovery;
    if !discovery.draft.method.eq_ignore_ascii_case("GET")
        || discovery.draft.path != "/forms/post"
        || discovery.expect.request.path != "/forms/post"
    {
        return Err("HTML form discovery must GET `/forms/post`".to_string());
    }
    let ResponseSpec::Success {
        status,
        body_contains,
        ..
    } = &discovery.expect.response
    else {
        return Err("HTML form discovery must expect a successful response".to_string());
    };
    if *status != 200 || body_contains.as_deref() != Some("<form method=\"post\" action=\"/post\">")
    {
        return Err("HTML form discovery must assert the POST form action".to_string());
    }

    let submission = workflow.submission;
    if !submission.draft.method.eq_ignore_ascii_case("POST")
        || submission.draft.path != "/post"
        || submission.expect.request.path != "/post"
        || !submission
            .draft
            .body_kind
            .as_deref()
            .is_some_and(|kind| kind.eq_ignore_ascii_case("url_encoded"))
    {
        return Err("HTML form submission must URL-encode a POST to `/post`".to_string());
    }
    let encoded_body = submission
        .draft
        .body
        .as_deref()
        .ok_or_else(|| "HTML form submission is missing its encoded body".to_string())?;
    if submission.expect.request.body.as_deref() != Some(encoded_body) {
        return Err("HTML form draft and expected request body differ".to_string());
    }
    let actual_fields = form_urlencoded::parse(encoded_body.as_bytes())
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    let expected_fields = [
        ("custname", "Ada Lovelace"),
        ("custtel", "+86 123456"),
        ("custemail", "ada@example.com"),
        ("size", "large"),
        ("topping", "bacon"),
        ("topping", "cheese"),
        ("delivery", "18:30"),
        ("comments", "Ring the bell"),
    ];
    if actual_fields
        != expected_fields
            .into_iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect::<Vec<_>>()
    {
        return Err(format!(
            "HTML form submission fields are incomplete or out of order: {actual_fields:#?}"
        ));
    }
    if !submission
        .expect
        .request
        .headers
        .iter()
        .any(|(name, value)| {
            name.eq_ignore_ascii_case("content-type")
                && value == "application/x-www-form-urlencoded"
        })
    {
        return Err("HTML form submission is missing its URL-encoded Content-Type".to_string());
    }
    let ResponseSpec::Success {
        status,
        body_json_contains: Some(expected_echo),
        ..
    } = &submission.expect.response
    else {
        return Err("HTML form submission must expect HTTPBingo's JSON echo".to_string());
    };
    if *status != 200
        || expected_echo
            .get("form")
            .and_then(serde_json::Value::as_object)
            .is_none_or(|form| {
                [
                    "custname",
                    "custtel",
                    "custemail",
                    "size",
                    "topping",
                    "delivery",
                    "comments",
                ]
                .into_iter()
                .any(|field| !form.contains_key(field))
            })
    {
        return Err("HTML form submission must assert every echoed form field".to_string());
    }
    Ok(())
}

fn cookie_workflow(files: &[ScenarioFile]) -> Result<CookieWorkflow<'_>, String> {
    let workflow = CookieWorkflow {
        set: find_httpbingo_scenario(files, COOKIE_SET_SCENARIO)?,
        cleared: find_httpbingo_scenario(files, COOKIE_CLEARED_SCENARIO)?,
    };
    validate_cookie_workflow_contract(&workflow)?;
    Ok(workflow)
}

fn validate_cookie_workflow_contract(workflow: &CookieWorkflow<'_>) -> Result<(), String> {
    if !workflow.set.draft.method.eq_ignore_ascii_case("GET")
        || workflow.set.draft.path != "/cookies/set?session=cookie-e2e-demo"
        || workflow.set.expect.request.path != workflow.set.draft.path
        || workflow.set.expect.request.body.is_some()
        || !workflow.set.expect.request.headers.is_empty()
    {
        return Err(
            "cookie setup must author a bodyless GET /cookies/set with no Cookie header"
                .to_string(),
        );
    }
    let ResponseSpec::Success {
        status,
        body_json_contains: Some(expected_echo),
        ..
    } = &workflow.set.expect.response
    else {
        return Err("cookie setup must expect HTTPBingo's successful cookie echo".to_string());
    };
    if *status != 200
        || expected_echo
            .pointer("/cookies/session")
            .and_then(serde_json::Value::as_str)
            != Some("cookie-e2e-demo")
    {
        return Err("cookie setup must assert the stable session cookie echo".to_string());
    }

    if !workflow.cleared.draft.method.eq_ignore_ascii_case("GET")
        || workflow.cleared.draft.path != "/cookies"
        || workflow.cleared.expect.request.path != workflow.cleared.draft.path
        || workflow.cleared.expect.request.body.is_some()
        || !workflow.cleared.expect.request.headers.is_empty()
    {
        return Err(
            "cleared-cookie verification must author a bodyless GET /cookies with no Cookie header"
                .to_string(),
        );
    }
    let ResponseSpec::Success {
        status,
        body_json_contains: Some(expected_echo),
        ..
    } = &workflow.cleared.expect.response
    else {
        return Err("cleared-cookie verification must expect a successful empty echo".to_string());
    };
    if *status != 200
        || expected_echo
            .get("cookies")
            .and_then(serde_json::Value::as_object)
            .is_none_or(|cookies| !cookies.is_empty())
    {
        return Err("cleared-cookie verification must assert an empty cookies object".to_string());
    }
    Ok(())
}

#[test]
fn html_form_workflow_contract_links_discovery_to_complete_submission() {
    let files = load_suites(&scenario_root()).expect("scenario files should parse");
    html_form_workflow(&files).expect("Issue #59 workflow contract should be complete");
}

#[test]
fn cookie_workflow_contract_links_storage_send_and_clear_in_one_session() {
    let files = load_suites(&scenario_root()).expect("scenario files should parse");
    cookie_workflow(&files).expect("Issue #65 workflow contract should be complete");
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

#[gpui::test]
#[ignore = "requires public HTTPBingo network access"]
fn httpbingo_html_form_is_inspected_then_submitted_in_one_ui_lifecycle(
    test_cx: &mut TestAppContext,
) {
    let files = load_suites(&scenario_root()).expect("scenario files should parse");
    let workflow = html_form_workflow(&files).expect("Issue #59 workflow should be valid");
    run_html_form_workflow(test_cx, &workflow)
        .unwrap_or_else(|failure| panic!("Issue #59 HTML form workflow failed:\n{failure}"));
}

#[gpui::test]
#[ignore = "requires public HTTPBingo network access"]
fn httpbingo_cookie_is_stored_sent_and_cleared_in_one_ui_lifecycle(test_cx: &mut TestAppContext) {
    let files = load_suites(&scenario_root()).expect("scenario files should parse");
    let workflow = cookie_workflow(&files).expect("Issue #65 workflow should be valid");
    run_cookie_workflow(test_cx, &workflow)
        .unwrap_or_else(|failure| panic!("Issue #65 cookie workflow failed:\n{failure}"));
}

#[gpui::test]
#[ignore = "requires public HTTPBingo network access"]
fn httpbingo_delayed_requests_exercise_completion_cancellation_and_timeout(
    test_cx: &mut TestAppContext,
) {
    let files = load_suites(&scenario_root()).expect("scenario files should parse");
    for name in [
        DELAY_COMPLETED_SCENARIO,
        DELAY_CANCELLED_SCENARIO,
        DELAY_TIMEOUT_SCENARIO,
    ] {
        let scenario = files
            .iter()
            .filter(|file| file.suite.target == ScenarioTarget::Httpbingo)
            .flat_map(|file| &file.suite.cases)
            .find(|scenario| scenario.name == name)
            .unwrap_or_else(|| panic!("Issue #66 scenario `{name}` should exist"));
        run_application_scenario(test_cx, scenario)
            .unwrap_or_else(|failure| panic!("Issue #66 scenario `{name}` failed:\n{failure}"));
    }
}

fn run_html_form_workflow(
    test_cx: &mut TestAppContext,
    workflow: &HtmlFormWorkflow<'_>,
) -> Result<(), String> {
    let discovery_request =
        expected_request(&workflow.discovery.expect.request, Some(HTTPBINGO_BASE_URL))?;
    let submission_request = expected_request(
        &workflow.submission.expect.request,
        Some(HTTPBINGO_BASE_URL),
    )?;
    let workspace = test_cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        test_cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    choose_method(cx, &workflow.discovery.draft.method)?;
    type_into(
        cx,
        "url-input",
        &format!("{HTTPBINGO_BASE_URL}{}", workflow.discovery.draft.path),
    )?;
    click(cx, "send-button")?;
    cx.run_until_parked();

    let discovery_response = workspace.read_with(cx, |workspace, _| workspace.response().clone());
    assert_response_state(&discovery_response, &workflow.discovery.expect.response)?;
    assert_response_quick_copy(cx, &workspace, &discovery_response)?;
    let (discovery_history_request, discovery_history_status) = workspace
        .read_with(cx, |workspace, _| {
            workspace
                .history()
                .first()
                .map(|entry| (entry.request.clone(), entry.status))
        })
        .ok_or_else(|| "GET /forms/post is missing from History".to_string())?;
    assert_requests_equivalent(&discovery_history_request, &discovery_request)
        .map_err(|error| format!("HTML form discovery History mismatch: {error}"))?;
    if discovery_history_status != Some(200) {
        return Err(format!(
            "HTML form discovery History status mismatch: {discovery_history_status:?}"
        ));
    }

    // Continue in the same PostmanApp, creating the POST request through the visible left-rail
    // action so the GET response and both History entries remain part of one user lifecycle.
    click(cx, "rail-new-request")?;
    let (tab_count, active_tab, response) = workspace.read_with(cx, |workspace, _| {
        (
            workspace.tab_count(),
            workspace.active_tab_index(),
            workspace.response().clone(),
        )
    });
    if tab_count != 2 || active_tab != 1 || !matches!(response, ResponseState::NotSent) {
        return Err(format!(
            "New Request did not create a clean second tab: tabs={tab_count}, active={active_tab}, response={response:?}"
        ));
    }

    choose_method(cx, &workflow.submission.draft.method)?;
    type_into(
        cx,
        "url-input",
        &format!("{HTTPBINGO_BASE_URL}{}", workflow.submission.draft.path),
    )?;
    apply_body(cx, &workflow.submission.draft)?;
    assert_url_encoded_body_editor_contract(cx, &workspace, workflow.submission)?;
    let active_body = workspace.read_with(cx, |workspace, _| workspace.request_body().clone());
    if active_body != submission_request.body {
        return Err(format!(
            "active comments edit was not saved before Send\n  expected: {:?}\n  actual:   {active_body:?}",
            submission_request.body
        ));
    }

    // `apply_body` leaves the final comments value active. Sending immediately proves that the
    // visible value reaches the shared ViewModel without Enter, Tab, blur, or an extra Add action.
    click(cx, "send-button")?;
    cx.run_until_parked();

    let submission_response = workspace.read_with(cx, |workspace, _| workspace.response().clone());
    assert_response_state(&submission_response, &workflow.submission.expect.response)?;
    assert_response_quick_copy(cx, &workspace, &submission_response)?;
    let history = workspace.read_with(cx, |workspace, _| {
        workspace
            .history()
            .iter()
            .map(|entry| (entry.request.clone(), entry.status))
            .collect::<Vec<_>>()
    });
    if history.len() != 2 {
        return Err(format!(
            "HTML form lifecycle should create two History entries, found {}",
            history.len()
        ));
    }
    assert_requests_equivalent(&history[0].0, &submission_request)
        .map_err(|error| format!("HTML form submission History mismatch: {error}"))?;
    assert_requests_equivalent(&history[1].0, &discovery_request)
        .map_err(|error| format!("HTML form discovery History changed: {error}"))?;
    if history[0].1 != Some(200) || history[1].1 != Some(200) {
        return Err(format!(
            "HTML form lifecycle History statuses are incomplete: {:?}",
            history
                .iter()
                .map(|(_, status)| *status)
                .collect::<Vec<_>>()
        ));
    }
    for selector in ["history-method-0", "history-method-1"] {
        if cx.debug_bounds(selector).is_none() {
            return Err(format!(
                "HTML form lifecycle History entry `{selector}` is not rendered"
            ));
        }
    }

    click(cx, "request-tab-0")?;
    let restored_discovery = workspace.read_with(cx, |workspace, _| workspace.response().clone());
    assert_response_state(&restored_discovery, &workflow.discovery.expect.response)?;
    click(cx, "request-tab-1")?;
    let restored_submission = workspace.read_with(cx, |workspace, _| workspace.response().clone());
    assert_response_state(&restored_submission, &workflow.submission.expect.response)?;

    Ok(())
}

fn run_cookie_workflow(
    test_cx: &mut TestAppContext,
    workflow: &CookieWorkflow<'_>,
) -> Result<(), String> {
    let set_request = expected_request(&workflow.set.expect.request, Some(HTTPBINGO_BASE_URL))?;
    let cookies_request =
        expected_request(&workflow.cleared.expect.request, Some(HTTPBINGO_BASE_URL))?;
    let workspace = test_cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        test_cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    let set_url = format!("{HTTPBINGO_BASE_URL}{}", workflow.set.draft.path);
    type_into(cx, "url-input", &set_url)?;
    let active_set_url = workspace.read_with(cx, |workspace, _| workspace.url().to_string());
    if active_set_url != set_url {
        return Err(format!(
            "active cookie-setting URL was not saved before Send\n  expected: {set_url:?}\n  actual:   {active_set_url:?}"
        ));
    }
    click(cx, "send-button")?;
    cx.run_until_parked();

    let set_response = workspace.read_with(cx, |workspace, _| workspace.response().clone());
    assert_response_state(&set_response, &workflow.set.expect.response)?;
    assert_response_quick_copy(cx, &workspace, &set_response)?;
    let (cookies, set_history) = workspace.read_with(cx, |workspace, _| {
        (workspace.cookies().to_vec(), workspace.history().to_vec())
    });
    if cookies.len() != 1 || cookies[0].name != "session" || cookies[0].origin != HTTPBINGO_BASE_URL
    {
        return Err(format!(
            "intermediate Set-Cookie was not projected as one protected session cookie: {cookies:#?}"
        ));
    }
    if set_history.len() != 1 || set_history[0].status != Some(200) {
        return Err(format!(
            "cookie-setting request did not create one completed History entry: {set_history:#?}"
        ));
    }
    assert_requests_equivalent(&set_history[0].request, &set_request)
        .map_err(|error| format!("cookie-setting History mismatch: {error}"))?;
    if cx.debug_bounds("request-pane-cookies").is_some() {
        return Err("the application Cookie Jar must not remain a request editor tab".to_string());
    }
    if cx.debug_bounds("cookie-jar-trigger").is_none() {
        return Err("the workspace-level Cookie Jar trigger is not rendered".to_string());
    }
    click(cx, "response-pane-cookies")?;
    for selector in [
        "response-cookies-panel",
        "response-cookie-list",
        "response-cookie-row-0",
        "response-cookie-name-0",
        "response-cookie-storage-0",
        "response-open-cookie-jar",
    ] {
        if cx.debug_bounds(selector).is_none() {
            return Err(format!(
                "response-cookie contract element `{selector}` is not rendered"
            ));
        }
    }
    click(cx, "response-open-cookie-jar")?;
    if cx.debug_bounds("cookie-jar-workspace-overlay").is_none() {
        return Err("Response Open Cookie Jar did not open the workspace surface".to_string());
    }
    click(cx, "cookie-jar-close")?;

    // Keep the same PostmanApp/RequestRunner session but author the verification request through a
    // rendered New Request and the focused URL field.
    click(cx, "rail-new-request")?;
    let cookies_url = format!("{HTTPBINGO_BASE_URL}{}", workflow.cleared.draft.path);
    type_into(cx, "url-input", &cookies_url)?;
    let active_cookies_url = workspace.read_with(cx, |workspace, _| workspace.url().to_string());
    if active_cookies_url != cookies_url {
        return Err(format!(
            "active cookie verification URL was not saved before Send\n  expected: {cookies_url:?}\n  actual:   {active_cookies_url:?}"
        ));
    }
    click(cx, "send-button")?;
    cx.run_until_parked();

    let echoed_response = workspace.read_with(cx, |workspace, _| workspace.response().clone());
    // The setting scenario's stable response subset is also the proof that this separate request
    // automatically sent Cookie: session=cookie-e2e-demo.
    assert_response_state(&echoed_response, &workflow.set.expect.response)?;
    let echoed_history = workspace.read_with(cx, |workspace, _| workspace.history().to_vec());
    if echoed_history.len() != 2 || echoed_history[0].status != Some(200) {
        return Err(format!(
            "automatic-cookie request did not extend History correctly: {echoed_history:#?}"
        ));
    }
    assert_requests_equivalent(&echoed_history[0].request, &cookies_request)
        .map_err(|error| format!("automatic-cookie History mismatch: {error}"))?;
    if !echoed_history[0].request.headers.is_empty() {
        return Err(
            "automatic Cookie must remain transport state instead of leaking into authored History"
                .to_string(),
        );
    }

    if cx.debug_bounds("response-cookies-empty").is_none() {
        return Err("the later /cookies response must expose Cookies (0)".to_string());
    }
    click(cx, "cookie-jar-trigger")?;
    for selector in [
        "cookie-jar-workspace-overlay",
        "cookie-jar-panel",
        "cookie-jar-scope",
        "cookie-jar-count",
        "cookie-jar-clear-all",
        "cookie-row-0",
        "cookie-name-0",
        "cookie-origin-0",
        "cookie-value-protected-0",
    ] {
        if cx.debug_bounds(selector).is_none() {
            return Err(format!(
                "stored-cookie contract element `{selector}` is not rendered"
            ));
        }
    }
    click(cx, "cookie-jar-clear-all")?;
    let (cookie_count, cleared_count, response_after_clear, history_after_clear) = workspace
        .read_with(cx, |workspace, _| {
            (
                workspace.cookie_count(),
                workspace.last_cookie_clear_count(),
                workspace.response().clone(),
                workspace.history().to_vec(),
            )
        });
    if cookie_count != 0 || cleared_count != Some(1) {
        return Err(format!(
            "Clear all did not produce the expected 1 → 0 jar transition: count={cookie_count}, cleared={cleared_count:?}"
        ));
    }
    let history_changed = history_after_clear.len() != echoed_history.len()
        || history_after_clear
            .iter()
            .zip(&echoed_history)
            .any(|(after, before)| {
                after.request != before.request
                    || after.editor_intent != before.editor_intent
                    || after.timestamp != before.timestamp
                    || after.name != before.name
                    || after.status != before.status
                    || after.elapsed_ms != before.elapsed_ms
                    || after.response_size != before.response_size
            });
    if response_after_clear != echoed_response || history_changed {
        return Err("clearing cookies changed the completed ResponseState or History".to_string());
    }
    for selector in ["cookie-jar-empty", "cookie-jar-clear-feedback"] {
        if cx.debug_bounds(selector).is_none() {
            return Err(format!(
                "cleared-cookie contract element `{selector}` is not rendered"
            ));
        }
    }
    if cx.debug_bounds("cookie-row-0").is_some() {
        return Err("a stored cookie row remains rendered after Clear all".to_string());
    }

    click(cx, "cookie-jar-close")?;
    click(cx, "send-button")?;
    cx.run_until_parked();
    let cleared_response = workspace.read_with(cx, |workspace, _| workspace.response().clone());
    assert_response_state(&cleared_response, &workflow.cleared.expect.response)?;
    assert_response_quick_copy(cx, &workspace, &cleared_response)?;
    let (final_cookie_count, final_history) = workspace.read_with(cx, |workspace, _| {
        (workspace.cookie_count(), workspace.history().to_vec())
    });
    if final_cookie_count != 0 || final_history.len() != 3 {
        return Err(format!(
            "after-clear request lifecycle is incomplete: cookies={final_cookie_count}, history={}",
            final_history.len()
        ));
    }
    assert_requests_equivalent(&final_history[0].request, &cookies_request)
        .map_err(|error| format!("after-clear History mismatch: {error}"))?;
    assert_requests_equivalent(&final_history[1].request, &cookies_request)
        .map_err(|error| format!("automatic-cookie History changed: {error}"))?;
    assert_requests_equivalent(&final_history[2].request, &set_request)
        .map_err(|error| format!("cookie-setting History changed: {error}"))?;
    if final_history.iter().any(|entry| entry.status != Some(200)) {
        return Err(format!(
            "cookie lifecycle History contains an incomplete status: {final_history:#?}"
        ));
    }
    for selector in [
        "history-status-200-0",
        "history-status-200-1",
        "history-status-200-2",
    ] {
        if cx.debug_bounds(selector).is_none() {
            return Err(format!(
                "cookie lifecycle History element `{selector}` is not rendered"
            ));
        }
    }

    Ok(())
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
    let selected_method = workspace.read_with(cx, |workspace, _| workspace.method().to_string());
    if selected_method != scenario.draft.method {
        return Err(format!(
            "method selector was not saved to the ViewModel\n  expected: {:?}\n  actual:   {selected_method:?}",
            scenario.draft.method
        ));
    }
    for selector in ["method-dropdown-selected-value", "request-tab-method-0"] {
        if cx.debug_bounds(selector).is_none() {
            return Err(format!(
                "selected request method surface `{selector}` is not rendered"
            ));
        }
    }
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
        assert_basic_auth_editor_contract(cx)?;
        type_into(cx, "basic-auth-username-input", &credentials.username)?;
        let live_username =
            workspace.read_with(cx, |workspace, _| workspace.basic_username().to_string());
        if live_username != credentials.username {
            return Err(format!(
                "active Basic username was not saved to the ViewModel\n  expected: {:?}\n  actual:   {live_username:?}",
                credentials.username
            ));
        }
        type_into(cx, "basic-auth-password-input", &credentials.password)?;
        let (kind, live_password, header_preview) = workspace.read_with(cx, |workspace, _| {
            (
                workspace.authorization_kind(),
                workspace.basic_password().to_string(),
                workspace.authorization_header_preview(),
            )
        });
        if kind != AuthorizationKind::Basic {
            return Err(
                "Basic Auth editor did not select the Basic authorization kind".to_string(),
            );
        }
        if live_password != credentials.password {
            return Err(format!(
                "active Basic password was not saved to the ViewModel\n  expected: {:?}\n  actual:   {live_password:?}",
                credentials.password
            ));
        }
        let expected_authorization = scenario
            .expect
            .request
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("authorization"))
            .map(|(_, value)| format!("Authorization: {value}"));
        if header_preview != expected_authorization {
            return Err(format!(
                "Basic Authorization preview mismatch\n  expected: {expected_authorization:?}\n  actual:   {header_preview:?}"
            ));
        }
    }

    apply_body(cx, &scenario.draft)?;
    assert_json_body_editor_contract(cx, &workspace, scenario)?;
    assert_raw_body_editor_contract(cx, &workspace, scenario)?;
    assert_url_encoded_body_editor_contract(cx, &workspace, scenario)?;
    assert_multipart_body_editor_contract(cx, &workspace, scenario)?;

    if let Some(timeout_ms) = scenario.draft.timeout_ms {
        click(cx, "request-pane-options")?;
        for selector in [
            "request-options-panel",
            "timeout-configuration",
            "request-timeout-input",
            "request-timeout-unit",
            "request-timeout-contract",
            "request-lifecycle-state",
            "request-id-state",
            "request-in-flight-count",
        ] {
            if cx.debug_bounds(selector).is_none() {
                return Err(format!(
                    "request timeout contract element `{selector}` is not rendered"
                ));
            }
        }
        type_into(cx, "request-timeout-input", &timeout_ms.to_string())?;
        let configured_timeout = workspace.read_with(cx, |workspace, _| workspace.timeout_ms());
        if configured_timeout != timeout_ms {
            return Err(format!(
                "timeout input was not saved to the active request\n  expected: {timeout_ms}\n  actual:   {configured_timeout}"
            ));
        }
        let timeout_selector = if timeout_ms == 0 {
            "request-timeout-disabled"
        } else {
            "request-timeout-enabled"
        };
        if cx.debug_bounds(timeout_selector).is_none() {
            return Err(format!(
                "configured timeout surface `{timeout_selector}` is not rendered"
            ));
        }
    }

    let assembled_url = workspace.read_with(cx, |workspace, _| workspace.effective_url());
    if assembled_url != expected.url {
        return Err(format!(
            "application URL mismatch before Send\n  expected: {:?}\n  actual:   {:?}",
            expected.url, assembled_url
        ));
    }

    if matches!(&scenario.expect.response, ResponseSpec::Cancelled) {
        click_without_wait(cx, "send-button")?;
        let (request_id, in_flight, response) = workspace.read_with(cx, |workspace, _| {
            (
                workspace.active_request_id(),
                workspace.in_flight_count(),
                workspace.response().clone(),
            )
        });
        if request_id.is_none() || in_flight != 1 || !matches!(response, ResponseState::Loading) {
            return Err(format!(
                "Send did not expose one stable in-flight request before cancellation: request_id={request_id:?}, in_flight={in_flight}, response={response:?}"
            ));
        }
        for _ in 0..32 {
            if cx.debug_bounds("cancel-send-control").is_some() {
                break;
            }
            if !cx.executor().tick() {
                break;
            }
        }
        for selector in [
            "request-in-flight-id",
            "cancel-send-control",
            "response-loading",
        ] {
            if cx.debug_bounds(selector).is_none() {
                return Err(format!(
                    "in-flight cancellation control `{selector}` is not rendered"
                ));
            }
        }
        click_without_wait(cx, "send-button")?;
    } else {
        click(cx, "send-button")?;
    }
    cx.run_until_parked();

    let response = workspace.read_with(cx, |workspace, _| workspace.response().clone());
    assert_response_state(&response, &scenario.expect.response)?;
    assert_disabled_headers_absent_from_echo(&response, &scenario.draft.headers)?;
    assert_disabled_url_encoded_rows_absent_from_echo(&response, &scenario.draft)?;
    assert_disabled_multipart_parts_absent_from_echo(&response, &scenario.draft)?;
    assert_multipart_transport_echo(&response, &scenario.draft)?;
    assert_response_quick_copy(cx, &workspace, &response)?;

    if cx.debug_bounds("response-container").is_none() {
        return Err("response panel is not rendered in the application window".to_string());
    }
    if matches!(
        &scenario.expect.response,
        ResponseSpec::Success { .. } | ResponseSpec::Error { .. } | ResponseSpec::Cancelled
    ) && cx.debug_bounds("response-content").is_none()
    {
        return Err("response content is not rendered in the application window".to_string());
    }
    match &scenario.expect.response {
        ResponseSpec::Success { status, .. } => {
            if cx.debug_bounds("response-status").is_none() {
                return Err(format!(
                    "completed HTTP status {status} is not rendered in the response header"
                ));
            }
            let exact_status_selector = match *status {
                200 => Some("response-status-200"),
                418 => Some("response-status-418"),
                _ => None,
            };
            if exact_status_selector.is_some_and(|selector| cx.debug_bounds(selector).is_none()) {
                return Err(format!(
                    "exact completed status surface `{}` is not rendered",
                    exact_status_selector.expect("checked selector")
                ));
            }
            if cx.debug_bounds("response-transport-error").is_some() {
                return Err(format!(
                    "completed HTTP status {status} is rendered as a transport failure"
                ));
            }
        }
        ResponseSpec::Error { .. } => {
            let is_timeout = matches!(
                &response,
                ResponseState::Error { message } if message.starts_with("Request timed out after")
            );
            if is_timeout {
                for selector in ["response-timeout-error", "response-timeout-content"] {
                    if cx.debug_bounds(selector).is_none() {
                        return Err(format!(
                            "timeout terminal surface `{selector}` is not rendered"
                        ));
                    }
                }
            } else if cx.debug_bounds("response-transport-error").is_none() {
                return Err("transport failure is not rendered as an error".to_string());
            }
        }
        ResponseSpec::Cancelled => {
            for selector in ["response-cancelled", "response-cancelled-content"] {
                if cx.debug_bounds(selector).is_none() {
                    return Err(format!(
                        "user-cancelled terminal surface `{selector}` is not rendered"
                    ));
                }
            }
            if cx.debug_bounds("response-timeout-error").is_some() {
                return Err("user cancellation is rendered as a timeout".to_string());
            }
        }
    }

    let (terminal_request_id, terminal_in_flight) = workspace.read_with(cx, |workspace, _| {
        (workspace.active_request_id(), workspace.in_flight_count())
    });
    if terminal_request_id.is_some() || terminal_in_flight != 0 {
        return Err(format!(
            "terminal request lifecycle was not cleared: request_id={terminal_request_id:?}, in_flight={terminal_in_flight}"
        ));
    }

    let history_len = workspace.read_with(cx, |workspace, _| workspace.history_len());
    if history_len != scenario.expect.history_len {
        return Err(format!(
            "history length mismatch: expected {}, actual {history_len}",
            scenario.expect.history_len
        ));
    }
    if scenario.expect.history_len > 0 && cx.debug_bounds("history-method-0").is_none() {
        return Err("completed request method is not rendered in History".to_string());
    }

    let recorded_entry =
        workspace.read_with(cx, |workspace, _| workspace.history().first().cloned());
    match (scenario.expect.history_len > 0, recorded_entry.as_ref()) {
        (true, Some(actual)) => {
            assert_requests_equivalent(&actual.request, &expected).map_err(|error| {
                format!("request recorded by the real application is incorrect: {error}")
            })?
        }
        (true, None) => return Err("request history is missing the completed request".to_string()),
        (false, Some(actual)) => {
            return Err(format!(
                "request history unexpectedly contains a request: {:#?}",
                actual.request
            ));
        }
        (false, None) => {}
    }
    if let (Some(entry), ResponseSpec::Success { status, .. }) =
        (recorded_entry.as_ref(), &scenario.expect.response)
    {
        if entry.status != Some(*status) {
            return Err(format!(
                "History status mismatch: expected {status}, actual {:?}",
                entry.status
            ));
        }
        if cx.debug_bounds("history-response-detail-0").is_none() {
            return Err(format!(
                "completed HTTP status {status} is not rendered in History"
            ));
        }
        let exact_history_status_selector = match *status {
            200 => Some("history-status-200-0"),
            418 => Some("history-status-418-0"),
            _ => None,
        };
        if exact_history_status_selector.is_some_and(|selector| cx.debug_bounds(selector).is_none())
        {
            return Err(format!(
                "exact History status surface `{}` is not rendered",
                exact_history_status_selector.expect("checked selector")
            ));
        }
    }
    if let Some(entry) = recorded_entry {
        let expected_intent = expected_editor_intent(&scenario.draft)?;
        if entry.editor_intent != expected_intent {
            return Err(format!(
                "History did not retain the complete editor intent\n  expected: {expected_intent:#?}\n  actual:   {:#?}",
                entry.editor_intent
            ));
        }
    }

    assert_multipart_file_selection_retained(cx, &workspace, scenario)?;

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

fn assert_basic_auth_editor_contract(cx: &mut VisualTestContext) -> Result<(), String> {
    for selector in [
        "authorization-summary",
        "authorization-kind-selector",
        "basic-auth-credentials",
        "basic-auth-username-field",
        "basic-auth-username-input",
        "basic-auth-username-saved",
        "basic-auth-password-field",
        "basic-auth-password-input",
        "basic-auth-password-masked",
        "basic-auth-password-saved",
        "basic-auth-header-preview",
        "basic-auth-projection-note",
        "authorization-ready-indicator",
    ] {
        if cx.debug_bounds(selector).is_none() {
            return Err(format!(
                "Basic Auth design contract element `{selector}` is not rendered"
            ));
        }
    }
    Ok(())
}

fn assert_json_body_editor_contract(
    cx: &mut VisualTestContext,
    workspace: &Entity<WorkspaceViewModel>,
    scenario: &RequestScenario,
) -> Result<(), String> {
    if !scenario
        .draft
        .body_kind
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("json"))
    {
        return Ok(());
    }

    let expected_body = scenario
        .draft
        .body
        .as_deref()
        .ok_or_else(|| "a JSON UI scenario must contain a body".to_string())?;
    let (kind, active_body, effective_headers) = workspace.read_with(cx, |workspace, _| {
        (
            workspace.body_kind(),
            workspace.body().to_string(),
            workspace.effective_headers(),
        )
    });
    if kind != BodyKind::Json || active_body != expected_body {
        return Err(format!(
            "active JSON was not saved directly to the ViewModel\n  expected: {expected_body:?}\n  actual:   {active_body:?}"
        ));
    }

    for selector in [
        "body-kind-selector",
        "body-kind-json",
        "body-live-saved",
        "body-editor-shell",
        "body-input",
        "body-effective-headers",
        "body-effective-header-count",
        "body-source-of-truth",
    ] {
        if cx.debug_bounds(selector).is_none() {
            return Err(format!(
                "JSON Body design contract element `{selector}` is not rendered"
            ));
        }
    }

    for (name, value) in &scenario.expect.request.headers {
        if !effective_headers
            .iter()
            .any(|header| header.name.eq_ignore_ascii_case(name) && header.value == *value)
        {
            return Err(format!(
                "effective JSON header preview is missing `{name}: {value}`"
            ));
        }
        let selector = body_effective_header_selector(name)?;
        if cx.debug_bounds(selector).is_none() {
            return Err(format!(
                "effective JSON header row `{selector}` is not rendered"
            ));
        }
    }

    Ok(())
}

fn assert_raw_body_editor_contract(
    cx: &mut VisualTestContext,
    workspace: &Entity<WorkspaceViewModel>,
    scenario: &RequestScenario,
) -> Result<(), String> {
    if !scenario
        .draft
        .body_kind
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("raw"))
    {
        return Ok(());
    }

    let expected_body = scenario
        .draft
        .body
        .as_deref()
        .ok_or_else(|| "a Raw UI scenario must contain a body".to_string())?;
    let (kind, active_body, request_body, effective_headers) =
        workspace.read_with(cx, |workspace, _| {
            (
                workspace.body_kind(),
                workspace.body(),
                workspace.request_body(),
                workspace.effective_headers(),
            )
        });
    if kind != BodyKind::Raw
        || active_body != expected_body
        || request_body != RequestBody::Raw(expected_body.to_string())
    {
        return Err(format!(
            "active Raw body was not saved directly to the typed ViewModel draft\n  expected: {expected_body:?}\n  actual:   {request_body:?}"
        ));
    }
    if effective_headers
        .iter()
        .any(|header| header.name.eq_ignore_ascii_case("content-type"))
    {
        return Err("Raw Body generated an unexpected Content-Type header".to_string());
    }

    for selector in [
        "body-kind-selector",
        "body-kind-raw",
        "body-raw-live-saved",
        "body-editor-shell",
        "body-input",
        "body-raw-effective-request",
        "body-raw-generated-header-count",
        "body-raw-content-type-state",
        "body-raw-exact-bytes",
        "body-raw-effective-body",
        "body-raw-request-target",
        "body-raw-ready-indicator",
        "body-source-of-truth",
    ] {
        if cx.debug_bounds(selector).is_none() {
            return Err(format!(
                "Raw Body design contract element `{selector}` is not rendered"
            ));
        }
    }
    if cx.debug_bounds("body-sample-json").is_some() {
        return Err("Raw Body must not render the JSON sample action".to_string());
    }

    Ok(())
}

fn assert_url_encoded_body_editor_contract(
    cx: &mut VisualTestContext,
    workspace: &Entity<WorkspaceViewModel>,
    scenario: &RequestScenario,
) -> Result<(), String> {
    if !scenario
        .draft
        .body_kind
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("url_encoded"))
    {
        return Ok(());
    }

    let expected_body = scenario
        .draft
        .body
        .as_deref()
        .ok_or_else(|| "a URL-encoded UI scenario must contain a body".to_string())?;
    let (kind, active_body, effective_headers) = workspace.read_with(cx, |workspace, _| {
        (
            workspace.body_kind(),
            workspace.request_body().clone(),
            workspace.effective_headers(),
        )
    });
    if kind != BodyKind::UrlEncoded
        || active_body != postman_gpui::models::RequestBody::UrlEncoded(expected_body.to_string())
    {
        return Err(format!(
            "active URL-encoded form was not saved directly to the ViewModel\n  expected: {expected_body:?}\n  actual:   {active_body:?}"
        ));
    }

    for selector in [
        "body-kind-selector",
        "body-kind-url-encoded",
        "body-url-encoded-live-saved",
        "body-url-encoded-row-count",
        "body-url-encoded-editor",
        "body-form-table-header",
        "body-form-scroll",
        "body-form-add-row",
        "body-form-add-row-hint",
        "body-url-encoded-effective-request",
        "body-url-encoded-effective-body",
        "body-url-encoded-effective-headers",
        "body-url-encoded-field-count",
        "body-url-encoded-ready-indicator",
    ] {
        if cx.debug_bounds(selector).is_none() {
            return Err(format!(
                "URL-encoded Body design contract element `{selector}` is not rendered"
            ));
        }
    }

    let row_count = if scenario.draft.precreate_body_rows > 0 {
        scenario.draft.precreate_body_rows
    } else if !scenario.draft.body_rows.is_empty() {
        scenario.draft.body_rows.len()
    } else {
        form_urlencoded::parse(expected_body.as_bytes()).count()
    };
    if row_count > BODY_FORM_KEY_SELECTORS.len() {
        return Err("the URL-encoded UI contract supports at most 16 fields".to_string());
    }
    for index in 0..row_count {
        for selector in [
            BODY_FORM_ROW_SELECTORS[index],
            BODY_FORM_TOGGLE_SELECTORS[index],
            BODY_FORM_KEY_SELECTORS[index],
            BODY_FORM_VALUE_SELECTORS[index],
            BODY_FORM_DELETE_SELECTORS[index],
        ] {
            if cx.debug_bounds(selector).is_none() {
                return Err(format!(
                    "URL-encoded Body row contract element `{selector}` is not rendered"
                ));
            }
        }
    }
    if row_count > BODY_FORM_MAX_VISIBLE_ROWS {
        for selector in ["body-form-scrollbar", "body-form-scrollbar-thumb"] {
            if cx.debug_bounds(selector).is_none() {
                return Err(format!(
                    "overflowing URL-encoded Body rows are missing `{selector}`"
                ));
            }
        }
    }

    let rows_viewport = cx
        .debug_bounds("body-form-scroll")
        .ok_or_else(|| "URL-encoded row viewport is not rendered".to_string())?;
    let add_action = cx
        .debug_bounds("body-form-add-row")
        .ok_or_else(|| "URL-encoded Add form field action is not rendered".to_string())?;
    let effective_preview = cx
        .debug_bounds("body-url-encoded-effective-request")
        .ok_or_else(|| "URL-encoded effective request preview is not rendered".to_string())?;
    if add_action.origin.y < rows_viewport.bottom()
        || effective_preview.origin.y < add_action.bottom()
    {
        return Err(
            "URL-encoded Add and effective preview must remain fixed below the row viewport"
                .to_string(),
        );
    }

    for (name, value) in &scenario.expect.request.headers {
        let matching_header_count = effective_headers
            .iter()
            .filter(|header| header.name.eq_ignore_ascii_case(name) && header.value == *value)
            .count();
        if matching_header_count != 1 {
            return Err(format!(
                "effective URL-encoded header preview must contain exactly one `{name}: {value}`, found {matching_header_count}"
            ));
        }
        let selector = body_effective_header_selector(name)?;
        if cx.debug_bounds(selector).is_none() {
            return Err(format!(
                "effective URL-encoded header chip `{selector}` is not rendered"
            ));
        }
    }

    Ok(())
}

fn assert_multipart_body_editor_contract(
    cx: &mut VisualTestContext,
    workspace: &Entity<WorkspaceViewModel>,
    scenario: &RequestScenario,
) -> Result<(), String> {
    if !scenario
        .draft
        .body_kind
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("multipart"))
    {
        return Ok(());
    }

    let expected = expected_request(&scenario.expect.request, Some(HTTPBINGO_BASE_URL))?;
    let RequestBody::Multipart(expected_parts) = &expected.body else {
        return Err("a multipart scenario must expect typed multipart parts".to_string());
    };
    let (kind, active_body, effective_headers) = workspace.read_with(cx, |workspace, _| {
        (
            workspace.body_kind(),
            workspace.request_body(),
            workspace.effective_headers(),
        )
    });
    if kind != BodyKind::Multipart || active_body != expected.body {
        return Err(format!(
            "active multipart rows were not saved directly to the ViewModel\n  expected: {:?}\n  actual:   {active_body:?}",
            expected.body
        ));
    }
    let expected_intent = expected_editor_intent(&scenario.draft)?;
    let active_intent = workspace.read_with(cx, |workspace, _| workspace.request_editor_intent());
    if active_intent != expected_intent {
        return Err(format!(
            "multipart editor intent lost disabled rows or file metadata\n  expected: {expected_intent:#?}\n  actual:   {active_intent:#?}"
        ));
    }
    if effective_headers
        .iter()
        .any(|header| header.name.eq_ignore_ascii_case("content-type"))
    {
        return Err(
            "multipart boundary generation leaked into the ViewModel header projection".to_string(),
        );
    }
    for (name, value) in &expected.headers {
        if !effective_headers
            .iter()
            .any(|header| header.name.eq_ignore_ascii_case(name) && header.value == *value)
        {
            return Err(format!(
                "multipart effective headers are missing `{name}: {value}`"
            ));
        }
    }

    for selector in [
        "body-kind-selector",
        "body-kind-form-data",
        "body-multipart-live-saved",
        "body-multipart-row-count",
        "body-multipart-editor",
        "body-form-table-header",
        "body-form-scroll",
        "body-form-add-row",
        "body-form-add-row-hint",
        "body-multipart-effective-request",
        "body-multipart-effective-parts",
        "body-multipart-part-count",
        "body-multipart-omitted-count",
        "body-multipart-boundary",
        "body-multipart-ready-indicator",
    ] {
        if cx.debug_bounds(selector).is_none() {
            return Err(format!(
                "multipart Body design contract element `{selector}` is not rendered"
            ));
        }
    }

    let row_count = if scenario.draft.precreate_body_rows > 0 {
        scenario.draft.precreate_body_rows
    } else if !scenario.draft.multipart_parts.is_empty() {
        scenario.draft.multipart_parts.len()
    } else if !scenario.draft.body_rows.is_empty() {
        scenario.draft.body_rows.len()
    } else {
        expected_parts.len().max(1)
    };
    if row_count > BODY_FORM_ROW_SELECTORS.len() {
        return Err("the multipart UI contract supports at most 16 fields".to_string());
    }
    for index in 0..row_count {
        let enabled = scenario
            .draft
            .multipart_parts
            .get(index)
            .map(MultipartPartSpec::enabled)
            .or_else(|| scenario.draft.body_rows.get(index).map(|row| row.enabled))
            .unwrap_or(true);
        let value_selector = if scenario
            .draft
            .multipart_parts
            .get(index)
            .is_some_and(|part| matches!(part, MultipartPartSpec::File { .. }))
        {
            BODY_FORM_FILE_SELECTORS[index]
        } else {
            BODY_FORM_VALUE_SELECTORS[index]
        };
        for selector in [
            BODY_FORM_ROW_SELECTORS[index],
            BODY_FORM_TOGGLE_SELECTORS[index],
            BODY_FORM_KEY_SELECTORS[index],
            BODY_FORM_TYPE_SELECTORS[index],
            value_selector,
            BODY_FORM_STATE_SELECTORS[index],
            if enabled {
                BODY_FORM_READY_SELECTORS[index]
            } else {
                BODY_FORM_OMITTED_SELECTORS[index]
            },
            BODY_FORM_DELETE_SELECTORS[index],
        ] {
            if cx.debug_bounds(selector).is_none() {
                return Err(format!(
                    "multipart Body row contract element `{selector}` is not rendered"
                ));
            }
        }
    }
    for (index, part) in scenario.draft.multipart_parts.iter().enumerate() {
        if matches!(part, MultipartPartSpec::File { .. }) {
            for selector in [
                BODY_FORM_FILE_NAME_SELECTORS[index],
                BODY_FORM_FILE_METADATA_SELECTORS[index],
            ] {
                if cx.debug_bounds(selector).is_none() {
                    return Err(format!(
                        "multipart File row {index} does not render `{selector}`"
                    ));
                }
            }
        }
    }

    let rows_viewport = cx
        .debug_bounds("body-form-scroll")
        .ok_or_else(|| "multipart row viewport is not rendered".to_string())?;
    let add_action = cx
        .debug_bounds("body-form-add-row")
        .ok_or_else(|| "multipart Add form field action is not rendered".to_string())?;
    let effective_preview = cx
        .debug_bounds("body-multipart-effective-request")
        .ok_or_else(|| "multipart effective request preview is not rendered".to_string())?;
    if add_action.origin.y < rows_viewport.bottom()
        || effective_preview.origin.y < add_action.bottom()
    {
        return Err(
            "multipart Add and effective preview must remain fixed below the row viewport"
                .to_string(),
        );
    }

    Ok(())
}

fn assert_multipart_file_selection_retained(
    cx: &mut VisualTestContext,
    workspace: &Entity<WorkspaceViewModel>,
    scenario: &RequestScenario,
) -> Result<(), String> {
    if !scenario
        .draft
        .multipart_parts
        .iter()
        .any(|part| matches!(part, MultipartPartSpec::File { .. }))
    {
        return Ok(());
    }

    let expected = expected_request(&scenario.expect.request, Some(HTTPBINGO_BASE_URL))?;
    let active_body = workspace.read_with(cx, |workspace, _| workspace.request_body());
    if active_body != expected.body {
        return Err(format!(
            "selected multipart file was not retained after Send\n  expected: {:?}\n  actual:   {active_body:?}",
            expected.body
        ));
    }

    for (index, part) in scenario.draft.multipart_parts.iter().enumerate() {
        if matches!(part, MultipartPartSpec::File { .. }) {
            for selector in [
                BODY_FORM_FILE_SELECTORS[index],
                BODY_FORM_FILE_NAME_SELECTORS[index],
                BODY_FORM_FILE_METADATA_SELECTORS[index],
            ] {
                if cx.debug_bounds(selector).is_none() {
                    return Err(format!(
                        "multipart File row {index} lost `{selector}` after Send"
                    ));
                }
            }
        }
    }
    Ok(())
}

fn body_effective_header_selector(name: &str) -> Result<&'static str, String> {
    match name.to_ascii_lowercase().as_str() {
        "content-type" => Ok("body-effective-header-content-type"),
        "accept" => Ok("body-effective-header-accept"),
        "x-scenario" => Ok("body-effective-header-x-scenario"),
        _ => Err(format!(
            "the JSON Body UI scenario driver has no stable selector for header `{name}`"
        )),
    }
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

fn assert_disabled_url_encoded_rows_absent_from_echo(
    response: &ResponseState,
    draft: &DraftSpec,
) -> Result<(), String> {
    if !draft
        .body_kind
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("url_encoded"))
    {
        return Ok(());
    }
    let disabled_keys = draft
        .body_rows
        .iter()
        .filter(|row| !row.enabled && !row.key.trim().is_empty())
        .map(|row| row.key.as_str())
        .collect::<Vec<_>>();
    if disabled_keys.is_empty() {
        return Ok(());
    }

    let ResponseState::Success { body, .. } = response else {
        return Err(
            "cannot verify disabled URL-encoded rows because the request did not succeed"
                .to_string(),
        );
    };
    let payload: serde_json::Value = serde_json::from_str(body).map_err(|error| {
        format!("cannot verify disabled URL-encoded rows in HTTPBingo JSON: {error}")
    })?;
    let echoed_form = payload
        .get("form")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "HTTPBingo response does not contain a `form` object".to_string())?;

    for disabled_key in disabled_keys {
        if echoed_form.contains_key(disabled_key) {
            return Err(format!(
                "disabled URL-encoded field `{disabled_key}` was unexpectedly echoed"
            ));
        }
    }
    Ok(())
}

fn assert_disabled_multipart_parts_absent_from_echo(
    response: &ResponseState,
    draft: &DraftSpec,
) -> Result<(), String> {
    let disabled_parts = draft
        .multipart_parts
        .iter()
        .filter(|part| !part.enabled())
        .collect::<Vec<_>>();
    if disabled_parts.is_empty() {
        return Ok(());
    }

    let ResponseState::Success { body, .. } = response else {
        return Err(
            "cannot verify disabled multipart parts because the request did not succeed"
                .to_string(),
        );
    };
    let payload: serde_json::Value = serde_json::from_str(body).map_err(|error| {
        format!("cannot verify disabled multipart parts in HTTPBingo JSON: {error}")
    })?;
    let echoed_form = payload
        .get("form")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "HTTPBingo response does not contain a `form` object".to_string())?;
    let echoed_files = payload
        .get("files")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "HTTPBingo response does not contain a `files` object".to_string())?;

    for part in disabled_parts {
        let echoed = match part {
            MultipartPartSpec::Text { .. } => echoed_form.contains_key(part.name()),
            MultipartPartSpec::File { .. } => echoed_files.contains_key(part.name()),
        };
        if echoed {
            return Err(format!(
                "disabled multipart part `{}` was unexpectedly echoed",
                part.name()
            ));
        }
    }
    Ok(())
}

fn assert_multipart_transport_echo(
    response: &ResponseState,
    draft: &DraftSpec,
) -> Result<(), String> {
    if !draft
        .body_kind
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("multipart"))
    {
        return Ok(());
    }
    let ResponseState::Success { body, .. } = response else {
        return Err("cannot verify multipart transport because Send did not succeed".to_string());
    };
    let payload: serde_json::Value = serde_json::from_str(body)
        .map_err(|error| format!("multipart HTTPBingo response is not valid JSON: {error}"))?;
    let headers = payload
        .get("headers")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "HTTPBingo multipart response has no `headers` object".to_string())?;
    let content_type = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        .and_then(|(_, values)| values.as_array())
        .and_then(|values| values.first())
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "HTTPBingo did not echo the multipart Content-Type".to_string())?;
    let boundary_prefix = "multipart/form-data; boundary=";
    if !content_type
        .to_ascii_lowercase()
        .starts_with(boundary_prefix)
        || content_type.len() <= boundary_prefix.len()
    {
        return Err(format!(
            "HTTPBingo echoed an invalid multipart Content-Type: {content_type:?}"
        ));
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
        "url_encoded" => {
            // POST starts with a sample JSON body. Clear it through the same body-kind controls a
            // user sees, then select the key/value editor.
            click(cx, "body-kind-none")?;
            click(cx, body_kind_selector(kind)?)?;
            if draft.body_rows.is_empty()
                && draft.multipart_parts.is_empty()
                && draft.precreate_body_rows == 0
            {
                let body = draft
                    .body
                    .as_deref()
                    .ok_or_else(|| format!("`{kind}` body scenario is missing `body`"))?;
                type_form_rows(cx, body)?;
            } else {
                type_form_body_rows(cx, draft)?;
            }
        }
        "multipart" => {
            click(cx, "body-kind-none")?;
            click(cx, body_kind_selector(kind)?)?;
            if draft.body_rows.is_empty()
                && draft.multipart_parts.is_empty()
                && draft.precreate_body_rows == 0
            {
                let body = draft
                    .body
                    .as_deref()
                    .ok_or_else(|| format!("`{kind}` body scenario is missing `body`"))?;
                type_form_rows(cx, body)?;
            } else {
                type_form_body_rows(cx, draft)?;
            }
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

fn type_form_body_rows(cx: &mut VisualTestContext, draft: &DraftSpec) -> Result<(), String> {
    validate_body_row_contract(draft)?;
    let row_count = if draft.precreate_body_rows == 0 {
        draft
            .body_rows
            .len()
            .max(draft.multipart_parts.len())
            .max(1)
    } else {
        draft.precreate_body_rows
    };
    if row_count > BODY_FORM_ROW_SELECTORS.len() {
        return Err(format!(
            "the UI body driver supports at most {} form rows",
            BODY_FORM_ROW_SELECTORS.len()
        ));
    }

    for (index, row_selector) in BODY_FORM_ROW_SELECTORS
        .iter()
        .copied()
        .enumerate()
        .take(row_count)
        .skip(1)
    {
        if cx.debug_bounds(row_selector).is_some() {
            return Err(format!(
                "form row {index} existed before its Add form field click"
            ));
        }
        click(cx, "body-form-add-row")?;
        cx.run_until_parked();
        if cx.debug_bounds(row_selector).is_none() {
            return Err(format!("Add form field did not append form row {index}"));
        }
    }
    if row_count < BODY_FORM_ROW_SELECTORS.len()
        && cx
            .debug_bounds(BODY_FORM_ROW_SELECTORS[row_count])
            .is_some()
    {
        return Err(format!(
            "form Add created more than the requested {row_count} rows"
        ));
    }

    for index in 0..row_count {
        for selector in [
            BODY_FORM_ROW_SELECTORS[index],
            BODY_FORM_TOGGLE_SELECTORS[index],
            BODY_FORM_KEY_SELECTORS[index],
            BODY_FORM_VALUE_SELECTORS[index],
            BODY_FORM_DELETE_SELECTORS[index],
        ] {
            if cx.debug_bounds(selector).is_none() {
                return Err(format!(
                    "form row {index} created by Add is missing `{selector}`"
                ));
            }
        }
    }

    if row_count > BODY_FORM_MAX_VISIBLE_ROWS {
        for selector in [
            "body-form-scrollbar",
            "body-form-scrollbar-thumb",
            "body-form-add-row",
            "body-form-add-row-hint",
        ] {
            if cx.debug_bounds(selector).is_none() {
                return Err(format!("overflowing form rows do not render `{selector}`"));
            }
        }
    }

    scroll_up(cx, "body-form-scroll", 1000.0)?;
    if !draft.multipart_parts.is_empty() {
        for (index, part) in draft.multipart_parts.iter().enumerate() {
            if index > 0 {
                scroll_down(cx, "body-form-scroll", 52.0)?;
            }
            type_into(cx, BODY_FORM_KEY_SELECTORS[index], part.name())?;
            match part {
                MultipartPartSpec::Text { value, .. } => {
                    if !value.is_empty() {
                        type_into(cx, BODY_FORM_VALUE_SELECTORS[index], value)?;
                    }
                }
                MultipartPartSpec::File { path, .. } => {
                    click(cx, BODY_FORM_TYPE_SELECTORS[index])?;
                    if cx.debug_bounds(BODY_FORM_FILE_SELECTORS[index]).is_none() {
                        return Err(format!(
                            "multipart row {index} did not change from Text to File"
                        ));
                    }
                    click(cx, BODY_FORM_FILE_SELECTORS[index])?;
                    if !cx.did_prompt_for_paths() {
                        return Err(format!(
                            "multipart File row {index} did not activate the rendered file picker"
                        ));
                    }
                    let selected = resolve_scenario_fixture_path(path)?;
                    cx.simulate_path_prompt_response(move |options| {
                        assert!(options.files);
                        assert!(!options.directories);
                        assert!(!options.multiple);
                        Some(vec![selected])
                    });
                    cx.run_until_parked();
                    for selector in [
                        BODY_FORM_FILE_NAME_SELECTORS[index],
                        BODY_FORM_FILE_METADATA_SELECTORS[index],
                    ] {
                        if cx.debug_bounds(selector).is_none() {
                            return Err(format!(
                                "multipart File row {index} did not render selected metadata `{selector}`"
                            ));
                        }
                    }
                }
            }
            if !part.enabled() {
                click(cx, BODY_FORM_TOGGLE_SELECTORS[index])?;
            }
        }
        return Ok(());
    }

    for (index, row) in draft.body_rows.iter().enumerate() {
        if index > 0 {
            scroll_down(cx, "body-form-scroll", 52.0)?;
        }
        if !row.key.is_empty() {
            type_into(cx, BODY_FORM_KEY_SELECTORS[index], &row.key)?;
        }
        if !row.value.is_empty() {
            type_into(cx, BODY_FORM_VALUE_SELECTORS[index], &row.value)?;
        }
        if !row.enabled {
            click(cx, BODY_FORM_TOGGLE_SELECTORS[index])?;
        }
    }

    Ok(())
}

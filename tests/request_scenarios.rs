//! Executable request specifications discovered recursively under `tests/cases`.

mod common;

use common::scenario::{
    expected_request, load_suite, load_suites, resolve_scenario_fixture_path, run_scenario,
    validate_body_row_contract, ScenarioFile, ScenarioTarget,
};
use postman_gpui::models::{MultipartPart, MultipartValue, RequestBody};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

fn scenario_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases")
}

fn scenario_files() -> Vec<ScenarioFile> {
    load_suites(&scenario_root()).expect("request scenario files should parse")
}

fn source_name(path: &Path) -> String {
    path.strip_prefix(env!("CARGO_MANIFEST_DIR"))
        .unwrap_or(path)
        .display()
        .to_string()
}

#[test]
fn request_scenarios_are_valid_and_unique() {
    let mut names = HashMap::new();
    for file in scenario_files() {
        for scenario in file.suite.cases {
            if let Some(previous) = names.insert(scenario.name.clone(), file.path.clone()) {
                panic!(
                    "duplicate request scenario name `{}` in {} and {}",
                    scenario.name,
                    source_name(&previous),
                    source_name(&file.path)
                );
            }
        }
    }
}

#[test]
fn request_scenarios_reject_unknown_contract_fields() {
    let invalid = r#"
    {
      "schema_version": 5,
      "target": "local",
      "cases": [{
        "name": "a typo must not weaken the contract",
        "draft": { "method": "GET" },
        "expect": {
          "request": { "method": "GET", "path": "", "body": null },
          "response": {
            "kind": "error",
            "contains": "URL",
            "body_contians": "misspelled"
          },
          "history_len": 0
        }
      }]
    }
    "#;

    let error = load_suite(invalid).expect_err("unknown scenario fields must be rejected");
    assert!(
        error.contains("body_contians"),
        "error should identify the unknown field, got: {error}"
    );
}

#[test]
fn multipart_text_scenario_builds_a_typed_request_contract() {
    let files = scenario_files();
    let scenario = files
        .iter()
        .filter(|file| file.suite.target == ScenarioTarget::Httpbingo)
        .flat_map(|file| &file.suite.cases)
        .find(|scenario| {
            scenario.name == "HTTPBingo receives ordered multipart text rows from the active editor"
        })
        .expect("Issue #91 multipart scenario should exist");

    validate_body_row_contract(&scenario.draft)
        .expect("Issue #91 form rows should match their effective body");
    assert_eq!(scenario.draft.precreate_body_rows, 3);
    assert_eq!(scenario.draft.body_rows.len(), 2);
    let expected = expected_request(&scenario.expect.request, Some("https://httpbingo.org"))
        .expect("Issue #91 expected request should be valid");
    assert_eq!(
        expected.body,
        RequestBody::Multipart(vec![
            MultipartPart::text("note", "hello multipart"),
            MultipartPart::text("category", "gpui"),
        ])
    );
    assert!(expected
        .headers
        .iter()
        .all(|(name, _)| !name.eq_ignore_ascii_case("content-type")));
}

#[test]
fn multipart_file_scenario_builds_typed_metadata_and_rejects_path_traversal() {
    let files = scenario_files();
    let scenario = files
        .iter()
        .filter(|file| file.suite.target == ScenarioTarget::Httpbingo)
        .flat_map(|file| &file.suite.cases)
        .find(|scenario| {
            scenario.name
                == "HTTPBingo uploads a repository file through the rendered multipart picker"
        })
        .expect("Issue #92 multipart file scenario should exist");

    validate_body_row_contract(&scenario.draft)
        .expect("Issue #92 typed multipart parts should be valid");
    assert_eq!(scenario.draft.precreate_body_rows, 2);
    assert_eq!(scenario.draft.multipart_parts.len(), 2);
    let fixture = resolve_scenario_fixture_path(Path::new("tests/fixtures/httpbingo-upload.txt"))
        .expect("Issue #92 fixture should resolve inside the repository");
    let expected = expected_request(&scenario.expect.request, Some("https://httpbingo.org"))
        .expect("Issue #92 expected request should be valid");
    assert_eq!(
        expected.body,
        RequestBody::Multipart(vec![
            MultipartPart::text("note", "hello multipart"),
            MultipartPart {
                name: "upload".to_string(),
                value: MultipartValue::File {
                    path: fixture,
                    file_name: Some("httpbingo-upload.txt".to_string()),
                    content_type: Some("text/plain".to_string()),
                },
            },
        ])
    );
    assert!(expected
        .headers
        .iter()
        .all(|(name, _)| !name.eq_ignore_ascii_case("content-type")));

    let traversal = resolve_scenario_fixture_path(Path::new("../Cargo.toml"))
        .expect_err("scenario fixture paths must reject parent-directory traversal");
    assert!(traversal.contains("path traversal"));
}

#[test]
fn local_request_scenarios_define_the_product_contract() {
    let failures: Vec<String> = scenario_files()
        .iter()
        .filter(|file| file.suite.target == ScenarioTarget::Local)
        .flat_map(|file| {
            file.suite.cases.iter().filter_map(|scenario| {
                run_scenario(scenario).err().map(|failure| {
                    format!(
                        "- {} :: {}\n{failure}",
                        source_name(&file.path),
                        scenario.name
                    )
                })
            })
        })
        .collect();

    assert!(
        failures.is_empty(),
        "local request scenario failures:\n\n{}",
        failures.join("\n\n")
    );
}

//! Executable request specifications discovered recursively under `tests/cases`.

mod common;

use common::scenario::{load_suite, load_suites, run_scenario, ScenarioFile, ScenarioTarget};
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
      "schema_version": 3,
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

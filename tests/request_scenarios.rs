//! Executable request specifications loaded from `tests/cases/request_scenarios.json`.

mod common;

use common::scenario::{load_suite, run_scenario};
use std::collections::HashSet;

const SCENARIOS: &str = include_str!("cases/request_scenarios.json");

#[test]
fn request_scenarios_are_valid_and_unique() {
    let suite = load_suite(SCENARIOS).expect("request scenarios should parse");
    let mut names = HashSet::new();
    for scenario in &suite.cases {
        assert!(
            names.insert(scenario.name.as_str()),
            "duplicate request scenario name: {}",
            scenario.name
        );
    }
}

#[test]
fn request_scenarios_reject_unknown_contract_fields() {
    let invalid = r#"
    {
      "schema_version": 2,
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
fn request_scenarios_define_the_product_contract() {
    let suite = load_suite(SCENARIOS).expect("request scenarios should parse");
    let failures: Vec<String> = suite
        .cases
        .iter()
        .filter_map(|scenario| {
            run_scenario(scenario)
                .err()
                .map(|failure| format!("- {}\n{failure}", scenario.name))
        })
        .collect();

    assert!(
        failures.is_empty(),
        "request scenario failures:\n\n{}",
        failures.join("\n\n")
    );
}

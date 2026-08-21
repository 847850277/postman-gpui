//! Executable request specifications discovered recursively under `tests/cases`.

mod common;

use common::scenario::{
    expected_editor_intent, expected_request, load_suite, load_suites,
    resolve_scenario_fixture_path, run_scenario, validate_body_row_contract, ResponseSpec,
    ScenarioFile, ScenarioTarget,
};
use postman_gpui::models::{
    HttpMethod, MultipartEditorPart, MultipartPart, MultipartValue, RequestBody,
    RequestEditorIntent,
};
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
fn raw_put_scenario_requires_exact_body_without_generated_headers() {
    let files = scenario_files();
    let scenario = files
        .iter()
        .filter(|file| file.suite.target == ScenarioTarget::Httpbingo)
        .flat_map(|file| &file.suite.cases)
        .find(|scenario| {
            scenario.name == "HTTPBingo receives a raw PUT body without generated content type"
        })
        .expect("Issue #60 raw PUT scenario should exist");

    assert_eq!(scenario.draft.body_kind.as_deref(), Some("raw"));
    let expected = expected_request(&scenario.expect.request, Some("https://httpbingo.org"))
        .expect("Issue #60 expected request should be valid");
    assert_eq!(expected.method, HttpMethod::PUT);
    assert_eq!(expected.url, "https://httpbingo.org/anything/raw");
    assert_eq!(
        expected.body,
        RequestBody::Raw("plain text body".to_string())
    );
    assert!(expected.headers.is_empty());
}

#[test]
fn head_and_options_scenarios_require_exact_bodyless_methods_and_stable_headers() {
    let files = scenario_files();
    let scenarios = files
        .iter()
        .filter(|file| file.suite.target == ScenarioTarget::Httpbingo)
        .flat_map(|file| &file.suite.cases)
        .collect::<Vec<_>>();
    let head = scenarios
        .iter()
        .copied()
        .find(|scenario| {
            scenario.name == "HTTPBingo receives HEAD and returns headers without a response body"
        })
        .expect("Issue #78 HEAD scenario should exist");
    let options = scenarios
        .iter()
        .copied()
        .find(|scenario| scenario.name == "HTTPBingo receives OPTIONS without rewriting it to GET")
        .expect("Issue #78 OPTIONS scenario should exist");

    for (scenario, method, path) in [
        (head, HttpMethod::HEAD, "/get"),
        (options, HttpMethod::OPTIONS, "/anything/options"),
    ] {
        let request = expected_request(&scenario.expect.request, Some("https://httpbingo.org"))
            .expect("Issue #78 expected request should be valid");
        assert_eq!(request.method, method);
        assert_eq!(request.url, format!("https://httpbingo.org{path}"));
        assert!(request.headers.is_empty());
        assert_eq!(request.body, RequestBody::None);
        assert_eq!(scenario.expect.history_len, 1);
    }

    let ResponseSpec::Success {
        status,
        body_contains,
        body_json_contains,
        headers_contain,
    } = &head.expect.response
    else {
        panic!("HEAD must expect a successful empty-body response");
    };
    assert_eq!(*status, 200);
    assert!(body_contains.is_none() && body_json_contains.is_none());
    assert!(headers_contain.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("content-type") && value == "application/json; charset=utf-8"
    }));

    let ResponseSpec::Success {
        status,
        body_contains,
        body_json_contains,
        headers_contain,
    } = &options.expect.response
    else {
        panic!("OPTIONS must expect a successful empty-body response");
    };
    assert_eq!(*status, 200);
    assert!(body_contains.is_none() && body_json_contains.is_none());
    assert!(headers_contain.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("access-control-allow-methods") && value.contains("OPTIONS")
    }));
}

#[test]
fn compression_scenarios_define_decoded_json_and_provider_capability_contracts() {
    let files = scenario_files();
    let file = files
        .iter()
        .find(|file| {
            file.path
                .ends_with("tests/cases/httpbingo/compression.json")
        })
        .expect("Issue #67 compression scenario file should exist");
    assert_eq!(file.suite.target, ScenarioTarget::Httpbingo);
    assert_eq!(file.suite.cases.len(), 3);

    let cases = [
        (
            "HTTPBingo gzip response decodes into readable JSON",
            "/gzip",
            200,
            serde_json::json!({
                "headers": { "Accept-Encoding": ["gzip,deflate,br"] },
                "method": "GET",
                "gzipped": true
            }),
        ),
        (
            "HTTPBingo deflate response decodes into readable JSON",
            "/deflate",
            200,
            serde_json::json!({
                "headers": { "Accept-Encoding": ["gzip,deflate,br"] },
                "method": "GET",
                "deflated": true
            }),
        ),
        (
            "HTTPBingo reports the current Brotli provider capability",
            "/brotli",
            501,
            serde_json::json!({
                "status_code": 501,
                "error": "Not Implemented"
            }),
        ),
    ];

    for (name, path, expected_status, expected_json) in cases {
        let scenario = file
            .suite
            .cases
            .iter()
            .find(|scenario| scenario.name == name)
            .unwrap_or_else(|| panic!("Issue #67 scenario `{name}` should exist"));
        assert!(scenario.mock.is_none());
        assert_eq!(scenario.draft.method, "GET");
        assert_eq!(scenario.draft.path, path);
        assert_eq!(scenario.draft.body_kind.as_deref(), Some("none"));
        assert!(scenario.draft.body.is_none());
        assert!(scenario.draft.headers.is_empty());

        let expected = expected_request(&scenario.expect.request, Some("https://httpbingo.org"))
            .expect("Issue #67 expected request should be valid");
        assert_eq!(expected.method, HttpMethod::GET);
        assert_eq!(expected.url, format!("https://httpbingo.org{path}"));
        assert!(expected.headers.is_empty());
        assert_eq!(expected.body, RequestBody::None);
        assert_eq!(scenario.expect.history_len, 1);

        let ResponseSpec::Success {
            status,
            body_contains,
            body_json_contains,
            headers_contain,
        } = &scenario.expect.response
        else {
            panic!("Issue #67 scenario `{name}` should be a completed HTTP response");
        };
        assert_eq!(*status, expected_status);
        assert!(body_contains.is_none());
        assert_eq!(body_json_contains.as_ref(), Some(&expected_json));
        assert!(headers_contain.iter().any(|(header_name, value)| {
            header_name.eq_ignore_ascii_case("content-type")
                && value == "application/json; charset=utf-8"
        }));
    }
}

#[test]
fn json_response_scenario_asserts_only_the_stable_nested_subset() {
    let files = scenario_files();
    let scenario = files
        .iter()
        .filter(|file| file.suite.target == ScenarioTarget::Httpbingo)
        .flat_map(|file| &file.suite.cases)
        .find(|scenario| {
            scenario.name == "HTTPBingo JSON response is parsed with stable subset assertions"
        })
        .expect("Issue #63 JSON response scenario should exist");

    let expected = expected_request(&scenario.expect.request, Some("https://httpbingo.org"))
        .expect("Issue #63 expected request should be valid");
    assert_eq!(expected.method, HttpMethod::GET);
    assert_eq!(expected.url, "https://httpbingo.org/json");
    assert!(expected.headers.is_empty());
    assert_eq!(expected.body, RequestBody::None);
    assert_eq!(scenario.expect.history_len, 1);

    let ResponseSpec::Success {
        status,
        body_contains,
        body_json_contains,
        headers_contain,
    } = &scenario.expect.response
    else {
        panic!("Issue #63 must expect a completed HTTP response");
    };
    assert_eq!(*status, 200);
    assert!(body_contains.is_none());
    assert!(headers_contain.is_empty());
    assert_eq!(
        body_json_contains.as_ref(),
        Some(&serde_json::json!({
            "slideshow": {
                "title": "Sample Slide Show"
            }
        })),
        "the public scenario must not snapshot dynamic headers, timestamps, or unrelated JSON fields"
    );
}

#[test]
fn cookie_scenarios_define_the_stable_set_and_cleared_echo_contracts() {
    let files = scenario_files();
    let scenarios = files
        .iter()
        .filter(|file| file.suite.target == ScenarioTarget::Httpbingo)
        .flat_map(|file| &file.suite.cases)
        .collect::<Vec<_>>();
    let stored = scenarios
        .iter()
        .copied()
        .find(|scenario| {
            scenario.name == "HTTPBingo stores a session cookie through the followed redirect"
        })
        .expect("Issue #65 cookie-setting scenario should exist");
    let cleared = scenarios
        .iter()
        .copied()
        .find(|scenario| {
            scenario.name
                == "HTTPBingo returns an empty cookie echo after the application jar is cleared"
        })
        .expect("Issue #65 cleared-cookie scenario should exist");

    let stored_request = expected_request(&stored.expect.request, Some("https://httpbingo.org"))
        .expect("the cookie-setting request should be valid");
    assert_eq!(stored_request.method, HttpMethod::GET);
    assert_eq!(
        stored_request.url,
        "https://httpbingo.org/cookies/set?session=cookie-e2e-demo"
    );
    assert!(stored_request.headers.is_empty());
    assert_eq!(stored_request.body, RequestBody::None);
    assert_eq!(stored.expect.history_len, 1);
    let ResponseSpec::Success {
        status,
        body_json_contains,
        body_contains,
        headers_contain,
    } = &stored.expect.response
    else {
        panic!("the cookie-setting redirect must complete successfully");
    };
    assert_eq!(*status, 200);
    assert_eq!(
        body_json_contains.as_ref(),
        Some(&serde_json::json!({
            "cookies": { "session": "cookie-e2e-demo" }
        }))
    );
    assert!(body_contains.is_none());
    assert!(headers_contain.is_empty());

    let cleared_request = expected_request(&cleared.expect.request, Some("https://httpbingo.org"))
        .expect("the cleared-cookie request should be valid");
    assert_eq!(cleared_request.method, HttpMethod::GET);
    assert_eq!(cleared_request.url, "https://httpbingo.org/cookies");
    assert!(cleared_request.headers.is_empty());
    assert_eq!(cleared_request.body, RequestBody::None);
    assert_eq!(cleared.expect.history_len, 1);
    let ResponseSpec::Success {
        status,
        body_json_contains,
        body_contains,
        headers_contain,
    } = &cleared.expect.response
    else {
        panic!("the after-clear verification must complete successfully");
    };
    assert_eq!(*status, 200);
    assert_eq!(
        body_json_contains.as_ref(),
        Some(&serde_json::json!({ "cookies": {} }))
    );
    assert!(body_contains.is_none());
    assert!(headers_contain.is_empty());
}

#[test]
fn delayed_request_scenarios_keep_completion_cancellation_and_timeout_distinct() {
    let files = scenario_files();
    let scenarios = files
        .iter()
        .filter(|file| file.suite.target == ScenarioTarget::Httpbingo)
        .flat_map(|file| &file.suite.cases)
        .collect::<Vec<_>>();
    let find = |name: &str| {
        scenarios
            .iter()
            .copied()
            .find(|scenario| scenario.name == name)
            .unwrap_or_else(|| panic!("Issue #66 scenario `{name}` should exist"))
    };
    let completed = find("HTTPBingo completes a delayed request before any deadline");
    let cancelled = find("HTTPBingo delayed request is cancelled by the user");
    let timed_out = find("HTTPBingo delayed request reaches its configured timeout");

    assert_eq!(completed.draft.timeout_ms, None);
    assert_eq!(completed.expect.history_len, 1);
    assert!(matches!(
        &completed.expect.response,
        ResponseSpec::Success { status: 200, .. }
    ));
    assert_eq!(cancelled.draft.timeout_ms, None);
    assert_eq!(cancelled.expect.history_len, 0);
    assert!(matches!(
        &cancelled.expect.response,
        ResponseSpec::Cancelled
    ));
    assert_eq!(timed_out.draft.timeout_ms, Some(1_000));
    assert_eq!(timed_out.expect.history_len, 0);
    assert!(matches!(
        &timed_out.expect.response,
        ResponseSpec::Error { contains }
            if contains == "Request timed out after 1,000 ms"
    ));

    for (scenario, path) in [
        (completed, "/delay/1"),
        (cancelled, "/delay/5"),
        (timed_out, "/delay/3"),
    ] {
        let request = expected_request(&scenario.expect.request, Some("https://httpbingo.org"))
            .expect("Issue #66 expected request should be valid");
        assert_eq!(request.method, HttpMethod::GET);
        assert_eq!(request.url, format!("https://httpbingo.org{path}"));
        assert!(request.headers.is_empty());
        assert_eq!(request.body, RequestBody::None);
    }
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
fn disabled_multipart_scenario_separates_effective_request_from_editor_intent() {
    let files = scenario_files();
    let scenario = files
        .iter()
        .filter(|file| file.suite.target == ScenarioTarget::Httpbingo)
        .flat_map(|file| &file.suite.cases)
        .find(|scenario| {
            scenario.name
                == "HTTPBingo omits disabled multipart Text and File rows without losing editor intent"
        })
        .expect("Issue #93 disabled multipart scenario should exist");
    validate_body_row_contract(&scenario.draft)
        .expect("Issue #93 multipart scenario should pass strict validation");

    let fixture = resolve_scenario_fixture_path(Path::new("tests/fixtures/httpbingo-upload.txt"))
        .expect("Issue #93 fixture should resolve inside the repository");
    let expected = expected_request(&scenario.expect.request, Some("https://httpbingo.org"))
        .expect("Issue #93 effective request should be valid");
    assert_eq!(
        expected.body,
        RequestBody::Multipart(vec![
            MultipartPart::text("enabled_note", "sent"),
            MultipartPart {
                name: "enabled_upload".to_string(),
                value: MultipartValue::File {
                    path: fixture.clone(),
                    file_name: Some("httpbingo-upload.txt".to_string()),
                    content_type: Some("text/plain".to_string()),
                },
            },
        ])
    );
    assert_eq!(
        expected_editor_intent(&scenario.draft).expect("Issue #93 editor intent should be valid"),
        Some(RequestEditorIntent::Multipart(vec![
            MultipartEditorPart {
                enabled: true,
                name: "enabled_note".to_string(),
                value: MultipartValue::Text("sent".to_string()),
            },
            MultipartEditorPart {
                enabled: true,
                name: "enabled_upload".to_string(),
                value: MultipartValue::File {
                    path: fixture.clone(),
                    file_name: Some("httpbingo-upload.txt".to_string()),
                    content_type: Some("text/plain".to_string()),
                },
            },
            MultipartEditorPart {
                enabled: false,
                name: "disabled_upload".to_string(),
                value: MultipartValue::File {
                    path: fixture,
                    file_name: Some("httpbingo-upload.txt".to_string()),
                    content_type: Some("text/plain".to_string()),
                },
            },
            MultipartEditorPart {
                enabled: false,
                name: "disabled_note".to_string(),
                value: MultipartValue::Text("omit-me".to_string()),
            },
        ]))
    );
}

#[test]
fn multipart_schema_rejects_unknown_part_fields_and_non_multipart_usage() {
    let unknown_field = r#"
    {
      "schema_version": 5,
      "target": "httpbingo",
      "cases": [{
        "name": "unknown multipart field",
        "draft": {
          "method": "POST",
          "body": null,
          "body_kind": "multipart",
          "multipart_parts": [{
            "kind": "text",
            "name": "note",
            "value": "sent",
            "enabeld": false
          }]
        },
        "expect": {
          "request": { "method": "POST", "path": "/post", "body": null },
          "response": { "kind": "success", "status": 200 },
          "history_len": 1
        }
      }]
    }
    "#;
    let error = load_suite(unknown_field)
        .expect_err("unknown multipart part fields must fail strict deserialization");
    assert!(error.contains("enabeld"));

    let wrong_kind = r#"
    {
      "schema_version": 5,
      "target": "httpbingo",
      "cases": [{
        "name": "multipart parts on JSON",
        "draft": {
          "method": "POST",
          "body": null,
          "body_kind": "json",
          "multipart_parts": [{ "kind": "text", "name": "note", "value": "sent" }]
        },
        "expect": {
          "request": { "method": "POST", "path": "/post", "body": null },
          "response": { "kind": "success", "status": 200 },
          "history_len": 1
        }
      }]
    }
    "#;
    let error = load_suite(wrong_kind)
        .expect_err("typed multipart parts must be rejected outside multipart bodies");
    assert!(error.contains("multipart_parts"));
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

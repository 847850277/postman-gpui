use std::{
    fs,
    io::Write,
    process::{Command, Output},
};

use mockito::Matcher;
use serde_json::{json, Value};

#[test]
fn flow_file_drives_real_transport_and_produces_json_report() {
    let mut server = mockito::Server::new();
    let uuid_mock = server
        .mock("GET", "/uuid")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"uuid":"flow-abc-123"}"#)
        .create();
    let order_mock = server
        .mock("POST", "/anything/order")
        .match_header("content-type", "application/json")
        .match_body(Matcher::Json(serde_json::json!({
            "order_id": "flow-abc-123",
            "amount": 100
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"status":"created","order_id":"flow-abc-123"}"#)
        .create();

    let mut fixture = tempfile::Builder::new()
        .suffix(".http.yml")
        .tempfile()
        .expect("a temporary flow file should open");
    write!(
        fixture,
        r#"flow:
  name: test-e2e-flow
  inputs:
  - name: host
  - name: amount
    default: 100
  steps:
  - id: get-uuid
    name: Get UUID
    request:
      kind: http
      method: GET
      url:
        concat:
        - input: host
        - literal: /uuid
    checks:
    - kind: status
      equals: 200
    exports:
    - name: uuid
      path: $.uuid
  - id: create-order
    name: Create Order
    request:
      kind: http
      method: POST
      url:
        concat:
        - input: host
        - literal: /anything/order
      headers:
      - name:
          literal: Content-Type
        value:
          literal: application/json
      body:
        kind: json
        value:
          object:
            order_id:
              output:
                step: get-uuid
                name: uuid
            amount:
              input: amount
    checks:
    - kind: status
      equals: 200
    - kind: jsonpath
      path: $.status
      equals:
        literal: created
schema_version: 1
"#
    )
    .expect("the temporary flow file should be writable");

    let output = Command::new(env!("CARGO_BIN_EXE_postman-g"))
        .args([
            "run",
            fixture.path().to_str().unwrap(),
            "--input",
            &format!("host={}", server.url()),
            "--json",
        ])
        .output()
        .expect("the postman process should start");

    assert!(
        output.status.success(),
        "postman-g run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be a JSON report");
    assert_eq!(report["success"], true);
    assert_eq!(report["schema_version"], 1);
    assert_eq!(
        report["files"][0]["report"]["outputs"],
        serde_json::json!({})
    );
    assert_eq!(
        report["files"][0]["report"]["redacted_outputs"],
        serde_json::json!([])
    );
    let requests = report["files"][0]["report"]["requests"].as_array().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["name"], "get-uuid (Get UUID)");
    assert_eq!(requests[0]["captures"][0], "uuid");
    assert_eq!(requests[1]["name"], "create-order (Create Order)");
    assert_eq!(requests[1]["assertions"][0]["success"], true);
    assert_eq!(requests[1]["assertions"][1]["success"], true);

    uuid_mock.assert();
    order_mock.assert();
}

#[test]
fn flow_file_check_only_validates_without_network() {
    let mut fixture = tempfile::Builder::new()
        .suffix(".http.yml")
        .tempfile()
        .expect("a temporary flow file should open");
    write!(
        fixture,
        r#"flow:
  name: static-check-flow
  inputs:
  - name: host
    default: https://example.com
  steps:
  - id: step-one
    name: First Step
    request:
      kind: http
      method: GET
      url:
        concat:
        - input: host
        - literal: /test
    checks:
    - kind: status
      equals: 200
schema_version: 1
"#
    )
    .expect("the temporary flow file should be writable");

    let output = Command::new(env!("CARGO_BIN_EXE_postman-g"))
        .args(["run", fixture.path().to_str().unwrap(), "--check"])
        .output()
        .expect("the postman process should start");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("OK: static-check-flow (1 steps)"));
}

#[test]
fn flow_assertion_failure_returns_exit_code_one() {
    let mut server = mockito::Server::new();
    let mock = server
        .mock("GET", "/status")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"code":500}"#)
        .create();

    let mut fixture = tempfile::Builder::new()
        .suffix(".http.yml")
        .tempfile()
        .expect("a temporary flow file should open");
    write!(
        fixture,
        r#"flow:
  name: fail-flow
  inputs:
  - name: host
  steps:
  - id: check-status
    name: Check Status
    request:
      kind: http
      method: GET
      url:
        concat:
        - input: host
        - literal: /status
    checks:
    - kind: jsonpath
      path: $.code
      equals:
        literal: '200'
schema_version: 1
"#
    )
    .expect("the temporary flow file should be writable");

    let output = Command::new(env!("CARGO_BIN_EXE_postman-g"))
        .args([
            "run",
            fixture.path().to_str().unwrap(),
            "--input",
            &format!("host={}", server.url()),
            "--json",
        ])
        .output()
        .expect("the postman process should start");

    assert_eq!(output.status.code(), Some(1));
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be a JSON report");
    assert_eq!(report["success"], false);
    mock.assert();
}

#[test]
fn mixed_directory_runs_both_http_and_flow_files() {
    let mut server = mockito::Server::new();
    let http_mock = server
        .mock("GET", "/http-call")
        .with_status(200)
        .with_body("ok")
        .create();
    let flow_mock = server
        .mock("GET", "/flow-call")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"status":"ok"}"#)
        .create();

    let dir = tempfile::tempdir().expect("temporary directory should open");
    let http_path = dir.path().join("01-sample.http");
    fs::write(
        &http_path,
        format!(
            "@host = {}\n### Single Call\n# @assert status == 200\nGET {{{{host}}}}/http-call\n",
            server.url()
        ),
    )
    .unwrap();

    let flow_path = dir.path().join("02-sample.http.yml");
    fs::write(
        &flow_path,
        r#"flow:
  name: sample-flow
  inputs:
  - name: host
  steps:
  - id: flow-call
    name: Flow Call
    request:
      kind: http
      method: GET
      url:
        concat:
        - input: host
        - literal: /flow-call
    checks:
    - kind: status
      equals: 200
schema_version: 1
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_postman-g"))
        .args([
            "run",
            dir.path().to_str().unwrap(),
            "--var",
            &format!("host={}", server.url()),
            "--json",
        ])
        .output()
        .expect("the postman process should start");

    assert!(
        output.status.success(),
        "postman-g run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be a JSON report");
    assert_eq!(report["success"], true);
    assert_eq!(report["files"].as_array().unwrap().len(), 2);
    http_mock.assert();
    flow_mock.assert();
}

#[test]
fn extra_cli_bindings_are_ignored_by_files_that_did_not_declare_them() {
    let mut server = mockito::Server::new();
    let flow_mock = server.mock("GET", "/ping").with_status(200).create();

    let mut fixture = tempfile::Builder::new()
        .suffix(".http.yml")
        .tempfile()
        .expect("a temporary flow file should open");
    write!(
        fixture,
        r#"schema_version: 1
flow:
  name: no-host-input
  steps:
  - id: ping
    request:
      kind: http
      method: GET
      url:
        literal: {url}/ping
    checks:
    - kind: status
      equals: 200
"#,
        url = server.url()
    )
    .expect("the temporary flow file should be writable");

    let output = Command::new(env!("CARGO_BIN_EXE_postman-g"))
        .args([
            "run",
            fixture.path().to_str().unwrap(),
            "--var",
            "host=https://unused.example",
            "--json",
        ])
        .output()
        .expect("the postman process should start");

    assert!(
        output.status.success(),
        "extra --var should not fail a flow that did not declare it\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    flow_mock.assert();
}

#[test]
fn check_compiles_http_files_and_rejects_interpolation_cycles() {
    let mut valid = tempfile::Builder::new()
        .suffix(".http")
        .tempfile()
        .expect("a temporary .http file should open");
    write!(valid, "@host = https://example.com\nGET {{{{host}}}}/ok\n")
        .expect("the valid fixture should be writable");

    let output = Command::new(env!("CARGO_BIN_EXE_postman-g"))
        .args(["run", valid.path().to_str().unwrap(), "--check"])
        .output()
        .expect("the postman process should start");
    assert!(
        output.status.success(),
        "valid .http --check failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("OK:"));

    let mut cyclic = tempfile::Builder::new()
        .suffix(".http")
        .tempfile()
        .expect("a cyclic .http file should open");
    write!(cyclic, "@cycle = {{{{cycle}}}}\nGET {{{{cycle}}}}\n")
        .expect("the cyclic fixture should be writable");

    let output = Command::new(env!("CARGO_BIN_EXE_postman-g"))
        .args(["run", cyclic.path().to_str().unwrap(), "--check"])
        .output()
        .expect("the postman process should start");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cycle"), "unexpected stderr: {stderr}");
}

#[test]
fn directories_ignore_plain_yaml_and_explicit_unknown_files_are_rejected() {
    let mut server = mockito::Server::new();
    let http_mock = server.mock("GET", "/only-http").with_status(200).create();

    let dir = tempfile::tempdir().expect("temporary directory should open");
    fs::write(
        dir.path().join("01-sample.http"),
        format!(
            "@host = {}\n# @assert status == 200\nGET {{{{host}}}}/only-http\n",
            server.url()
        ),
    )
    .unwrap();
    fs::write(
        dir.path().join("docker-compose.yml"),
        "services:\n  web:\n    image: nginx\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_postman-g"))
        .args(["run", dir.path().to_str().unwrap(), "--json"])
        .output()
        .expect("the directory runner should start");
    assert!(
        output.status.success(),
        "plain yaml in a directory should be ignored\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be a suite JSON report");
    assert_eq!(report["files"].as_array().unwrap().len(), 1);
    http_mock.assert();

    let ignored = dir.path().join("docker-compose.yml");
    let output = Command::new(env!("CARGO_BIN_EXE_postman-g"))
        .args(["run", ignored.to_str().unwrap()])
        .output()
        .expect("an explicit non-flow yaml path should start");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not a .http or .http.yml file"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn cli_reports_condition_evaluation_failures_without_sending_requests() {
    let mut server = mockito::Server::new();
    let request = server.mock("GET", "/").expect(0).create();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("condition.http.yml");
    std::fs::write(
        &path,
        format!(
            r#"
schema_version: 1
flow:
  name: condition-error
  steps:
    - id: broken-condition
      when: {{gt: [{{literal: true}}, {{literal: 1}}]}}
      request:
        kind: http
        method: GET
        url: {{literal: "{}"}}
"#,
            server.url()
        ),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_postman-g"))
        .arg("run")
        .arg(&path)
        .arg("--json")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let requests = report["files"][0]["report"]["requests"].as_array().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["name"], "broken-condition");
    assert_eq!(requests[0]["success"], false);
    assert_ne!(requests[0]["skipped"], true);
    assert!(requests[0]["error"]
        .as_str()
        .unwrap()
        .contains("failed to evaluate condition"));
    request.assert();
}

#[test]
fn cli_branching_executes_matched_steps_and_records_skipped_steps() {
    let mut server = mockito::Server::new();
    let query_mock = server
        .mock("POST", "/anything/order-status")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"json":{"order_id":"ORD-1","status":2}}"#)
        .create();
    let fulfill_mock = server
        .mock("POST", "/anything/fulfill")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"json":{"order_id":"ORD-1","action":"SHIP_IMMEDIATELY","invoice_no":"INV-PAID-999"}}"#)
        .create();
    let notify_mock = server
        .mock("POST", "/anything/notify")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"json":{"ok":true}}"#)
        .create();

    let fixture = format!(
        "{}/../postman-flow/examples/flows/httpbingo_branching.http.yml",
        env!("CARGO_MANIFEST_DIR")
    );

    let output = Command::new(env!("CARGO_BIN_EXE_postman-g"))
        .args([
            "run",
            &fixture,
            "--input",
            &format!("host={}", server.url()),
            "--input",
            "status=2",
            "--json",
        ])
        .output()
        .expect("CLI should start");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["success"], true);

    let requests = report["files"][0]["report"]["requests"].as_array().unwrap();
    assert_eq!(requests.len(), 5);

    // query-status: executed
    assert!(!requests[0]["skipped"].as_bool().unwrap_or(false));
    assert_eq!(requests[0]["success"], true);

    // branch-paid: executed
    assert!(!requests[1]["skipped"].as_bool().unwrap_or(false));
    assert_eq!(requests[1]["success"], true);

    // branch-unpaid: skipped
    assert_eq!(requests[2]["skipped"], true);
    assert_eq!(requests[2]["success"], true);

    // branch-other: skipped
    assert_eq!(requests[3]["skipped"], true);
    assert_eq!(requests[3]["success"], true);

    // notify-summary: executed
    assert!(!requests[4]["skipped"].as_bool().unwrap_or(false));
    assert_eq!(requests[4]["success"], true);

    // coalesce outputs
    let outputs = &report["files"][0]["report"]["outputs"];
    assert_eq!(outputs["final_action"], "SHIP_IMMEDIATELY");
    assert_eq!(outputs["final_invoice"], "INV-PAID-999");

    query_mock.assert();
    fulfill_mock.assert();
    notify_mock.assert();
}

const FLOW_OUTPUTS_FIXTURE: &str = r#"schema_version: 1
flow:
  name: flow-output-report
  inputs:
  - name: host
  - name: token
    sensitive: true
  - name: optional
    default: null
  - name: marker
    default: '[REDACTED]'
  steps:
  - id: fetch
    request:
      kind: http
      method: GET
      url:
        concat:
        - input: host
        - literal: /result
      headers:
      - name: { literal: Authorization }
        value:
          concat:
          - literal: 'Bearer '
          - input: token
    checks:
    - kind: status
      equals: 200
    - kind: jsonpath
      path: $.ok
      equals: { literal: true }
    exports:
    - name: data
      path: $.data
    - name: private
      path: $.secret
      sensitive: true
  outputs:
  - name: data
    value: { output: { step: fetch, name: data } }
  - name: secret_response
    value: { output: { step: fetch, name: private } }
  - name: secret_input
    value: { input: token }
  - name: optional
    value: { input: optional }
  - name: marker
    value: { input: marker }
"#;

fn outputs_fixture() -> tempfile::NamedTempFile {
    let mut fixture = tempfile::Builder::new()
        .suffix(".http.yml")
        .tempfile()
        .expect("a temporary flow file should open");
    use std::io::Write;
    write!(fixture, "{FLOW_OUTPUTS_FIXTURE}").expect("fixture should be writable");
    fixture
}

fn run(path: &str, host: &str, json_output: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_postman-g"));
    command.env_remove("RUST_LOG").args([
        "run",
        path,
        "--input",
        &format!("host={host}"),
        "--input",
        "token=private-input-token",
    ]);
    if json_output {
        command.arg("--json");
    }
    command.output().expect("CLI should start")
}

#[test]
fn cli_preserves_json_types_and_redacts_sensitive_returns_in_both_formats() {
    let mut server = mockito::Server::new();
    let data = json!({
        "orders": [{"id": "00123", "paid": false, "amount": 19.5, "note": null}],
        "total": 1,
        "largest": u64::MAX,
        "empty": [],
        "text": "张\"三\"\n二楼"
    });
    let response = server
        .mock("GET", "/result")
        .match_header("authorization", "Bearer private-input-token")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({"ok": true, "data": data, "secret": {"nested": "private-response-token"}})
                .to_string(),
        )
        .expect(2)
        .create();

    let fixture = outputs_fixture();
    let output = run(fixture.path().to_str().unwrap(), &server.url(), true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let suite: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(suite["schema_version"], 1);
    let report = &suite["files"][0]["report"];
    assert_eq!(report["outputs"]["data"], data);
    assert_eq!(report["outputs"]["optional"], Value::Null);
    assert_eq!(report["outputs"]["marker"], "[REDACTED]");
    assert_eq!(report["outputs"]["secret_input"], "[REDACTED]");
    assert_eq!(report["outputs"]["secret_response"], "[REDACTED]");
    assert_eq!(
        report["redacted_outputs"],
        json!(["secret_input", "secret_response"])
    );
    for bytes in [&output.stdout, &output.stderr] {
        let text = String::from_utf8_lossy(bytes);
        assert!(!text.contains("private-input-token"));
        assert!(!text.contains("private-response-token"));
    }

    let fixture = outputs_fixture();
    let output = run(fixture.path().to_str().unwrap(), &server.url(), false);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("OUTPUT data:"));
    assert!(stdout.contains(r#""id":"00123""#));
    assert!(stdout.contains("OUTPUT optional: null"));
    assert!(stdout.contains("OUTPUT secret_input: \"[REDACTED]\""));
    assert!(stdout.contains("OUTPUT secret_response: \"[REDACTED]\""));
    assert!(!stdout.contains("private-input-token"));
    assert!(!stdout.contains("private-response-token"));
    response.assert();
}

#[test]
fn failed_flows_do_not_return_inputs_or_partial_response_values() {
    let mut server = mockito::Server::new();
    let response = server.mock("GET", "/result")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!({"ok": false, "data": {"id": "must-not-return"}, "secret": "private-response-token"}).to_string())
        .create();
    let fixture = outputs_fixture();
    let output = run(fixture.path().to_str().unwrap(), &server.url(), true);
    assert_eq!(output.status.code(), Some(1));
    let suite: Value = serde_json::from_slice(&output.stdout).unwrap();
    let report = &suite["files"][0]["report"];
    assert_eq!(report["success"], false);
    assert_eq!(report["outputs"], json!({}));
    assert_eq!(report["redacted_outputs"], json!([]));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("must-not-return"));
    response.assert();
}

#[test]
fn crmeb_example_returns_order_data_without_exporting_login_credentials() {
    let mut server = mockito::Server::new();
    let pre_login = server
        .mock("GET", "/adminapi/login/info")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data":{"key":"private-login-key"}}"#)
        .create();
    let login = server
        .mock("POST", "/adminapi/login")
        .match_body(Matcher::PartialJson(json!({"key": "private-login-key"})))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data":{"token":"private-login-token"}}"#)
        .create();
    let data = json!({"count": 1, "list": [{"id": "00123", "paid": true, "remark": null}]});
    let orders = server
        .mock("GET", "/adminapi/order/list")
        .match_query(Matcher::Any)
        .match_header("authori-zation", "Bearer private-login-token")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!({"status": 200, "data": data}).to_string())
        .create();
    let path = format!(
        "{}/../postman-flow/examples/flows/crmeb_order_list.http.yml",
        env!("CARGO_MANIFEST_DIR"),
    );
    let output = run(&path, &server.url(), true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let suite: Value = serde_json::from_slice(&output.stdout).unwrap();
    let report = &suite["files"][0]["report"];
    assert_eq!(report["outputs"], json!({"order_data": data}));
    assert_eq!(report["redacted_outputs"], json!([]));
    assert_eq!(report["requests"].as_array().unwrap().len(), 3);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("private-login-key"));
    assert!(!stdout.contains("private-login-token"));
    pre_login.assert();
    login.assert();
    orders.assert();
}

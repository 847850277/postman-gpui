use std::{fs, io::Write, process::Command};

use mockito::Matcher;

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

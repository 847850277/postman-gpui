use serde_json::Value;
use std::process::Command;

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

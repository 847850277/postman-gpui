use std::process::{Command, Output};

use mockito::Matcher;
use serde_json::{json, Value};

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

fn outputs_fixture() -> String {
    format!(
        "{}/tests/fixtures/flow_outputs.http.yml",
        env!("CARGO_MANIFEST_DIR")
    )
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

    let output = run(&outputs_fixture(), &server.url(), true);
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

    let output = run(&outputs_fixture(), &server.url(), false);
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
    let output = run(&outputs_fixture(), &server.url(), true);
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

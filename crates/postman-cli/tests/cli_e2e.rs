use std::{fs, io::Write, process::Command};

use mockito::Matcher;

#[test]
fn http_file_drives_the_real_transport_without_linking_gpui() {
    let mut server = mockito::Server::new();
    let uuid = server
        .mock("GET", "/uuid")
        .match_header("accept", "application/json")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"uuid":"flow-123"}"#)
        .create();
    let echo = server
        .mock("POST", "/anything/headless-e2e/flow-123")
        .match_header("accept", "application/json")
        .match_header("content-type", "application/json")
        .match_header("x-postman-e2e", "postman-gpui-headless")
        .match_body(Matcher::Json(serde_json::json!({
            "client": "postman-gpui-headless",
            "correlation_id": "flow-123"
        })))
        .with_status(200)
        .with_header("content-type", "application/json; charset=utf-8")
       .with_body(
           r#"{"method":"POST","json":{"client":"postman-gpui-headless","correlation_id":"flow-123"},"marker":"headless-e2e"}"#,
       )
       .create();
    let mut fixture = tempfile::Builder::new()
        .suffix(".http")
        .tempfile()
        .expect("a temporary .http file should open");
    write!(
        fixture,
        r#"@host = {host}
@client = postman-gpui-headless

### Generate a correlation id
# @name generate-correlation-id
# @assert status == 200
# @assert header "content-type" contains "application/json"
# @capture correlation_id = jsonpath "$.uuid"
GET {{{{host}}}}/uuid
Accept: application/json

### Send a typed request with the captured value
# @name echo-http-file-request
# @assert status == 200
# @assert header "content-type" contains "application/json"
# @assert jsonpath "$.method" == "POST"
# @assert jsonpath "$.json.client" == "postman-gpui-headless"
# @assert jsonpath "$.json.correlation_id" == "{{{{correlation_id}}}}"
# @assert body contains "headless-e2e"
POST {{{{host}}}}/anything/headless-e2e/{{{{correlation_id}}}}
Accept: application/json
Content-Type: application/json
X-Postman-E2E: {{{{client}}}}

{{
  "client": "{{{{client}}}}",
  "correlation_id": "{{{{correlation_id}}}}"
}}
"#,
        host = server.url()
    )
    .expect("the temporary .http file should be writable");

    let output = Command::new(env!("CARGO_BIN_EXE_postman-g"))
        .args(["run", fixture.path().to_str().unwrap(), "--json"])
        .output()
        .expect("the headless process should start");

    assert!(
        output.status.success(),
        "headless runner failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be a JSON report");
    assert_eq!(report["success"], true);
    assert_eq!(report["schema_version"], 1);
    assert_eq!(
        report["files"][0]["report"]["outputs"]["correlation_id"],
        "flow-123"
    );
    assert_eq!(
        report["files"][0]["report"]["requests"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    assert_eq!(
        report["files"][0]["report"]["requests"][0]["captures"][0],
        "correlation_id"
    );
    uuid.assert();
    echo.assert();
}

#[test]
fn assertion_failure_returns_exit_code_one_and_a_machine_readable_report() {
    let mut server = mockito::Server::new();
    let response = server
        .mock("GET", "/status")
        .with_status(200)
        .with_body("ok")
        .create();
    let mut fixture = tempfile::Builder::new()
        .suffix(".http")
        .tempfile()
        .expect("a temporary .http file should open");
    write!(
        fixture,
        "@host = {}\n### expected failure\n# @assert status == 201\nGET {{{{host}}}}/status\n",
        server.url()
    )
    .expect("the temporary .http file should be writable");

    let output = Command::new(env!("CARGO_BIN_EXE_postman-g"))
        .args(["run", fixture.path().to_str().unwrap(), "--json"])
        .output()
        .expect("the headless process should start");

    assert_eq!(output.status.code(), Some(1));
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be a JSON report");
    assert_eq!(report["success"], false);
    assert_eq!(
        report["files"][0]["report"]["requests"][0]["assertions"][0]["success"],
        false
    );
    response.assert();
}

#[test]
fn invalid_http_file_returns_exit_code_two_with_a_source_location() {
    let mut fixture = tempfile::Builder::new()
        .suffix(".http")
        .tempfile()
        .expect("a temporary .http file should open");
    writeln!(fixture, "NOT_A_METHOD https://example.com")
        .expect("the temporary .http file should be writable");

    let output = Command::new(env!("CARGO_BIN_EXE_postman-g"))
        .args(["run", fixture.path().to_str().unwrap()])
        .output()
        .expect("the headless process should start");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(":line 1:"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("Unsupported HTTP method"));
}

#[test]
fn directory_runs_http_files_in_order_with_one_shared_http_session() {
    let mut server = mockito::Server::new();
    let set_cookie = server
        .mock("GET", "/set-cookie")
        .with_status(200)
        .with_header("set-cookie", "session=directory-matrix; Path=/")
        .create();
    let echo_cookie = server
        .mock("GET", "/echo-cookie")
        .match_header("cookie", "session=directory-matrix")
        .with_status(200)
        .create();
    let fixtures = tempfile::tempdir().expect("a fixture directory should be created");
    fs::write(
        fixtures.path().join("01-set.http"),
        "# @assert status == 200\nGET {{host}}/set-cookie\n",
    )
    .expect("the first fixture should be writable");
    fs::write(
        fixtures.path().join("02-echo.http"),
        "# @assert status == 200\nGET {{host}}/echo-cookie\n",
    )
    .expect("the second fixture should be writable");

    let output = Command::new(env!("CARGO_BIN_EXE_postman-g"))
        .args([
            "run",
            fixtures.path().to_str().unwrap(),
            "--var",
            &format!("host={}", server.url()),
            "--json",
        ])
        .output()
        .expect("the directory suite should start");

    assert!(
        output.status.success(),
        "directory runner failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be a suite JSON report");
    let files = report["files"]
        .as_array()
        .expect("suite files should be an array");
    assert_eq!(files.len(), 2);
    assert!(files[0]["path"].as_str().unwrap().ends_with("01-set.http"));
    assert!(files[1]["path"].as_str().unwrap().ends_with("02-echo.http"));
    set_cookie.assert();
    echo_cookie.assert();
}

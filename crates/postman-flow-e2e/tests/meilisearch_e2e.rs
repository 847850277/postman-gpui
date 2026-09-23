use postman_flow_e2e::harness::find_postman_g;
use postman_flow_e2e::harness::meilisearch::MeilisearchServer;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

fn suite_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("suites/meilisearch")
        .join(path)
}

async fn server() -> Option<MeilisearchServer> {
    match MeilisearchServer::start().await {
        Ok(server) => Some(server),
        Err(error) => {
            // Only the dedicated E2E job provisions a server. Generic workspace CI
            // also sets CI=true, but may run on a host without Docker or Meilisearch.
            let required = std::env::var("MEILISEARCH_E2E_REQUIRED")
                .is_ok_and(|value| value == "true" || value == "1");
            assert!(
                !required,
                "Meilisearch E2E requires a working server: {error}"
            );
            eprintln!("Skipping Meilisearch E2E (server unavailable): {error}");
            None
        }
    }
}

fn run_flow(server: &MeilisearchServer, file: &str, dataset: Option<&Path>) -> Value {
    let default_dataset = suite_path("data/hackernews_sample.ndjson");
    let output = Command::new(find_postman_g())
        .arg("run")
        .arg(suite_path(&format!("flows/{file}.http.yml")))
        .args([
            "--input",
            &format!("host={}", server.base_url),
            "--input",
            &format!("master_key={}", server.master_key),
            "--input",
            &format!(
                "dataset_path={}",
                dataset.unwrap_or(&default_dataset).display()
            ),
            "--json",
        ])
        .output()
        .expect("failed to execute postman-g");
    // Save raw output before parsing/asserting, including a failing or malformed report.
    // Unique invocation IDs preserve repeated imports and concurrent test results.
    if let Some(directory) = std::env::var_os("MEILISEARCH_E2E_ARTIFACT_DIR") {
        static INVOCATION: AtomicUsize = AtomicUsize::new(0);
        let directory = PathBuf::from(directory);
        std::fs::create_dir_all(&directory).expect("create E2E artifact directory");
        let prefix = format!(
            "flow-{}-{}-{file}",
            std::process::id(),
            INVOCATION.fetch_add(1, Ordering::Relaxed)
        );
        std::fs::write(directory.join(format!("{prefix}.json")), &output.stdout)
            .expect("save flow JSON report");
        std::fs::write(
            directory.join(format!("{prefix}.stderr.log")),
            &output.stderr,
        )
        .expect("save flow stderr");
    }
    let report: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{file}: invalid report: {error}\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(output.status.success(), report["success"] == true);
    report["files"][0]["report"].clone()
}

fn assert_passed(report: &Value, operations: usize, waits: usize) {
    assert_eq!(report["success"], true, "{report:#}");
    let requests = report["requests"].as_array().unwrap();
    // Poll requests vary with indexing speed; count business operations separately.
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.get("loop_path").is_none())
            .count(),
        operations
    );
    assert!(requests.iter().all(|r| r["success"] == true));
    let loops = report["loops"].as_array().cloned().unwrap_or_default();
    assert_eq!(loops.len(), waits);
    for task_wait in loops {
        assert_eq!(task_wait["success"], true, "{task_wait:#}");
        assert_eq!(task_wait["reason"], "condition_met");
        assert!(task_wait["total_executed"].as_u64().unwrap() >= 1);
    }
}

#[tokio::test]
async fn test_meilisearch_e2e_api_keys_flow() {
    let Some(server) = server().await else { return };
    assert_passed(&run_flow(&server, "api_keys", None), 8, 0);
}

#[tokio::test]
async fn test_meilisearch_e2e_hackernews_streaming_flow() {
    let Some(server) = server().await else { return };
    // First run exercises missing-index cleanup; second run deletes an existing index.
    // Queries run immediately after the YAML returns, with no external task drain.
    for _ in 0..2 {
        assert_passed(&run_flow(&server, "hackernews_streaming", None), 4, 4);
        assert_passed(&run_flow(&server, "hackernews_query", None), 3, 0);
    }
}

#[tokio::test]
async fn test_meilisearch_e2e_documents_crud_lifecycle_flow() {
    let Some(server) = server().await else { return };
    assert_passed(&run_flow(&server, "documents_crud_lifecycle", None), 7, 7);
}

#[tokio::test]
async fn test_meilisearch_e2e_multi_search_and_facets_flow() {
    let Some(server) = server().await else { return };
    assert_passed(&run_flow(&server, "documents_ingestion", None), 4, 3);
    assert_passed(&run_flow(&server, "search_and_ranking", None), 3, 0);
    assert_passed(&run_flow(&server, "hackernews_streaming", None), 4, 4);
    assert_passed(&run_flow(&server, "multi_search_and_facets", None), 5, 2);
}

#[tokio::test]
async fn test_meilisearch_e2e_rejected_document_stops_flow_after_http_202() {
    let Some(server) = server().await else { return };
    let dataset = tempfile::NamedTempFile::new().unwrap();
    // Valid NDJSON is accepted with HTTP 202, then fails the required primary key check.
    std::fs::write(dataset.path(), "{\"title\":\"missing id\"}\n").unwrap();
    let report = run_flow(&server, "hackernews_streaming", Some(dataset.path()));
    assert_eq!(report["success"], false, "{report:#}");
    let loops = report["loops"].as_array().unwrap();
    assert_eq!(loops.len(), 3);
    assert_eq!(loops[2]["step_id"], "wait-step-3-stream-hackernews-dataset");
    assert_eq!(loops[2]["reason"], "failure_condition");
    let operations: Vec<_> = report["requests"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r.get("loop_path").is_none())
        .collect();
    assert_eq!(
        operations.len(),
        3,
        "settings must not run after a failed import"
    );
    assert!(operations.iter().all(|r| r["status"] == 202));
}

#[test]
fn server_availability_is_required_only_when_explicitly_enabled() {
    // Subprocesses avoid mutating environment variables shared by parallel tests.
    for (ci, required, must_fail) in [
        ("true", Some("1"), true),
        ("false", Some("1"), true),
        ("true", Some("true"), true),
        ("false", Some("true"), true),
        ("true", Some("0"), false),
        ("false", Some("0"), false),
        ("true", None, false),
        ("false", None, false),
    ] {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "test_meilisearch_e2e_api_keys_flow",
                "--nocapture",
            ])
            .env("CI", ci)
            .env("MEILISEARCH_URL", "invalid-url")
            .env_remove("MEILISEARCH_E2E_ARTIFACT_DIR");
        match required {
            Some(value) => command.env("MEILISEARCH_E2E_REQUIRED", value),
            None => command.env_remove("MEILISEARCH_E2E_REQUIRED"),
        };
        let output = command.output().expect("run unavailable-server check");
        let logs = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.success(), !must_fail, "{logs}");
        assert!(
            logs.contains(if must_fail {
                "Meilisearch E2E requires a working server"
            } else {
                "Skipping Meilisearch E2E"
            }),
            "{logs}"
        );
    }
}

#[cfg(unix)]
#[test]
fn docker_start_failure_preserves_diagnostics_and_cleans_up() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let docker = directory.path().join("docker");
    // Simulate Docker failing after container creation. This exercises the real
    // harness failure/cleanup path without modifying the developer's daemon.
    std::fs::write(
        &docker,
        r#"#!/bin/sh
case "$1" in
    --version) echo 'Docker test double' ;;
    run) echo 'intentional container startup failure' >&2; exit 42 ;;
    logs) echo 'diagnostic from failed container' >&2 ;;
    rm) echo cleaned > "$MEILISEARCH_E2E_ARTIFACT_DIR/cleanup.marker" ;;
    *) exit 99 ;;
esac
"#,
    )
    .unwrap();
    std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).unwrap();
    let artifacts = directory.path().join("artifacts");
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "test_meilisearch_e2e_api_keys_flow",
            "--nocapture",
        ])
        .env("PATH", directory.path())
        .env_remove("MEILISEARCH_URL")
        .env("MEILISEARCH_E2E_REQUIRED", "1")
        .env("MEILISEARCH_E2E_ARTIFACT_DIR", &artifacts)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("intentional container startup failure")
    );
    let logs: String = std::fs::read_dir(&artifacts)
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect();
    assert!(logs.contains("intentional container startup failure"));
    assert!(logs.contains("diagnostic from failed container"));
    assert_eq!(
        std::fs::read_to_string(artifacts.join("cleanup.marker")).unwrap(),
        "cleaned\n"
    );
}

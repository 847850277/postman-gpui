use postman_flow_e2e::harness::find_postman_g;
use postman_flow_e2e::harness::meilisearch::MeilisearchServer;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn suite_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("suites/meilisearch")
        .join(path)
}

async fn server() -> Option<MeilisearchServer> {
    match MeilisearchServer::start().await {
        Ok(server) => Some(server),
        Err(error) => {
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

use postman_flow_e2e::harness::find_postman_g;
use postman_flow_e2e::harness::meilisearch::MeilisearchServer;
use std::path::PathBuf;
use std::process::Command;

#[tokio::test]
async fn test_meilisearch_e2e_api_keys_flow() {
    let server = match MeilisearchServer::start().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "Skipping test_meilisearch_e2e_api_keys_flow (server start skipped): {}",
                e
            );
            return;
        }
    };

    let postman_g = find_postman_g();
    let flow_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("suites/meilisearch/flows/api_keys.http.yml");

    let output = Command::new(&postman_g)
        .args([
            "run",
            flow_path.to_str().unwrap(),
            "--input",
            &format!("host={}", server.base_url),
            "--input",
            &format!("master_key={}", server.master_key),
            "--json",
        ])
        .output()
        .expect("failed to execute postman-g");

    assert!(
        output.status.success(),
        "Flow execution failed:
{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("output should be valid JSON");
    assert_eq!(report["success"], true);

    let requests = report["files"][0]["report"]["requests"]
        .as_array()
        .expect("requests should be array");
    assert_eq!(requests.len(), 8);
    for req in requests {
        assert_eq!(req["success"], true, "Request failed: {:?}", req);
    }
}

#[tokio::test]
async fn test_meilisearch_e2e_hackernews_streaming_flow() {
    let server = match MeilisearchServer::start().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "Skipping test_meilisearch_e2e_hackernews_streaming_flow (server start skipped): {}",
                e
            );
            return;
        }
    };

    let postman_g = find_postman_g();
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let streaming_flow =
        manifest_dir.join("suites/meilisearch/flows/hackernews_streaming.http.yml");
    let query_flow = manifest_dir.join("suites/meilisearch/flows/hackernews_query.http.yml");
    let dataset = manifest_dir.join("suites/meilisearch/data/hackernews_sample.ndjson");

    // 1. Ingest streaming dataset via file body
    let output = Command::new(&postman_g)
        .args([
            "run",
            streaming_flow.to_str().unwrap(),
            "--input",
            &format!("host={}", server.base_url),
            "--input",
            &format!("master_key={}", server.master_key),
            "--input",
            &format!("dataset_path={}", dataset.display()),
            "--json",
        ])
        .output()
        .expect("failed to execute postman-g");

    assert!(
        output.status.success(),
        "Streaming flow failed:
{}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 2. Wait for background task queue to drain
    let client = reqwest::Client::new();
    let tasks_url = format!("{}/tasks?statuses=enqueued,processing", server.base_url);
    for _ in 0..100 {
        if let Ok(res) = client
            .get(&tasks_url)
            .header("Authorization", format!("Bearer {}", server.master_key))
            .send()
            .await
        {
            if let Ok(text) = res.text().await {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                    if val.get("total").and_then(|v| v.as_u64()) == Some(0) {
                        break;
                    }
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // 3. Run search queries against ingested dataset
    let query_output = Command::new(&postman_g)
        .args([
            "run",
            query_flow.to_str().unwrap(),
            "--input",
            &format!("host={}", server.base_url),
            "--input",
            &format!("master_key={}", server.master_key),
            "--json",
        ])
        .output()
        .expect("failed to execute postman-g");

    assert!(
        query_output.status.success(),
        "Query flow failed:
{}",
        String::from_utf8_lossy(&query_output.stderr)
    );
}

#[tokio::test]
async fn test_meilisearch_e2e_documents_crud_lifecycle_flow() {
    let server = match MeilisearchServer::start().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "Skipping test_meilisearch_e2e_documents_crud_lifecycle_flow (server start skipped): {}",
                e
            );
            return;
        }
    };

    let postman_g = find_postman_g();
    let flow_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("suites/meilisearch/flows/documents_crud_lifecycle.http.yml");

    let output = Command::new(&postman_g)
        .args([
            "run",
            flow_path.to_str().unwrap(),
            "--input",
            &format!("host={}", server.base_url),
            "--input",
            &format!("master_key={}", server.master_key),
            "--json",
        ])
        .output()
        .expect("failed to execute postman-g");

    assert!(
        output.status.success(),
        "Flow execution failed:
{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("output should be valid JSON");
    assert_eq!(report["success"], true);

    let requests = report["files"][0]["report"]["requests"]
        .as_array()
        .expect("requests should be array");
    assert_eq!(requests.len(), 7);
    for req in requests {
        assert_eq!(req["success"], true, "Request failed: {:?}", req);
    }
}

#[tokio::test]
async fn test_meilisearch_e2e_multi_search_and_facets_flow() {
    let server = match MeilisearchServer::start().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "Skipping test_meilisearch_e2e_multi_search_and_facets_flow (server start skipped): {}",
                e
            );
            return;
        }
    };

    let postman_g = find_postman_g();
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let doc_ingest = manifest_dir.join("suites/meilisearch/flows/documents_ingestion.http.yml");
    let hn_streaming = manifest_dir.join("suites/meilisearch/flows/hackernews_streaming.http.yml");
    let multi_flow = manifest_dir.join("suites/meilisearch/flows/multi_search_and_facets.http.yml");
    let dataset = manifest_dir.join("suites/meilisearch/data/hackernews_sample.ndjson");

    // 1. Ingest movies
    let output1 = Command::new(&postman_g)
        .args([
            "run",
            doc_ingest.to_str().unwrap(),
            "--input",
            &format!("host={}", server.base_url),
            "--input",
            &format!("master_key={}", server.master_key),
            "--json",
        ])
        .output()
        .expect("failed to execute postman-g");
    assert!(output1.status.success(), "Ingest movies failed");

    // 2. Ingest hackernews
    let output2 = Command::new(&postman_g)
        .args([
            "run",
            hn_streaming.to_str().unwrap(),
            "--input",
            &format!("host={}", server.base_url),
            "--input",
            &format!("master_key={}", server.master_key),
            "--input",
            &format!("dataset_path={}", dataset.display()),
            "--json",
        ])
        .output()
        .expect("failed to execute postman-g");
    assert!(output2.status.success(), "Ingest hackernews failed");

    // Wait tasks to drain
    let client = reqwest::Client::new();
    let tasks_url = format!("{}/tasks?statuses=enqueued,processing", server.base_url);
    for _ in 0..100 {
        if let Ok(res) = client
            .get(&tasks_url)
            .header("Authorization", format!("Bearer {}", server.master_key))
            .send()
            .await
        {
            if let Ok(text) = res.text().await {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                    if val.get("total").and_then(|v| v.as_u64()) == Some(0) {
                        break;
                    }
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // 3. Run multi search and facets flow
    let output3 = Command::new(&postman_g)
        .args([
            "run",
            multi_flow.to_str().unwrap(),
            "--input",
            &format!("host={}", server.base_url),
            "--input",
            &format!("master_key={}", server.master_key),
            "--json",
        ])
        .output()
        .expect("failed to execute postman-g");

    assert!(
        output3.status.success(),
        "Multi-search & facets flow failed:
{}",
        String::from_utf8_lossy(&output3.stderr)
    );

    let report: serde_json::Value =
        serde_json::from_slice(&output3.stdout).expect("output should be valid JSON");
    assert_eq!(report["success"], true);
}

use postman_flow_e2e::harness::find_postman_g;
use postman_flow_e2e::harness::qdrant::QdrantServer;
use std::path::PathBuf;
use std::process::Command;

#[tokio::test]
async fn test_qdrant_e2e_full_suite() {
    let server = match QdrantServer::start().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "Skipping test_qdrant_e2e_full_suite (server start skipped): {}",
                e
            );
            return;
        }
    };

    let postman_g = find_postman_g();
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let flows_dir = manifest_dir.join("suites/qdrant/flows");

    let suites = [
        ("collections_lifecycle.http.yml", 7),
        ("points_and_vector_search.http.yml", 5),
        ("payload_crud_and_cleanup.http.yml", 6),
    ];

    for (file_name, expected_requests) in suites {
        let flow_path = flows_dir.join(file_name);
        let output = Command::new(&postman_g)
            .args([
                "run",
                flow_path.to_str().unwrap(),
                "--input",
                &format!("host={}", server.base_url),
                "--json",
            ])
            .output()
            .expect("failed to execute postman-g");

        assert!(
            output.status.success(),
            "Suite {} failed:\n{}",
            file_name,
            String::from_utf8_lossy(&output.stderr)
        );

        let report: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("output should be valid JSON");
        assert_eq!(
            report["success"], true,
            "Suite report failed for {}",
            file_name
        );

        let requests = report["files"][0]["report"]["requests"]
            .as_array()
            .expect("requests should be array");
        assert_eq!(
            requests.len(),
            expected_requests,
            "Request count mismatch for {}",
            file_name
        );

        for req in requests {
            assert_eq!(
                req["success"], true,
                "Request failed in {}: {:?}",
                file_name, req
            );
        }
    }
}


use postman_flow_e2e::harness::find_postman_g;
use postman_flow_e2e::harness::vaultwarden::VaultwardenServer;
use std::path::PathBuf;
use std::process::Command;

#[tokio::test]
async fn test_vaultwarden_e2e_login_flow() {
    let server = match VaultwardenServer::start().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "Skipping test_vaultwarden_e2e_login_flow (server start skipped): {}",
                e
            );
            return;
        }
    };

    let postman_g = find_postman_g();
    let flow_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("suites/vaultwarden/flows/login.http.yml");

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
    assert_eq!(requests.len(), 5);
    for req in requests {
        assert_eq!(req["success"], true, "Request failed: {:?}", req);
    }
}

#[tokio::test]
async fn test_vaultwarden_e2e_send_flow() {
    let server = match VaultwardenServer::start().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "Skipping test_vaultwarden_e2e_send_flow (server start skipped): {}",
                e
            );
            return;
        }
    };

    let postman_g = find_postman_g();
    let flow_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("suites/vaultwarden/flows/send.http.yml");

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
    assert_eq!(requests.len(), 11);
    for req in requests {
        assert_eq!(req["success"], true, "Request failed: {:?}", req);
    }
}

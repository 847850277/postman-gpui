pub mod qdrant;
pub mod meilisearch;
pub mod vaultwarden;

use std::path::PathBuf;

pub fn find_postman_g() -> PathBuf {
    if let Ok(p) = std::env::var("POSTMAN_G") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return pb;
        }
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let debug = root.join("target/debug/postman-g");
    if debug.is_file() {
        return debug;
    }
    let release = root.join("target/release/postman-g");
    if release.is_file() {
        return release;
    }
    if let Ok(output) = std::process::Command::new("which")
        .arg("postman-g")
        .output()
    {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() {
                return PathBuf::from(s);
            }
        }
    }
    panic!("postman-g binary not found. Please build postman-cli first or set POSTMAN_G.");
}

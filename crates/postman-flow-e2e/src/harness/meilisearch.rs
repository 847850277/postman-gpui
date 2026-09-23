use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

pub struct MeilisearchServer {
    child: Option<Child>,
    docker_container_name: Option<String>,
    pub port: u16,
    pub base_url: String,
    pub master_key: String,
    data_dir: Option<PathBuf>,
}

impl MeilisearchServer {
    /// Start a Meilisearch instance:
    /// 1. If MEILISEARCH_URL is set, use it directly.
    /// 2. If docker is available, run MEILISEARCH_IMAGE (default: getmeili/meilisearch:latest).
    /// 3. Otherwise, fallback to local binary if present.
    pub async fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let master_key = std::env::var("MEILISEARCH_MASTER_KEY")
            .unwrap_or_else(|_| "masterKey12345678901234567890".to_string());

        // 1. External / CI configured URL
        if let Ok(url) = std::env::var("MEILISEARCH_URL") {
            let u = url.trim().trim_end_matches('/').to_string();
            if !u.is_empty() {
                let s = Self {
                    child: None,
                    docker_container_name: None,
                    port: 7700,
                    base_url: u.clone(),
                    master_key,
                    data_dir: None,
                };
                Self::wait_healthy(&u).await?;
                return Ok(s);
            }
        }

        // Allocate a free dynamic port
        let port = {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            listener.local_addr()?.port()
        };

        // 2. Try Docker container first (zero local build / universal cross-platform)
        if Self::has_docker() {
            let container_name = format!("postman_flow_e2e_meili_{}_{}", std::process::id(), port);
            let image = std::env::var("MEILISEARCH_IMAGE")
                .unwrap_or_else(|_| "getmeili/meilisearch:latest".into());
            // Keep the container until logs are collected, even if the server exits early.
            let output = Command::new("docker")
                .args([
                    "run",
                    "-d",
                    "--name",
                    &container_name,
                    "-p",
                    &format!("127.0.0.1:{}:7700", port),
                    "-e",
                    &format!("MEILI_MASTER_KEY={}", master_key),
                    "-e",
                    "MEILI_NO_ANALYTICS=true",
                    "-e",
                    "MEILI_ENV=development",
                    &image,
                ])
                .output()?;
            Self::save_docker_output(&container_name, "start", &output);

            if output.status.success() {
                let base_url = format!("http://127.0.0.1:{}", port);
                let instance = Self {
                    child: None,
                    docker_container_name: Some(container_name),
                    port,
                    base_url: base_url.clone(),
                    master_key: master_key.clone(),
                    data_dir: None,
                };
                if let Err(e) = Self::wait_healthy(&base_url).await {
                    drop(instance);
                    return Err(e);
                }
                return Ok(instance);
            }
            Self::collect_and_remove_container(&container_name);
            return Err(format!(
                "Could not start Meilisearch container: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }

        // 3. Fallback: local binary if installed/compiled
        if let Some(binary) = Self::find_local_binary() {
            let temp_dir = tempfile::tempdir()?;
            let db_path = temp_dir.path().to_path_buf();
            let _ = Box::leak(Box::new(temp_dir));

            let mut cmd = Command::new(&binary);
            cmd.arg(format!("--http-addr=127.0.0.1:{}", port))
                .arg(format!("--master-key={}", master_key))
                .arg(format!("--db-path={}", db_path.display()))
                .arg("--no-analytics")
                .arg("--env=development")
                .stdout(Stdio::null())
                .stderr(Stdio::null());

            let child = cmd.spawn()?;
            let base_url = format!("http://127.0.0.1:{}", port);

            let mut instance = Self {
                child: Some(child),
                docker_container_name: None,
                port,
                base_url: base_url.clone(),
                master_key,
                data_dir: Some(db_path),
            };

            if let Err(e) = Self::wait_healthy(&base_url).await {
                instance.stop();
                return Err(e);
            }
            return Ok(instance);
        }

        Err("Could not start Meilisearch. Please install Docker or set MEILISEARCH_URL / MEILISEARCH_BIN.".into())
    }

    fn has_docker() -> bool {
        Command::new("docker")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn find_local_binary() -> Option<PathBuf> {
        if let Ok(p) = std::env::var("MEILISEARCH_BIN") {
            let pb = PathBuf::from(p);
            if pb.is_file() {
                return Some(pb);
            }
        }
        if let Ok(output) = Command::new("which").arg("meilisearch").output() {
            if output.status.success() {
                let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !s.is_empty() {
                    return Some(PathBuf::from(s));
                }
            }
        }
        None
    }

    async fn wait_healthy(base_url: &str) -> Result<(), Box<dyn std::error::Error>> {
        let url = reqwest::Url::parse(&format!("{}/health", base_url))?;
        let start = Instant::now();
        let timeout = Duration::from_secs(15);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(1))
            .build()?;

        while start.elapsed() < timeout {
            if let Ok(res) = client.get(url.clone()).send().await {
                if res.status().is_success() {
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }

        Err(format!("Meilisearch health check failed at {} within 15s", url).into())
    }

    fn save_docker_output(container: &str, kind: &str, output: &Output) {
        let Some(directory) = std::env::var_os("MEILISEARCH_E2E_ARTIFACT_DIR") else {
            return;
        };
        let directory = PathBuf::from(directory);
        let mut contents = output.stdout.clone();
        contents.extend_from_slice(&output.stderr);
        if let Err(error) = std::fs::create_dir_all(&directory).and_then(|()| {
            std::fs::write(directory.join(format!("{container}.{kind}.log")), contents)
        }) {
            // Do not panic in Drop while unwinding an already failed test.
            eprintln!("Could not save Meilisearch {kind} log: {error}");
        }
    }

    fn collect_and_remove_container(container: &str) {
        if let Ok(output) = Command::new("docker").args(["logs", container]).output() {
            Self::save_docker_output(container, "server", &output);
        }
        let _ = Command::new("docker")
            .args(["rm", "-f", container])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    pub fn stop(&mut self) {
        if let Some(container) = self.docker_container_name.take() {
            Self::collect_and_remove_container(&container);
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(data_dir) = self.data_dir.take() {
            let _ = std::fs::remove_dir_all(&data_dir);
        }
    }
}

impl Drop for MeilisearchServer {
    fn drop(&mut self) {
        self.stop();
    }
}

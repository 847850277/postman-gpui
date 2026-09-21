use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub struct QdrantServer {
    child: Option<Child>,
    docker_container_name: Option<String>,
    pub port: u16,
    pub base_url: String,
    data_dir: Option<PathBuf>,
}

impl QdrantServer {
    /// Start a Qdrant instance:
    /// 1. If QDRANT_URL is set, use it directly.
    /// 2. If docker is available, run official `qdrant/qdrant:latest` container.
    /// 3. Otherwise, fallback to local binary if present.
    pub async fn start() -> Result<Self, Box<dyn std::error::Error>> {
        // 1. External / CI configured URL
        if let Ok(url) = std::env::var("QDRANT_URL") {
            let u = url.trim().trim_end_matches('/').to_string();
            if !u.is_empty() {
                let s = Self {
                    child: None,
                    docker_container_name: None,
                    port: 6333,
                    base_url: u.clone(),
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
            let container_name = format!("postman_flow_e2e_qdrant_{}", port);
            let status = Command::new("docker")
                .args([
                    "run",
                    "-d",
                    "--rm",
                    "--name",
                    &container_name,
                    "-p",
                    &format!("{}:6333", port),
                    "qdrant/qdrant:latest",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();

            if let Ok(st) = status {
                if st.success() {
                    let base_url = format!("http://127.0.0.1:{}", port);
                    let instance = Self {
                        child: None,
                        docker_container_name: Some(container_name),
                        port,
                        base_url: base_url.clone(),
                        data_dir: None,
                    };
                    if let Err(e) = Self::wait_healthy(&base_url).await {
                        drop(instance);
                        return Err(e);
                    }
                    return Ok(instance);
                }
            }
        }

        // 3. Fallback: local binary if installed/compiled
        if let Some(binary) = Self::find_local_binary() {
            let temp_dir = tempfile::tempdir()?;
            let db_path = temp_dir.path().to_path_buf();
            let _ = Box::leak(Box::new(temp_dir));

            let mut cmd = Command::new(&binary);
            cmd.env("QDRANT__SERVICE__HTTP_PORT", port.to_string())
                .env("QDRANT__STORAGE__STORAGE_PATH", db_path.display().to_string())
                .stdout(Stdio::null())
                .stderr(Stdio::null());

            let child = cmd.spawn()?;
            let base_url = format!("http://127.0.0.1:{}", port);

            let mut instance = Self {
                child: Some(child),
                docker_container_name: None,
                port,
                base_url: base_url.clone(),
                data_dir: Some(db_path),
            };

            if let Err(e) = Self::wait_healthy(&base_url).await {
                instance.stop();
                return Err(e);
            }
            return Ok(instance);
        }

        Err("Could not start Qdrant. Please install Docker or set QDRANT_URL / QDRANT_BIN.".into())
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
        if let Ok(p) = std::env::var("QDRANT_BIN") {
            let pb = PathBuf::from(p);
            if pb.is_file() {
                return Some(pb);
            }
        }
        if let Ok(output) = Command::new("which").arg("qdrant").output() {
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
        let start = Instant::now();
        let timeout = Duration::from_secs(15);
        let client = reqwest::Client::new();
        let url = format!("{}/readyz", base_url);

        while start.elapsed() < timeout {
            if let Ok(res) = client.get(&url).send().await {
                if res.status().is_success() {
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }

        Err(format!("Qdrant ready check failed at {} within 15s", url).into())
    }

    pub fn stop(&mut self) {
        if let Some(container) = self.docker_container_name.take() {
            let _ = Command::new("docker")
                .args(["stop", "-t", "1", &container])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
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

impl Drop for QdrantServer {
    fn drop(&mut self) {
        self.stop();
    }
}


use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub struct VaultwardenServer {
    child: Option<Child>,
    docker_container_name: Option<String>,
    pub port: u16,
    pub base_url: String,
    db_path: Option<PathBuf>,
}

impl VaultwardenServer {
    /// Start a Vaultwarden instance:
    /// 1. If VAULTWARDEN_URL is set (e.g. CI service container or remote server), use it directly.
    /// 2. If docker is available, run official `vaultwarden/server:latest` container.
    /// 3. Otherwise, fallback to local binary if present.
    pub async fn start() -> Result<Self, Box<dyn std::error::Error>> {
        // 1. External / CI configured URL
        if let Ok(url) = std::env::var("VAULTWARDEN_URL") {
            let u = url.trim().trim_end_matches('/').to_string();
            if !u.is_empty() {
                let s = Self {
                    child: None,
                    docker_container_name: None,
                    port: 80,
                    base_url: u.clone(),
                    db_path: None,
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
            let container_name = format!("postman_flow_e2e_vw_{}", port);
            let status = Command::new("docker")
                .args([
                    "run",
                    "-d",
                    "--rm",
                    "--name",
                    &container_name,
                    "-p",
                    &format!("{}:80", port),
                    "-e",
                    "I_REALLY_WANT_VOLATILE_STORAGE=true",
                    "-e",
                    "SIGNUPS_ALLOWED=true",
                    "-e",
                    "WEB_VAULT_ENABLED=false",
                    "-e",
                    "LOG_LEVEL=warn",
                    "-e",
                    "LOGIN_RATELIMIT_MAX_BURST=1000",
                    "-e",
                    "UNAUTHENTICATED_RATELIMIT_MAX_BURST=1000",
                    "-e",
                    "ADMIN_TOKEN=testadmin1234567890",
                    "vaultwarden/server:latest",
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
                        db_path: None,
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
            let db_path = temp_dir.path().join(format!("vw_test_{}.sqlite3", port));
            let _ = Box::leak(Box::new(temp_dir));

            let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let rsa_key_path = manifest_dir.join("suites/vaultwarden/data/rsa_key.pem");

            let mut cmd = Command::new(&binary);
            cmd.env("ROCKET_PORT", port.to_string())
                .env("ROCKET_ADDRESS", "127.0.0.1")
                .env("DATABASE_URL", format!("sqlite://{}", db_path.display()))
                .env("LOG_LEVEL", "warn")
                .env("SIGNUPS_ALLOWED", "true")
                .env("WEB_VAULT_ENABLED", "false")
                .env("EXTENDED_LOGGING", "false")
                .env("LOGIN_RATELIMIT_MAX_BURST", "1000")
                .env("UNAUTHENTICATED_RATELIMIT_MAX_BURST", "1000")
                .env("ADMIN_TOKEN", "testadmin1234567890")
                .stdout(Stdio::null())
                .stderr(Stdio::null());

            if rsa_key_path.is_file() {
                cmd.env("RSA_KEY_FILENAME", &rsa_key_path);
            }

            let child = cmd.spawn()?;
            let base_url = format!("http://127.0.0.1:{}", port);

            let mut instance = Self {
                child: Some(child),
                docker_container_name: None,
                port,
                base_url: base_url.clone(),
                db_path: Some(db_path),
            };

            if let Err(e) = Self::wait_healthy(&base_url).await {
                instance.stop();
                return Err(e);
            }
            return Ok(instance);
        }

        Err("Could not start Vaultwarden. Please install Docker or set VAULTWARDEN_URL / VAULTWARDEN_BIN.".into())
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
        if let Ok(p) = std::env::var("VAULTWARDEN_BIN") {
            let pb = PathBuf::from(p);
            if pb.is_file() {
                return Some(pb);
            }
        }
        if let Ok(output) = Command::new("which").arg("vaultwarden").output() {
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
        let url = format!("{}/alive", base_url);

        while start.elapsed() < timeout {
            if let Ok(res) = client.get(&url).send().await {
                if res.status().is_success() {
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }

        Err(format!("Vaultwarden health check failed at {} within 15s", url).into())
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
        if let Some(db_path) = self.db_path.take() {
            let _ = std::fs::remove_file(&db_path);
            let shm = format!("{}-shm", db_path.display());
            let wal = format!("{}-wal", db_path.display());
            let _ = std::fs::remove_file(shm);
            let _ = std::fs::remove_file(wal);
        }
    }
}

impl Drop for VaultwardenServer {
    fn drop(&mut self) {
        self.stop();
    }
}

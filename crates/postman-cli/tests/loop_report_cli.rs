use std::{
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

struct Server {
    host: String,
    stopped: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Server {
    fn new(states: Vec<&'static str>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let host = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = stopped.clone();
        let thread = thread::spawn(move || {
            let mut states = states.into_iter();
            while !stop.load(Ordering::Relaxed) {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    Err(error) => panic!("accept failed: {error}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                    let mut chunk = [0; 1024];
                    let count = stream.read(&mut chunk).unwrap();
                    assert_ne!(count, 0, "client closed before sending headers");
                    request.extend_from_slice(&chunk[..count]);
                }
                assert!(request.starts_with(b"GET /task "));
                let state = states.next().expect("unexpected extra request");
                let body = format!(r#"{{"state":"{state}"}}"#);
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        Self {
            host,
            stopped,
            thread: Some(thread),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        if let Err(error) = self.thread.take().unwrap().join() {
            if !thread::panicking() {
                std::panic::resume_unwind(error);
            }
        }
    }
}

#[test]
fn cli_outputs_loop_progress_human_summaries_and_clean_json_with_correct_exit_codes() {
    for (states, reason, exit_code) in [
        (vec!["pending", "done"], "condition_met", 0),
        (vec!["pending", "pending", "pending"], "max_iterations", 1),
    ] {
        for json_mode in [false, true] {
            let server = Server::new(states.clone());
            let mut command = Command::new(env!("CARGO_BIN_EXE_postman-g"));
            command
                .env_remove("RUST_LOG")
                .arg("run")
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/poll.http.yml"))
                .arg("--input")
                .arg(format!("host={}", server.host));
            if json_mode {
                command.arg("--json");
            }
            let output = command.output().unwrap();
            let stdout = String::from_utf8(output.stdout).unwrap();
            let stderr = String::from_utf8(output.stderr).unwrap();
            assert_eq!(output.status.code(), Some(exit_code), "{stdout}\n{stderr}");
            if json_mode {
                assert!(stderr.is_empty(), "{stderr}");
                let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
                assert_eq!(report["schema_version"], 1);
                assert_eq!(report["success"], exit_code == 0);
                let file = &report["files"][0]["report"];
                assert_eq!(file["requests"].as_array().unwrap().len(), states.len());
                assert_eq!(file["loops"][0]["reason"], reason);
                assert_eq!(file["loops"][0]["total_executed"], states.len());
                assert_eq!(file["requests"][1]["loop_path"][0]["iteration"], 2);
            } else {
                assert!(
                    stderr.contains("LOOP wait-task [1/3] — running"),
                    "{stderr}"
                );
                assert!(
                    stderr.contains("WAIT wait-task [1/3] — interval 5 ms"),
                    "{stderr}"
                );
                assert!(stderr.contains(reason), "{stderr}");
                assert!(stdout.contains("wait-task[2/3] > query — 200"), "{stdout}");
                assert!(
                    stdout.contains(&format!("LOOP wait-task [{}/3] — {reason}", states.len())),
                    "{stdout}"
                );
                assert!(stdout.contains("PASS iteration 1"), "{stdout}");
                assert!(stdout.contains("(interval 5 ms)"), "{stdout}");
                assert!(
                    stdout.contains(&format!("{0}/{0} request(s) passed", states.len())),
                    "{stdout}"
                );
                assert!(
                    stdout.contains(if exit_code == 0 {
                        "1/1 loop(s) passed"
                    } else {
                        "0/1 loop(s) passed"
                    }),
                    "{stdout}"
                );
            }
        }
    }
}

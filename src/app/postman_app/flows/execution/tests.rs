use super::super::tests::{add, click, create, press, replace, settle, setup};
use super::*;
use gpui::{Entity, InputEvent, TestAppContext, VisualTestContext};
use gpui_kit::test::TestWindowExt;
use postman_flow::{compile_flow, parse_flow_yaml, CompileEnvironment, FlowDocument, FlowInputs};
use postman_http::{
    request::{Request, RequestOptions},
    HttpError, HttpResponse,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

fn document(url: &str) -> FlowDocument {
    parse_flow_yaml(&format!(
        r#"
schema_version: 1
flow:
  name: Native execution
  steps:
    - id: first
      request:
        kind: http
        method: GET
        url: {{ literal: "{url}" }}
      checks:
        - kind: status
          equals: 200
    - id: second
      request:
        kind: http
        method: GET
        url: {{ literal: "{url}" }}
"#
    ))
    .unwrap()
}
fn plan(document: &FlowDocument) -> FlowPlan {
    compile_flow(
        &document.flow,
        &document.apis,
        &CompileEnvironment::default(),
    )
    .unwrap()
}
struct Controlled {
    requests: Arc<AtomicUsize>,
    response: Result<HttpResponse, HttpError>,
    delay: bool,
    dropped: Arc<AtomicUsize>,
}
struct DropCount(Arc<AtomicUsize>);
impl Drop for DropCount {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl HttpTransport for Controlled {
    async fn execute(&self, _: Request, _: RequestOptions) -> Result<HttpResponse, HttpError> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        let _guard = DropCount(self.dropped.clone());
        if self.delay {
            std::future::pending::<()>().await;
        }
        self.response.clone()
    }
}
fn controlled(response: Result<HttpResponse, HttpError>, delay: bool) -> Controlled {
    Controlled {
        requests: Arc::default(),
        dropped: Arc::default(),
        response,
        delay,
    }
}
async fn finish(receiver: &mut mpsc::Receiver<Message>, run: &mut Run) -> Vec<String> {
    let mut text = Vec::new();
    loop {
        let message = tokio::time::timeout(Duration::from_secs(3), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        match message {
            Message::Event(event, observed) => {
                if let Some(event) = run.event(event, observed) {
                    text.push(event.message);
                }
            }
            Message::Tick => {}
            Message::Finished(status) => {
                run.finish(status);
                break;
            }
        }
    }
    text
}

#[tokio::test]
async fn cancellation_drops_the_active_request_before_ack_and_prevents_later_steps() {
    let document = document("http://localhost/test");
    let transport = controlled(Ok(HttpResponse::new(200, vec![], "".into())), true);
    let requests = transport.requests.clone();
    let dropped = transport.dropped.clone();
    let (sender, mut receiver) = mpsc::channel(64);
    let (cancel, cancelled) = oneshot::channel();
    let worker = tokio::spawn(drive(
        plan(&document),
        transport,
        FlowSessionEnvironment::new(FlowInputs::new()),
        cancelled,
        sender,
    ));
    while requests.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }
    cancel.send(()).unwrap();
    let mut run = Run::new(1, 1, &document);
    finish(&mut receiver, &mut run).await;
    worker.await.unwrap();
    assert_eq!(run.status, Status::Cancelled);
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    assert_eq!(run.steps[0].status, Status::Cancelled);
    assert_eq!(run.steps[1].status, Status::NotExecuted);
    assert_eq!(run.results.nodes[0].status, Status::Cancelled);
    assert_eq!(run.results.nodes[1].status, Status::NotExecuted);
    assert!(run.results.nodes[0].http[0].is_some());
    assert!(run.results.nodes[0].http[1].is_none());
    assert!(run.results.nodes[1].http.iter().all(Option::is_none));
    assert!(run.results.outputs_json().is_none());
}

#[tokio::test]
async fn outcomes_are_real_and_raw_failures_or_response_values_do_not_reach_feedback() {
    let document = document("http://localhost/test");
    for (response, status, expected) in [
        (
            Ok(HttpResponse::new(200, vec![], "secret-response".into())),
            Status::Succeeded,
            "HTTP 200",
        ),
        (
            Ok(HttpResponse::new(503, vec![], "secret-response".into())),
            Status::Failed,
            "Response checks failed",
        ),
        (
            Err(HttpError::Timeout { timeout_ms: 10 }),
            Status::Failed,
            "Request timed out",
        ),
        (
            Err(HttpError::network(
                "Bearer secret-runtime https://user:pass@test",
            )),
            Status::Failed,
            "Network request failed",
        ),
    ] {
        let (sender, mut receiver) = mpsc::channel(64);
        let (_cancel, cancelled) = oneshot::channel();
        let worker = tokio::spawn(drive(
            plan(&document),
            controlled(response, false),
            FlowSessionEnvironment::new(FlowInputs::new()),
            cancelled,
            sender,
        ));
        let mut run = Run::new(1, 1, &document);
        let text = finish(&mut receiver, &mut run).await.join("\n");
        worker.await.unwrap();
        assert_eq!(run.status, status);
        assert!(text.contains(expected), "{text}");
        for secret in ["secret-response", "secret-runtime", "user:pass"] {
            assert!(!text.contains(secret));
        }
    }
}

fn install(
    flows: &Entity<FlowsView>,
    document: FlowDocument,
    cx: &mut VisualTestContext,
) -> FlowId {
    let id = cx.update(|w, cx| {
        flows.update(cx, |v, cx| {
            let id = v.library.insert(document);
            v.sync_fields(w, cx);
            v.sync_picker(w, cx);
            cx.notify();
            id
        })
    });
    settle(cx);
    id
}
fn status(flows: &Entity<FlowsView>, flow: FlowId, cx: &mut VisualTestContext) -> Option<Status> {
    flows.read_with(cx, |v, _| v.execution.runs.get(&flow).map(|r| r.status))
}

#[gpui_kit::test]
fn toolbar_and_console_execute_real_http_and_rerun_with_fresh_results(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let mock = server
        .mock("GET", "/flow")
        .with_status(200)
        .with_body("secret-body")
        .expect(4)
        .create();
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    click(cx, "nav-flows");
    let id = install(&flows, document(&format!("{}/flow", server.url())), cx);
    let before = flows.read_with(cx, |v, _| v.library.get(id).unwrap().document().clone());
    click(cx, "flow-run");
    settle(cx);
    assert_eq!(status(&flows, id, cx), Some(Status::Succeeded));
    click(cx, "console-copy");
    let copied = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    assert!(copied.contains("HTTP 200"));
    assert!(!copied.contains("secret-body"));
    replace(cx, "console-command", "clear");
    press(cx, "enter");
    settle(cx);
    assert_eq!(status(&flows, id, cx), Some(Status::Succeeded));
    replace(cx, "console-command", "run");
    press(cx, "enter");
    settle(cx);
    flows.read_with(cx, |v, _| {
        let run = &v.execution.runs[&id];
        assert_eq!(run.id, 2);
        assert_eq!(run.status, Status::Succeeded);
        assert!(run.steps.iter().all(|s| s.status == Status::Succeeded));
        assert_eq!(*v.library.get(id).unwrap().document(), before);
        assert!(v.execution.tasks.is_empty());
    });
    mock.assert();
}

#[gpui_kit::test]
fn native_toolbar_reports_check_failure_timeout_and_network_failure(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let failed_check = server
        .mock("GET", "/failed-check")
        .with_status(503)
        .with_body("private-response")
        .expect(1)
        .create();
    let held = HeldServer::new();
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    click(cx, "nav-flows");
    for (url, expected) in [
        (
            format!("{}/failed-check", server.url()),
            "Response checks failed.",
        ),
        (held.url.clone(), "Request timed out."),
        (
            "http://127.0.0.1:0/private-url".into(),
            "Network request failed.",
        ),
    ] {
        let mut doc = document(&url);
        let postman_flow::HttpRequestSource::Inline(request) = &mut doc.flow.steps[0].request
        else {
            unreachable!()
        };
        if url == held.url {
            request.options.timeout_ms = Some(100);
        }
        let id = install(&flows, doc, cx);
        click(cx, "flow-run");
        settle(cx);
        flows.read_with(cx, |v, _| {
            let run = &v.execution.runs[&id];
            assert_eq!(run.status, Status::Failed);
            assert_eq!(run.steps[0].status, Status::Failed);
            assert_eq!(run.steps[1].status, Status::NotExecuted);
            assert!(v.execution.tasks.is_empty());
        });
        click(cx, "console-copy");
        let copied = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
        assert!(copied.contains(expected), "{copied}");
        assert!(!copied.contains("private-response"));
        assert!(!copied.contains("private-url"));
    }
    failed_check.assert();
}

#[gpui_kit::test]
fn invalid_hidden_drafts_and_runtime_inputs_block_network(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let mock = server.mock("GET", "/blocked").expect(0).create();
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    click(cx, "nav-flows");
    let mut doc = document(&format!("{}/blocked", server.url()));
    doc.flow
        .inputs
        .push(postman_flow::FlowInputSpec::required("required_token").sensitive());
    let id = install(&flows, doc, cx);
    click(cx, "flow-run");
    settle(cx);
    assert_eq!(status(&flows, id, cx), None);
    click(cx, "console-copy");
    assert!(cx
        .update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap())
        .contains("Required input"));
    create(cx, "Invalid draft");
    add(cx);
    replace(cx, "flow-step-url", "{");
    add(cx); // Previous inspector remains retained while hidden.
    click(cx, "flow-run");
    settle(cx);
    flows.read_with(cx, |v, _| assert!(v.execution.tasks.is_empty()));
    mock.assert();
}

#[gpui_kit::test]
fn cancel_then_switch_modes_and_rerun_rejects_late_events(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let mock = server.mock("GET", "/fast").with_status(200).create();
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    click(cx, "nav-flows");
    let id = install(&flows, document(&format!("{}/fast", server.url())), cx);
    click(cx, "flow-run"); // No settle: cancel the owned session before the first UI batch.
    click(cx, "flow-run");
    click(cx, "nav-http");
    settle(cx);
    assert_eq!(status(&flows, id, cx), Some(Status::Cancelled));
    click(cx, "nav-flows");
    click(cx, "flow-run");
    settle(cx);
    assert_eq!(status(&flows, id, cx), Some(Status::Succeeded));
    cx.update(|w, cx| {
        flows.update(cx, |v, cx| {
            assert!(!v.apply_run_batch(id, 1, vec![Message::Finished(Status::Failed)], w, cx));
        })
    });
    assert_eq!(status(&flows, id, cx), Some(Status::Succeeded));
    drop(mock);
}

fn polling(interval: u64) -> FlowDocument {
    parse_flow_yaml(&format!(
        r#"
schema_version: 1
flow:
  name: polling
  steps:
    - id: poll
      kind: repeat_until
      max_iterations: 2
      interval_ms: {interval}
      timeout_ms: 60000
      until: {{ eq: [{{ literal: false }}, {{ literal: true }}] }}
      steps:
        - id: fetch
          request:
            kind: http
            method: GET
            url: {{ literal: "http://localhost/poll" }}
"#
    ))
    .unwrap()
}

#[tokio::test]
async fn cancel_polling_wait_and_report_iteration_limit_truthfully() {
    for cancelled_run in [true, false] {
        let document = polling(if cancelled_run { 30000 } else { 1 });
        let transport = controlled(Ok(HttpResponse::new(200, vec![], "".into())), false);
        let count = transport.requests.clone();
        let (sender, mut receiver) = mpsc::channel(64);
        let (cancel, cancelled) = oneshot::channel();
        let worker = tokio::spawn(drive(
            plan(&document),
            transport,
            FlowSessionEnvironment::new(FlowInputs::new()),
            cancelled,
            sender,
        ));
        let mut run = Run::new(1, 1, &document);
        if cancelled_run {
            loop {
                let message = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
                    .await
                    .unwrap()
                    .unwrap();
                if let Message::Event(event, observed) = message {
                    let waiting = matches!(event, FlowEvent::LoopWaiting { .. });
                    run.event(event, observed);
                    if waiting {
                        break;
                    }
                }
            }
            assert_eq!(run.steps[0].status, Status::Waiting);
            cancel.send(()).unwrap();
        }
        let logs = finish(&mut receiver, &mut run).await.join("\n");
        worker.await.unwrap();
        assert_eq!(
            run.status,
            if cancelled_run {
                Status::Cancelled
            } else {
                Status::Failed
            }
        );
        assert_eq!(
            count.load(Ordering::SeqCst),
            if cancelled_run { 1 } else { 2 }
        );
        assert!(run.results.outputs_json().is_none());
        assert_eq!(run.results.nodes[0].status, run.status);
        if cancelled_run {
            assert_eq!(run.results.nodes[0].executed, Some(1));
            assert!(run.results.nodes[0].detail.contains("Cancelled"));
        } else {
            assert!(logs.contains("Iteration limit reached"), "{logs}");
            assert_eq!(run.results.nodes[0].executed, Some(2));
        }
    }
}

#[tokio::test]
async fn cancellation_is_responsive_even_when_the_ui_mailbox_is_full() {
    let document = document("http://localhost/full");
    let transport = controlled(Ok(HttpResponse::new(200, vec![], "".into())), false);
    let count = transport.requests.clone();
    let (sender, mut receiver) = mpsc::channel(1);
    let (cancel, cancelled) = oneshot::channel();
    let worker = tokio::spawn(drive(
        plan(&document),
        transport,
        FlowSessionEnvironment::new(FlowInputs::new()),
        cancelled,
        sender,
    ));
    tokio::time::sleep(Duration::from_millis(10)).await;
    cancel.send(()).unwrap();
    let mut run = Run::new(1, 1, &document);
    finish(&mut receiver, &mut run).await;
    worker.await.unwrap();
    assert_eq!(run.status, Status::Cancelled);
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn skipped_steps_and_sensitive_outputs_have_no_false_success_or_value_projection() {
    let mut document = document("http://localhost/test");
    document
        .flow
        .inputs
        .push(postman_flow::FlowInputSpec::with_default("token", "secret-output").sensitive());
    document.flow.outputs.push(postman_flow::FlowOutputSpec {
        name: "token".into(),
        value: postman_flow::ValueReference::input("token"),
    });
    document.flow.steps[0].when = Some(postman_flow::ConditionExpr::eq(
        postman_flow::JsonTemplate::literal(false),
        postman_flow::JsonTemplate::literal(true),
    ));
    let mut run = Run::new(1, 4, &document);
    let (sender, mut receiver) = mpsc::channel(64);
    let (_cancel, cancelled) = oneshot::channel();
    let worker = tokio::spawn(drive(
        plan(&document),
        controlled(Ok(HttpResponse::new(200, vec![], String::new())), false),
        FlowSessionEnvironment::new(FlowInputs::new()),
        cancelled,
        sender,
    ));
    let logs = finish(&mut receiver, &mut run).await.join("\n");
    worker.await.unwrap();
    assert!(!logs.contains("secret-output"));
    assert_eq!(run.steps[0].status, Status::Skipped);
    assert_eq!(run.steps[1].status, Status::Succeeded);
    assert_eq!(run.output_count, 1);
    assert!(run.label(5).contains("Earlier revision"));
    let next = Run::new(2, 5, &document);
    assert_eq!(next.output_count, 0);
    assert!(next.steps.iter().all(|s| s.status == Status::NotExecuted));
}

// Hold the real request open without running the GPUI executor. Only the test's release
// or teardown may finish this server; CI setup/rendering must not consume a wall-clock lease.
struct HeldServer {
    url: String,
    release: Option<oneshot::Sender<()>>,
    started: std::sync::mpsc::Receiver<()>,
    shutdown: Option<oneshot::Sender<()>>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl HeldServer {
    fn new() -> Self {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/held", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let (release, released) = oneshot::channel();
        let (shutdown, shutdown_requested) = oneshot::channel();
        let (started, request_started) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                let serve = async move {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let mut request = Vec::new();
                    loop {
                        let mut bytes = [0; 4096];
                        let count = stream.read(&mut bytes).await.unwrap_or(0);
                        if count == 0 {
                            return;
                        }
                        request.extend_from_slice(&bytes[..count]);
                        if request.windows(4).any(|chunk| chunk == b"\r\n\r\n") {
                            break;
                        }
                    }
                    let _ = started.send(());
                    if released.await.is_ok() {
                        let _ = stream.write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        ).await;
                    }
                };
                tokio::select! {
                    biased;
                    _ = shutdown_requested => {},
                    _ = serve => {},
                }
            });
        });
        Self {
            url,
            release: Some(release),
            started: request_started,
            shutdown: Some(shutdown),
            worker: Some(worker),
        }
    }
    fn wait_for_request(&self) {
        self.started
            .recv_timeout(Duration::from_secs(10))
            .expect("the real request should reach the held server before UI interactions");
    }
    fn release(&mut self) {
        self.release.take().unwrap().send(()).unwrap();
    }
}
impl Drop for HeldServer {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[test]
fn held_server_can_be_released_before_the_request_arrives() {
    use std::io::{Read, Write};
    let mut server = HeldServer::new();
    server.release();
    let address = server
        .url
        .strip_prefix("http://")
        .unwrap()
        .strip_suffix("/held")
        .unwrap();
    let mut stream = std::net::TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream
        .write_all(b"GET /held HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .unwrap();
    server.wait_for_request();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
}

#[gpui_kit::test]
fn clean_running_close_cancel_preserves_session_then_confirm_waits_for_shutdown(
    cx: &mut TestAppContext,
) {
    let server = HeldServer::new();
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    click(cx, "nav-flows");
    let doc = document(&server.url);
    let id = install(&flows, doc.clone(), cx);
    let directory = tempfile::tempdir().unwrap();
    cx.update(|_, cx| {
        flows.update(cx, |v, _| {
            v.library
                .associate(id, directory.path().join("saved.http.yml"), vec![], doc);
        })
    });
    click(cx, "flow-run");
    server.wait_for_request();
    press(cx, "secondary-w");
    assert!(cx.debug_bounds("flow-stop-close-dialog").is_some());
    click(cx, "flow-stop-close-cancel");
    flows.read_with(cx, |v, _| {
        assert!(v.running(id));
        assert!(v.library.get(id).is_some());
    });
    press(cx, "secondary-w");
    click(cx, "flow-stop-close-confirm");
    settle(cx);
    flows.read_with(cx, |v, _| {
        assert!(v.library.get(id).is_none());
        assert!(!v.running(id));
        assert!(!v.execution.runs.contains_key(&id));
    });
}

#[gpui_kit::test]
fn dirty_running_close_cancel_and_save_failure_keep_execution(cx: &mut TestAppContext) {
    let server = HeldServer::new();
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    click(cx, "nav-flows");
    let id = install(&flows, document(&server.url), cx);
    click(cx, "flow-run");
    server.wait_for_request();
    press(cx, "secondary-w");
    assert!(cx.debug_bounds("flow-close-dialog").is_some());
    click(cx, "flow-close-cancel");
    flows.read_with(cx, |v, _| {
        assert!(v.running(id));
        assert!(v.library.get(id).unwrap().is_dirty());
    });
    // Closing an invalid hidden draft must not bypass protection or stop the running snapshot.
    click(cx, "flow-url-expression");
    let field = cx.debug_bounds("flow-url-source").unwrap();
    let position = field.origin + gpui::point(gpui::px(12.), gpui::px(12.));
    cx.update(|w, cx| {
        w.dispatch_event(
            gpui::MouseDownEvent {
                position,
                button: gpui::MouseButton::Left,
                modifiers: gpui::Modifiers::none(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        w.dispatch_event(
            gpui::MouseUpEvent {
                position,
                button: gpui::MouseButton::Left,
                modifiers: gpui::Modifiers::none(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
    });
    press(cx, "secondary-a");
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("{unfinished".into()));
    press(cx, "secondary-v");
    press(cx, "secondary-w");
    click(cx, "flow-close-save");
    assert!(cx.debug_bounds("flow-close-error").is_some());
    flows.read_with(cx, |v, _| assert!(v.running(id)));
    click(cx, "flow-close-cancel");
    click(cx, "flow-run");
    settle(cx);
    assert_eq!(status(&flows, id, cx), Some(Status::Cancelled));
}

#[gpui_kit::test]
fn native_run_prepares_environment_and_typed_inputs_executes_api_and_shares_http_cookies(
    cx: &mut TestAppContext,
) {
    let mut server = mockito::Server::new();
    let api = server
        .mock("POST", "/typed")
        .match_body(mockito::Matcher::Json(
            serde_json::json!({"nothing":null,"zero":0,"flag":false,"text":""}),
        ))
        .with_status(200)
        .with_header("set-cookie", "flow_session=from-flow; Path=/")
        .create();
    let http = server
        .mock("GET", "/cookie")
        .match_header("cookie", "flow_session=from-flow")
        .with_status(200)
        .create();
    let doc = parse_flow_yaml(
        r#"
schema_version: 1
apis:
  typed:
    parameters: [base_url, payload]
    request:
      method: POST
      url: { concat: [{ input: base_url }, { literal: /typed }] }
      body: { kind: json, value: { input: payload } }
flow:
  name: typed API
  inputs:
    - name: base_url
      default: http://127.0.0.1:1/must-not-be-used
    - name: payload
      default: { nothing: null, zero: 0, flag: false, text: "" }
      sensitive: true
  steps:
    - id: call
      request:
        kind: api
        api: typed
        bindings:
          base_url: { input: base_url }
          payload: { input: payload }
      checks: [{ kind: status, equals: 200 }]
"#,
    )
    .unwrap();
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    click(cx, "nav-flows");
    let id = install(&flows, doc.clone(), cx);
    click(cx, "environment-settings");
    cx.simulate_input("Local execution");
    press(cx, "tab");
    cx.simulate_input(&server.url());
    click(cx, "environment-save");
    settle(cx);
    click(cx, "flow-run");
    settle(cx);
    assert_eq!(status(&flows, id, cx), Some(Status::Succeeded));
    flows.read_with(cx, |v, _| {
        assert_eq!(*v.library.get(id).unwrap().document(), doc)
    });
    api.assert();
    click(cx, "nav-http");
    replace(cx, "request-url-input", &format!("{}/cookie", server.url()));
    click(cx, "send-button");
    settle(cx);
    http.assert();
}

#[gpui_kit::test]
fn repeated_console_run_and_quit_cancel_preserve_one_owned_run(cx: &mut TestAppContext) {
    let server = HeldServer::new();
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    click(cx, "nav-http");
    replace(cx, "request-url-input", "http://127.0.0.1:1/unsaved");
    click(cx, "nav-flows");
    let id = install(&flows, document(&server.url), cx);
    click(cx, "flow-run");
    server.wait_for_request();
    cx.update(|w, cx| {
        w.click("console-command", cx);
        w.input("run", cx);
        w.press("enter", cx);
    });
    flows.read_with(cx, |v, _| {
        assert_eq!(v.execution.next_id, 1);
        assert_eq!(v.execution.tasks.len(), 1);
    });
    cx.update(|w, cx| w.dispatch_action(Box::new(crate::app::Quit), cx));
    assert!(cx.debug_bounds("unsaved-request-dialog").is_some());
    click(cx, "request-save-discard");
    assert!(cx.debug_bounds("flow-close-dialog").is_some());
    click(cx, "flow-close-discard");
    assert!(cx.debug_bounds("flow-stop-close-dialog").is_some());
    click(cx, "flow-stop-close-cancel");
    flows.read_with(cx, |v, _| {
        assert!(v.running(id));
        assert!(v.library.get(id).is_some());
    });
    click(cx, "flow-run");
    settle(cx);
    assert_eq!(status(&flows, id, cx), Some(Status::Cancelled));
}

#[gpui_kit::test]
fn editing_a_later_step_does_not_change_the_running_plan(cx: &mut TestAppContext) {
    let mut held = HeldServer::new();
    let mut server = mockito::Server::new();
    let original = server
        .mock("GET", "/original")
        .with_status(200)
        .expect(1)
        .create();
    let edited = server.mock("GET", "/edited").expect(0).create();
    let doc = document(&held.url);
    let source = postman_flow::write_flow_yaml(&doc).unwrap();
    let split = source.rfind(&held.url).unwrap();
    let source = format!(
        "{}{}{}",
        &source[..split],
        format_args!("{}/original", server.url()),
        &source[split + held.url.len()..]
    );
    let doc = parse_flow_yaml(&source).unwrap();
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    click(cx, "nav-flows");
    let id = install(&flows, doc, cx);
    click(cx, "flow-run");
    held.wait_for_request();
    cx.update(|w, cx| {
        w.click("flow-node-1-second", cx);
        w.click("flow-step-url", cx);
        w.press("secondary-a", cx);
        w.input(&format!("{}/edited", server.url()), cx);
    });
    held.release();
    settle(cx);
    assert_eq!(status(&flows, id, cx), Some(Status::Succeeded));
    flows.read_with(cx, |v, _| {
        assert_ne!(
            v.execution.runs[&id].revision,
            v.library.get(id).unwrap().revision()
        )
    });
    original.assert();
    edited.assert();
    click(cx, "console-tab-steps");
    for theme in [
        gpui_kit::component::ThemeMode::Light,
        gpui_kit::component::ThemeMode::Dark,
    ] {
        cx.update(|_, cx| crate::ui::theme::apply(theme, cx));
        for (width, height) in [
            (1440., 960.),
            (1920., 1080.),
            (1024., 768.),
            (768., 720.),
            (1024., 600.),
        ] {
            let handle = cx.update(|w, _| w.window_handle());
            cx.simulate_window_resize(handle, gpui::size(gpui::px(width), gpui::px(height)));
            settle(cx);
            for selector in [
                "flow-run",
                "flow-run-status",
                "console-content",
                "console-command-bar",
            ] {
                let bounds = cx.debug_bounds(selector).unwrap();
                assert!(
                    bounds.left() >= gpui::px(0.)
                        && bounds.right() <= gpui::px(width + 1.)
                        && bounds.bottom() <= gpui::px(height + 1.),
                    "{selector}: {bounds:?}"
                );
            }
        }
    }
}

#[gpui_kit::test]
fn invalid_runtime_json_is_not_a_stale_default_and_does_not_fail_document_check(
    cx: &mut TestAppContext,
) {
    let mut server = mockito::Server::new();
    let request = server.mock("GET", "/blocked").expect(0).create();
    let mut doc = document(&format!("{}/blocked", server.url()));
    doc.flow
        .inputs
        .push(postman_flow::FlowInputSpec::with_default(
            "payload",
            serde_json::Value::Null,
        ));
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    click(cx, "nav-flows");
    let id = install(&flows, doc, cx);
    click(cx, "flow-inputs");
    cx.update(|w, cx| w.click(("flow-input-override", 0_u64), cx));
    settle(cx);
    let field = cx.debug_bounds("flow-input-runtime-0").unwrap();
    cx.simulate_click(
        field.origin + gpui::point(gpui::px(12.), gpui::px(12.)),
        gpui::Modifiers::none(),
    );
    press(cx, "secondary-a");
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("{secret-invalid".into()));
    press(cx, "secondary-v");
    click(cx, "flow-node-1-first");
    click(cx, "flow-check");
    click(cx, "flow-run");
    settle(cx);
    assert_eq!(status(&flows, id, cx), None);
    click(cx, "console-copy");
    let text = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    assert!(text.contains("Check passed"));
    assert!(text.contains("Run blocked"));
    assert!(!text.contains("secret-invalid"));
    request.assert();
}

mod results;

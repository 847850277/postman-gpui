use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
    sync::{Arc, Mutex},
};

use postman_cli::{run_flow, run_flow_with_progress, LoopProgress};
use postman_flow::LoopFinishReason;
use postman_http::{
    request::{Request, RequestOptions},
    HttpError, HttpResponse, HttpTransport,
};
use serde_json::{json, Value};

const POLL: &str = include_str!("fixtures/poll.http.yml");
const NESTED: &str = include_str!("fixtures/nested.http.yml");

struct FakeTransport {
    responses: Mutex<VecDeque<Result<HttpResponse, HttpError>>>,
    trace: Arc<Mutex<Vec<String>>>,
}

impl FakeTransport {
    fn new(responses: impl IntoIterator<Item = Result<HttpResponse, HttpError>>) -> Self {
        Self {
            responses: Mutex::new(responses.into_iter().collect()),
            trace: Arc::default(),
        }
    }
}

impl HttpTransport for FakeTransport {
    async fn execute(&self, _: Request, _: RequestOptions) -> Result<HttpResponse, HttpError> {
        self.trace.lock().unwrap().push("HTTP request".into());
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected HTTP request")
    }
}

fn response(value: Value) -> Result<HttpResponse, HttpError> {
    let mut response = HttpResponse::new(200, vec![], value.to_string());
    response.elapsed_ms = 7;
    Ok(response)
}

#[tokio::test]
async fn reports_poll_iterations_timing_waits_and_live_progress() {
    let transport = FakeTransport::new([
        response(json!({"state":"pending"})),
        response(json!({"state":"pending"})),
        response(json!({"state":"done"})),
    ]);
    let trace = transport.trace.clone();
    let report = run_flow_with_progress(
        transport,
        Path::new("poll.http.yml"),
        POLL,
        &BTreeMap::new(),
        RequestOptions::default(),
        |event| {
            trace.lock().unwrap().push(event.to_string());
        },
    )
    .await
    .unwrap();
    assert!(report.success);
    assert_eq!(report.requests.len(), 3);
    assert_eq!(report.loops.len(), 1);
    let summary = &report.loops[0];
    assert_eq!(summary.reason, Some(LoopFinishReason::ConditionMet));
    assert_eq!(summary.total_executed, 3);
    assert_eq!(summary.iterations.len(), 3);
    assert!(summary.iterations.iter().all(|iteration| iteration.success));
    assert_eq!(summary.iterations[0].wait_interval_ms, Some(5));
    assert!(summary.iterations[0].wait_elapsed_ms.is_some());
    assert_eq!(summary.iterations[2].wait_interval_ms, None);
    assert!(
        summary.elapsed_ms
            >= summary
                .iterations
                .iter()
                .filter_map(|i| i.wait_elapsed_ms)
                .sum::<u128>()
    );
    for (index, request) in report.requests.iter().enumerate() {
        assert_eq!(request.status, Some(200));
        assert_eq!(request.elapsed_ms, Some(7));
        assert_eq!(request.captures, ["state"]);
        assert_eq!(request.assertions.len(), 1);
        assert_eq!(request.loop_path[0].iteration, index + 1);
        assert_eq!(request.loop_path[0].step_id, "wait-task");
    }
    let trace = trace.lock().unwrap();
    assert!(trace[0].starts_with("LOOP wait-task [1/3]"));
    assert_eq!(trace[1], "HTTP request");
    assert!(trace[2].starts_with("WAIT wait-task [1/3]"));
    assert!(trace[3].starts_with("LOOP wait-task [2/3]"));
    assert!(trace
        .last()
        .unwrap()
        .starts_with("PASS LOOP wait-task [3/3] — condition_met"));
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["loops"][0]["kind"], "repeat_until");
    assert_eq!(json["loops"][0]["reason"], "condition_met");
}

#[tokio::test]
async fn preserves_loop_failure_reasons_even_when_http_requests_succeed() {
    let cases = [
        (
            LoopFinishReason::MaxIterations,
            "max_iterations",
            vec![
                response(json!({"state":"pending"})),
                response(json!({"state":"pending"})),
                response(json!({"state":"pending"})),
            ],
        ),
        (
            LoopFinishReason::FailureCondition,
            "failure_condition",
            vec![response(json!({"state":"failed"}))],
        ),
        (
            LoopFinishReason::Cancelled,
            "cancelled",
            vec![Err(HttpError::Cancelled)],
        ),
        (
            LoopFinishReason::StepFailed,
            "step_failed",
            vec![Ok(HttpResponse::new(500, vec![], "{}".into()))],
        ),
    ];
    for (reason, reason_name, responses) in cases {
        let expected_requests = responses.len();
        let report = run_flow(
            FakeTransport::new(responses),
            Path::new("poll.http.yml"),
            POLL,
            &BTreeMap::new(),
            RequestOptions::default(),
        )
        .await
        .unwrap();
        assert!(!report.success);
        assert_eq!(report.requests.len(), expected_requests);
        assert_eq!(report.loops[0].reason, Some(reason));
        assert_eq!(report.loops[0].total_executed, expected_requests);
        assert!(report.loops[0].error.is_some());
        assert!(report.loops[0].summary().contains(reason_name));
        if matches!(
            reason,
            LoopFinishReason::MaxIterations | LoopFinishReason::FailureCondition
        ) {
            assert!(report
                .requests
                .iter()
                .all(|request| request.success && request.status == Some(200)));
        }
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["loops"][0]["reason"], reason_name);
    }
}

#[tokio::test]
async fn records_timeout_and_wait_cut_short_by_deadline() {
    let source = POLL
        .replace("interval_ms: 5", "interval_ms: 1000")
        .replace("timeout_ms: 5000", "timeout_ms: 100");
    let report = run_flow(
        FakeTransport::new([response(json!({"state":"pending"}))]),
        Path::new("timeout.http.yml"),
        &source,
        &BTreeMap::new(),
        RequestOptions::default(),
    )
    .await
    .unwrap();
    assert!(!report.success);
    assert_eq!(report.loops[0].reason, Some(LoopFinishReason::Timeout));
    assert_eq!(report.loops[0].total_executed, 1);
    assert!(report.requests[0].success);
    let iteration = &report.loops[0].iterations[0];
    assert_eq!(iteration.wait_interval_ms, Some(1000));
    assert!(iteration.wait_elapsed_ms.is_some());
    assert!(report.loops[0].elapsed_ms >= iteration.wait_elapsed_ms.unwrap());
}

#[tokio::test]
async fn nested_invocations_keep_distinct_paths_and_skipped_loops_are_not_requests() {
    let report = run_flow(
        FakeTransport::new([
            response(json!({"state":"pending"})),
            response(json!({"state":"done"})),
            response(json!({"state":"done"})),
        ]),
        Path::new("nested.http.yml"),
        NESTED,
        &BTreeMap::new(),
        RequestOptions::default(),
    )
    .await
    .unwrap();
    assert!(report.success);
    assert_eq!(report.requests.len(), 3);
    assert_eq!(report.loops.len(), 4);
    assert_eq!(report.loops[0].reason, Some(LoopFinishReason::Completed));
    assert_eq!(report.loops[0].iterations.len(), 3);
    assert_eq!(report.loops[1].loop_path[0].iteration, 1);
    assert_eq!(report.loops[1].total_executed, 2);
    assert!(report.loops[2].skipped);
    assert_eq!(report.loops[2].reason, None);
    assert_eq!(report.loops[2].loop_path[0].iteration, 2);
    assert_eq!(report.loops[3].loop_path[0].iteration, 3);
    assert_eq!(report.loops[3].total_executed, 1);
    assert_eq!(
        report.requests[1].display_name(),
        "outer[1/3] > inner[2/3] > query"
    );
    assert_eq!(
        report.requests[2].display_name(),
        "outer[3/3] > inner[1/3] > query"
    );
    assert!(report
        .requests
        .iter()
        .all(|request| request.captures == ["state"]));
}

#[tokio::test]
async fn nested_cancellation_retains_inner_and_outer_failure_summaries() {
    let report = run_flow(
        FakeTransport::new([Err(HttpError::Cancelled)]),
        Path::new("nested.http.yml"),
        NESTED,
        &BTreeMap::new(),
        RequestOptions::default(),
    )
    .await
    .unwrap();
    assert!(!report.success);
    assert_eq!(report.requests.len(), 1);
    assert_eq!(report.loops.len(), 2);
    for loop_report in &report.loops {
        assert_eq!(loop_report.reason, Some(LoopFinishReason::Cancelled));
        assert!(!loop_report.success);
        assert_eq!(loop_report.total_executed, 1);
    }
}

#[tokio::test]
async fn collected_exports_belong_to_loop_and_sensitive_values_stay_redacted() {
    let source = r#"
schema_version: 1
flow:
  name: loop captures
  steps:
    - id: batch
      kind: for_each
      items: { literal: [1, 2] }
      as: item
      steps:
        - id: fetch
          request: { kind: http, method: GET, url: { literal: 'https://example.test' } }
          exports: [{ name: token, path: $.token, sensitive: true }]
      exports:
        - name: tokens
          collect: { step: fetch, name: token }
  outputs:
    - name: tokens
      value: { output: { step: batch, name: tokens } }
"#;
    let mut progress = Vec::new();
    let report = run_flow_with_progress(
        FakeTransport::new([
            response(json!({"token":"fake-secret-1"})),
            response(json!({"token":"fake-secret-2"})),
        ]),
        Path::new("captures.http.yml"),
        source,
        &BTreeMap::new(),
        RequestOptions::default(),
        |event| progress.push(event.to_string()),
    )
    .await
    .unwrap();
    assert_eq!(report.loops[0].captures, ["tokens"]);
    assert_eq!(report.requests[0].captures, ["token"]);
    assert_eq!(report.outputs["tokens"], "[REDACTED]");
    assert_eq!(report.redacted_outputs, ["tokens"]);
    assert!(!serde_json::to_string(&report)
        .unwrap()
        .contains("fake-secret"));
    assert!(!progress.join("\n").contains("fake-secret"));
}

#[tokio::test]
async fn early_loop_errors_and_empty_loops_have_reports_without_fake_requests() {
    let source = r#"
schema_version: 1
flow:
  name: early loop outcomes
  steps:
    - id: batch
      kind: for_each
      items: { literal: [] }
      as: item
      steps:
        - id: fetch
          request: { kind: http, method: GET, url: { literal: 'https://example.test' } }
"#;
    for (source, expected) in [
        (source.to_owned(), LoopFinishReason::Completed),
        (
            source.replace("literal: []", "literal: 42"),
            LoopFinishReason::StepFailed,
        ),
        (
            source.replace(
                "kind: for_each",
                "kind: for_each\n      when: { gt: [{ literal: {} }, { literal: 0 }] }",
            ),
            LoopFinishReason::StepFailed,
        ),
    ] {
        let mut finished = 0;
        let report = run_flow_with_progress(
            FakeTransport::new([]),
            Path::new("early.http.yml"),
            &source,
            &BTreeMap::new(),
            RequestOptions::default(),
            |event| {
                if matches!(event, LoopProgress::Finished(_)) {
                    finished += 1;
                }
            },
        )
        .await
        .unwrap();
        assert!(report.requests.is_empty());
        assert_eq!(report.loops.len(), 1);
        assert_eq!(report.loops[0].reason, Some(expected));
        assert_eq!(report.loops[0].total_executed, 0);
        assert_eq!(finished, 1);
    }
}

#[tokio::test]
async fn flat_reports_keep_existing_json_shape() {
    let source = r#"
schema_version: 1
flow:
  name: flat
  steps:
    - id: fetch
      request: { kind: http, method: GET, url: { literal: 'https://example.test' } }
"#;
    let report = run_flow(
        FakeTransport::new([response(json!({}))]),
        Path::new("flat.http.yml"),
        source,
        &BTreeMap::new(),
        RequestOptions::default(),
    )
    .await
    .unwrap();
    assert!(report.success);
    let json = serde_json::to_value(&report).unwrap();
    assert!(json.get("loops").is_none());
    assert!(json["requests"][0].get("loop_path").is_none());
    assert_eq!(report.requests[0].display_name(), "fetch");
}

use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
    sync::Mutex,
};

use postman_cli::{run_flow, RunReport};
use postman_flow::LoopFinishReason;
use postman_http::{
    request::{Request, RequestOptions},
    HttpError, HttpResponse, HttpTransport,
};
use serde_json::{json, Value};

const INGEST: &str = include_str!("../suites/meilisearch/flows/documents_ingestion.http.yml");
const CLEANUP: &str = include_str!("../suites/meilisearch/flows/documents_crud_lifecycle.http.yml");

struct ScriptedTransport(Mutex<VecDeque<(&'static str, HttpResponse)>>);

impl HttpTransport for ScriptedTransport {
    async fn execute(
        &self,
        request: Request,
        _: RequestOptions,
    ) -> Result<HttpResponse, HttpError> {
        let (path, response) = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected request after task failure");
        assert_eq!(request.url, format!("http://127.0.0.1:7700{path}"));
        Ok(response)
    }
}

fn response(status: u16, body: Value) -> HttpResponse {
    HttpResponse::new(status, vec![], body.to_string())
}

async fn run(source: &str, responses: Vec<(&'static str, HttpResponse)>) -> RunReport {
    run_flow(
        ScriptedTransport(Mutex::new(responses.into())),
        Path::new("meilisearch.http.yml"),
        source,
        &BTreeMap::new(),
        RequestOptions::default(),
    )
    .await
    .unwrap()
}

fn ingest_prefix() -> Vec<(&'static str, HttpResponse)> {
    vec![
        ("/health", response(200, json!({"status":"available"}))),
        (
            "/indexes",
            response(202, json!({"taskUid":73,"status":"enqueued"})),
        ),
    ]
}

#[tokio::test]
async fn failed_and_canceled_tasks_stop_before_document_upload() {
    for status in ["failed", "canceled"] {
        let mut responses = ingest_prefix();
        responses.push(("/tasks/73", response(200, json!({"status":status}))));
        let report = run(INGEST, responses).await;
        assert!(!report.success);
        assert_eq!(report.requests.len(), 3);
        assert_eq!(
            report.loops[0].reason,
            Some(LoopFinishReason::FailureCondition)
        );
    }
}

#[tokio::test]
async fn pending_task_is_bounded_by_deadline_and_iteration_limit() {
    for (source, reason) in [
        (
            INGEST
                .replace("timeout_ms: 300000", "timeout_ms: 100")
                .replace("interval_ms: 100", "interval_ms: 1000"),
            LoopFinishReason::Timeout,
        ),
        (
            INGEST.replace("max_iterations: 3000", "max_iterations: 1"),
            LoopFinishReason::MaxIterations,
        ),
    ] {
        let mut responses = ingest_prefix();
        responses.push(("/tasks/73", response(200, json!({"status":"processing"}))));
        let report = run(&source, responses).await;
        assert!(!report.success);
        assert_eq!(report.requests.len(), 3);
        assert_eq!(report.loops[0].reason, Some(reason));
    }
}

#[tokio::test]
async fn poll_http_errors_are_not_treated_as_an_empty_task_queue() {
    let mut responses = ingest_prefix();
    responses.push(("/tasks/73", response(503, json!({"message":"unavailable"}))));
    let report = run(INGEST, responses).await;
    assert!(!report.success);
    assert_eq!(report.loops[0].reason, Some(LoopFinishReason::StepFailed));
}

#[tokio::test]
async fn initial_cleanup_only_tolerates_index_not_found() {
    for code in ["index_not_found", "internal"] {
        let mut responses = vec![
            ("/indexes/books", response(202, json!({"taskUid":91}))),
            ("/tasks/91", response(200, json!({"status":"failed"}))),
            (
                "/tasks/91",
                response(200, json!({"status":"failed", "error":{"code":code}})),
            ),
        ];
        if code == "index_not_found" {
            // The next operation is reached only for the explicitly allowed cleanup error.
            responses.push(("/indexes", response(500, json!({}))));
        }
        let report = run(CLEANUP, responses).await;
        assert!(!report.success);
        assert_eq!(
            report.loops[0].reason,
            Some(if code == "index_not_found" {
                LoopFinishReason::ConditionMet
            } else {
                LoopFinishReason::StepFailed
            })
        );
        assert_eq!(
            report.requests.len(),
            if code == "index_not_found" { 4 } else { 3 }
        );
    }
}

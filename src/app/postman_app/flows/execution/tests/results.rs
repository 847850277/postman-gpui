use super::*;
use postman_flow::{FlowEvent, HttpRequestSource, LoopFinishReason};
use serde_json::json;

fn inspected_document(url: &str) -> FlowDocument {
    parse_flow_yaml(&format!(
        r#"
schema_version: 1
flow:
  name: Detailed results
  steps:
    - id: fetch
      name: Fetch result
      request: {{kind: http, method: GET, url: {{literal: '{url}'}}}}
      checks: [{{kind: status, equals: 200}}]
      exports:
        - {{name: data, path: $}}
  outputs:
    - name: result
      value: {{output: {{step: fetch, name: data}}}}
"#
    ))
    .unwrap()
}
async fn projected(document: &FlowDocument, response: Result<HttpResponse, HttpError>) -> Run {
    let (sender, mut receiver) = mpsc::channel(64);
    let (_cancel, cancelled) = oneshot::channel();
    let worker = tokio::spawn(drive(
        plan(document),
        controlled(response, false),
        FlowSessionEnvironment::default(),
        cancelled,
        sender,
    ));
    let mut run = Run::new(7, 3, document);
    finish(&mut receiver, &mut run).await;
    worker.await.unwrap();
    run
}
#[tokio::test]
async fn types_partial_evidence_failures_and_new_runs_are_truthful() {
    let doc = inspected_document("https://example.test");
    let payload = json!({"zero":0,"false":false,"null":null,"empty":"","array":[],"literal":"[REDACTED]","nested":{"token":"SYNTH-private","ok":42}});
    let success = projected(
        &doc,
        Ok(HttpResponse::new(200, vec![], payload.to_string())),
    )
    .await;
    let result = &success.results;
    let value = result.nodes[0].values[0].value.value().unwrap();
    assert_eq!(value["zero"], json!(0));
    assert_eq!(value["false"], json!(false));
    assert_eq!(value["null"], json!(null));
    assert_eq!(value["empty"], json!(""));
    assert_eq!(value["literal"], json!("[REDACTED]"));
    let copied = result.outputs_json().unwrap();
    assert!(!copied.contains("SYNTH-private"));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&copied).unwrap()["result"],
        *value
    );
    assert_eq!(result.nodes[0].checks.len(), 1);
    assert!(result.nodes[0].checks[0].detail.contains("Actual 200"));
    for response in [
        Ok(HttpResponse::new(503, vec![], "secret".into())),
        Err(HttpError::Timeout { timeout_ms: 10 }),
        Err(HttpError::network("secret-SYNTH")),
        Ok(HttpResponse::new(200, vec![], "not json".into())),
    ] {
        let failed = projected(&doc, response).await;
        assert_eq!(failed.status, Status::Failed);
        assert!(failed.results.outputs_json().is_none());
        assert!(failed.results.outputs.is_empty());
        assert!(!failed.results.nodes[0].summary().contains("SYNTH"));
    }
    let next = Run::new(8, 4, &doc);
    assert!(next.results.outputs.is_empty());
    assert!(next.results.nodes[0].values.is_empty());
    assert_eq!(next.results.nodes[0].status, Status::NotExecuted);
}

#[tokio::test]
async fn non_2xx_success_is_decided_by_explicit_checks_and_explained_in_steps() {
    for status in [200, 400, 404, 503] {
        for expected in [None, Some(200), Some(status)] {
            let mut doc = inspected_document("https://example.test");
            doc.flow.steps[0].checks = expected
                .map(postman_flow::ResponseCheck::StatusEquals)
                .into_iter()
                .collect();
            let run = projected(&doc, Ok(HttpResponse::new(status, vec![], "{}".into()))).await;
            let succeeds = expected.is_none_or(|code| code == status);
            let outcome = if succeeds {
                Status::Succeeded
            } else {
                Status::Failed
            };
            assert_eq!(run.status, outcome);
            let step = &run.results.nodes[0];
            assert_eq!(step.status, outcome);
            assert_eq!(step.check_count, usize::from(expected.is_some()));
            assert_eq!(step.response.unwrap().0, status);
            assert_eq!(
                step.summary().contains("No response checks configured"),
                status >= 300 && expected.is_none()
            );
            assert_eq!(run.results.outputs_json().is_some(), succeeds);
        }
    }
}
fn nested() -> FlowDocument {
    parse_flow_yaml(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/crates/postman-flow/tests/fixtures/inspection-nested.http.yml"
    )))
    .unwrap()
}
#[tokio::test]
async fn scoped_nested_invocations_and_continue_keep_failed_iterations() {
    let doc = nested();
    let run = projected(&doc, Ok(HttpResponse::new(503, vec![], "{}".into()))).await;
    assert_eq!(run.status, Status::Succeeded); // continue permits overall success
    let nodes = &run.results.nodes;
    let inner: Vec<_> = nodes
        .iter()
        .filter(|n| n.step == "inner" && n.iteration.is_none())
        .collect();
    assert_eq!(inner.len(), 3);
    assert_eq!(inner[1].status, Status::Skipped);
    assert_ne!(inner[0].parent, inner[2].parent);
    let failures: Vec<_> = nodes
        .iter()
        .filter(|n| n.step == "inner" && n.iteration.is_some())
        .collect();
    assert_eq!(failures.len(), 4);
    assert!(failures.iter().all(|n| n.status == Status::Failed));
    assert_eq!(failures[0].name, "Iteration 1");
    assert_eq!(failures[1].name, "Iteration 2");
    assert!(nodes
        .iter()
        .filter(|n| n.step == "fetch")
        .all(|n| n.checks.len() == 1 && !n.checks[0].success));
    assert_eq!(
        run.results.outputs_json().unwrap(),
        "{\n  \"batches\": [\n    [],\n    []\n  ]\n}"
    );
}
#[tokio::test]
async fn loops_show_zero_limit_conditions_and_bounded_retention() {
    let mut doc = nested();
    let HttpRequestSource::ForEach(outer) = &mut doc.flow.steps[0].request else {
        unreachable!()
    };
    outer.items = postman_flow::JsonTemplate::literal(json!([]));
    let run = projected(&doc, Ok(HttpResponse::new(200, vec![], "{}".into()))).await;
    assert_eq!(run.results.nodes.len(), 1);
    assert_eq!(run.results.nodes[0].executed, Some(0));
    assert_eq!(run.results.nodes[0].detail, "Completed");
    let poll = polling(1);
    let run = projected(&poll, Ok(HttpResponse::new(200, vec![], "{}".into()))).await;
    assert_eq!(run.results.nodes[0].executed, Some(2));
    assert_eq!(run.results.nodes[0].detail, "Iteration limit reached");
    assert_eq!(run.results.nodes[0].interval, Some(1));
    assert!(run.results.nodes[0].wait > Duration::ZERO);
    let mut projection = super::super::results::Results::new(&poll);
    let time = Instant::now();
    projection.event(
        &FlowEvent::StepStarted {
            step_id: "poll".into(),
            name: "poll".into(),
        },
        time,
    );
    projection.event(
        &FlowEvent::LoopStarted {
            step_id: "poll".into(),
            kind: postman_flow::LoopKind::RepeatUntil,
            limit: 5000,
        },
        time,
    );
    for index in 0..5000 {
        projection.event(
            &FlowEvent::IterationStarted {
                step_id: "poll".into(),
                index,
            },
            time,
        );
        projection.event(
            &FlowEvent::IterationFinished {
                step_id: "poll".into(),
                index,
                outcome: postman_flow::StepOutcome::Succeeded,
            },
            time + Duration::from_millis(10),
        );
    }
    projection.event(
        &FlowEvent::LoopFinished {
            step_id: "poll".into(),
            total_executed: 5000,
            reason: LoopFinishReason::Completed,
            success: true,
        },
        time,
    );
    assert_eq!(projection.nodes.len(), super::super::results::MAX_ROWS);
    assert_eq!(projection.total_rows, 10001);
    assert_eq!(projection.nodes[0].executed, Some(5000));
}

#[gpui_kit::test]
fn native_steps_values_outputs_keyboard_copy_and_rerun(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let payload = json!({"zero":0,"flag":false,"empty":"","nested":{"password":"native-SYNTH","safe":[null,1]},"literal":"[REDACTED]"});
    let mock = server
        .mock("GET", "/result")
        .with_status(200)
        .with_body(payload.to_string())
        .expect(2)
        .create();
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |v, _| v.flows.clone());
    click(cx, "nav-flows");
    let id = install(
        &flows,
        inspected_document(&format!("{}/result", server.url())),
        cx,
    );
    click(cx, "flow-run");
    settle(cx);
    assert_eq!(status(&flows, id, cx), Some(Status::Succeeded));
    click(cx, "console-tab-steps");
    settle(cx);
    click(cx, "console-maximize");
    settle(cx);
    click(cx, "result-row-0");
    settle(cx); // request
    click(cx, "result-row-1");
    settle(cx); // extraction (after check)
    press(cx, "secondary-a");
    press(cx, "secondary-c");
    let editor_copy = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    assert!(!editor_copy.contains("native-SYNTH"));
    assert!(serde_json::from_str::<serde_json::Value>(&editor_copy).is_ok());
    click(cx, "result-copy-value");
    let copied = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    assert!(!copied.contains("native-SYNTH"));
    let value: serde_json::Value = serde_json::from_str(&copied).unwrap();
    assert_eq!(value["zero"], json!(0));
    assert_eq!(value["flag"], json!(false));
    press(cx, "escape");
    settle(cx);
    click(cx, "console-tab-outputs");
    settle(cx);
    click(cx, "console-copy");
    let all = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&all).unwrap()["result"],
        value
    );
    click(cx, "result-row-0");
    settle(cx);
    click(cx, "result-copy-value");
    assert_eq!(
        cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap()),
        copied
    );
    click(cx, "console-tab-steps");
    settle(cx); // restores request scope
    click(cx, "result-back");
    settle(cx); // root
    click(cx, "result-row-0");
    settle(cx);
    press(cx, "down");
    press(cx, "enter");
    settle(cx);
    click(cx, "result-copy-value");
    assert_eq!(
        cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap()),
        copied
    );
    click(cx, "flow-run");
    settle(cx);
    click(cx, "console-tab-steps");
    settle(cx);
    click(cx, "result-row-0");
    settle(cx); // selection reset to root on rerun
    mock.assert();
}

#[gpui_kit::test]
fn native_nested_iterations_expose_failures_with_distinct_paths(cx: &mut TestAppContext) {
    let mut server = mockito::Server::new();
    let mock = server
        .mock("GET", "/")
        .with_status(503)
        .with_body("{}")
        .expect(4)
        .create();
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |v, _| v.flows.clone());
    click(cx, "nav-flows");
    let source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/crates/postman-flow/tests/fixtures/inspection-nested.http.yml"
    ))
    .replace("https://example.test", &server.url());
    let id = install(&flows, parse_flow_yaml(&source).unwrap(), cx);
    click(cx, "flow-run");
    settle(cx);
    assert_eq!(status(&flows, id, cx), Some(Status::Succeeded));
    click(cx, "console-maximize");
    click(cx, "console-tab-steps");
    settle(cx);
    // outer -> iteration 1 -> inner -> iteration 1 -> failed request -> actual failed check.
    for _ in 0..5 {
        click(cx, "result-row-0");
        settle(cx);
    }
    cx.update(|w, _| {
        let row = w.find("result-row-0");
        let label = row.label().unwrap();
        assert!(
            label.contains("Failed") && label.contains("Actual 503"),
            "{label}"
        );
    });
    for _ in 0..4 {
        press(cx, "escape");
        settle(cx);
    }
    click(cx, "result-row-2");
    settle(cx); // outer iteration 3, separate invocation
    click(cx, "result-row-0");
    settle(cx); // inner
    flows.read_with(cx, |v, _| {
        let nodes = &v.execution.runs[&id].results.nodes;
        assert!(nodes
            .iter()
            .any(|n| n.path == "outer / Iteration 3 / inner / Iteration 2 / fetch"));
    });
    mock.assert();
}

#[tokio::test]
async fn value_and_output_retention_are_bounded_and_copy_is_disabled_when_incomplete() {
    let doc = inspected_document("https://example.test");
    let source = projected(
        &doc,
        Ok(HttpResponse::new(
            200,
            vec![],
            json!({"large":"x".repeat(16000)}).to_string(),
        )),
    )
    .await;
    let value = source.results.nodes[0].values[0].value.clone();
    let mut results = super::super::results::Results::new(&doc);
    results.event(
        &FlowEvent::StepStarted {
            step_id: "fetch".into(),
            name: "fetch".into(),
        },
        Instant::now(),
    );
    for index in 0..1000 {
        results.event(
            &FlowEvent::OutputExported {
                step_id: "fetch".into(),
                name: format!("value{index}"),
                value: value.clone(),
            },
            Instant::now(),
        );
    }
    assert_eq!(results.nodes[0].value_count, 1000);
    assert!(results.nodes[0].values.len() <= 128);
    assert!(
        results.nodes[0]
            .values
            .iter()
            .map(|v| v.value.byte_len())
            .sum::<usize>()
            <= 2 * 1024 * 1024
    );
    let huge = projected(
        &doc,
        Ok(HttpResponse::new(
            200,
            vec![],
            json!({"large":"x".repeat(20000)}).to_string(),
        )),
    )
    .await;
    assert!(huge.results.nodes[0].values[0].value.is_truncated());
    assert!(!huge.results.can_copy_outputs());
}

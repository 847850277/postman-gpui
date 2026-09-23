mod support;

use futures::StreamExt;
use postman_flow::{
    compile_flow, execute_flow, parse_flow_yaml, write_flow_yaml, CompileEnvironment, FlowEvent,
    FlowInputs, FlowSessionEnvironment, LoopFinishReason,
};
use serde_json::json;
use support::{response, run, FakeTransport};

fn compile(source: &str) -> postman_flow::FlowPlan {
    let document = parse_flow_yaml(source).unwrap();
    compile_flow(
        &document.flow,
        &document.apis,
        &CompileEnvironment::default(),
    )
    .unwrap()
}

#[tokio::test]
async fn for_each_binds_items_and_indexes_and_collects_outputs() {
    let source = r#"
schema_version: 1
flow:
  name: collect users
  inputs:
    - name: users
  steps:
    - id: users-loop
      kind: for_each
      items: { input: users }
      as: user
      index_as: user_index
      max_iterations: 10
      on_error: fail_fast
      steps:
        - id: fetch-user
          request:
            kind: http
            method: GET
            url:
              concat:
                - { literal: "https://example.test/users/" }
                - { input: user }
                - { literal: "?index=" }
                - { input: user_index }
          checks:
            - kind: status
              equals: 200
          exports:
            - name: detail
              path: $.data
      exports:
        - name: all_details
          collect: { step: fetch-user, name: detail }
  outputs:
    - name: details
      value: { output: { step: users-loop, name: all_details } }
"#;
    let document = parse_flow_yaml(source).unwrap();
    let canonical = write_flow_yaml(&document).unwrap();
    assert_eq!(parse_flow_yaml(&canonical).unwrap(), document);

    let plan = compile_flow(
        &document.flow,
        &document.apis,
        &CompileEnvironment::default(),
    )
    .unwrap();
    let transport = FakeTransport::new([
        response(json!({"data": {"id": "alice"}})),
        response(json!({"data": {"id": "bob"}})),
    ]);
    let events = run(
        plan,
        FlowInputs::new().with("users", json!(["alice", "bob"])),
        transport.clone(),
    )
    .await;

    let FlowEvent::FlowFinished {
        success: true,
        outputs,
    } = events.last().unwrap()
    else {
        panic!("flow failed: {events:?}")
    };
    assert_eq!(
        outputs["details"].value(),
        &json!([{"id": "alice"}, {"id": "bob"}])
    );
    let requests = transport.requests();
    assert_eq!(
        requests[0].0.url,
        "https://example.test/users/alice?index=0"
    );
    assert_eq!(requests[1].0.url, "https://example.test/users/bob?index=1");
    assert!(events.iter().any(|event| matches!(
        event,
        FlowEvent::LoopFinished {
            total_executed: 2,
            reason: LoopFinishReason::Completed,
            success: true,
            ..
        }
    )));
}

const POLL_FLOW: &str = r#"
schema_version: 1
flow:
  name: wait for receipt
  inputs:
    - name: transaction_id
  steps:
    - id: wait-for-receipt
      kind: repeat_until
      max_iterations: 4
      interval_ms: 1
      timeout_ms: 1000
      until:
        eq:
          - { output: { step: query-receipt, name: status } }
          - { literal: success }
      fail_when:
        eq:
          - { output: { step: query-receipt, name: status } }
          - { literal: failed }
      steps:
        - id: query-receipt
          request:
            kind: http
            method: GET
            url:
              concat:
                - { literal: "https://example.test/transactions/" }
                - { input: transaction_id }
          checks:
            - kind: status
              equals: 200
          exports:
            - name: status
              path: $.status
"#;

#[tokio::test]
async fn repeat_until_polls_pending_receipts_until_success() {
    let transport = FakeTransport::new([
        response(json!({"status": "pending"})),
        response(json!({"status": "pending"})),
        response(json!({"status": "success"})),
    ]);
    let events = run(
        compile(POLL_FLOW),
        FlowInputs::new().with("transaction_id", "0xabc"),
        transport.clone(),
    )
    .await;
    assert_eq!(transport.requests().len(), 3);
    assert!(matches!(
        events.last(),
        Some(FlowEvent::FlowFinished { success: true, .. })
    ));
    assert!(events.iter().any(|event| matches!(
        event,
        FlowEvent::LoopFinished {
            total_executed: 3,
            reason: LoopFinishReason::ConditionMet,
            success: true,
            ..
        }
    )));
}

#[tokio::test]
async fn repeat_until_stops_on_terminal_failure_without_another_request() {
    let transport = FakeTransport::new([
        response(json!({"status": "pending"})),
        response(json!({"status": "failed"})),
    ]);
    let events = run(
        compile(POLL_FLOW),
        FlowInputs::new().with("transaction_id", "0xabc"),
        transport.clone(),
    )
    .await;
    assert_eq!(transport.requests().len(), 2);
    assert!(matches!(
        events.last(),
        Some(FlowEvent::FlowFinished { success: false, .. })
    ));
    assert!(events.iter().any(|event| matches!(
        event,
        FlowEvent::LoopFinished {
            reason: LoopFinishReason::FailureCondition,
            success: false,
            ..
        }
    )));
}

#[tokio::test]
async fn repeat_until_preserves_transport_cancellation_as_a_loop_outcome() {
    let transport = FakeTransport::new([Err(postman_http::HttpError::Cancelled)]);
    let events = run(
        compile(POLL_FLOW),
        FlowInputs::new().with("transaction_id", "0xabc"),
        transport,
    )
    .await;
    assert!(events.iter().any(|event| matches!(
        event,
        FlowEvent::LoopFinished {
            reason: LoopFinishReason::Cancelled,
            success: false,
            ..
        }
    )));
}

#[tokio::test]
async fn repeat_until_failure_condition_short_circuits_unavailable_until_output() {
    let source = r#"
schema_version: 1
flow:
  name: failure condition wins
  steps:
    - id: poll
      kind: repeat_until
      max_iterations: 2
      interval_ms: 1
      timeout_ms: 1000
      fail_when: { eq: [{ literal: true }, { literal: true }] }
      until: { eq: [{ output: { step: fetch, name: status } }, { literal: success }] }
      steps:
        - id: fetch
          when: { eq: [{ literal: false }, { literal: true }] }
          request: { kind: http, method: GET, url: { literal: 'https://example.test' } }
          exports: [{ name: status, path: $.status }]
    - id: after-poll
      request: { kind: http, method: GET, url: { literal: 'https://example.test/after' } }
"#;
    let transport = FakeTransport::new([]);
    let events = run(compile(source), FlowInputs::new(), transport.clone()).await;
    assert!(transport.requests().is_empty());
    assert_eq!(
        events
            .iter()
            .filter_map(|event| match event {
                FlowEvent::LoopFinished {
                    reason,
                    total_executed,
                    success,
                    ..
                } => Some((*reason, *total_executed, *success)),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![(LoopFinishReason::FailureCondition, 1, false)]
    );
    assert!(matches!(
        events.last(),
        Some(FlowEvent::FlowFinished { success: false, .. })
    ));
}

#[tokio::test]
async fn for_each_collect_omits_when_skipped_sources_without_failing_the_iteration() {
    let source = r#"
schema_version: 1
flow:
  name: collect conditional branches
  inputs: [{ name: items }]
  steps:
    - id: each
      kind: for_each
      items: { input: items }
      as: item
      steps:
        - id: left
          when: { eq: [{ input: item }, { literal: left }] }
          request: { kind: http, method: GET, url: { literal: 'https://example.test/left' } }
          exports: [{ name: value, path: $.value, sensitive: true }]
        - id: right
          when: { eq: [{ input: item }, { literal: right }] }
          request: { kind: http, method: GET, url: { literal: 'https://example.test/right' } }
          exports: [{ name: value, path: $.value }]
      exports:
        - name: left_values
          collect: { step: left, name: value }
        - name: right_values
          collect: { step: right, name: value }
  outputs:
    - name: left_values
      value: { output: { step: each, name: left_values } }
    - name: right_values
      value: { output: { step: each, name: right_values } }
"#;
    for items in [json!(["left", "right", "left", "skip"]), json!(["skip"])] {
        let all_skipped = items == json!(["skip"]);
        let transport = FakeTransport::new(if all_skipped {
            vec![]
        } else {
            vec![
                response(json!({"value": "left-1"})),
                response(json!({"value": "right-1"})),
                response(json!({"value": "left-2"})),
            ]
        });
        let events = run(
            compile(source),
            FlowInputs::new().with("items", items.clone()),
            transport.clone(),
        )
        .await;
        let Some(FlowEvent::FlowFinished {
            success: true,
            outputs,
        }) = events.last()
        else {
            panic!("conditional collection failed: {events:?}")
        };
        assert_eq!(
            outputs["left_values"].value(),
            &if all_skipped {
                json!([])
            } else {
                json!(["left-1", "left-2"])
            }
        );
        assert_eq!(
            outputs["right_values"].value(),
            &if all_skipped {
                json!([])
            } else {
                json!(["right-1"])
            }
        );
        if !all_skipped {
            assert!(outputs["left_values"].is_sensitive());
        }
        assert_eq!(transport.requests().len(), if all_skipped { 0 } else { 3 });
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    FlowEvent::IterationFinished {
                        outcome: postman_flow::StepOutcome::Succeeded,
                        ..
                    }
                ))
                .count(),
            items.as_array().unwrap().len()
        );
    }
}

#[tokio::test]
async fn for_each_collect_omits_skipped_nested_loops_and_resets_skip_state() {
    let source = r#"
schema_version: 1
flow:
  name: collect optional nested loops
  steps:
    - id: outer
      kind: for_each
      items: { literal: [true, false, true] }
      as: enabled
      steps:
        - id: inner
          kind: for_each
          when: { eq: [{ input: enabled }, { literal: true }] }
          items: { literal: [1] }
          as: item
          steps:
            - id: fetch
              request: { kind: http, method: GET, url: { literal: 'https://example.test' } }
              exports: [{ name: value, path: $.value }]
          exports:
            - name: values
              collect: { step: fetch, name: value }
      exports:
        - name: batches
          collect: { step: inner, name: values }
  outputs:
    - name: batches
      value: { output: { step: outer, name: batches } }
"#;
    let transport =
        FakeTransport::new([response(json!({"value": 1})), response(json!({"value": 2}))]);
    let events = run(compile(source), FlowInputs::new(), transport.clone()).await;
    let Some(FlowEvent::FlowFinished {
        success: true,
        outputs,
    }) = events.last()
    else {
        panic!("nested collection failed: {events:?}")
    };
    assert_eq!(outputs["batches"].value(), &json!([[1], [2]]));
    assert_eq!(transport.requests().len(), 2);
}

#[tokio::test]
async fn repeat_until_carries_a_pagination_cursor_explicitly() {
    let source = r#"
schema_version: 1
flow:
  name: paginate
  steps:
    - id: pages
      kind: repeat_until
      max_iterations: 5
      interval_ms: 1
      timeout_ms: 1000
      carry:
        - name: cursor
          initial: { literal: start }
          from: { step: fetch-page, name: next_cursor }
      until:
        eq:
          - { output: { step: fetch-page, name: done } }
          - { literal: true }
      steps:
        - id: fetch-page
          request:
            kind: http
            method: GET
            url:
              concat:
                - { literal: "https://example.test/pages?cursor=" }
                - { input: cursor }
          exports:
            - name: next_cursor
              path: $.next
            - name: done
              path: $.done
"#;
    let transport = FakeTransport::new([
        response(json!({"next": "second", "done": false})),
        response(json!({"next": "third", "done": false})),
        response(json!({"next": "unused", "done": true})),
    ]);
    let events = run(compile(source), FlowInputs::new(), transport.clone()).await;
    let urls = transport
        .requests()
        .into_iter()
        .map(|(request, _)| request.url)
        .collect::<Vec<_>>();
    assert_eq!(
        urls,
        [
            "https://example.test/pages?cursor=start",
            "https://example.test/pages?cursor=second",
            "https://example.test/pages?cursor=third",
        ]
    );
    assert!(matches!(
        events.last(),
        Some(FlowEvent::FlowFinished { success: true, .. })
    ));
}

#[tokio::test]
async fn repeat_until_reports_iteration_limit_and_timeout() {
    let limited = POLL_FLOW.replace("max_iterations: 4", "max_iterations: 2");
    let transport = FakeTransport::new([
        response(json!({"status": "pending"})),
        response(json!({"status": "pending"})),
    ]);
    let events = run(
        compile(&limited),
        FlowInputs::new().with("transaction_id", "0xabc"),
        transport.clone(),
    )
    .await;
    assert_eq!(transport.requests().len(), 2);
    assert!(events.iter().any(|event| matches!(
        event,
        FlowEvent::LoopFinished {
            reason: LoopFinishReason::MaxIterations,
            ..
        }
    )));

    let timed = POLL_FLOW
        .replace("interval_ms: 1", "interval_ms: 10")
        .replace("timeout_ms: 1000", "timeout_ms: 2");
    let transport = FakeTransport::new([response(json!({"status": "pending"}))]);
    let events = run(
        compile(&timed),
        FlowInputs::new().with("transaction_id", "0xabc"),
        transport.clone(),
    )
    .await;
    let requests = transport.requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].1.timeout_ms.is_some_and(|timeout| timeout <= 2));
    assert!(events.iter().any(|event| matches!(
        event,
        FlowEvent::LoopFinished {
            reason: LoopFinishReason::Timeout,
            ..
        }
    )));
}

#[tokio::test]
async fn for_each_continue_skips_failed_iteration_collections() {
    let source = r#"
schema_version: 1
flow:
  name: continue failed items
  inputs:
    - name: items
  steps:
    - id: loop
      kind: for_each
      items: { input: items }
      as: item
      on_error: continue
      steps:
        - id: fetch
          request:
            kind: http
            method: GET
            url: { input: item }
          checks: [{ kind: status, equals: 200 }]
          exports: [{ name: value, path: $.value }]
      exports:
        - name: values
          collect: { step: fetch, name: value }
  outputs:
    - name: values
      value: { output: { step: loop, name: values } }
"#;
    let transport = FakeTransport::new([
        Ok(postman_http::HttpResponse::new(
            500,
            Vec::new(),
            r#"{"value":"bad"}"#.to_owned(),
        )),
        response(json!({"value": "kept"})),
    ]);
    let events = run(
        compile(source),
        FlowInputs::new().with("items", json!(["https://a", "https://b"])),
        transport,
    )
    .await;
    let FlowEvent::FlowFinished {
        success: true,
        outputs,
    } = events.last().unwrap()
    else {
        panic!("flow should continue: {events:?}")
    };
    assert_eq!(outputs["values"].value(), &json!(["kept"]));
    assert!(events.iter().any(|event| matches!(
        event,
        FlowEvent::IterationFinished {
            outcome: postman_flow::StepOutcome::Failed { .. },
            ..
        }
    )));
}

#[tokio::test]
async fn for_each_continue_stops_on_transport_cancellation() {
    let source = r#"
schema_version: 1
flow:
  name: cancel remaining items
  steps:
    - id: loop
      kind: for_each
      items: { literal: [first, second, third] }
      as: item
      on_error: continue
      steps:
        - id: fetch
          request: { kind: http, method: GET, url: { literal: "https://example.test/item" } }
          exports: [{ name: value, path: $.value }]
        - id: after-fetch
          request: { kind: http, method: GET, url: { literal: "https://example.test/after-item" } }
      exports:
        - name: values
          collect: { step: fetch, name: value }
    - id: after-loop
      request: { kind: http, method: GET, url: { literal: "https://example.test/after-loop" } }
  outputs:
    - name: values
      value: { output: { step: loop, name: values } }
"#;
    let transport = FakeTransport::new([
        response(json!({"value": "first"})),
        response(json!({})),
        Err(postman_http::HttpError::Cancelled),
    ]);
    let events = run(compile(source), FlowInputs::new(), transport.clone()).await;

    assert_eq!(transport.requests().len(), 3);
    assert_eq!(
        events
            .iter()
            .filter_map(|event| match event {
                FlowEvent::LoopFinished {
                    step_id,
                    total_executed,
                    reason,
                    success,
                } => Some((step_id.as_str(), *total_executed, *reason, *success)),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![("loop", 2, LoopFinishReason::Cancelled, false)]
    );
    assert!(matches!(
        events.last(),
        Some(FlowEvent::FlowFinished { success: false, outputs }) if outputs.is_empty()
    ));
}

#[tokio::test]
async fn nested_for_each_continue_propagates_cancellation_through_repeat_until() {
    let source = r#"
schema_version: 1
flow:
  name: cancel nested loops
  steps:
    - id: outer
      kind: for_each
      items: { literal: [first, second] }
      as: outer_item
      on_error: continue
      steps:
        - id: poll
          kind: repeat_until
          max_iterations: 2
          interval_ms: 1
          timeout_ms: 1000
          until: { eq: [{ literal: false }, { literal: true }] }
          steps:
            - id: inner
              kind: for_each
              items: { literal: [first, second] }
              as: inner_item
              on_error: continue
              steps:
                - id: fetch
                  request: { kind: http, method: GET, url: { literal: "https://example.test/item" } }
        - id: after-poll
          request: { kind: http, method: GET, url: { literal: "https://example.test/after-poll" } }
    - id: after-outer
      request: { kind: http, method: GET, url: { literal: "https://example.test/after-outer" } }
"#;
    let transport = FakeTransport::new([Err(postman_http::HttpError::Cancelled)]);
    let events = run(compile(source), FlowInputs::new(), transport.clone()).await;

    assert_eq!(transport.requests().len(), 1);
    assert_eq!(
        events
            .iter()
            .filter_map(|event| match event {
                FlowEvent::LoopFinished {
                    step_id,
                    total_executed,
                    reason,
                    success,
                } => Some((step_id.as_str(), *total_executed, *reason, *success)),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![
            ("inner", 1, LoopFinishReason::Cancelled, false),
            ("poll", 1, LoopFinishReason::Cancelled, false),
            ("outer", 1, LoopFinishReason::Cancelled, false),
        ]
    );
    assert!(matches!(
        events.last(),
        Some(FlowEvent::FlowFinished { success: false, .. })
    ));
}

#[tokio::test]
async fn collected_sensitive_exports_remain_redacted() {
    let source = r#"
schema_version: 1
flow:
  name: sensitive collection
  inputs:
    - name: items
  steps:
    - id: loop
      kind: for_each
      items: { input: items }
      as: item
      steps:
        - id: fetch
          request: { kind: http, method: GET, url: { input: item } }
          exports:
            - name: token
              path: $.token
              sensitive: true
      exports:
        - name: tokens
          collect: { step: fetch, name: token }
  outputs:
    - name: tokens
      value: { output: { step: loop, name: tokens } }
"#;
    let events = run(
        compile(source),
        FlowInputs::new().with("items", json!(["https://a"])),
        FakeTransport::new([response(json!({"token": "secret-value"}))]),
    )
    .await;
    let FlowEvent::FlowFinished { outputs, .. } = events.last().unwrap() else {
        panic!()
    };
    assert!(outputs["tokens"].is_sensitive());
    assert!(!format!("{:?}", outputs["tokens"]).contains("secret-value"));
}

#[tokio::test]
async fn dropping_poll_stream_after_wait_event_prevents_the_next_request() {
    let transport = FakeTransport::new([
        response(json!({"status": "pending"})),
        response(json!({"status": "success"})),
    ]);
    let stream = execute_flow(
        compile(POLL_FLOW),
        transport.clone(),
        FlowSessionEnvironment::new(FlowInputs::new().with("transaction_id", "0xabc")),
    )
    .unwrap();
    let mut stream = Box::pin(stream);
    while let Some(event) = stream.next().await {
        if matches!(event.unwrap(), FlowEvent::LoopWaiting { .. }) {
            break;
        }
    }
    drop(stream);
    futures_timer::Delay::new(std::time::Duration::from_millis(5)).await;
    assert_eq!(transport.requests().len(), 1);
}

#[test]
fn compiler_rejects_unbounded_or_out_of_scope_loops() {
    let invalid = POLL_FLOW.replace("max_iterations: 4", "max_iterations: 0");
    let document = parse_flow_yaml(&invalid).unwrap();
    let errors = compile_flow(
        &document.flow,
        &document.apis,
        &CompileEnvironment::default(),
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.location.field.ends_with("max_iterations")));

    let leaked = r#"
schema_version: 1
flow:
  name: bad scope
  inputs:
    - name: values
  steps:
    - id: loop
      kind: for_each
      items: { input: values }
      as: item
      steps:
        - id: inside
          request: { kind: http, method: GET, url: { input: item } }
    - id: outside
      request: { kind: http, method: GET, url: { input: item } }
"#;
    let document = parse_flow_yaml(leaked).unwrap();
    let errors = compile_flow(
        &document.flow,
        &document.apis,
        &CompileEnvironment::default(),
    )
    .unwrap_err();
    assert!(errors.iter().any(|error| error.message.contains("item")));

    let too_deep = r#"
schema_version: 1
flow:
  name: too deep
  steps:
    - id: one
      kind: for_each
      items: { literal: [] }
      as: one_item
      steps:
        - id: two
          kind: for_each
          items: { literal: [] }
          as: two_item
          steps:
            - id: three
              kind: for_each
              items: { literal: [] }
              as: three_item
              steps:
                - id: four
                  kind: for_each
                  items: { literal: [] }
                  as: four_item
                  steps:
                    - id: leaf
                      request:
                        kind: http
                        method: GET
                        url: { literal: "https://example.test" }
"#;
    let document = parse_flow_yaml(too_deep).unwrap();
    let errors = compile_flow(
        &document.flow,
        &document.apis,
        &CompileEnvironment::default(),
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.message.contains("at most 3 levels")));
}

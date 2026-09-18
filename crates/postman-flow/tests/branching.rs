mod support;
use postman_flow::*;
use postman_http::request::HttpMethod;
use serde_json::json;
use support::{response, FakeTransport};

#[tokio::test]
async fn coalesce_preserves_concat_candidates_and_api_bindings() {
    let source = r#"
schema_version: 1
apis:
  fetch:
    parameters: [url]
    request:
      method: GET
      url:
        coalesce:
          - input: url
          - literal: https://fallback.invalid
flow:
  name: concat-candidates
  steps:
    - id: skipped
      when: {eq: [{literal: 1}, {literal: 2}]}
      request: {kind: http, method: GET, url: {literal: "https://example.com"}}
      exports: [{name: token, path: "$.token"}]
    - id: direct
      request:
        kind: http
        method: GET
        url:
          coalesce:
            - concat:
                - literal: https://unavailable.invalid/
                - output: {step: skipped, name: token}
            - concat:
                - literal: https://example.com/
                - literal: direct
    - id: catalog
      request:
        kind: api
        api: fetch
        bindings:
          url:
            concat:
              - literal: https://example.com/
              - literal: catalog
"#;
    let document = parse_flow_yaml(source).unwrap();
    assert_eq!(
        document,
        parse_flow_yaml(&write_flow_yaml(&document).unwrap()).unwrap()
    );
    let plan = compile_flow(
        &document.flow,
        &document.apis,
        &CompileEnvironment::default(),
    )
    .unwrap();
    let transport = FakeTransport::new([response(json!({})), response(json!({}))]);
    let events = support::run(plan, FlowInputs::new(), transport.clone()).await;
    assert!(matches!(
        events.last(),
        Some(FlowEvent::FlowFinished { success: true, .. })
    ));
    let requests = transport.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].0.url, "https://example.com/direct");
    assert_eq!(requests[1].0.url, "https://example.com/catalog");
}

#[tokio::test]
async fn step_when_skips_unmatched_steps_and_executes_matched_steps() {
    let mut definition = FlowDefinition::new("branch-test");
    definition
        .inputs
        .push(FlowInputSpec::with_default("role", "admin"));

    definition.steps.push(
        HttpStepDefinition::new(
            "admin-step",
            "Admin Step",
            HttpRequestTemplate::new(
                HttpMethod::GET,
                TextTemplate::literal("https://example.com/admin"),
            ),
        )
        .when(ConditionExpr::eq(
            JsonTemplate::input("role"),
            JsonTemplate::literal("admin"),
        ))
        .export(ResponseExport::json("secret", "$.key")),
    );

    definition.steps.push(
        HttpStepDefinition::new(
            "user-step",
            "User Step",
            HttpRequestTemplate::new(
                HttpMethod::GET,
                TextTemplate::literal("https://example.com/user"),
            ),
        )
        .when(ConditionExpr::eq(
            JsonTemplate::input("role"),
            JsonTemplate::literal("user"),
        ))
        .export(ResponseExport::json("secret", "$.key")),
    );

    definition.outputs.push(FlowOutputSpec {
        name: "result_key".into(),
        value: ValueReference::coalesce([
            ValueReference::step_output("admin-step", "secret"),
            ValueReference::step_output("user-step", "secret"),
            ValueReference::literal("fallback-key"),
        ]),
    });

    let plan = compile_flow(
        &definition,
        &ApiCatalog::new(),
        &CompileEnvironment::default(),
    )
    .unwrap();
    let transport = FakeTransport::new([response(json!({"key": "admin-super-token"}))]);

    let session = FlowSessionEnvironment::new(FlowInputs::new());
    let events = execute_flow(plan, transport.clone(), session).unwrap();
    let mut events = std::pin::pin!(events);
    let mut skipped = Vec::new();
    let mut finished = None;

    while let Some(event) = futures::StreamExt::next(&mut events).await {
        let event = event.unwrap();
        match event {
            FlowEvent::StepSkipped { step_id, .. } => skipped.push(step_id),
            FlowEvent::FlowFinished { outputs, success } => finished = Some((success, outputs)),
            _ => {}
        }
    }

    assert_eq!(skipped, vec!["user-step"]);
    let (success, outputs) = finished.unwrap();
    assert!(success);
    assert_eq!(outputs["result_key"].value(), &json!("admin-super-token"));

    let requests = transport.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].0.url, "https://example.com/admin");
}

#[tokio::test]
async fn condition_operators_evaluate_accurately() {
    let cases = [
        (
            ConditionExpr::ne(JsonTemplate::literal(1), JsonTemplate::literal(2)),
            true,
        ),
        (
            ConditionExpr::ne(JsonTemplate::literal("a"), JsonTemplate::literal("a")),
            false,
        ),
        (
            ConditionExpr::gt(JsonTemplate::literal(10), JsonTemplate::literal(5)),
            true,
        ),
        (
            ConditionExpr::gt(JsonTemplate::literal(5), JsonTemplate::literal(10)),
            false,
        ),
        (
            ConditionExpr::gte(JsonTemplate::literal(10), JsonTemplate::literal(10)),
            true,
        ),
        (
            ConditionExpr::lt(JsonTemplate::literal(3), JsonTemplate::literal(7)),
            true,
        ),
        (
            ConditionExpr::lte(JsonTemplate::literal(7), JsonTemplate::literal(7)),
            true,
        ),
        (
            ConditionExpr::is_in(
                JsonTemplate::literal("b"),
                JsonTemplate::literal(json!(["a", "b", "c"])),
            ),
            true,
        ),
        (
            ConditionExpr::is_in(
                JsonTemplate::literal("z"),
                JsonTemplate::literal(json!(["a", "b"])),
            ),
            false,
        ),
        (
            ConditionExpr::is_in(
                JsonTemplate::literal("sub"),
                JsonTemplate::literal("haystack_sub_string"),
            ),
            true,
        ),
        (
            ConditionExpr::and([
                ConditionExpr::eq(JsonTemplate::literal(1), JsonTemplate::literal(1)),
                ConditionExpr::eq(JsonTemplate::literal(2), JsonTemplate::literal(2)),
            ]),
            true,
        ),
        (
            ConditionExpr::or([
                ConditionExpr::eq(JsonTemplate::literal(1), JsonTemplate::literal(2)),
                ConditionExpr::eq(JsonTemplate::literal(2), JsonTemplate::literal(2)),
            ]),
            true,
        ),
        (
            ConditionExpr::not(ConditionExpr::eq(
                JsonTemplate::literal(1),
                JsonTemplate::literal(2),
            )),
            true,
        ),
    ];

    for (idx, (expr, expected_pass)) in cases.into_iter().enumerate() {
        let mut definition = FlowDefinition::new(format!("test-{idx}"));
        definition.steps.push(
            HttpStepDefinition::new(
                "step1",
                "Step 1",
                HttpRequestTemplate::new(
                    HttpMethod::GET,
                    TextTemplate::literal("https://example.com/test"),
                ),
            )
            .when(expr),
        );

        let plan = compile_flow(
            &definition,
            &ApiCatalog::new(),
            &CompileEnvironment::default(),
        )
        .unwrap();
        let transport = FakeTransport::new(if expected_pass {
            vec![response(json!({}))]
        } else {
            vec![]
        });
        let session = FlowSessionEnvironment::new(FlowInputs::new());
        let events = execute_flow(plan, transport.clone(), session).unwrap();
        let mut events = std::pin::pin!(events);

        let mut step_ran = false;
        let mut step_skipped = false;
        while let Some(event) = futures::StreamExt::next(&mut events).await {
            match event.unwrap() {
                FlowEvent::StepStarted { .. } => step_ran = true,
                FlowEvent::StepSkipped { .. } => step_skipped = true,
                _ => {}
            }
        }

        assert_eq!(step_ran, expected_pass, "case {idx} step_ran mismatch");
        assert_eq!(
            step_skipped, !expected_pass,
            "case {idx} step_skipped mismatch"
        );
    }
}

#[test]
fn yaml_branching_roundtrip_preserves_conditions_and_coalesce() {
    let yaml_src = include_str!("../examples/flows/httpbingo_branching.http.yml");
    let document = parse_flow_yaml(yaml_src).expect("branching fixture should parse");
    assert_eq!(document.flow.steps.len(), 5);
    assert!(document.flow.steps[1].when.is_some());
    assert!(document.flow.steps[2].when.is_some());
    assert!(document.flow.steps[3].when.is_some());
    assert!(document.flow.steps[4].when.is_none());

    let serialized = write_flow_yaml(&document).expect("branching document should write back");
    let roundtrip = parse_flow_yaml(&serialized).expect("roundtrip should parse");
    assert_eq!(document, roundtrip);
}

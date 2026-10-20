mod support;
use postman_flow::*;
use postman_http::{request::HttpMethod, HttpResponse};
use serde_json::json;
use support::{run, FakeTransport};

#[tokio::test]
async fn actual_extractions_and_final_returns_keep_types_and_redact_credential_echoes() {
    let payload = json!({"zero":0,"false":false,"null":null,"empty":"","array":[],"object":{},"literal":"[REDACTED]","nested":{"password":"body-SYNTH","ok":[0,false]},"echo":"Bearer header-SYNTH cookie-SYNTH query-SYNTH body-SYNTH","hidden":{"shape":[1,2]}});
    let mut flow = FlowDefinition::new("inspection");
    let mut request = HttpRequestTemplate::new(
        HttpMethod::GET,
        TextTemplate::literal("https://example.test/?token=query-SYNTH"),
    );
    request.headers.push((
        TextTemplate::literal("Authorization"),
        TextTemplate::literal("Bearer header-SYNTH"),
    ));
    let mut step =
        HttpStepDefinition::new("fetch", "Fetch", request).check(ResponseCheck::StatusEquals(200));
    for name in payload.as_object().unwrap().keys() {
        let mut export = ResponseExport::json(name, format!("$.{name}"));
        if name == "hidden" {
            export = export.sensitive();
        }
        step = step.export(export);
        flow.outputs.push(FlowOutputSpec {
            name: name.clone(),
            value: ValueReference::step_output("fetch", name),
        });
    }
    flow.steps.push(step);
    let transport = FakeTransport::new([Ok(HttpResponse::new(
        200,
        vec![("Set-Cookie".into(), "sid=cookie-SYNTH; Path=/".into())],
        payload.to_string(),
    ))]);
    let plan = compile_flow(&flow, &ApiCatalog::new(), &CompileEnvironment::default()).unwrap();
    let events = run(plan, FlowInputs::new(), transport.clone()).await;
    assert_eq!(transport.requests().len(), 1);
    let FlowEvent::FlowFinished {
        success: true,
        outputs,
    } = events.last().unwrap()
    else {
        panic!("run failed")
    };
    for event in &events {
        if let FlowEvent::OutputExported { name, value, .. } = event {
            assert_eq!(value, outputs[name].inspection());
            let text = value.json().unwrap();
            assert!(!text.contains("SYNTH"), "{name}: {text}");
            assert!(!format!("{value:?}").contains("SYNTH"));
        }
    }
    for name in [
        "zero", "false", "null", "empty", "array", "object", "literal",
    ] {
        assert_eq!(outputs[name].inspection().value(), Some(&payload[name]));
        assert!(outputs[name].inspection().redacted_paths().is_empty());
    }
    assert_eq!(outputs["hidden"].inspection().kind(), "redacted");
    assert_eq!(outputs["hidden"].value(), &payload["hidden"]); // Execution/CLI bindings unchanged.
    assert!(outputs["hidden"].is_sensitive());
    assert!(!format!("{outputs:?}").contains("SYNTH"));
    assert!(events.iter().any(|e| matches!(e,FlowEvent::CheckFinished {inspection,success:true,..} if inspection=="HTTP status · Expected 200 · Actual 200")));
}

#[tokio::test]
async fn sibling_sensitive_export_redacts_an_earlier_echo_and_atomic_failure_has_no_values() {
    let source = r#"
schema_version: 1
flow:
  name: siblings
  steps:
    - id: fetch
      request: {kind: http, method: GET, url: {literal: 'https://example.test'}}
      exports:
        - {name: echo, path: $.a}
        - {name: protected, path: $.b, sensitive: true}
"#;
    let mut doc = parse_flow_yaml(source).unwrap();
    for missing in [false, true] {
        if missing {
            doc.flow.steps[0]
                .exports
                .push(ResponseExport::json("missing", "$.missing"));
        }
        let plan = compile_flow(&doc.flow, &doc.apis, &CompileEnvironment::default()).unwrap();
        let events = run(
            plan,
            FlowInputs::new(),
            FakeTransport::new([support::response(
                json!({"a":"sibling-SYNTH","b":"sibling-SYNTH"}),
            )]),
        )
        .await;
        let values: Vec<_> = events
            .iter()
            .filter_map(|e| {
                if let FlowEvent::OutputExported { value, .. } = e {
                    Some(value)
                } else {
                    None
                }
            })
            .collect();
        if missing {
            assert!(values.is_empty());
        } else {
            assert_eq!(values.len(), 2);
            assert!(values.iter().all(|v| v.kind() == "redacted"));
        }
    }
}

#[tokio::test]
async fn inspection_preserves_query_metadata_but_masks_encoded_credentials_and_basic_auth() {
    let mut flow = FlowDefinition::new("Query and header inspection");
    let mut request = HttpRequestTemplate::new(
        HttpMethod::GET,
        TextTemplate::literal(
            "https://user:p%40ss%20word@example.test/?page=1&user=id&token=hello%20world",
        ),
    );
    request.headers.push((
        TextTemplate::literal("Authorization"),
        TextTemplate::literal("Basic dXNlcjE6cA=="),
    ));
    flow.steps.push(
        HttpStepDefinition::new("fetch", "Fetch", request)
            .export(ResponseExport::json("data", "$")),
    );
    flow.outputs.push(FlowOutputSpec {
        name: "result".into(),
        value: ValueReference::step_output("fetch", "data"),
    });
    let payload = json!({"width":10, "id":1, "name":"order-123", "token_count":42,
        "formula":"a=b", "basic":"dXNlcjE6cA==", "decoded":"p@ss word", "encoded":"hello%20world"});
    let plan = compile_flow(&flow, &ApiCatalog::new(), &CompileEnvironment::default()).unwrap();
    let events = run(
        plan,
        FlowInputs::new(),
        FakeTransport::new([support::response(payload.clone())]),
    )
    .await;
    let FlowEvent::FlowFinished {
        success: true,
        outputs,
    } = events.last().unwrap()
    else {
        panic!("run failed")
    };
    let expected = json!({"width":10, "id":1, "name":"order-123", "token_count":42,
        "formula":"a=b", "basic":"[REDACTED]", "decoded":"[REDACTED]", "encoded":"[REDACTED]"});
    assert_eq!(outputs["result"].value(), &payload);
    assert_eq!(outputs["result"].inspection().value(), Some(&expected));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&outputs["result"].inspection().json().unwrap())
            .unwrap(),
        expected
    );
    assert!(events.iter().any(|event| matches!(event, FlowEvent::OutputExported {value, ..} if value.value() == Some(&expected))));
}

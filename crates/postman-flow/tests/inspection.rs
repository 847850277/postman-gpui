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
        TextTemplate::literal("https://example.test/?q=query-SYNTH"),
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

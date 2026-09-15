mod support;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures::{StreamExt, TryStreamExt};
use postman_flow::*;
use postman_http::{
    request::{HttpMethod, RequestBody, RequestOptions},
    HttpError, HttpResponse, HttpTransport,
};
use serde_json::{json, Value};
use support::{response, run, FakeTransport};

fn source() -> FlowDefinition {
    FlowDefinition {
        name: "source".into(),
        inputs: vec![
            FlowInputSpec::required("host"),
            FlowInputSpec::with_default("data", json!({"enabled": true, "note": null})),
        ],
        steps: vec![
            HttpStepDefinition::new(
                "seed",
                "Generate an ID",
                HttpRequestTemplate::new(
                    HttpMethod::GET,
                    TextTemplate::parts([
                        TemplatePart::input("host"),
                        TemplatePart::literal("/uuid"),
                    ]),
                ),
            )
            .check(ResponseCheck::StatusEquals(200))
            .export(ResponseExport::json("id", "$.uuid")),
            HttpStepDefinition::new(
                "echo",
                "Reuse typed values",
                HttpRequestTemplate::new(
                    HttpMethod::POST,
                    TextTemplate::parts([
                        TemplatePart::input("host"),
                        TemplatePart::literal("/echo/"),
                        TemplatePart::step_output("seed", "id"),
                    ]),
                )
                .json_value_body(JsonTemplate::object([
                    ("id", JsonTemplate::step_output("seed", "id")),
                    ("data", JsonTemplate::input("data")),
                ])),
            )
            .check(ResponseCheck::JsonValueEquals {
                path: "$.data".into(),
                expected: JsonTemplate::input("data"),
            })
            .export(ResponseExport::json("document", "$")),
        ],
        outputs: vec![
            FlowOutputSpec {
                name: "id".into(),
                value: ValueReference::StepOutput {
                    step_id: "seed".into(),
                    name: "id".into(),
                },
            },
            FlowOutputSpec {
                name: "data".into(),
                value: ValueReference::Input("data".into()),
            },
        ],
    }
}

fn compile(source: &FlowDefinition) -> FlowPlan {
    compile_flow(source, &ApiCatalog::new(), &CompileEnvironment::default()).unwrap()
}

fn inputs() -> FlowInputs {
    FlowInputs::new().with("host", "https://example.test")
}

#[test]
fn compile_requires_no_runtime_inputs_and_execute_rejects_bad_inputs_before_io() {
    let plan = compile(&source());
    for input in [FlowInputs::new(), inputs().with("typo", "unknown")] {
        let transport = FakeTransport::new([]);
        assert!(matches!(
            execute_flow(
                plan.clone(),
                transport.clone(),
                FlowSessionEnvironment::new(input)
            ),
            Err(FlowError::InvalidInputs(_))
        ));
        assert!(transport.requests().is_empty());
    }
}

#[tokio::test]
async fn snapshots_and_repeated_runs_have_independent_inputs_outputs_and_policies() {
    let mut definition = source();
    let plan = compile(&definition);
    definition.steps.clear();
    definition.inputs[1].default = Some(json!({"changed": true}));
    for data in [json!({"note": "a\"b\nc", "enabled": true}), Value::Null] {
        let transport = FakeTransport::new([
            response(json!({"uuid": "server-id"})),
            response(json!({"data": data})),
        ]);
        let options = RequestOptions {
            timeout_ms: Some(321),
            ..RequestOptions::default()
        };
        let events: Vec<_> = execute_flow(
            plan.clone(),
            transport.clone(),
            FlowSessionEnvironment::new(inputs().with("data", data.clone()))
                .with_request_options(options),
        )
        .unwrap()
        .try_collect()
        .await
        .unwrap();
        let FlowEvent::FlowFinished {
            success: true,
            outputs,
        } = events.last().unwrap()
        else {
            panic!("flow failed")
        };
        assert_eq!(outputs["id"].value(), &json!("server-id"));
        assert_eq!(outputs["data"].value(), &data);
        let requests = transport.requests();
        assert_eq!(requests[1].0.url, "https://example.test/echo/server-id");
        let RequestBody::Json(body) = &requests[1].0.body else {
            panic!("missing JSON")
        };
        assert_eq!(
            serde_json::from_str::<Value>(body).unwrap(),
            json!({"id": "server-id", "data": data})
        );
        assert!(requests.iter().all(|(_, supplied)| supplied == &options));
    }
}

#[test]
fn diagnostics_aggregate_and_identify_the_affected_fields_and_steps() {
    let mut definition = source();
    definition.name.clear();
    definition.inputs.push(definition.inputs[0].clone());
    definition.steps[0].request.as_inline_mut().unwrap().url =
        TextTemplate::parts([TemplatePart::input("missing")]);
    definition.steps[0].exports[0].json_path = "$[".into();
    definition.steps[1].id = "seed".into();
    definition.outputs.push(FlowOutputSpec {
        name: "data".into(),
        value: ValueReference::Input("unknown".into()),
    });
    let diagnostics = compile_flow(
        &definition,
        &ApiCatalog::new(),
        &CompileEnvironment::default(),
    )
    .unwrap_err();
    for code in [
        DiagnosticCode::InvalidName,
        DiagnosticCode::DuplicateName,
        DiagnosticCode::UnknownInput,
        DiagnosticCode::InvalidJsonPath,
    ] {
        assert!(diagnostics.iter().any(|diagnostic| diagnostic.code == code));
    }
    assert!(diagnostics
        .iter()
        .any(
            |diagnostic| diagnostic.location.field == "flow.steps[0].request.url.parts[0]"
                && diagnostic.location.step_id.as_deref() == Some("seed")
        ));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.location.field == "flow.outputs[2].name"));
}

#[test]
fn invalid_shapes_paths_and_forward_references_fail_during_compilation() {
    let mut invalid = Vec::new();
    invalid.push(FlowDefinition::new("empty"));
    let mut value = source();
    value.steps[0].request.as_inline_mut().unwrap().body =
        BodyTemplate::Json(TextTemplate::literal("{invalid}"));
    invalid.push(value);
    let mut value = source();
    value.steps[0].checks.push(ResponseCheck::JsonValueEquals {
        path: "$.uuid".into(),
        expected: JsonTemplate::step_output("echo", "document"),
    });
    invalid.push(value);
    let mut value = source();
    let duplicate = value.steps[0].exports[0].clone();
    value.steps[0].exports.push(duplicate);
    invalid.push(value);
    for value in invalid {
        assert!(compile_flow(&value, &ApiCatalog::new(), &CompileEnvironment::default()).is_err());
    }
    assert!(compile_flow(
        &source(),
        &ApiCatalog::new(),
        &CompileEnvironment { max_steps: Some(1) }
    )
    .is_err());
}

#[tokio::test]
async fn catalog_arguments_expand_once_and_plans_do_not_borrow_the_catalog() {
    let mut catalog = ApiCatalog::new();
    catalog.insert(
        "fetch",
        ApiDefinition::new(HttpRequestTemplate::new(
            HttpMethod::GET,
            TextTemplate::parts([TemplatePart::input("base"), TemplatePart::literal("/uuid")]),
        ))
        .parameter("base"),
    );
    let mut definition = source();
    definition.steps[0].request = ApiCall::new("fetch")
        .bind("base", TextTemplate::parts([TemplatePart::input("host")]))
        .into();
    let plan = compile_flow(&definition, &catalog, &CompileEnvironment::default()).unwrap();
    catalog.insert(
        "fetch",
        ApiDefinition::new(HttpRequestTemplate::new(
            HttpMethod::DELETE,
            TextTemplate::literal("https://changed.test"),
        )),
    );
    drop(catalog);
    let transport = FakeTransport::new([
        response(json!({"uuid": "fresh"})),
        response(json!({"data": {"enabled": true, "note": null}})),
    ]);
    let events = run(plan, inputs(), transport.clone()).await;
    assert!(matches!(
        events.last(),
        Some(FlowEvent::FlowFinished { success: true, .. })
    ));
    assert_eq!(transport.requests()[0].0.method, HttpMethod::GET);
    assert_eq!(transport.requests()[0].0.url, "https://example.test/uuid");
}

#[test]
fn catalog_validation_covers_parameters_and_definition_scope() {
    let mut definition = source();
    definition.steps[0].request = ApiCall::new("fetch")
        .bind("typo", TextTemplate::literal("unused"))
        .into();
    let mut catalog = ApiCatalog::new();
    catalog.insert(
        "fetch",
        ApiDefinition::new(HttpRequestTemplate::new(
            HttpMethod::GET,
            TextTemplate::parts([
                TemplatePart::input("not_declared"),
                TemplatePart::step_output("echo", "document"),
            ]),
        ))
        .parameter("base"),
    );
    let diagnostics =
        compile_flow(&definition, &catalog, &CompileEnvironment::default()).unwrap_err();
    for code in [
        DiagnosticCode::MissingArgument,
        DiagnosticCode::UnknownArgument,
        DiagnosticCode::UnknownInput,
        DiagnosticCode::UnavailableOutput,
    ] {
        assert!(diagnostics.iter().any(|diagnostic| diagnostic.code == code));
    }
    assert!(diagnostics
        .iter()
        .any(
            |diagnostic| diagnostic.location.api_id.as_deref() == Some("fetch")
                && diagnostic.location.step_id.as_deref() == Some("seed")
        ));
}

#[tokio::test]
async fn failed_checks_or_exports_do_not_commit_partial_exports_or_flow_results() {
    for fail_export in [false, true] {
        let mut definition = source();
        if fail_export {
            definition.steps[0]
                .exports
                .push(ResponseExport::json("missing", "$.absent"));
        } else {
            definition.steps[0]
                .checks
                .push(ResponseCheck::StatusEquals(201));
        }
        let transport = FakeTransport::new([response(json!({"uuid": "received"}))]);
        let events = run(compile(&definition), inputs(), transport.clone()).await;
        assert!(!events
            .iter()
            .any(|event| matches!(event, FlowEvent::OutputExported { .. })));
        assert!(
            matches!(events.last(), Some(FlowEvent::FlowFinished { success: false, outputs }) if outputs.is_empty())
        );
        assert_eq!(transport.requests().len(), 1);
    }
}

#[tokio::test]
async fn dropping_the_owning_stream_stops_later_steps() {
    let transport = FakeTransport::new([response(json!({"uuid": "id"}))]);
    {
        let stream = execute_flow(
            compile(&source()),
            transport.clone(),
            FlowSessionEnvironment::new(inputs()),
        )
        .unwrap();
        assert!(transport.requests().is_empty());
        let mut stream = std::pin::pin!(stream);
        assert!(matches!(
            stream.next().await,
            Some(Ok(FlowEvent::FlowStarted { .. }))
        ));
        assert!(transport.requests().is_empty());
        while !matches!(
            stream.next().await,
            Some(Ok(FlowEvent::StepFinished { .. }))
        ) {}
    }
    assert_eq!(transport.requests().len(), 1);
}

#[tokio::test]
async fn typed_sensitive_returns_keep_values_but_mask_debug_output() {
    let mut definition = source();
    definition.inputs[1].sensitive = true;
    definition.steps[0].exports[0].sensitive = true;
    let data = json!({"secret": "do-not-print"});
    let events = run(
        compile(&definition),
        inputs().with("data", data.clone()),
        FakeTransport::new([
            response(json!({"uuid": "private-id"})),
            response(json!({"data": data})),
        ]),
    )
    .await;
    let FlowEvent::FlowFinished { outputs, .. } = events.last().unwrap() else {
        panic!()
    };
    assert_eq!(outputs["id"].value(), &json!("private-id"));
    assert!(outputs["id"].is_sensitive());
    assert!(outputs["data"].is_sensitive());
    let debug = format!("{outputs:?}");
    assert!(!debug.contains("private-id"));
    assert!(!debug.contains("do-not-print"));
}

#[tokio::test]
async fn transport_error_messages_redact_known_sensitive_inputs() {
    let mut definition = source();
    definition
        .inputs
        .push(FlowInputSpec::required("token").sensitive());
    let events = run(
        compile(&definition),
        inputs().with("token", "keep-private"),
        FakeTransport::new([Err(HttpError::network("token=keep-private"))]),
    )
    .await;
    assert!(!format!("{events:?}").contains("keep-private"));
    assert!(format!("{events:?}").contains("[REDACTED]"));
}

#[tokio::test]
async fn transport_may_borrow_local_state_without_boxing_the_flow_stream() {
    struct Borrowed<'a>(&'a AtomicUsize);
    impl HttpTransport for Borrowed<'_> {
        async fn execute(
            &self,
            _: postman_http::request::Request,
            _: RequestOptions,
        ) -> Result<HttpResponse, HttpError> {
            self.0.fetch_add(1, Ordering::Relaxed);
            response(json!({"uuid": "borrowed-id"}))
        }
    }
    let mut definition = source();
    definition.steps.truncate(1);
    let calls = AtomicUsize::new(0);
    let events = execute_flow(
        compile(&definition),
        Borrowed(&calls),
        FlowSessionEnvironment::new(inputs()),
    )
    .unwrap();
    fn assert_send(_: &impl Send) {}
    assert_send(&events);
    let events: Vec<_> = events.try_collect().await.unwrap();
    assert!(matches!(
        events.last(),
        Some(FlowEvent::FlowFinished { success: true, .. })
    ));
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

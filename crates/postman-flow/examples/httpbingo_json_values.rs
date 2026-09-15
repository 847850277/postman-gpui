//! Live HTTPBingo example: inputs -> JSON values -> response exports -> another JSON request.
//! Strings are escaped by serialization; numeric-looking strings stay strings.
//! The Rust definition and matching .http.yml fixture compile to the same execution plan.

mod support;

use postman_flow::{
    FlowDefinition, FlowInputSpec, FlowInputs, JsonTemplate, ResponseExport, TemplatePart,
};
use postman_http::request::HttpMethod;
use serde_json::{json, Value};

use support::{equals, json_request};

const FIELDS: [&str; 10] = [
    "text",
    "integer",
    "decimal",
    "enabled",
    "optional",
    "object",
    "array",
    "numeric_string",
    "boolean_string",
    "null_string",
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("postman_flow=debug,info")),
        )
        .init();

    tracing::info!("HTTPBingo: JSON escaping and type preservation across two requests.");
    support::run_live(json_values_definition(), FlowInputs::new()).await
}

fn sample_values() -> [(&'static str, Value); 10] {
    [
        ("text", json!("张\"三\"\n二楼\\东侧\t收件 😀")),
        ("integer", json!(42)),
        ("decimal", json!(19.95)),
        ("enabled", json!(true)),
        ("optional", Value::Null),
        (
            "object",
            json!({"name": "实验\"小学\"", "active": false, "missing": null}),
        ),
        (
            "array",
            json!([7, false, null, "00123", {"line": "第一行\n第二行"}]),
        ),
        ("numeric_string", json!("00123")),
        ("boolean_string", json!("true")),
        ("null_string", json!("null")),
    ]
}

pub(crate) fn json_values_definition() -> FlowDefinition {
    let mut inputs = vec![FlowInputSpec::with_default("host", "https://httpbingo.org")];
    inputs.extend(
        sample_values()
            .into_iter()
            .map(|(name, value)| FlowInputSpec::with_default(name, value)),
    );

    let mut first = json_request(
        "send-values",
        HttpMethod::POST,
        "/anything/flow/json-values",
        JsonTemplate::object(
            FIELDS
                .into_iter()
                .map(|name| (name, JsonTemplate::input(name))),
        ),
    );
    for name in FIELDS {
        first = first
            .check(equals(&format!("$.json.{name}"), TemplatePart::input(name)))
            .export(ResponseExport::json(name, format!("$.json.{name}")));
    }
    first = first.export(ResponseExport::json("document", "$.json"));

    let mut second = json_request(
        "reuse-values",
        HttpMethod::POST,
        "/anything/flow/reuse-values",
        JsonTemplate::object([
            (
                "document",
                JsonTemplate::step_output("send-values", "document"),
            ),
            (
                "fields",
                JsonTemplate::object(
                    FIELDS
                        .into_iter()
                        .map(|name| (name, JsonTemplate::step_output("send-values", name))),
                ),
            ),
            (
                "nested",
                JsonTemplate::array([
                    JsonTemplate::step_output("send-values", "integer"),
                    JsonTemplate::object([
                        ("text", JsonTemplate::step_output("send-values", "text")),
                        (
                            "optional",
                            JsonTemplate::step_output("send-values", "optional"),
                        ),
                    ]),
                ]),
            ),
            // Neither literal strings nor object keys are recursively interpreted.
            (
                "{{literal.key}}",
                JsonTemplate::literal("{{not_a_variable}}"),
            ),
        ]),
    )
    .check(equals(
        "$.json.document",
        TemplatePart::step_output("send-values", "document"),
    ))
    .check(equals("$.json.nested[0]", TemplatePart::input("integer")))
    .check(equals("$.json.nested[1].text", TemplatePart::input("text")))
    .check(equals(
        "$.json.nested[1].optional",
        TemplatePart::input("optional"),
    ));
    for name in FIELDS {
        second = second.check(equals(
            &format!("$.json.fields.{name}"),
            TemplatePart::input(name),
        ));
    }

    FlowDefinition {
        name: "httpbingo-json-values".into(),
        inputs,
        steps: vec![first, second],
        outputs: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use postman_flow::{
        compile_flow, ApiCatalog, BodyTemplate, CompileEnvironment, FlowEvent, StepOutcome,
        TextTemplate,
    };

    use super::*;
    use support::testing::{body, run, HttpBingoFixture};

    #[tokio::test]
    async fn quotes_newlines_and_json_types_survive_input_and_output_bindings() {
        let transport = HttpBingoFixture::default();
        let events = run(
            json_values_definition(),
            FlowInputs::new(),
            transport.clone(),
        )
        .await
        .unwrap();
        assert_eq!(
            events.last(),
            Some(&FlowEvent::FlowFinished {
                success: true,
                outputs: Default::default()
            })
        );
        let requests = transport.requests();
        assert_eq!(requests.len(), 2);
        let first = body(&requests[0]);
        let second = body(&requests[1]);
        for (name, value) in sample_values() {
            assert_eq!(first[name], value, "{name}");
            assert_eq!(second["fields"][name], value, "{name}");
        }
        assert_eq!(second["document"], first);
        assert_eq!(second["nested"][0], json!(42));
        assert_eq!(second["nested"][1]["text"], first["text"]);
        assert_eq!(second["nested"][1]["optional"], Value::Null);
        assert_eq!(second["{{literal.key}}"], "{{not_a_variable}}");
    }

    #[tokio::test]
    async fn root_values_and_empty_collections_are_not_coerced_to_strings() {
        for value in [
            Value::Null,
            json!(false),
            json!(0),
            json!(-42),
            json!(19.95),
            json!(u64::MAX),
            json!(i64::MIN),
            json!("123"),
            json!("null"),
            json!("a\"b\\c\n\t\r\u{0000}\u{0008} 😀"),
            json!([]),
            json!({}),
        ] {
            let plan = FlowDefinition {
                name: "root-json-values".into(),
                inputs: vec![
                    FlowInputSpec::with_default("host", "https://httpbingo.org"),
                    FlowInputSpec::required("value"),
                ],
                steps: vec![
                    json_request(
                        "seed",
                        HttpMethod::POST,
                        "/anything/root",
                        JsonTemplate::input("value"),
                    )
                    .check(equals("$.json", TemplatePart::input("value")))
                    .export(ResponseExport::json("value", "$.json")),
                    json_request(
                        "copy",
                        HttpMethod::POST,
                        "/anything/root-copy",
                        JsonTemplate::step_output("seed", "value"),
                    )
                    .check(equals("$.json", TemplatePart::input("value"))),
                ],
                outputs: Vec::new(),
            };
            let transport = HttpBingoFixture::default();
            let events = run(
                plan,
                FlowInputs::new().with("value", value.clone()),
                transport.clone(),
            )
            .await
            .unwrap();
            assert_eq!(
                events.last(),
                Some(&FlowEvent::FlowFinished {
                    success: true,
                    outputs: Default::default()
                })
            );
            for request in transport.requests() {
                assert_eq!(body(&request), value);
            }
        }
    }

    #[test]
    fn invalid_references_inside_nested_json_are_rejected_before_execution() {
        for (reference, expected) in [
            (JsonTemplate::input("missing"), "undeclared input"),
            (
                JsonTemplate::step_output("reuse-values", "x"),
                "earlier step",
            ),
            (
                JsonTemplate::step_output("send-values", "text"),
                "earlier step",
            ),
        ] {
            let mut plan = json_values_definition();
            plan.steps[0].request.as_inline_mut().unwrap().body = BodyTemplate::JsonValue(
                JsonTemplate::array([JsonTemplate::object([("nested", reference)])]),
            );
            let transport = HttpBingoFixture::default();
            let result = compile_flow(&plan, &ApiCatalog::new(), &CompileEnvironment::default());
            assert!(
                matches!(result, Err(errors) if errors.iter().any(|error| error.message.contains(expected)))
            );
            assert!(transport.requests().is_empty());
        }
    }

    #[test]
    fn an_unknown_export_on_an_earlier_step_is_rejected() {
        let mut plan = json_values_definition();
        plan.steps[1].request.as_inline_mut().unwrap().body =
            BodyTemplate::JsonValue(JsonTemplate::step_output("send-values", "missing"));
        assert!(compile_flow(&plan, &ApiCatalog::new(), &CompileEnvironment::default()).is_err());
    }

    #[tokio::test]
    async fn raw_json_text_keeps_its_explicit_escaping_contract() {
        let mut plan = json_values_definition();
        plan.steps.truncate(1);
        plan.steps[0].checks.truncate(1);
        plan.steps[0].exports.clear();
        plan.steps[0].request.as_inline_mut().unwrap().body =
            BodyTemplate::Json(TextTemplate::parts([
                TemplatePart::literal("{\"text\":\""),
                TemplatePart::input("text"),
                TemplatePart::literal("\"}"),
            ]));
        let transport = HttpBingoFixture::default();
        let events = run(plan.clone(), FlowInputs::new(), transport.clone())
            .await
            .unwrap();
        assert_eq!(
            events.last(),
            Some(&FlowEvent::FlowFinished {
                success: false,
                outputs: Default::default()
            })
        );
        assert!(events.iter().any(|event| matches!(
            event,
            FlowEvent::StepFinished { outcome: StepOutcome::Failed { message }, .. }
                if message.contains("rendered JSON request body is invalid")
        )));
        assert!(transport.requests().is_empty());

        // Explicitly escaped raw text is still supported; its semantics were not silently changed.
        plan.steps[0].request.as_inline_mut().unwrap().body =
            BodyTemplate::Json(TextTemplate::literal(r#"{"text":"a\"b\nc"}"#));
        let events = run(plan, FlowInputs::new(), transport.clone())
            .await
            .unwrap();
        assert_eq!(
            events.last(),
            Some(&FlowEvent::FlowFinished {
                success: true,
                outputs: Default::default()
            })
        );
        assert_eq!(body(&transport.requests()[0]), json!({"text": "a\"b\nc"}));
    }

    #[tokio::test]
    async fn a_failed_response_stops_json_output_consumers() {
        let transport = HttpBingoFixture::default();
        transport.fail_on_request(0);
        let events = run(
            json_values_definition(),
            FlowInputs::new(),
            transport.clone(),
        )
        .await
        .unwrap();
        assert_eq!(
            events.last(),
            Some(&FlowEvent::FlowFinished {
                success: false,
                outputs: Default::default()
            })
        );
        assert_eq!(transport.requests().len(), 1);
        assert!(!events
            .iter()
            .any(|event| matches!(event, FlowEvent::OutputExported { .. })));
    }
}

#[test]
fn native_yaml_compiles_to_the_same_plan_as_the_rust_definition() {
    let document =
        postman_flow::parse_flow_yaml(include_str!("flows/httpbingo_json_values.http.yml"))
            .unwrap();
    let environment = postman_flow::CompileEnvironment::default();
    let native = postman_flow::compile_flow(&document.flow, &document.apis, &environment).unwrap();
    let constructed = postman_flow::compile_flow(
        &json_values_definition(),
        &postman_flow::ApiCatalog::new(),
        &environment,
    )
    .unwrap();
    assert_eq!(native, constructed);
    let saved = postman_flow::write_flow_yaml(&document).unwrap();
    assert_eq!(postman_flow::parse_flow_yaml(&saved).unwrap(), document);
}

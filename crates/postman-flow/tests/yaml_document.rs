mod support;
use postman_flow::*;
use postman_http::request::{HttpMethod, RequestBody};
use serde_json::{json, Value};
use support::{response, run, FakeTransport};

const CATALOG: &str = include_str!("../examples/flows/httpbingo_catalog.http.yml");

fn document() -> FlowDocument {
    parse_flow_yaml(CATALOG).unwrap()
}

fn altered(change: impl FnOnce(&mut Value)) -> Result<FlowDocument, DocumentError> {
    let mut value: Value = yaml_serde::from_str(CATALOG).unwrap();
    change(&mut value);
    parse_flow_yaml(&yaml_serde::to_string(&value).unwrap())
}

#[tokio::test]
async fn yaml_catalog_flow_executes_and_returns_typed_named_values() {
    let document = document();
    let plan = compile_flow(
        &document.flow,
        &document.apis,
        &CompileEnvironment::default(),
    )
    .unwrap();
    let returned = json!({"client": "a\"b\nc", "correlation_id": "server-generated", "optional": null, "literal_data": {"input": "this-is-data"}});
    let transport = FakeTransport::new([
        response(json!({"uuid": "server-generated"})),
        response(json!({"json": returned})),
    ]);
    let events = run(
        plan,
        FlowInputs::new().with("client", "a\"b\nc"),
        transport.clone(),
    )
    .await;
    let FlowEvent::FlowFinished {
        success: true,
        outputs,
    } = events.last().unwrap()
    else {
        panic!("flow failed")
    };
    assert_eq!(
        outputs["correlation_id"].value(),
        &json!("server-generated")
    );
    assert_eq!(outputs["document"].value(), &returned);
    let requests = transport.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].0.method, HttpMethod::GET);
    assert_eq!(requests[0].0.url, "https://httpbingo.org/uuid");
    assert_eq!(
        requests[1].0.url,
        "https://httpbingo.org/anything/yaml/server-generated"
    );
    let RequestBody::Json(body) = &requests[1].0.body else {
        panic!()
    };
    assert_eq!(serde_json::from_str::<Value>(body).unwrap(), returned);
}

#[test]
fn canonical_roundtrip_preserves_catalog_layout_and_null_defaults() {
    let document = document();
    let yaml = write_flow_yaml(&document).unwrap();
    assert_eq!(parse_flow_yaml(&yaml).unwrap(), document);
    assert_eq!(document.flow.inputs[2].default, Some(Value::Null));
    assert_eq!(document.editor.nodes["echo"].x, 420.0);
    assert!(!yaml.contains("!literal"));
    assert_eq!(
        yaml,
        write_flow_yaml(&parse_flow_yaml(&yaml).unwrap()).unwrap()
    );
}

#[test]
fn incomplete_drafts_save_but_cannot_execute_until_compiled() {
    let mut document = FlowDocument::new(FlowDefinition::new("draft"));
    document.flow.inputs = vec![
        FlowInputSpec::required("required"),
        FlowInputSpec::with_default("nullable", Value::Null),
    ];
    document
        .editor
        .nodes
        .insert("unfinished".into(), NodePosition { x: -25.0, y: 13.5 });
    let loaded = parse_flow_yaml(&write_flow_yaml(&document).unwrap()).unwrap();
    assert_eq!(loaded, document);
    assert_eq!(loaded.flow.inputs[0].default, None);
    assert_eq!(loaded.flow.inputs[1].default, Some(Value::Null));
    assert!(compile_flow(&loaded.flow, &loaded.apis, &CompileEnvironment::default()).is_err());
}

#[test]
fn schema_errors_reject_unknown_fields_kinds_methods_and_ambiguous_expressions() {
    for mutate in [
        (|value: &mut Value| value["unexpected"] = json!(1)) as fn(&mut Value),
        |value| value["flow"]["parallel"] = json!(true),
        |value| value["flow"]["inputs"][0]["defualt"] = json!("typo"),
        |value| value["flow"]["steps"][1]["request"]["method"] = json!("FETCH"),
        |value| value["flow"]["steps"][1]["request"]["kind"] = json!("shell"),
        |value| value["flow"]["steps"][1]["request"]["body"]["kind"] = json!("xml"),
        |value| {
            value["flow"]["steps"][1]["request"]["body"]["value"] =
                json!({"input": "client", "literal": "conflict"})
        },
        |value| value["flow"]["steps"][1]["request"]["url"] = json!({"literal": 123}),
        |value| {
            value["flow"]["steps"][1]["request"]["url"] =
                json!({"output": {"step": "seed", "name": "correlation_id", "typo": true}})
        },
        |value| value["flow"]["steps"][1]["checks"][0]["kind"] = json!("javascript"),
        |value| value["flow"]["steps"][1]["checks"][1]["kind"] = json!("jsonpath_template"),
        |value| value["flow"]["outputs"][0]["value"] = json!({"literal": "unsupported"}),
        |value| value["editor"]["nodes"]["seed"]["z"] = json!(1),
    ] {
        let error = altered(mutate).unwrap_err();
        assert_eq!(error.code, DocumentErrorCode::Schema, "{error}");
        assert!(!error.field.is_empty());
    }
    let error = altered(|value| value["flow"]["steps"][1]["request"]["method"] = json!("FETCH"))
        .unwrap_err();
    assert!(error.field.starts_with("flow.steps[1].request"), "{error}");
}

#[test]
fn unsupported_versions_are_rejected_before_decoding_their_schema() {
    let error = parse_flow_yaml("schema_version: 999\nanything: [may, change]").unwrap_err();
    assert_eq!(error.code, DocumentErrorCode::UnsupportedVersion);
    assert_eq!(error.field, "schema_version");
    for version in ["null", "-1", "'1'", "1.5"] {
        assert!(parse_flow_yaml(&format!(
            "schema_version: {version}\nflow: {{name: test, steps: []}}"
        ))
        .is_err());
    }
}

#[test]
fn syntax_errors_include_line_and_column_and_duplicate_keys_never_overwrite() {
    let error = parse_flow_yaml("schema_version: 1\nflow: [\n").unwrap_err();
    assert_eq!(error.code, DocumentErrorCode::Syntax);
    assert!(error.line.is_some());
    assert!(error.column.is_some());
    for invalid in [
        "schema_version: 1\nschema_version: 1\nflow: {name: test, steps: []}",
        "schema_version: 1\nflow: {name: test, name: other, steps: []}",
        "schema_version: 1\nflow:\n  name: test\n  steps: []\n  inputs:\n    - name: data\n      default: {key: 1, key: 2}",
        "schema_version: 1\nflow: {name: test, steps: []}\n---\nschema_version: 1",
    ] { assert!(parse_flow_yaml(invalid).is_err(), "{invalid}"); }
}

#[test]
fn non_json_values_tags_and_nonfinite_coordinates_are_rejected() {
    for invalid in [
        "schema_version: 1\nflow: {name: test, steps: []}\neditor: {nodes: {a: {x: .nan, y: 0}}}",
        "schema_version: 1\nflow: {name: test, steps: []}\neditor: {nodes: {a: {x: .inf, y: 0}}}",
        "schema_version: 1\nflow: {name: test, steps: [], inputs: [{name: value, default: !custom tagged}]}",
        "schema_version: 1\nflow: {name: test, steps: [], inputs: [{name: value, default: {3: value}}]}",
    ] { assert!(parse_flow_yaml(invalid).is_err(), "{invalid}"); }
    let mut document = document();
    document.editor.nodes.insert(
        "invalid".into(),
        NodePosition {
            x: f64::INFINITY,
            y: 0.0,
        },
    );
    assert!(write_flow_yaml(&document).is_err());
}

#[test]
fn layout_changes_do_not_change_the_execution_plan() {
    let mut document = document();
    let before = compile_flow(
        &document.flow,
        &document.apis,
        &CompileEnvironment::default(),
    )
    .unwrap();
    document.editor.nodes.clear();
    document.editor.nodes.insert(
        "seed".into(),
        NodePosition {
            x: 999.0,
            y: -200.0,
        },
    );
    let after = compile_flow(
        &document.flow,
        &document.apis,
        &CompileEnvironment::default(),
    )
    .unwrap();
    assert_eq!(before, after);
}

#[test]
fn header_duplicates_and_all_body_modes_survive_save_and_load() {
    for body in [
        BodyTemplate::None,
        BodyTemplate::Raw(TextTemplate::literal("raw\nbody")),
        BodyTemplate::UrlEncoded(TextTemplate::literal("a=1&a=2")),
        BodyTemplate::Json(TextTemplate::literal(r#"{"number":"123"}"#)),
        BodyTemplate::JsonValue(JsonTemplate::String(TextTemplate::parts([
            TemplatePart::literal("Hello, "),
            TemplatePart::input("client"),
        ]))),
    ] {
        let mut document = document();
        let request = document.flow.steps[1].request.as_inline_mut().unwrap();
        request.body = body;
        request.headers.extend([
            (
                TextTemplate::literal("X-Repeated"),
                TextTemplate::literal("one"),
            ),
            (
                TextTemplate::literal("X-Repeated"),
                TextTemplate::literal("two"),
            ),
        ]);
        let roundtrip = parse_flow_yaml(&write_flow_yaml(&document).unwrap()).unwrap();
        assert_eq!(roundtrip, document);
    }
}

#[test]
fn json_number_and_string_types_remain_distinct_in_document_roundtrips() {
    for value in [
        json!(u64::MAX),
        json!(i64::MIN),
        json!(1.5),
        json!(false),
        Value::Null,
        json!("00123"),
        json!("true"),
        json!("null"),
        json!("a\"b\\c\n\t\u{0000}"),
        json!({"literal": {"input": "data"}, "array": [true, 3, null], "<<": {"ordinary": "data"}}),
    ] {
        let mut document = document();
        document.flow.inputs[2].default = Some(value);
        assert_eq!(
            parse_flow_yaml(&write_flow_yaml(&document).unwrap()).unwrap(),
            document
        );
    }
}

#[test]
fn oversized_documents_are_rejected() {
    let error = parse_flow_yaml(&" ".repeat(2 * 1024 * 1024 + 1)).unwrap_err();
    assert_eq!(error.code, DocumentErrorCode::LimitExceeded);
}

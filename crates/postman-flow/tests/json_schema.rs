use postman_flow::{
    compile_flow, parse_flow_yaml, write_flow_yaml, CompileEnvironment, FLOW_DOCUMENT_SCHEMA_JSON,
    FLOW_DOCUMENT_VERSION,
};
use serde_json::{json, Value};

fn validator() -> jsonschema::Validator {
    let schema: Value = serde_json::from_str(FLOW_DOCUMENT_SCHEMA_JSON).unwrap();
    assert_eq!(
        schema["properties"]["schema_version"]["const"],
        FLOW_DOCUMENT_VERSION
    );
    jsonschema::validator_for(&schema).unwrap()
}

fn minimal() -> Value {
    json!({"schema_version":1,"flow":{"name":"schema-test","steps":[{
        "id":"read","request":{"kind":"http","method":"GET","url":{"literal":"https://example.test"}}
    }]}})
}

#[test]
fn shipped_examples_match_schema_before_and_after_canonical_serialization() {
    let validator = validator();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/flows");
    let mut checked = 0;
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("yml") {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        let document = parse_flow_yaml(&source).unwrap();
        compile_flow(
            &document.flow,
            &document.apis,
            &CompileEnvironment::default(),
        )
        .unwrap();
        for source in [source, write_flow_yaml(&document).unwrap()] {
            let value: Value = yaml_serde::from_str(&source).unwrap();
            let errors: Vec<_> = validator
                .iter_errors(&value)
                .map(|e| e.to_string())
                .collect();
            assert!(errors.is_empty(), "{}: {errors:?}", path.display());
        }
        checked += 1;
    }
    assert!(checked > 0);
}

#[test]
fn schema_and_parser_reject_malformed_document_shapes() {
    let validator = validator();
    for mutate in [
        (|v: &mut Value| v["schema_version"] = json!(999)) as fn(&mut Value),
        |v| v["flow"]["unknown"] = json!(true),
        |v| v["flow"]["steps"][0]["request"] = Value::Null,
        |v| v["flow"]["steps"][0]["request"]["method"] = json!("FETCH"),
        |v| v["flow"]["steps"][0]["request"]["url"] = json!({"literal":"x","input":"host"}),
        |v| v["flow"]["steps"][0]["kind"] = json!("repeat_until"),
        |v| {
            v["flow"]["steps"][0]["checks"] =
                json!([{"kind":"jsonpath_template","path":"$","equals":{"literal":"true"}}])
        },
    ] {
        let mut value = minimal();
        mutate(&mut value);
        assert!(!validator.is_valid(&value), "schema accepted {value}");
        assert!(
            parse_flow_yaml(&value.to_string()).is_err(),
            "parser accepted {value}"
        );
    }
}

#[test]
fn schema_is_structural_and_does_not_replace_compilation_or_draft_parsing() {
    let validator = validator();
    let mut value = minimal();
    assert!(validator.is_valid(&value));
    value["flow"]["steps"][0]["request"]["url"] = json!({"input":"missing"});
    assert!(validator.is_valid(&value));
    let document = parse_flow_yaml(&value.to_string()).unwrap();
    assert!(compile_flow(
        &document.flow,
        &document.apis,
        &CompileEnvironment::default()
    )
    .is_err());

    value["flow"]["steps"] = json!([]);
    assert!(!validator.is_valid(&value));
    assert!(
        parse_flow_yaml(&value.to_string()).is_ok(),
        "editable drafts remain parseable"
    );
}

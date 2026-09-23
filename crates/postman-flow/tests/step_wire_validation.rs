use postman_flow::{
    compile_flow, parse_flow_yaml, write_flow_yaml, CompileEnvironment, DocumentErrorCode,
};
use serde_json::{json, Value};

fn http() -> Value {
    json!({"id": "fetch", "request": {
        "kind": "http", "method": "POST", "url": {"literal": "https://example.test"}
    }})
}

fn each() -> Value {
    json!({"id": "each", "kind": "for_each", "items": {"literal": [1]},
        "as": "item", "steps": [http()]})
}

fn poll() -> Value {
    json!({"id": "poll", "kind": "repeat_until",
        "until": {"eq": [{"literal": true}, {"literal": true}]}, "steps": [http()]})
}

fn source(step: Value) -> String {
    yaml_serde::to_string(&json!({"schema_version": 1, "flow": {
        "name": "wire validation", "steps": [step]
    }}))
    .unwrap()
}

fn rejected(step: Value, field: &str) {
    let error = parse_flow_yaml(&source(step)).unwrap_err();
    assert_eq!(error.code, DocumentErrorCode::Schema, "{error}");
    assert_eq!(error.field, format!("flow.steps[0].{field}"), "{error}");
}

#[test]
fn required_variant_fields_cannot_be_missing_or_null() {
    for (step, fields) in [
        (http(), vec!["request"]),
        (each(), vec!["items", "as", "steps"]),
        (poll(), vec!["until", "steps"]),
    ] {
        for field in fields {
            let mut missing = step.clone();
            missing.as_object_mut().unwrap().remove(field);
            rejected(missing, field);
            let mut null = step.clone();
            null[field] = Value::Null;
            rejected(null, field);
        }
    }
}

#[test]
fn fields_for_other_variants_are_rejected_even_when_empty_or_null() {
    for (step, fields) in [
        (
            http(),
            vec![
                "items",
                "as",
                "index_as",
                "max_iterations",
                "on_error",
                "steps",
                "until",
                "interval_ms",
                "timeout_ms",
                "fail_when",
                "carry",
            ],
        ),
        (
            each(),
            vec![
                "request",
                "checks",
                "until",
                "interval_ms",
                "timeout_ms",
                "fail_when",
                "carry",
            ],
        ),
        (
            poll(),
            vec![
                "request", "checks", "items", "as", "index_as", "on_error", "exports",
            ],
        ),
    ] {
        for field in fields {
            for value in [Value::Null, json!([]), json!({})] {
                let mut invalid = step.clone();
                invalid[field] = value;
                rejected(invalid, field);
            }
        }
    }
    for kind in [Value::Null, json!("http"), json!("for-each"), json!(1)] {
        let mut invalid = http();
        invalid["kind"] = kind;
        rejected(invalid, "kind");
    }
}

#[test]
fn nested_loop_errors_keep_the_full_field_path() {
    let mut invalid = poll();
    invalid.as_object_mut().unwrap().remove("until");
    let mut outer = each();
    outer["steps"] = json!([invalid]);
    rejected(outer, "steps[0].until");
}

#[test]
fn exports_cannot_mix_response_paths_and_collection_references() {
    let reference = json!({"step": "fetch", "name": "value"});
    for (step, required, valid, forbidden, other) in [
        (
            http(),
            "path",
            json!("$.value"),
            "collect",
            reference.clone(),
        ),
        (each(), "collect", reference, "path", json!("$.value")),
    ] {
        for null in [false, true] {
            let mut invalid = step.clone();
            invalid["exports"] = json!([{"name": "result"}]);
            if null {
                invalid["exports"][0][required] = Value::Null;
            }
            rejected(invalid, &format!("exports[0].{required}"));
        }
        let mut invalid = step;
        invalid["exports"] = json!([{"name": "result", required: valid, forbidden: other}]);
        rejected(invalid, &format!("exports[0].{forbidden}"));
    }
    let mut invalid = each();
    invalid["exports"] = json!([{"name": "result", "collect": {"step": "fetch", "name": "value"}, "sensitive": false}]);
    rejected(invalid, "exports[0].sensitive");
}

#[test]
fn valid_variants_and_structurally_complete_drafts_round_trip() {
    for mut step in [http(), each(), poll()] {
        let document = parse_flow_yaml(&source(step.clone())).unwrap();
        let plan = compile_flow(
            &document.flow,
            &document.apis,
            &CompileEnvironment::default(),
        );
        assert!(plan.is_ok(), "{plan:?}");
        let yaml = write_flow_yaml(&document).unwrap();
        assert_eq!(parse_flow_yaml(&yaml).unwrap(), document);

        if step.get("kind").is_some() {
            step["steps"] = json!([]);
        } else {
            step["request"]["url"] = json!({"input": "undeclared"});
        }
        let draft = parse_flow_yaml(&source(step)).unwrap();
        let yaml = write_flow_yaml(&draft).unwrap();
        assert_eq!(parse_flow_yaml(&yaml).unwrap(), draft);
        assert!(compile_flow(&draft.flow, &draft.apis, &CompileEnvironment::default()).is_err());
    }
}

#[test]
fn explicit_null_expression_is_distinct_from_a_missing_items_field() {
    let mut step = each();
    step["items"] = json!({"literal": null});
    let document = parse_flow_yaml(&source(step)).unwrap();
    let yaml = write_flow_yaml(&document).unwrap();
    assert_eq!(parse_flow_yaml(&yaml).unwrap(), document);
}

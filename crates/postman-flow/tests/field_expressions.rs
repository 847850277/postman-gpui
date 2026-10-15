use postman_flow::*;
use postman_http::request::HttpMethod;
use serde_json::json;

#[test]
fn field_codec_matches_document_codec_without_resolving_or_stringifying() {
    let texts = [
        json!({"literal":"https://httpbingo.org/{{literal}}/中文:\""}),
        json!({"input":"base_url"}),
        json!({"output":{"step":"stable-id","name":"token"}}),
        json!({"concat":[{"input":"base_url"},{"literal":"/post"},{"coalesce":[{"output":{"step":"a","name":"v"}},{"literal":"fallback"}]}]}),
        json!({"calc":"inputs.count + 1"}),
        json!({"concat":[]}),
    ];
    for source in texts {
        let url = parse_text_expression(&source.to_string()).unwrap();
        assert_eq!(
            parse_text_expression(&write_text_expression(&url).unwrap()).unwrap(),
            url
        );
        let header_source = json!([{"name":{"literal":"X-Test"},"value":source}, {"name":{"literal":"X-Test"},"value":{"literal":""}}]);
        let headers = parse_header_expressions(&header_source.to_string()).unwrap();
        assert_eq!(
            parse_header_expressions(&write_header_expressions(&headers).unwrap()).unwrap(),
            headers
        );
        let bodies = [
            json!({"kind":"none"}),
            json!({"kind":"json", "value":{"object":{"typed":{"literal":[true,12,null,{"input":"literal data"}]},"ref":{"input":"count"},"str":{"string":source.clone()}}}}),
            json!({"kind":"json_template","value":source.clone()}),
            json!({"kind":"raw","value":source.clone()}),
            json!({"kind":"url_encoded","value":source.clone()}),
            json!({"kind":"file","value":source.clone()}),
        ];
        for body in bodies {
            let body = parse_body_expression(&body.to_string()).unwrap();
            assert_eq!(
                parse_body_expression(&write_body_expression(&body).unwrap()).unwrap(),
                body
            );
            let mut request = HttpRequestTemplate::new(HttpMethod::POST, url.clone());
            request.headers = headers.clone();
            request.body = body;
            let mut flow = FlowDefinition::new("Field equivalence");
            flow.steps.push(HttpStepDefinition::new(
                "stable-id",
                "Display name",
                request,
            ));
            let document = FlowDocument::new(flow);
            assert_eq!(
                parse_flow_yaml(&write_flow_yaml(&document).unwrap()).unwrap(),
                document
            );
        }
    }
    assert_eq!(parse_body_expression(" \n ").unwrap(), BodyTemplate::None);
    assert!(parse_header_expressions("").unwrap().is_empty());
}

#[test]
fn malformed_shapes_and_unknown_fields_are_errors_not_empty_values() {
    for source in [
        "{",
        "null",
        r#"{"literal":12}"#,
        r#"{"input":"a","literal":"b"}"#,
        r#"{"output":{"step":"a","name":"v","extra":1}}"#,
        r#"{"literal":"x"} false"#,
    ] {
        assert!(parse_text_expression(source).is_err(), "{source}");
    }
    for source in [
        "[",
        r#"[{"name":{"literal":"x"}}]"#,
        r#"[{"name":{"literal":"x"},"value":{"literal":"y"},"extra":1}]"#,
    ] {
        assert!(parse_header_expressions(source).is_err(), "{source}");
    }
    for source in [
        "{",
        "null",
        r#"{"kind":"xml","value":{"literal":"x"}}"#,
        r#"{"kind":"none","value":1}"#,
        r#"{"kind":"raw","value":{"object":{}}}"#,
        r#"{"kind":"json","value":{"literal":true},"extra":1}"#,
    ] {
        assert!(parse_body_expression(source).is_err(), "{source}");
    }
    assert!(parse_text_expression(&format!(
        r#"{{"literal":"{}"}}"#,
        "x".repeat(2 * 1024 * 1024)
    ))
    .is_err());
}

#[test]
fn condition_fields_preserve_all_operators_and_typed_operands() {
    let operands = [
        json!({"literal":"{{literal}}"}),
        json!({"literal":null}),
        json!({"literal":true}),
        json!({"literal":2}),
        json!({"literal":[1,false,{"input":"literal data"}]}),
        json!({"array":[{"input":"name"},{"literal":null}]}),
        json!({"object":{"v":{"output":{"step":"query-status","name":"status"}}}}),
        json!({"coalesce":[{"input":"name"},{"calc":"inputs.count + 1"}]}),
    ];
    for operand in operands {
        for operator in ["eq", "ne", "gt", "gte", "lt", "lte", "in"] {
            let source = json!({"and":[{operator:[operand,{"literal":2}]},{"or":[{"not":{"eq":[{"literal":false},{"literal":null}]}}]}]});
            let condition = parse_condition_expression(&source.to_string())
                .unwrap()
                .unwrap();
            assert_eq!(
                parse_condition_expression(&write_condition_expression(&condition).unwrap())
                    .unwrap(),
                Some(condition.clone())
            );
            let mut flow = FlowDefinition::new("Condition codec");
            flow.steps.push(
                HttpStepDefinition::new(
                    "request",
                    "Request",
                    HttpRequestTemplate::new(
                        HttpMethod::GET,
                        TextTemplate::literal("https://httpbingo.org/get"),
                    ),
                )
                .when(condition),
            );
            let doc = FlowDocument::new(flow);
            assert_eq!(
                parse_flow_yaml(&write_flow_yaml(&doc).unwrap()).unwrap(),
                doc
            );
        }
    }
    assert_eq!(parse_condition_expression(" \n").unwrap(), None);
    for source in [
        "{",
        "null",
        r#"{"eq":[1,2]}"#,
        r#"{"eq":[{"literal":2}]}"#,
        r#"{"and":{}}"#,
        r#"{"not":[]}"#,
        r#"{"eq":[{"literal":1},{"literal":1}],"extra":true}"#,
        r#"{"unknown":[]}"#,
        r#"{"eq":[{"literal":1},{"literal":1}]} false"#,
    ] {
        assert!(parse_condition_expression(source).is_err(), "{source}");
    }
    assert!(parse_condition_expression(&format!(
        r#"{{"eq":[{{"literal":"{}"}},{{"literal":0}}]}}"#,
        "x".repeat(2 * 1024 * 1024)
    ))
    .is_err());
    assert!(parse_condition_expression(&format!(
        "{}{}{}",
        "{\"not\":".repeat(150),
        r#"{"eq":[{"literal":1},{"literal":1}]}"#,
        "}".repeat(150)
    ))
    .is_err());
}

#[test]
fn editor_validation_uses_the_engine_subset() {
    for status in [100, 200, 599] {
        assert!(validate_response_status(status).is_ok());
    }
    for status in [0, 99, 600, u16::MAX] {
        assert!(validate_response_status(status).is_err());
    }
    assert!(validate_definition_name(" ").is_err());
    assert!(validate_definition_name("中文.id").is_ok());
    for path in ["$", "$.json.id", "$.items[0].id"] {
        assert!(validate_json_path(path).is_ok(), "{path}");
    }
    for path in [
        "",
        "json.id",
        "$.items[*]",
        "$..id",
        "$.items[?(@.id)]",
        "$[\"id\"]",
    ] {
        assert!(validate_json_path(path).is_err(), "{path}");
    }
}

#[test]
fn input_values_share_document_limits_and_preserve_literal_types() {
    for value in [
        json!(null),
        json!(false),
        json!(0),
        json!("00123"),
        json!("{{name}}"),
        json!([1, null]),
        json!({"input":"literal"}),
    ] {
        let parsed = parse_input_value(&value.to_string()).unwrap();
        assert_eq!(parsed, value);
        let mut flow = FlowDefinition::new("Input values");
        flow.inputs = vec![
            FlowInputSpec::required("required"),
            FlowInputSpec::with_default("value", parsed).sensitive(),
        ];
        let document = FlowDocument::new(flow);
        assert_eq!(
            parse_flow_yaml(&write_flow_yaml(&document).unwrap()).unwrap(),
            document
        );
    }
    for source in ["", "null false", "{", &"[".repeat(200)] {
        assert!(parse_input_value(source).is_err());
    }
    assert_eq!(
        parse_input_value(&" ".repeat(2 * 1024 * 1024 + 1))
            .unwrap_err()
            .code,
        DocumentErrorCode::LimitExceeded
    );
}

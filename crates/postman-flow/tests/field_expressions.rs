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

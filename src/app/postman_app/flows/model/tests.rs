use super::*;
use postman_flow::{
    parse_flow_yaml, write_flow_yaml, ConditionExpr, JsonTemplate, RequestOptionOverrides,
};

#[test]
fn structural_edits_preserve_ids_references_and_opaque_definitions() {
    for source in [
        include_str!(
            "../../../../../crates/postman-flow/examples/flows/httpbingo_catalog.http.yml"
        ),
        include_str!("../../../../../crates/postman-flow/examples/flows/httpbingo_loops.http.yml"),
    ] {
        let document = parse_flow_yaml(source).unwrap();
        let mut library = FlowLibrary::default();
        let flow = library.insert(document.clone());
        let first = document.flow.steps[0].id.clone();
        assert!(library
            .reorder_step(flow, &first, StepDirection::Up)
            .is_none());
        assert_eq!(library.active().unwrap().revision(), 0);
        if document.flow.steps.len() > 1 {
            library
                .reorder_step(flow, &first, StepDirection::Down)
                .unwrap();
            let mut expected = document.clone();
            expected.flow.steps.swap(0, 1);
            assert_eq!(library.active().unwrap().document(), &expected);
            assert_eq!(library.active().unwrap().selected().unwrap().id, first);
            assert_eq!(
                parse_flow_yaml(&write_flow_yaml(&expected).unwrap()).unwrap(),
                expected
            );
            library
                .reorder_step(flow, &first, StepDirection::Up)
                .unwrap();
            assert_eq!(library.active().unwrap().document(), &document);
        }
        let before = library.active().unwrap().revision();
        let removed = library.remove_step(flow, &first).unwrap();
        let mut expected = document.clone();
        expected.flow.steps.remove(0);
        let surviving = step_ids(&expected.flow.steps);
        expected
            .editor
            .nodes
            .retain(|id, _| !removed.ids.contains(id) || surviving.contains(id));
        assert_eq!(library.active().unwrap().document(), &expected);
        assert_eq!(library.active().unwrap().revision(), before + 1);
        assert_eq!(
            parse_flow_yaml(&write_flow_yaml(&expected).unwrap()).unwrap(),
            expected
        );
        assert!(library.remove_step(flow, &first).is_none());
        library.edit_step(flow, &first, &StepEdit::Invalid);
        assert_eq!(library.active().unwrap().revision(), before + 1);
    }
}

#[test]
fn removal_selection_tombstones_and_flow_isolation() {
    let mut library = FlowLibrary::default();
    let first = library.create("First").unwrap();
    for _ in 0..3 {
        library.append_step();
    }
    library.select_step("request-2");
    library.remove_step(first, "request-2").unwrap();
    assert_eq!(
        library.active().unwrap().selected().unwrap().id,
        "request-3"
    );
    library.remove_step(first, "request-3").unwrap();
    assert_eq!(
        library.active().unwrap().selected().unwrap().id,
        "request-1"
    );
    let second = library.create("Second").unwrap();
    library.append_step();
    let other = library.active().unwrap().document().clone();
    library.remove_step(first, "request-1").unwrap();
    assert_eq!(library.active().unwrap().id(), second);
    assert_eq!(library.active().unwrap().document(), &other);
    library.select(first);
    assert!(library.active().unwrap().selected().is_none());
    library.append_step();
    assert_eq!(
        library.active().unwrap().selected().unwrap().id,
        "request-4"
    );
    assert!(library
        .reorder_step(first, "request-4", StepDirection::Down)
        .is_none());
    assert!(library
        .reorder_step(first, "request-4", StepDirection::Up)
        .is_none());
    assert!(library.remove_step(FlowId(999), "request-4").is_none());
}

#[test]
fn imported_nested_ids_are_reserved_after_removal() {
    let mut document = parse_flow_yaml(include_str!(
        "../../../../../crates/postman-flow/examples/flows/httpbingo_loops.http.yml"
    ))
    .unwrap();
    let loop_step = document
        .flow
        .steps
        .iter_mut()
        .find(|s| {
            matches!(
                s.request,
                HttpRequestSource::ForEach(_) | HttpRequestSource::RepeatUntil(_)
            )
        })
        .unwrap();
    match &mut loop_step.request {
        HttpRequestSource::ForEach(v) => v.steps[0].id = "request-1".into(),
        HttpRequestSource::RepeatUntil(v) => v.steps[0].id = "request-1".into(),
        _ => unreachable!(),
    }
    let ids: Vec<_> = document.flow.steps.iter().map(|s| s.id.clone()).collect();
    let mut library = FlowLibrary::default();
    let flow = library.insert(document);
    for id in ids {
        library.remove_step(flow, &id).unwrap();
    }
    library.append_step();
    assert_eq!(
        library.active().unwrap().selected().unwrap().id,
        "request-2"
    );
}

#[test]
fn structural_edits_leave_dependency_errors_for_the_compiler() {
    use postman_flow::{compile_flow, CompileEnvironment, ResponseExport};
    let mut library = FlowLibrary::default();
    let flow = library.create("References").unwrap();
    library.append_step();
    library.edit_step(
        flow,
        "request-1",
        &StepEdit::Url(TextTemplate::literal("https://httpbingo.org/uuid")),
    );
    library.edit_step(
        flow,
        "request-1",
        &StepEdit::Exports(vec![ResponseExport::json("uuid", "$.uuid")]),
    );
    library.append_step();
    library.edit_step(
        flow,
        "request-2",
        &StepEdit::Url(
            postman_flow::parse_text_expression(r#"{"output":{"step":"request-1","name":"uuid"}}"#)
                .unwrap(),
        ),
    );
    let valid = library.active().unwrap().document().clone();
    assert!(compile_flow(&valid.flow, &valid.apis, &CompileEnvironment::default()).is_ok());
    library
        .reorder_step(flow, "request-2", StepDirection::Up)
        .unwrap();
    let draft = library.active().unwrap().document();
    assert!(compile_flow(&draft.flow, &draft.apis, &CompileEnvironment::default()).is_err());
    library
        .reorder_step(flow, "request-2", StepDirection::Down)
        .unwrap();
    library.remove_step(flow, "request-1").unwrap();
    let draft = library.active().unwrap().document();
    assert_eq!(draft.flow.steps[0], valid.flow.steps[1]);
    assert!(compile_flow(&draft.flow, &draft.apis, &CompileEnvironment::default()).is_err());
}

#[test]
fn documents_keep_identity_selection_and_unfinished_edits() {
    let mut library = FlowLibrary::default();
    assert!(library.create("   ").is_err());
    assert!(library.create(&"流".repeat(81)).is_err());
    assert!(library.create("a\nb").is_err());
    assert!(library.drafts().is_empty());
    let first = library.create("第一个流程").unwrap();
    library.append_step();
    edit(&mut library, StepEdit::Name("请求用户".into()));
    edit(
        &mut library,
        StepEdit::Url(TextTemplate::literal("https://")),
    );
    edit(&mut library, StepEdit::Method(HttpMethod::POST));
    library.append_step();
    edit(&mut library, StepEdit::Name(String::new()));
    library.select_step("request-1");
    let original = library.active().unwrap().document().clone();
    let revision = library.active().unwrap().revision();
    let second = library.create("第一个流程").unwrap();
    assert_ne!(first, second, "names do not identify documents");
    library.append_step();
    edit(
        &mut library,
        StepEdit::Url(TextTemplate::literal("https://httpbingo.org/get")),
    );
    library.select(first);
    assert_eq!(library.active().unwrap().document(), &original);
    assert_eq!(
        library.active().unwrap().selected().unwrap().id,
        "request-1"
    );
    assert_eq!(library.active().unwrap().revision(), revision);
    library.rename(String::new());
    assert_eq!(library.active().unwrap().title(), "Untitled flow");
    assert_eq!(library.active().unwrap().id(), first);
    library.select_step("request-2");
    assert_eq!(library.active().unwrap().selected().unwrap().name, "");
    let revision = library.active().unwrap().revision();
    edit(&mut library, StepEdit::Name(String::new()));
    assert_eq!(library.active().unwrap().revision(), revision);
    assert!(!library.select(FlowId(999)));
    assert_eq!(library.active().unwrap().id(), first);
}

#[test]
fn basic_edits_preserve_catalog_expressions_conditions_options_and_layout() {
    let mut document = parse_flow_yaml(include_str!(
        "../../../../../crates/postman-flow/examples/flows/httpbingo_catalog.http.yml"
    ))
    .unwrap();
    document.flow.steps[1].when = Some(ConditionExpr::eq(
        JsonTemplate::input("client"),
        JsonTemplate::literal("postman-flow-yaml"),
    ));
    if let HttpRequestSource::Inline(request) = &mut document.flow.steps[1].request {
        request.options = RequestOptionOverrides {
            timeout_ms: Some(3456),
            ..Default::default()
        };
    }
    let original = document.clone();
    let mut library = FlowLibrary::default();
    library.insert(document);
    edit(&mut library, StepEdit::Method(HttpMethod::DELETE)); // API reference is not an inline request.
    edit(
        &mut library,
        StepEdit::Url(TextTemplate::literal("https://httpbingo.org/get")),
    );
    assert_eq!(library.active().unwrap().document(), &original);
    library.select_step("echo");
    assert_eq!(library.active().unwrap().document(), &original);
    edit(&mut library, StepEdit::Name("Updated echo".into()));
    edit(&mut library, StepEdit::Method(HttpMethod::PATCH));
    let mut expected = original;
    expected.flow.steps[1].name = "Updated echo".into();
    if let HttpRequestSource::Inline(request) = &mut expected.flow.steps[1].request {
        request.method = HttpMethod::PATCH;
    }
    assert_eq!(library.active().unwrap().document(), &expected);
    assert_eq!(
        parse_flow_yaml(&write_flow_yaml(&expected).unwrap()).unwrap(),
        expected
    );
    library.append_step();
    let draft = library.active().unwrap();
    assert_eq!(draft.document().flow.steps[..2], expected.flow.steps);
    assert_eq!(draft.document().apis, expected.apis);
    assert_eq!(draft.document().editor, expected.editor);
    assert_eq!(draft.document().flow.inputs, expected.flow.inputs);
    assert_eq!(draft.document().flow.outputs, expected.flow.outputs);
}

fn edit(library: &mut FlowLibrary, edit: StepEdit) {
    let draft = library.active().unwrap();
    let flow = draft.id();
    let id = draft.selected().unwrap().id.clone();
    library.edit_step(flow, &id, &edit);
}

#[test]
fn request_field_edits_and_invalid_revisions_preserve_every_other_field() {
    let mut doc = parse_flow_yaml(include_str!(
        "../../../../../crates/postman-flow/examples/flows/httpbingo_catalog.http.yml"
    ))
    .unwrap();
    doc.flow.steps[1].when = Some(ConditionExpr::eq(
        JsonTemplate::input("client"),
        JsonTemplate::literal("test"),
    ));
    if let HttpRequestSource::Inline(r) = &mut doc.flow.steps[1].request {
        r.options.timeout_ms = Some(1379);
        r.auth = Some(postman_flow::AuthTemplate::HmacSha256 {
            secret: TextTemplate::parts([postman_flow::TemplatePart::Input("token".into())]),
            param: "sig".into(),
        });
    }
    let mut expected = doc.clone();
    let mut library = FlowLibrary::default();
    let id = library.insert(doc);
    let url = postman_flow::parse_text_expression(
        r#"{"coalesce":[{"input":"base_url"},{"calc":"inputs.fallback"}]}"#,
    )
    .unwrap();
    let headers = vec![(
        TextTemplate::literal("X-A"),
        TextTemplate::parts([postman_flow::TemplatePart::Input("token".into())]),
    )];
    let body = postman_flow::BodyTemplate::JsonValue(JsonTemplate::literal(serde_json::json!([
        true, 1, null
    ])));
    library.edit_step(id, "echo", &StepEdit::Url(url.clone()));
    library.edit_step(id, "echo", &StepEdit::Headers(headers.clone()));
    library.edit_step(id, "echo", &StepEdit::Body(body.clone()));
    if let HttpRequestSource::Inline(r) = &mut expected.flow.steps[1].request {
        r.url = url;
        r.headers = headers;
        r.body = body;
    }
    assert_eq!(library.active().unwrap().document(), &expected);
    let revision = library.active().unwrap().revision();
    library.edit_step(id, "echo", &StepEdit::Invalid);
    assert_eq!(library.active().unwrap().revision(), revision + 1);
    assert_eq!(library.active().unwrap().document(), &expected);
    library.edit_step(
        id,
        "seed",
        &StepEdit::Body(postman_flow::BodyTemplate::None),
    );
    assert_eq!(library.active().unwrap().revision(), revision + 1);
    let second = library.create("Other flow").unwrap();
    library.append_step();
    library.edit_step(id, "echo", &StepEdit::Name("Target identity".into()));
    assert_eq!(library.active().unwrap().id(), second);
    assert_eq!(
        library.active().unwrap().selected().unwrap().name,
        "Request 1"
    );
    library.select(id);
    expected.flow.steps[1].name = "Target identity".into();
    assert_eq!(library.active().unwrap().document(), &expected);
}

#[test]
fn definition_edits_preserve_requests_references_catalog_and_other_flows() {
    use postman_flow::{ResponseCheck, ResponseExport};
    let mut expected = parse_flow_yaml(include_str!(
        "../../../../../crates/postman-flow/examples/flows/httpbingo_catalog.http.yml"
    ))
    .unwrap();
    if let HttpRequestSource::Inline(r) = &mut expected.flow.steps[1].request {
        r.options.timeout_ms = Some(1379);
        r.auth = Some(postman_flow::AuthTemplate::HmacSha256 {
            secret: TextTemplate::literal("fixture"),
            param: "sig".into(),
        });
    }
    let mut library = FlowLibrary::default();
    let first = library.insert(expected.clone());
    let checks = vec![
        ResponseCheck::StatusEquals(201),
        ResponseCheck::JsonValueEquals {
            path: "$.json".into(),
            expected: JsonTemplate::literal(serde_json::json!({"key":[true,1,null]})),
        },
    ];
    let exports = vec![ResponseExport::json("renamed", "$.json.id").sensitive()];
    let condition = Some(ConditionExpr::eq(
        JsonTemplate::input("client"),
        JsonTemplate::literal("test"),
    ));
    for edit in [
        StepEdit::Checks(checks.clone()),
        StepEdit::Exports(exports.clone()),
        StepEdit::Condition(condition.clone()),
    ] {
        library.edit_step(first, "seed", &edit); // API is readonly, including delayed events.
    }
    assert_eq!(library.active().unwrap().document(), &expected);
    let second = library.create("Other flow").unwrap();
    library.append_step();
    let other = library.active().unwrap().document().clone();
    for edit in [
        StepEdit::Checks(checks.clone()),
        StepEdit::Exports(exports.clone()),
        StepEdit::Condition(condition.clone()),
    ] {
        library.edit_step(first, "echo", &edit);
    }
    assert_eq!(library.active().unwrap().id(), second);
    assert_eq!(library.active().unwrap().document(), &other);
    library.select(first);
    expected.flow.steps[1].checks = checks;
    expected.flow.steps[1].exports = exports;
    expected.flow.steps[1].when = condition;
    assert_eq!(library.active().unwrap().document(), &expected);
    assert_eq!(
        parse_flow_yaml(&write_flow_yaml(&expected).unwrap()).unwrap(),
        expected
    );
}

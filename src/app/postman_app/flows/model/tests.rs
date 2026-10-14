use super::*;
use postman_flow::{
    parse_flow_yaml, write_flow_yaml, ConditionExpr, JsonTemplate, RequestOptionOverrides,
};

#[test]
fn documents_keep_identity_selection_and_unfinished_edits() {
    let mut library = FlowLibrary::default();
    assert!(library.create("   ").is_err());
    assert!(library.create(&"流".repeat(81)).is_err());
    assert!(library.create("a\nb").is_err());
    assert!(library.drafts().is_empty());
    let first = library.create("第一个流程").unwrap();
    library.append_step();
    library.edit_step(StepEdit::Name("请求用户".into()));
    library.edit_step(StepEdit::Url("https://".into()));
    library.edit_step(StepEdit::Method(HttpMethod::POST));
    library.append_step();
    library.edit_step(StepEdit::Name(String::new()));
    library.select_step("request-1");
    let original = library.active().unwrap().document().clone();
    let revision = library.active().unwrap().revision();
    let second = library.create("第一个流程").unwrap();
    assert_ne!(first, second, "names do not identify documents");
    library.append_step();
    library.edit_step(StepEdit::Url("https://httpbingo.org/get".into()));
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
    library.edit_step(StepEdit::Name(String::new()));
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
    library.edit_step(StepEdit::Method(HttpMethod::DELETE)); // API reference is not an inline request.
    library.edit_step(StepEdit::Url("https://httpbingo.org/get".into()));
    assert_eq!(library.active().unwrap().document(), &original);
    library.select_step("echo");
    library.edit_step(StepEdit::Url("https://httpbingo.org/anything".into())); // Never flatten an expression.
    assert_eq!(library.active().unwrap().document(), &original);
    library.edit_step(StepEdit::Name("Updated echo".into()));
    library.edit_step(StepEdit::Method(HttpMethod::PATCH));
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

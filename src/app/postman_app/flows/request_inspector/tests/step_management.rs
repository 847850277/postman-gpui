use super::*;
use postman_flow::{FlowDocument, ResponseExport};

#[gpui_kit::test]
fn reorder_refreshes_cached_hints_and_preserves_invalid_sources_tabs_and_ime(
    cx: &mut TestAppContext,
) {
    use gpui_kit::{ElementInputHandler, InputHandler};
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    let mut flow = postman_flow::FlowDefinition::new("Draft retention");
    for id in ["producer", "consumer"] {
        let mut step = postman_flow::HttpStepDefinition::new(
            id,
            id,
            postman_flow::HttpRequestTemplate::new(
                crate::models::HttpMethod::GET,
                TextTemplate::literal("https://httpbingo.org/get"),
            ),
        );
        step.exports.push(ResponseExport::json("value", "$.json"));
        flow.steps.push(step);
    }
    cx.update(|w, cx| {
        flows.update(cx, |v, cx| {
            v.library.insert(FlowDocument::new(flow));
            v.sync_fields(w, cx);
            v.sync_picker(w, cx);
        })
    });
    click(cx, "nav-flows");
    settle(cx);
    let producer = inspector(&flows, cx);
    click(cx, "flow-node-1-consumer");
    settle(cx);
    let consumer = inspector(&flows, cx);
    edit_source(cx, "flow-body-source", "{");
    let before = request(&flows, cx);
    assert!(consumer.read_with(cx, |v, _| v.body_error.is_some()));
    click(cx, "flow-move-step-up");
    settle(cx);
    consumer.read_with(cx, |v, cx| {
        assert_eq!(v.body.read(cx).value(), "{");
        assert!(v.body_error.is_some());
        assert!(v.variables.is_empty());
    });
    assert_eq!(request(&flows, cx), before);
    producer.read_with(cx, |v, _| assert_eq!(v.variables, ["steps.consumer.value"]));
    click(cx, "flow-checks-tab");
    click(cx, "flow-move-step-down");
    settle(cx);
    consumer.read_with(cx, |v, _| {
        assert_eq!(v.tab, 1);
        assert_eq!(v.variables, ["steps.producer.value"]);
    });
    producer.read_with(cx, |v, _| assert!(v.variables.is_empty()));
    click(cx, "flow-request-tab");
    top(cx);
    click(cx, "flow-step-name");
    cx.update(|w, cx| {
        let name = consumer.read(cx).name.clone();
        let mut handler = ElementInputHandler::new(w.find("flow-step-name").bounds(), name);
        handler.replace_and_mark_text_in_range(None, "ni", Some(2..2), w, cx);
        assert!(handler.marked_text_range(w, cx).is_some());
    });
    click(cx, "flow-move-step-up");
    settle(cx);
    cx.update(|w, cx| {
        let mut handler = ElementInputHandler::new(
            w.find("flow-step-name").bounds(),
            consumer.read(cx).name.clone(),
        );
        assert!(handler.marked_text_range(w, cx).is_some());
        handler.replace_text_in_range(None, "你好", w, cx);
    });
    click(cx, "flow-node-1-producer");
    click(cx, "flow-remove-step");
    click(cx, "flow-remove-confirm");
    settle(cx);
    assert_eq!(inspector(&flows, cx), consumer);
    consumer.read_with(cx, |v, cx| {
        assert_eq!(v.body.read(cx).value(), "{");
        assert!(v.variables.is_empty());
    });
    // Keep the removed entity alive deliberately: its owner subscription must be gone.
    let before = revision(&flows, cx);
    producer.update(cx, |_, cx| cx.emit(StepEdit::Invalid));
    settle(cx);
    assert_eq!(revision(&flows, cx), before);
}

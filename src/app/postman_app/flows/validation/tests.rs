use super::super::tests::{add, click, create, press, replace, settle, setup};
use super::*;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    point, px, ClipboardItem, Entity, Modifiers, ScrollDelta, ScrollWheelEvent, SharedString,
    TestAppContext, TouchPhase, VisualTestContext,
};
use postman_flow::{
    parse_flow_yaml, FlowDefinition, FlowDocument, FlowInputSpec, HttpRequestTemplate,
    HttpStepDefinition, TextTemplate,
};

fn status(flows: &Entity<FlowsView>, cx: &mut VisualTestContext) -> &'static str {
    flows.read_with(cx, |v, cx| v.console.read(cx).check_label())
}
fn logs(cx: &mut VisualTestContext) -> String {
    click(cx, "console-copy");
    cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap())
}
fn install(flows: &Entity<FlowsView>, doc: FlowDocument, cx: &mut VisualTestContext) {
    cx.update(|w, cx| {
        flows.update(cx, |v, cx| {
            v.library.insert(doc);
            v.sync_fields(w, cx);
            v.sync_picker(w, cx);
            cx.notify();
        })
    });
    settle(cx);
}
fn fixture() -> FlowDocument {
    let mut doc = FlowDocument::new(FlowDefinition::new("Validation fixture"));
    doc.flow.steps.push(HttpStepDefinition::new(
        "request-1",
        "Request",
        HttpRequestTemplate::new(
            crate::models::HttpMethod::GET,
            TextTemplate::literal("http://127.0.0.1:1/no-network"),
        ),
    ));
    doc
}
fn source(cx: &mut VisualTestContext, id: &'static str, text: &str) {
    for _ in 0..5 {
        let panel = cx.debug_bounds("flow-inspector-scroll").unwrap();
        let field = cx.debug_bounds(id).unwrap();
        if field.top() >= panel.top() && field.bottom() <= panel.bottom() {
            break;
        }
        cx.simulate_event(ScrollWheelEvent {
            position: panel.origin + point(px(16.), px(4.)),
            delta: ScrollDelta::Pixels(point(px(0.), panel.center().y - field.center().y)),
            modifiers: Modifiers::none(),
            touch_phase: TouchPhase::Moved,
        });
        settle(cx);
    }
    let b = cx.debug_bounds(id).unwrap();
    cx.simulate_click(b.origin + point(px(12.), px(12.)), Modifiers::none());
    press(cx, "secondary-a");
    cx.write_to_clipboard(ClipboardItem::new_string(text.into()));
    press(
        cx,
        if text.is_empty() {
            "backspace"
        } else {
            "secondary-v"
        },
    );
    settle(cx);
}
fn locate_first(flows: &Entity<FlowsView>, cx: &mut VisualTestContext) {
    let id = flows.read_with(cx, |v, cx| v.console.read(cx).diagnostics()[0].0);
    cx.update(|w, cx| w.click(SharedString::from(format!("console-diagnostic-{id}")), cx));
    settle(cx);
}
fn preview(
    flows: &Entity<FlowsView>,
    cx: &mut VisualTestContext,
) -> Entity<super::super::yaml::YamlPreview> {
    flows.read_with(cx, |v, _| {
        v.previews[&v.library.active().unwrap().id()].clone()
    })
}

#[gpui_kit::test]
fn check_entry_points_validate_focused_edits_and_track_document_freshness(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    create(cx, "Check boundaries");
    assert_eq!(status(&flows, cx), "Not checked");
    click(cx, "flow-check");
    assert_eq!(status(&flows, cx), "Check failed");
    assert!(logs(cx).contains("EmptyFlow · flow.steps"));
    add(cx);
    replace(cx, "flow-step-url", "http://127.0.0.1:1/no-network");
    click(cx, "flow-check");
    assert_eq!(status(&flows, cx), "Check passed");
    click(cx, "flow-checks-tab");
    click(cx, "flow-request-tab");
    click(cx, "nav-http");
    click(cx, "nav-flows");
    assert_eq!(status(&flows, cx), "Check passed");
    replace(cx, "flow-step-name", "");
    assert_eq!(status(&flows, cx), "Check stale");
    replace(cx, "console-command", " CHECK ");
    press(cx, "enter");
    settle(cx);
    assert_eq!(status(&flows, cx), "Check failed");
    assert!(logs(cx).contains("InvalidName · flow.steps[0].name (step request-1)"));
    create(cx, "Second");
    assert_eq!(status(&flows, cx), "Not checked");
    click(cx, "flow-row-1");
    assert_eq!(status(&flows, cx), "Check failed");
    app.read_with(cx, |a, cx| {
        assert_eq!(a.view_model.read(cx).in_flight_count(), 0)
    });
}

#[gpui_kit::test]
fn hidden_drafts_block_check_and_yaml_and_diagnostics_restore_the_editing_surface(
    cx: &mut TestAppContext,
) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    install(&flows, fixture(), cx);
    click(cx, "flow-yaml-tab");
    let yaml = preview(&flows, cx);
    let applied = yaml.read_with(cx, |v, _| v.source.clone());
    click(cx, "flow-canvas-tab");
    settle(cx);
    source(cx, "flow-body-source", "{broken");
    flows.read_with(cx, |v, cx| {
        assert!(
            !v.draft_issues(v.library.active().unwrap().id(), cx)
                .is_empty(),
            "body edit must reach retained textarea"
        )
    });
    add(cx);
    replace(cx, "flow-step-url", "http://127.0.0.1:1/also-offline");
    click(cx, "flow-check");
    assert_eq!(status(&flows, cx), "Check failed");
    assert!(logs(cx).contains("UnappliedDraft · step request-1 · request.body"));
    click(cx, "flow-yaml-tab");
    yaml.read_with(cx, |v, _| {
        assert!(!v.current);
        assert_eq!(v.source, applied);
        assert!(v.status.contains("Last applied document"));
    });
    cx.write_to_clipboard(ClipboardItem::new_string("sentinel".into()));
    click(cx, "flow-yaml-copy");
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().unwrap(),
        "sentinel"
    );
    click(cx, "console-maximize");
    locate_first(&flows, cx);
    assert!(cx.debug_bounds("flow-inspector").is_some());
    assert!(cx.debug_bounds("flow-yaml").is_none());
    // The diagnostic restores and reveals the original textarea; no blur is needed to apply it.
    source(cx, "flow-body-source", "");
    assert_eq!(status(&flows, cx), "Check stale");
    click(cx, "flow-check");
    assert_eq!(status(&flows, cx), "Check passed");
    click(cx, "flow-yaml-tab");
    assert!(yaml.read_with(cx, |v, _| v.current));
}

#[gpui_kit::test]
fn yaml_is_readonly_roundtrips_full_documents_and_isolates_flow_editor_state(
    cx: &mut TestAppContext,
) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    let mut doc = parse_flow_yaml(include_str!(
        "../../../../../crates/postman-flow/examples/flows/httpbingo_loops.http.yml"
    ))
    .unwrap();
    doc.flow.inputs.push(FlowInputSpec::with_default(
        "explicit_secret",
        serde_json::json!("document-value"),
    ));
    doc.flow.inputs.last_mut().unwrap().sensitive = true;
    let catalog = parse_flow_yaml(include_str!(
        "../../../../../crates/postman-flow/examples/flows/httpbingo_catalog.http.yml"
    ))
    .unwrap();
    doc.apis = catalog.apis;
    doc.flow.inputs.extend(catalog.flow.inputs);
    doc.flow.steps.extend(catalog.flow.steps);
    doc.flow.outputs.extend(catalog.flow.outputs);
    let postman_flow::HttpRequestSource::Inline(request) =
        &mut doc.flow.steps.last_mut().unwrap().request
    else {
        panic!("inline catalog example")
    };
    request.auth = Some(postman_flow::AuthTemplate::HmacSha256 {
        secret: TextTemplate::literal("explicit-document-secret"),
        param: "signature".into(),
    });
    request.options.timeout_ms = Some(1234);
    request.options.max_redirect_hops = Some(2);

    doc.editor.nodes.insert(
        "layout".into(),
        postman_flow::NodePosition { x: 12., y: -5. },
    );
    install(&flows, doc.clone(), cx);
    let rev = flows.read_with(cx, |v, _| v.library.active().unwrap().revision());
    click(cx, "flow-yaml-tab");
    let yaml = preview(&flows, cx);
    click(cx, "flow-yaml-copy");
    let copied = cx.read_from_clipboard().unwrap().text().unwrap();
    assert_eq!(parse_flow_yaml(&copied).unwrap(), doc);
    let editor = yaml.read_with(cx, |v, _| v.editor.clone());
    cx.update(|w, cx| editor.focus_handle(cx).focus(w, cx));
    press(cx, "secondary-a");
    cx.simulate_input("must not replace YAML");
    cx.write_to_clipboard(ClipboardItem::new_string("must not paste into YAML".into()));
    press(cx, "secondary-v");
    press(cx, "backspace");
    assert_eq!(editor.read_with(cx, |v, _| v.value().to_string()), copied);
    let selection = editor.read_with(cx, |v, _| v.selected_range());
    click(cx, "flow-canvas-tab");
    click(cx, "flow-yaml-tab");
    assert_eq!(editor.read_with(cx, |v, _| v.selected_range()), selection);
    create(cx, "Second flow");
    click(cx, "flow-yaml-copy");
    assert!(cx
        .read_from_clipboard()
        .unwrap()
        .text()
        .unwrap()
        .contains("Second flow"));
    click(cx, "flow-row-1");
    assert_eq!(preview(&flows, cx), yaml);
    assert_eq!(editor.read_with(cx, |v, _| v.selected_range()), selection);
    flows.read_with(cx, |v, _| {
        assert_eq!(v.library.active().unwrap().document(), &doc);
        assert_eq!(v.library.active().unwrap().revision(), rev);
    });
    add(cx);
    assert!(cx.debug_bounds("flow-yaml").is_none());
    assert!(cx.debug_bounds("flow-step-name").is_some());
}

#[gpui_kit::test]
fn stale_diagnostics_do_not_retarget_after_structural_edits_and_codec_errors_block_copy(
    cx: &mut TestAppContext,
) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    let mut doc = fixture();
    doc.flow.steps[0].name.clear();
    doc.editor.nodes.insert(
        "bad".into(),
        postman_flow::NodePosition {
            x: f64::INFINITY,
            y: 0.,
        },
    );
    install(&flows, doc, cx);
    click(cx, "flow-check");
    add(cx);
    click(cx, "flow-move-step-up");
    let selected = flows.read_with(cx, |v, _| {
        v.library.active().unwrap().selected().unwrap().id.clone()
    });
    locate_first(&flows, cx);
    flows.read_with(cx, |v, _| {
        assert_eq!(v.library.active().unwrap().selected().unwrap().id, selected)
    });
    click(cx, "flow-node-1-request-1");
    click(cx, "flow-remove-step");
    click(cx, "flow-remove-confirm");
    settle(cx);
    locate_first(&flows, cx);
    flows.read_with(cx, |v, _| {
        assert_eq!(v.library.active().unwrap().selected().unwrap().id, selected)
    });
    click(cx, "flow-yaml-tab");
    preview(&flows, cx).read_with(cx, |v, _| {
        assert!(!v.current);
        assert!(v.status.contains("node coordinates must be finite"));
    });
    cx.write_to_clipboard(ClipboardItem::new_string("unchanged".into()));
    click(cx, "flow-yaml-copy");
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().unwrap(),
        "unchanged"
    );
}

#[gpui_kit::test]
fn catalog_diagnostics_reveal_the_readonly_owner_with_keyboard_navigation(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    let mut doc = fixture();
    doc.apis.insert(
        "broken",
        postman_flow::ApiDefinition::new(HttpRequestTemplate::new(
            crate::models::HttpMethod::GET,
            TextTemplate::parts([postman_flow::TemplatePart::Input("undeclared".into())]),
        )),
    );
    doc.flow.steps.push(HttpStepDefinition::new(
        "catalog",
        "Catalog request",
        postman_flow::ApiCall::new("broken"),
    ));
    install(&flows, doc, cx);
    click(cx, "flow-check");
    assert!(logs(cx).contains("api broken"));
    click(cx, "flow-yaml-tab");
    click(cx, "console-command");
    press(cx, "shift-tab");
    press(cx, "enter");
    settle(cx);
    assert!(cx.debug_bounds("flow-yaml").is_none());
    assert!(cx.debug_bounds("flow-step-readonly").is_some());
    flows.read_with(cx, |v, _| {
        assert_eq!(
            v.library.active().unwrap().selected().unwrap().id,
            "catalog"
        )
    });
    let mut nested = parse_flow_yaml(include_str!(
        "../../../../../crates/postman-flow/examples/flows/httpbingo_loops.http.yml"
    ))
    .unwrap();
    let owner = nested
        .flow
        .steps
        .iter_mut()
        .find_map(|step| {
            let children = match &mut step.request {
                postman_flow::HttpRequestSource::ForEach(value) => &mut value.steps,
                postman_flow::HttpRequestSource::RepeatUntil(value) => &mut value.steps,
                _ => return None,
            };
            children[0].name.clear();
            Some(step.id.clone())
        })
        .unwrap();
    nested.flow.steps.insert(0, fixture().flow.steps.remove(0));
    install(&flows, nested, cx);
    click(cx, "flow-check");
    click(cx, "flow-yaml-tab");
    locate_first(&flows, cx);
    assert!(cx.debug_bounds("flow-step-readonly").is_some());
    flows.read_with(cx, |v, _| {
        assert_eq!(v.library.active().unwrap().selected().unwrap().id, owner)
    });
}

#[gpui_kit::test]
fn check_keeps_a_complete_report_beyond_the_normal_log_history_cap(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    let mut doc = fixture();
    let template = doc.flow.steps[0].clone();
    doc.flow.steps = (0..520)
        .map(|index| {
            let mut step = template.clone();
            step.id = format!("step-{index}");
            step.name.clear();
            step
        })
        .collect();
    install(&flows, doc, cx);
    click(cx, "flow-check");
    let report = logs(cx);
    assert_eq!(report.matches("InvalidName · ").count(), 520);
    assert!(report.contains("flow.steps[0].name"));
    assert!(report.contains("flow.steps[519].name"));
    assert_eq!(status(&flows, cx), "Check failed");
}

#[gpui_kit::test]
fn yaml_reports_native_codec_depth_limits_even_for_compiler_valid_documents(
    cx: &mut TestAppContext,
) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    let mut doc = fixture();
    let mut value = serde_json::Value::Null;
    for _ in 0..100 {
        value = serde_json::json!([value]);
    }
    doc.flow
        .inputs
        .push(FlowInputSpec::with_default("deep", value));
    install(&flows, doc, cx);
    click(cx, "flow-check");
    assert_eq!(status(&flows, cx), "Check passed");
    click(cx, "flow-yaml-tab");
    preview(&flows, cx).read_with(cx, |v, _| {
        assert!(!v.current);
        assert!(v.source.is_none());
        assert!(v.status.contains("LimitExceeded"));
    });
}

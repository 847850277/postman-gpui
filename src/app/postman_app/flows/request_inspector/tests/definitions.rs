use super::*;
use postman_flow::{
    compile_flow, parse_flow_yaml, write_flow_yaml, CompileEnvironment, FlowDocument,
    ResponseCheck, ResponseExport,
};

fn install(flows: &Entity<FlowsView>, document: FlowDocument, cx: &mut VisualTestContext) {
    cx.update(|w, cx| {
        flows.update(cx, |v, cx| {
            v.library.insert(document);
            v.sync_fields(w, cx);
            v.sync_picker(w, cx);
            cx.notify();
        })
    });
    settle(cx);
}
fn document(flows: &Entity<FlowsView>, cx: &mut VisualTestContext) -> FlowDocument {
    flows.read_with(cx, |v, _| v.library.active().unwrap().document().clone())
}
fn fixture() -> FlowDocument {
    let mut doc = parse_flow_yaml(include_str!(
        "../../../../../../crates/postman-flow/examples/flows/httpbingo_loops.http.yml"
    ))
    .unwrap();
    doc.flow.steps.insert(
        0,
        HttpStepDefinition::new(
            "seed",
            "Seed",
            postman_flow::HttpRequestTemplate::new(
                crate::models::HttpMethod::GET,
                TextTemplate::literal("https://httpbingo.org/get"),
            ),
        ),
    );
    let first = &mut doc.flow.steps[0];
    first.checks = vec![
        ResponseCheck::StatusEquals(200),
        ResponseCheck::HeaderExists {
            name: "Content-Type".into(),
        },
        ResponseCheck::StatusEquals(201),
        ResponseCheck::BodyContains {
            expected: TextTemplate::literal("{{literal}}"),
        },
        ResponseCheck::HeaderContains {
            name: "X-Test".into(),
            expected: TextTemplate::literal("value"),
        },
        ResponseCheck::JsonValueEquals {
            path: "$".into(),
            expected: postman_flow::JsonTemplate::literal(serde_json::json!([true, 2, null])),
        },
        ResponseCheck::RedirectsEquals(0),
        ResponseCheck::ErrorEquals(postman_flow::ExpectedError::Timeout),
    ];
    first.exports = vec![
        ResponseExport::json("first", "$.json.id"),
        ResponseExport::json("second", "$").sensitive(),
    ];
    doc
}
fn row_click(cx: &mut VisualTestContext, name: &'static str, id: u64) {
    for _ in 0..4 {
        let panel = cx.debug_bounds("flow-inspector-scroll").unwrap();
        let bounds = cx.update(|w, _| w.find((name, id)).bounds());
        if bounds.top() >= panel.top() && bounds.bottom() <= panel.bottom() {
            break;
        }
        scroll(
            cx,
            panel.origin + point(px(5.), px(5.)),
            panel.center().y - bounds.center().y,
        );
        settle(cx);
    }
    cx.update(|w, cx| w.click((name, id), cx));
    settle(cx);
}
fn row_replace(cx: &mut VisualTestContext, name: &'static str, id: u64, value: &str) {
    row_click(cx, name, id);
    press(cx, "secondary-a");
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(value.into())));
    press(
        cx,
        if value.is_empty() {
            "backspace"
        } else {
            "secondary-v"
        },
    );
    settle(cx);
}
fn row_value(cx: &mut VisualTestContext, name: &'static str, id: u64) -> String {
    cx.update(|w, _| w.find((name, id)).value().unwrap().to_string())
}

#[gpui_kit::test]
fn checks_keep_mixed_order_and_blank_invalid_and_independent_rows(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    settle(cx);
    let original = fixture();
    install(&flows, original.clone(), cx);
    click(cx, "flow-checks-tab");
    let before = revision(&flows, cx);
    for invalid in ["99", "600", "200.5", "2e2", "-100", "abc"] {
        row_replace(cx, "flow-check-status", 0, invalid);
        assert_eq!(
            document(&flows, cx).flow.steps[0].checks,
            original.flow.steps[0].checks
        );
    }
    assert!(revision(&flows, cx) > before);
    row_replace(cx, "flow-check-status", 0, "100");
    assert_eq!(
        document(&flows, cx).flow.steps[0].checks[0],
        ResponseCheck::StatusEquals(100)
    );
    row_replace(cx, "flow-check-status", 0, "599");
    row_replace(cx, "flow-check-status", 2, "{");
    row_click(cx, "flow-remove-check", 0);
    assert_eq!(row_value(cx, "flow-check-status", 2), "{");
    press(cx, "secondary-z");
    assert_eq!(
        row_value(cx, "flow-check-status", 2),
        "201",
        "removal preserves next row undo and moves focus to it"
    );
    row_replace(cx, "flow-check-status", 2, "");
    let mut expected = original;
    expected.flow.steps[0]
        .checks
        .retain(|c| !matches!(c, ResponseCheck::StatusEquals(_)));
    assert_eq!(document(&flows, cx), expected);
    assert_eq!(
        parse_flow_yaml(&write_flow_yaml(&expected).unwrap()).unwrap(),
        expected
    );
}

#[gpui_kit::test]
fn exports_retain_invalid_rows_sensitivity_and_refresh_cached_hints(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    settle(cx);
    create(cx, "Exports");
    add(cx);
    replace(cx, "flow-step-url", "https://httpbingo.org/get");
    let first = inspector(&flows, cx);
    add(cx); // Create and retain the later inspector before editing the earlier exports.
    replace(cx, "flow-step-url", "https://httpbingo.org/get");
    let later = inspector(&flows, cx);
    edit_source(cx, "flow-body-source", "{later-draft");
    click(cx, "flow-node-1-request-1");
    click(cx, "flow-exports-tab");
    click(cx, "flow-add-export");
    let a = first.read_with(cx, |v, _| v.exports[0].id);
    row_replace(cx, "flow-export-name", a, "id");
    assert!(document(&flows, cx).flow.steps[0].exports.is_empty());
    row_replace(cx, "flow-export-path", a, "$.items[0].id");
    row_click(cx, "flow-export-sensitive", a);
    assert!(document(&flows, cx).flow.steps[0].exports[0].sensitive);
    assert!(later.read_with(cx, |v, _| v
        .variables
        .contains(&"steps.request-1.id".into())));
    assert!(first.read_with(cx, |v, _| v.variables.is_empty()));
    click(cx, "flow-add-export");
    let b = first.read_with(cx, |v, _| v.exports[1].id);
    row_replace(cx, "flow-export-name", b, "second");
    row_replace(cx, "flow-export-path", b, "$");
    row_replace(cx, "flow-export-name", a, "second");
    assert_eq!(document(&flows, cx).flow.steps[0].exports[0].name, "id");
    row_replace(cx, "flow-export-path", b, "$.changed");
    assert_eq!(
        document(&flows, cx).flow.steps[0].exports[1].json_path,
        "$.changed",
        "an invalid rename must not block a valid row"
    );
    row_replace(cx, "flow-export-path", b, "$");
    row_replace(cx, "flow-export-name", a, "renamed");
    assert!(later.read_with(cx, |v, _| v
        .variables
        .contains(&"steps.request-1.renamed".into())
        && !v.variables.contains(&"steps.request-1.id".into())));
    row_replace(cx, "flow-export-path", b, "$.items[*]");
    assert_eq!(document(&flows, cx).flow.steps[0].exports[1].json_path, "$");
    row_replace(cx, "flow-export-name", b, "draft");
    row_click(cx, "flow-remove-export", a);
    assert_eq!(row_value(cx, "flow-export-name", b), "draft");
    press(cx, "secondary-z");
    assert_eq!(row_value(cx, "flow-export-name", b), "second");
    press(cx, "secondary-shift-z");
    assert_eq!(row_value(cx, "flow-export-name", b), "draft");
    assert_eq!(row_value(cx, "flow-export-path", b), "$.items[*]");
    row_replace(cx, "flow-export-path", b, "$");
    let doc = document(&flows, cx);
    assert_eq!(
        doc.flow.steps[0].exports,
        vec![ResponseExport::json("draft", "$")]
    );
    assert!(compile_flow(&doc.flow, &doc.apis, &CompileEnvironment::default()).is_ok());
    assert_eq!(
        parse_flow_yaml(&write_flow_yaml(&doc).unwrap()).unwrap(),
        doc
    );
    assert_eq!(
        later.read_with(cx, |v, cx| v.body.read(cx).value()),
        "{later-draft"
    );
    click(cx, "flow-node-1-request-2");
    edit_source(cx, "flow-body-source", "");
    press(cx, "secondary-z");
    assert_eq!(
        later.read_with(cx, |v, cx| v.body.read(cx).value()),
        "{later-draft",
        "hint refresh must not reset later editor undo"
    );
}

#[gpui_kit::test]
fn condition_drafts_tabs_flows_keyboard_and_ime_are_isolated(cx: &mut TestAppContext) {
    use gpui::EntityInputHandler;
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    settle(cx);
    create(cx, "First");
    add(cx);
    let owner = inspector(&flows, cx);
    // Reach the disclosure through real pointer scrolling before opening the source.
    let panel = cx.debug_bounds("flow-inspector-scroll").unwrap();
    scroll(cx, panel.origin + point(px(5.), px(5.)), px(-5000.));
    settle(cx);
    click(cx, "flow-condition-toggle");
    let valid = r#"{"eq":[{"output":{"step":"query-status","name":"status"}},{"literal":2}]}"#;
    edit_source(cx, "flow-condition-source", valid);
    assert_eq!(
        document(&flows, cx).flow.steps[0].when,
        parse_condition_expression(valid).unwrap()
    );
    assert!(
        compile_flow(
            &document(&flows, cx).flow,
            &postman_flow::ApiCatalog::default(),
            &CompileEnvironment::default()
        )
        .is_err(),
        "local syntax checks do not claim reference resolution"
    );
    let before = revision(&flows, cx);
    edit_source(cx, "flow-condition-source", "{");
    assert!(revision(&flows, cx) > before);
    click(cx, "flow-checks-tab");
    press(cx, "right");
    assert_eq!(owner.read_with(cx, |v, _| v.tab), 2);
    press(cx, "home");
    assert_eq!(owner.read_with(cx, |v, _| v.tab), 0);
    add(cx);
    create(cx, "Second");
    add(cx);
    click(cx, "flow-row-1");
    click(cx, "flow-node-1-request-1");
    assert_eq!(
        owner.read_with(cx, |v, cx| v.condition.read(cx).value()),
        "{"
    );
    edit_source(cx, "flow-condition-source", "");
    assert_eq!(document(&flows, cx).flow.steps[0].when, None);
    let input = owner.read_with(cx, |v, cx| v.condition.read(cx).input_state());
    cx.update(|w, cx| {
        input.update(cx, |s, cx| {
            s.replace_and_mark_text_in_range(None, "ni", Some(2..2), w, cx)
        })
    });
    cx.update(|_, cx| crate::ui::theme::apply(gpui_kit::component::ThemeMode::Dark, cx));
    settle(cx);
    cx.update(|w, cx| {
        input.update(cx, |s, cx| {
            assert!(s.marked_text_range(w, cx).is_some());
            s.replace_text_in_range(None, "你好", w, cx);
        })
    });
    assert_eq!(
        owner.read_with(cx, |v, cx| v.condition.read(cx).value()),
        "你好"
    );
    press(cx, "secondary-a");
    press(cx, "secondary-c");
    cx.update(|_, cx| assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "你好"));
    press(cx, "secondary-z");
    press(cx, "secondary-shift-z");
    press(cx, "tab");
    cx.update(|w, cx| assert!(!input.focus_handle(cx).is_focused(w)));
    press(cx, "shift-tab");
    cx.update(|w, cx| assert!(input.focus_handle(cx).is_focused(w)));
    press(cx, "enter");
    press(cx, "secondary-enter");
    press(cx, "secondary-s");
    assert!(owner.read_with(cx, |v, cx| v.condition.read(cx).value().contains('\n')));
    app.read_with(cx, |a, cx| {
        assert_eq!(a.view_model.read(cx).in_flight_count(), 0)
    });
    assert_eq!(document(&flows, cx).flow.steps[0].when, None);
}

#[gpui_kit::test]
fn readonly_api_and_loops_keep_checks_exports_and_conditions(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    settle(cx);
    let mut api = fixture();
    api.flow.steps[0].request = postman_flow::ApiCall::new("unresolved-api").into();
    api.flow.steps[0].when =
        parse_condition_expression(r#"{"eq":[{"literal":1},{"literal":1}]}"#).unwrap();
    install(&flows, api.clone(), cx);
    let before = revision(&flows, cx);
    click(cx, "flow-checks-tab");
    cx.update(|w, _| assert!(w.try_find(("flow-check-status", 0u64)).is_none()));
    click(cx, "flow-exports-tab");
    let id = inspector(&flows, cx).read_with(cx, |v, _| v.exports[0].id);
    row_replace(cx, "flow-export-name", id, "must not change");
    row_click(cx, "flow-export-sensitive", id);
    cx.update(|w, _| assert!(w.try_find("flow-add-export").is_none()));
    assert_eq!(revision(&flows, cx), before);
    assert_eq!(document(&flows, cx), api);
    let loop_doc = parse_flow_yaml(include_str!(
        "../../../../../../crates/postman-flow/examples/flows/httpbingo_loops.http.yml"
    ))
    .unwrap();
    install(&flows, loop_doc.clone(), cx);
    click(cx, "flow-checks-tab");
    click(cx, "flow-exports-tab");
    click(cx, "flow-request-tab");
    assert!(cx.debug_bounds("flow-condition-source").is_none());
    assert_eq!(document(&flows, cx), loop_doc);
}

#[gpui_kit::test]
fn definition_panes_scroll_at_all_sizes_without_moving_console_or_tabs(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    settle(cx);
    let mut doc = fixture();
    doc.flow.steps[0].exports = (0..12)
        .map(|i| ResponseExport::json(format!("export_{i}"), format!("$.{}", "long".repeat(80))))
        .collect();
    install(&flows, doc, cx);
    let owner = inspector(&flows, cx);
    click(cx, "flow-exports-tab");
    let first = owner.read_with(cx, |v, _| v.exports[0].id);
    row_replace(
        cx,
        "flow-export-path",
        first,
        &format!("$.{}[*]", "long".repeat(80)),
    );
    let before = revision(&flows, cx);
    let handle = cx.update(|w, _| w.window_handle());
    for theme in [
        gpui_kit::component::ThemeMode::Light,
        gpui_kit::component::ThemeMode::Dark,
    ] {
        cx.update(|_, cx| crate::ui::theme::apply(theme, cx));
        for scale in [1., 2.] {
            cx.simulate_window_scale_factor_change(handle, scale);
            for (width, height) in [
                (1440., 960.),
                (1920., 1080.),
                (1024., 768.),
                (768., 720.),
                (1024., 600.),
            ] {
                cx.simulate_window_resize(handle, gpui::size(px(width), px(height)));
                settle(cx);
                if cx.debug_bounds("flow-inspector").is_none() {
                    click(cx, "flow-toggle-details");
                    settle(cx);
                }
                for (tab, action) in [
                    ("flow-checks-tab", "flow-add-check"),
                    ("flow-exports-tab", "flow-add-export"),
                ] {
                    click(cx, tab);
                    top(cx);
                    let panel = cx.debug_bounds("flow-inspector-scroll").unwrap();
                    let console = cx.debug_bounds("flow-results").unwrap();
                    let tabs = cx.update(|w, _| w.find("flow-exports-tab").bounds());
                    assert!(panel.bottom() <= console.top() + px(1.));
                    if tab == "flow-exports-tab" {
                        for name in [
                            "flow-export-name",
                            "flow-export-path",
                            "flow-remove-export",
                            "flow-export-sensitive",
                        ] {
                            let b = cx.update(|w, _| w.find((name, first)).bounds());
                            assert!(
                                b.left() >= panel.left() && b.right() <= panel.right() + px(1.),
                                "{name} at {width}x{height}: {b:?}"
                            );
                        }
                    }
                    scroll(cx, panel.origin + point(px(5.), px(5.)), px(-10000.));
                    settle(cx);
                    // At short heights the footer can fill the viewport. Scroll back to
                    // the action itself to verify it is reachable, rather than just to the end.
                    let bounds = cx.update(|w, _| w.find(action).bounds());
                    scroll(
                        cx,
                        panel.origin + point(px(5.), px(5.)),
                        panel.center().y - bounds.center().y,
                    );
                    settle(cx);
                    cx.update(|w, _| {
                        assert!(
                            w.find(action).visible(),
                            "{action} reachable at {width}x{height}"
                        )
                    });
                    assert_eq!(cx.debug_bounds("flow-results").unwrap(), console);
                    assert_eq!(cx.update(|w, _| w.find("flow-exports-tab").bounds()), tabs);
                    assert_eq!(revision(&flows, cx), before);
                }
            }
        }
    }
    click(cx, "flow-exports-tab");
    let panel = cx.debug_bounds("flow-inspector-scroll").unwrap();
    let action_bounds = cx.update(|w, _| w.find("flow-add-export").bounds());
    scroll(
        cx,
        panel.origin + point(px(5.), px(5.)),
        panel.center().y - action_bounds.center().y,
    );
    settle(cx);
    click(cx, "flow-add-export");
    let last = owner.read_with(cx, |v, _| v.exports.last().unwrap().id);
    cx.update(|w, _| {
        assert!(
            w.find(("flow-export-name", last)).visible(),
            "new row scrolls into view"
        )
    });
    // A new row has its own input, even after long-list scrolling and every resize mode.
    row_replace(cx, "flow-export-name", last, "unfinished");
    click(cx, "flow-request-tab");
    click(cx, "flow-exports-tab");
    assert_eq!(row_value(cx, "flow-export-name", last), "unfinished");
}

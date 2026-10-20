use super::super::{
    tests::{add, click, create, press, replace, settle, setup},
    FlowsView,
};
use super::*;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    point, px, ClipboardItem, Modifiers, ScrollDelta, ScrollWheelEvent, SharedString,
    TestAppContext, TouchPhase, VisualTestContext,
};
use postman_flow::{parse_flow_yaml, write_flow_yaml, FlowDocument};
use serde_json::json;

fn editor(flows: &Entity<FlowsView>, cx: &mut VisualTestContext) -> Entity<InputsInspector> {
    flows.read_with(cx, |v, _| {
        v.inputs[&v.library.active().unwrap().id()].clone()
    })
}
fn document(flows: &Entity<FlowsView>, cx: &mut VisualTestContext) -> FlowDocument {
    flows.read_with(cx, |v, _| v.library.active().unwrap().document().clone())
}
fn revision(flows: &Entity<FlowsView>, cx: &mut VisualTestContext) -> u64 {
    flows.read_with(cx, |v, _| v.library.active().unwrap().revision())
}
fn install(flows: &Entity<FlowsView>, values: Vec<FlowInputSpec>, cx: &mut VisualTestContext) {
    let mut doc = parse_flow_yaml(include_str!(
        "../../../../../crates/postman-flow/examples/flows/httpbingo_loops.http.yml"
    ))
    .unwrap();
    doc.flow.inputs = values;
    cx.update(|w, cx| {
        flows.update(cx, |v, cx| {
            v.library.insert(doc);
            v.sync_fields(w, cx);
            v.sync_picker(w, cx);
            cx.notify();
        })
    });
    settle(cx);
    click(cx, "flow-inputs");
    settle(cx);
}
fn reveal(
    cx: &mut VisualTestContext,
    id: gpui_kit::ElementId,
) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    for _ in 0..8 {
        let bounds = cx.update(|w, _| w.find(id.clone()).bounds());
        let panel = cx.debug_bounds("flow-inputs-scroll").unwrap();
        if bounds.top() >= panel.top() && bounds.bottom() <= panel.bottom() {
            return bounds;
        }
        cx.simulate_event(ScrollWheelEvent {
            position: panel.origin + point(px(16.), px(4.)),
            delta: ScrollDelta::Pixels(point(px(0.), panel.center().y - bounds.center().y)),
            modifiers: Modifiers::none(),
            touch_phase: TouchPhase::Moved,
        });
        settle(cx);
    }
    cx.update(|w, _| w.find(id.clone()).bounds())
}
fn row_click(cx: &mut VisualTestContext, kind: &'static str, id: u64) {
    reveal(cx, (kind, id).into());
    cx.update(|w, cx| w.click((kind, id), cx));
    settle(cx);
}
fn named_click(cx: &mut VisualTestContext, id: &str) {
    let id: SharedString = id.to_owned().into();
    reveal(cx, id.clone().into());
    cx.update(|w, cx| w.click(id, cx));
    settle(cx);
}
fn edit(cx: &mut VisualTestContext, id: gpui_kit::ElementId, source: &str) {
    let b = reveal(cx, id);
    cx.simulate_click(b.origin + point(px(12.), px(12.)), Modifiers::none());
    press(cx, "secondary-a");
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(source.into())));
    press(
        cx,
        if source.is_empty() {
            "backspace"
        } else {
            "secondary-v"
        },
    );
    settle(cx);
}
fn name(cx: &mut VisualTestContext, id: u64, value: &str) {
    edit(cx, ("flow-input-name", id).into(), value);
}
fn source(cx: &mut VisualTestContext, kind: &str, id: u64, value: &str) {
    edit(
        cx,
        SharedString::from(format!("flow-input-{kind}-{id}")).into(),
        value,
    );
}

/// Exercise the engine's binding boundary without polling an execution stream or sending I/O.
fn native_binding_accepts(specs: Vec<FlowInputSpec>, inputs: FlowInputs) {
    use postman_flow::{
        compile_flow, execute_flow, ApiCatalog, CompileEnvironment, FlowDefinition,
        FlowSessionEnvironment, HttpRequestTemplate, HttpStepDefinition, JsonTemplate,
    };
    use postman_http::{
        request::{HttpMethod, Request, RequestOptions},
        HttpError, HttpResponse, HttpTransport,
    };
    struct NoIo;
    impl HttpTransport for NoIo {
        async fn execute(&self, _: Request, _: RequestOptions) -> Result<HttpResponse, HttpError> {
            panic!("parameter preparation must not perform I/O")
        }
    }
    let mut flow = FlowDefinition::new("Prepared inputs");
    flow.inputs = specs;
    flow.steps.push(HttpStepDefinition::new(
        "echo",
        "Echo",
        HttpRequestTemplate::new(
            HttpMethod::POST,
            postman_flow::TextTemplate::literal("https://httpbingo.org/post"),
        )
        .json_value_body(JsonTemplate::input("arg")),
    ));
    let plan = compile_flow(&flow, &ApiCatalog::new(), &CompileEnvironment::default()).unwrap();
    assert!(execute_flow(plan, NoIo, FlowSessionEnvironment::new(inputs)).is_ok());
}

#[gpui_kit::test]
fn definitions_preserve_null_types_drafts_identity_and_unrelated_fields(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    settle(cx);
    install(
        &flows,
        vec![
            FlowInputSpec::required("required"),
            FlowInputSpec::with_default("nullable", Value::Null),
        ],
        cx,
    );
    let original = document(&flows, cx);
    let e = editor(&flows, cx);
    click(cx, "flow-inputs-definitions-tab");
    row_click(cx, "flow-input-has-default", 0);
    assert_eq!(document(&flows, cx).flow.inputs[0].default, Some(json!("")));
    named_click(cx, "flow-input-default-0-json");
    source(cx, "default", 0, "{\"a\":[true,2,null]}");
    assert_eq!(
        document(&flows, cx).flow.inputs[0].default,
        Some(json!({"a":[true,2,null]}))
    );
    let before = revision(&flows, cx);
    source(cx, "default", 0, "{secret-invalid");
    assert!(revision(&flows, cx) > before);
    assert_eq!(
        document(&flows, cx).flow.inputs[0].default,
        Some(json!({"a":[true,2,null]}))
    );
    name(cx, 1, "required");
    assert!(e.read_with(cx, |v, cx| v.prepare(cx).is_err()));
    name(cx, 1, "nullable");
    row_click(cx, "flow-input-remove", 0);
    assert_eq!(
        document(&flows, cx).flow.inputs,
        vec![FlowInputSpec::with_default("nullable", Value::Null)]
    );
    let mut expected = original;
    expected.flow.inputs = vec![FlowInputSpec::with_default("nullable", Value::Null)];
    assert_eq!(document(&flows, cx), expected);
    assert_eq!(
        parse_flow_yaml(&write_flow_yaml(&expected).unwrap()).unwrap(),
        expected
    );
    named_click(cx, "flow-input-add");
    name(cx, 2, "nullable");
    assert!(e.read_with(cx, |v, _| v.rows[1].error.is_some()));
    name(cx, 2, "new");
    assert!(document(&flows, cx).flow.inputs[1].default.is_none());
}

#[gpui_kit::test]
fn overrides_keep_types_block_invalid_and_never_mutate_document(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    settle(cx);
    install(
        &flows,
        vec![FlowInputSpec::with_default("arg", Value::Null)],
        cx,
    );
    let e = editor(&flows, cx);
    let original = document(&flows, cx);
    let revision = revision(&flows, cx);
    assert_eq!(
        e.read_with(cx, |v, cx| v.prepare(cx).unwrap().get("arg").cloned()),
        Some(Value::Null)
    );
    row_click(cx, "flow-input-override", 0);
    for (source_text, expected) in [
        ("null", Value::Null),
        ("false", json!(false)),
        ("0", json!(0)),
        ("[1,true]", json!([1, true])),
        ("\"00123\"", json!("00123")),
    ] {
        source(cx, "runtime", 0, source_text);
        assert_eq!(
            e.read_with(cx, |v, cx| v.prepare(cx).unwrap().get("arg").cloned()),
            Some(expected)
        );
        native_binding_accepts(
            document(&flows, cx).flow.inputs,
            e.read_with(cx, |v, cx| v.prepare(cx).unwrap()),
        );
    }
    source(cx, "runtime", 0, "{");
    assert!(e.read_with(cx, |v, cx| v.prepare(cx).is_err()));
    click(cx, "nav-home");
    click(cx, "nav-flows");
    assert!(e.read_with(cx, |v, cx| v.prepare(cx).is_err()));
    source(cx, "runtime", 0, "\"\"");
    named_click(cx, "flow-input-runtime-0-text");
    source(cx, "runtime", 0, "");
    assert_eq!(
        e.read_with(cx, |v, cx| v.prepare(cx).unwrap().get("arg").cloned()),
        Some(json!(""))
    );
    row_click(cx, "flow-input-reset", 0);
    assert_eq!(
        e.read_with(cx, |v, cx| v.prepare(cx).unwrap().get("arg").cloned()),
        Some(Value::Null)
    );
    assert_eq!(document(&flows, cx), original);
    assert_eq!(super::tests::revision(&flows, cx), revision);
}

#[gpui_kit::test]
fn rename_keeps_override_remove_recycle_cannot_inherit_it(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    settle(cx);
    install(&flows, vec![FlowInputSpec::required("name")], cx);
    let e = editor(&flows, cx);
    row_click(cx, "flow-input-override", 0);
    source(cx, "runtime", 0, "synthetic-value");
    click(cx, "flow-inputs-definitions-tab");
    name(cx, 0, "renamed");
    e.read_with(cx, |v, cx| {
        let p = v.prepare(cx).unwrap();
        assert_eq!(p.get("renamed"), Some(&json!("synthetic-value")));
        assert!(p.get("name").is_none());
        assert!(!format!("{p:?}").contains("synthetic-value"));
    });
    row_click(cx, "flow-input-remove", 0);
    named_click(cx, "flow-input-add");
    name(cx, 1, "renamed");
    assert!(e.read_with(cx, |v, cx| v.prepare(cx).is_err()));
    assert!(!e.read_with(cx, |v, _| v.rows[0].overridden));
}

#[gpui_kit::test]
fn inputs_work_without_steps_and_preserve_step_drafts_cached_hints_and_flow_isolation(
    cx: &mut TestAppContext,
) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    settle(cx);
    create(cx, "Empty");
    click(cx, "flow-inputs");
    named_click(cx, "flow-input-add");
    name(cx, 0, "base_url");
    let first = editor(&flows, cx);
    add(cx);
    replace(cx, "flow-step-url", "https://");
    let step = flows.read_with(cx, |v, _| v.inspector().unwrap().clone());
    click(cx, "flow-inputs");
    click(cx, "flow-inputs-definitions-tab");
    name(cx, 0, "host");
    step.read_with(cx, |v, cx| {
        assert_eq!(v.url().read(cx).value(), "https://");
        assert!(v.variables().iter().any(|v| v == "inputs.host"));
        assert!(!v.variables().iter().any(|v| v == "inputs.base_url"));
    });
    click(cx, "flow-inputs-run-tab");
    row_click(cx, "flow-input-override", 0);
    source(cx, "runtime", 0, "{{literal}}");
    create(cx, "Other");
    click(cx, "flow-inputs");
    named_click(cx, "flow-input-add");
    name(cx, 0, "host");
    let second = editor(&flows, cx);
    assert!(second.read_with(cx, |v, cx| v.prepare(cx).is_err()));
    click(cx, "flow-row-1");
    assert_eq!(
        first.read_with(cx, |v, cx| v.prepare(cx).unwrap().get("host").cloned()),
        Some(json!("{{literal}}"))
    );
    click(cx, "flow-inputs-back");
    assert!(cx.debug_bounds("flow-inspector").is_some());
}

#[test]
fn parser_modes_are_explicit_bounded_and_errors_do_not_echo_values() {
    for s in ["00123", "{{literal}}", "null", ""] {
        assert_eq!(value::parse(s, false), Ok(json!(s)));
    }
    for s in ["null", "false", "0", "[]", "{}"] {
        assert_eq!(value::parse(s, true), Ok(serde_json::from_str(s).unwrap()));
    }
    for s in ["", "{sensitive-token", &"[".repeat(200)] {
        let error = value::parse(s, true).unwrap_err();
        assert!(s.is_empty() || !error.contains(s));
    }
    assert!(value::parse(&"x".repeat(2 * 1024 * 1024 + 1), false).is_err());
}

#[gpui_kit::test]
fn environment_commits_cancel_and_overrides_are_independent(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    settle(cx);
    install(
        &flows,
        vec![FlowInputSpec::with_default(
            "base_url",
            "https://httpbingo.org",
        )],
        cx,
    );
    let e = editor(&flows, cx);
    let original = document(&flows, cx);
    let before = revision(&flows, cx);
    click(cx, "environment-settings");
    cx.simulate_input("Local");
    press(cx, "tab");
    cx.simulate_input("http://127.0.0.1:3000");
    click(cx, "environment-save");
    settle(cx);
    assert_eq!(
        e.read_with(cx, |v, cx| v.prepare(cx).unwrap().get("base_url").cloned()),
        Some(json!("http://127.0.0.1:3000"))
    );
    row_click(cx, "flow-input-override", 0);
    source(cx, "runtime", 0, "explicit");
    click(cx, "environment-settings");
    press(cx, "tab");
    press(cx, "secondary-a");
    cx.simulate_input("http://127.0.0.1:4000");
    click(cx, "environment-cancel");
    settle(cx);
    row_click(cx, "flow-input-reset", 0);
    assert_eq!(
        e.read_with(cx, |v, cx| v.prepare(cx).unwrap().get("base_url").cloned()),
        Some(json!("http://127.0.0.1:3000"))
    );
    row_click(cx, "flow-input-override", 0);
    named_click(cx, "flow-input-runtime-0-json");
    source(cx, "runtime", 0, "{");
    click(cx, "environment-settings");
    press(cx, "tab");
    press(cx, "secondary-a");
    cx.simulate_input("http://127.0.0.1:4000");
    click(cx, "environment-save");
    settle(cx);
    assert!(e.read_with(cx, |v, cx| v.prepare(cx).is_err()));
    row_click(cx, "flow-input-reset", 0);
    assert_eq!(
        e.read_with(cx, |v, cx| v.prepare(cx).unwrap().get("base_url").cloned()),
        Some(json!("http://127.0.0.1:4000"))
    );
    assert_eq!(document(&flows, cx), original);
    assert_eq!(revision(&flows, cx), before);
}

#[gpui_kit::test]
fn sensitive_reveal_is_temporary_and_parameter_errors_are_redacted(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    settle(cx);
    install(
        &flows,
        vec![FlowInputSpec::with_default("token", "synthetic-secret").sensitive()],
        cx,
    );
    let e = editor(&flows, cx);
    row_click(cx, "flow-input-override", 0);
    cx.update(|w, _| assert!(w.try_find("flow-input-runtime-0").is_none()));
    named_click(cx, "flow-input-runtime-0-reveal");
    source(cx, "runtime", 0, "other-synthetic");
    named_click(cx, "flow-input-runtime-0-json");
    source(cx, "runtime", 0, "{secret-draft");
    assert!(e.read_with(cx, |v, cx| v
        .prepare(cx)
        .unwrap_err()
        .iter()
        .all(|e| !e.contains("secret-draft"))));
    click(cx, "nav-home");
    click(cx, "nav-flows");
    settle(cx);
    cx.update(|w, _| assert!(w.try_find("flow-input-runtime-0").is_none()));
    named_click(cx, "flow-input-runtime-0-reveal");
    assert_eq!(
        e.read_with(cx, |v, cx| v.rows[0]
            .runtime
            .read(cx)
            .input
            .read(cx)
            .value()),
        "{secret-draft"
    );
    press(cx, "secondary-z");
    settle(cx);
    assert!(e.read_with(cx, |v, cx| v.prepare(cx).is_ok()));
    named_click(cx, "flow-input-add");
    click(cx, "flow-inputs-run-tab");
    cx.update(|w, _| assert!(w.try_find("flow-input-runtime-0").is_none()));
}

#[gpui_kit::test]
fn inputs_keep_ime_undo_focus_and_row_identity(cx: &mut TestAppContext) {
    use gpui_kit::EntityInputHandler;
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    settle(cx);
    install(
        &flows,
        vec![
            FlowInputSpec::required("first"),
            FlowInputSpec::with_default("second", ""),
        ],
        cx,
    );
    click(cx, "flow-inputs-definitions-tab");
    source(cx, "default", 1, "");
    let e = editor(&flows, cx);
    let input = e.read_with(cx, |v, cx| {
        v.rows[1].default.read(cx).input.read(cx).input_state()
    });
    cx.update(|w, cx| {
        input.update(cx, |s, cx| {
            s.replace_and_mark_text_in_range(None, "ni", Some(2..2), w, cx);
            assert!(s.marked_text_range(w, cx).is_some());
        })
    });
    cx.update(|_, cx| crate::ui::theme::apply(gpui_kit::component::ThemeMode::Dark, cx));
    settle(cx);
    cx.update(|w, cx| {
        input.update(cx, |s, cx| {
            assert!(s.marked_text_range(w, cx).is_some());
            s.replace_text_in_range(None, "你好", w, cx);
            assert!(s.marked_text_range(w, cx).is_none());
        })
    });
    assert_eq!(
        document(&flows, cx).flow.inputs[1].default,
        Some(json!("你好"))
    );
    press(cx, "secondary-z");
    assert_eq!(document(&flows, cx).flow.inputs[1].default, Some(json!("")));
    press(cx, "secondary-shift-z");
    press(cx, "secondary-a");
    press(cx, "secondary-c");
    cx.update(|_, cx| assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "你好"));
    press(cx, "tab");
    cx.update(|w, cx| assert!(!input.focus_handle(cx).is_focused(w)));
    press(cx, "shift-tab");
    cx.update(|w, cx| assert!(input.focus_handle(cx).is_focused(w)));
    press(cx, "enter");
    assert_eq!(
        document(&flows, cx).flow.inputs[1].default,
        Some(json!("\n"))
    );
    press(cx, "secondary-z");
    row_click(cx, "flow-input-remove", 0);
    cx.update(|w, cx| assert!(e.read(cx).rows[0].name.focus_handle(cx).is_focused(w)));
    assert_eq!(
        e.read_with(cx, |v, cx| v.rows[0]
            .default
            .read(cx)
            .input
            .read(cx)
            .input_state()),
        input
    );
    let bounds = reveal(cx, "flow-input-default-1".into());
    cx.simulate_click(bounds.origin + point(px(12.), px(12.)), Modifiers::none());
    press(cx, "secondary-z");
    assert_eq!(document(&flows, cx).flow.inputs[0].default, Some(json!("")));
    row_click(cx, "flow-input-remove", 1);
    cx.update(|w, cx| assert!(e.read(cx).add_focus.is_focused(w)));
}

#[gpui_kit::test]
fn inputs_layout_keeps_actions_reachable_and_console_geometry(cx: &mut TestAppContext) {
    use gpui_kit::size;
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    settle(cx);
    install(
        &flows,
        (0..12)
            .map(|i| {
                FlowInputSpec::with_default(
                    format!("input_{i}"),
                    json!({"long_value":"x".repeat(300)}),
                )
            })
            .collect(),
        cx,
    );
    for theme in [
        gpui_kit::component::ThemeMode::Light,
        gpui_kit::component::ThemeMode::Dark,
    ] {
        for scale in [1., 2.] {
            for (width, height) in [
                (1440., 960.),
                (1920., 1080.),
                (1024., 768.),
                (768., 720.),
                (1024., 600.),
            ] {
                cx.update(|_, cx| crate::ui::theme::apply(theme, cx));
                let handle = cx.update(|w, _| w.window_handle());
                cx.simulate_window_scale_factor_change(handle, scale);
                cx.simulate_window_resize(handle, size(px(width), px(height)));
                settle(cx);
                click(cx, "flow-inputs");
                settle(cx);
                let pane = cx.debug_bounds("flow-inputs-inspector").unwrap();
                let scroll = cx.debug_bounds("flow-inputs-scroll").unwrap();
                assert!(pane.size.width > px(230.));
                // Even the shortest viewport keeps a full action row reachable by scrolling.
                assert!(
                    scroll.size.height >= px(48.),
                    "{width}x{height} scale {scale}: {scroll:?}, {pane:?}"
                );
                assert!(scroll.bottom() <= pane.bottom() + px(1.));
                let before = cx.debug_bounds("flow-results-divider").unwrap();
                named_click(cx, "flow-input-add");
                settle(cx);
                let e = editor(&flows, cx);
                let id = e.read_with(cx, |v, _| v.rows.last().unwrap().id);
                name(cx, id, " ");
                assert!(e.read_with(cx, |v, cx| v.prepare(cx).is_err()));
                row_click(cx, "flow-input-remove", id);
                let after = cx.debug_bounds("flow-results-divider").unwrap();
                assert_eq!(before, after);
            }
        }
    }
}

#[gpui_kit::test]
fn check_ignores_runtime_errors_but_tracks_hidden_definition_errors_and_recovery(
    cx: &mut TestAppContext,
) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    create(cx, "Input validation boundary");
    add(cx);
    replace(cx, "flow-step-url", "http://127.0.0.1:1/local");
    click(cx, "flow-inputs");
    named_click(cx, "flow-input-add");
    name(cx, 0, "required");
    click(cx, "flow-check");
    let status =
        |cx: &mut VisualTestContext| flows.read_with(cx, |v, cx| v.console.read(cx).check_label());
    assert_eq!(
        status(cx),
        "Check passed",
        "missing run input does not block compilation"
    );
    click(cx, "flow-inputs-run-tab");
    row_click(cx, "flow-input-override", 0);
    named_click(cx, "flow-input-runtime-0-json");
    source(cx, "runtime", 0, "{runtime-draft");
    assert_eq!(status(cx), "Check passed");
    click(cx, "flow-check");
    assert_eq!(status(cx), "Check passed");
    click(cx, "flow-yaml-tab");
    click(cx, "flow-yaml-copy");
    let yaml = cx.read_from_clipboard().unwrap().text().unwrap();
    assert!(!yaml.contains("runtime-draft"));
    assert_eq!(parse_flow_yaml(&yaml).unwrap(), document(&flows, cx));
    click(cx, "flow-inputs");
    click(cx, "flow-inputs-definitions-tab");
    row_click(cx, "flow-input-has-default", 0);
    named_click(cx, "flow-input-default-0-json");
    source(cx, "default", 0, "{definition-draft");
    click(cx, "flow-inputs-back");
    click(cx, "flow-check");
    assert_eq!(status(cx), "Check failed");
    let id = flows.read_with(cx, |v, cx| v.console.read(cx).diagnostics()[0].0);
    cx.update(|w, cx| w.click(SharedString::from(format!("console-diagnostic-{id}")), cx));
    settle(cx);
    assert!(editor(&flows, cx).read_with(cx, |v, _| v.definitions));
    source(cx, "default", 0, "\"\"");
    assert_eq!(
        status(cx),
        "Check stale",
        "recovering to the same applied value changes draft revision"
    );
    click(cx, "flow-check");
    assert_eq!(status(cx), "Check passed");
}

#[gpui_kit::test]
fn file_save_excludes_runtime_values_and_blocks_hidden_invalid_defaults(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inputs.http.yml");
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    settle(cx);
    install(
        &flows,
        vec![FlowInputSpec::with_default("arg", json!(42))],
        cx,
    );
    let original = document(&flows, cx);
    click(cx, "flow-save");
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| Some(path.clone()));
    settle(cx);
    let saved = std::fs::read(&path).unwrap();
    row_click(cx, "flow-input-override", 0);
    source(cx, "runtime", 0, "\"runtime-secret-not-in-yaml\"");
    click(cx, "flow-save");
    settle(cx);
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    assert_eq!(document(&flows, cx), original);
    click(cx, "flow-inputs-definitions-tab");
    source(cx, "default", 0, "{broken-default");
    click(cx, "flow-canvas-tab");
    add(cx);
    click(cx, "flow-save");
    settle(cx);
    assert!(cx.debug_bounds("flow-file-error").is_some());
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    click(cx, "flow-inputs");
    click(cx, "flow-inputs-definitions-tab");
    source(cx, "default", 0, "42");
    click(cx, "flow-save");
    settle(cx);
    let stored = postman_flow::parse_flow_yaml(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(stored.flow.inputs, original.flow.inputs);
    assert!(!std::fs::read_to_string(&path)
        .unwrap()
        .contains("runtime-secret-not-in-yaml"));
}

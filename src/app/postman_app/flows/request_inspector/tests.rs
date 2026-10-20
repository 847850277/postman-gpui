mod definitions;
mod step_management;
use super::super::{
    tests::{add, click, create, press, replace, settle, setup},
    FlowsView,
};
use super::*;
use gpui::{point, px, ClipboardItem, Modifiers, TestAppContext, VisualTestContext};
use gpui_kit::test::TestWindowExt;

fn inspector(flows: &Entity<FlowsView>, cx: &mut VisualTestContext) -> Entity<RequestInspector> {
    flows.read_with(cx, |v, _| v.inspector().unwrap().clone())
}
fn request(
    flows: &Entity<FlowsView>,
    cx: &mut VisualTestContext,
) -> postman_flow::HttpRequestTemplate {
    flows.read_with(cx, |v, _| {
        match &v.library.active().unwrap().selected().unwrap().request {
            HttpRequestSource::Inline(r) => r.clone(),
            _ => panic!("inline request"),
        }
    })
}
fn revision(flows: &Entity<FlowsView>, cx: &mut VisualTestContext) -> u64 {
    flows.read_with(cx, |v, _| v.library.active().unwrap().revision())
}
/// Scroll the actual inspector before sending pointer/keyboard events into its retained Kit textarea.
fn edit_source(cx: &mut VisualTestContext, id: &'static str, text: &str) {
    for _ in 0..3 {
        let panel = cx.debug_bounds("flow-inspector-scroll").unwrap();
        let field = cx.debug_bounds(id).unwrap();
        if field.top() >= panel.top() && field.bottom() <= panel.bottom() {
            break;
        }
        let delta = panel.center().y - field.center().y;
        scroll(cx, panel.origin + point(px(5.), px(5.)), delta);
        settle(cx);
    }
    let field = cx.debug_bounds(id).unwrap();
    cx.simulate_click(field.origin + point(px(12.), px(12.)), Modifiers::none());
    press(cx, "secondary-a");
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.into())));
    if text.is_empty() {
        press(cx, "backspace");
    } else {
        press(cx, "secondary-v");
    }
    settle(cx);
}
fn top(cx: &mut VisualTestContext) {
    let panel = cx.debug_bounds("flow-inspector-scroll").unwrap();
    scroll(cx, panel.origin + point(px(5.), px(5.)), px(5000.));
    settle(cx);
}

fn inspector_click(cx: &mut VisualTestContext, id: &'static str) {
    for _ in 0..3 {
        let panel = cx.debug_bounds("flow-inspector-scroll").unwrap();
        let bounds = cx.update(|w, _| w.find(id).bounds());
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
    click(cx, id);
}

#[gpui_kit::test]
fn literal_url_uses_the_inspector_width_and_keeps_native_editing(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    click(cx, "nav-flows");
    create(cx, "URL editing");
    add(cx);
    let url = "https://httpbingo.org/delay/8";
    replace(cx, "flow-step-url", url);
    settle(cx);
    let method = cx.debug_bounds("flow-step-method").unwrap();
    let field = cx.debug_bounds("flow-step-url").unwrap();
    let panel = cx.debug_bounds("flow-inspector-scroll").unwrap();
    assert!(
        field.size.width >= panel.size.width - px(40.),
        "{field:?} in {panel:?}"
    );
    assert!(field.top() >= method.bottom());
    click(cx, "flow-step-url");
    press(cx, "secondary-a");
    press(cx, "secondary-c");
    assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), url);
    cx.write_to_clipboard(ClipboardItem::new_string(
        "https://example.test/a/much/longer/path?name=value&next=another-value".into(),
    ));
    press(cx, "secondary-v");
    assert_eq!(
        request(&flows, cx).url,
        TextTemplate::literal(
            "https://example.test/a/much/longer/path?name=value&next=another-value"
        )
    );
    press(cx, "secondary-z");
    assert_eq!(request(&flows, cx).url, TextTemplate::literal(url));
}

#[gpui_kit::test]
fn source_controls_apply_native_semantics_and_keep_last_valid_fields(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    click(cx, "nav-flows");
    settle(cx);
    create(cx, "Request editing");
    add(cx);
    replace(cx, "flow-step-url", "https://httpbingo.org/{{literal}}");
    click(cx, "flow-url-expression");
    assert_eq!(
        request(&flows, cx).url,
        TextTemplate::literal("https://httpbingo.org/{{literal}}")
    );
    let expression = r#"{"concat":[{"input":"host"},{"literal":"/post"},{"coalesce":[{"output":{"step":"seed","name":"id"}},{"calc":"inputs.fallback"}]}]}"#;
    edit_source(cx, "flow-url-source", expression);
    assert_eq!(
        request(&flows, cx).url,
        parse_text_expression(expression).unwrap()
    );
    top(cx);
    click(cx, "flow-url-literal");
    assert!(inspector(&flows, cx).read_with(cx, |v, _| v.expression_url));
    edit_source(cx, "flow-url-source", "{");
    assert!(inspector(&flows, cx).read_with(cx, |v, _| v.url_error.is_some()));
    assert_eq!(
        request(&flows, cx).url,
        parse_text_expression(expression).unwrap()
    );
    edit_source(
        cx,
        "flow-url-source",
        r#"{"literal":"https://httpbingo.org/get"}"#,
    );
    top(cx);
    click(cx, "flow-url-literal");
    assert!(!inspector(&flows, cx).read_with(cx, |v, _| v.expression_url));
    inspector_click(cx, "flow-headers-toggle");
    edit_source(
        cx,
        "flow-headers-source",
        "X-Test: first:second\nX-Test: \nAccept: application/json",
    );
    let headers = request(&flows, cx).headers;
    assert_eq!(headers.len(), 3);
    assert_eq!(headers[0].1, TextTemplate::literal("first:second"));
    assert_eq!(headers[1].1, TextTemplate::literal(""));
    edit_source(cx, "flow-headers-source", "missing separator");
    assert_eq!(request(&flows, cx).headers, headers);
    assert!(inspector(&flows, cx).read_with(cx, |v, _| v.headers_error.is_some()));
    let body = r#"{"kind":"json","value":{"object":{"typed":{"literal":[true,42,null,{"input":"literal data"}]},"ref":{"input":"name"}}}}"#;
    edit_source(cx, "flow-body-source", body);
    assert_eq!(
        request(&flows, cx).body,
        parse_body_expression(body).unwrap()
    );
    assert_eq!(
        request(&flows, cx).headers,
        headers,
        "body editing never updates headers"
    );
    edit_source(cx, "flow-body-source", r#"{"kind":"unknown"}"#);
    assert_eq!(
        request(&flows, cx).body,
        parse_body_expression(body).unwrap()
    );
    assert!(inspector(&flows, cx).read_with(cx, |v, _| v.body_error.is_some()));
    edit_source(cx, "flow-body-source", "");
    assert_eq!(request(&flows, cx).body, BodyTemplate::None);
    edit_source(cx, "flow-headers-source", "");
    assert!(request(&flows, cx).headers.is_empty());
    top(cx);
    inspector_click(cx, "flow-headers-expression");
    let headers =
        r#"[{"name":{"input":"header_name"},"value":{"output":{"step":"seed","name":"token"}}}]"#;
    edit_source(cx, "flow-headers-source", headers);
    assert_eq!(
        request(&flows, cx).headers,
        parse_header_expressions(headers).unwrap()
    );
    assert!(inspector(&flows, cx).read_with(cx, |v, _| !v.simple_headers_available));
}

#[gpui_kit::test]
fn invalid_buffers_survive_steps_flows_theme_resize_navigation_and_undo(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    click(cx, "nav-flows");
    settle(cx);
    create(cx, "First");
    add(cx);
    let first = inspector(&flows, cx);
    let valid = r#"{"kind":"raw","value":{"literal":"你好"}}"#;
    edit_source(cx, "flow-body-source", valid);
    let before = revision(&flows, cx);
    edit_source(cx, "flow-body-source", "{");
    assert!(revision(&flows, cx) > before);
    assert_eq!(
        request(&flows, cx).body,
        parse_body_expression(valid).unwrap()
    );
    add(cx);
    edit_source(cx, "flow-body-source", "{\"second\":");
    create(cx, "Second");
    add(cx);
    edit_source(cx, "flow-body-source", "{\"other-flow\":");
    click(cx, "flow-row-1");
    assert_eq!(
        inspector(&flows, cx).read_with(cx, |v, cx| v.body.read(cx).value()),
        "{\"second\":"
    );
    click(cx, "flow-node-1-request-1");
    assert_eq!(first.read_with(cx, |v, cx| v.body.read(cx).value()), "{");
    let before = revision(&flows, cx);
    click(cx, "nav-home");
    click(cx, "nav-http");
    click(cx, "nav-flows");
    cx.update(|_, cx| crate::ui::theme::apply(gpui_kit::component::ThemeMode::Dark, cx));
    click(cx, "console-maximize");
    click(cx, "console-maximize");
    let divider = cx.debug_bounds("flow-results-divider").unwrap().center();
    cx.update(|w, cx| w.drag(divider, divider + point(px(0.), px(-40.)), cx));
    settle(cx);
    assert_eq!(revision(&flows, cx), before);
    assert_eq!(first.read_with(cx, |v, cx| v.body.read(cx).value()), "{");
    edit_source(cx, "flow-body-source", valid);
    press(cx, "secondary-z");
    assert_eq!(first.read_with(cx, |v, cx| v.body.read(cx).value()), "{");
    press(cx, "secondary-shift-z");
    assert_eq!(first.read_with(cx, |v, cx| v.body.read(cx).value()), valid);
    press(cx, "end");
    press(cx, "enter");
    assert!(first.read_with(cx, |v, cx| v.body.read(cx).value().contains('\n')));
    press(cx, "secondary-enter");
    press(cx, "secondary-s");
    app.read_with(cx, |a, cx| {
        assert_eq!(a.view_model.read(cx).in_flight_count(), 0)
    });
    // Source edits don't overwrite command drafts or submit commands in the hidden owner.
    replace(cx, "console-command", "unfinished");
    click(cx, "flow-row-2");
    click(cx, "flow-row-1");
    cx.update(|w, _| assert_eq!(w.find("console-command").value(), Some("unfinished")));
    assert!(first.read_with(cx, |v, _| v.body_error.is_none()));
}

fn scroll(cx: &mut VisualTestContext, position: gpui::Point<gpui::Pixels>, delta: gpui::Pixels) {
    cx.simulate_event(gpui::ScrollWheelEvent {
        position,
        delta: gpui::ScrollDelta::Pixels(point(px(0.), delta)),
        touch_phase: gpui::TouchPhase::Moved,
        modifiers: Modifiers::none(),
    });
}

#[gpui_kit::test]
fn body_ime_clipboard_and_keyboard_traversal_use_retained_kit_state(cx: &mut TestAppContext) {
    use gpui::{EntityInputHandler, Focusable};
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    click(cx, "nav-flows");
    settle(cx);
    create(cx, "IME body");
    add(cx);
    let owner = inspector(&flows, cx);
    edit_source(cx, "flow-body-source", "");
    let input = owner.read_with(cx, |v, cx| v.body.read(cx).input_state());
    cx.update(|w, cx| {
        input.update(cx, |s, cx| {
            s.replace_and_mark_text_in_range(None, "ni", Some(2..2), w, cx);
            assert!(s.marked_text_range(w, cx).is_some());
        })
    });
    // Rerendering and theme changes must leave composition intact.
    cx.update(|_, cx| crate::ui::theme::apply(gpui_kit::component::ThemeMode::Dark, cx));
    settle(cx);
    cx.update(|w, cx| {
        input.update(cx, |s, cx| {
            assert!(s.marked_text_range(w, cx).is_some());
            s.replace_text_in_range(None, "你好", w, cx);
            assert!(s.marked_text_range(w, cx).is_none());
        })
    });
    assert_eq!(owner.read_with(cx, |v, cx| v.body.read(cx).value()), "你好");
    assert_eq!(request(&flows, cx).body, BodyTemplate::None);
    press(cx, "secondary-z");
    assert_eq!(owner.read_with(cx, |v, cx| v.body.read(cx).value()), "");
    press(cx, "secondary-shift-z");
    press(cx, "secondary-a");
    press(cx, "secondary-c");
    cx.update(|_, cx| assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "你好"));
    press(cx, "tab");
    cx.update(|w, cx| {
        assert!(
            !input.focus_handle(cx).is_focused(w),
            "Tab leaves multiline source"
        )
    });
    press(cx, "shift-tab");
    cx.update(|w, cx| {
        assert!(
            input.focus_handle(cx).is_focused(w),
            "Shift+Tab returns to source"
        )
    });
    assert_eq!(owner.read_with(cx, |v, cx| v.body.read(cx).value()), "你好");
}

#[gpui_kit::test]
fn long_source_errors_fit_the_inspector_at_all_supported_sizes(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    click(cx, "nav-flows");
    settle(cx);
    create(cx, "Responsive Request");
    add(cx);
    inspector_click(cx, "flow-headers-toggle");
    edit_source(cx, "flow-headers-source", &"missing-separator".repeat(40));
    edit_source(
        cx,
        "flow-body-source",
        &format!(r#"{{"kind":"{}"}}"#, "unknown".repeat(60)),
    );
    let owner = inspector(&flows, cx);
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
                top(cx);
                let panel = cx.debug_bounds("flow-inspector-scroll").unwrap();
                let console = cx.debug_bounds("flow-results").unwrap();
                assert!(panel.bottom() <= console.top() + px(1.));
                for id in [
                    "flow-step-name",
                    "flow-step-url",
                    "flow-headers-source",
                    "flow-body-source",
                ] {
                    let field = cx.debug_bounds(id).unwrap();
                    assert!(
                        field.left() >= panel.left() && field.right() <= panel.right() + px(1.),
                        "{id} overflow at {width}x{height}: {field:?}"
                    );
                    if id == "flow-step-url" {
                        let name = cx.debug_bounds("flow-step-name").unwrap();
                        assert!((field.left() - name.left()).abs() <= px(1.));
                        assert!((field.right() - name.right()).abs() <= px(1.));
                    }
                }
                let panel_height = panel.size.height;
                scroll(cx, panel.origin + point(px(5.), px(5.)), -panel_height);
                settle(cx);
                assert!(owner.read_with(cx, |v, _| v.tab_scroll[0].offset().y < px(0.)));
                assert_eq!(cx.debug_bounds("flow-results").unwrap(), console);
                assert_eq!(revision(&flows, cx), before);
                assert!(owner.read_with(cx, |v, _| v.body_error.is_some()
                    && v.headers_error.is_some()));
            }
        }
    }
}

#[gpui_kit::test]
fn loops_remain_readonly_and_selection_keeps_their_nested_documents(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    let document = postman_flow::parse_flow_yaml(include_str!(
        "../../../../../crates/postman-flow/examples/flows/httpbingo_loops.http.yml"
    ))
    .unwrap();
    let loop_id = document
        .flow
        .steps
        .iter()
        .find(|s| {
            matches!(
                s.request,
                HttpRequestSource::ForEach(_) | HttpRequestSource::RepeatUntil(_)
            )
        })
        .unwrap()
        .id
        .clone();
    cx.update(|w, cx| {
        flows.update(cx, |v, cx| {
            v.library.insert(document.clone());
            v.library.select_step(&loop_id);
            v.sync_fields(w, cx);
            v.sync_picker(w, cx);
            cx.notify();
        })
    });
    click(cx, "nav-flows");
    settle(cx);
    assert!(cx.debug_bounds("flow-step-readonly").is_some());
    assert!(cx.debug_bounds("flow-body-source").is_none());
    assert!(cx.debug_bounds("flow-step-url").is_none());
    flows.read_with(cx, |v, _| {
        assert_eq!(v.library.active().unwrap().document(), &document)
    });
}

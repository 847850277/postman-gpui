use super::*;
use crate::app::{PostmanApp, WorkspaceViewModel};
use gpui::{
    point, px, size, Bounds, ElementInputHandler, InputHandler, TestAppContext, VisualTestContext,
    WindowBounds, WindowOptions,
};
use gpui_kit::{component::WindowExt, test::TestWindowExt};

pub(super) fn setup(cx: &mut TestAppContext) -> (Entity<PostmanApp>, &mut VisualTestContext) {
    cx.update(|cx| {
        crate::assets::fonts::load_embedded_fonts(cx).unwrap();
        crate::ui::kit::init(cx);
        cx.set_reduce_motion(true);
    });
    let model = cx.new(|_| WorkspaceViewModel::new());
    let (handle, app) = cx
        .update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(0.), px(0.)),
                        size(px(1440.), px(960.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |w, cx| cx.new(|cx| PostmanApp::with_view_model(model, w, cx)),
            )
        })
        .unwrap();
    (app, VisualTestContext::from_window(handle, cx).into_mut())
}
pub(super) fn click(cx: &mut VisualTestContext, id: &'static str) {
    cx.update(|w, cx| w.click(id, cx));
}
pub(super) fn press(cx: &mut VisualTestContext, key: &str) {
    cx.update(|w, cx| w.press(key, cx));
}
pub(super) fn settle(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    for _ in 0..6 {
        cx.update(|w, cx| w.render_frame(cx));
    }
}
pub(super) fn replace(cx: &mut VisualTestContext, id: &'static str, text: &str) {
    click(cx, id);
    press(cx, "secondary-a");
    if text.is_empty() {
        press(cx, "backspace");
    } else {
        cx.simulate_input(text);
    }
}
pub(super) fn create(cx: &mut VisualTestContext, name: &str) {
    click(cx, "new-flow");
    cx.simulate_input(name);
    click(cx, "flow-create-submit");
    settle(cx);
}
pub(super) fn add(cx: &mut VisualTestContext) {
    click(cx, "add-step");
    click(cx, "flow-add-http");
    settle(cx);
}
fn current(flows: &Entity<FlowsView>, cx: &mut VisualTestContext) -> postman_flow::FlowDocument {
    flows.read_with(cx, |view, _| {
        view.library.active().unwrap().document().clone()
    })
}

#[gpui_kit::test]
fn kit_fields_sync_before_blur_and_documents_survive_navigation(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    click(cx, "nav-http");
    replace(cx, "request-url-input", "https://httpbingo.org/get?keep=1");
    click(cx, "nav-flows");
    settle(cx);
    create(cx, "First flow");
    add(cx);
    replace(cx, "flow-step-name", "请求一");
    replace(cx, "flow-step-url", "https://");
    assert_eq!(current(&flows, cx).flow.steps[0].name, "请求一");
    assert_eq!(
        model::request_summary(&current(&flows, cx).flow.steps[0]).1,
        "https://"
    );
    click(cx, "flow-step-method");
    press(cx, "down");
    press(cx, "enter");
    assert_eq!(
        model::request_summary(&current(&flows, cx).flow.steps[0]).0,
        Some(crate::models::HttpMethod::POST)
    );
    add(cx);
    replace(cx, "flow-step-name", "");
    replace(cx, "flow-step-url", "https://httpbingo.org/status/200");
    create(cx, "Second flow");
    add(cx);
    replace(cx, "flow-step-name", "Second document");
    click(cx, "flow-row-1");
    settle(cx);
    flows.read_with(cx, |v, cx| {
        assert_eq!(v.inspector().unwrap().read(cx).name().read(cx).value(), "");
        assert_eq!(
            v.inspector().unwrap().read(cx).url().read(cx).value(),
            "https://httpbingo.org/status/200"
        );
    });
    click(cx, "flow-node-1-request-1");
    flows.read_with(cx, |v, cx| {
        assert_eq!(
            v.inspector().unwrap().read(cx).url().read(cx).value(),
            "https://"
        )
    });
    replace(cx, "flow-title", "重命名流程");
    click(cx, "nav-home");
    click(cx, "recent-flow-2");
    flows.read_with(cx, |v, cx| {
        assert_eq!(
            v.inspector().unwrap().read(cx).name().read(cx).value(),
            "Second document"
        )
    });
    click(cx, "nav-http");
    app.read_with(cx, |app, cx| {
        assert_eq!(
            app.view_model.read(cx).active_request().unwrap().url(),
            "https://httpbingo.org/get?keep=1"
        )
    });
    click(cx, "nav-flows");
    press(cx, "secondary-enter");
    press(cx, "secondary-s");
    app.read_with(cx, |app, cx| {
        assert_eq!(app.view_model.read(cx).in_flight_count(), 0)
    });
    cx.update(|w, cx| assert!(!w.has_active_dialog(cx)));
    flows.read_with(cx, |v, cx| {
        assert_eq!(
            v.inspector().unwrap().read(cx).name().read(cx).value(),
            "Second document"
        )
    });
}

#[gpui_kit::test]
fn create_cancel_validation_focus_and_filter_keep_the_active_document(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    click(cx, "nav-flows");
    settle(cx);
    let previous_focus = cx.update(|w, cx| w.focused(cx));
    click(cx, "new-flow");
    cx.update(|w, cx| assert!(flows.read(cx).create_name.focus_handle(cx).is_focused(w)));
    press(cx, "enter");
    cx.update(|w, cx| assert!(w.has_active_dialog(cx)));
    flows.read_with(cx, |v, _| assert!(v.library.drafts().is_empty()));
    cx.simulate_input("取消的流程");
    press(cx, "escape");
    cx.update(|w, cx| assert!(!w.has_active_dialog(cx)));
    flows.read_with(cx, |v, _| assert!(v.library.drafts().is_empty()));
    // Restore the pre-dialog focus, including pointer-opened dialogs.
    cx.update(|w, cx| assert_eq!(w.focused(cx), previous_focus));
    click(cx, "new-flow");
    cx.update(|w, cx| assert!(w.has_active_dialog(cx)));
    flows.read_with(cx, |v, cx| assert_eq!(v.create_name.read(cx).value(), ""));
    cx.simulate_input("Flow A");
    press(cx, "enter");
    settle(cx);
    assert_eq!(current(&flows, cx).flow.name, "Flow A");
    create(cx, "Flow B");
    replace(cx, "flow-filter", "missing");
    flows.read_with(cx, |v, cx| {
        assert!(v.filtered(cx).is_empty());
        assert_eq!(v.library.active().unwrap().title(), "Flow B");
        assert_eq!(v.library.active().unwrap().revision(), 0);
    });
    replace(cx, "flow-filter", "flow a");
    click(cx, "flow-row-1");
    flows.read_with(cx, |v, _| {
        assert_eq!(v.library.active().unwrap().title(), "Flow A");
        assert_eq!(v.library.drafts().len(), 2);
    });
}

#[gpui_kit::test]
fn ime_commit_and_undo_do_not_cross_selected_steps(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    click(cx, "nav-flows");
    settle(cx);
    create(cx, "IME flow");
    add(cx);
    replace(cx, "flow-step-name", "A🦀");
    cx.update(|w, cx| {
        let mut handler = ElementInputHandler::new(
            w.find("flow-step-name").bounds(),
            flows.read(cx).inspector().unwrap().read(cx).name().clone(),
        );
        handler.replace_and_mark_text_in_range(None, "ni", Some(2..2), w, cx);
        assert_eq!(handler.marked_text_range(w, cx), Some(3..5));
        handler.replace_and_mark_text_in_range(None, "你好", Some(2..2), w, cx);
        handler.replace_text_in_range(None, "你好", w, cx);
    });
    assert_eq!(current(&flows, cx).flow.steps[0].name, "A🦀你好");
    add(cx);
    click(cx, "flow-step-name");
    press(cx, "secondary-z");
    flows.read_with(cx, |v, cx| {
        assert_eq!(
            v.inspector().unwrap().read(cx).name().read(cx).value(),
            "Request 2"
        )
    });
    click(cx, "flow-node-1-request-1");
    flows.read_with(cx, |v, cx| {
        assert_eq!(
            v.inspector().unwrap().read(cx).name().read(cx).value(),
            "A🦀你好"
        )
    });
}

#[gpui_kit::test]
fn api_sources_are_readonly_and_inline_expressions_are_editable(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    cx.update(|w, cx| {
        flows.update(cx, |view, cx| {
            let document = postman_flow::parse_flow_yaml(include_str!(
                "../../../../crates/postman-flow/examples/flows/httpbingo_catalog.http.yml"
            ))
            .unwrap();
            view.library.insert(document);
            view.library.select_step("seed");
            view.sync_fields(w, cx);
            view.sync_picker(w, cx);
            cx.notify();
        })
    });
    click(cx, "nav-flows");
    settle(cx);
    assert!(cx.debug_bounds("flow-step-readonly").is_some());
    assert!(cx.debug_bounds("flow-step-url").is_none());
    click(cx, "flow-node-1-echo");
    assert!(cx.debug_bounds("flow-url-source").is_some());
    replace(cx, "flow-step-name", "Preserved echo");
    let doc = current(&flows, cx);
    assert!(model::request_summary(&doc.flow.steps[1])
        .1
        .contains("expression"));
    assert_eq!(doc.flow.steps[1].checks.len(), 4);
}

#[gpui_kit::test]
fn long_library_and_canvas_keep_keyboard_selection_visible(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    cx.update(|w, cx| {
        flows.update(cx, |view, cx| {
            // Large deterministic documents exercise viewport virtualization, not an alternate UI path.
            for index in 0..80 {
                view.library.create(&format!("Flow {index:02}")).unwrap();
            }
            view.library.select(FlowId(1));
            for _ in 0..150 {
                view.library.append_step();
            }
            view.library.select_step("request-1");
            view.sync_fields(w, cx);
            view.sync_picker(w, cx);
            cx.notify();
        })
    });
    click(cx, "nav-flows");
    settle(cx);
    click(cx, "flow-node-1-request-1");
    press(cx, "end");
    settle(cx);
    flows.read_with(cx, |v, _| {
        assert_eq!(
            v.library.active().unwrap().selected().unwrap().id,
            "request-150"
        )
    });
    let canvas = cx.debug_bounds("flow-canvas").unwrap();
    let last = cx.debug_bounds("flow-node-1-request-150").unwrap();
    assert!(last.top() >= canvas.top() && last.bottom() <= canvas.bottom());
    press(cx, "up");
    settle(cx);
    flows.read_with(cx, |v, _| {
        assert_eq!(
            v.library.active().unwrap().selected().unwrap().id,
            "request-149"
        )
    });
    click(cx, "flow-row-1");
    for _ in 0..79 {
        press(cx, "down");
    }
    settle(cx);
    flows.read_with(cx, |v, _| {
        assert_eq!(v.library.active().unwrap().id(), FlowId(80))
    });
    let library = cx.debug_bounds("flow-library-list").unwrap();
    let last = cx.debug_bounds("flow-row-80").unwrap();
    assert!(last.top() >= library.top() && last.bottom() <= library.bottom());
    // Search changes only presentation. It neither drops documents nor moves selection.
    replace(cx, "flow-filter", "Flow 00");
    click(cx, "flow-row-1");
    settle(cx);
    flows.read_with(cx, |v, _| {
        assert_eq!(
            v.library.active().unwrap().selected().unwrap().id,
            "request-149"
        )
    });
    assert!(cx.debug_bounds("flow-node-1-request-149").is_some());
}

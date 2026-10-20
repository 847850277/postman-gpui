use super::super::{
    model::literal_url,
    tests::{click, create, press, replace, settle, setup},
};
use super::*;
use crate::app::PostmanApp;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{px, size, TestAppContext, VisualTestContext};

fn owners(
    app: &Entity<PostmanApp>,
    cx: &mut VisualTestContext,
) -> (Entity<FlowsView>, Entity<WorkspaceViewModel>) {
    app.read_with(cx, |a, _| (a.flows.clone(), a.view_model.clone()))
}
fn count(flows: &Entity<FlowsView>, cx: &mut VisualTestContext) -> usize {
    flows.read_with(cx, |v, _| v.library.drafts().len())
}
fn copied_url(flows: &Entity<FlowsView>, cx: &mut VisualTestContext) -> String {
    flows.read_with(cx, |v, _| {
        let postman_flow::HttpRequestSource::Inline(request) =
            &v.library.active().unwrap().selected().unwrap().request
        else {
            panic!("expected inline request")
        };
        literal_url(&request.url).unwrap().to_owned()
    })
}

#[gpui_kit::test]
fn http_transfer_copies_live_edits_once_and_opens_an_independent_request(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let (flows, workspace) = owners(&app, cx);
    click(cx, "nav-http");
    replace(
        cx,
        "request-url-input",
        "https://httpbingo.org/get?x=1&x=2&secret=hidden",
    );
    let before = workspace.read_with(cx, |v, _| {
        v.active_request().unwrap().request_draft().clone()
    });
    click(cx, "add-request-to-flow");
    replace(cx, "flow-transfer-name", "HTTP copy");
    click(cx, "flow-transfer-submit");
    settle(cx);
    assert_eq!(count(&flows, cx), 1);
    assert_eq!(copied_url(&flows, cx), before.url());
    flows.read_with(cx, |v, cx| {
        let d = v.library.active().unwrap();
        assert_eq!(d.revision(), 1);
        assert_eq!(d.document().flow.steps.len(), 1);
        let s = d.selected().unwrap();
        assert!(s.checks.is_empty() && s.exports.is_empty() && s.when.is_none());
        assert!(v.details && !v.inputs_open);
        assert_eq!(
            v.inspector().unwrap().read(cx).name().read(cx).value(),
            "Request to httpbingo.org"
        );
    });
    cx.update(|w, cx| {
        assert!(!w.has_active_dialog(cx));
        assert!(flows
            .read(cx)
            .inspector()
            .unwrap()
            .focus_handle(cx)
            .is_focused(w));
    });
    replace(cx, "flow-step-url", "https://httpbingo.org/changed");
    workspace.read_with(cx, |v, _| {
        assert_eq!(v.active_request().unwrap().request_draft(), &before);
        assert!(v.active_request().unwrap().is_dirty());
        assert_eq!(v.in_flight_count(), 0);
        assert!(v.history().is_empty());
    });
    click(cx, "nav-http");
    click(cx, "add-request-to-flow");
    press(cx, "escape");
    assert_eq!(count(&flows, cx), 1);
    click(cx, "add-request-to-flow");
    click(cx, "flow-transfer-cancel");
    flows.read_with(cx, |v, _| {
        assert_eq!(v.library.active().unwrap().document().flow.steps.len(), 1)
    });
}

#[gpui_kit::test]
fn transfer_revalidates_changed_closed_and_unsupported_sources_without_orphans(
    cx: &mut TestAppContext,
) {
    let (app, cx) = setup(cx);
    let (flows, workspace) = owners(&app, cx);
    click(cx, "nav-http");
    replace(cx, "request-url-input", "https://httpbingo.org/get");
    click(cx, "add-request-to-flow");
    cx.update(|_, cx| {
        workspace.update(cx, |v, cx| {
            v.active_request_mut().unwrap().set_tests_script("secret()");
            cx.notify();
        })
    });
    settle(cx);
    assert!(cx.debug_bounds("flow-transfer-error").is_some());
    click(cx, "flow-transfer-submit");
    assert_eq!(count(&flows, cx), 0);
    press(cx, "escape");
    click(cx, "add-request-to-flow");
    click(cx, "flow-transfer-submit");
    assert_eq!(count(&flows, cx), 0);
    press(cx, "escape");
    cx.update(|_, cx| {
        workspace.update(cx, |v, cx| {
            v.active_request_mut().unwrap().set_tests_script("");
            cx.notify();
        })
    });
    click(cx, "add-request-to-flow");
    cx.update(|_, cx| {
        workspace.update(cx, |v, cx| {
            v.close_tab_by_id(v.active_tab_id().unwrap());
            cx.notify();
        })
    });
    settle(cx);
    click(cx, "flow-transfer-submit");
    assert_eq!(count(&flows, cx), 0);
    press(cx, "escape");
}

#[gpui_kit::test]
fn add_step_chooses_an_inactive_http_tab_without_switching_it(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let (flows, workspace) = owners(&app, cx);
    click(cx, "nav-http");
    replace(cx, "request-url-input", "https://httpbingo.org/get?first=1");
    let source = workspace.read_with(cx, |v, _| v.active_tab_id().unwrap());
    click(cx, "new-tab-button");
    replace(
        cx,
        "request-url-input",
        "https://httpbingo.org/get?second=2",
    );
    let active = workspace.read_with(cx, |v, _| v.active_tab_id().unwrap());
    click(cx, "nav-flows");
    create(cx, "Destination");
    click(cx, "add-step");
    click(cx, "flow-transfer-source");
    cx.simulate_input(&format!("Tab {source}"));
    settle(cx);
    press(cx, "enter");
    click(cx, "flow-transfer-submit");
    settle(cx);
    assert_eq!(copied_url(&flows, cx), "https://httpbingo.org/get?first=1");
    workspace.read_with(cx, |v, _| {
        assert_eq!(v.active_tab_id(), Some(active));
        assert!(v.active_request().unwrap().url().contains("second=2"));
    });
    click(cx, "add-step");
    click(cx, "flow-add-http");
    settle(cx);
    flows.read_with(cx, |v, _| {
        assert_eq!(
            v.library.active().unwrap().selected().unwrap().id,
            "request-2"
        );
        assert_eq!(v.library.active().unwrap().revision(), 2);
    });
}

#[gpui_kit::test]
fn transfer_uses_the_editable_draft_while_send_owns_its_original_snapshot(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let (flows, workspace) = owners(&app, cx);
    click(cx, "nav-http");
    replace(cx, "request-url-input", "https://httpbingo.org/get?sent=1");
    let pending = cx.update(|_, cx| workspace.update(cx, |v, _| v.begin_send().unwrap()));
    replace(
        cx,
        "request-url-input",
        "https://httpbingo.org/get?edited=2",
    );
    click(cx, "add-request-to-flow");
    click(cx, "flow-transfer-submit");
    settle(cx);
    assert!(copied_url(&flows, cx).contains("edited=2"));
    assert!(pending.request().url.contains("sent=1"));
    workspace.read_with(cx, |v, _| {
        assert_eq!(v.in_flight_count(), 1);
        assert_eq!(
            v.send_id_for_tab_id(pending.tab_id()),
            Some(pending.send_id())
        );
        assert!(v.history().is_empty());
    });
}

#[gpui_kit::test]
fn transfer_dialog_and_toolbar_fit_themes_scales_and_compact_windows(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let (flows, _) = owners(&app, cx);
    click(cx, "nav-http");
    replace(cx, "request-url-input", "https://httpbingo.org/get");
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
                let handle = cx.update(|w, cx| {
                    crate::ui::theme::apply(theme, cx);
                    w.window_handle()
                });
                cx.simulate_window_scale_factor_change(handle, scale);
                cx.simulate_window_resize(handle, size(px(width), px(height)));
                settle(cx);
                cx.update(|w, cx| {
                    for id in ["add-request-to-flow", "save-request", "send-button"] {
                        let b = w.find(id).bounds();
                        assert!(
                            b.left() >= px(0.) && b.right() <= w.viewport_size().width,
                            "{id} at {width}×{height}"
                        );
                    }
                    w.click("add-request-to-flow", cx);
                });
                settle(cx);
                let content = cx.debug_bounds("flow-transfer-dialog").unwrap();
                cx.update(|w, _| {
                    let submit = w.find("flow-transfer-submit").bounds();
                    assert!(content.size.height > px(40.));
                    assert!(submit.bottom() <= w.viewport_size().height);
                    assert!(submit.right() <= w.viewport_size().width);
                });
                press(cx, "escape");
            }
        }
    }
    assert_eq!(count(&flows, cx), 0);
}

#[gpui_kit::test]
fn existing_duplicate_destinations_keep_editors_console_and_keyboard_focus(
    cx: &mut TestAppContext,
) {
    use super::super::tests::add;
    let (app, cx) = setup(cx);
    let (flows, _) = owners(&app, cx);
    click(cx, "nav-flows");
    create(cx, "Same name");
    add(cx);
    replace(cx, "flow-step-name", "Retained editor");
    replace(cx, "console-command", "unfinished command");
    let retained = flows.read_with(cx, |v, _| v.inspector().unwrap().clone());
    create(cx, "Same name");
    click(cx, "nav-http");
    replace(cx, "request-url-input", "https://httpbingo.org/get");
    click(cx, "add-request-to-flow");
    click(cx, "flow-transfer-destination");
    cx.simulate_input("#1");
    settle(cx);
    press(cx, "enter");
    flows.read_with(cx, |v, _| {
        assert!(v.library.active().unwrap().document().flow.steps.is_empty())
    });
    for _ in 0..10 {
        if cx.update(|w, _| w.find("flow-transfer-submit").focused() == Some(true)) {
            break;
        }
        press(cx, "tab");
    }
    cx.update(|w, _| assert_eq!(w.find("flow-transfer-submit").focused(), Some(true)));
    press(cx, "enter");
    settle(cx);
    flows.read_with(cx, |v, _| {
        assert_eq!(v.library.active().unwrap().id(), FlowId(1));
        assert_eq!(v.library.active().unwrap().document().flow.steps.len(), 2);
        assert_eq!(
            v.inspectors.get(&(FlowId(1), "request-1".into())).unwrap(),
            &retained
        );
        assert!(v.library.drafts()[1].document().flow.steps.is_empty());
    });
    cx.update(|w, _| {
        assert_eq!(
            w.find("console-command").value(),
            Some("unfinished command")
        )
    });
    click(cx, "flow-node-1-request-1");
    cx.update(|w, _| assert_eq!(w.find("flow-step-name").value(), Some("Retained editor")));
    click(cx, "nav-http");
    click(cx, "add-request-to-flow");
    press(cx, "escape");
    cx.update(|w, _| assert_eq!(w.find("add-request-to-flow").focused(), Some(true)));
}

#[gpui_kit::test]
fn long_duplicate_lists_and_invalid_new_names_remain_atomic(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let (flows, _) = owners(&app, cx);
    cx.update(|_, cx| {
        flows.update(cx, |v, _| {
            for _ in 0..80 {
                v.library.create(&"Long flow name ".repeat(5)).unwrap();
            }
        })
    });
    click(cx, "nav-http");
    replace(cx, "request-url-input", "https://httpbingo.org/get");
    click(cx, "add-request-to-flow");
    click(cx, "flow-transfer-destination");
    cx.simulate_input("Create a new flow");
    settle(cx);
    press(cx, "enter");
    replace(cx, "flow-transfer-name", " ");
    click(cx, "flow-transfer-submit");
    assert_eq!(count(&flows, cx), 80);
    assert!(cx.debug_bounds("flow-transfer-error").is_some());
    click(cx, "flow-transfer-destination");
    cx.simulate_input("#79");
    settle(cx);
    press(cx, "enter");
    click(cx, "flow-transfer-submit");
    settle(cx);
    flows.read_with(cx, |v, _| {
        assert_eq!(v.library.active().unwrap().id(), FlowId(79));
        assert_eq!(v.library.active().unwrap().revision(), 1);
        assert_eq!(
            v.library
                .drafts()
                .iter()
                .map(|d| d.document().flow.steps.len())
                .sum::<usize>(),
            1
        );
    });
}

#[gpui_kit::test]
fn canvas_cancel_returns_to_its_trigger_and_space_still_adds_a_blank_step(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let (flows, _) = owners(&app, cx);
    click(cx, "nav-flows");
    create(cx, "Canvas focus");
    click(cx, "canvas-add-step");
    press(cx, "escape");
    settle(cx);
    cx.update(|w, cx| assert!(flows.read(cx).canvas_add_step_focus.is_focused(w)));
    click(cx, "add-step");
    press(cx, "shift-tab");
    press(cx, "space");
    settle(cx);
    flows.read_with(cx, |v, _| {
        assert_eq!(v.library.active().unwrap().document().flow.steps.len(), 1)
    });
}

#[gpui_kit::test]
fn repeated_confirmation_callbacks_append_once_and_keep_the_same_selection(
    cx: &mut TestAppContext,
) {
    let (app, cx) = setup(cx);
    let (flows, workspace) = owners(&app, cx);
    click(cx, "nav-http");
    replace(cx, "request-url-input", "https://httpbingo.org/get");
    let tab = workspace.read_with(cx, |v, _| v.active_tab_id());
    let dialog = cx.update(|w, cx| {
        cx.new(|cx| TransferDialog::new(flows.clone(), workspace, tab, None, w, cx))
    });
    // A queued second activation may still own the dialog entity after its surface closes.
    for _ in 0..2 {
        cx.update(|w, cx| dialog.update(cx, |d, cx| d.submit(w, cx)));
        settle(cx);
    }
    flows.read_with(cx, |v, _| {
        assert_eq!(v.library.drafts().len(), 1);
        let draft = v.library.active().unwrap();
        assert_eq!(draft.revision(), 1);
        assert_eq!(draft.document().flow.steps.len(), 1);
        assert_eq!(draft.selected().unwrap().id, "request-1");
    });
}

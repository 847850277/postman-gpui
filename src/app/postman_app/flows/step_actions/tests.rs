use super::super::tests::{add, click, create, press, replace, settle, setup};
use super::*;
use gpui::{TestAppContext, VisualTestContext};
use gpui_kit::test::TestWindowExt;

fn tab_to(cx: &mut VisualTestContext, id: &'static str) {
    for _ in 0..6 {
        if cx.update(|w, _| w.find(id).focused() == Some(true)) {
            break;
        }
        press(cx, "tab");
    }
    cx.update(|w, _| assert_eq!(w.find(id).focused(), Some(true)));
}

#[gpui_kit::test]
fn kit_step_actions_keep_editors_and_confirm_removal(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    click(cx, "nav-flows");
    settle(cx);
    create(cx, "Step management");
    add(cx);
    replace(cx, "flow-step-name", "保留中文 draft");
    let first = flows.read_with(cx, |v, _| v.inspector().unwrap().clone());
    add(cx);
    let second = flows.read_with(cx, |v, _| v.inspector().unwrap().clone());
    click(cx, "flow-move-step-up");
    settle(cx);
    flows.read_with(cx, |v, _| {
        let draft = v.library.active().unwrap();
        assert_eq!(draft.document().flow.steps[0].id, "request-2");
        assert_eq!(draft.selected().unwrap().id, "request-2");
        assert_eq!(v.inspector().unwrap(), &second);
        assert_eq!(v.inspectors[&(draft.id(), "request-1".into())], first);
    });
    let boundary_revision = flows.read_with(cx, |v, _| v.library.active().unwrap().revision());
    cx.update(|w, cx| {
        // The pinned Kit version does not expose disabled state in every AX snapshot.
        w.click("flow-move-step-up", cx);
    });
    flows.read_with(cx, |v, _| {
        assert_eq!(v.library.active().unwrap().revision(), boundary_revision)
    });
    click(cx, "flow-remove-step");
    let before = flows.read_with(cx, |v, _| v.library.active().unwrap().revision());
    press(cx, "enter"); // A stray Enter on the dialog itself must not remove anything.
    cx.update(|w, cx| assert!(w.has_active_dialog(cx)));
    press(cx, "escape");
    settle(cx);
    cx.update(|w, cx| assert!(!w.has_active_dialog(cx)));
    flows.read_with(cx, |v, _| {
        assert_eq!(v.library.active().unwrap().revision(), before);
    });
    cx.update(|w, cx| assert!(flows.read(cx).remove_step_focus.is_focused(w)));
    click(cx, "flow-remove-step");
    click(cx, "flow-remove-cancel");
    settle(cx);
    click(cx, "flow-remove-step");
    tab_to(cx, "flow-remove-cancel");
    press(cx, "enter");
    settle(cx);
    cx.update(|w, cx| assert!(!w.has_active_dialog(cx)));
    flows.read_with(cx, |v, _| {
        assert_eq!(v.library.active().unwrap().revision(), before)
    });
    click(cx, "flow-remove-step");
    tab_to(cx, "flow-remove-confirm");
    press(cx, "space");
    settle(cx);
    flows.read_with(cx, |v, cx| {
        assert_eq!(v.library.active().unwrap().revision(), before + 1);
        assert_eq!(v.inspector().unwrap(), &first);
        assert_eq!(first.read(cx).name().read(cx).value(), "保留中文 draft");
        assert_eq!(v.inspectors.len(), 1);
        assert_eq!(v.step_subscriptions.len(), 1);
    });
    click(cx, "flow-step-name");
    press(cx, "secondary-z");
    flows.read_with(cx, |v, cx| {
        assert_ne!(
            v.inspector().unwrap().read(cx).name().read(cx).value(),
            "保留中文 draft"
        )
    });
    click(cx, "flow-remove-step");
    tab_to(cx, "flow-remove-confirm");
    press(cx, "enter");
    settle(cx);
    flows.read_with(cx, |v, _| {
        assert!(v.library.active().unwrap().selected().is_none());
        assert!(v.inspectors.is_empty());
        assert!(v.step_subscriptions.is_empty());
    });
    cx.update(|w, cx| {
        assert!(w.try_find("flow-remove-step").is_none());
        assert!(flows.read(cx).add_step_focus.is_focused(w));
    });
    add(cx);
    flows.read_with(cx, |v, _| {
        assert_eq!(
            v.library.active().unwrap().selected().unwrap().id,
            "request-3"
        )
    });
}

#[gpui_kit::test]
fn stale_confirmation_and_canvas_navigation_do_not_mutate_another_flow(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |app, _| app.flows.clone());
    click(cx, "nav-flows");
    settle(cx);
    create(cx, "First");
    add(cx);
    add(cx);
    click(cx, "flow-node-1-request-1");
    let before = flows.read_with(cx, |v, _| v.library.active().unwrap().document().clone());
    press(cx, "end");
    press(cx, "up");
    flows.read_with(cx, |v, _| {
        assert_eq!(v.library.active().unwrap().document(), &before)
    });
    create(cx, "Second");
    add(cx);
    click(cx, "flow-row-1");
    settle(cx);
    click(cx, "flow-remove-step");
    // Simulate an external context change while an old dialog is still alive.
    cx.update(|w, cx| flows.update(cx, |v, cx| v.select_flow(FlowId(2), w, cx)));
    click(cx, "flow-remove-confirm");
    settle(cx);
    flows.read_with(cx, |v, _| {
        assert_eq!(v.library.active().unwrap().document().flow.steps.len(), 1);
        assert_eq!(v.library.drafts()[0].document(), &before);
    });
}

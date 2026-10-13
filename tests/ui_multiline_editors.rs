//! Kit text editors retain request-owned values while resetting local history on rebind.
#[path = "common/ui.rs"]
mod ui;

use gpui::{AppContext, TestAppContext};
use postman_gpui::app::{PostmanApp, RequestViewModel, WorkspaceViewModel};

#[gpui::test]
fn body_scripts_and_tests_keep_newlines_focus_and_equal_valued_request_boundaries(
    cx: &mut TestAppContext,
) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);

    for (pane, kind, field) in [
        ("request-pane-body", Some("body-kind-json"), "body-input"),
        ("request-pane-body", Some("body-kind-raw"), "body-input"),
        ("request-pane-scripts", None, "script-editor"),
        ("request-pane-tests", None, "tests-editor"),
    ] {
        let read_value = |request: &RequestViewModel| match pane {
            "request-pane-scripts" => request.pre_request_script().to_string(),
            "request-pane-tests" => request.tests_script().to_string(),
            _ => request.body(),
        };
        ui::click(cx, "request-tab-0").unwrap();
        ui::click(cx, pane).unwrap();
        if let Some(kind) = kind {
            ui::choose_body_kind(cx, kind).unwrap();
        }
        ui::replace_text(cx, field, "first 😀").unwrap();
        ui::press(cx, "end enter");
        cx.simulate_input("second 中 e\u{301}");
        let complete = "first 😀\nsecond 中 e\u{301}";
        assert_eq!(
            workspace.read_with(cx, |m, _| read_value(m.active_request().unwrap())),
            complete
        );
        assert_eq!(
            workspace.read_with(cx, |m, _| m.history_len()),
            0,
            "plain Enter must never send"
        );
        ui::press(cx, "ctrl-z");
        assert_eq!(
            workspace.read_with(cx, |m, _| read_value(m.active_request().unwrap())),
            "first 😀\n"
        );
        ui::press(cx, "ctrl-y");
        let editing_focus = cx.update(|window, cx| window.focused(cx).unwrap());
        ui::press(cx, "tab");
        cx.update(|window, cx| assert_ne!(window.focused(cx).as_ref(), Some(&editing_focus)));
        ui::press(cx, "shift-tab");
        cx.update(|window, cx| assert_eq!(window.focused(cx).as_ref(), Some(&editing_focus)));
        assert_eq!(
            workspace.read_with(cx, |m, _| read_value(m.active_request().unwrap())),
            complete,
            "Tab traverses without adding indentation"
        );

        // Equal text is still a different request: old undo and IME state must not follow it.
        if workspace.read_with(cx, |m, _| m.tab_count()) == 1 {
            ui::click(cx, "new-tab-button").unwrap();
        } else {
            ui::click(cx, "request-tab-1").unwrap();
        }
        ui::click(cx, pane).unwrap();
        if let Some(kind) = kind {
            ui::choose_body_kind(cx, kind).unwrap();
        }
        ui::replace_text(cx, field, complete).unwrap();
        for tab in ["request-tab-0", "request-tab-1"] {
            ui::click(cx, tab).unwrap();
            ui::click(cx, field).unwrap();
            ui::press(cx, "ctrl-z");
            assert_eq!(
                workspace.read_with(cx, |m, _| read_value(m.active_request().unwrap())),
                complete,
                "{pane}: Undo must not cross an equal-valued request projection"
            );
        }
    }
}

//! Kit text editors retain request-owned values while resetting local history on rebind.
#[path = "common/ui.rs"]
mod ui;

use gpui::{AppContext, TestAppContext};
use postman_gpui::app::{PostmanApp, RequestViewModel, WorkspaceViewModel};

#[gpui_kit::test]
fn same_request_pane_switch_preserves_caret_and_undo(cx: &mut TestAppContext) {
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
        let value = |cx: &gpui::VisualTestContext| {
            workspace.read_with(cx, |m, _| {
                let r = m.active_request().unwrap();
                match pane {
                    "request-pane-scripts" => r.pre_request_script().to_owned(),
                    "request-pane-tests" => r.tests_script().to_owned(),
                    _ => r.body(),
                }
            })
        };
        ui::click(cx, pane).unwrap();
        if let Some(kind) = kind {
            ui::choose_body_kind(cx, kind).unwrap();
        }
        ui::replace_text(cx, field, "base").unwrap();
        ui::press(cx, "end enter");
        cx.simulate_input("delta");
        ui::press(cx, "ctrl-z");
        assert_eq!(value(cx), "base\n");
        ui::press(cx, "ctrl-y");
        let focus = cx.update(|w, cx| w.focused(cx).unwrap());
        ui::click(cx, "request-pane-headers").unwrap();
        ui::click(cx, pane).unwrap();
        // Reselecting the active pane must also preserve its editing state.
        ui::click(cx, pane).unwrap();
        cx.update(|w, cx| focus.focus(w, cx));
        ui::press(cx, "ctrl-z");
        assert_eq!(value(cx), "base\n", "{pane}: undo survives pane navigation");
        ui::press(cx, "ctrl-y");
        ui::press(cx, "left");
        ui::click(cx, "request-pane-headers").unwrap();
        ui::click(cx, pane).unwrap();
        cx.update(|w, cx| focus.focus(w, cx));
        cx.simulate_input("!");
        assert_eq!(
            value(cx),
            "base\ndelt!a",
            "{pane}: caret survives pane navigation"
        );
    }
}

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

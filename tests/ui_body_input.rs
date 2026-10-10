//! Focused interaction coverage for the split text/form body-input entities.

#[path = "common/ui.rs"]
mod ui;

use gpui::{AppContext, ClipboardItem, TestAppContext, VisualTestContext};
use gpui_kit::test::TestWindowExt;
use postman_gpui::app::{
    BodyKind, MultipartDraftValue, PostmanApp, RequestBodyDraft, WorkspaceViewModel,
};
use ui::{click, replace_text, right_click, scroll_down};

fn clipboard_text(cx: &TestAppContext) -> String {
    cx.read_from_clipboard()
        .and_then(|item| item.text())
        .unwrap_or_default()
}

/// Use the real Kit menu's focus and keyboard path, then verify it dismissed.
fn choose_body_menu_action(cx: &mut VisualTestContext, index: usize, label: &str) {
    let editor_focus = cx.update(|window, app| window.focused(app).unwrap());
    right_click(cx, "body-input").unwrap();
    cx.update(|window, app| {
        window.render_frame(app);
        let menu = window.find("popup-menu");
        assert!(menu.visible());
        assert_eq!(menu.focused(), Some(true));
        assert_eq!(window.within("popup-menu").find(index).label(), Some(label));
    });
    // A new popup has no selected item. Let deferred dismissal finish before
    // rendering again so the closing overlay cannot steal restored editor focus.
    for _ in 0..=index {
        cx.simulate_keystrokes("down");
    }
    cx.simulate_keystrokes("enter");
    assert!(!ui::kit_control_exists(cx, "popup-menu"));
    cx.update(|window, app| {
        assert_eq!(window.focused(app).as_ref(), Some(&editor_focus));
    });
}

#[gpui::test]
fn text_body_keeps_unicode_graphemes_intact_across_cursor_selection_and_context_menu(
    cx: &mut TestAppContext,
) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);
    let body = "A😀中e\u{301}";

    click(cx, "request-pane-body").unwrap();
    ui::choose_body_kind(cx, "body-kind-json").unwrap();
    replace_text(cx, "body-input", body).unwrap();
    click(cx, "body-input").unwrap();
    cx.simulate_keystrokes("home right shift-right cmd-c");
    assert_eq!(clipboard_text(cx), "😀");

    let editor_focus = cx.update(|window, app| window.focused(app).unwrap());
    cx.write_to_clipboard(ClipboardItem::new_string("copy sentinel".into()));
    choose_body_menu_action(cx, 3, "Copy");
    assert_eq!(clipboard_text(cx), "😀");

    // Right-click from a different focused control must preserve the selection;
    // Escape returns keyboard input to this editor, without another left click.
    click(cx, "url-input").unwrap();
    right_click(cx, "body-input").unwrap();
    cx.update(|window, _| {
        assert_eq!(window.find("popup-menu").focused(), Some(true));
    });
    cx.simulate_keystrokes("escape");
    assert!(!ui::kit_control_exists(cx, "popup-menu"));
    cx.update(|window, app| {
        assert_eq!(window.focused(app).as_ref(), Some(&editor_focus));
    });
    cx.write_to_clipboard(ClipboardItem::new_string("escape sentinel".into()));
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(clipboard_text(cx), "😀");

    cx.simulate_keystrokes("end shift-left cmd-c");
    assert_eq!(clipboard_text(cx), "e\u{301}");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .body()
            .to_string()),
        body
    );
}

#[gpui::test]
fn multiline_body_history_context_menu_and_mode_switch_keep_the_saved_draft(
    cx: &mut TestAppContext,
) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);
    let body = "first 😀\n中间 e\u{301}\nlast";

    click(cx, "request-pane-body").unwrap();
    ui::choose_body_kind(cx, "body-kind-json").unwrap();
    cx.write_to_clipboard(ClipboardItem::new_string(body.to_string()));
    click(cx, "body-input").unwrap();
    cx.simulate_keystrokes("ctrl-v");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .body()
            .to_string()),
        body
    );

    choose_body_menu_action(cx, 0, "Undo");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .body()
            .to_string()),
        ""
    );
    choose_body_menu_action(cx, 1, "Redo");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .body()
            .to_string()),
        body
    );

    choose_body_menu_action(cx, 5, "Select All");
    choose_body_menu_action(cx, 3, "Copy");
    assert_eq!(clipboard_text(cx), body);

    ui::choose_body_kind(cx, "body-kind-raw").unwrap();
    workspace.read_with(cx, |workspace, _| {
        let request = workspace.active_request().unwrap();
        assert_eq!(request.body_kind(), BodyKind::Raw);
        assert_eq!(request.body(), "");
    });
    ui::choose_body_kind(cx, "body-kind-json").unwrap();
    workspace.read_with(cx, |workspace, _| {
        let request = workspace.active_request().unwrap();
        assert_eq!(request.body_kind(), BodyKind::Json);
        assert_eq!(request.body(), body);
    });
}

#[gpui::test]
fn form_body_tab_navigation_persists_unicode_active_cells_and_scrolls(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);

    click(cx, "request-pane-body").unwrap();
    ui::choose_body_kind(cx, "body-kind-url-encoded").unwrap();
    click(cx, "body-form-key-0").unwrap();
    cx.simulate_input("标签");
    cx.simulate_keystrokes("tab");
    cx.simulate_input("你好 世界");
    cx.simulate_keystrokes("tab");
    cx.simulate_input("第二项");

    workspace.read_with(cx, |workspace, _| {
        let RequestBodyDraft::UrlEncoded(rows) = workspace.active_request().unwrap().body_draft()
        else {
            panic!("URL-encoded selection should keep a typed form draft");
        };
        assert_eq!(rows[0].key, "标签");
        assert_eq!(rows[0].value, "你好 世界");
        assert_eq!(rows[1].key, "第二项");
        assert!(rows[1].value.is_empty());
    });

    // Fill beyond the actual resizable viewport, including tall column layouts.
    for _ in 0..26 {
        click(cx, "body-form-add-row").unwrap();
    }
    assert!(cx.debug_bounds("body-form-scrollbar").is_some());
    scroll_down(cx, "body-form-scroll", 1_000.0).unwrap();
    assert!(cx.debug_bounds("body-form-row-27").is_some());
}

#[gpui::test]
fn cancelling_multipart_file_selection_leaves_the_typed_row_unchanged(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) = cx.add_window_view(move |window, cx| {
        ui::shell(window, cx, |window, cx| {
            PostmanApp::with_view_model(observed, window, cx)
        })
    });
    ui::open_http(cx);

    click(cx, "request-pane-body").unwrap();
    ui::choose_body_kind(cx, "body-kind-form-data").unwrap();
    click(cx, "body-form-key-0").unwrap();
    cx.simulate_input("upload");
    cx.simulate_keystrokes("enter");
    click(cx, "body-form-type-0").unwrap();
    let before = workspace.read_with(cx, |workspace, _| {
        workspace.active_request().unwrap().body_draft().clone()
    });

    click(cx, "body-form-file-0").unwrap();
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|options| {
        assert!(options.files);
        assert!(!options.directories);
        assert!(!options.multiple);
        None
    });
    cx.run_until_parked();

    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .active_request()
            .unwrap()
            .body_draft()
            .clone()),
        before
    );
    let RequestBodyDraft::Multipart(parts) = before else {
        panic!("multipart selection should keep a typed multipart draft");
    };
    assert_eq!(parts[0].name, "upload");
    assert!(matches!(
        &parts[0].value,
        MultipartDraftValue::File { path, file_name, content_type }
            if path.as_os_str().is_empty() && file_name.is_none() && content_type.is_none()
    ));
    assert!(cx.debug_bounds("body-form-file-0").is_some());
}

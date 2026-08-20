//! Focused interaction coverage for the split text/form body-input entities.

#[path = "common/ui.rs"]
mod ui;

use gpui::{AppContext, TestAppContext};
use postman_gpui::app::{MultipartDraftValue, PostmanApp, RequestBodyDraft, WorkspaceViewModel};
use ui::{click, replace_text, right_click, scroll_down};

fn clipboard_text(cx: &TestAppContext) -> String {
    cx.read_from_clipboard()
        .and_then(|item| item.text())
        .unwrap_or_default()
}

#[gpui::test]
fn text_body_keeps_unicode_graphemes_intact_across_cursor_selection_and_context_menu(
    cx: &mut TestAppContext,
) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));
    let body = "A😀中e\u{301}";

    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-json").unwrap();
    replace_text(cx, "body-input", body).unwrap();
    click(cx, "body-input").unwrap();
    cx.simulate_keystrokes("home right shift-right cmd-c");
    assert_eq!(clipboard_text(cx), "😀");

    right_click(cx, "body-input").unwrap();
    assert!(cx.debug_bounds("body-edit-menu").is_some());
    click(cx, "body-edit-menu-copy").unwrap();
    assert_eq!(clipboard_text(cx), "😀");

    click(cx, "body-input").unwrap();
    cx.simulate_keystrokes("end shift-left cmd-c");
    assert_eq!(clipboard_text(cx), "e\u{301}");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.body().to_string()),
        body
    );
}

#[gpui::test]
fn form_body_tab_navigation_persists_unicode_active_cells_and_scrolls(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-url-encoded").unwrap();
    click(cx, "body-form-key-0").unwrap();
    cx.simulate_input("标签");
    cx.simulate_keystrokes("tab");
    cx.simulate_input("你好 世界");
    cx.simulate_keystrokes("tab");
    cx.simulate_input("第二项");

    workspace.read_with(cx, |workspace, _| {
        let RequestBodyDraft::UrlEncoded(rows) = workspace.body_draft() else {
            panic!("URL-encoded selection should keep a typed form draft");
        };
        assert_eq!(rows[0].key, "标签");
        assert_eq!(rows[0].value, "你好 世界");
        assert_eq!(rows[1].key, "第二项");
        assert!(rows[1].value.is_empty());
    });

    for _ in 0..6 {
        click(cx, "body-form-add-row").unwrap();
    }
    assert!(cx.debug_bounds("body-form-scrollbar").is_some());
    scroll_down(cx, "body-form-scroll", 1_000.0).unwrap();
    assert!(cx.debug_bounds("body-form-row-7").is_some());
}

#[gpui::test]
fn cancelling_multipart_file_selection_leaves_the_typed_row_unchanged(cx: &mut TestAppContext) {
    let workspace = cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    click(cx, "request-pane-body").unwrap();
    click(cx, "body-kind-form-data").unwrap();
    click(cx, "body-form-key-0").unwrap();
    cx.simulate_input("upload");
    cx.simulate_keystrokes("enter");
    click(cx, "body-form-type-0").unwrap();
    let before = workspace.read_with(cx, |workspace, _| workspace.body_draft().clone());

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
        workspace.read_with(cx, |workspace, _| workspace.body_draft().clone()),
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

use super::super::tests::{add, click, create, press, replace, settle, setup};
use super::*;
use gpui::{Entity, TestAppContext, VisualTestContext};
use gpui_kit::test::TestWindowExt;
use std::{fs, path::Path};

mod acceptance;

fn save_key(cx: &mut VisualTestContext) {
    press(cx, "secondary-s");
}
fn save_as(cx: &mut VisualTestContext) {
    press(cx, "secondary-shift-s");
}
fn choose_save(cx: &mut VisualTestContext, path: &Path) {
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| Some(path.to_path_buf()));
    settle(cx);
}
fn dirty(flows: &Entity<FlowsView>, cx: &mut VisualTestContext) -> bool {
    flows.read_with(cx, |v, cx| v.is_dirty(v.library.active().unwrap().id(), cx))
}
fn recent(cx: &mut VisualTestContext, path: &Path) {
    let id = gpui::SharedString::from(format!(
        "flow-recent-{}",
        path.canonicalize().unwrap().display()
    ));
    cx.update(|w, cx| w.click(id, cx));
    settle(cx);
}
fn source(flows: &Entity<FlowsView>, cx: &mut VisualTestContext) -> Option<PathBuf> {
    flows.read_with(cx, |v, _| {
        v.library
            .active()
            .and_then(|d| d.saved().map(|s| s.path.clone()))
    })
}
fn document(path: &Path) -> FlowDocument {
    storage::open(path).unwrap().document
}

#[gpui_kit::test]
fn focused_save_export_save_as_and_restart_recent_reopen(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("flow.http.yml");
    let copy = dir.path().join("copy.http.yml");
    let renamed = dir.path().join("other.http.yml");
    let index = dir.path().join("recent-flows.json");
    let (app, win) = setup(cx);
    settle(win);
    let flows = app.read_with(win, |a, _| a.flows.clone());
    flows.update(win, |v, cx| v.configure_files(Some(index.clone()), cx));
    settle(win);
    click(win, "nav-flows");
    create(win, "Keep me");
    add(win);
    replace(win, "flow-step-url", "https://httpbingo.org/get?latest=1");
    save_key(win);
    choose_save(win, &path);
    assert_eq!(source(&flows, win), Some(path.canonicalize().unwrap()));
    assert!(!dirty(&flows, win));
    let first = document(&path);
    replace(win, "flow-title", "Changed name");
    click(win, "flow-export");
    choose_save(win, &copy);
    assert_eq!(document(&copy).flow.name, "Changed name");
    assert_eq!(document(&path), first);
    assert!(dirty(&flows, win));
    save_as(win);
    choose_save(win, &renamed);
    assert_eq!(source(&flows, win), Some(renamed.canonicalize().unwrap()));
    assert!(!dirty(&flows, win));
    assert_eq!(document(&path), first);
    press(win, "secondary-w");
    settle(win);
    assert!(flows.read_with(win, |v, _| v.library.active().is_none()));
    let (app2, win2) = setup(cx);
    settle(win2);
    let restored = app2.read_with(win2, |a, _| a.flows.clone());
    restored.update(win2, |v, cx| v.configure_files(Some(index), cx));
    settle(win2);
    click(win2, "nav-flows");
    click(win2, "flow-recent");
    recent(win2, &renamed);
    settle(win2);
    assert_eq!(
        source(&restored, win2),
        Some(renamed.canonicalize().unwrap())
    );
    assert!(!dirty(&restored, win2));
    restored.read_with(win2, |v, _| {
        assert_eq!(v.library.active().unwrap().document(), &document(&renamed))
    });
    // Reopening from Recent must leave app commands reachable without clicking an editor.
    press(win2, "secondary-o");
    assert!(win2.did_prompt_for_paths());
    win2.simulate_path_prompt_response(|_| None);
    settle(win2);
    save_as(win2);
    assert!(win2.did_prompt_for_new_path());
    win2.simulate_new_path_selection(|_| None);
    settle(win2);
    click(win2, "flow-recent");
    recent(win2, &renamed);
    settle(win2);
    assert_eq!(restored.read_with(win2, |v, _| v.library.drafts().len()), 1);
}
#[gpui_kit::test]
fn cancel_picker_close_and_edit_during_save_preserve_drafts(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("flow.http.yml");
    let (app, cx) = setup(cx);
    settle(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    create(cx, "Captured");
    save_key(cx);
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| None);
    settle(cx);
    assert!(dirty(&flows, cx));
    assert!(source(&flows, cx).is_none());
    save_key(cx);
    replace(cx, "flow-title", "Edited while picking");
    choose_save(cx, &path);
    assert_eq!(document(&path).flow.name, "Captured");
    assert!(dirty(&flows, cx));
    press(cx, "secondary-w");
    assert!(cx.debug_bounds("flow-close-dialog").is_some());
    save_key(cx);
    assert!(!cx.did_prompt_for_new_path());
    click(cx, "flow-close-cancel");
    assert!(dirty(&flows, cx));
    press(cx, "secondary-w");
    click(cx, "flow-close-save");
    settle(cx);
    assert_eq!(document(&path).flow.name, "Edited while picking");
    assert!(flows.read_with(cx, |v, _| v.library.active().is_none()));
    create(cx, "Discard this");
    press(cx, "secondary-w");
    click(cx, "flow-close-discard");
    settle(cx);
    assert!(flows.read_with(cx, |v, _| v.library.active().is_none()));
    assert_eq!(document(&path).flow.name, "Edited while picking");
}
#[gpui_kit::test]
fn save_identity_and_close_rechecks_survive_late_edits(cx: &mut TestAppContext) {
    use gpui::Focusable;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("first.http.yml");
    let second = dir.path().join("second.http.yml");
    let (app, cx) = setup(cx);
    settle(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    create(cx, "First");
    create(cx, "Second");
    click(cx, "flow-row-1");
    save_key(cx);
    save_key(cx); // A repeated command cannot start a second picker/write.
    click(cx, "flow-row-2");
    replace(cx, "flow-title", "Other active document");
    choose_save(cx, &path);
    assert_eq!(document(&path).flow.name, "First");
    assert!(source(&flows, cx).is_none());
    assert!(dirty(&flows, cx));
    click(cx, "flow-row-1");
    assert!(!dirty(&flows, cx));
    replace(cx, "flow-title", "Changed");
    assert!(dirty(&flows, cx));
    replace(cx, "flow-title", "First");
    assert!(!dirty(&flows, cx));

    click(cx, "flow-row-2");
    click(cx, "flow-title");
    cx.update(|w, cx| {
        assert!(
            flows.read(cx).title.focus_handle(cx).is_focused(w),
            "title initially focused"
        )
    });
    press(cx, "secondary-w");
    press(cx, "escape");
    settle(cx);
    cx.update(|w, cx| assert!(flows.read(cx).title.focus_handle(cx).is_focused(w)));
    press(cx, "secondary-w");
    click(cx, "flow-close-save");
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| None);
    settle(cx);
    assert!(cx.debug_bounds("flow-close-dialog").is_some());
    assert!(dirty(&flows, cx));
    click(cx, "flow-close-save");
    // A late retained-field update while the OS picker is active must not be discarded.
    cx.update(|w, cx| {
        flows.update(cx, |v, cx| {
            v.title.update(cx, |title, cx| {
                title.set_value("Arrived after Save", w, cx);
                cx.emit(gpui_kit::component::input::InputEvent::Change);
            });
        })
    });
    choose_save(cx, &second);
    assert_eq!(document(&second).flow.name, "Other active document");
    assert!(cx.debug_bounds("flow-close-dialog").is_some());
    assert!(dirty(&flows, cx));
    click(cx, "flow-close-cancel");
    assert_eq!(flows.read_with(cx, |v, _| v.library.drafts().len()), 2);
}

#[gpui_kit::test]
fn file_menu_supports_keyboard_recent_and_save_as(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    settle(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    click(cx, "flow-file-menu");
    press(cx, "down");
    press(cx, "down");
    press(cx, "enter");
    settle(cx);
    assert!(cx.debug_bounds("flow-recent-dialog").is_some());
    save_key(cx);
    assert!(!cx.did_prompt_for_new_path());
    press(cx, "escape");
    create(cx, "Menu Save as");
    click(cx, "flow-file-menu");
    press(cx, "down");
    press(cx, "enter");
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| None);
    settle(cx);
    assert!(dirty(&flows, cx));
}
#[gpui_kit::test]
fn external_conflict_and_failed_save_keep_last_baseline(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("flow.http.yml");
    let (app, cx) = setup(cx);
    settle(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    create(cx, "Saved");
    save_key(cx);
    choose_save(cx, &path);
    let original = fs::read(&path).unwrap();
    replace(cx, "flow-title", "Local edits");
    fs::write(&path, b"external edit").unwrap();
    save_key(cx);
    settle(cx);
    assert!(cx.debug_bounds("flow-conflict-dialog").is_some());
    click(cx, "flow-conflict-cancel");
    settle(cx);
    assert_eq!(fs::read(&path).unwrap(), b"external edit");
    assert!(dirty(&flows, cx));
    save_key(cx);
    settle(cx);
    click(cx, "flow-conflict-replace");
    settle(cx);
    assert_eq!(document(&path).flow.name, "Local edits");
    assert!(!dirty(&flows, cx));
    replace(cx, "flow-title", "Still open");
    save_as(cx);
    choose_save(cx, &dir.path().join("missing/flow.http.yml"));
    assert!(cx.debug_bounds("flow-file-error").is_some());
    assert!(dirty(&flows, cx));
    assert_eq!(document(&path).flow.name, "Local edits");
    assert_ne!(fs::read(&path).unwrap(), original);
}

fn edit_source(cx: &mut VisualTestContext, id: &'static str, text: &str) {
    use gpui::{point, px, Modifiers, ScrollDelta, ScrollWheelEvent, TouchPhase};
    for _ in 0..6 {
        let panel = cx.debug_bounds("flow-inspector-scroll").unwrap();
        let field = cx.debug_bounds(id).unwrap();
        if field.top() >= panel.top() && field.bottom() <= panel.bottom() {
            break;
        }
        cx.simulate_event(ScrollWheelEvent {
            position: panel.origin + point(px(16.), px(4.)),
            delta: ScrollDelta::Pixels(point(px(0.), panel.center().y - field.center().y)),
            modifiers: Modifiers::none(),
            touch_phase: TouchPhase::Moved,
        });
        settle(cx);
    }
    let b = cx.debug_bounds(id).unwrap();
    cx.simulate_click(b.origin + point(px(12.), px(12.)), Modifiers::none());
    press(cx, "secondary-a");
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.into()));
    press(
        cx,
        if text.is_empty() {
            "backspace"
        } else {
            "secondary-v"
        },
    );
    settle(cx);
}
fn choose_open(cx: &mut VisualTestContext, path: &Path) {
    click(cx, "flow-open");
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|_| Some(vec![path.to_path_buf()]));
    settle(cx);
}
#[gpui_kit::test]
fn hidden_invalid_edit_blocks_all_file_writes_and_close_save(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("flow.http.yml");
    let (app, cx) = setup(cx);
    settle(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    create(cx, "Pending draft");
    add(cx);
    replace(cx, "flow-step-url", "http://127.0.0.1:1/no-network");
    save_key(cx);
    choose_save(cx, &path);
    let bytes = fs::read(&path).unwrap();
    edit_source(cx, "flow-body-source", "{unfinished");
    add(cx);
    replace(cx, "flow-step-url", "http://127.0.0.1:1/second");
    for action in ["flow-save", "flow-export"] {
        click(cx, action);
        settle(cx);
        assert!(!cx.did_prompt_for_new_path());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    save_as(cx);
    settle(cx);
    assert!(!cx.did_prompt_for_new_path());
    assert!(dirty(&flows, cx));
    press(cx, "secondary-w");
    click(cx, "flow-close-save");
    settle(cx);
    assert!(cx.debug_bounds("flow-close-error").is_some());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    click(cx, "flow-review-edits");
    settle(cx);
    assert!(cx.debug_bounds("flow-close-dialog").is_none());
    assert!(dirty(&flows, cx));
    click(cx, "flow-node-1-request-1");
    edit_source(cx, "flow-body-source", "");
    save_key(cx);
    settle(cx);
    assert!(!dirty(&flows, cx));
}
#[gpui_kit::test]
fn open_errors_collision_and_missing_recent_leave_existing_editors_intact(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("first.http.yml");
    let second = dir.path().join("second.http.yml");
    let broken = dir.path().join("broken.http.yml");
    let (app, cx) = setup(cx);
    settle(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    create(cx, "Same name");
    save_key(cx);
    choose_save(cx, &path);
    let saved = fs::read(&path).unwrap();
    fs::write(&second, &saved).unwrap();
    fs::write(&broken, b"broken: [").unwrap();
    replace(cx, "flow-title", "Retained draft");
    choose_open(cx, &path);
    assert!(dirty(&flows, cx));
    assert_eq!(
        flows.read_with(cx, |v, _| v.library.active().unwrap().title().to_owned()),
        "Retained draft"
    );
    choose_open(cx, &broken);
    assert!(cx.debug_bounds("flow-file-error").is_some());
    assert!(dirty(&flows, cx));
    choose_open(cx, &second);
    assert!(!dirty(&flows, cx));
    assert_eq!(flows.read_with(cx, |v, _| v.library.drafts().len()), 2);
    save_as(cx);
    choose_save(cx, &path);
    assert!(cx.debug_bounds("flow-file-error").is_some());
    assert_eq!(fs::read(&path).unwrap(), saved);
    click(cx, "flow-export");
    choose_save(cx, &second);
    assert!(cx.debug_bounds("flow-file-error").is_some());
    press(cx, "secondary-w");
    settle(cx);
    fs::remove_file(&second).unwrap();
    click(cx, "flow-recent");
    let id = gpui::SharedString::from(format!(
        "flow-recent-{}",
        path.canonicalize()
            .unwrap()
            .with_file_name("second.http.yml")
            .display()
    ));
    cx.update(|w, cx| w.click(id, cx));
    settle(cx);
    assert!(cx.debug_bounds("flow-file-error").is_some());
    assert_eq!(flows.read_with(cx, |v, _| v.library.drafts().len()), 1);
    assert!(dirty(&flows, cx));
}

#[gpui_kit::test]
fn mixed_http_flow_quit_cancellation_keeps_both_sessions(cx: &mut TestAppContext) {
    let (app, cx) = setup(cx);
    settle(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-http");
    replace(cx, "request-url-input", "http://127.0.0.1:1/unsaved");
    click(cx, "nav-flows");
    create(cx, "Unsaved Flow");
    cx.update(|w, cx| w.dispatch_action(Box::new(crate::app::Quit), cx));
    settle(cx);
    assert!(cx.debug_bounds("unsaved-request-dialog").is_some());
    click(cx, "request-save-discard");
    settle(cx);
    assert!(cx.debug_bounds("flow-close-dialog").is_some());
    click(cx, "flow-close-cancel");
    settle(cx);
    assert!(dirty(&flows, cx));
    app.read_with(cx, |a, cx| {
        assert!(a.view_model.read(cx).active_request().unwrap().is_dirty())
    });
    cx.update(|w, cx| w.dispatch_action(Box::new(crate::app::Quit), cx));
    settle(cx);
    assert!(cx.debug_bounds("unsaved-request-dialog").is_some());
    click(cx, "request-save-cancel");
    settle(cx);
    assert!(dirty(&flows, cx));
    assert_eq!(flows.read_with(cx, |v, _| v.library.drafts().len()), 1);
}
#[gpui_kit::test]
fn file_commands_and_close_dialog_fit_themes_and_compact_windows(cx: &mut TestAppContext) {
    use gpui::{px, size};
    use gpui_kit::component::ThemeMode;
    let (_, cx) = setup(cx);
    settle(cx);
    click(cx, "nav-flows");
    create(
        cx,
        "A long Flow name that must not push document actions outside the viewport",
    );
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        for (width, height) in [
            (1440., 960.),
            (1920., 1080.),
            (1024., 768.),
            (768., 720.),
            (1024., 600.),
        ] {
            let handle = cx.update(|w, cx| {
                crate::ui::theme::apply(mode, cx);
                w.window_handle()
            });
            cx.simulate_window_resize(handle, size(px(width), px(height)));
            settle(cx);
            for id in [
                "flow-open",
                "flow-save",
                "flow-file-menu",
                "flow-dirty-status",
            ] {
                let b = cx.debug_bounds(id).unwrap();
                assert!(
                    b.left() >= px(0.) && b.right() <= px(width) && b.bottom() <= px(height),
                    "{id} at {width}×{height}: {b:?}"
                );
            }
            press(cx, "secondary-w");
            settle(cx);
            for id in ["flow-close-cancel", "flow-close-discard", "flow-close-save"] {
                let b = cx.debug_bounds(id).unwrap();
                assert!(
                    b.left() >= px(0.) && b.right() <= px(width) && b.bottom() <= px(height),
                    "{id} at {width}×{height}: {b:?}"
                );
            }
            press(cx, "escape");
            settle(cx);
            assert!(cx.debug_bounds("flow-close-dialog").is_none());
        }
    }
}

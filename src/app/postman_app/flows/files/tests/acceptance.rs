//! Complete native journeys use the exact HTML design samples and controlled HTTP evidence.
use super::*;
use crate::app::postman_app::flows::execution::Status;
use gpui::{point, px, ClipboardItem, Modifiers};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

fn counted(
    counter: &Arc<AtomicUsize>,
    value: Value,
) -> impl Fn(&mockito::Request) -> Vec<u8> + Send + Sync + 'static {
    let counter = counter.clone();
    move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
        value.to_string().into_bytes()
    }
}

fn clipboard(cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap())
}

fn runtime_base(cx: &mut VisualTestContext, url: &str) {
    click(cx, "flow-inputs");
    settle(cx);
    cx.update(|w, cx| w.click(("flow-input-override", 0_u64), cx));
    settle(cx);
    let bounds = cx.debug_bounds("flow-input-runtime-0").unwrap();
    cx.simulate_click(bounds.origin + point(px(12.), px(12.)), Modifiers::none());
    press(cx, "secondary-a");
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(url.into())));
    press(cx, "secondary-v");
    click(cx, "flow-inputs-back");
    settle(cx);
}

#[gpui_kit::test]
fn design_examples_open_check_save_run_inspect_and_reopen_without_session_values(
    cx: &mut TestAppContext,
) {
    let mut server = mockito::Server::new();
    let uuid = "00000000-0000-4000-8000-000000000247";
    let requests = Arc::new(AtomicUsize::new(0));
    let mocks = [
        server
            .mock("GET", "/uuid")
            .with_body_from_request(counted(&requests, json!({"uuid":uuid})))
            .expect(2)
            .create(),
        server
            .mock("POST", "/post")
            .match_body(mockito::Matcher::Json(
                json!({"id":uuid,"name":"Maya Chen"}),
            ))
            .with_body_from_request(counted(
                &requests,
                json!({"json":{"id":uuid,"name":"Maya Chen"}}),
            ))
            .expect(2)
            .create(),
        server
            .mock("PATCH", "/patch")
            .match_body(mockito::Matcher::Json(json!({"id":uuid,"status":"active"})))
            .with_body_from_request(counted(
                &requests,
                json!({"json":{"id":uuid,"status":"active"}}),
            ))
            .expect(2)
            .create(),
        server
            .mock("GET", "/anything/health?status=ready")
            .with_body_from_request(counted(&requests, json!({"args":{"status":["ready"]}})))
            .expect(2)
            .create(),
    ];
    for (sample, name) in [
        ("echo-chain", "UUID and JSON echo"),
        ("poll-status", "Poll echoed status"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.http.yml");
        let saved = dir.path().join("saved.yaml");
        let exported = dir.path().join("exported");
        let index = dir.path().join("recent-flows.json");
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("tests/fixtures/flows/{sample}.http.yml")),
            &input,
        )
        .unwrap();
        let original = document(&input);
        let (app, win) = setup(cx);
        settle(win);
        let flows = app.read_with(win, |a, _| a.flows.clone());
        flows.update(win, |v, cx| v.configure_files(Some(index.clone()), cx));
        settle(win);
        click(win, "nav-http");
        settle(win);
        let url_bounds = win.debug_bounds("url-input").unwrap();
        win.simulate_click(url_bounds.center(), Modifiers::none());
        press(win, "secondary-a");
        win.simulate_input("https://unrelated.test/retained?draft=1");
        click(win, "nav-home");
        click(win, "nav-flows");
        choose_open(win, &input);
        let id = flows.read_with(win, |v, _| v.library.active().unwrap().id());
        runtime_base(win, &server.url());
        click(win, "flow-check");
        settle(win);
        click(win, "console-copy");
        assert!(clipboard(win).contains("Check passed"));
        flows.read_with(win, |v, _| assert!(v.execution.runs.is_empty()));
        // Check/YAML and file operations must not contact the server.
        let before = if sample == "echo-chain" { 0 } else { 6 };
        assert_eq!(requests.load(Ordering::SeqCst), before);
        click(win, "flow-yaml-tab");
        settle(win);
        click(win, "flow-yaml-copy");
        assert_eq!(
            postman_flow::parse_flow_yaml(&clipboard(win)).unwrap(),
            original
        );
        replace(win, "flow-title", &format!("{name} accepted"));
        save_as(win);
        choose_save(win, &saved);
        click(win, "flow-export");
        choose_save(win, &exported);
        let mut expected = original.clone();
        expected.flow.name = format!("{name} accepted");
        assert_eq!(document(&saved), expected);
        assert_eq!(document(&exported), expected);
        assert_eq!(document(&input), original);
        assert!(!dirty(&flows, win));
        assert_eq!(requests.load(Ordering::SeqCst), before);
        click(win, "flow-run");
        settle(win);
        flows.read_with(win, |v, _| {
            let run = &v.execution.runs[&id];
            assert_eq!(run.status, Status::Succeeded);
            assert!(run.steps.iter().all(|s| s.status == Status::Succeeded));
        });
        click(win, "console-tab-steps");
        settle(win);
        click(win, "console-maximize");
        settle(win);
        click(win, "result-row-0");
        settle(win);
        if sample == "poll-status" {
            // Loop -> first iteration -> request, preserving scoped identity.
            click(win, "result-row-0");
            settle(win);
            click(win, "result-row-0");
            settle(win);
        }
        click(win, "result-row-1"); // extraction after its status check
        settle(win);
        click(win, "result-copy-value");
        let extracted: Value = serde_json::from_str(&clipboard(win)).unwrap();
        // Ordinary query values stay readable in Steps/Copy. The loop evaluates
        // the same unmodified internal value and succeeds on attempt 1.
        assert_eq!(
            extracted,
            json!(if sample == "echo-chain" {
                uuid
            } else {
                "ready"
            })
        );
        press(win, "escape");
        click(win, "console-tab-outputs");
        settle(win);
        if sample == "echo-chain" {
            click(win, "console-copy");
            assert_eq!(
                serde_json::from_str::<Value>(&clipboard(win)).unwrap(),
                json!({"request_id":uuid,"status":"active"})
            );
        }
        replace(win, "console-command", "run");
        press(win, "enter");
        settle(win);
        flows.read_with(win, |v, _| {
            assert_eq!(v.execution.runs[&id].id, 2);
            assert_eq!(v.execution.runs[&id].status, Status::Succeeded);
        });
        click(win, "nav-http");
        settle(win);
        app.read_with(win, |a, cx| {
            assert_eq!(
                a.view_model.read(cx).active_request().unwrap().url(),
                "https://unrelated.test/retained?draft=1"
            )
        });
        click(win, "nav-flows");
        press(win, "secondary-w");
        settle(win);
        // Fresh production entity reads only durable state. A separate process restart is
        // additionally exercised during native acceptance, not claimed by this test.
        let (reopened, win) = setup(cx);
        settle(win);
        let restored = reopened.read_with(win, |a, _| a.flows.clone());
        restored.update(win, |v, cx| v.configure_files(Some(index), cx));
        settle(win);
        click(win, "nav-flows");
        click(win, "flow-recent");
        recent(win, &saved);
        assert!(!dirty(&restored, win));
        restored.read_with(win, |v, _| {
            assert_eq!(v.library.active().unwrap().document(), &expected);
            assert!(v.execution.runs.is_empty());
        });
        click(win, "flow-inputs");
        settle(win);
        restored.read_with(win, |v, cx| {
            let id = v.library.active().unwrap().id();
            assert_eq!(
                v.inputs[&id].read(cx).prepare(cx).unwrap().get("base_url"),
                Some(&json!("https://httpbingo.org"))
            );
        });
    }
    for mock in mocks {
        mock.assert();
    }
}

#[gpui_kit::test]
fn readonly_catalog_and_nested_loop_fields_survive_native_edit_export_and_reopen(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let (app, cx) = setup(cx);
    settle(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    for (index, relative) in [
        "crates/postman-flow/examples/flows/httpbingo_multi_hop.http.yml",
        "crates/postman-flow/examples/flows/httpbingo_catalog.http.yml",
        "crates/postman-flow/tests/fixtures/inspection-nested.http.yml",
    ]
    .iter()
    .enumerate()
    {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
        let source_bytes = fs::read(&source).unwrap();
        let saved = dir.path().join(format!("{index}.yaml"));
        let exported = dir.path().join(format!("{index}.export"));
        choose_open(cx, &source);
        let mut expected = document(&source);
        expected.flow.name = format!("Acceptance {index}");
        replace(cx, "flow-title", &expected.flow.name);
        save_as(cx);
        choose_save(cx, &saved);
        click(cx, "flow-export");
        choose_save(cx, &exported);
        assert_eq!(document(&saved), expected);
        assert_eq!(document(&exported), expected);
        assert_eq!(fs::read(&source).unwrap(), source_bytes);
        press(cx, "secondary-w");
        settle(cx);
        choose_open(cx, &exported);
        assert!(!dirty(&flows, cx));
        flows.read_with(cx, |v, _| {
            assert_eq!(v.library.active().unwrap().document(), &expected)
        });
        click(cx, "flow-check");
        settle(cx);
        click(cx, "console-copy");
        assert!(clipboard(cx).contains("Check passed"));
        flows.read_with(cx, |v, _| assert!(v.execution.runs.is_empty()));
        press(cx, "secondary-w");
        settle(cx);
    }
}

#[gpui_kit::test]
fn rejected_rust_source_preserves_pending_edits_and_recents_then_yaml_recovers(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let saved = dir.path().join("draft.http.yml");
    let index = dir.path().join("recent-flows.json");
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("crates/postman-flow/examples/httpbingo_multi_hop_chain.rs");
    let yaml = source
        .with_file_name("flows")
        .join("httpbingo_multi_hop.http.yml");
    let source_bytes = fs::read(&source).unwrap();
    let (app, cx) = setup(cx);
    settle(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    flows.update(cx, |v, cx| v.configure_files(Some(index.clone()), cx));
    settle(cx);
    click(cx, "nav-flows");
    create(cx, "Retained draft");
    add(cx);
    replace(cx, "flow-step-url", "http://127.0.0.1:1/no-network");
    save_key(cx);
    choose_save(cx, &saved);
    let saved_bytes = fs::read(&saved).unwrap();
    let recent_bytes = fs::read(&index).unwrap();
    let id = flows.read_with(cx, |v, _| v.library.active().unwrap().id());
    edit_source(cx, "flow-body-source", "{unfinished");
    let pending = flows.read_with(cx, |v, cx| v.draft_issues(id, cx));
    assert!(!pending.is_empty());
    choose_open(cx, &source);
    flows.read_with(cx, |v, cx| {
        assert_eq!(v.library.active().unwrap().id(), id);
        assert!(v.files.error.as_ref().unwrap().contains("Rust source"));
        assert_eq!(v.draft_issues(id, cx).len(), pending.len());
        assert!(v.is_dirty(id, cx));
    });
    assert_eq!(fs::read(&saved).unwrap(), saved_bytes);
    assert_eq!(fs::read(&source).unwrap(), source_bytes);
    assert_eq!(fs::read(&index).unwrap(), recent_bytes);
    choose_open(cx, &yaml);
    assert!(cx.debug_bounds("flow-file-error").is_none());
    flows.read_with(cx, |v, cx| {
        assert_eq!(v.library.active().unwrap().document(), &document(&yaml));
        assert_eq!(v.draft_issues(id, cx).len(), pending.len());
        assert!(v.is_dirty(id, cx));
    });
}

#[gpui_kit::test]
fn long_open_diagnostic_keeps_compact_document_and_console_reachable(cx: &mut TestAppContext) {
    use gpui::{point, px, size, Modifiers, ScrollDelta, ScrollWheelEvent, TouchPhase};
    let dir = tempfile::tempdir().unwrap();
    let broken = dir.path().join("broken.yaml");
    // A parser error can contain an arbitrarily long invalid enum value.
    fs::write(&broken, format!("schema_version: 1\nflow:\n  name: Broken\n  steps: [{{id: bad, request: {{kind: http, method: '{}', url: {{literal: 'https://example.test'}}}}}}]\n", "unexpected ".repeat(300))).unwrap();
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |a, _| a.flows.clone());
    click(cx, "nav-flows");
    create(cx, "Keep editing");
    for (width, height) in [(768., 600.), (1024., 600.)] {
        let handle = cx.update(|w, _| w.window_handle());
        cx.simulate_window_resize(handle, size(px(width), px(height)));
        settle(cx);
        choose_open(cx, &broken);
        assert!(flows.read_with(cx, |v, _| v.files.error.as_ref().unwrap().len() > 2000));
        let error = cx.debug_bounds("flow-file-error").unwrap();
        let console = cx.debug_bounds("flow-results").unwrap();
        let command_bar = cx.debug_bounds("console-command-bar").unwrap();
        assert!(
            command_bar.bottom() <= console.bottom(),
            "command hints are clipped by the console: {command_bar:?}, {console:?}"
        );
        assert!(
            error.size.height <= px(96.),
            "error consumes editor: {error:?}"
        );
        for id in ["flow-toolbar", "console-command"] {
            let bounds = cx.debug_bounds(id).unwrap();
            assert!(
                bounds.top() >= px(0.) && bounds.bottom() <= px(height),
                "{id}: {bounds:?}"
            );
        }
        let text = cx.debug_bounds("flow-file-error-text").unwrap();
        let command = cx.debug_bounds("console-command").unwrap();
        cx.simulate_event(ScrollWheelEvent {
            position: error.center(),
            delta: ScrollDelta::Pixels(point(px(0.), px(-160.))),
            modifiers: Modifiers::none(),
            touch_phase: TouchPhase::Moved,
        });
        settle(cx);
        assert!(cx.debug_bounds("flow-file-error-text").unwrap().top() < text.top());
        assert_eq!(cx.debug_bounds("console-command").unwrap(), command);
    }
    press(cx, "secondary-w");
    click(cx, "flow-close-discard");
    settle(cx);
    choose_open(cx, &broken);
    let canvas = cx.debug_bounds("flow-canvas").unwrap();
    cx.simulate_event(ScrollWheelEvent {
        position: canvas.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(-400.))),
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
    });
    settle(cx);
    let create = cx.update(|w, _| w.find("flow-empty-create").bounds());
    assert!(
        canvas.contains(&create.center()),
        "empty-canvas action must be reachable by scrolling"
    );
    click(cx, "flow-empty-create");
    settle(cx);
    assert!(cx.update(|w, _| w.find("flow-create-submit").bounds().size.height) > px(0.));
}

#[gpui_kit::test]
fn compact_flow_actions_share_one_row_and_environment_settings_remain_accessible(
    cx: &mut TestAppContext,
) {
    use gpui::{px, size};
    let (_, cx) = setup(cx);
    click(cx, "nav-flows");
    create(cx, "Compact workflow");
    let handle = cx.update(|w, _| w.window_handle());
    cx.simulate_window_resize(handle, size(px(768.), px(600.)));
    settle(cx);
    let open = cx.debug_bounds("flow-open").unwrap();
    let run = cx.debug_bounds("flow-run").unwrap();
    assert_eq!(
        open.center().y,
        run.center().y,
        "file and execution actions wrap in the minimum viewport"
    );
    click(cx, "environment-settings");
    settle(cx);
    assert!(cx.debug_bounds("environment-dialog").is_some());
    press(cx, "escape");
    settle(cx);
    assert!(cx.debug_bounds("environment-dialog").is_none());
    click(cx, "flow-open");
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|_| None);
    settle(cx);
}

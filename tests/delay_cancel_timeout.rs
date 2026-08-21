#[path = "common/ui.rs"]
mod ui;

use gpui::{AppContext, TestAppContext};
use postman_gpui::app::{PostmanApp, ResponseState, WorkspaceViewModel};
use std::{
    sync::{mpsc, Arc, Condvar, Mutex},
    time::Duration,
};
use ui::{click, click_without_wait, type_into};

fn release(gate: &Arc<(Mutex<bool>, Condvar)>) {
    let (released, wake) = &**gate;
    *released
        .lock()
        .expect("response gate should not be poisoned") = true;
    wake.notify_all();
}

#[gpui::test]
fn rendered_controls_keep_complete_cancel_and_timeout_in_one_real_lifecycle(
    test_cx: &mut TestAppContext,
) {
    let mut server = mockito::Server::new();
    let completed = server
        .mock("GET", "/delay/complete")
        .with_status(200)
        .with_body("completed")
        .create();

    let (cancel_started_tx, cancel_started_rx) = mpsc::channel();
    let cancel_gate = Arc::new((Mutex::new(false), Condvar::new()));
    let cancel_response_gate = cancel_gate.clone();
    let cancelled = server
        .mock("GET", "/delay/cancel")
        .with_chunked_body(move |writer| {
            writer.write_all(b"started")?;
            let _ = cancel_started_tx.send(());
            let (released, wake) = &*cancel_response_gate;
            let released = released
                .lock()
                .expect("response gate should not be poisoned");
            let _ = wake
                .wait_timeout_while(released, Duration::from_secs(2), |released| !*released)
                .expect("response gate should remain available");
            Ok(())
        })
        .create();

    let timeout_gate = Arc::new((Mutex::new(false), Condvar::new()));
    let timeout_response_gate = timeout_gate.clone();
    let timed_out = server
        .mock("GET", "/delay/timeout")
        .with_chunked_body(move |writer| {
            writer.write_all(b"started")?;
            let (released, wake) = &*timeout_response_gate;
            let released = released
                .lock()
                .expect("response gate should not be poisoned");
            let _ = wake
                .wait_timeout_while(released, Duration::from_secs(2), |released| !*released)
                .expect("response gate should remain available");
            Ok(())
        })
        .create();

    let workspace = test_cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        test_cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    type_into(cx, "url-input", &format!("{}/delay/complete", server.url()))
        .expect("completion URL should be editable");
    click(cx, "send-button").expect("completion should send through the rendered control");
    cx.run_until_parked();
    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Success { status: 200, .. }
    ));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.history_len()),
        1
    );
    completed.assert();

    click(cx, "new-tab-button").expect("cancellation should use a fresh request draft");
    type_into(cx, "url-input", &format!("{}/delay/cancel", server.url()))
        .expect("cancellation URL should be editable");
    click_without_wait(cx, "send-button").expect("delayed request should start");
    cancel_started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("the transport should be active before Cancel");
    let (request_id, in_flight, response) = workspace.read_with(cx, |workspace, _| {
        (
            workspace.active_request_id(),
            workspace.in_flight_count(),
            workspace.response().clone(),
        )
    });
    assert_eq!(request_id.as_deref(), Some("req-02"));
    assert_eq!(in_flight, 1);
    assert!(matches!(response, ResponseState::Loading));
    for _ in 0..32 {
        if cx.debug_bounds("cancel-send-control").is_some() {
            break;
        }
        if !cx.executor().tick() {
            break;
        }
    }
    for selector in [
        "request-in-flight-id",
        "cancel-send-control",
        "response-loading",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "`{selector}` should expose the active cancellation contract"
        );
    }
    click_without_wait(cx, "send-button").expect("the live Send control should become Cancel");
    release(&cancel_gate);
    cx.run_until_parked();
    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Cancelled
    ));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| (
            workspace.active_request_id(),
            workspace.in_flight_count(),
            workspace.history_len(),
        )),
        (None, 0, 1)
    );
    assert!(cx.debug_bounds("response-cancelled").is_some());
    cancelled.assert();

    click(cx, "new-tab-button").expect("timeout should use a fresh request draft");
    type_into(cx, "url-input", &format!("{}/delay/timeout", server.url()))
        .expect("timeout URL should be editable");
    click(cx, "request-pane-options").expect("Options should be rendered");
    type_into(cx, "request-timeout-input", "50")
        .expect("the per-request timeout should be directly editable");
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.timeout_ms()),
        50
    );
    assert!(cx.debug_bounds("request-timeout-enabled").is_some());
    click(cx, "send-button").expect("timed request should start");
    cx.run_until_parked();
    release(&timeout_gate);
    cx.run_until_parked();

    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Error { message } if message == "Request timed out after 50 ms"
    ));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| (
            workspace.active_request_id(),
            workspace.in_flight_count(),
            workspace.history_len(),
        )),
        (None, 0, 1)
    );
    for selector in ["response-timeout-error", "response-timeout-content"] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "`{selector}` should expose timeout separately from user cancellation"
        );
    }
    timed_out.assert();
}

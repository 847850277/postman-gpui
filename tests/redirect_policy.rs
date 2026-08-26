#[path = "common/ui.rs"]
mod ui;

use gpui::{AppContext, TestAppContext};
use postman_gpui::{
    app::{PostmanApp, ResponseState, WorkspaceViewModel},
    models::{RedirectHop, RedirectPolicy},
};
use ui::{click, replace_text, type_into};

#[gpui::test]
fn rendered_redirect_controls_cover_follow_no_follow_and_limit_in_one_lifecycle(
    test_cx: &mut TestAppContext,
) {
    let mut server = mockito::Server::new();
    let relative_three = server
        .mock("GET", "/relative-redirect/3")
        .with_status(302)
        .with_header("location", "/relative-redirect/2")
        .create();
    let relative_two = server
        .mock("GET", "/relative-redirect/2")
        .with_status(302)
        .with_header("location", "/relative-redirect/1")
        .create();
    let relative_one = server
        .mock("GET", "/relative-redirect/1")
        .with_status(302)
        .with_header("location", "/get")
        .create();
    let terminal = server
        .mock("GET", "/get")
        .with_status(200)
        .with_body("terminal")
        .create();
    let no_follow = server
        .mock("GET", "/redirect-to")
        .with_status(302)
        .with_header("location", "/anything/stop")
        .with_body("first redirect")
        .create();
    let absolute_two_url = format!("{}/absolute-redirect/2", server.url());
    let absolute_one_url = format!("{}/absolute-redirect/1", server.url());
    let absolute_three = server
        .mock("GET", "/absolute-redirect/3")
        .with_status(302)
        .with_header("location", &absolute_two_url)
        .create();
    let absolute_two = server
        .mock("GET", "/absolute-redirect/2")
        .with_status(302)
        .with_header("location", &absolute_one_url)
        .create();

    let workspace = test_cx.new(|_| WorkspaceViewModel::new());
    let observed = workspace.clone();
    let (_app, cx) =
        test_cx.add_window_view(move |_window, cx| PostmanApp::with_view_model(observed, cx));

    type_into(
        cx,
        "url-input",
        &format!("{}/relative-redirect/3", server.url()),
    )
    .unwrap();
    click(cx, "request-pane-options").unwrap();
    for selector in [
        "redirect-configuration",
        "redirect-policy-follow",
        "redirect-policy-do-not-follow",
        "redirect-max-hops-input",
        "redirect-max-hops-decrease",
        "redirect-max-hops-increase",
        "effective-redirect-preview",
        "redirect-options-contract",
        "redirect-policy-follow-active",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "redirect control `{selector}` should be rendered"
        );
    }
    assert_eq!(
        workspace.read_with(cx, |workspace, _| (
            workspace.redirect_policy(),
            workspace.max_redirect_hops(),
        )),
        (RedirectPolicy::Follow, 10)
    );
    click(cx, "redirect-max-hops-decrease").unwrap();
    click(cx, "redirect-max-hops-increase").unwrap();
    replace_text(cx, "redirect-max-hops-input", "101").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.max_redirect_hops()),
        10,
        "values outside 1..=100 must be rejected"
    );
    replace_text(cx, "redirect-max-hops-input", "5").unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    let relative_start = format!("{}/relative-redirect/3", server.url());
    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Success { status: 200, body, .. } if body == "terminal"
    ));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.redirect_chain().to_vec()),
        vec![
            RedirectHop::new(302, &relative_start, Some("/relative-redirect/2")),
            RedirectHop::new(
                302,
                format!("{}/relative-redirect/2", server.url()),
                Some("/relative-redirect/1"),
            ),
            RedirectHop::new(
                302,
                format!("{}/relative-redirect/1", server.url()),
                Some("/get"),
            ),
            RedirectHop::terminal(200, format!("{}/get", server.url())),
        ]
    );
    for selector in [
        "response-redirect-count",
        "redirect-chain",
        "redirect-chain-count",
        "redirect-hop-0",
        "redirect-hop-status-0",
        "redirect-hop-location-0",
        "redirect-hop-3",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "followed-chain element `{selector}` should be rendered"
        );
    }
    assert_eq!(
        workspace.read_with(cx, |workspace, _| {
            let entry = &workspace.history()[0];
            (
                entry.request.url.clone(),
                entry.request_options.redirect_policy,
                entry.request_options.max_redirect_hops,
            )
        }),
        (relative_start, RedirectPolicy::Follow, 5)
    );

    click(cx, "new-tab-button").unwrap();
    type_into(cx, "url-input", &format!("{}/redirect-to", server.url())).unwrap();
    click(cx, "request-pane-options").unwrap();
    click(cx, "redirect-policy-do-not-follow").unwrap();
    assert!(cx
        .debug_bounds("redirect-policy-do-not-follow-active")
        .is_some());
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Success { status: 302, headers, body, .. }
            if body == "first redirect"
                && headers.iter().any(|(name, value)| {
                    name.eq_ignore_ascii_case("location") && value == "/anything/stop"
                })
    ));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.redirect_chain().len()),
        1
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.history()[0]
            .request_options
            .redirect_policy),
        RedirectPolicy::DoNotFollow
    );

    click(cx, "new-tab-button").unwrap();
    type_into(
        cx,
        "url-input",
        &format!("{}/absolute-redirect/3", server.url()),
    )
    .unwrap();
    click(cx, "request-pane-options").unwrap();
    replace_text(cx, "redirect-max-hops-input", "2").unwrap();
    click(cx, "send-button").unwrap();
    cx.run_until_parked();

    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.response().clone()),
        ResponseState::Error { message }
            if message == "Redirect limit exceeded after 2 hops."
    ));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.redirect_chain().to_vec()),
        vec![
            RedirectHop::new(
                302,
                format!("{}/absolute-redirect/3", server.url()),
                Some(&absolute_two_url),
            ),
            RedirectHop::new(
                302,
                format!("{}/absolute-redirect/2", server.url()),
                Some(&absolute_one_url),
            ),
        ]
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.history_len()),
        2,
        "redirect-limit failures must not fabricate History"
    );
    for selector in [
        "response-transport-error",
        "redirect-chain",
        "redirect-chain-partial",
        "redirect-hop-1",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "limit-failure element `{selector}` should be rendered"
        );
    }

    relative_three.assert();
    relative_two.assert();
    relative_one.assert();
    terminal.assert();
    no_follow.assert();
    absolute_three.assert();
    absolute_two.assert();
}

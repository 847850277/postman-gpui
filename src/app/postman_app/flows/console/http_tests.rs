use super::super::execution::{results::HttpPart, Status};
use super::super::tests::{click, press, settle, setup};
use super::*;
use gpui::{TestAppContext, VisualTestContext};
use gpui_kit::test::TestWindowExt;
use postman_flow::{parse_flow_yaml, BodyTemplate, HttpRequestSource, JsonTemplate};
use serde_json::{json, Value};

fn reveal(console: &Entity<FlowConsole>, id: u64, cx: &mut VisualTestContext) {
    console.update(cx, |v, cx| {
        let index = v.visible.iter().position(|entry| *entry == id).unwrap();
        v.scroll.scroll_to(gpui::ListOffset {
            item_ix: index,
            offset_in_item: gpui::px(0.),
        });
        cx.notify();
    });
    settle(cx);
}

#[gpui_kit::test]
fn http_log_drilldown_is_scoped_readonly_redacted_and_stale_links_are_disabled(
    cx: &mut TestAppContext,
) {
    let mut server = mockito::Server::new();
    let mocks: Vec<_> = [1, 2]
        .into_iter()
        .map(|page| {
            server
                .mock("POST", "/inspect")
                .match_body(mockito::Matcher::Json(
                    json!({"page":page,"password":"request-SYNTH"}),
                ))
                .with_status(503)
                .with_header("set-cookie", "sid=response-SYNTH; Path=/")
                .with_body(
                    json!({"page":page,"token":"response-SYNTH","echo":"request-SYNTH"})
                        .to_string(),
                )
                .expect(2)
                .create()
        })
        .collect();
    let (app, cx) = setup(cx);
    let flows = app.read_with(cx, |v, _| v.flows.clone());
    let console = flows.read_with(cx, |v, _| v.console.clone());
    click(cx, "nav-flows");
    let mut document = parse_flow_yaml(&format!(
        r#"
schema_version: 1
flow:
  name: HTTP details
  steps:
    - id: pages
      kind: for_each
      items: {{literal: [1, 2]}}
      as: page
      on_error: continue
      steps:
        - id: fetch
          request: {{kind: http, method: POST, url: {{literal: '{}/inspect'}}}}
          checks: [{{kind: status, equals: 200}}]
"#,
        server.url()
    ))
    .unwrap();
    let HttpRequestSource::ForEach(pages) = &mut document.flow.steps[0].request else {
        unreachable!()
    };
    pages.steps[0].request.as_inline_mut().unwrap().body =
        BodyTemplate::JsonValue(JsonTemplate::object([
            ("page", JsonTemplate::input("page")),
            ("password", JsonTemplate::literal("request-SYNTH")),
        ]));
    cx.update(|w, cx| {
        flows.update(cx, |v, cx| {
            v.library.insert(document);
            v.sync_fields(w, cx);
            v.sync_picker(w, cx);
            cx.notify();
        })
    });
    settle(cx);
    click(cx, "flow-run");
    settle(cx);
    click(cx, "console-maximize");
    settle(cx);
    let links = console.read_with(cx, |v, _| {
        let s = v.current().unwrap();
        assert_eq!(s.run.as_ref().unwrap().status, Status::Succeeded);
        assert!(!s.copy_text().contains("SYNTH"));
        s.logs
            .iter()
            .filter_map(|entry| entry.http.map(|t| (entry.id, t)))
            .collect::<Vec<_>>()
    });
    assert_eq!(links.len(), 4);
    assert_ne!(links[0].1.node, links[2].1.node);
    for theme in [
        gpui_kit::component::ThemeMode::Light,
        gpui_kit::component::ThemeMode::Dark,
    ] {
        cx.update(|_, cx| crate::ui::theme::apply(theme, cx));
        for (ix, (entry, target)) in links.iter().enumerate() {
            click(cx, "console-tab-logs");
            reveal(&console, *entry, cx);
            let flow = console.read_with(cx, |v, _| v.active.unwrap());
            cx.update(|w, _| {
                let detail = w.find(format!("console-http-{entry}")).bounds();
                let step = w.find(format!("console-step-{}-{entry}", flow.0)).bounds();
                assert_eq!(detail.top(), step.top());
                assert!(detail.right() < step.left());
            });
            cx.update(|w, cx| w.click(format!("console-http-{entry}"), cx));
            settle(cx);
            console.read_with(cx, |v, _| {
                assert_eq!(v.current().unwrap().view, View::Steps);
            });
            press(cx, "secondary-a");
            press(cx, "secondary-c");
            let before = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
            assert!(!before.contains("SYNTH"));
            let value: Value = serde_json::from_str(&before).unwrap();
            assert_eq!(value["body"]["value"]["page"], ix / 2 + 1);
            if target.part == HttpPart::Response {
                assert_eq!(value["status"], 503);
            }
            // Selection/copy remain enabled while deletion/paste are rejected.
            press(cx, "backspace");
            cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("modified".into())));
            press(cx, "secondary-v");
            press(cx, "secondary-a");
            press(cx, "secondary-c");
            assert_eq!(
                cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap()),
                before
            );
            click(cx, "result-copy-value");
            assert_eq!(
                cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap()),
                before
            );
            press(cx, "escape");
            settle(cx);
            cx.update(|w, _| {
                assert!(w
                    .find("result-row-0")
                    .label()
                    .unwrap()
                    .contains("Actual 503"))
            });
        }
    }
    click(cx, "flow-run");
    settle(cx);
    reveal(&console, links[0].0, cx);
    console.read_with(cx, |v, _| {
        let session = v.current().unwrap();
        assert_ne!(session.run.as_ref().unwrap().id, links[0].1.run);
        assert_eq!(session.view, View::Logs);
    });
    cx.update(|w, cx| {
        let id = format!("console-http-{}", links[0].0);
        assert!(w.find(id.clone()).visible());
        assert_eq!(
            w.find(id.clone()).label(),
            Some("Request unavailable · Earlier run")
        );
        // Kit 0.7.3 does not expose Button's disabled accessibility flag;
        // prove the actual pointer behavior and the reason announced instead.
        w.click(id, cx);
    });
    console.read_with(cx, |v, _| assert_eq!(v.current().unwrap().view, View::Logs));
    for mock in mocks {
        mock.assert();
    }
}

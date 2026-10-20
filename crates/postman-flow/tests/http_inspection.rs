mod support;
use futures::StreamExt;
use postman_flow::*;
use postman_http::{request::HttpMethod, HttpError, HttpResponse};
use serde_json::json;
use support::{run, FakeTransport};

fn definition() -> FlowDefinition {
    let mut request = HttpRequestTemplate::new(HttpMethod::POST, TextTemplate::literal(
        "https://user:p%40ss@example.test/orders?q=one&q=two&access_token=query-SYNTH#fragment-SYNTH",
    ));
    request.headers = vec![
        (
            TextTemplate::literal("Authorization"),
            TextTemplate::literal("Basic auth-SYNTH=="),
        ),
        (
            TextTemplate::literal("X-Label"),
            TextTemplate::literal("first"),
        ),
        (
            TextTemplate::literal("X-Label"),
            TextTemplate::literal("second"),
        ),
    ];
    request.body = BodyTemplate::JsonValue(JsonTemplate::literal(
        json!({"count": 2, "password": "body-SYNTH"}),
    ));
    let mut flow = FlowDefinition::new("HTTP inspection");
    flow.steps.push(
        HttpStepDefinition::new("fetch", "Fetch", request)
            .check(ResponseCheck::StatusEquals(200))
            .export(ResponseExport::json("private", "$.opaque").sensitive()),
    );
    flow
}
fn plan(flow: &FlowDefinition) -> FlowPlan {
    compile_flow(flow, &ApiCatalog::new(), &CompileEnvironment::default()).unwrap()
}
fn inspections(events: &[FlowEvent]) -> Vec<&InspectionValue> {
    events
        .iter()
        .filter_map(|event| match event {
            FlowEvent::RequestPrepared { inspection, .. }
            | FlowEvent::ResponseReceived { inspection, .. } => Some(inspection),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn resolved_parameters_preserve_duplicates_and_redact_both_sides_even_when_checks_fail() {
    for status in [200, 503] {
        let payload = json!({"opaque":"export-SYNTH", "echo":"auth-SYNTH== query-SYNTH body-SYNTH cookie-SYNTH", "safe":[0,false,null]});
        let transport = FakeTransport::new([Ok(HttpResponse::new(
            status,
            vec![
                ("Content-Type".into(), "application/json".into()),
                ("Set-Cookie".into(), "sid=cookie-SYNTH; Path=/".into()),
                ("X-Label".into(), "first".into()),
                ("X-Label".into(), "second".into()),
            ],
            payload.to_string(),
        )
        .with_elapsed_ms(12))]);
        let events = run(plan(&definition()), FlowInputs::new(), transport.clone()).await;
        let snapshots = inspections(&events);
        assert_eq!(snapshots.len(), 2);
        for value in &snapshots {
            assert!(!value.is_truncated());
            assert!(!value.json().unwrap().contains("SYNTH"));
            assert!(!format!("{value:?}").contains("SYNTH"));
            assert!(!value.redacted_paths().is_empty());
        }
        let req = snapshots[0].value().unwrap();
        assert_eq!(req["method"], "POST");
        assert_eq!(req["url"], "https://example.test/orders");
        assert_eq!(
            req["query"],
            json!([{"q":"one"},{"q":"two"},{"access_token":"[REDACTED]"}])
        );
        assert_eq!(req["headers"][0]["Authorization"], "[REDACTED]");
        assert_eq!(req["headers"][2]["X-Label"], "second");
        assert_eq!(
            req["body"]["value"],
            json!({"count":2,"password":"[REDACTED]"})
        );
        let resp = snapshots[1].value().unwrap();
        assert_eq!(resp["status"], status);
        assert_eq!(resp["elapsed_ms"], 12);
        assert_eq!(resp["body"]["value"]["opaque"], "[REDACTED]");
        assert_eq!(resp["body"]["value"]["safe"], json!([0, false, null]));
        assert_eq!(resp["headers"][3]["X-Label"], "second");
        assert!(transport.requests()[0].0.url.contains("query-SYNTH"));
        assert!(transport.requests()[0]
            .0
            .body
            .as_text()
            .unwrap()
            .contains("body-SYNTH"));
        assert!(
            matches!(events.last(), Some(FlowEvent::FlowFinished {success, ..}) if *success == (status == 200))
        );
    }
}

#[tokio::test]
async fn request_is_yielded_before_transport_and_network_errors_have_no_fake_response() {
    let transport = FakeTransport::new([]);
    let mut stream = Box::pin(
        execute_flow(
            plan(&definition()),
            transport.clone(),
            FlowSessionEnvironment::default(),
        )
        .unwrap(),
    );
    while let Some(event) = stream.next().await {
        if let FlowEvent::RequestPrepared { inspection, .. } = event.unwrap() {
            assert_eq!(inspection.value().unwrap()["method"], "POST");
            break;
        }
    }
    assert!(transport.requests().is_empty());
    drop(stream);
    assert!(transport.requests().is_empty());
    let events = run(
        plan(&definition()),
        FlowInputs::new(),
        FakeTransport::new([Err(HttpError::Timeout { timeout_ms: 10 })]),
    )
    .await;
    assert_eq!(inspections(&events).len(), 1);
    assert!(matches!(
        events.last(),
        Some(FlowEvent::FlowFinished { success: false, .. })
    ));
}

#[tokio::test]
async fn large_binary_and_plain_text_bodies_keep_truthful_metadata() {
    for (response, reason) in [
        (
            HttpResponse::new(200, vec![], "large".repeat(2000)),
            Some("Body size limit exceeded"),
        ),
        (
            HttpResponse::from_bytes(200, vec![], vec![0, 255, 1], None),
            Some("Binary contents"),
        ),
        (HttpResponse::new(200, vec![], "plain reply".into()), None),
        (HttpResponse::new(204, vec![], String::new()), None),
    ] {
        let expected_bytes = response.body_bytes().len();
        let events = run(
            plan(&definition()),
            FlowInputs::new(),
            FakeTransport::new([Ok(response)]),
        )
        .await;
        let snapshots = inspections(&events);
        let resp = snapshots[1].value().unwrap();
        assert_eq!(resp["body"]["bytes"], expected_bytes);
        if let Some(reason) = reason {
            assert!(snapshots[1].json().unwrap().contains(reason));
        } else {
            assert!(resp["body"]["value"].is_string());
        }
    }
}

#[tokio::test]
async fn forms_keep_safe_fields_and_file_bodies_do_not_load_or_expose_paths() {
    let mut flow = definition();
    flow.steps[0].checks.clear();
    flow.steps[0].exports.clear();
    for body in [
        BodyTemplate::UrlEncoded(TextTemplate::literal("q=one&q=two&password=form-SYNTH")),
        BodyTemplate::File(TextTemplate::literal("/does/not/exist/private-SYNTH")),
    ] {
        let HttpRequestSource::Inline(request) = &mut flow.steps[0].request else {
            unreachable!()
        };
        request.body = body;
        let events = run(
            plan(&flow),
            FlowInputs::new(),
            FakeTransport::new([Ok(HttpResponse::new(
                200,
                vec![(
                    "Content-Type".into(),
                    "application/x-www-form-urlencoded; charset=utf-8".into(),
                )],
                "q=one&q=two&token=resp-SYNTH".into(),
            ))]),
        )
        .await;
        for value in inspections(&events) {
            assert!(!value.json().unwrap().contains("SYNTH"));
        }
        assert_eq!(
            inspections(&events)[1].value().unwrap()["body"]["value"],
            json!([{"q":"one"},{"q":"two"},{"token":"[REDACTED]"}])
        );
    }
}

#[tokio::test]
async fn custom_signatures_relative_locations_and_raw_form_credentials_are_redacted() {
    for body in [
        BodyTemplate::None,
        BodyTemplate::UrlEncoded(TextTemplate::literal("q=one")),
    ] {
        let mut flow = definition();
        let request = flow.steps[0].request.as_inline_mut().unwrap();
        request.body = body;
        request.auth = Some(AuthTemplate::HmacSha256 {
            secret: TextTemplate::literal("hmac-SYNTH"),
            param: "custom_sig".into(),
        });
        let transport = FakeTransport::new([Ok(HttpResponse::new(
            302,
            vec![("Location".into(), "/next?token=location-SYNTH".into())],
            json!({"echo":"location-SYNTH"}).to_string(),
        ))]);
        let events = run(plan(&flow), FlowInputs::new(), transport.clone()).await;
        let snapshots = inspections(&events);
        assert!(snapshots[0].json().unwrap().contains("custom_sig"));
        let sent = &transport.requests()[0].0;
        let url = url::Url::parse(&sent.url).unwrap();
        let signed = sent.body.as_text().unwrap_or_else(|| url.query().unwrap());
        let signature = url::form_urlencoded::parse(signed.as_bytes())
            .find(|(k, _)| k == "custom_sig")
            .unwrap()
            .1;
        for value in snapshots {
            let text = value.json().unwrap();
            assert!(!text.contains(signature.as_ref()));
            assert!(!text.contains("SYNTH"));
        }
    }
    let mut flow = definition();
    let request = flow.steps[0].request.as_inline_mut().unwrap();
    request.body = BodyTemplate::Raw(TextTemplate::literal("q=one&password=raw-SYNTH"));
    request.headers.push((
        TextTemplate::literal("Content-Type"),
        TextTemplate::literal("application/x-www-form-urlencoded"),
    ));
    let events = run(
        plan(&flow),
        FlowInputs::new(),
        FakeTransport::new([Ok(HttpResponse::new(200, vec![], "raw-SYNTH".into()))]),
    )
    .await;
    for value in inspections(&events) {
        assert!(!value.json().unwrap().contains("SYNTH"));
    }
    assert_eq!(
        inspections(&events)[0].value().unwrap()["body"]["value"][0],
        json!({"q":"one"})
    );
}

use futures::StreamExt;
use postman_flow::*;
use postman_http::{
    error::HttpError,
    request::{HttpMethod, Request, RequestBody},
    response::HttpResponse,
    HttpTransport,
};
use serde_json::json;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct RecordingTransport {
    requests: Arc<Mutex<Vec<Request>>>,
}

impl HttpTransport for RecordingTransport {
    fn execute(
        &self,
        request: Request,
        _options: postman_http::request::RequestOptions,
    ) -> impl std::future::Future<Output = Result<HttpResponse, HttpError>> + Send + '_ {
        self.requests.lock().unwrap().push(request.clone());
        let body = if request.url.contains("/price") {
            json!({"symbol": "UNIUSDT", "price": "8.5"}).to_string()
        } else if request.url.contains("/balance") {
            json!([{"asset": "USDT", "availableBalance": "100.0"}]).to_string()
        } else {
            json!({"orderId": 123456, "status": "FILLED"}).to_string()
        };
        std::future::ready(Ok(HttpResponse::new(200, vec![], body)))
    }
}

#[tokio::test]
async fn test_builtins_calc_and_hmac_auth_flow() {
    let yaml = r#"
schema_version: 1
flow:
  name: advanced-features-flow
  inputs:
    - name: api_secret
      default: my_secret_key_123
  steps:
    # 1. 验证内置动态变量 $uuid 和 $timestamp_ms
    - id: step-1-time-and-uuid
      request:
        kind: http
        method: GET
        url:
          concat:
            - literal: "https://example.com/ping?trace_id="
            - input: $uuid
            - literal: "&ts="
            - input: $timestamp_ms
      checks:
        - kind: status
          equals: 200

    # 2. 查询余额与价格
    - id: step-2-balance
      request:
        kind: http
        method: GET
        url: { literal: "https://example.com/balance" }
      exports:
        - name: usdt_avail
          path: "$[0].availableBalance"

    - id: step-3-price
      request:
        kind: http
        method: GET
        url: { literal: "https://example.com/price" }
      exports:
        - name: uni_price
          path: "$.price"

    # 3. 动态 calc 计算下单数量：(100 * 0.9 * 3) / 8.5 = 31.76 -> floor -> 31
    # 并且使用 auth: hmac_sha256 自动计算签名追加到 Query！
    - id: step-4-calc-and-hmac-order
      request:
        kind: http
        method: POST
        url:
          concat:
            - literal: "https://example.com/order?symbol=UNIUSDT&quantity="
            - calc: "floor((step-2-balance.usdt_avail * 0.9 * 3) / step-3-price.uni_price)"
            - literal: "&timestamp="
            - input: $timestamp_ms
        auth:
          type: hmac_sha256
          secret: { input: api_secret }
      checks:
        - kind: status
          equals: 200
"#;

    let doc = parse_flow_yaml(yaml).expect("valid yaml");
    let plan = compile_flow(
        &doc.flow,
        &ApiCatalog::new(),
        &CompileEnvironment::default(),
    )
    .expect("compiled");
    let transport = RecordingTransport::default();
    let stream =
        execute_flow(plan, transport.clone(), FlowSessionEnvironment::default()).expect("started");
    let events: Vec<_> = stream.collect().await;
    assert!(matches!(
        events.last(),
        Some(Ok(FlowEvent::FlowFinished { success: true, .. }))
    ));

    let reqs = transport.requests.lock().unwrap();
    assert_eq!(reqs.len(), 4);

    // 验证 step 1: trace_id 包含 36 位 uuid, ts 包含 13 位毫秒时间戳
    let url = url::Url::parse(&reqs[0].url).unwrap();
    let fields: std::collections::BTreeMap<_, _> = url.query_pairs().collect();
    assert_eq!(
        uuid::Uuid::parse_str(&fields["trace_id"])
            .unwrap()
            .get_version_num(),
        4
    );
    assert!(fields["ts"].parse::<u64>().unwrap() > 1_000_000_000_000);

    // 验证 step 4: quantity 正确由 calc 算出 31，且自动追加了 signature=...
    let order_url = &reqs[3].url;
    assert!(order_url.contains("quantity=31"));
    let query = url::Url::parse(order_url)
        .unwrap()
        .query()
        .unwrap()
        .to_owned();
    let (payload, signature) = query.rsplit_once("&signature=").unwrap();
    use hmac::{Hmac, Mac};
    let mut verifier = Hmac::<sha2::Sha256>::new_from_slice(b"my_secret_key_123").unwrap();
    verifier.update(payload.as_bytes());
    verifier
        .verify_slice(&hex::decode(signature).unwrap())
        .unwrap();
}

async fn run_document(yaml: &str) -> (Vec<Request>, Vec<FlowEvent>) {
    let document = parse_flow_yaml(yaml).unwrap();
    assert_eq!(
        parse_flow_yaml(&write_flow_yaml(&document).unwrap()).unwrap(),
        document
    );
    let plan = compile_flow(
        &document.flow,
        &document.apis,
        &CompileEnvironment::default(),
    )
    .unwrap();
    let transport = RecordingTransport::default();
    let events = execute_flow(plan, transport.clone(), FlowSessionEnvironment::default())
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>()
        .await;
    let requests = transport.requests.lock().unwrap().clone();
    (requests, events)
}

fn single_request(request: HttpRequestTemplate) -> FlowDefinition {
    let mut flow = FlowDefinition::new("regression");
    flow.steps
        .push(HttpStepDefinition::new("step", "step", request));
    flow
}

#[tokio::test]
async fn catalog_builtins_calc_and_auth_resolve_in_the_correct_scope() {
    let (requests, events) = run_document(
        r#"
schema_version: 1
apis:
  signed:
    parameters: [qty, key]
    request:
      method: POST
      url:
        concat:
          - literal: https://example.invalid/order?quantity=
          - calc: qty*2+1
          - literal: '&timestamp='
          - input: $timestamp_ms
      headers:
        - name: {literal: X-Trace}
          value: {input: $uuid}
      auth:
        type: hmac_sha256
        secret: {input: key}
  json:
    parameters: [qty]
    request:
      method: POST
      url: {literal: 'https://example.invalid/json'}
      body:
        kind: json
        value:
          object:
            raw: {input: qty}
            calculated: {calc: qty-1}
            timestamp: {input: $timestamp_ms}
            next_timestamp: {calc: $timestamp_ms+1}
flow:
  name: catalog-regression
  inputs:
    - {name: qty, default: 4}
    - {name: key, default: wrong-global-secret}
    - {name: $timestamp_ms, default: 1234}
  steps:
    - id: signed
      request:
        kind: api
        api: signed
        bindings:
          qty: {calc: qty-1}
          key: {concat: [{literal: test-}, {literal: secret}]}
    - id: json
      request:
        kind: api
        api: json
        bindings:
          qty: {input: qty}
"#,
    )
    .await;
    assert!(matches!(
        events.last(),
        Some(FlowEvent::FlowFinished { success: true, .. })
    ));
    assert_eq!(requests[0].url, "https://example.invalid/order?quantity=7&timestamp=1234&signature=097b91353747e9612b8ffdaf8d559e8290ed9f80098abc10d65bf15e84b92ee8");
    assert_eq!(
        uuid::Uuid::parse_str(&requests[0].headers[0].1)
            .unwrap()
            .get_version_num(),
        4
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(requests[1].body.as_text().unwrap()).unwrap(),
        json!({"raw":4, "calculated":3, "timestamp":1234, "next_timestamp":1235})
    );
}

#[tokio::test]
async fn catalog_bindings_stay_lazy_for_skipped_outputs_and_coalesce() {
    let (requests, events) = run_document(
        r#"
schema_version: 1
apis:
  fallback:
    parameters: [quantity]
    request:
      method: POST
      url: {literal: 'https://example.invalid/fallback'}
      body:
        kind: json
        value:
          coalesce:
            - calc: quantity*2
            - literal: 42
flow:
  name: lazy-arguments
  steps:
    - id: skipped
      when: {eq: [{literal: 1}, {literal: 2}]}
      request: {kind: http, method: GET, url: {literal: 'https://example.invalid/skip'}}
      exports: [{name: quantity, path: $.quantity}]
    - id: fallback
      request:
        kind: api
        api: fallback
        bindings:
          quantity: {output: {step: skipped, name: quantity}}
"#,
    )
    .await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].body, RequestBody::Json("42".into()));
    assert!(matches!(
        events.last(),
        Some(FlowEvent::FlowFinished { success: true, .. })
    ));
}

#[test]
fn compiler_rejects_calc_syntax_and_references_in_all_expression_positions() {
    for (expression, expected) in [
        ("(", DiagnosticCode::InvalidExpression),
        ("floor(", DiagnosticCode::InvalidExpression),
        ("unknown(1)", DiagnosticCode::InvalidExpression),
        ("missing+1", DiagnosticCode::UnknownInput),
        ("future.balance*2", DiagnosticCode::UnavailableOutput),
    ] {
        for position in 0..4 {
            let mut request = HttpRequestTemplate::new(
                HttpMethod::POST,
                TextTemplate::literal("https://example.invalid"),
            );
            if position == 0 {
                request.url = TextTemplate::parts([TemplatePart::Calc(expression.into())]);
            }
            if position == 1 {
                request.body = BodyTemplate::JsonValue(JsonTemplate::Calc(expression.into()));
            }
            let mut flow = single_request(request);
            if position == 2 {
                flow.steps[0].when = Some(ConditionExpr::gt(
                    JsonTemplate::Calc(expression.into()),
                    JsonTemplate::literal(0),
                ));
            }
            if position == 3 {
                flow.steps[0].checks.push(ResponseCheck::JsonValueEquals {
                    path: "$.value".into(),
                    expected: JsonTemplate::Calc(expression.into()),
                });
            }
            let errors = compile_flow(&flow, &ApiCatalog::new(), &CompileEnvironment::default())
                .unwrap_err();
            assert!(
                errors.iter().any(|error| error.code == expected),
                "{expression} at {position}: {errors:?}"
            );
        }
    }
}

#[test]
fn compiler_validates_auth_secret_parameter_and_supported_body() {
    let mut request = HttpRequestTemplate::new(
        HttpMethod::POST,
        TextTemplate::literal("https://example.invalid"),
    );
    request.auth = Some(AuthTemplate::HmacSha256 {
        secret: TextTemplate::parts([TemplatePart::input("missing")]),
        param: "bad&param".into(),
    });
    request.body = BodyTemplate::JsonValue(JsonTemplate::literal(json!({})));
    let errors = compile_flow(
        &single_request(request),
        &ApiCatalog::new(),
        &CompileEnvironment::default(),
    )
    .unwrap_err();
    for field in ["auth.secret", "auth.param", "auth"] {
        assert!(
            errors
                .iter()
                .any(|error| error.location.field.contains(field)),
            "{errors:?}"
        );
    }
    let invalid = r#"
schema_version: 1
flow:
  name: typo
  steps:
    - id: one
      request:
        kind: http
        method: GET
        url: {literal: 'https://example.invalid'}
        auth: {type: hmac_sha256, secret: {literal: test}, parma: sig}
"#;
    assert!(parse_flow_yaml(invalid).is_err());
}

#[test]
fn catalog_calc_and_auth_cannot_capture_unbound_flow_inputs() {
    let mut request = HttpRequestTemplate::new(
        HttpMethod::GET,
        TextTemplate::parts([TemplatePart::Calc("global*2".into())]),
    );
    request.auth = Some(AuthTemplate::HmacSha256 {
        secret: TextTemplate::parts([TemplatePart::input("global")]),
        param: "signature".into(),
    });
    let mut catalog = ApiCatalog::new();
    catalog.insert("api", ApiDefinition::new(request));
    let mut flow = FlowDefinition::new("scope");
    flow.inputs.push(FlowInputSpec::with_default("global", 3));
    flow.steps
        .push(HttpStepDefinition::new("one", "one", ApiCall::new("api")));
    let errors = compile_flow(&flow, &catalog, &CompileEnvironment::default()).unwrap_err();
    assert_eq!(
        errors
            .iter()
            .filter(|error| error.code == DiagnosticCode::UnknownInput)
            .count(),
        2
    );
    assert!(errors
        .iter()
        .all(|error| error.location.api_id.as_deref() == Some("api")));
}

#[tokio::test]
async fn nonfinite_calculations_fail_before_sending_any_request() {
    for input in ["NaN", "inf", "1e308"] {
        for json_body in [false, true] {
            let mut request = HttpRequestTemplate::new(
                HttpMethod::POST,
                TextTemplate::literal("https://example.invalid"),
            );
            if json_body {
                request.body = BodyTemplate::JsonValue(JsonTemplate::Calc("x*2".into()));
            } else {
                request.url = TextTemplate::parts([
                    TemplatePart::literal("https://example.invalid/?x="),
                    TemplatePart::Calc("x*2".into()),
                ]);
            }
            let mut flow = single_request(request);
            flow.inputs.push(FlowInputSpec::with_default("x", input));
            let plan =
                compile_flow(&flow, &ApiCatalog::new(), &CompileEnvironment::default()).unwrap();
            let transport = RecordingTransport::default();
            let events = execute_flow(plan, transport.clone(), FlowSessionEnvironment::default())
                .unwrap()
                .map(Result::unwrap)
                .collect::<Vec<_>>()
                .await;
            assert!(transport.requests.lock().unwrap().is_empty());
            assert!(matches!(
                events.last(),
                Some(FlowEvent::FlowFinished { success: false, .. })
            ));
        }
    }
}

fn signed_request(url: &str, body: Option<&str>, secret: &str) -> HttpRequestTemplate {
    let mut request = HttpRequestTemplate::new(HttpMethod::POST, TextTemplate::literal(url));
    request.auth = Some(AuthTemplate::HmacSha256 {
        secret: TextTemplate::literal(secret),
        param: "signature".into(),
    });
    if let Some(body) = body {
        request.body = BodyTemplate::UrlEncoded(TextTemplate::literal(body));
    }
    request
}

async fn record_signed(request: HttpRequestTemplate) -> (Vec<Request>, Vec<FlowEvent>) {
    let plan = compile_flow(
        &single_request(request),
        &ApiCatalog::new(),
        &CompileEnvironment::default(),
    )
    .unwrap();
    let transport = RecordingTransport::default();
    let events = execute_flow(plan, transport.clone(), FlowSessionEnvironment::default())
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>()
        .await;
    let requests = transport.requests.lock().unwrap().clone();
    (requests, events)
}

#[tokio::test]
async fn hmac_matches_fixed_query_body_and_mixed_vectors() {
    // Public Binance documentation example; these are not real account credentials.
    let secret = "2b5eb11e18796d12d88f13dc27dbbd02c2cc51ff7059765ed9821957d82bb4d9";
    let params = "symbol=BTCUSDT&side=BUY&type=LIMIT&quantity=1&price=9000&timeInForce=GTC&recvWindow=5000&timestamp=1591702613943";
    let signature = "3c661234138461fcc7a7d8746c6558c9842d4e10870d2ecbedf7777cad694af9";
    let (requests, _) = record_signed(signed_request(
        &format!("https://example.invalid/order?{params}"),
        None,
        secret,
    ))
    .await;
    assert_eq!(
        requests[0].url,
        format!("https://example.invalid/order?{params}&signature={signature}")
    );
    let (requests, _) = record_signed(signed_request(
        "https://example.invalid/order",
        Some(params),
        secret,
    ))
    .await;
    assert_eq!(
        requests[0].body,
        RequestBody::UrlEncoded(format!("{params}&signature={signature}"))
    );
    assert_eq!(requests[0].url, "https://example.invalid/order");
    let query = "symbol=BTCUSDT&side=BUY&type=LIMIT&timeInForce=GTC";
    let body = "quantity=1&price=9000&recvWindow=5000&timestamp=1591702613943";
    // Independently computed with OpenSSL over query + body, without an inserted '&'.
    let (requests, _) = record_signed(signed_request(
        &format!("https://example.invalid/order?{query}"),
        Some(body),
        secret,
    ))
    .await;
    assert_eq!(
        requests[0].url,
        format!("https://example.invalid/order?{query}")
    );
    assert_eq!(
        requests[0].body,
        RequestBody::UrlEncoded(format!(
            "{body}&signature=30baaf0fab549bbeda7f5ef201898b34122da25fd23c646cac2c529aebe670a4"
        ))
    );
    assert!(requests[0].headers.iter().any(
        |(name, value)| name == "Content-Type" && value == "application/x-www-form-urlencoded"
    ));
}

#[tokio::test]
async fn signing_uses_serialized_url_bytes_and_ignores_fragments() {
    let (requests, _) = record_signed(signed_request(
        "https://example.invalid/order?note=hello world&tag=中&x=%2f&x=%2F&plus=a+b#not-sent",
        None,
        "test-secret",
    ))
    .await;
    assert_eq!(requests[0].url, "https://example.invalid/order?note=hello%20world&tag=%E4%B8%AD&x=%2f&x=%2F&plus=a+b&signature=189bf15f2aa6754b4730ed9f774c04a01a065358e8409be8aa2efbc57f574707");
    let (requests, _) = record_signed(signed_request(
        "https://example.invalid/order#only-fragment",
        None,
        "test-secret",
    ))
    .await;
    let url = url::Url::parse(&requests[0].url).unwrap();
    assert!(url.fragment().is_none());
    assert_eq!(
        url.query_pairs()
            .filter(|(name, _)| name == "signature")
            .count(),
        1
    );
}

#[tokio::test]
async fn signing_rejects_duplicate_signatures_before_io() {
    for (url, body) in [
        ("https://example.invalid?signature=old", None),
        ("https://example.invalid?%73ignature=old", None),
        ("https://example.invalid", Some("signature=old")),
    ] {
        let (requests, events) = record_signed(signed_request(url, body, "test-secret")).await;
        assert!(requests.is_empty());
        assert!(matches!(
            events.last(),
            Some(FlowEvent::FlowFinished { success: false, .. })
        ));
    }
}

#[test]
fn literal_api_arguments_keep_static_request_validation() {
    for invalid_body in [false, true] {
        let mut request = HttpRequestTemplate::new(
            HttpMethod::POST,
            TextTemplate::literal("https://example.invalid"),
        );
        if invalid_body {
            request.body = BodyTemplate::Json(TextTemplate::parts([TemplatePart::input("value")]));
        } else {
            request.url = TextTemplate::parts([TemplatePart::input("value")]);
        }
        let mut catalog = ApiCatalog::new();
        catalog.insert("api", ApiDefinition::new(request).parameter("value"));
        let mut flow = FlowDefinition::new("literal-argument-validation");
        flow.steps.push(HttpStepDefinition::new(
            "one",
            "one",
            ApiCall::new("api").bind(
                "value",
                TextTemplate::literal(if invalid_body { "{bad-json}" } else { "" }),
            ),
        ));
        let errors = compile_flow(&flow, &catalog, &CompileEnvironment::default()).unwrap_err();
        assert!(errors
            .iter()
            .any(|error| error.code == DiagnosticCode::InvalidRequest));
    }
}

#[tokio::test]
async fn signatures_match_the_bytes_sent_by_the_real_http_transport() {
    use postman_request::RequestClient;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        time::{Duration, Instant},
    };

    for form in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            Instant::now() < deadline,
                            "no request reached the local server"
                        );
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("local accept failed: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut received = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0, "incomplete HTTP request");
                received.extend_from_slice(&buffer[..count]);
                if let Some(end) = received.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&received[..end]);
                    let length = headers
                        .lines()
                        .filter_map(|line| line.split_once(':'))
                        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                        .map(|(_, value)| value.trim().parse::<usize>().unwrap())
                        .unwrap_or(0);
                    if received.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
                .unwrap();
            String::from_utf8(received).unwrap()
        });
        let url =
            format!("http://{address}/order?note=hello world&tag=中&x=%2f&x=%2F&plus=a+b#ignored");
        let request = signed_request(&url, form.then_some("quantity=7"), "test-secret");
        let plan = compile_flow(
            &single_request(request),
            &ApiCatalog::new(),
            &CompileEnvironment::default(),
        )
        .unwrap();
        let events = execute_flow(
            plan,
            RequestClient::try_new("flow-regression").unwrap(),
            FlowSessionEnvironment::default(),
        )
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>()
        .await;
        assert!(matches!(
            events.last(),
            Some(FlowEvent::FlowFinished { success: true, .. })
        ));
        let wire = server.join().unwrap();
        let (headers, body) = wire.split_once("\r\n\r\n").unwrap();
        let target = headers
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap();
        let query = target.split_once('?').unwrap().1;
        assert!(!target.contains('#'));
        let (query, body, signature) = if form {
            let (body, signature) = body.rsplit_once("&signature=").unwrap();
            (query, body, signature)
        } else {
            let (query, signature) = query.rsplit_once("&signature=").unwrap();
            (query, body, signature)
        };
        assert_eq!(
            query,
            "note=hello%20world&tag=%E4%B8%AD&x=%2f&x=%2F&plus=a+b"
        );
        assert_eq!(body, if form { "quantity=7" } else { "" });
        use hmac::{Hmac, Mac};
        let mut verifier = Hmac::<sha2::Sha256>::new_from_slice(b"test-secret").unwrap();
        verifier.update(format!("{query}{body}").as_bytes());
        verifier
            .verify_slice(&hex::decode(signature).unwrap())
            .unwrap();
    }
}

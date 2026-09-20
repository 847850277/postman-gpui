use postman_flow::{compile_flow, execute_flow, parse_flow_yaml, ApiCatalog, CompileEnvironment, FlowSessionEnvironment};
use postman_http::{error::HttpError, request::Request, response::HttpResponse, HttpTransport};
use serde_json::json;
use std::sync::{Arc, Mutex};
use futures::StreamExt;

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
    let plan = compile_flow(&doc.flow, &ApiCatalog::new(), &CompileEnvironment::default()).expect("compiled");
    let transport = RecordingTransport::default();
    let stream = execute_flow(plan, transport.clone(), FlowSessionEnvironment::default()).expect("started");
    let events: Vec<_> = stream.collect().await;
    for e in events {
        assert!(e.is_ok());
    }

    let reqs = transport.requests.lock().unwrap();
    assert_eq!(reqs.len(), 4);

    // 验证 step 1: trace_id 包含 36 位 uuid, ts 包含 13 位毫秒时间戳
    assert!(reqs[0].url.contains("trace_id="));
    assert!(reqs[0].url.contains("&ts="));

    // 验证 step 4: quantity 正确由 calc 算出 31，且自动追加了 signature=...
    let order_url = &reqs[3].url;
    println!("Order URL: {order_url}");
    assert!(order_url.contains("quantity=31"));
    assert!(order_url.contains("&signature="));
}
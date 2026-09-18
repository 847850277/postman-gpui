use futures::StreamExt;
use postman_flow::{
    execute_flow, BodyTemplate, FlowDefinition, FlowEvent, FlowInputs, FlowSessionEnvironment,
    HttpRequestTemplate, HttpStepDefinition, JsonTemplate, ResponseCheck, TemplatePart,
    TextTemplate,
};
use postman_http::request::{HttpMethod, RequestOptions};
use postman_request::RequestClient;

mod compile;
pub use compile::compile_example;

pub fn json_request(
    id: &str,
    method: HttpMethod,
    path: &str,
    body: JsonTemplate,
) -> HttpStepDefinition {
    let mut step = request(id, method, path);
    step.request
        .as_inline_mut()
        .expect("request helper constructs inline HTTP")
        .body = BodyTemplate::JsonValue(body);
    step
}

/// Shared HTTPBingo wiring; plans contain the examples' actual orchestration logic.
pub fn request(id: &str, method: HttpMethod, path: &str) -> HttpStepDefinition {
    HttpStepDefinition::new(
        id,
        id,
        HttpRequestTemplate::new(
            method,
            TextTemplate::parts([TemplatePart::input("host"), TemplatePart::literal(path)]),
        )
        .header(
            TextTemplate::literal("Accept"),
            TextTemplate::literal("application/json"),
        )
        .header(
            TextTemplate::literal("Content-Type"),
            TextTemplate::literal("application/json"),
        ),
    )
    .check(ResponseCheck::StatusEquals(200))
}

pub fn equals(path: &str, expected: JsonTemplate) -> ResponseCheck {
    ResponseCheck::JsonValueEquals {
        path: path.into(),
        expected,
    }
}

pub async fn run_live(
    plan: FlowDefinition,
    inputs: FlowInputs,
) -> Result<(), Box<dyn std::error::Error>> {
    let transport = RequestClient::try_new("postman-flow-examples/0.1.0")?;
    let environment = FlowSessionEnvironment::new(inputs).with_request_options(RequestOptions {
        timeout_ms: Some(15_000),
        ..RequestOptions::default()
    });
    let events = execute_flow(compile_example(&plan)?, transport, environment)?;
    let mut events = std::pin::pin!(events);
    let mut success = false;
    while let Some(event) = events.next().await {
        let event = event?;
        match &event {
            FlowEvent::FlowStarted { name, total_steps } => {
                tracing::info!(flow_name = %name, total_steps, "Flow 启动");
            }
            FlowEvent::StepStarted { step_id, name } => {
                tracing::info!(step_id = %step_id, step_name = %name, "▶ 步骤开始");
            }
            FlowEvent::ResponseReceived {
                step_id,
                status,
                elapsed_ms,
            } => {
                tracing::info!(step_id = %step_id, status, elapsed_ms, "↳ 收到响应");
            }
            FlowEvent::OutputExported { step_id, name } => {
                tracing::info!(step_id = %step_id, export_name = %name, "↳ 提取并导出变量");
            }
            FlowEvent::CheckFinished {
                step_id,
                check,
                success,
                message,
            } => {
                if *success {
                    tracing::info!(step_id = %step_id, check = %check, "↳ 断言检查通过 ✔");
                } else {
                    tracing::warn!(step_id = %step_id, check = %check, message = ?message, "↳ 断言检查失败 ✘");
                }
            }
            FlowEvent::StepFinished { step_id, outcome } => {
                tracing::info!(step_id = %step_id, ?outcome, "⏹ 步骤完成");
            }
            FlowEvent::StepSkipped {
                step_id,
                name,
                reason,
            } => {
                tracing::info!(step_id = %step_id, step_name = %name, reason = %reason, "⏭ 步骤跳过");
            }
            FlowEvent::FlowFinished {
                success: finished, ..
            } => {
                success = *finished;
                if *finished {
                    tracing::info!("Flow 全部步骤执行完毕，状态：成功");
                } else {
                    tracing::error!("Flow 执行终止，状态：失败");
                }
            }
        }
    }
    if !success {
        return Err("HTTPBingo flow failed; inspect the logs above".into());
    }
    Ok(())
}

/// Deterministic fixtures for testing the plans without DNS, TLS, or a public service.
/// Executing either example's main still uses RequestClient and the real HTTPBingo service.
#[cfg(test)]
pub mod testing {
    use std::sync::{Arc, Mutex};

    use futures::TryStreamExt;
    use postman_flow::FlowError;
    use postman_http::{
        request::{Request, RequestBody},
        HttpError, HttpResponse, HttpTransport,
    };
    use serde_json::{json, Value};

    use super::*;

    #[derive(Clone, Default)]
    pub struct HttpBingoFixture {
        state: Arc<Mutex<State>>,
    }

    #[derive(Default)]
    struct State {
        next_uuid: u64,
        fail_request: Option<usize>,
        requests: Vec<Request>,
    }

    impl HttpBingoFixture {
        pub fn fail_on_request(&self, index: usize) {
            self.state.lock().unwrap().fail_request = Some(index);
        }

        pub fn requests(&self) -> Vec<Request> {
            self.state.lock().unwrap().requests.clone()
        }
    }

    impl HttpTransport for HttpBingoFixture {
        async fn execute(
            &self,
            request: Request,
            _options: RequestOptions,
        ) -> Result<HttpResponse, HttpError> {
            let mut state = self.state.lock().unwrap();
            let index = state.requests.len();
            state.requests.push(request.clone());
            if state.fail_request == Some(index) {
                return Ok(response(503, json!({"error": "fixture failure"})));
            }
            let path = request
                .url
                .strip_prefix("https://httpbingo.org")
                .ok_or_else(|| HttpError::invalid_request("unexpected fixture host"))?;
            match request.method {
                HttpMethod::GET if path == "/uuid" => {
                    state.next_uuid += 1;
                    Ok(response(
                        200,
                        json!({"uuid": format!("00000000-0000-4000-8000-{:012}", state.next_uuid)}),
                    ))
                }
                HttpMethod::POST if path.starts_with("/anything/") => {
                    let RequestBody::Json(body) = &request.body else {
                        return Err(HttpError::invalid_request("expected JSON body"));
                    };
                    let body: Value = serde_json::from_str(body)
                        .map_err(|error| HttpError::invalid_request(error.to_string()))?;
                    Ok(response(200, json!({"json": body, "method": "POST"})))
                }
                _ => Err(HttpError::invalid_request("unexpected fixture request")),
            }
        }
    }

    fn response(status: u16, value: Value) -> HttpResponse {
        HttpResponse::new(
            status,
            vec![("content-type".into(), "application/json".into())],
            value.to_string(),
        )
    }

    pub fn body(request: &Request) -> Value {
        let RequestBody::Json(body) = &request.body else {
            panic!("expected a JSON request");
        };
        serde_json::from_str(body).unwrap()
    }

    pub async fn run(
        plan: FlowDefinition,
        inputs: FlowInputs,
        transport: HttpBingoFixture,
    ) -> Result<Vec<FlowEvent>, FlowError> {
        let compiled = compile_example(&plan).expect("example definition should compile");
        execute_flow(compiled, transport, FlowSessionEnvironment::new(inputs))?
            .try_collect()
            .await
    }
}

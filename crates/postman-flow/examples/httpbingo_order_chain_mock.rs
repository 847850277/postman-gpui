#[path = "support/compile.rs"]
mod compile;
use futures::StreamExt;
use postman_flow::{
    execute_flow, FlowDefinition, FlowEvent, FlowInputSpec, FlowInputs, FlowSessionEnvironment,
    HttpRequestTemplate, HttpStepDefinition, JsonTemplate, ResponseCheck, ResponseExport,
    TemplatePart, TextTemplate,
};
use postman_http::request::HttpMethod;
use postman_request::RequestClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志订阅器（可通过 RUST_LOG=debug 控制日志级别）
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("postman_flow=debug,info")),
        )
        .init();

    // 初始化网络传输客户端
    let transport = RequestClient::try_new("postman-flow-order-example/0.1.0")?;

    // 执行流
    let events = execute_flow(
        compile::compile_example(&order_business_chain_definition())?,
        transport,
        FlowSessionEnvironment::new(FlowInputs::new()),
    )?;
    let mut events = std::pin::pin!(events);

    let mut succeeded = false;

    tracing::info!("启动业务流程链：登录 -> 筛选订单 -> 关联下单");

    // 观察事件流推进
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
            FlowEvent::FlowFinished { success, .. } => {
                succeeded = *success;
                if *success {
                    tracing::info!("Flow 全部步骤执行完毕，状态：成功");
                } else {
                    tracing::error!("Flow 执行终止，状态：失败");
                }
            }
        }
    }

    if !succeeded {
        return Err("业务流程链执行失败，请检查上方日志".into());
    }

    tracing::info!("✅ 业务流程链路执行成功：登录 -> 筛选订单 -> 提交下单 全链路闭环！");
    Ok(())
}

pub(crate) fn order_business_chain_definition() -> FlowDefinition {
    FlowDefinition {
        name: "order-business-chain-simulation".to_owned(),
        // 1. 全局入参配置
        inputs: vec![
            FlowInputSpec::with_default("host", "https://httpbingo.org"),
            FlowInputSpec::with_default("customer", "张三"),
            FlowInputSpec::with_default("school", "实验小学"),
        ],
        steps: vec![
            // ==============================================================
            // 步骤 1: 模拟登录，生成并获取 Token
            // ==============================================================
            HttpStepDefinition::new(
                "step-login",
                "1. 用户登录（获取 Token）",
                HttpRequestTemplate::new(
                    HttpMethod::GET,
                    TextTemplate::parts([
                        TemplatePart::input("host"),
                        TemplatePart::literal("/uuid"),
                    ]),
                )
                .header(
                    TextTemplate::literal("Accept"),
                    TextTemplate::literal("application/json"),
                ),
            )
            .check(ResponseCheck::StatusEquals(200))
            // 关键：从 httpbingo 返回的 {"uuid": "..."} 中导出 auth_token
            .export(ResponseExport::json("auth_token", "$.uuid").sensitive()),
            // ==============================================================
            // 步骤 2: 模拟订单筛选（筛选张三在实验小学的订单）
            // ==============================================================
            HttpStepDefinition::new(
                "step-filter-orders",
                "2. 筛选指定客户和学校的订单",
                HttpRequestTemplate::new(
                    HttpMethod::POST,
                    TextTemplate::parts([
                        TemplatePart::input("host"),
                        TemplatePart::literal("/anything/orders/query"),
                    ]),
                )
                // 鉴权头注入步骤 1 的 Token
                .header(
                    TextTemplate::literal("Authorization"),
                    TextTemplate::parts([
                        TemplatePart::literal("Bearer "),
                        TemplatePart::step_output("step-login", "auth_token"),
                    ]),
                )
                .header(
                    TextTemplate::literal("Content-Type"),
                    TextTemplate::literal("application/json"),
                )
                .json_value_body(JsonTemplate::object([
                    ("customer", JsonTemplate::input("customer")),
                    ("school", JsonTemplate::input("school")),
                    ("order_id", JsonTemplate::literal("ORD-2026-999")),
                ])),
            )
            .check(ResponseCheck::StatusEquals(200))
            // 关键：从 httpbingo 回显的 $.json.order_id 提取订单号，导出给步骤 3
            .export(ResponseExport::json("target_order_id", "$.json.order_id")),
            // ==============================================================
            // 步骤 3: 模拟关联订单提交下单
            // ==============================================================
            HttpStepDefinition::new(
                "step-place-order",
                "3. 关联前序订单提交新订单",
                HttpRequestTemplate::new(
                    HttpMethod::POST,
                    TextTemplate::parts([
                        TemplatePart::input("host"),
                        TemplatePart::literal("/anything/orders/submit"),
                    ]),
                )
                // 复用步骤 1 的 Token
                .header(
                    TextTemplate::literal("Authorization"),
                    TextTemplate::parts([
                        TemplatePart::literal("Bearer "),
                        TemplatePart::step_output("step-login", "auth_token"),
                    ]),
                )
                .header(
                    TextTemplate::literal("Content-Type"),
                    TextTemplate::literal("application/json"),
                )
                .json_value_body(JsonTemplate::object([
                    (
                        "source_order_id",
                        JsonTemplate::step_output("step-filter-orders", "target_order_id"),
                    ),
                    ("target_school", JsonTemplate::input("school")),
                    ("customer_name", JsonTemplate::input("customer")),
                ])),
            )
            // 断言 1：状态码必须为 200
            .check(ResponseCheck::StatusEquals(200))
            // 断言 2：验证步骤 2 的订单号是否正确注入到步骤 3 的请求体中
            .check(ResponseCheck::JsonValueEquals {
                path: "$.json.source_order_id".to_owned(),
                expected: JsonTemplate::step_output("step-filter-orders", "target_order_id"),
            })
            .check(ResponseCheck::JsonValueEquals {
                path: "$.json.target_school".to_owned(),
                expected: JsonTemplate::input("school"),
            }),
        ],
        outputs: Vec::new(),
    }
}

#[test]
fn native_yaml_compiles_to_the_same_plan_as_the_rust_definition() {
    let document =
        postman_flow::parse_flow_yaml(include_str!("flows/httpbingo_order_chain.http.yml"))
            .unwrap();
    let environment = postman_flow::CompileEnvironment::default();
    let native = postman_flow::compile_flow(&document.flow, &document.apis, &environment).unwrap();
    let constructed = postman_flow::compile_flow(
        &order_business_chain_definition(),
        &postman_flow::ApiCatalog::new(),
        &environment,
    )
    .unwrap();
    assert_eq!(native, constructed);
    let saved = postman_flow::write_flow_yaml(&document).unwrap();
    assert_eq!(postman_flow::parse_flow_yaml(&saved).unwrap(), document);
}

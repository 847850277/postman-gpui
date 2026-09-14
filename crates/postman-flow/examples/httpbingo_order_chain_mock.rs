use futures::StreamExt;
use postman_flow::{
    execute_flow, FlowEvent, FlowInputSpec, FlowInputs, FlowPlan, FlowSessionEnvironment,
    HttpStepPlan, ResponseCheck, ResponseExport, TemplatePart, TextTemplate,
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
    let mut events = execute_flow(
        order_business_chain_plan(),
        FlowInputs::new(),
        FlowSessionEnvironment::new(transport),
    )?;

    let mut succeeded = false;

    // 观察事件流推进
    while let Some(event) = events.next().await {
        let event = event?;
        println!("{event:?}");
        if let FlowEvent::FlowFinished { success } = event {
            succeeded = success;
        }
    }

    if !succeeded {
        return Err("业务流程链执行失败，请检查上方事件输出".into());
    }

    println!("\n✅ 业务流程链路执行成功：登录 -> 筛选订单 -> 提交下单 全链路闭环！");
    Ok(())
}

fn order_business_chain_plan() -> FlowPlan {
    FlowPlan {
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
            HttpStepPlan::new(
                "step-login",
                "1. 用户登录（获取 Token）",
                HttpMethod::GET,
                TextTemplate::parts([TemplatePart::input("host"), TemplatePart::literal("/uuid")]),
            )
                .header(
                    TextTemplate::literal("Accept"),
                    TextTemplate::literal("application/json"),
                )
                .check(ResponseCheck::StatusEquals(200))
                // 关键：从 httpbingo 返回的 {"uuid": "..."} 中导出 auth_token
                .export(ResponseExport::json("auth_token", "$.uuid").sensitive()),

            // ==============================================================
            // 步骤 2: 模拟订单筛选（筛选张三在实验小学的订单）
            // ==============================================================
            HttpStepPlan::new(
                "step-filter-orders",
                "2. 筛选指定客户和学校的订单",
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
                // 模拟查询条件，并生成一个模拟订单号 "ORD-2026-999"
                .json_body(TextTemplate::parts([
                    TemplatePart::literal(r#"{"customer":""#),
                    TemplatePart::input("customer"),
                    TemplatePart::literal(r#"","school":""#),
                    TemplatePart::input("school"),
                    TemplatePart::literal(r#"","order_id":"ORD-2026-999"}"#),
                ]))
                .check(ResponseCheck::StatusEquals(200))
                // 关键：从 httpbingo 回显的 $.json.order_id 提取订单号，导出给步骤 3
                .export(ResponseExport::json("target_order_id", "$.json.order_id")),

            // ==============================================================
            // 步骤 3: 模拟关联订单提交下单
            // ==============================================================
            HttpStepPlan::new(
                "step-place-order",
                "3. 关联前序订单提交新订单",
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
                // 关键：Body 组合步骤 2 导出的 target_order_id、输入的 school、输入的 customer
                .json_body(TextTemplate::parts([
                    TemplatePart::literal(r#"{"source_order_id":""#),
                    TemplatePart::step_output("step-filter-orders", "target_order_id"),
                    TemplatePart::literal(r#"","target_school":""#),
                    TemplatePart::input("school"),
                    TemplatePart::literal(r#"","customer_name":""#),
                    TemplatePart::input("customer"),
                    TemplatePart::literal(r#""}"#),
                ]))
                // 断言 1：状态码必须为 200
                .check(ResponseCheck::StatusEquals(200))
                // 断言 2：验证步骤 2 的订单号是否正确注入到步骤 3 的请求体中
                .check(ResponseCheck::JsonPathEquals {
                    path: "$.json.source_order_id".to_owned(),
                    expected: TextTemplate::parts([
                        TemplatePart::step_output("step-filter-orders", "target_order_id"),
                    ]),
                })
                // 断言 3：验证学校是否正确注入
                .check(ResponseCheck::JsonPathEquals {
                    path: "$.json.target_school".to_owned(),
                    expected: TextTemplate::parts([TemplatePart::input("school")]),
                }),
        ],
    }
}
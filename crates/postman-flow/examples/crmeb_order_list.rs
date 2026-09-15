use futures::StreamExt;
use postman_flow::{
    execute_flow, FlowEvent, FlowInputSpec, FlowInputs, FlowPlan, FlowSessionEnvironment,
    HttpStepPlan, JsonTemplate, ResponseCheck, ResponseExport, TemplatePart, TextTemplate,
};
use postman_http::request::HttpMethod;
use postman_request::RequestClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化结构化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("postman_flow=debug,info")),
        )
        .init();

    let transport = RequestClient::try_new("postman-flow-example/0.1.0")?;
    let mut events = execute_flow(
        crmeb_order_list_plan(),
        FlowInputs::new(),
        FlowSessionEnvironment::new(transport),
    )?;

    let mut succeeded = false;

    while let Some(event) = events.next().await {
        let event = event?;
        tracing::info!("{event:?}");
        if let FlowEvent::FlowFinished { success } = event {
            succeeded = success;
        }
    }

    if !succeeded {
        return Err("crmeb order list Flow failed; inspect the events above".into());
    }
    Ok(())
}

fn crmeb_order_list_plan() -> FlowPlan {
    FlowPlan {
        name: "crmeb_order_list".to_owned(),
        // 1. 全局入参配置
        inputs: vec![
            FlowInputSpec::with_default("host", "https://v6.crmeb.net"),
            FlowInputSpec::with_default("account", "demo"),
            FlowInputSpec::with_default("pwd", "crmeb.com").sensitive(),
            FlowInputSpec::with_default("captchaType", "blockPuzzle"),
            FlowInputSpec::with_default("captchaVerification", ""),
        ],
        steps: vec![
            HttpStepPlan::new(
                "pre-step-login",
                "0. 登陆前置获取key",
                HttpMethod::GET,
                TextTemplate::parts([TemplatePart::input("host"), TemplatePart::literal("/adminapi/login/info")]),
            )
                .header(
                    TextTemplate::literal("Content-Type"),
                    TextTemplate::literal("application/json"),
                )
                .check(ResponseCheck::StatusEquals(200))
                // 关键：提取前置接口的 key 导出
                .export(ResponseExport::json("key", "$.data.key").sensitive()),
            HttpStepPlan::new(
                "step-login",
                "1. 用户登录（获取 Token）",
                HttpMethod::POST,
                TextTemplate::parts([TemplatePart::input("host"), TemplatePart::literal("/adminapi/login")]),
            )
            .header(
                TextTemplate::literal("Content-Type"),
                TextTemplate::literal("application/json"),
            )
            .json_value_body(JsonTemplate::object([
                ("account", JsonTemplate::input("account")),
                ("pwd", JsonTemplate::input("pwd")),
                // 关键：从前置步骤 pre-step-login 提取导出的 key
                ("key", JsonTemplate::step_output("pre-step-login", "key")),
                ("captchaType", JsonTemplate::input("captchaType")),
                ("captchaVerification", JsonTemplate::input("captchaVerification")),
            ]))
            .check(ResponseCheck::StatusEquals(200))
            // 关键：提取 token 导出
            .export(ResponseExport::json("token", "$.data.token").sensitive())
            ,
            HttpStepPlan::new(
                "order-list",
                "2. 订单列表",
                HttpMethod::GET,
                TextTemplate::parts([TemplatePart::input("host"), TemplatePart::literal("/adminapi/order/list?page=1&limit=15&status=&pay_type=&data=&real_name=&field_key=all&type=")]),
            ).header(
                TextTemplate::literal("Content-Type"),
                TextTemplate::literal("application/json"),
            ).header(
                TextTemplate::literal("Authori-zation"),
                TextTemplate::parts([
                    TemplatePart::literal("Bearer "),
                    TemplatePart::step_output("step-login", "token"),
                ]),
            ).check(ResponseCheck::StatusEquals(200))
            .check(ResponseCheck::JsonPathEquals {
                path: "$.status".to_string(),
                expected: TextTemplate::literal("200"),
            })
        ],
    }
}

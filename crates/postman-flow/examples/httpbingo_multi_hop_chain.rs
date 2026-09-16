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
    // 初始化结构化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("postman_flow=debug,info")),
        )
        .init();

    let transport = RequestClient::try_new("postman-flow-multihop/0.1.0")?;

    tracing::info!("启动多跳级联业务流：下单 -> 支付流 -> 凭证核销 -> 履约发货");

    let events = execute_flow(
        compile::compile_example(&multi_hop_commerce_definition())?,
        transport,
        FlowSessionEnvironment::new(FlowInputs::new()),
    )?;
    let mut events = std::pin::pin!(events);

    let mut succeeded = false;

    while let Some(event) = events.next().await {
        let event = event?;
        match &event {
            FlowEvent::FlowStarted { name, total_steps } => {
                tracing::info!(flow_name = %name, total_steps, "Flow 启动");
            }
            FlowEvent::StepStarted { step_id, name } => {
                tracing::info!(step_id = %step_id, step_name = %name, "▶ 开始执行步骤");
            }
            FlowEvent::ResponseReceived {
                step_id,
                status,
                elapsed_ms,
            } => {
                tracing::info!(step_id = %step_id, status, elapsed_ms, "↳ 收到响应");
            }
            FlowEvent::OutputExported { step_id, name } => {
                tracing::info!(step_id = %step_id, export_name = %name, "↳ 提取并导出参数");
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
                tracing::info!(step_id = %step_id, ?outcome, "⏹ 步骤执行完毕");
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
        return Err("多跳流水线执行失败".into());
    }

    tracing::info!("🎉 恭喜！5 步级联业务链（结果做参数、多级接力）全部执行成功！");
    Ok(())
}

pub(crate) fn multi_hop_commerce_definition() -> FlowDefinition {
    FlowDefinition {
        name: "e-commerce-multi-hop-pipeline".to_owned(),
        inputs: vec![
            FlowInputSpec::with_default("host", "https://httpbingo.org"),
            FlowInputSpec::with_default("user_id", "USR-8888"),
            FlowInputSpec::with_default("shipping_address", "北京市海淀区中关村南大街 1 号"),
        ],
        steps: vec![
            // =========================================================================
            // 步骤 1: 获取会话/认证凭证（Auth）
            // =========================================================================
            HttpStepDefinition::new(
                "step-1-auth",
                "1. 用户登录获取 AuthToken",
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
            // 导出 token
            .export(ResponseExport::json("auth_token", "$.uuid").sensitive()),
            // =========================================================================
            // 步骤 2: 创建草稿订单（Create Draft Order）
            // 依赖：步骤 1 的 auth_token + 全局 user_id
            // 产出：草稿订单号 draft_order_id 和 应付金额 total_amount
            // =========================================================================
            HttpStepDefinition::new(
                "step-2-create-draft-order",
                "2. 创建交易草稿订单",
                HttpRequestTemplate::new(
                    HttpMethod::POST,
                    TextTemplate::parts([
                        TemplatePart::input("host"),
                        TemplatePart::literal("/anything/orders/draft"),
                    ]),
                )
                .header(
                    TextTemplate::literal("Authorization"),
                    TextTemplate::parts([
                        TemplatePart::literal("Bearer "),
                        TemplatePart::step_output("step-1-auth", "auth_token"),
                    ]),
                )
                .header(
                    TextTemplate::literal("Content-Type"),
                    TextTemplate::literal("application/json"),
                )
                .json_value_body(JsonTemplate::object([
                    ("buyer_id", JsonTemplate::input("user_id")),
                    ("sku_id", JsonTemplate::literal("SKU-IPHONE-16")),
                    ("quantity", JsonTemplate::literal(1_i64)),
                    ("order_id", JsonTemplate::literal("ORD-2026-9001")),
                    ("amount", JsonTemplate::literal("7999.00")),
                ])),
            )
            .check(ResponseCheck::StatusEquals(200))
            // 导出本次提交后生成的数据：订单号 与 待付金额
            .export(ResponseExport::json("draft_order_id", "$.json.order_id"))
            .export(ResponseExport::json("total_amount", "$.json.amount")),
            // =========================================================================
            // 步骤 3: 请求支付网关，生成支付流水号（Create Payment Transaction）
            // 关键依赖：必须根据步骤 2 生成的 draft_order_id 和 total_amount 作为请求参数！
            // 产出：支付网关返回的 payment_txn_id 和 签名 pay_sign
            // =========================================================================
            HttpStepDefinition::new(
                "step-3-create-payment-txn",
                "3. 根据草稿订单发起支付申请",
                HttpRequestTemplate::new(
                    HttpMethod::POST,
                    TextTemplate::parts([
                        TemplatePart::input("host"),
                        TemplatePart::literal("/anything/pay/gateway/create"),
                    ]),
                )
                .header(
                    TextTemplate::literal("Authorization"),
                    TextTemplate::parts([
                        TemplatePart::literal("Bearer "),
                        TemplatePart::step_output("step-1-auth", "auth_token"),
                    ]),
                )
                .header(
                    TextTemplate::literal("Content-Type"),
                    TextTemplate::literal("application/json"),
                )
                .json_value_body(JsonTemplate::object([
                    (
                        "target_order_id",
                        JsonTemplate::step_output("step-2-create-draft-order", "draft_order_id"),
                    ),
                    (
                        "pay_amount",
                        JsonTemplate::step_output("step-2-create-draft-order", "total_amount"),
                    ),
                    ("channel", JsonTemplate::literal("ALIPAY")),
                    ("gateway_txn_id", JsonTemplate::literal("TXN-PAY-556677")),
                    ("signature", JsonTemplate::literal("SIG-SEC-XYZ999")),
                ])),
            )
            .check(ResponseCheck::StatusEquals(200))
            // 验证支付网关收到的订单号确实等于步骤 2 的订单号
            .check(ResponseCheck::JsonValueEquals {
                path: "$.json.target_order_id".to_owned(),
                expected: JsonTemplate::step_output("step-2-create-draft-order", "draft_order_id"),
            })
            // 导出支付网关生成的流水号和凭证签名
            .export(ResponseExport::json(
                "payment_txn_id",
                "$.json.gateway_txn_id",
            ))
            .export(ResponseExport::json("pay_signature", "$.json.signature")),
            // =========================================================================
            // 步骤 4: 支付渠道凭证核销与清算（Confirm & Clear Payment）
            // 关键依赖：必须使用步骤 3 生成的 payment_txn_id 和 pay_signature 提交核销！
            // 产出：银行清算回执号 clearing_receipt_no
            // =========================================================================
            HttpStepDefinition::new(
                "step-4-confirm-payment",
                "4. 凭支付流水与签名提交银行清算核销",
                HttpRequestTemplate::new(
                    HttpMethod::POST,
                    TextTemplate::parts([
                        TemplatePart::input("host"),
                        TemplatePart::literal("/anything/pay/clearing/confirm"),
                    ]),
                )
                .header(
                    TextTemplate::literal("Content-Type"),
                    TextTemplate::literal("application/json"),
                )
                .json_value_body(JsonTemplate::object([
                    (
                        "txn_id",
                        JsonTemplate::step_output("step-3-create-payment-txn", "payment_txn_id"),
                    ),
                    (
                        "verify_sign",
                        JsonTemplate::step_output("step-3-create-payment-txn", "pay_signature"),
                    ),
                    ("receipt_no", JsonTemplate::literal("RCPT-BANK-2026-8888")),
                    ("clear_status", JsonTemplate::literal("CLEARED")),
                ])),
            )
            .check(ResponseCheck::StatusEquals(200))
            // 验证核销接口收到正确的支付流水号
            .check(ResponseCheck::JsonValueEquals {
                path: "$.json.txn_id".to_owned(),
                expected: JsonTemplate::step_output("step-3-create-payment-txn", "payment_txn_id"),
            })
            // 导出最终清算回执号
            .export(ResponseExport::json(
                "clearing_receipt_no",
                "$.json.receipt_no",
            )),
            // =========================================================================
            // 步骤 5: 仓储履约发货与开票（Fulfillment & Dispatch）
            // 关键依赖：汇总前面所有步骤产出的关键字段！
            // 需要：
            //   - 步骤 2 的 draft_order_id
            //   - 步骤 3 的 payment_txn_id
            //   - 步骤 4 的 clearing_receipt_no
            //   - 全局输入的 shipping_address
            // =========================================================================
            HttpStepDefinition::new(
                "step-5-fulfillment-dispatch",
                "5. 汇总全链路单据，通知仓库发货",
                HttpRequestTemplate::new(
                    HttpMethod::POST,
                    TextTemplate::parts([
                        TemplatePart::input("host"),
                        TemplatePart::literal("/anything/warehouse/dispatch"),
                    ]),
                )
                .header(
                    TextTemplate::literal("Authorization"),
                    TextTemplate::parts([
                        TemplatePart::literal("Bearer "),
                        TemplatePart::step_output("step-1-auth", "auth_token"),
                    ]),
                )
                .header(
                    TextTemplate::literal("Content-Type"),
                    TextTemplate::literal("application/json"),
                )
                .json_value_body(JsonTemplate::object([
                    (
                        "order_id",
                        JsonTemplate::step_output("step-2-create-draft-order", "draft_order_id"),
                    ),
                    (
                        "payment_txn_id",
                        JsonTemplate::step_output("step-3-create-payment-txn", "payment_txn_id"),
                    ),
                    (
                        "receipt_no",
                        JsonTemplate::step_output("step-4-confirm-payment", "clearing_receipt_no"),
                    ),
                    ("destination", JsonTemplate::input("shipping_address")),
                ])),
            )
            .check(ResponseCheck::StatusEquals(200))
            // 全链路严格断言验证：检查发货单据中各个单号是否完全吻合
            .check(ResponseCheck::JsonValueEquals {
                path: "$.json.order_id".to_owned(),
                expected: JsonTemplate::step_output("step-2-create-draft-order", "draft_order_id"),
            })
            .check(ResponseCheck::JsonValueEquals {
                path: "$.json.payment_txn_id".to_owned(),
                expected: JsonTemplate::step_output("step-3-create-payment-txn", "payment_txn_id"),
            })
            .check(ResponseCheck::JsonValueEquals {
                path: "$.json.receipt_no".to_owned(),
                expected: JsonTemplate::step_output(
                    "step-4-confirm-payment",
                    "clearing_receipt_no",
                ),
            }),
        ],
        outputs: Vec::new(),
    }
}

#[test]
fn native_yaml_compiles_to_the_same_plan_as_the_rust_definition() {
    let document =
        postman_flow::parse_flow_yaml(include_str!("flows/httpbingo_multi_hop.http.yml")).unwrap();
    let environment = postman_flow::CompileEnvironment::default();
    let native = postman_flow::compile_flow(&document.flow, &document.apis, &environment).unwrap();
    let constructed = postman_flow::compile_flow(
        &multi_hop_commerce_definition(),
        &postman_flow::ApiCatalog::new(),
        &environment,
    )
    .unwrap();
    assert_eq!(native, constructed);
    let saved = postman_flow::write_flow_yaml(&document).unwrap();
    assert_eq!(postman_flow::parse_flow_yaml(&saved).unwrap(), document);
}

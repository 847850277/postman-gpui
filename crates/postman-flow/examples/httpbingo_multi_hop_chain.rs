use futures::StreamExt;
use postman_flow::{
    execute_flow, FlowEvent, FlowInputSpec, FlowInputs, FlowPlan, FlowSessionEnvironment,
    HttpStepPlan, ResponseCheck, ResponseExport, TemplatePart, TextTemplate,
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

    let mut events = execute_flow(
        multi_hop_commerce_pipeline(),
        FlowInputs::new(),
        FlowSessionEnvironment::new(transport),
    )?;

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
            FlowEvent::FlowFinished { success } => {
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

fn multi_hop_commerce_pipeline() -> FlowPlan {
    FlowPlan {
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
            HttpStepPlan::new(
                "step-1-auth",
                "1. 用户登录获取 AuthToken",
                HttpMethod::GET,
                TextTemplate::parts([TemplatePart::input("host"), TemplatePart::literal("/uuid")]),
            )
            .header(TextTemplate::literal("Accept"), TextTemplate::literal("application/json"))
            .check(ResponseCheck::StatusEquals(200))
            // 导出 token
            .export(ResponseExport::json("auth_token", "$.uuid").sensitive()),

            // =========================================================================
            // 步骤 2: 创建草稿订单（Create Draft Order）
            // 依赖：步骤 1 的 auth_token + 全局 user_id
            // 产出：草稿订单号 draft_order_id 和 应付金额 total_amount
            // =========================================================================
            HttpStepPlan::new(
                "step-2-create-draft-order",
                "2. 创建交易草稿订单",
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
            .header(TextTemplate::literal("Content-Type"), TextTemplate::literal("application/json"))
            .json_body(TextTemplate::parts([
                TemplatePart::literal(r#"{"buyer_id":""#),
                TemplatePart::input("user_id"),
                TemplatePart::literal(r#"","sku_id":"SKU-IPHONE-16","quantity":1,"order_id":"ORD-2026-9001","amount":"7999.00"}"#),
            ]))
            .check(ResponseCheck::StatusEquals(200))
            // 导出本次提交后生成的数据：订单号 与 待付金额
            .export(ResponseExport::json("draft_order_id", "$.json.order_id"))
            .export(ResponseExport::json("total_amount", "$.json.amount")),

            // =========================================================================
            // 步骤 3: 请求支付网关，生成支付流水号（Create Payment Transaction）
            // 关键依赖：必须根据步骤 2 生成的 draft_order_id 和 total_amount 作为请求参数！
            // 产出：支付网关返回的 payment_txn_id 和 签名 pay_sign
            // =========================================================================
            HttpStepPlan::new(
                "step-3-create-payment-txn",
                "3. 根据草稿订单发起支付申请",
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
            .header(TextTemplate::literal("Content-Type"), TextTemplate::literal("application/json"))
            // 请求参数完全引用 步骤 2 提交后得到的结果！
            .json_body(TextTemplate::parts([
                TemplatePart::literal(r#"{"target_order_id":""#),
                TemplatePart::step_output("step-2-create-draft-order", "draft_order_id"),
                TemplatePart::literal(r#"","pay_amount":""#),
                TemplatePart::step_output("step-2-create-draft-order", "total_amount"),
                TemplatePart::literal(r#"","channel":"ALIPAY","gateway_txn_id":"TXN-PAY-556677","signature":"SIG-SEC-XYZ999"}"#),
            ]))
            .check(ResponseCheck::StatusEquals(200))
            // 验证支付网关收到的订单号确实等于步骤 2 的订单号
            .check(ResponseCheck::JsonPathEquals {
                path: "$.json.target_order_id".to_owned(),
                expected: TextTemplate::parts([
                    TemplatePart::step_output("step-2-create-draft-order", "draft_order_id"),
                ]),
            })
            // 导出支付网关生成的流水号和凭证签名
            .export(ResponseExport::json("payment_txn_id", "$.json.gateway_txn_id"))
            .export(ResponseExport::json("pay_signature", "$.json.signature")),

            // =========================================================================
            // 步骤 4: 支付渠道凭证核销与清算（Confirm & Clear Payment）
            // 关键依赖：必须使用步骤 3 生成的 payment_txn_id 和 pay_signature 提交核销！
            // 产出：银行清算回执号 clearing_receipt_no
            // =========================================================================
            HttpStepPlan::new(
                "step-4-confirm-payment",
                "4. 凭支付流水与签名提交银行清算核销",
                HttpMethod::POST,
                TextTemplate::parts([
                    TemplatePart::input("host"),
                    TemplatePart::literal("/anything/pay/clearing/confirm"),
                ]),
            )
            .header(TextTemplate::literal("Content-Type"), TextTemplate::literal("application/json"))
            // 请求参数完全引用 步骤 3 提交后得到的结果！
            .json_body(TextTemplate::parts([
                TemplatePart::literal(r#"{"txn_id":""#),
                TemplatePart::step_output("step-3-create-payment-txn", "payment_txn_id"),
                TemplatePart::literal(r#"","verify_sign":""#),
                TemplatePart::step_output("step-3-create-payment-txn", "pay_signature"),
                TemplatePart::literal(r#"","receipt_no":"RCPT-BANK-2026-8888","clear_status":"CLEARED"}"#),
            ]))
            .check(ResponseCheck::StatusEquals(200))
            // 验证核销接口收到正确的支付流水号
            .check(ResponseCheck::JsonPathEquals {
                path: "$.json.txn_id".to_owned(),
                expected: TextTemplate::parts([
                    TemplatePart::step_output("step-3-create-payment-txn", "payment_txn_id"),
                ]),
            })
            // 导出最终清算回执号
            .export(ResponseExport::json("clearing_receipt_no", "$.json.receipt_no")),

            // =========================================================================
            // 步骤 5: 仓储履约发货与开票（Fulfillment & Dispatch）
            // 关键依赖：汇总前面所有步骤产出的关键字段！
            // 需要：
            //   - 步骤 2 的 draft_order_id
            //   - 步骤 3 的 payment_txn_id
            //   - 步骤 4 的 clearing_receipt_no
            //   - 全局输入的 shipping_address
            // =========================================================================
            HttpStepPlan::new(
                "step-5-fulfillment-dispatch",
                "5. 汇总全链路单据，通知仓库发货",
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
            .header(TextTemplate::literal("Content-Type"), TextTemplate::literal("application/json"))
            // 终极汇总报文：跨步骤级联组装！
            .json_body(TextTemplate::parts([
                TemplatePart::literal(r#"{"order_id":""#),
                TemplatePart::step_output("step-2-create-draft-order", "draft_order_id"),
                TemplatePart::literal(r#"","payment_txn_id":""#),
                TemplatePart::step_output("step-3-create-payment-txn", "payment_txn_id"),
                TemplatePart::literal(r#"","receipt_no":""#),
                TemplatePart::step_output("step-4-confirm-payment", "clearing_receipt_no"),
                TemplatePart::literal(r#"","destination":""#),
                TemplatePart::input("shipping_address"),
                TemplatePart::literal(r#""}"#),
            ]))
            .check(ResponseCheck::StatusEquals(200))
            // 全链路严格断言验证：检查发货单据中各个单号是否完全吻合
            .check(ResponseCheck::JsonPathEquals {
                path: "$.json.order_id".to_owned(),
                expected: TextTemplate::parts([
                    TemplatePart::step_output("step-2-create-draft-order", "draft_order_id"),
                ]),
            })
            .check(ResponseCheck::JsonPathEquals {
                path: "$.json.payment_txn_id".to_owned(),
                expected: TextTemplate::parts([
                    TemplatePart::step_output("step-3-create-payment-txn", "payment_txn_id"),
                ]),
            })
            .check(ResponseCheck::JsonPathEquals {
                path: "$.json.receipt_no".to_owned(),
                expected: TextTemplate::parts([
                    TemplatePart::step_output("step-4-confirm-payment", "clearing_receipt_no"),
                ]),
            }),
        ],
    }
}

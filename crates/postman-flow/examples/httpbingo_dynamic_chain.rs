//! HTTPBingo-only dynamic chain: every identifier comes from a separate /uuid response.
//! /anything verifies data propagation, not order creation, payment, or settlement business rules.

mod support;

use postman_flow::{
    FlowInputSpec, FlowInputs, FlowPlan, HttpStepPlan, JsonTemplate, ResponseExport, TemplatePart,
    TextTemplate,
};
use postman_http::request::HttpMethod;
use serde_json::json;

use support::{equals, request};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("postman_flow=debug,info")),
        )
        .init();

    tracing::info!(
        "HTTPBingo: server-generated IDs -> order -> payment -> receipt -> combined payload."
    );
    tracing::info!(
        "This is data-flow verification; HTTPBingo does not execute real business operations."
    );
    support::run_live(dynamic_chain_plan(), FlowInputs::new()).await
}

fn uuid(id: &str) -> HttpStepPlan {
    request(id, HttpMethod::GET, "/uuid").export(ResponseExport::json("id", "$.uuid"))
}

fn authorized(id: &str, path: &str) -> HttpStepPlan {
    request(id, HttpMethod::POST, path).header(
        TextTemplate::literal("Authorization"),
        TextTemplate::parts([
            TemplatePart::literal("Bearer "),
            TemplatePart::step_output("auth", "token"),
        ]),
    )
}

fn dynamic_chain_plan() -> FlowPlan {
    FlowPlan {
        name: "httpbingo-dynamic-chain".into(),
        inputs: vec![
            FlowInputSpec::with_default("host", "https://httpbingo.org"),
            FlowInputSpec::with_default("customer", "张\"三\""),
            FlowInputSpec::with_default("address", "实验小学\n二楼\\东侧"),
            FlowInputSpec::with_default("quantity", 2),
            // Amount is an explicit test input, not a price calculated by HTTPBingo.
            FlowInputSpec::with_default("amount_minor", 129900),
            FlowInputSpec::with_default("metadata", json!({"gift": false, "note": null})),
        ],
        steps: vec![
            request("auth", HttpMethod::GET, "/uuid")
                .export(ResponseExport::json("token", "$.uuid").sensitive()),
            uuid("allocate-order-id"),
            authorized("order", "/anything/flow/orders")
                .json_value_body(JsonTemplate::object([
                    (
                        "order_id",
                        JsonTemplate::step_output("allocate-order-id", "id"),
                    ),
                    ("customer", JsonTemplate::input("customer")),
                    ("quantity", JsonTemplate::input("quantity")),
                    ("amount_minor", JsonTemplate::input("amount_minor")),
                    ("metadata", JsonTemplate::input("metadata")),
                ]))
                .check(equals(
                    "$.json.order_id",
                    TemplatePart::step_output("allocate-order-id", "id"),
                ))
                .check(equals("$.json.customer", TemplatePart::input("customer")))
                .check(equals("$.json.quantity", TemplatePart::input("quantity")))
                .check(equals(
                    "$.json.amount_minor",
                    TemplatePart::input("amount_minor"),
                ))
                .check(equals("$.json.metadata", TemplatePart::input("metadata")))
                .export(ResponseExport::json("order_id", "$.json.order_id"))
                .export(ResponseExport::json("amount_minor", "$.json.amount_minor"))
                .export(ResponseExport::json("document", "$.json")),
            uuid("allocate-payment-id"),
            authorized("payment", "/anything/flow/payments")
                .json_value_body(JsonTemplate::object([
                    ("order_id", JsonTemplate::step_output("order", "order_id")),
                    (
                        "amount_minor",
                        JsonTemplate::step_output("order", "amount_minor"),
                    ),
                    (
                        "payment_id",
                        JsonTemplate::step_output("allocate-payment-id", "id"),
                    ),
                ]))
                .check(equals(
                    "$.json.order_id",
                    TemplatePart::step_output("order", "order_id"),
                ))
                .check(equals(
                    "$.json.amount_minor",
                    TemplatePart::step_output("order", "amount_minor"),
                ))
                .check(equals(
                    "$.json.payment_id",
                    TemplatePart::step_output("allocate-payment-id", "id"),
                ))
                .export(ResponseExport::json("payment_id", "$.json.payment_id"))
                .export(ResponseExport::json("document", "$.json")),
            uuid("allocate-receipt-id"),
            authorized("settlement", "/anything/flow/settlements")
                .json_value_body(JsonTemplate::object([
                    ("order_id", JsonTemplate::step_output("order", "order_id")),
                    (
                        "payment_id",
                        JsonTemplate::step_output("payment", "payment_id"),
                    ),
                    (
                        "receipt_id",
                        JsonTemplate::step_output("allocate-receipt-id", "id"),
                    ),
                ]))
                .check(equals(
                    "$.json.order_id",
                    TemplatePart::step_output("order", "order_id"),
                ))
                .check(equals(
                    "$.json.payment_id",
                    TemplatePart::step_output("payment", "payment_id"),
                ))
                .check(equals(
                    "$.json.receipt_id",
                    TemplatePart::step_output("allocate-receipt-id", "id"),
                ))
                .export(ResponseExport::json("receipt_id", "$.json.receipt_id"))
                .export(ResponseExport::json("document", "$.json")),
            authorized("dispatch", "/anything/flow/dispatch")
                .json_value_body(JsonTemplate::object([
                    ("order", JsonTemplate::step_output("order", "document")),
                    ("payment", JsonTemplate::step_output("payment", "document")),
                    (
                        "settlement",
                        JsonTemplate::step_output("settlement", "document"),
                    ),
                    ("address", JsonTemplate::input("address")),
                ]))
                .check(equals(
                    "$.json.order",
                    TemplatePart::step_output("order", "document"),
                ))
                .check(equals(
                    "$.json.payment",
                    TemplatePart::step_output("payment", "document"),
                ))
                .check(equals(
                    "$.json.settlement",
                    TemplatePart::step_output("settlement", "document"),
                ))
                .check(equals(
                    "$.json.settlement.order_id",
                    TemplatePart::step_output("allocate-order-id", "id"),
                ))
                .check(equals(
                    "$.json.settlement.payment_id",
                    TemplatePart::step_output("allocate-payment-id", "id"),
                ))
                .check(equals(
                    "$.json.settlement.receipt_id",
                    TemplatePart::step_output("allocate-receipt-id", "id"),
                ))
                .check(equals("$.json.address", TemplatePart::input("address"))),
        ],
    }
}

#[cfg(test)]
mod tests {
    use postman_flow::{BodyTemplate, FlowEvent, StepOutcome};

    use super::*;
    use support::testing::{body, run, HttpBingoFixture};

    #[tokio::test]
    async fn ids_from_distinct_responses_flow_into_the_final_combined_document() {
        let transport = HttpBingoFixture::default();
        let events = run(dynamic_chain_plan(), FlowInputs::new(), transport.clone())
            .await
            .unwrap();
        assert_eq!(
            events.last(),
            Some(&FlowEvent::FlowFinished { success: true })
        );
        let requests = transport.requests();
        assert_eq!(requests.len(), 8);
        let order = body(&requests[2]);
        let payment = body(&requests[4]);
        let settlement = body(&requests[6]);
        let dispatch = body(&requests[7]);
        // These values are produced by the fixture's /uuid responses, never put into the plan.
        assert_eq!(order["order_id"], "00000000-0000-4000-8000-000000000002");
        assert_eq!(
            payment["payment_id"],
            "00000000-0000-4000-8000-000000000003"
        );
        assert_eq!(
            settlement["receipt_id"],
            "00000000-0000-4000-8000-000000000004"
        );
        assert_eq!(payment["order_id"], order["order_id"]);
        assert_eq!(payment["amount_minor"], json!(129900));
        assert_eq!(settlement["order_id"], order["order_id"]);
        assert_eq!(settlement["payment_id"], payment["payment_id"]);
        assert_eq!(dispatch["order"], order);
        assert_eq!(dispatch["payment"], payment);
        assert_eq!(dispatch["settlement"], settlement);
        assert_eq!(dispatch["address"], "实验小学\n二楼\\东侧");
        for request in requests
            .iter()
            .filter(|request| request.method == HttpMethod::POST)
        {
            assert!(request.headers.iter().any(|(name, value)| {
                name.eq_ignore_ascii_case("authorization")
                    && value == "Bearer 00000000-0000-4000-8000-000000000001"
            }));
        }
    }

    #[tokio::test]
    async fn rerunning_the_same_plan_uses_new_server_values_and_session_inputs() {
        let transport = HttpBingoFixture::default();
        for customer in ["first buyer", "second \"buyer\"\n"] {
            let events = run(
                dynamic_chain_plan(),
                FlowInputs::new().with("customer", customer),
                transport.clone(),
            )
            .await
            .unwrap();
            assert_eq!(
                events.last(),
                Some(&FlowEvent::FlowFinished { success: true })
            );
        }
        let requests = transport.requests();
        assert_eq!(requests.len(), 16);
        let first = body(&requests[7]);
        let second = body(&requests[15]);
        assert_ne!(first["order"]["order_id"], second["order"]["order_id"]);
        assert_ne!(
            first["payment"]["payment_id"],
            second["payment"]["payment_id"]
        );
        assert_ne!(
            first["settlement"]["receipt_id"],
            second["settlement"]["receipt_id"]
        );
        assert_eq!(first["order"]["customer"], "first buyer");
        assert_eq!(second["order"]["customer"], "second \"buyer\"\n");
    }

    #[tokio::test]
    async fn a_failure_at_any_hop_stops_every_later_request() {
        for index in 0..8 {
            let plan = dynamic_chain_plan();
            let failed_id = plan.steps[index].id.clone();
            let transport = HttpBingoFixture::default();
            transport.fail_on_request(index);
            let events = run(plan, FlowInputs::new(), transport.clone())
                .await
                .unwrap();
            assert_eq!(
                events.last(),
                Some(&FlowEvent::FlowFinished { success: false })
            );
            assert_eq!(transport.requests().len(), index + 1);
            assert!(events.iter().any(|event| matches!(
                event,
                FlowEvent::StepFinished { step_id, outcome: StepOutcome::Failed { .. } }
                    if step_id == &failed_id
            )));
            assert!(!events.iter().any(|event| matches!(
                event, FlowEvent::OutputExported { step_id, .. } if step_id == &failed_id
            )));
        }
    }

    #[tokio::test]
    async fn incorrect_link_or_stringified_amount_fails_even_when_http_succeeds() {
        for (field, wrong_value) in [
            ("order_id", JsonTemplate::literal("not-the-server-order")),
            ("amount_minor", JsonTemplate::literal("129900")),
        ] {
            let mut plan = dynamic_chain_plan();
            let BodyTemplate::JsonValue(JsonTemplate::Object(fields)) = &mut plan.steps[4].body
            else {
                panic!("payment must use structured JSON");
            };
            fields.insert(field.into(), wrong_value);
            let transport = HttpBingoFixture::default();
            let events = run(plan, FlowInputs::new(), transport.clone())
                .await
                .unwrap();
            assert_eq!(
                events.last(),
                Some(&FlowEvent::FlowFinished { success: false })
            );
            assert_eq!(transport.requests().len(), 5);
            assert!(events.iter().any(|event| matches!(
                event, FlowEvent::ResponseReceived { step_id, status: 200, .. } if step_id == "payment"
            )));
            assert!(events.iter().any(|event| matches!(
                event, FlowEvent::CheckFinished { step_id, success: false, .. } if step_id == "payment"
            )));
            assert!(!events.iter().any(|event| matches!(
                event, FlowEvent::OutputExported { step_id, .. } if step_id == "payment"
            )));
        }
    }
}

use futures::StreamExt;
use postman_flow::{
    execute_flow, FlowEvent, FlowInputSpec, FlowInputs, FlowPlan, FlowSessionEnvironment,
    HttpStepPlan, ResponseCheck, ResponseExport, TemplatePart, TextTemplate,
};
use postman_http::request::HttpMethod;
use postman_request::RequestClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let transport = RequestClient::try_new("postman-flow-example/0.1.0")?;
    let mut events = execute_flow(
        httpbingo_plan(),
        FlowInputs::new(),
        FlowSessionEnvironment::new(transport),
    )?;
    let mut succeeded = false;

    while let Some(event) = events.next().await {
        let event = event?;
        println!("{event:?}");
        if let FlowEvent::FlowFinished { success } = event {
            succeeded = success;
        }
    }

    if !succeeded {
        return Err("httpbingo Flow failed; inspect the events above".into());
    }
    Ok(())
}

fn httpbingo_plan() -> FlowPlan {
    FlowPlan {
        name: "httpbingo-minimal".to_owned(),
        inputs: vec![
            FlowInputSpec::with_default("host", "https://httpbingo.org"),
            FlowInputSpec::with_default("client", "postman-flow-headless"),
        ],
        steps: vec![
            HttpStepPlan::new(
                "generate-correlation-id",
                "Generate a correlation id",
                HttpMethod::GET,
                TextTemplate::parts([TemplatePart::input("host"), TemplatePart::literal("/uuid")]),
            )
            .header(
                TextTemplate::literal("Accept"),
                TextTemplate::literal("application/json"),
            )
            .check(ResponseCheck::StatusEquals(200))
            .export(ResponseExport::json("correlation_id", "$.uuid")),
            HttpStepPlan::new(
                "echo-http-request",
                "Reuse the correlation id",
                HttpMethod::POST,
                TextTemplate::parts([
                    TemplatePart::input("host"),
                    TemplatePart::literal("/anything/headless-e2e/"),
                    TemplatePart::step_output("generate-correlation-id", "correlation_id"),
                ]),
            )
            .header(
                TextTemplate::literal("Accept"),
                TextTemplate::literal("application/json"),
            )
            .header(
                TextTemplate::literal("Content-Type"),
                TextTemplate::literal("application/json"),
            )
            .json_body(TextTemplate::parts([
                TemplatePart::literal("{\"client\":\""),
                TemplatePart::input("client"),
                TemplatePart::literal("\",\"correlation_id\":\""),
                TemplatePart::step_output("generate-correlation-id", "correlation_id"),
                TemplatePart::literal("\"}"),
            ]))
            .check(ResponseCheck::StatusEquals(200))
            .check(ResponseCheck::JsonPathEquals {
                path: "$.method".to_owned(),
                expected: TextTemplate::literal("POST"),
            })
            .check(ResponseCheck::JsonPathEquals {
                path: "$.json.client".to_owned(),
                expected: TextTemplate::parts([TemplatePart::input("client")]),
            })
            .check(ResponseCheck::JsonPathEquals {
                path: "$.json.correlation_id".to_owned(),
                expected: TextTemplate::parts([TemplatePart::step_output(
                    "generate-correlation-id",
                    "correlation_id",
                )]),
            }),
        ],
    }
}

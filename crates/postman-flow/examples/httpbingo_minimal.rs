#[path = "support/compile.rs"]
mod compile;
use futures::StreamExt;
use postman_flow::{
    execute_flow, FlowDefinition, FlowEvent, FlowInputSpec, FlowInputs, FlowSessionEnvironment,
    HttpRequestTemplate, HttpStepDefinition, ResponseCheck, ResponseExport, TemplatePart,
    TextTemplate,
};
use postman_http::request::HttpMethod;
use postman_request::RequestClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let transport = RequestClient::try_new("postman-flow-example/0.1.0")?;
    let events = execute_flow(
        compile::compile_example(&httpbingo_definition())?,
        transport,
        FlowSessionEnvironment::new(FlowInputs::new()),
    )?;
    let mut events = std::pin::pin!(events);
    let mut succeeded = false;

    while let Some(event) = events.next().await {
        let event = event?;
        println!("{event:?}");
        if let FlowEvent::FlowFinished { success, .. } = event {
            succeeded = success;
        }
    }

    if !succeeded {
        return Err("httpbingo Flow failed; inspect the events above".into());
    }
    Ok(())
}

pub(crate) fn httpbingo_definition() -> FlowDefinition {
    FlowDefinition {
        name: "httpbingo-minimal".to_owned(),
        inputs: vec![
            FlowInputSpec::with_default("host", "https://httpbingo.org"),
            FlowInputSpec::with_default("client", "postman-flow-headless"),
        ],
        steps: vec![
            HttpStepDefinition::new(
                "generate-correlation-id",
                "Generate a correlation id",
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
            .export(ResponseExport::json("correlation_id", "$.uuid")),
            HttpStepDefinition::new(
                "echo-http-request",
                "Reuse the correlation id",
                HttpRequestTemplate::new(
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
                ])),
            )
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
        outputs: Vec::new(),
    }
}

#[test]
fn native_yaml_compiles_to_the_same_plan_as_the_rust_definition() {
    let document =
        postman_flow::parse_flow_yaml(include_str!("flows/httpbingo_minimal.http.yml")).unwrap();
    let environment = postman_flow::CompileEnvironment::default();
    let native = postman_flow::compile_flow(&document.flow, &document.apis, &environment).unwrap();
    let constructed = postman_flow::compile_flow(
        &httpbingo_definition(),
        &postman_flow::ApiCatalog::new(),
        &environment,
    )
    .unwrap();
    assert_eq!(native, constructed);
    let saved = postman_flow::write_flow_yaml(&document).unwrap();
    assert_eq!(postman_flow::parse_flow_yaml(&saved).unwrap(), document);
}

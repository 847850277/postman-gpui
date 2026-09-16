//! Library-oriented example for one native .http.yml document.
//! The product entry point is `postman-g run` from postman-cli.
use std::{collections::HashSet, env, fs};

use futures::StreamExt;
use postman_flow::{
    compile_flow, execute_flow, parse_flow_yaml, CompileEnvironment, FlowEvent, FlowInputs,
    FlowSessionEnvironment,
};
use postman_http::request::RequestOptions;
use postman_request::RequestClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("postman_flow=debug,info")),
        )
        .init();

    let mut args = env::args().skip(1);
    let path = args
        .next()
        .ok_or("usage: run_yaml FILE.http.yml [--check] [--input NAME=JSON_OR_TEXT]...")?;
    let mut check_only = false;
    let mut inputs = FlowInputs::new();
    let mut names = HashSet::new();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--check" => check_only = true,
            "--input" => {
                let binding = args.next().ok_or("--input requires NAME=JSON_OR_TEXT")?;
                let (name, value) = binding
                    .split_once('=')
                    .ok_or("--input requires NAME=JSON_OR_TEXT")?;
                if name.is_empty() || !names.insert(name.to_owned()) {
                    return Err("input names must be nonempty and cannot be repeated".into());
                }
                let value = serde_json::from_str(value)
                    .unwrap_or_else(|_| serde_json::Value::String(value.into()));
                inputs.insert(name, value);
            }
            _ => return Err(format!("unknown option '{argument}'").into()),
        }
    }
    let document =
        parse_flow_yaml(&fs::read_to_string(&path)?).map_err(|error| format!("{path}: {error}"))?;
    let plan = compile_flow(
        &document.flow,
        &document.apis,
        &CompileEnvironment::default(),
    )
    .map_err(|errors| {
        errors
            .iter()
            .map(|error| format!("{path}: {error}"))
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    if check_only {
        println!("OK: {} ({} steps)", plan.name(), plan.step_count());
        return Ok(());
    }
    let transport = RequestClient::try_new("postman-flow-yaml/0.1.0")?;
    let session = FlowSessionEnvironment::new(inputs).with_request_options(RequestOptions {
        timeout_ms: Some(15_000),
        ..RequestOptions::default()
    });
    let events = execute_flow(plan, transport, session)?;
    let mut events = std::pin::pin!(events);
    let mut success = false;
    while let Some(event) = events.next().await {
        let event = event?;
        println!("{event:?}");
        if let FlowEvent::FlowFinished {
            success: finished, ..
        } = event
        {
            success = finished;
        }
    }
    if success {
        Ok(())
    } else {
        Err("flow failed; inspect the events above".into())
    }
}

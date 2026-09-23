use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

use futures::StreamExt;
use postman_flow::{
    compile_flow, execute_flow, parse_flow_yaml, CompileEnvironment, FlowInputs, FlowPlan,
    FlowSessionEnvironment,
};
use postman_http::{request::RequestOptions, HttpTransport};
use serde_json::Value;

use crate::{report::ReportBuilder, LoopProgress, RunReport};

#[derive(Debug, Clone)]
pub struct FlowCheckReport {
    pub name: String,
    pub step_count: usize,
}

pub fn check_flow(source: &str, path: &Path) -> Result<FlowCheckReport, String> {
    let document =
        parse_flow_yaml(source).map_err(|error| format!("{}: {error}", path.display()))?;
    let plan = compile_flow(
        &document.flow,
        &document.apis,
        &CompileEnvironment::default(),
    )
    .map_err(|errors| {
        errors
            .iter()
            .map(|error| format!("{}: {error}", path.display()))
            .collect::<Vec<_>>()
            .join("\n")
    })?;

    Ok(FlowCheckReport {
        name: plan.name().to_string(),
        step_count: plan.step_count(),
    })
}

pub async fn run_flow<T: HttpTransport>(
    transport: T,
    path: &Path,
    source: &str,
    variables: &BTreeMap<String, String>,
    options: RequestOptions,
) -> Result<RunReport, String> {
    run_flow_with_progress(transport, path, source, variables, options, |_| {}).await
}

/// Run a native flow and observe loop progress without changing the JSON report.
pub async fn run_flow_with_progress<T: HttpTransport>(
    transport: T,
    path: &Path,
    source: &str,
    variables: &BTreeMap<String, String>,
    options: RequestOptions,
    progress: impl FnMut(LoopProgress<'_>),
) -> Result<RunReport, String> {
    let document =
        parse_flow_yaml(source).map_err(|error| format!("{}: {error}", path.display()))?;
    let plan = compile_flow(
        &document.flow,
        &document.apis,
        &CompileEnvironment::default(),
    )
    .map_err(|errors| {
        errors
            .iter()
            .map(|error| format!("{}: {error}", path.display()))
            .collect::<Vec<_>>()
            .join("\n")
    })?;

    let mut inputs = FlowInputs::new();
    for (name, value) in variables {
        inputs.insert(name, parse_flow_input_value(value));
    }

    execute_plan(
        transport,
        plan,
        inputs,
        options,
        ReportBuilder::new(&document.flow.steps),
        progress,
    )
    .await
    .map_err(|error| format!("{}: {error}", path.display()))
}

pub async fn run_flow_plan<T: HttpTransport>(
    transport: T,
    plan: FlowPlan,
    inputs: FlowInputs,
    options: RequestOptions,
) -> Result<RunReport, String> {
    execute_plan(
        transport,
        plan,
        inputs,
        options,
        ReportBuilder::default(),
        |_| {},
    )
    .await
}

async fn execute_plan<T: HttpTransport>(
    transport: T,
    plan: FlowPlan,
    inputs: FlowInputs,
    options: RequestOptions,
    mut report: ReportBuilder,
    mut progress: impl FnMut(LoopProgress<'_>),
) -> Result<RunReport, String> {
    let declared: HashSet<&str> = plan
        .inputs()
        .iter()
        .map(|input| input.name.as_str())
        .collect();
    let inputs = inputs.declared_only(|name| declared.contains(name));
    let session = FlowSessionEnvironment::new(inputs).with_request_options(options);
    let events = execute_flow(plan, transport, session)
        .map_err(|error| format!("failed to start flow: {error}"))?;
    let mut events = std::pin::pin!(events);

    while let Some(event) = events.next().await {
        report.record(event.map_err(|error| error.to_string())?, &mut progress);
    }
    Ok(report.finish())
}

/// CLI `--input` keeps explicit JSON types, but does not coerce `00123` into `123`.
pub(crate) fn parse_flow_input_value(value: &str) -> Value {
    serde_json::from_str::<Value>(value).unwrap_or_else(|_| Value::String(value.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::parse_flow_input_value;
    use serde_json::json;

    #[test]
    fn flow_inputs_keep_json_types_without_eating_leading_zeros() {
        assert_eq!(parse_flow_input_value("100"), json!(100));
        assert_eq!(parse_flow_input_value("1.5"), json!(1.5));
        assert_eq!(parse_flow_input_value("1e3"), json!(1000.0));
        assert_eq!(parse_flow_input_value("-42.5"), json!(-42.5));
        assert_eq!(parse_flow_input_value("true"), json!(true));
        assert_eq!(parse_flow_input_value("null"), json!(null));
        assert_eq!(parse_flow_input_value(r#""00123""#), json!("00123"));
        assert_eq!(parse_flow_input_value("00123"), json!("00123"));
        assert_eq!(
            parse_flow_input_value("https://example.com"),
            json!("https://example.com")
        );
    }
}

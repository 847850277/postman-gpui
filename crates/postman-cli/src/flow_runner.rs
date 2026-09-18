use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

use futures::StreamExt;
use postman_flow::{
    compile_flow, execute_flow, parse_flow_yaml, CompileEnvironment, FlowEvent, FlowInputs,
    FlowPlan, FlowSessionEnvironment, StepOutcome,
};
use postman_http::{request::RequestOptions, HttpTransport};
use serde_json::Value;

use crate::{AssertionReport, RequestReport, RunReport};

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

    run_flow_plan(transport, plan, inputs, options)
        .await
        .map_err(|error| format!("{}: {error}", path.display()))
}

pub async fn run_flow_plan<T: HttpTransport>(
    transport: T,
    plan: FlowPlan,
    inputs: FlowInputs,
    options: RequestOptions,
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

    let mut requests = Vec::new();
    let mut current_request: Option<RequestReport> = None;
    let mut flow_success = false;
    let mut reported_outputs = BTreeMap::new();
    let mut redacted_outputs = Vec::new();

    while let Some(event) = events.next().await {
        let event = event.map_err(|error| format!("{error}"))?;
        match event {
            FlowEvent::StepStarted { step_id, name } => {
                let display_name = if name.is_empty() || name == step_id {
                    step_id
                } else {
                    format!("{step_id} ({name})")
                };
                current_request = Some(RequestReport {
                    name: display_name,
                    success: false,
                    skipped: false,
                    status: None,
                    elapsed_ms: None,
                    assertions: Vec::new(),
                    captures: Vec::new(),
                    error: None,
                });
            }
            FlowEvent::StepSkipped {
                step_id,
                name,
                reason,
            } => {
                let display_name = if name.is_empty() || name == step_id {
                    step_id
                } else {
                    format!("{step_id} ({name})")
                };
                requests.push(RequestReport {
                    name: display_name,
                    success: true,
                    skipped: true,
                    status: None,
                    elapsed_ms: None,
                    assertions: Vec::new(),
                    captures: Vec::new(),
                    error: Some(format!("Skipped: {reason}")),
                });
            }
            FlowEvent::ResponseReceived {
                status, elapsed_ms, ..
            } => {
                if let Some(req) = current_request.as_mut() {
                    req.status = Some(status);
                    req.elapsed_ms = Some(elapsed_ms);
                }
            }
            FlowEvent::CheckFinished {
                check,
                success,
                message,
                ..
            } => {
                if let Some(req) = current_request.as_mut() {
                    req.assertions.push(AssertionReport {
                        expression: check,
                        success,
                        message,
                    });
                }
            }
            FlowEvent::OutputExported { name, .. } => {
                if let Some(req) = current_request.as_mut() {
                    req.captures.push(name);
                }
            }
            FlowEvent::StepFinished { outcome, .. } => {
                if let Some(mut req) = current_request.take() {
                    match outcome {
                        StepOutcome::Succeeded => {
                            req.success = true;
                        }
                        StepOutcome::Failed { message } => {
                            req.success = false;
                            req.error = Some(message);
                        }
                    }
                    requests.push(req);
                }
            }
            FlowEvent::FlowFinished { success, outputs } => {
                flow_success = success;
                if success {
                    for (name, output) in outputs {
                        let value = if output.is_sensitive() {
                            redacted_outputs.push(name.clone());
                            Value::String("[REDACTED]".into())
                        } else {
                            output.value().clone()
                        };
                        reported_outputs.insert(name, value);
                    }
                }
            }
            _ => {}
        }
    }

    Ok(RunReport {
        success: flow_success,
        requests,
        outputs: reported_outputs,
        redacted_outputs,
    })
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

//! Literal presentation only. Native expressions are handled by postman-flow's document codec.
use super::model::literal_url;
use postman_flow::{FlowDefinition, TextTemplate};

pub(super) fn parse_headers(source: &str) -> Result<Vec<(TextTemplate, TextTemplate)>, String> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.is_empty())
        .map(|(index, line)| {
            let (name, value) = line
                .split_once(':')
                .ok_or_else(|| format!("Line {}: expected Name: value.", index + 1))?;
            // Remove only the formatting space we add after ':', never trim the value itself.
            Ok((
                TextTemplate::literal(name),
                TextTemplate::literal(value.strip_prefix(' ').unwrap_or(value)),
            ))
        })
        .collect()
}

pub(super) fn simple_headers(headers: &[(TextTemplate, TextTemplate)]) -> Option<String> {
    let source = headers
        .iter()
        .map(|(name, value)| {
            let name = literal_url(name)?;
            let value = literal_url(value)?;
            if name.contains([':', '\r', '\n']) || value.contains(['\r', '\n']) {
                return None;
            }
            Some(format!("{name}: {value}"))
        })
        .collect::<Option<Vec<_>>>()?
        .join("\n");
    // Do not change even unusual native shapes (e.g. an empty concat) by switching modes.
    (parse_headers(&source).ok()?.as_slice() == headers).then_some(source)
}

/// Names only: never clone defaults, sensitive values, runtime inputs or later-step exports.
pub(super) fn variables(flow: &FlowDefinition, step_id: &str) -> Vec<String> {
    let mut names: Vec<_> = flow
        .inputs
        .iter()
        .map(|input| format!("inputs.{}", input.name))
        .collect();
    for step in flow.steps.iter().take_while(|step| step.id != step_id) {
        match &step.request {
            postman_flow::HttpRequestSource::ForEach(loop_step) => names.extend(
                loop_step
                    .collect
                    .iter()
                    .map(|export| format!("steps.{}.{}", step.id, export.name)),
            ),
            postman_flow::HttpRequestSource::RepeatUntil(_) => {}
            _ => names.extend(
                step.exports
                    .iter()
                    .map(|export| format!("steps.{}.{}", step.id, export.name)),
            ),
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_headers_are_ordered_lossless_and_invalid_lines_do_not_clear() {
        let source = "X-重复: first:second\nX-重复: \nEmpty:\nSpaces:   padded  \n";
        let headers = parse_headers(source).unwrap();
        assert_eq!(headers.len(), 4);
        assert_eq!(headers[0].1, TextTemplate::literal("first:second"));
        assert_eq!(headers[1].1, TextTemplate::literal(""));
        assert_eq!(headers[3].1, TextTemplate::literal("  padded  "));
        assert_eq!(
            parse_headers(&simple_headers(&headers).unwrap()).unwrap(),
            headers
        );
        assert!(parse_headers("Missing separator").is_err());
        assert!(parse_headers("  ").is_err());
        assert!(parse_headers("").unwrap().is_empty());
        assert!(
            simple_headers(&[(TextTemplate::literal("a:b"), TextTemplate::literal("v"))]).is_none()
        );
        assert!(
            simple_headers(&[(TextTemplate::literal("a"), TextTemplate::literal("a\nb"))])
                .is_none()
        );
        assert!(simple_headers(&[(
            TextTemplate::parts([postman_flow::TemplatePart::Input("name".into())]),
            TextTemplate::literal("v")
        )])
        .is_none());
    }
    #[test]
    fn loop_hints_expose_collection_exports_but_not_nested_step_outputs() {
        let document = postman_flow::parse_flow_yaml(include_str!(
            "../../../../crates/postman-flow/examples/flows/httpbingo_loops.http.yml"
        ))
        .unwrap();
        assert_eq!(
            variables(&document.flow, "wait-for-known-document"),
            ["inputs.host", "steps.collect-items.urls"]
        );
    }
    #[test]
    fn hints_only_expose_declared_names_and_preceding_export_ids() {
        let document = postman_flow::parse_flow_yaml(include_str!(
            "../../../../crates/postman-flow/examples/flows/httpbingo_catalog.http.yml"
        ))
        .unwrap();
        assert_eq!(
            variables(&document.flow, "seed"),
            ["inputs.host", "inputs.client", "inputs.optional"]
        );
        assert_eq!(
            variables(&document.flow, "echo"),
            [
                "inputs.host",
                "inputs.client",
                "inputs.optional",
                "steps.seed.correlation_id"
            ]
        );
    }
}

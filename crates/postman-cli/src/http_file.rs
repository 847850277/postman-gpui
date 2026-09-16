use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use postman_flow::{
    BodyTemplate, FlowDefinition, FlowInputSpec, FlowOutputSpec, HttpRequestTemplate,
    HttpStepDefinition, ResponseCheck, ResponseExport, TemplatePart, TextTemplate, ValueReference,
};
pub use postman_flow::{ExpectedError, RequestOptionOverrides};
use postman_http::request::{HttpMethod, RedirectPolicy, RequestBody, MAX_REDIRECT_HOPS};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpFile {
    pub variables: BTreeMap<String, String>,
    pub requests: Vec<HttpFileRequest>,
}

impl HttpFile {
    pub fn to_flow_definition(&self) -> Result<FlowDefinition, ParseError> {
        let mut captured = BTreeMap::new();
        let mut referenced_inputs = BTreeSet::new();
        let mut steps = Vec::with_capacity(self.requests.len());
        let mut outputs = BTreeMap::new();

        let mut seen_ids = BTreeSet::new();
        for (i, request) in self.requests.iter().enumerate() {
            let line = request.source_line;
            let step_id = if request.name.is_empty() || seen_ids.contains(&request.name) {
                format!("step_{}", i + 1)
            } else {
                request.name.clone()
            };
            seen_ids.insert(step_id.clone());
            let step_name = step_id.clone();

            let url = parse_text_template(
                &request.url,
                &captured,
                &self.variables,
                &mut referenced_inputs,
                &mut Vec::new(),
                line,
            )?;
            let mut template = HttpRequestTemplate::new(request.method, url);
            for (h_name, h_val) in &request.headers {
                template = template.header(
                    parse_text_template(
                        h_name,
                        &captured,
                        &self.variables,
                        &mut referenced_inputs,
                        &mut Vec::new(),
                        line,
                    )?,
                    parse_text_template(
                        h_val,
                        &captured,
                        &self.variables,
                        &mut referenced_inputs,
                        &mut Vec::new(),
                        line,
                    )?,
                );
            }

            template.body = match &request.body {
                RequestBody::None => BodyTemplate::None,
                RequestBody::Json(raw) => BodyTemplate::Json(parse_text_template(
                    raw,
                    &captured,
                    &self.variables,
                    &mut referenced_inputs,
                    &mut Vec::new(),
                    line,
                )?),
                RequestBody::Raw(raw) => BodyTemplate::Raw(parse_text_template(
                    raw,
                    &captured,
                    &self.variables,
                    &mut referenced_inputs,
                    &mut Vec::new(),
                    line,
                )?),
                RequestBody::UrlEncoded(raw) => BodyTemplate::UrlEncoded(parse_text_template(
                    raw,
                    &captured,
                    &self.variables,
                    &mut referenced_inputs,
                    &mut Vec::new(),
                    line,
                )?),
                RequestBody::Multipart(_) => {
                    return Err(ParseError::new(
                        line,
                        "parsed multipart parts are not supported by the first headless slice",
                    ));
                }
            };

            template.options = request.options;

            let mut step = HttpStepDefinition::new(&step_id, step_name, template);

            for assertion in &request.assertions {
                let check = match assertion {
                    Assertion::StatusEquals(status) => ResponseCheck::StatusEquals(*status),
                    Assertion::RedirectsEquals(count) => ResponseCheck::RedirectsEquals(*count),
                    Assertion::ErrorEquals(err) => ResponseCheck::ErrorEquals(*err),
                    Assertion::HeaderExists { name } => {
                        ResponseCheck::HeaderExists { name: name.clone() }
                    }
                    Assertion::HeaderContains { name, expected } => {
                        let expected_str = strip_surrounding_quotes(expected);
                        ResponseCheck::HeaderContains {
                            name: name.clone(),
                            expected: parse_text_template(
                                expected_str,
                                &captured,
                                &self.variables,
                                &mut referenced_inputs,
                                &mut Vec::new(),
                                line,
                            )?,
                        }
                    }
                    Assertion::BodyContains { expected } => {
                        let expected_str = strip_surrounding_quotes(expected);
                        ResponseCheck::BodyContains {
                            expected: parse_text_template(
                                expected_str,
                                &captured,
                                &self.variables,
                                &mut referenced_inputs,
                                &mut Vec::new(),
                                line,
                            )?,
                        }
                    }
                    Assertion::JsonPathEquals { path, expected } => ResponseCheck::JsonPathEquals {
                        path: path.clone(),
                        expected: parse_text_template(
                            expected,
                            &captured,
                            &self.variables,
                            &mut referenced_inputs,
                            &mut Vec::new(),
                            line,
                        )?,
                    },
                };
                step = step.check(check);
            }

            for capture in &request.captures {
                step = step.export(ResponseExport::json(&capture.name, &capture.path));
                captured.insert(
                    capture.name.clone(),
                    (step_id.clone(), capture.name.clone()),
                );
                outputs.insert(
                    capture.name.clone(),
                    FlowOutputSpec {
                        name: capture.name.clone(),
                        value: ValueReference::StepOutput {
                            step_id: step_id.clone(),
                            name: capture.name.clone(),
                        },
                    },
                );
            }

            steps.push(step);
        }

        let mut inputs = Vec::new();
        for (k, v) in &self.variables {
            inputs.push(FlowInputSpec::with_default(
                k.clone(),
                serde_json::Value::String(v.clone()),
            ));
        }
        for ref_input in referenced_inputs {
            if !self.variables.contains_key(&ref_input) {
                inputs.push(FlowInputSpec::required(ref_input));
            }
        }

        Ok(FlowDefinition {
            name: "http_file".to_owned(),
            inputs,
            steps,
            outputs: outputs.into_values().collect(),
        })
    }

    pub fn into_flow_definition(self) -> Result<FlowDefinition, ParseError> {
        self.to_flow_definition()
    }
}

fn strip_surrounding_quotes(s: &str) -> &str {
    let trimmed = s.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    }
}

const MAX_VARIABLE_EXPANSION_DEPTH: usize = 16;

fn parse_text_template(
    template: &str,
    captured: &BTreeMap<String, (String, String)>,
    file_variables: &BTreeMap<String, String>,
    referenced_inputs: &mut BTreeSet<String>,
    expanding: &mut Vec<String>,
    line: usize,
) -> Result<TextTemplate, ParseError> {
    if expanding.len() > MAX_VARIABLE_EXPANSION_DEPTH {
        return Err(ParseError::new(
            line,
            "variable interpolation exceeded 16 expansions (possible cycle)",
        ));
    }

    let mut parts = Vec::new();
    let mut remaining = template;

    while let Some(open) = remaining.find("{{") {
        if open > 0 {
            parts.push(TemplatePart::Literal(remaining[..open].to_owned()));
        }
        let after_open = &remaining[open + 2..];
        let Some(close) = after_open.find("}}") else {
            return Err(ParseError::new(line, "unclosed variable expression"));
        };
        let name = after_open[..close].trim();
        if name.is_empty() {
            return Err(ParseError::new(line, "empty variable expression"));
        }

        if let Some((step_id, export_name)) = captured.get(name) {
            parts.push(TemplatePart::StepOutput {
                step_id: step_id.clone(),
                name: export_name.clone(),
            });
        } else if expanding.iter().any(|current| current == name) {
            let mut cycle = expanding.clone();
            cycle.push(name.to_owned());
            return Err(ParseError::new(
                line,
                format!(
                    "variable interpolation contains a cycle: {}",
                    cycle.join(" -> ")
                ),
            ));
        } else if let Some(value) = file_variables.get(name) {
            if value.contains("{{") {
                expanding.push(name.to_owned());
                let nested = parse_text_template(
                    value,
                    captured,
                    file_variables,
                    referenced_inputs,
                    expanding,
                    line,
                )?;
                expanding.pop();
                parts.extend(nested.parts);
            } else {
                referenced_inputs.insert(name.to_owned());
                parts.push(TemplatePart::Input(name.to_owned()));
            }
        } else {
            referenced_inputs.insert(name.to_owned());
            parts.push(TemplatePart::Input(name.to_owned()));
        }

        remaining = &after_open[close + 2..];
    }
    if !remaining.is_empty() {
        parts.push(TemplatePart::Literal(remaining.to_owned()));
    }
    if parts.is_empty() {
        parts.push(TemplatePart::Literal(String::new()));
    }
    Ok(TextTemplate::parts(parts))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpFileRequest {
    pub name: String,
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: RequestBody,
    pub assertions: Vec<Assertion>,
    pub captures: Vec<Capture>,
    pub options: RequestOptionOverrides,
    pub source_line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Assertion {
    StatusEquals(u16),
    RedirectsEquals(usize),
    ErrorEquals(ExpectedError),
    HeaderExists { name: String },
    HeaderContains { name: String, expected: String },
    BodyContains { expected: String },
    JsonPathEquals { path: String, expected: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capture {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl ParseError {
    fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

#[derive(Default)]
struct RequestMetadata {
    name: Option<String>,
    assertions: Vec<Assertion>,
    captures: Vec<Capture>,
    options: RequestOptionOverrides,
}

struct Section<'source> {
    title: Option<String>,
    first_line: usize,
    lines: Vec<(usize, &'source str)>,
}

pub fn parse_http_file(source: &str) -> Result<HttpFile, ParseError> {
    let mut sections = Vec::new();
    let mut current = Section {
        title: None,
        first_line: 1,
        lines: Vec::new(),
    };

    for (index, line) in source.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("###") {
            if !current.lines.is_empty() {
                sections.push(current);
            }
            current = Section {
                title: non_empty(title.trim()).map(str::to_owned),
                first_line: line_number + 1,
                lines: Vec::new(),
            };
        } else {
            current.lines.push((line_number, line));
        }
    }
    if !current.lines.is_empty() {
        sections.push(current);
    }

    let mut variables = BTreeMap::new();
    let mut requests = Vec::new();
    for section in sections {
        if let Some(request) = parse_section(&section, &mut variables)? {
            requests.push(request);
        }
    }

    if requests.is_empty() {
        return Err(ParseError::new(1, "the .http file contains no requests"));
    }

    Ok(HttpFile {
        variables,
        requests,
    })
}

fn parse_section(
    section: &Section<'_>,
    variables: &mut BTreeMap<String, String>,
) -> Result<Option<HttpFileRequest>, ParseError> {
    let mut metadata = RequestMetadata::default();
    let mut request_index = None;

    for (index, (line_number, line)) in section.lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(directive) = comment_directive(trimmed) {
            parse_directive(directive, *line_number, &mut metadata)?;
            continue;
        }
        if is_comment(trimmed) {
            continue;
        }
        if trimmed.starts_with('@') {
            let (name, value) = parse_variable(trimmed, *line_number)?;
            variables.insert(name, value);
            continue;
        }
        request_index = Some(index);
        break;
    }

    let Some(request_index) = request_index else {
        if metadata.name.is_some()
            || !metadata.assertions.is_empty()
            || !metadata.captures.is_empty()
            || metadata.options != RequestOptionOverrides::default()
        {
            return Err(ParseError::new(
                section.first_line,
                "request metadata is not followed by an HTTP request",
            ));
        }
        return Ok(None);
    };

    let (source_line, request_line) = section.lines[request_index];
    let (method, url) = parse_request_line(request_line.trim(), source_line)?;
    let mut headers = Vec::new();
    let mut body_start = section.lines.len();
    let mut cursor = request_index + 1;

    while cursor < section.lines.len() {
        let (line_number, line) = section.lines[cursor];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            body_start = cursor + 1;
            break;
        }
        if let Some(directive) = comment_directive(trimmed) {
            parse_directive(directive, line_number, &mut metadata)?;
            cursor += 1;
            continue;
        }
        if is_comment(trimmed) {
            cursor += 1;
            continue;
        }
        if let Some((name, value)) = parse_header(trimmed) {
            headers.push((name.to_owned(), value.to_owned()));
            cursor += 1;
            body_start = cursor;
            continue;
        }
        body_start = cursor;
        break;
    }

    let body_lines = &section.lines[body_start..];
    let body = parse_body(body_lines, &headers)?;
    let name = metadata
        .name
        .or_else(|| section.title.clone())
        .unwrap_or_else(|| format!("{method} request at line {source_line}"));

    Ok(Some(HttpFileRequest {
        name,
        method,
        url,
        headers,
        body,
        assertions: metadata.assertions,
        captures: metadata.captures,
        options: metadata.options,
        source_line,
    }))
}

fn parse_request_line(line: &str, line_number: usize) -> Result<(HttpMethod, String), ParseError> {
    let mut parts = line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| ParseError::new(line_number, "missing HTTP method"))?
        .parse()
        .map_err(|message: String| ParseError::new(line_number, message))?;
    let url = parts
        .next()
        .ok_or_else(|| ParseError::new(line_number, "missing request URL"))?;
    if let Some(version) = parts.next() {
        if !version.starts_with("HTTP/") {
            return Err(ParseError::new(
                line_number,
                format!("unexpected request-line token `{version}`"),
            ));
        }
    }
    if let Some(extra) = parts.next() {
        return Err(ParseError::new(
            line_number,
            format!("unexpected request-line token `{extra}`"),
        ));
    }
    Ok((method, url.to_owned()))
}

fn parse_header(line: &str) -> Option<(&str, &str)> {
    let (name, value) = line.split_once(':')?;
    let name = name.trim();
    if name.is_empty() || name.chars().any(char::is_whitespace) {
        return None;
    }
    Some((name, value.trim()))
}

fn parse_body(
    lines: &[(usize, &str)],
    headers: &[(String, String)],
) -> Result<RequestBody, ParseError> {
    let first = lines.iter().position(|(_, line)| !line.trim().is_empty());
    let Some(first) = first else {
        return Ok(RequestBody::None);
    };
    let last = lines
        .iter()
        .rposition(|(_, line)| !line.trim().is_empty())
        .expect("a first non-empty line guarantees a last line");
    let body_lines = &lines[first..=last];

    if let Some((line_number, _)) = body_lines
        .iter()
        .find(|(_, line)| line.trim_start().starts_with("< "))
    {
        return Err(ParseError::new(
            *line_number,
            "external file bodies (`< path`) are not supported by the first headless slice",
        ));
    }

    let body = body_lines
        .iter()
        .map(|(_, line)| *line)
        .collect::<Vec<_>>()
        .join("\n");
    let content_type = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        .map(|(_, value)| value.to_ascii_lowercase());

    Ok(match content_type.as_deref() {
        Some(value) if value.contains("application/json") || value.contains("+json") => {
            RequestBody::Json(body)
        }
        Some(value) if value.contains("application/x-www-form-urlencoded") => {
            RequestBody::UrlEncoded(body)
        }
        _ => RequestBody::Raw(body),
    })
}

fn parse_variable(line: &str, line_number: usize) -> Result<(String, String), ParseError> {
    let (name, value) = line[1..].split_once('=').ok_or_else(|| {
        ParseError::new(
            line_number,
            "file variables must use `@name = value` syntax",
        )
    })?;
    let name = name.trim();
    validate_variable_name(name, line_number)?;
    Ok((name.to_owned(), value.trim().to_owned()))
}

fn parse_directive(
    directive: &str,
    line_number: usize,
    metadata: &mut RequestMetadata,
) -> Result<(), ParseError> {
    if let Some(name) = directive_value(directive, "@name") {
        let name = name.trim();
        if name.is_empty() {
            return Err(ParseError::new(line_number, "`@name` requires a value"));
        }
        metadata.name = Some(name.to_owned());
        return Ok(());
    }
    if let Some(assertion) = directive_value(directive, "@assert") {
        metadata
            .assertions
            .push(parse_assertion(assertion.trim(), line_number)?);
        return Ok(());
    }
    if let Some(capture) = directive_value(directive, "@capture") {
        metadata
            .captures
            .push(parse_capture(capture.trim(), line_number)?);
        return Ok(());
    }
    if let Some(timeout_ms) = directive_value(directive, "@timeout-ms") {
        let timeout_ms = timeout_ms
            .trim()
            .parse::<u64>()
            .map_err(|_| ParseError::new(line_number, "`@timeout-ms` requires an integer"))?;
        if timeout_ms == 0 {
            return Err(ParseError::new(
                line_number,
                "`@timeout-ms` must be greater than zero",
            ));
        }
        metadata.options.timeout_ms = Some(timeout_ms);
        return Ok(());
    }
    if let Some(policy) = directive_value(directive, "@redirect") {
        metadata.options.redirect_policy = Some(match policy.trim() {
            "follow" => RedirectPolicy::Follow,
            "no-follow" => RedirectPolicy::DoNotFollow,
            _ => {
                return Err(ParseError::new(
                    line_number,
                    "`@redirect` must be `follow` or `no-follow`",
                ))
            }
        });
        return Ok(());
    }
    if let Some(max_hops) = directive_value(directive, "@max-redirects") {
        let max_hops = max_hops
            .trim()
            .parse::<u32>()
            .map_err(|_| ParseError::new(line_number, "`@max-redirects` requires an integer"))?;
        if !(1..=MAX_REDIRECT_HOPS).contains(&max_hops) {
            return Err(ParseError::new(
                line_number,
                format!("`@max-redirects` must be between 1 and {MAX_REDIRECT_HOPS}"),
            ));
        }
        metadata.options.max_redirect_hops = Some(max_hops);
        return Ok(());
    }

    Err(ParseError::new(
        line_number,
        format!("unsupported .http directive `{directive}`"),
    ))
}

fn parse_assertion(assertion: &str, line_number: usize) -> Result<Assertion, ParseError> {
    if let Some(status) = directive_value(assertion, "status") {
        let status = status.trim().strip_prefix("==").ok_or_else(|| {
            ParseError::new(line_number, "status assertions must use `status == CODE`")
        })?;
        let status = status.trim().parse::<u16>().map_err(|_| {
            ParseError::new(line_number, "status assertion code must be an integer")
        })?;
        return Ok(Assertion::StatusEquals(status));
    }

    if let Some(redirects) = directive_value(assertion, "redirects") {
        let redirects = redirects.trim().strip_prefix("==").ok_or_else(|| {
            ParseError::new(
                line_number,
                "redirect assertions must use `redirects == COUNT`",
            )
        })?;
        let redirects = redirects.trim().parse::<usize>().map_err(|_| {
            ParseError::new(line_number, "redirect assertion count must be an integer")
        })?;
        return Ok(Assertion::RedirectsEquals(redirects));
    }

    if let Some(error) = directive_value(assertion, "error") {
        let error = error.trim().strip_prefix("==").ok_or_else(|| {
            ParseError::new(
                line_number,
                "error assertions must use `error == ERROR_KIND`",
            )
        })?;
        let error = match error.trim() {
            "timeout" => ExpectedError::Timeout,
            "redirect-limit" => ExpectedError::RedirectLimit,
            "network" => ExpectedError::Network,
            "invalid-request" => ExpectedError::InvalidRequest,
            "invalid-response" => ExpectedError::InvalidResponse,
            "response-too-large" => ExpectedError::ResponseTooLarge,
            "cancelled" => ExpectedError::Cancelled,
            kind => {
                return Err(ParseError::new(
                    line_number,
                    format!("unsupported expected error kind `{kind}`"),
                ))
            }
        };
        return Ok(Assertion::ErrorEquals(error));
    }

    if let Some(rest) = directive_value(assertion, "jsonpath") {
        let (path, rest) = take_quoted(rest.trim(), line_number, "JSONPath")?;
        let expected = rest.trim().strip_prefix("==").ok_or_else(|| {
            ParseError::new(
                line_number,
                "JSONPath assertions must use `jsonpath \"$.path\" == VALUE`",
            )
        })?;
        let expected = expected.trim();
        if expected.is_empty() {
            return Err(ParseError::new(
                line_number,
                "JSONPath assertion requires an expected value",
            ));
        }
        return Ok(Assertion::JsonPathEquals {
            path,
            expected: expected.to_owned(),
        });
    }

    if let Some(rest) = directive_value(assertion, "header") {
        let (name, rest) = take_quoted(rest.trim(), line_number, "header name")?;
        let operator = rest.trim();
        if operator == "exists" {
            return Ok(Assertion::HeaderExists { name });
        }
        let expected = operator.strip_prefix("contains").ok_or_else(|| {
            ParseError::new(
                line_number,
                "header assertions must use `header \"Name\" exists` or `header \"Name\" contains VALUE`",
            )
        })?;
        let expected = expected.trim();
        if expected.is_empty() {
            return Err(ParseError::new(
                line_number,
                "header assertion requires an expected value",
            ));
        }
        return Ok(Assertion::HeaderContains {
            name,
            expected: expected.to_owned(),
        });
    }

    if let Some(rest) = directive_value(assertion, "body") {
        let expected = rest.trim().strip_prefix("contains").ok_or_else(|| {
            ParseError::new(
                line_number,
                "body assertions must use `body contains VALUE`",
            )
        })?;
        let expected = expected.trim();
        if expected.is_empty() {
            return Err(ParseError::new(
                line_number,
                "body assertion requires an expected value",
            ));
        }
        return Ok(Assertion::BodyContains {
            expected: expected.to_owned(),
        });
    }

    Err(ParseError::new(
        line_number,
        format!("unsupported assertion `{assertion}`"),
    ))
}

fn parse_capture(capture: &str, line_number: usize) -> Result<Capture, ParseError> {
    let (name, expression) = capture.split_once('=').ok_or_else(|| {
        ParseError::new(
            line_number,
            "captures must use `@capture name = jsonpath \"$.path\"`",
        )
    })?;
    let name = name.trim();
    validate_variable_name(name, line_number)?;
    let expression = expression.trim();
    let rest = expression.strip_prefix("jsonpath").ok_or_else(|| {
        ParseError::new(line_number, "only JSONPath response captures are supported")
    })?;
    let (path, trailing) = take_quoted(rest.trim(), line_number, "JSONPath")?;
    if !trailing.trim().is_empty() {
        return Err(ParseError::new(
            line_number,
            "unexpected content after capture JSONPath",
        ));
    }
    Ok(Capture {
        name: name.to_owned(),
        path,
    })
}

fn take_quoted<'source>(
    value: &'source str,
    line_number: usize,
    subject: &str,
) -> Result<(String, &'source str), ParseError> {
    let Some(rest) = value.strip_prefix('"') else {
        return Err(ParseError::new(
            line_number,
            format!("{subject} must be double quoted"),
        ));
    };
    let end = rest
        .find('"')
        .ok_or_else(|| ParseError::new(line_number, format!("unterminated quoted {subject}")))?;
    Ok((rest[..end].to_owned(), &rest[end + 1..]))
}

fn validate_variable_name(name: &str, line_number: usize) -> Result<(), ParseError> {
    if name.is_empty()
        || !name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        })
    {
        return Err(ParseError::new(
            line_number,
            format!("invalid variable name `{name}`"),
        ));
    }
    Ok(())
}

fn comment_directive(line: &str) -> Option<&str> {
    let comment = line
        .strip_prefix('#')
        .or_else(|| line.strip_prefix("//"))?
        .trim();
    comment.starts_with('@').then_some(comment)
}

fn directive_value<'directive>(directive: &'directive str, name: &str) -> Option<&'directive str> {
    let value = directive.strip_prefix(name)?;
    (value.is_empty() || value.starts_with([' ', '\t'])).then_some(value)
}

fn is_comment(line: &str) -> bool {
    line.starts_with('#') || line.starts_with("//")
}

fn non_empty(value: &str) -> Option<&str> {
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_variables_requests_directives_headers_and_typed_bodies() {
        let file = parse_http_file(
            r#"
@api.host = https://example.com

### Create an item
# @name create-item
# @assert status == 201
# @assert header "content-type" contains "application/json"
# @assert jsonpath "$.json.id" == 42
# @capture item_id = jsonpath "$.json.id"
POST {{api.host}}/anything
Content-Type: application/json
X-Scenario: headless

{
  "id": 42
}

### Read it
GET {{api.host}}/anything/{{item_id}} HTTP/1.1
Accept: application/json
"#,
        )
        .expect("the supported .http subset should parse");

        assert_eq!(file.variables["api.host"], "https://example.com");
        assert_eq!(file.requests.len(), 2);
        assert_eq!(file.requests[0].name, "create-item");
        assert_eq!(file.requests[0].method, HttpMethod::POST);
        assert_eq!(file.requests[0].headers.len(), 2);
        assert!(matches!(file.requests[0].body, RequestBody::Json(_)));
        assert_eq!(file.requests[0].assertions.len(), 3);
        assert_eq!(file.requests[0].captures.len(), 1);
        assert_eq!(file.requests[1].name, "Read it");
        assert_eq!(file.requests[1].method, HttpMethod::GET);
        assert_eq!(file.requests[1].body, RequestBody::None);
    }

    #[test]
    fn parses_header_existence_assertions() {
        let file =
            parse_http_file("# @assert header \"etag\" exists\nGET https://example.com/resource\n")
                .expect("header existence assertion should parse");

        assert_eq!(
            file.requests[0].assertions[0],
            Assertion::HeaderExists {
                name: "etag".to_owned()
            }
        );
    }

    #[test]
    fn reports_the_source_line_for_unsupported_external_file_bodies() {
        let error = parse_http_file(
            "### upload\nPOST https://example.com/upload\nContent-Type: text/plain\n\n< ./payload.txt\n",
        )
        .expect_err("unsupported body syntax should fail during parsing");

        assert_eq!(error.line, 5);
        assert!(error.message.contains("external file bodies"));
    }

    #[test]
    fn rejects_metadata_without_a_request() {
        let error = parse_http_file("### orphan\n# @assert status == 200\n")
            .expect_err("orphan metadata cannot be executed");

        assert_eq!(error.line, 2);
        assert!(error.message.contains("not followed"));
    }

    #[test]
    fn similarly_prefixed_unknown_directives_are_not_treated_as_supported_ones() {
        let error = parse_http_file(
            "### request\n# @namespace this-is-not-a-name\nGET https://example.com\n",
        )
        .expect_err("unknown directives should not be accepted by prefix");

        assert_eq!(error.line, 2);
        assert!(error.message.contains("unsupported .http directive"));
    }

    #[test]
    fn parses_per_request_wire_options_and_error_assertions() {
        let file = parse_http_file(
            "### timeout\n# @timeout-ms 125\n# @redirect no-follow\n# @max-redirects 3\n# @assert redirects == 1\n# @assert error == timeout\nGET https://example.com\n",
        )
        .expect("wire behavior directives should parse");
        let request = &file.requests[0];

        assert_eq!(request.options.timeout_ms, Some(125));
        assert_eq!(
            request.options.redirect_policy,
            Some(RedirectPolicy::DoNotFollow)
        );
        assert_eq!(request.options.max_redirect_hops, Some(3));
        assert_eq!(request.assertions[0], Assertion::RedirectsEquals(1));
        assert_eq!(
            request.assertions[1],
            Assertion::ErrorEquals(ExpectedError::Timeout)
        );
    }

    #[test]
    fn rejects_invalid_per_request_wire_options() {
        for (source, expected) in [
            (
                "# @timeout-ms 0\nGET https://example.com\n",
                "greater than zero",
            ),
            ("# @redirect sometimes\nGET https://example.com\n", "follow"),
            ("# @max-redirects 0\nGET https://example.com\n", "between 1"),
        ] {
            let error = parse_http_file(source).expect_err("invalid option must fail parsing");
            assert!(error.message.contains(expected), "{error}");
        }
    }

    #[test]
    fn nested_file_variables_inline_into_the_using_template() {
        let file = parse_http_file(
            "@base = https://example.com\n@url = {{base}}/api\nGET {{url}}/items\n",
        )
        .expect("nested file variables should parse");
        let flow = file
            .to_flow_definition()
            .expect("nested file variables should compile to a flow");
        let url = match &flow.steps[0].request {
            postman_flow::HttpRequestSource::Inline(request) => &request.url,
            postman_flow::HttpRequestSource::Api(_) => panic!("expected an inline request"),
        };
        assert_eq!(
            url.parts,
            [
                TemplatePart::input("base"),
                TemplatePart::literal("/api"),
                TemplatePart::literal("/items"),
            ]
        );
    }

    #[test]
    fn thirty_two_occurrences_of_a_file_variable_do_not_hit_the_nesting_limit() {
        let template = "{{value}}".repeat(32);
        let file = parse_http_file(&format!("@value = x\nGET https://example.com/{template}\n"))
            .expect("repeated interpolation should parse");
        file.to_flow_definition()
            .expect("occurrence count is not expansion depth");
    }

    #[test]
    fn cyclic_file_variables_fail_conversion() {
        let file = parse_http_file("@cycle = {{cycle}}\nGET {{cycle}}\n")
            .expect("cyclic variables should parse as source");
        let error = file
            .to_flow_definition()
            .expect_err("a self-referential variable must not loop forever");
        assert!(error.message.contains("cycle"), "{error}");
    }

    #[test]
    fn unclosed_and_empty_expressions_fail_conversion() {
        let unclosed = parse_http_file("GET https://example.com/{{host\n")
            .expect("unclosed expressions should parse as source");
        let error = unclosed
            .to_flow_definition()
            .expect_err("unclosed interpolation must fail conversion");
        assert!(error.message.contains("unclosed"), "{error}");

        let empty = parse_http_file("GET https://example.com/{{}}\n")
            .expect("empty expressions should parse as source");
        let error = empty
            .to_flow_definition()
            .expect_err("empty interpolation must fail conversion");
        assert!(error.message.contains("empty"), "{error}");
    }

    #[test]
    fn recapturing_the_same_name_keeps_the_latest_step_output() {
        let file = parse_http_file(
            r#"
### first
# @capture id = jsonpath "$.id"
GET https://example.com/one

### second
# @capture id = jsonpath "$.id"
GET https://example.com/two/{{id}}

### third
GET https://example.com/three/{{id}}
"#,
        )
        .expect("recapture fixture should parse");
        let flow = file
            .to_flow_definition()
            .expect("recapture should not produce duplicate flow outputs");
        assert_eq!(flow.outputs.len(), 1);
        assert_eq!(flow.outputs[0].name, "id");
        let url = match &flow.steps[2].request {
            postman_flow::HttpRequestSource::Inline(request) => &request.url,
            postman_flow::HttpRequestSource::Api(_) => panic!("expected an inline request"),
        };
        assert_eq!(
            url.parts,
            [
                TemplatePart::literal("https://example.com/three/"),
                TemplatePart::step_output("second", "id"),
            ]
        );
    }
}

use std::{
    collections::{BTreeMap, HashSet},
    fmt,
};

use crate::{
    json_path::JsonPath,
    plan::{CompiledCheck, CompiledCondition, CompiledExport, HttpStepPlan},
    ApiCatalog, BodyTemplate, ConditionExpr, FlowDefinition, FlowPlan, HttpRequestSource,
    HttpRequestTemplate, JsonTemplate, ResponseCheck, TemplatePart, TextTemplate, ValueReference,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompileEnvironment {
    pub max_steps: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCode {
    InvalidName,
    DuplicateName,
    EmptyFlow,
    StepLimitExceeded,
    UnknownInput,
    UnavailableOutput,
    InvalidJsonPath,
    InvalidRequest,
    UnknownApi,
    MissingArgument,
    UnknownArgument,
    ExpressionTooDeep,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticLocation {
    pub field: String,
    pub step_id: Option<String>,
    pub api_id: Option<String>,
}

impl DiagnosticLocation {
    fn root(field: &str) -> Self {
        Self {
            field: field.into(),
            step_id: None,
            api_id: None,
        }
    }
    fn child(&self, field: impl fmt::Display) -> Self {
        Self {
            field: format!("{}.{field}", self.field),
            ..self.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub location: DiagnosticLocation,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.location.field)?;
        if let Some(id) = &self.location.step_id {
            write!(f, " (step {id})")?;
        }
        if let Some(id) = &self.location.api_id {
            write!(f, " (api {id})")?;
        }
        write!(f, ": {}", self.message)
    }
}

/// Pure compilation: no session values, file reads, HTTP requests or task spawning.
/// The resulting plan owns its source and expanded catalog requests.
pub fn compile_flow(
    source: &FlowDefinition,
    api_catalog: &ApiCatalog,
    environment: &CompileEnvironment,
) -> Result<FlowPlan, Vec<Diagnostic>> {
    let mut compiler = Compiler::default();
    compiler.name(&source.name, &DiagnosticLocation::root("flow.name"));
    if source.steps.is_empty() {
        compiler.error(
            DiagnosticCode::EmptyFlow,
            &DiagnosticLocation::root("flow.steps"),
            "flow must contain at least one HTTP step",
        );
    }
    if environment
        .max_steps
        .is_some_and(|limit| source.steps.len() > limit)
    {
        compiler.error(
            DiagnosticCode::StepLimitExceeded,
            &DiagnosticLocation::root("flow.steps"),
            "flow exceeds the configured step limit",
        );
    }
    let mut inputs = HashSet::new();
    for (index, input) in source.inputs.iter().enumerate() {
        compiler.unique(
            &input.name,
            &mut inputs,
            &DiagnosticLocation::root(&format!("flow.inputs[{index}].name")),
        );
    }
    let mut ids = HashSet::new();
    let mut available = HashSet::new();
    let mut steps = Vec::new();
    for (index, step) in source.steps.iter().enumerate() {
        let location = DiagnosticLocation {
            field: format!("flow.steps[{index}]"),
            step_id: Some(step.id.clone()),
            api_id: None,
        };
        compiler.unique(&step.id, &mut ids, &location.child("id"));
        compiler.name(&step.name, &location.child("name"));
        let request = compiler.expand(
            &step.request,
            api_catalog,
            &inputs,
            &available,
            &location.child("request"),
        );
        if let Some(request) = &request {
            compiler.request(request, &inputs, &available, &location.child("request"));
        }
        let when = step.when.as_ref().and_then(|expr| {
            compiler.condition(expr, &inputs, &available, &location.child("when"), 0)
        });
        let mut checks = Vec::new();
        for (index, check) in step.checks.iter().enumerate() {
            let at = location.child(format!("checks[{index}]"));
            match check {
                ResponseCheck::StatusEquals(value) => {
                    if !(100..=599).contains(value) {
                        compiler.error(
                            DiagnosticCode::InvalidRequest,
                            &at.child("equals"),
                            "HTTP status must be between 100 and 599",
                        );
                    }
                    checks.push(CompiledCheck::Status(*value));
                }
                ResponseCheck::JsonValueEquals { path, expected } => {
                    compiler.json(expected, &inputs, &available, &at.child("equals"), 0);
                    if let Some(path) = compiler.path(path, &at.child("path")) {
                        checks.push(CompiledCheck::JsonValue {
                            path,
                            expected: expected.clone(),
                        });
                    }
                }
                ResponseCheck::HeaderExists { name } => {
                    if name.trim().is_empty() {
                        compiler.error(
                            DiagnosticCode::InvalidRequest,
                            &at.child("name"),
                            "header name cannot be empty",
                        );
                    }
                    checks.push(CompiledCheck::HeaderExists { name: name.clone() });
                }
                ResponseCheck::HeaderContains { name, expected } => {
                    if name.trim().is_empty() {
                        compiler.error(
                            DiagnosticCode::InvalidRequest,
                            &at.child("name"),
                            "header name cannot be empty",
                        );
                    }
                    compiler.text(expected, &inputs, &available, &at.child("equals"));
                    checks.push(CompiledCheck::HeaderContains {
                        name: name.clone(),
                        expected: expected.clone(),
                    });
                }
                ResponseCheck::BodyContains { expected } => {
                    compiler.text(expected, &inputs, &available, &at.child("equals"));
                    checks.push(CompiledCheck::BodyContains {
                        expected: expected.clone(),
                    });
                }
                ResponseCheck::RedirectsEquals(value) => {
                    checks.push(CompiledCheck::Redirects(*value));
                }
                ResponseCheck::ErrorEquals(expected) => {
                    checks.push(CompiledCheck::Error(*expected));
                }
            }
        }
        let mut names = HashSet::new();
        let mut exports = Vec::new();
        for (index, export) in step.exports.iter().enumerate() {
            let at = location.child(format!("exports[{index}]"));
            compiler.unique(&export.name, &mut names, &at.child("name"));
            if let Some(path) = compiler.path(&export.json_path, &at.child("path")) {
                exports.push(CompiledExport {
                    name: export.name.clone(),
                    path,
                    sensitive: export.sensitive,
                });
            }
        }
        available.extend(names.into_iter().map(|name| (step.id.clone(), name)));
        if let Some(request) = request {
            steps.push(HttpStepPlan {
                id: step.id.clone(),
                name: step.name.clone(),
                when,
                request,
                checks,
                exports,
            });
        }
    }
    let mut output_names = HashSet::new();
    for (index, output) in source.outputs.iter().enumerate() {
        let at = DiagnosticLocation::root(&format!("flow.outputs[{index}]"));
        compiler.unique(&output.name, &mut output_names, &at.child("name"));
        compiler.value_reference(&output.value, &inputs, &available, &at.child("value"));
    }
    if compiler.errors.is_empty() {
        Ok(FlowPlan {
            name: source.name.clone(),
            inputs: source.inputs.clone(),
            steps,
            outputs: source.outputs.clone(),
        })
    } else {
        Err(compiler.errors)
    }
}

#[derive(Default)]
struct Compiler {
    errors: Vec<Diagnostic>,
}
type Outputs = HashSet<(String, String)>;

impl Compiler {
    fn error(
        &mut self,
        code: DiagnosticCode,
        location: &DiagnosticLocation,
        message: impl Into<String>,
    ) {
        self.errors.push(Diagnostic {
            code,
            location: location.clone(),
            message: message.into(),
        });
    }
    fn name(&mut self, value: &str, at: &DiagnosticLocation) {
        if value.trim().is_empty() {
            self.error(DiagnosticCode::InvalidName, at, "name cannot be empty");
        }
    }
    fn unique(&mut self, value: &str, names: &mut HashSet<String>, at: &DiagnosticLocation) {
        self.name(value, at);
        if !names.insert(value.into()) {
            self.error(
                DiagnosticCode::DuplicateName,
                at,
                format!("duplicate name '{value}'"),
            );
        }
    }
    fn input(&mut self, name: &str, inputs: &HashSet<String>, at: &DiagnosticLocation) {
        if crate::runtime::is_builtin_variable(name) {
            return;
        }
        if !inputs.contains(name) {
            self.error(
                DiagnosticCode::UnknownInput,
                at,
                format!("undeclared input '{name}'"),
            );
        }
    }
    fn output(&mut self, step: &str, name: &str, outputs: &Outputs, at: &DiagnosticLocation) {
        if !outputs.contains(&(step.into(), name.into())) {
            self.error(
                DiagnosticCode::UnavailableOutput,
                at,
                format!("output '{step}.{name}' must refer to an export of an earlier step"),
            );
        }
    }
    fn text(
        &mut self,
        value: &TextTemplate,
        inputs: &HashSet<String>,
        outputs: &Outputs,
        at: &DiagnosticLocation,
    ) {
        self.text_at_depth(value, inputs, outputs, at, 0);
    }

    fn text_at_depth(
        &mut self,
        value: &TextTemplate,
        inputs: &HashSet<String>,
        outputs: &Outputs,
        at: &DiagnosticLocation,
        depth: usize,
    ) {
        if depth > 64 {
            self.error(
                DiagnosticCode::ExpressionTooDeep,
                at,
                "text expressions may nest at most 64 levels",
            );
            return;
        }
        for (index, part) in value.parts.iter().enumerate() {
            let at = at.child(format!("parts[{index}]"));
            match part {
                TemplatePart::Literal(_) => {}
                TemplatePart::Calc(_) => {}
                TemplatePart::Input(name) => self.input(name, inputs, &at),
                TemplatePart::StepOutput { step_id, name } => {
                    self.output(step_id, name, outputs, &at)
                }
                TemplatePart::Coalesce(candidates) => {
                    if candidates.is_empty() {
                        self.error(
                            DiagnosticCode::InvalidRequest,
                            &at,
                            "coalesce requires at least one candidate",
                        );
                    }
                    for (idx, c) in candidates.iter().enumerate() {
                        let c_at = at.child(format!("coalesce[{idx}]"));
                        self.text_at_depth(c, inputs, outputs, &c_at, depth + 1);
                    }
                }
            }
        }
    }
    fn json(
        &mut self,
        value: &JsonTemplate,
        inputs: &HashSet<String>,
        outputs: &Outputs,
        at: &DiagnosticLocation,
        depth: usize,
    ) {
        if depth > 64 {
            self.error(
                DiagnosticCode::ExpressionTooDeep,
                at,
                "JSON expressions may nest at most 64 levels",
            );
            return;
        }
        match value {
            JsonTemplate::Literal(_) => {}
            JsonTemplate::Calc(_) => {}
            JsonTemplate::String(value) => self.text(value, inputs, outputs, &at.child("string")),
            JsonTemplate::Input(name) => self.input(name, inputs, at),
            JsonTemplate::StepOutput { step_id, name } => self.output(step_id, name, outputs, at),
            JsonTemplate::Coalesce(candidates) => {
                if candidates.is_empty() {
                    self.error(
                        DiagnosticCode::InvalidRequest,
                        at,
                        "coalesce requires at least one candidate",
                    );
                }
                for (idx, candidate) in candidates.iter().enumerate() {
                    self.json(
                        candidate,
                        inputs,
                        outputs,
                        &at.child(format!("coalesce[{idx}]")),
                        depth + 1,
                    );
                }
            }
            JsonTemplate::Object(fields) => {
                for (name, value) in fields {
                    self.json(
                        value,
                        inputs,
                        outputs,
                        &at.child(format!("object[{name:?}]")),
                        depth + 1,
                    );
                }
            }
            JsonTemplate::Array(items) => {
                for (index, value) in items.iter().enumerate() {
                    self.json(
                        value,
                        inputs,
                        outputs,
                        &at.child(format!("array[{index}]")),
                        depth + 1,
                    );
                }
            }
        }
    }
    fn path(&mut self, source: &str, at: &DiagnosticLocation) -> Option<JsonPath> {
        match JsonPath::compile(source) {
            Ok(path) => Some(path),
            Err(message) => {
                self.error(DiagnosticCode::InvalidJsonPath, at, message);
                None
            }
        }
    }
    fn value_reference(
        &mut self,
        value: &ValueReference,
        inputs: &HashSet<String>,
        outputs: &Outputs,
        at: &DiagnosticLocation,
    ) {
        match value {
            ValueReference::Input(name) => self.input(name, inputs, at),
            ValueReference::StepOutput { step_id, name } => self.output(step_id, name, outputs, at),
            ValueReference::Literal(_) => {}
            ValueReference::Coalesce(candidates) => {
                if candidates.is_empty() {
                    self.error(
                        DiagnosticCode::InvalidRequest,
                        at,
                        "coalesce requires at least one candidate",
                    );
                }
                for (idx, candidate) in candidates.iter().enumerate() {
                    self.value_reference(
                        candidate,
                        inputs,
                        outputs,
                        &at.child(format!("coalesce[{idx}]")),
                    );
                }
            }
        }
    }
    fn condition(
        &mut self,
        expr: &ConditionExpr,
        inputs: &HashSet<String>,
        outputs: &Outputs,
        at: &DiagnosticLocation,
        depth: usize,
    ) -> Option<CompiledCondition> {
        if depth > 64 {
            self.error(
                DiagnosticCode::ExpressionTooDeep,
                at,
                "conditions may nest at most 64 levels",
            );
            return None;
        }
        match expr {
            ConditionExpr::Eq(l, r) => {
                self.json(l, inputs, outputs, &at.child("eq[0]"), depth + 1);
                self.json(r, inputs, outputs, &at.child("eq[1]"), depth + 1);
                Some(CompiledCondition::Eq(l.clone(), r.clone()))
            }
            ConditionExpr::Ne(l, r) => {
                self.json(l, inputs, outputs, &at.child("ne[0]"), depth + 1);
                self.json(r, inputs, outputs, &at.child("ne[1]"), depth + 1);
                Some(CompiledCondition::Ne(l.clone(), r.clone()))
            }
            ConditionExpr::Gt(l, r) => {
                self.json(l, inputs, outputs, &at.child("gt[0]"), depth + 1);
                self.json(r, inputs, outputs, &at.child("gt[1]"), depth + 1);
                Some(CompiledCondition::Gt(l.clone(), r.clone()))
            }
            ConditionExpr::Gte(l, r) => {
                self.json(l, inputs, outputs, &at.child("gte[0]"), depth + 1);
                self.json(r, inputs, outputs, &at.child("gte[1]"), depth + 1);
                Some(CompiledCondition::Gte(l.clone(), r.clone()))
            }
            ConditionExpr::Lt(l, r) => {
                self.json(l, inputs, outputs, &at.child("lt[0]"), depth + 1);
                self.json(r, inputs, outputs, &at.child("lt[1]"), depth + 1);
                Some(CompiledCondition::Lt(l.clone(), r.clone()))
            }
            ConditionExpr::Lte(l, r) => {
                self.json(l, inputs, outputs, &at.child("lte[0]"), depth + 1);
                self.json(r, inputs, outputs, &at.child("lte[1]"), depth + 1);
                Some(CompiledCondition::Lte(l.clone(), r.clone()))
            }
            ConditionExpr::In(item, coll) => {
                self.json(item, inputs, outputs, &at.child("in[0]"), depth + 1);
                self.json(coll, inputs, outputs, &at.child("in[1]"), depth + 1);
                Some(CompiledCondition::In(item.clone(), coll.clone()))
            }
            ConditionExpr::And(items) => {
                let compiled = items
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, item)| {
                        self.condition(
                            item,
                            inputs,
                            outputs,
                            &at.child(format!("and[{idx}]")),
                            depth + 1,
                        )
                    })
                    .collect();
                Some(CompiledCondition::And(compiled))
            }
            ConditionExpr::Or(items) => {
                let compiled = items
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, item)| {
                        self.condition(
                            item,
                            inputs,
                            outputs,
                            &at.child(format!("or[{idx}]")),
                            depth + 1,
                        )
                    })
                    .collect();
                Some(CompiledCondition::Or(compiled))
            }
            ConditionExpr::Not(inner) => {
                let compiled =
                    self.condition(inner, inputs, outputs, &at.child("not"), depth + 1)?;
                Some(CompiledCondition::Not(Box::new(compiled)))
            }
        }
    }
    fn request(
        &mut self,
        request: &HttpRequestTemplate,
        inputs: &HashSet<String>,
        outputs: &Outputs,
        at: &DiagnosticLocation,
    ) {
        self.text(&request.url, inputs, outputs, &at.child("url"));
        if literal_text(&request.url).is_some_and(|url| url.trim().is_empty()) {
            self.error(
                DiagnosticCode::InvalidRequest,
                &at.child("url"),
                "request URL is empty",
            );
        }
        for (index, (name, value)) in request.headers.iter().enumerate() {
            self.text(
                name,
                inputs,
                outputs,
                &at.child(format!("headers[{index}].name")),
            );
            self.text(
                value,
                inputs,
                outputs,
                &at.child(format!("headers[{index}].value")),
            );
        }
        let at = at.child("body.value");
        match &request.body {
            BodyTemplate::None => {}
            BodyTemplate::JsonValue(value) => self.json(value, inputs, outputs, &at, 0),
            BodyTemplate::Json(value)
            | BodyTemplate::Raw(value)
            | BodyTemplate::File(value)
            | BodyTemplate::UrlEncoded(value) => {
                self.text(value, inputs, outputs, &at);
                if matches!(&request.body, BodyTemplate::Json(_)) {
                    if let Some(literal) = literal_text(value) {
                        if serde_json::from_str::<serde_json::Value>(&literal).is_err() {
                            self.error(
                                DiagnosticCode::InvalidRequest,
                                &at,
                                "literal JSON template is not valid JSON",
                            );
                        }
                    }
                }
            }
        }
    }

    fn expand(
        &mut self,
        source: &HttpRequestSource,
        catalog: &ApiCatalog,
        inputs: &HashSet<String>,
        outputs: &Outputs,
        at: &DiagnosticLocation,
    ) -> Option<HttpRequestTemplate> {
        let HttpRequestSource::Api(call) = source else {
            let HttpRequestSource::Inline(request) = source else {
                unreachable!()
            };
            return Some(request.clone());
        };
        for (name, value) in &call.bindings {
            self.text(
                value,
                inputs,
                outputs,
                &at.child(format!("bindings[{name:?}]")),
            );
        }
        let Some(definition) = catalog.get(&call.api_id) else {
            self.error(
                DiagnosticCode::UnknownApi,
                &at.child("api"),
                format!("unknown API '{}'", call.api_id),
            );
            return None;
        };
        let start = self.errors.len();
        let api_at = DiagnosticLocation {
            api_id: Some(call.api_id.clone()),
            field: format!("apis[{:?}]", call.api_id),
            ..at.clone()
        };
        self.name(&call.api_id, &at.child("api"));
        let mut parameters = HashSet::new();
        for (index, name) in definition.parameters.iter().enumerate() {
            self.unique(
                name,
                &mut parameters,
                &api_at.child(format!("parameters[{index}]")),
            );
            if !call.bindings.contains_key(name) {
                self.error(
                    DiagnosticCode::MissingArgument,
                    &at.child("bindings"),
                    format!("missing API argument '{name}'"),
                );
            }
        }
        for name in call.bindings.keys() {
            if !parameters.contains(name) {
                self.error(
                    DiagnosticCode::UnknownArgument,
                    &at.child(format!("bindings[{name:?}]")),
                    "unknown API argument",
                );
            }
        }
        self.request(
            &definition.request,
            &parameters,
            &HashSet::new(),
            &api_at.child("request"),
        );
        if self.errors.len() != start {
            return None;
        }
        let mut request = definition.request.clone();
        request.url = substitute_text(&request.url, &call.bindings);
        request.headers = request
            .headers
            .iter()
            .map(|(name, value)| {
                (
                    substitute_text(name, &call.bindings),
                    substitute_text(value, &call.bindings),
                )
            })
            .collect();
        request.body = match &request.body {
            BodyTemplate::None => BodyTemplate::None,
            BodyTemplate::Json(value) => BodyTemplate::Json(substitute_text(value, &call.bindings)),
            BodyTemplate::Raw(value) => BodyTemplate::Raw(substitute_text(value, &call.bindings)),
            BodyTemplate::UrlEncoded(value) => {
                BodyTemplate::UrlEncoded(substitute_text(value, &call.bindings))
            }
            BodyTemplate::JsonValue(value) => {
                BodyTemplate::JsonValue(substitute_json(value, &call.bindings))
            }
            BodyTemplate::File(value) => BodyTemplate::File(substitute_text(value, &call.bindings)),
        };
        Some(request)
    }
}

fn literal_text(value: &TextTemplate) -> Option<String> {
    value
        .parts
        .iter()
        .map(|part| match part {
            TemplatePart::Literal(value) => Some(value.as_str()),
            _ => None,
        })
        .collect()
}

fn substitute_text(
    value: &TextTemplate,
    bindings: &BTreeMap<String, TextTemplate>,
) -> TextTemplate {
    TextTemplate::parts(value.parts.iter().flat_map(|part| match part {
        TemplatePart::Input(name) => bindings[name].parts.clone(),
        TemplatePart::Coalesce(candidates) => vec![TemplatePart::Coalesce(
            candidates
                .iter()
                .map(|candidate| substitute_text(candidate, bindings))
                .collect(),
        )],
        _ => vec![part.clone()],
    }))
}

fn substitute_json(
    value: &JsonTemplate,
    bindings: &BTreeMap<String, TextTemplate>,
) -> JsonTemplate {
    match value {
        JsonTemplate::Input(name) => match bindings[name].parts.as_slice() {
            [TemplatePart::Input(name)] => JsonTemplate::Input(name.clone()),
            [TemplatePart::StepOutput { step_id, name }] => {
                JsonTemplate::step_output(step_id, name)
            }
            _ => JsonTemplate::String(bindings[name].clone()),
        },
        JsonTemplate::String(value) => JsonTemplate::String(substitute_text(value, bindings)),
        JsonTemplate::Coalesce(candidates) => JsonTemplate::Coalesce(
            candidates
                .iter()
                .map(|c| substitute_json(c, bindings))
                .collect(),
        ),
        JsonTemplate::Object(fields) => JsonTemplate::object(
            fields
                .iter()
                .map(|(name, value)| (name, substitute_json(value, bindings))),
        ),
        JsonTemplate::Array(items) => {
            JsonTemplate::array(items.iter().map(|value| substitute_json(value, bindings)))
        }
        _ => value.clone(),
    }
}

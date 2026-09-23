use std::{
    collections::{BTreeMap, HashSet},
    fmt,
};

use crate::{
    json_path::JsonPath,
    plan::{
        CompiledCarry, CompiledCheck, CompiledCollection, CompiledCondition, CompiledExport,
        CompiledRequest, CompiledStepAction, ForEachPlan, HttpStepPlan, RepeatUntilPlan,
    },
    ApiCatalog, AuthTemplate, BodyTemplate, ConditionExpr, FlowDefinition, FlowPlan,
    HttpRequestSource, HttpRequestTemplate, HttpStepDefinition, JsonTemplate, ResponseCheck,
    TemplatePart, TextTemplate, ValueReference,
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
    InvalidExpression,
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
/// The resulting plan owns its source, catalog request snapshots and argument bindings.
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
            "flow must contain at least one step",
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
    compiler.max_steps = environment.max_steps;
    let (steps, available) = compiler.steps(
        &source.steps,
        api_catalog,
        &inputs,
        &HashSet::new(),
        "flow.steps",
        0,
    );
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
    step_count: usize,
    max_steps: Option<usize>,
    step_ids: HashSet<String>,
}
type Outputs = HashSet<(String, String)>;

impl Compiler {
    fn steps(
        &mut self,
        source: &[HttpStepDefinition],
        api_catalog: &ApiCatalog,
        inputs: &HashSet<String>,
        outer_available: &Outputs,
        field: &str,
        loop_depth: usize,
    ) -> (Vec<HttpStepPlan>, Outputs) {
        if source.is_empty() {
            self.error(
                DiagnosticCode::EmptyFlow,
                &DiagnosticLocation::root(field),
                "step group must not be empty",
            );
        }
        let mut available = outer_available.clone();
        let mut plans = Vec::new();
        for (index, step) in source.iter().enumerate() {
            self.step_count += 1;
            if self.max_steps.is_some_and(|limit| self.step_count > limit) {
                self.error(
                    DiagnosticCode::StepLimitExceeded,
                    &DiagnosticLocation::root(field),
                    "flow exceeds the configured step limit",
                );
            }
            let location = DiagnosticLocation {
                field: format!("{field}[{index}]"),
                step_id: Some(step.id.clone()),
                api_id: None,
            };
            self.name(&step.id, &location.child("id"));
            if !self.step_ids.insert(step.id.clone()) {
                self.error(
                    DiagnosticCode::DuplicateName,
                    &location.child("id"),
                    format!("duplicate step id '{}'", step.id),
                );
            }
            self.name(&step.name, &location.child("name"));
            let when = step.when.as_ref().and_then(|expr| {
                self.condition(expr, inputs, &available, &location.child("when"), 0)
            });

            let action = match &step.request {
                HttpRequestSource::Inline(_) | HttpRequestSource::Api(_) => {
                    let request = self.expand(
                        &step.request,
                        api_catalog,
                        inputs,
                        &available,
                        &location.child("request"),
                    );
                    let checks = self.checks(&step.checks, inputs, &available, &location);
                    let (exports, names) = self.exports(&step.exports, &location);
                    available.extend(names.into_iter().map(|name| (step.id.clone(), name)));
                    request.map(|request| CompiledStepAction::Http {
                        request,
                        checks,
                        exports,
                    })
                }
                HttpRequestSource::ForEach(loop_step) => {
                    self.control_fields(step, &location);
                    self.loop_depth(loop_depth, &location);
                    self.loop_limit(loop_step.max_iterations, &location.child("max_iterations"));
                    self.name(&loop_step.item_name, &location.child("as"));
                    if let Some(index_name) = &loop_step.index_name {
                        self.name(index_name, &location.child("index_as"));
                        if index_name == &loop_step.item_name {
                            self.error(
                                DiagnosticCode::DuplicateName,
                                &location.child("index_as"),
                                "index binding must differ from the item binding",
                            );
                        }
                    }
                    self.json(
                        &loop_step.items,
                        inputs,
                        &available,
                        &location.child("items"),
                        0,
                    );
                    let mut child_inputs = inputs.clone();
                    child_inputs.insert(loop_step.item_name.clone());
                    if let Some(index_name) = &loop_step.index_name {
                        child_inputs.insert(index_name.clone());
                    }
                    let (steps, child_available) = self.steps(
                        &loop_step.steps,
                        api_catalog,
                        &child_inputs,
                        &available,
                        &format!("{}.steps", location.field),
                        loop_depth + 1,
                    );
                    let child_outputs = child_available
                        .difference(&available)
                        .cloned()
                        .collect::<Outputs>();
                    let mut names = HashSet::new();
                    let collect = loop_step
                        .collect
                        .iter()
                        .enumerate()
                        .map(|(index, collection)| {
                            let at = location.child(format!("exports[{index}]"));
                            self.unique(&collection.name, &mut names, &at.child("name"));
                            self.output(
                                &collection.step_id,
                                &collection.output,
                                &child_outputs,
                                &at.child("collect"),
                            );
                            CompiledCollection {
                                name: collection.name.clone(),
                                step_id: collection.step_id.clone(),
                                output: collection.output.clone(),
                            }
                        })
                        .collect();
                    available.extend(names.into_iter().map(|name| (step.id.clone(), name)));
                    Some(CompiledStepAction::ForEach(ForEachPlan {
                        items: loop_step.items.clone(),
                        item_name: loop_step.item_name.clone(),
                        index_name: loop_step.index_name.clone(),
                        max_iterations: loop_step.max_iterations,
                        on_error: loop_step.on_error,
                        steps,
                        collect,
                    }))
                }
                HttpRequestSource::RepeatUntil(loop_step) => {
                    self.control_fields(step, &location);
                    self.loop_depth(loop_depth, &location);
                    self.loop_limit(loop_step.max_iterations, &location.child("max_iterations"));
                    if loop_step.interval_ms == 0 || loop_step.interval_ms > 60_000 {
                        self.error(
                            DiagnosticCode::InvalidRequest,
                            &location.child("interval_ms"),
                            "interval_ms must be between 1 and 60000",
                        );
                    }
                    if loop_step.timeout_ms == 0 || loop_step.timeout_ms > 86_400_000 {
                        self.error(
                            DiagnosticCode::InvalidRequest,
                            &location.child("timeout_ms"),
                            "timeout_ms must be between 1 and 86400000",
                        );
                    }
                    let mut child_inputs = inputs.clone();
                    let mut carry_names = HashSet::new();
                    for (index, carry) in loop_step.carry.iter().enumerate() {
                        let at = location.child(format!("carry[{index}]"));
                        self.unique(&carry.name, &mut carry_names, &at.child("name"));
                        self.json(&carry.initial, inputs, &available, &at.child("initial"), 0);
                        if inputs.contains(&carry.name) {
                            self.error(
                                DiagnosticCode::DuplicateName,
                                &at.child("name"),
                                "carry binding must not shadow a flow input",
                            );
                        }
                        child_inputs.insert(carry.name.clone());
                    }
                    let (steps, child_available) = self.steps(
                        &loop_step.steps,
                        api_catalog,
                        &child_inputs,
                        &available,
                        &format!("{}.steps", location.field),
                        loop_depth + 1,
                    );
                    let child_outputs = child_available
                        .difference(&available)
                        .cloned()
                        .collect::<Outputs>();
                    let carry = loop_step
                        .carry
                        .iter()
                        .enumerate()
                        .map(|(index, carry)| {
                            self.output(
                                &carry.step_id,
                                &carry.output,
                                &child_outputs,
                                &location.child(format!("carry[{index}].from")),
                            );
                            CompiledCarry {
                                name: carry.name.clone(),
                                initial: carry.initial.clone(),
                                step_id: carry.step_id.clone(),
                                output: carry.output.clone(),
                            }
                        })
                        .collect();
                    let until = self.condition(
                        &loop_step.until,
                        &child_inputs,
                        &child_available,
                        &location.child("until"),
                        0,
                    );
                    let fail_when = loop_step.fail_when.as_ref().and_then(|condition| {
                        self.condition(
                            condition,
                            &child_inputs,
                            &child_available,
                            &location.child("fail_when"),
                            0,
                        )
                    });
                    until.map(|until| {
                        CompiledStepAction::RepeatUntil(RepeatUntilPlan {
                            max_iterations: loop_step.max_iterations,
                            interval_ms: loop_step.interval_ms,
                            timeout_ms: loop_step.timeout_ms,
                            until,
                            fail_when,
                            carry,
                            steps,
                        })
                    })
                }
            };
            if let Some(action) = action {
                plans.push(HttpStepPlan {
                    id: step.id.clone(),
                    name: step.name.clone(),
                    when,
                    action,
                });
            }
        }
        (plans, available)
    }

    fn control_fields(&mut self, step: &HttpStepDefinition, at: &DiagnosticLocation) {
        if !step.checks.is_empty() || !step.exports.is_empty() {
            self.error(
                DiagnosticCode::InvalidRequest,
                at,
                "control-flow steps cannot define HTTP checks or response exports",
            );
        }
    }

    fn loop_depth(&mut self, depth: usize, at: &DiagnosticLocation) {
        if depth >= 3 {
            self.error(
                DiagnosticCode::InvalidRequest,
                at,
                "loops may nest at most 3 levels",
            );
        }
    }

    fn loop_limit(&mut self, value: usize, at: &DiagnosticLocation) {
        if value == 0 || value > 10_000 {
            self.error(
                DiagnosticCode::InvalidRequest,
                at,
                "max_iterations must be between 1 and 10000",
            );
        }
    }

    fn checks(
        &mut self,
        source: &[ResponseCheck],
        inputs: &HashSet<String>,
        available: &Outputs,
        location: &DiagnosticLocation,
    ) -> Vec<CompiledCheck> {
        let mut checks = Vec::new();
        for (index, check) in source.iter().enumerate() {
            let at = location.child(format!("checks[{index}]"));
            match check {
                ResponseCheck::StatusEquals(value) => {
                    if !(100..=599).contains(value) {
                        self.error(
                            DiagnosticCode::InvalidRequest,
                            &at.child("equals"),
                            "HTTP status must be between 100 and 599",
                        );
                    }
                    checks.push(CompiledCheck::Status(*value));
                }
                ResponseCheck::JsonValueEquals { path, expected } => {
                    self.json(expected, inputs, available, &at.child("equals"), 0);
                    if let Some(path) = self.path(path, &at.child("path")) {
                        checks.push(CompiledCheck::JsonValue {
                            path,
                            expected: expected.clone(),
                        });
                    }
                }
                ResponseCheck::HeaderExists { name } => {
                    if name.trim().is_empty() {
                        self.error(
                            DiagnosticCode::InvalidRequest,
                            &at.child("name"),
                            "header name cannot be empty",
                        );
                    }
                    checks.push(CompiledCheck::HeaderExists { name: name.clone() });
                }
                ResponseCheck::HeaderContains { name, expected } => {
                    if name.trim().is_empty() {
                        self.error(
                            DiagnosticCode::InvalidRequest,
                            &at.child("name"),
                            "header name cannot be empty",
                        );
                    }
                    self.text(expected, inputs, available, &at.child("equals"));
                    checks.push(CompiledCheck::HeaderContains {
                        name: name.clone(),
                        expected: expected.clone(),
                    });
                }
                ResponseCheck::BodyContains { expected } => {
                    self.text(expected, inputs, available, &at.child("equals"));
                    checks.push(CompiledCheck::BodyContains {
                        expected: expected.clone(),
                    });
                }
                ResponseCheck::RedirectsEquals(value) => {
                    checks.push(CompiledCheck::Redirects(*value))
                }
                ResponseCheck::ErrorEquals(expected) => {
                    checks.push(CompiledCheck::Error(*expected))
                }
            }
        }
        checks
    }

    fn exports(
        &mut self,
        source: &[crate::ResponseExport],
        location: &DiagnosticLocation,
    ) -> (Vec<CompiledExport>, HashSet<String>) {
        let mut names = HashSet::new();
        let mut exports = Vec::new();
        for (index, export) in source.iter().enumerate() {
            let at = location.child(format!("exports[{index}]"));
            self.unique(&export.name, &mut names, &at.child("name"));
            if let Some(path) = self.path(&export.json_path, &at.child("path")) {
                exports.push(CompiledExport {
                    name: export.name.clone(),
                    path,
                    sensitive: export.sensitive,
                });
            }
        }
        (exports, names)
    }
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
                TemplatePart::Calc(expr) => self.calc(expr, inputs, outputs, &at),
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
            JsonTemplate::Calc(expr) => self.calc(expr, inputs, outputs, at),
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
    fn calc(
        &mut self,
        source: &str,
        inputs: &HashSet<String>,
        outputs: &Outputs,
        at: &DiagnosticLocation,
    ) {
        match crate::calc::Expression::parse(source) {
            Ok(expression) => {
                for name in expression.variables() {
                    if inputs.contains(name) || crate::runtime::is_builtin_variable(name) {
                        self.input(name, inputs, at);
                    } else if let Some((step, output)) = name.split_once('.') {
                        self.output(step, output, outputs, at);
                    } else {
                        self.input(name, inputs, at);
                    }
                }
            }
            Err(error) => self.error(DiagnosticCode::InvalidExpression, at, error.to_string()),
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
        self.request_literals(request, None, at);
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
        if let Some(AuthTemplate::HmacSha256 { secret, param }) = &request.auth {
            self.text(secret, inputs, outputs, &at.child("auth.secret"));
            if let Err(message) = crate::model::validate_signature_param(param) {
                self.error(
                    DiagnosticCode::InvalidRequest,
                    &at.child("auth.param"),
                    message,
                );
            }
            if !matches!(
                request.body,
                BodyTemplate::None | BodyTemplate::UrlEncoded(_)
            ) {
                self.error(
                    DiagnosticCode::InvalidRequest,
                    &at.child("auth"),
                    "hmac_sha256 signs URL query parameters and an optional url_encoded body",
                );
            }
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
            }
        }
    }

    fn request_literals(
        &mut self,
        request: &HttpRequestTemplate,
        bindings: Option<&BTreeMap<String, TextTemplate>>,
        at: &DiagnosticLocation,
    ) {
        if literal_text(&request.url, bindings).is_some_and(|url| url.trim().is_empty()) {
            self.error(
                DiagnosticCode::InvalidRequest,
                &at.child("url"),
                "request URL is empty",
            );
        }
        if let BodyTemplate::Json(value) = &request.body {
            if let Some(literal) = literal_text(value, bindings) {
                if serde_json::from_str::<serde_json::Value>(&literal).is_err() {
                    self.error(
                        DiagnosticCode::InvalidRequest,
                        &at.child("body.value"),
                        "literal JSON template is not valid JSON",
                    );
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
    ) -> Option<CompiledRequest> {
        if matches!(
            source,
            HttpRequestSource::ForEach(_) | HttpRequestSource::RepeatUntil(_)
        ) {
            unreachable!("control-flow steps are compiled separately");
        }
        let HttpRequestSource::Api(call) = source else {
            let HttpRequestSource::Inline(request) = source else {
                unreachable!()
            };
            self.request(request, inputs, outputs, at);
            return Some(CompiledRequest {
                template: request.clone(),
                bindings: None,
            });
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
        self.request_literals(
            &definition.request,
            Some(&call.bindings),
            &api_at.child("request"),
        );
        Some(CompiledRequest {
            template: definition.request.clone(),
            bindings: Some(call.bindings.clone()),
        })
    }
}

fn literal_text(
    value: &TextTemplate,
    bindings: Option<&BTreeMap<String, TextTemplate>>,
) -> Option<String> {
    value
        .parts
        .iter()
        .map(|part| match part {
            TemplatePart::Literal(value) => Some(value.clone()),
            TemplatePart::Input(name) => literal_text(bindings?.get(name)?, None),
            _ => None,
        })
        .collect()
}

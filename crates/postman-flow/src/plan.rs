use std::collections::BTreeMap;

use crate::{
    json_path::JsonPath, ExpectedError, FlowInputSpec, FlowOutputSpec, HttpRequestTemplate,
    JsonTemplate, LoopErrorPolicy, TextTemplate,
};

/// Owned, statically validated snapshot. Public code cannot construct or modify a plan.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowPlan {
    pub(crate) name: String,
    pub(crate) inputs: Vec<FlowInputSpec>,
    pub(crate) steps: Vec<HttpStepPlan>,
    pub(crate) outputs: Vec<FlowOutputSpec>,
}

impl FlowPlan {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn inputs(&self) -> &[FlowInputSpec] {
        &self.inputs
    }
    pub fn step_count(&self) -> usize {
        self.steps.len()
    }
    pub fn outputs(&self) -> &[FlowOutputSpec] {
        &self.outputs
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HttpStepPlan {
    pub id: String,
    pub name: String,
    pub when: Option<CompiledCondition>,
    pub action: CompiledStepAction,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CompiledStepAction {
    Http {
        request: CompiledRequest,
        checks: Vec<CompiledCheck>,
        exports: Vec<CompiledExport>,
    },
    ForEach(ForEachPlan),
    RepeatUntil(RepeatUntilPlan),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompiledCollection {
    pub name: String,
    pub step_id: String,
    pub output: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ForEachPlan {
    pub items: JsonTemplate,
    pub item_name: String,
    pub index_name: Option<String>,
    pub max_iterations: usize,
    pub on_error: LoopErrorPolicy,
    pub steps: Vec<HttpStepPlan>,
    pub collect: Vec<CompiledCollection>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RepeatUntilPlan {
    pub max_iterations: usize,
    pub interval_ms: u64,
    pub timeout_ms: u64,
    pub until: CompiledCondition,
    pub fail_when: Option<CompiledCondition>,
    pub carry: Vec<CompiledCarry>,
    pub steps: Vec<HttpStepPlan>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompiledCarry {
    pub name: String,
    pub initial: JsonTemplate,
    pub step_id: String,
    pub output: String,
}

/// Catalog bindings keep their caller scope; the template uses only its local parameters.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompiledRequest {
    pub template: HttpRequestTemplate,
    pub bindings: Option<BTreeMap<String, TextTemplate>>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CompiledCondition {
    Eq(JsonTemplate, JsonTemplate),
    Ne(JsonTemplate, JsonTemplate),
    Gt(JsonTemplate, JsonTemplate),
    Gte(JsonTemplate, JsonTemplate),
    Lt(JsonTemplate, JsonTemplate),
    Lte(JsonTemplate, JsonTemplate),
    In(JsonTemplate, JsonTemplate),
    And(Vec<CompiledCondition>),
    Or(Vec<CompiledCondition>),
    Not(Box<CompiledCondition>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CompiledCheck {
    Status(u16),
    JsonValue {
        path: JsonPath,
        expected: JsonTemplate,
    },
    HeaderExists {
        name: String,
    },
    HeaderContains {
        name: String,
        expected: TextTemplate,
    },
    BodyContains {
        expected: TextTemplate,
    },
    Redirects(usize),
    Error(ExpectedError),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompiledExport {
    pub name: String,
    pub path: JsonPath,
    pub sensitive: bool,
}

use std::collections::BTreeMap;

use crate::{
    json_path::JsonPath, ExpectedError, FlowInputSpec, FlowOutputSpec, HttpRequestTemplate,
    JsonTemplate, SqlQueryTemplate, TextTemplate,
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
    pub request: CompiledRequest,
    pub checks: Vec<CompiledCheck>,
    pub exports: Vec<CompiledExport>,
}

/// Catalog bindings keep their caller scope; the template uses only its local parameters.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CompiledRequest {
    Http {
        template: HttpRequestTemplate,
        bindings: Option<BTreeMap<String, TextTemplate>>,
    },
    Sql(SqlQueryTemplate),
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

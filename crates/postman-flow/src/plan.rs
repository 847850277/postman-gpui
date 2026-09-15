use crate::{
    json_path::JsonPath, FlowInputSpec, FlowOutputSpec, HttpRequestTemplate, JsonTemplate,
    TextTemplate,
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
    pub request: HttpRequestTemplate,
    pub checks: Vec<CompiledCheck>,
    pub exports: Vec<CompiledExport>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CompiledCheck {
    Status(u16),
    JsonText {
        path: JsonPath,
        expected: TextTemplate,
    },
    JsonValue {
        path: JsonPath,
        expected: JsonTemplate,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompiledExport {
    pub name: String,
    pub path: JsonPath,
    pub sensitive: bool,
}

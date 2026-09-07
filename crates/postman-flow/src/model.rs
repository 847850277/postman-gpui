use std::collections::BTreeMap;

use postman_http::request::{HttpMethod, RequestBody};
use serde_json::Value;

/// A validated run receives values for these named inputs. `None` means the input is required.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowInputSpec {
    pub name: String,
    pub default: Option<Value>,
    pub sensitive: bool,
}

impl FlowInputSpec {
    pub fn required(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            default: None,
            sensitive: false,
        }
    }

    pub fn with_default(name: impl Into<String>, default: impl Into<Value>) -> Self {
        Self {
            name: name.into(),
            default: Some(default.into()),
            sensitive: false,
        }
    }

    pub fn sensitive(mut self) -> Self {
        self.sensitive = true;
        self
    }
}

/// Immutable values supplied when one Flow run starts.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FlowInputs {
    pub(crate) values: BTreeMap<String, Value>,
}

impl FlowInputs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<Value>) {
        self.values.insert(name.into(), value.into());
    }

    pub fn with(mut self, name: impl Into<String>, value: impl Into<Value>) -> Self {
        self.insert(name, value);
        self
    }
}

/// This minimal plan is deliberately a sequence. Control-flow nodes come after the data-flow
/// contract has proven stable.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowPlan {
    pub name: String,
    pub inputs: Vec<FlowInputSpec>,
    pub steps: Vec<HttpStepPlan>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HttpStepPlan {
    pub id: String,
    pub name: String,
    pub method: HttpMethod,
    pub url: TextTemplate,
    pub headers: Vec<(TextTemplate, TextTemplate)>,
    pub body: BodyTemplate,
    pub checks: Vec<ResponseCheck>,
    pub exports: Vec<ResponseExport>,
}

impl HttpStepPlan {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        method: HttpMethod,
        url: TextTemplate,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            method,
            url,
            headers: Vec::new(),
            body: BodyTemplate::None,
            checks: Vec::new(),
            exports: Vec::new(),
        }
    }

    pub fn header(mut self, name: TextTemplate, value: TextTemplate) -> Self {
        self.headers.push((name, value));
        self
    }

    pub fn json_body(mut self, body: TextTemplate) -> Self {
        self.body = BodyTemplate::Json(body);
        self
    }

    pub fn check(mut self, check: ResponseCheck) -> Self {
        self.checks.push(check);
        self
    }

    pub fn export(mut self, export: ResponseExport) -> Self {
        self.exports.push(export);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BodyTemplate {
    None,
    Json(TextTemplate),
    Raw(TextTemplate),
    UrlEncoded(TextTemplate),
}

impl BodyTemplate {
    pub(crate) fn render(
        &self,
        render: impl Fn(&TextTemplate) -> Result<String, String>,
    ) -> Result<RequestBody, String> {
        Ok(match self {
            Self::None => RequestBody::None,
            Self::Json(template) => RequestBody::Json(render(template)?),
            Self::Raw(template) => RequestBody::Raw(render(template)?),
            Self::UrlEncoded(template) => RequestBody::UrlEncoded(render(template)?),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResponseCheck {
    StatusEquals(u16),
    JsonPathEquals {
        path: String,
        expected: TextTemplate,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseExport {
    pub name: String,
    pub json_path: String,
    pub sensitive: bool,
}

impl ResponseExport {
    pub fn json(name: impl Into<String>, json_path: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            json_path: json_path.into(),
            sensitive: false,
        }
    }

    pub fn sensitive(mut self) -> Self {
        self.sensitive = true;
        self
    }
}

/// Templates contain explicit references. There is no string lookup ambiguity inside FlowPlan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextTemplate {
    pub parts: Vec<TemplatePart>,
}

impl TextTemplate {
    pub fn literal(value: impl Into<String>) -> Self {
        Self {
            parts: vec![TemplatePart::Literal(value.into())],
        }
    }

    pub fn parts(parts: impl IntoIterator<Item = TemplatePart>) -> Self {
        Self {
            parts: parts.into_iter().collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplatePart {
    Literal(String),
    Input(String),
    StepOutput { step_id: String, name: String },
}

impl TemplatePart {
    pub fn literal(value: impl Into<String>) -> Self {
        Self::Literal(value.into())
    }

    pub fn input(name: impl Into<String>) -> Self {
        Self::Input(name.into())
    }

    pub fn step_output(step_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self::StepOutput {
            step_id: step_id.into(),
            name: name.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepOutcome {
    Succeeded,
    Failed { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowEvent {
    FlowStarted {
        name: String,
        total_steps: usize,
    },
    StepStarted {
        step_id: String,
        name: String,
    },
    ResponseReceived {
        step_id: String,
        status: u16,
        elapsed_ms: u128,
    },
    CheckFinished {
        step_id: String,
        check: String,
        success: bool,
        message: Option<String>,
    },
    OutputExported {
        step_id: String,
        name: String,
    },
    StepFinished {
        step_id: String,
        outcome: StepOutcome,
    },
    FlowFinished {
        success: bool,
    },
}

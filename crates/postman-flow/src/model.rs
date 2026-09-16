use std::{collections::BTreeMap, fmt};

use postman_http::request::{HttpMethod, RedirectPolicy, RequestBody};
use serde::{Deserialize, Serialize};
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

    /// Drop bindings the plan did not declare so a mixed suite can share CLI flags.
    pub fn declared_only(&self, declared: impl Fn(&str) -> bool) -> Self {
        Self {
            values: self
                .values
                .iter()
                .filter(|(name, _)| declared(name.as_str()))
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect(),
        }
    }
}

/// Editable, format-independent source. Only compilation makes it executable.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowDefinition {
    pub name: String,
    pub inputs: Vec<FlowInputSpec>,
    pub steps: Vec<HttpStepDefinition>,
    pub outputs: Vec<FlowOutputSpec>,
}

impl FlowDefinition {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            inputs: Vec::new(),
            steps: Vec::new(),
            outputs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HttpStepDefinition {
    pub id: String,
    pub name: String,
    pub request: HttpRequestSource,
    pub checks: Vec<ResponseCheck>,
    pub exports: Vec<ResponseExport>,
}

impl HttpStepDefinition {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        request: impl Into<HttpRequestSource>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            request: request.into(),
            checks: Vec::new(),
            exports: Vec::new(),
        }
    }

    pub fn check(mut self, check: ResponseCheck) -> Self {
        self.checks.push(check);
        self
    }

    pub fn export(mut self, export: ResponseExport) -> Self {
        self.exports.push(export);
        self
    }

    pub fn options(mut self, options: RequestOptionOverrides) -> Self {
        if let HttpRequestSource::Inline(ref mut template) = self.request {
            template.options = options;
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum HttpRequestSource {
    Inline(HttpRequestTemplate),
    Api(ApiCall),
}

impl HttpRequestSource {
    pub fn as_inline_mut(&mut self) -> Option<&mut HttpRequestTemplate> {
        match self {
            Self::Inline(request) => Some(request),
            Self::Api(_) => None,
        }
    }
}

impl From<HttpRequestTemplate> for HttpRequestSource {
    fn from(value: HttpRequestTemplate) -> Self {
        Self::Inline(value)
    }
}

impl From<ApiCall> for HttpRequestSource {
    fn from(value: ApiCall) -> Self {
        Self::Api(value)
    }
}

/// Bindings are in the flow's scope; API templates use inputs as local parameter references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiCall {
    pub api_id: String,
    pub bindings: BTreeMap<String, TextTemplate>,
}

impl ApiCall {
    pub fn new(api_id: impl Into<String>) -> Self {
        Self {
            api_id: api_id.into(),
            bindings: BTreeMap::new(),
        }
    }
    pub fn bind(mut self, name: impl Into<String>, value: TextTemplate) -> Self {
        self.bindings.insert(name.into(), value);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RequestOptionOverrides {
    pub timeout_ms: Option<u64>,
    pub redirect_policy: Option<RedirectPolicy>,
    pub max_redirect_hops: Option<u32>,
}

impl RequestOptionOverrides {
    pub fn is_empty(&self) -> bool {
        self.timeout_ms.is_none()
            && self.redirect_policy.is_none()
            && self.max_redirect_hops.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExpectedError {
    Timeout,
    RedirectLimit,
    Network,
    InvalidRequest,
    InvalidResponse,
    ResponseTooLarge,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HttpRequestTemplate {
    pub method: HttpMethod,
    pub url: TextTemplate,
    pub headers: Vec<(TextTemplate, TextTemplate)>,
    pub body: BodyTemplate,
    pub options: RequestOptionOverrides,
}

impl HttpRequestTemplate {
    pub fn new(method: HttpMethod, url: TextTemplate) -> Self {
        Self {
            method,
            url,
            headers: Vec::new(),
            body: BodyTemplate::None,
            options: RequestOptionOverrides::default(),
        }
    }

    pub fn options(mut self, options: RequestOptionOverrides) -> Self {
        self.options = options;
        self
    }

    pub fn header(mut self, name: TextTemplate, value: TextTemplate) -> Self {
        self.headers.push((name, value));
        self
    }

    pub fn json_body(mut self, body: TextTemplate) -> Self {
        self.body = BodyTemplate::Json(body);
        self
    }

    /// Resolve a structured JSON value before serialization, preserving types and escaping strings.
    pub fn json_value_body(mut self, body: JsonTemplate) -> Self {
        self.body = BodyTemplate::JsonValue(body);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BodyTemplate {
    None,
    /// Raw JSON text interpolation. Callers are responsible for quoting and escaping its values.
    Json(TextTemplate),
    /// Structured values are resolved first and serialized as JSON exactly once.
    JsonValue(JsonTemplate),
    Raw(TextTemplate),
    UrlEncoded(TextTemplate),
}

/// A JSON value expression, independent of a text format or editor. References retain their
/// original JSON types. Object keys and literal values are never interpreted as templates.
#[derive(Debug, Clone, PartialEq)]
pub enum JsonTemplate {
    Literal(Value),
    /// Render a text template, then encode it as a JSON string.
    String(TextTemplate),
    Input(String),
    StepOutput {
        step_id: String,
        name: String,
    },
    Object(BTreeMap<String, JsonTemplate>),
    Array(Vec<JsonTemplate>),
}

impl JsonTemplate {
    pub fn literal(value: impl Into<Value>) -> Self {
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

    pub fn object<K: Into<String>>(fields: impl IntoIterator<Item = (K, Self)>) -> Self {
        Self::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        )
    }

    pub fn array(items: impl IntoIterator<Item = Self>) -> Self {
        Self::Array(items.into_iter().collect())
    }
}

impl BodyTemplate {
    pub(crate) fn render(
        &self,
        render: impl Fn(&TextTemplate) -> Result<String, String>,
        render_json: impl Fn(&JsonTemplate) -> Result<Value, String>,
    ) -> Result<RequestBody, String> {
        Ok(match self {
            Self::None => RequestBody::None,
            Self::Json(template) => RequestBody::Json(render(template)?),
            Self::JsonValue(template) => RequestBody::Json(
                serde_json::to_string(&render_json(template)?)
                    .map_err(|error| format!("could not serialize JSON request body: {error}"))?,
            ),
            Self::Raw(template) => RequestBody::Raw(render(template)?),
            Self::UrlEncoded(template) => RequestBody::UrlEncoded(render(template)?),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResponseCheck {
    StatusEquals(u16),
    JsonValueEquals {
        path: String,
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
    RedirectsEquals(usize),
    ErrorEquals(ExpectedError),
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

/// Text templates contain explicit references; literal text is never recursively interpreted.
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
pub enum ValueReference {
    Input(String),
    StepOutput { step_id: String, name: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowOutputSpec {
    pub name: String,
    pub value: ValueReference,
}

#[derive(Clone, PartialEq, Eq)]
pub struct FlowValue {
    pub(crate) value: Value,
    pub(crate) sensitive: bool,
}

impl FlowValue {
    pub fn value(&self) -> &Value {
        &self.value
    }
    pub fn is_sensitive(&self) -> bool {
        self.sensitive
    }
}

impl fmt::Debug for FlowValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut output = f.debug_struct("FlowValue");
        if self.sensitive {
            output.field("value", &"[REDACTED]");
        } else {
            output.field("value", &self.value);
        }
        output.field("sensitive", &self.sensitive).finish()
    }
}

pub type FlowOutputs = BTreeMap<String, FlowValue>;

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
        outputs: FlowOutputs,
    },
}

//! Private v1 wire schema, independent of the compiled runtime representation.
use std::collections::BTreeMap;

use postman_http::request::HttpMethod;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use super::{EditorLayout, FlowDocument, FLOW_DOCUMENT_VERSION};
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum WireAuth {
    HmacSha256 {
        secret: Text,
        #[serde(default = "default_signature_param")]
        param: String,
    },
}

fn default_signature_param() -> String {
    "signature".to_string()
}

use crate::{
    ApiCall, ApiCatalog, ApiDefinition, AuthTemplate, BodyTemplate, ConditionExpr, ExpectedError,
    FlowDefinition, FlowInputSpec, FlowOutputSpec, HttpRequestSource, HttpRequestTemplate,
    HttpStepDefinition, JsonTemplate, RequestOptionOverrides, ResponseCheck, ResponseExport,
    SqlQueryTemplate, TemplatePart, TextTemplate, ValueReference,
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Document {
    schema_version: u64,
    flow: Flow,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    apis: BTreeMap<String, Api>,
    #[serde(default, skip_serializing_if = "layout_empty")]
    editor: EditorLayout,
}

fn layout_empty(layout: &EditorLayout) -> bool {
    layout.nodes.is_empty()
}
fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Flow {
    name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    inputs: Vec<Input>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    steps: Vec<Step>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    outputs: Vec<Output>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    name: String,
    #[serde(
        default,
        deserialize_with = "present_default",
        skip_serializing_if = "Option::is_none"
    )]
    default: Option<Value>,
    #[serde(default, skip_serializing_if = "is_false")]
    sensitive: bool,
}

fn present_default<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    when: Option<ConditionWire>,
    request: Request,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    checks: Vec<Check>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    exports: Vec<Export>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    // Boxed so the HTTP variant stays the same size as the API variant (large_enum_variant).
    Http(Box<HttpWire>),
    Api {
        api: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        bindings: BTreeMap<String, Text>,
    },
    Sql {
        connection: Text,
        query: Text,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        params: Vec<Text>,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HttpWire {
    method: Method,
    url: Text,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    headers: Vec<Header>,
    #[serde(default, skip_serializing_if = "Body::is_none")]
    body: Body,
    #[serde(default, skip_serializing_if = "WireRequestOptions::is_empty")]
    options: WireRequestOptions,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth: Option<WireAuth>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
enum Method {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    name: Text,
    value: Text,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Body {
    #[default]
    None,
    Json {
        value: Json,
    },
    JsonTemplate {
        value: Text,
    },
    Raw {
        value: Text,
    },
    UrlEncoded {
        value: Text,
    },
    File {
        value: Text,
    },
}

impl Body {
    fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
enum ConditionWire {
    Eq(Json, Json),
    Ne(Json, Json),
    Gt(Json, Json),
    Gte(Json, Json),
    Lt(Json, Json),
    Lte(Json, Json),
    In(Json, Json),
    And(Vec<ConditionWire>),
    Or(Vec<ConditionWire>),
    Not(Box<ConditionWire>),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Text {
    Literal(String),
    Input(String),
    Output(Ref),
    Coalesce(Vec<Text>),
    Concat(Vec<Text>),
    Calc(String),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Json {
    Literal(Value),
    String(Text),
    Input(String),
    Output(Ref),
    Coalesce(Vec<Json>),
    Object(BTreeMap<String, Json>),
    Array(Vec<Json>),
    Calc(String),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
struct Ref {
    step: String,
    name: String,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Check {
    Status { equals: u16 },
    Jsonpath { path: String, equals: Json },
    HeaderExists { name: String },
    HeaderContains { name: String, equals: Text },
    BodyContains { equals: Text },
    Redirects { equals: usize },
    Error { equals: ExpectedError },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Export {
    name: String,
    path: String,
    #[serde(default, skip_serializing_if = "is_false")]
    sensitive: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Output {
    name: String,
    value: Reference,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Reference {
    Input(String),
    Output(Ref),
    Literal(Value),
    Coalesce(Vec<Reference>),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Api {
    #[serde(default)]
    parameters: Vec<String>,
    request: ApiRequest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WireRedirectPolicy {
    Follow,
    DoNotFollow,
}

impl From<postman_http::request::RedirectPolicy> for WireRedirectPolicy {
    fn from(policy: postman_http::request::RedirectPolicy) -> Self {
        match policy {
            postman_http::request::RedirectPolicy::Follow => Self::Follow,
            postman_http::request::RedirectPolicy::DoNotFollow => Self::DoNotFollow,
        }
    }
}

impl From<WireRedirectPolicy> for postman_http::request::RedirectPolicy {
    fn from(policy: WireRedirectPolicy) -> Self {
        match policy {
            WireRedirectPolicy::Follow => Self::Follow,
            WireRedirectPolicy::DoNotFollow => Self::DoNotFollow,
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRequestOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    redirect_policy: Option<WireRedirectPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_redirect_hops: Option<u32>,
}

impl WireRequestOptions {
    fn is_empty(&self) -> bool {
        self.timeout_ms.is_none()
            && self.redirect_policy.is_none()
            && self.max_redirect_hops.is_none()
    }
}

impl From<RequestOptionOverrides> for WireRequestOptions {
    fn from(options: RequestOptionOverrides) -> Self {
        Self {
            timeout_ms: options.timeout_ms,
            redirect_policy: options.redirect_policy.map(Into::into),
            max_redirect_hops: options.max_redirect_hops,
        }
    }
}

impl From<WireRequestOptions> for RequestOptionOverrides {
    fn from(options: WireRequestOptions) -> Self {
        Self {
            timeout_ms: options.timeout_ms,
            redirect_policy: options.redirect_policy.map(Into::into),
            max_redirect_hops: options.max_redirect_hops,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApiRequest {
    method: Method,
    url: Text,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    headers: Vec<Header>,
    #[serde(default, skip_serializing_if = "Body::is_none")]
    body: Body,
    #[serde(default, skip_serializing_if = "WireRequestOptions::is_empty")]
    options: WireRequestOptions,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth: Option<WireAuth>,
}

impl From<Document> for FlowDocument {
    fn from(value: Document) -> Self {
        let mut apis = ApiCatalog::new();
        for (id, api) in value.apis {
            apis.insert(
                id,
                ApiDefinition {
                    parameters: api.parameters,
                    request: api.request.into(),
                },
            );
        }
        Self {
            apis,
            editor: value.editor,
            flow: FlowDefinition {
                name: value.flow.name,
                inputs: value
                    .flow
                    .inputs
                    .into_iter()
                    .map(|input| FlowInputSpec {
                        name: input.name,
                        default: input.default,
                        sensitive: input.sensitive,
                    })
                    .collect(),
                steps: value
                    .flow
                    .steps
                    .into_iter()
                    .map(|step| HttpStepDefinition {
                        name: step.name.unwrap_or_else(|| step.id.clone()),
                        id: step.id,
                        when: step.when.map(Into::into),
                        request: step.request.into(),
                        checks: step.checks.into_iter().map(Into::into).collect(),
                        exports: step
                            .exports
                            .into_iter()
                            .map(|export| ResponseExport {
                                name: export.name,
                                json_path: export.path,
                                sensitive: export.sensitive,
                            })
                            .collect(),
                    })
                    .collect(),
                outputs: value
                    .flow
                    .outputs
                    .into_iter()
                    .map(|output| FlowOutputSpec {
                        name: output.name,
                        value: output.value.into(),
                    })
                    .collect(),
            },
        }
    }
}

impl From<&FlowDocument> for Document {
    fn from(value: &FlowDocument) -> Self {
        Self {
            schema_version: FLOW_DOCUMENT_VERSION,
            editor: value.editor.clone(),
            apis: value
                .apis
                .definitions
                .iter()
                .map(|(id, api)| {
                    (
                        id.clone(),
                        Api {
                            parameters: api.parameters.clone(),
                            request: (&api.request).into(),
                        },
                    )
                })
                .collect(),
            flow: Flow {
                name: value.flow.name.clone(),
                inputs: value
                    .flow
                    .inputs
                    .iter()
                    .map(|input| Input {
                        name: input.name.clone(),
                        default: input.default.clone(),
                        sensitive: input.sensitive,
                    })
                    .collect(),
                steps: value
                    .flow
                    .steps
                    .iter()
                    .map(|step| Step {
                        id: step.id.clone(),
                        name: Some(step.name.clone()),
                        when: step.when.as_ref().map(Into::into),
                        request: (&step.request).into(),
                        checks: step.checks.iter().map(Into::into).collect(),
                        exports: step
                            .exports
                            .iter()
                            .map(|export| Export {
                                name: export.name.clone(),
                                path: export.json_path.clone(),
                                sensitive: export.sensitive,
                            })
                            .collect(),
                    })
                    .collect(),
                outputs: value
                    .flow
                    .outputs
                    .iter()
                    .map(|output| Output {
                        name: output.name.clone(),
                        value: (&output.value).into(),
                    })
                    .collect(),
            },
        }
    }
}

impl From<Method> for HttpMethod {
    fn from(value: Method) -> Self {
        match value {
            Method::Get => Self::GET,
            Method::Post => Self::POST,
            Method::Put => Self::PUT,
            Method::Delete => Self::DELETE,
            Method::Patch => Self::PATCH,
            Method::Head => Self::HEAD,
            Method::Options => Self::OPTIONS,
        }
    }
}

impl From<HttpMethod> for Method {
    fn from(value: HttpMethod) -> Self {
        match value {
            HttpMethod::GET => Self::Get,
            HttpMethod::POST => Self::Post,
            HttpMethod::PUT => Self::Put,
            HttpMethod::DELETE => Self::Delete,
            HttpMethod::PATCH => Self::Patch,
            HttpMethod::HEAD => Self::Head,
            HttpMethod::OPTIONS => Self::Options,
        }
    }
}

impl From<Request> for HttpRequestSource {
    fn from(value: Request) -> Self {
        match value {
            Request::Http(http) => {
                let HttpWire {
                    method,
                    url,
                    headers,
                    body,
                    options,
                    auth,
                } = *http;
                HttpRequestTemplate::from(ApiRequest {
                    method,
                    url,
                    headers,
                    body,
                    options,
                    auth,
                })
                .into()
            }
            Request::Api { api, bindings } => ApiCall {
                api_id: api,
                bindings: bindings
                    .into_iter()
                    .map(|(name, value)| (name, value.into()))
                    .collect(),
            }
            .into(),
            Request::Sql {
                connection,
                query,
                params,
            } => SqlQueryTemplate {
                connection: connection.into(),
                query: query.into(),
                params: params.into_iter().map(Into::into).collect(),
            }
            .into(),
        }
    }
}

impl From<&HttpRequestSource> for Request {
    fn from(value: &HttpRequestSource) -> Self {
        match value {
            HttpRequestSource::Inline(value) => {
                let ApiRequest {
                    method,
                    url,
                    headers,
                    body,
                    options,
                    auth,
                } = value.into();
                Self::Http(Box::new(HttpWire {
                    method,
                    url,
                    headers,
                    body,
                    options,
                    auth,
                }))
            }
            HttpRequestSource::Api(value) => Self::Api {
                api: value.api_id.clone(),
                bindings: value
                    .bindings
                    .iter()
                    .map(|(name, value)| (name.clone(), value.into()))
                    .collect(),
            },
            HttpRequestSource::Sql(sql) => Self::Sql {
                connection: (&sql.connection).into(),
                query: (&sql.query).into(),
                params: sql.params.iter().map(Into::into).collect(),
            },
        }
    }
}

impl From<ApiRequest> for HttpRequestTemplate {
    fn from(value: ApiRequest) -> Self {
        Self {
            method: value.method.into(),
            url: value.url.into(),
            headers: value
                .headers
                .into_iter()
                .map(|header| (header.name.into(), header.value.into()))
                .collect(),
            body: value.body.into(),
            options: value.options.into(),
            auth: value.auth.map(Into::into),
        }
    }
}

impl From<&HttpRequestTemplate> for ApiRequest {
    fn from(value: &HttpRequestTemplate) -> Self {
        Self {
            method: value.method.into(),
            url: (&value.url).into(),
            headers: value
                .headers
                .iter()
                .map(|(name, value)| Header {
                    name: name.into(),
                    value: value.into(),
                })
                .collect(),
            body: (&value.body).into(),
            options: value.options.into(),
            auth: value.auth.as_ref().map(Into::into),
        }
    }
}

impl From<Text> for TextTemplate {
    fn from(value: Text) -> Self {
        match value {
            Text::Literal(value) => Self::literal(value),
            Text::Input(name) => Self::parts([TemplatePart::Input(name)]),
            Text::Output(value) => Self::parts([TemplatePart::StepOutput {
                step_id: value.step,
                name: value.name,
            }]),
            Text::Coalesce(items) => Self::parts([TemplatePart::Coalesce(
                items.into_iter().map(Self::from).collect(),
            )]),
            Text::Concat(items) => {
                Self::parts(items.into_iter().flat_map(|item| Self::from(item).parts))
            }
            Text::Calc(expr) => Self::parts([TemplatePart::Calc(expr)]),
        }
    }
}

fn part_to_text(part: &TemplatePart) -> Text {
    match part {
        TemplatePart::Literal(value) => Text::Literal(value.clone()),
        TemplatePart::Input(name) => Text::Input(name.clone()),
        TemplatePart::StepOutput { step_id, name } => Text::Output(Ref {
            step: step_id.clone(),
            name: name.clone(),
        }),
        TemplatePart::Coalesce(items) => Text::Coalesce(items.iter().map(Text::from).collect()),
        TemplatePart::Calc(expr) => Text::Calc(expr.clone()),
    }
}

impl From<&TextTemplate> for Text {
    fn from(value: &TextTemplate) -> Self {
        if let [single] = value.parts.as_slice() {
            part_to_text(single)
        } else {
            Self::Concat(value.parts.iter().map(part_to_text).collect())
        }
    }
}

impl From<Json> for JsonTemplate {
    fn from(value: Json) -> Self {
        match value {
            Json::Literal(value) => Self::Literal(value),
            Json::Input(name) => Self::Input(name),
            Json::String(value) => Self::String(value.into()),
            Json::Output(value) => Self::StepOutput {
                step_id: value.step,
                name: value.name,
            },
            Json::Coalesce(items) => Self::Coalesce(items.into_iter().map(Into::into).collect()),
            Json::Object(fields) => Self::Object(
                fields
                    .into_iter()
                    .map(|(name, value)| (name, value.into()))
                    .collect(),
            ),
            Json::Array(items) => Self::Array(items.into_iter().map(Into::into).collect()),
            Json::Calc(expr) => Self::Calc(expr),
        }
    }
}

impl From<&JsonTemplate> for Json {
    fn from(value: &JsonTemplate) -> Self {
        match value {
            JsonTemplate::Literal(value) => Self::Literal(value.clone()),
            JsonTemplate::Input(name) => Self::Input(name.clone()),
            JsonTemplate::String(value) => Self::String(value.into()),
            JsonTemplate::StepOutput { step_id, name } => Self::Output(Ref {
                step: step_id.clone(),
                name: name.clone(),
            }),
            JsonTemplate::Coalesce(items) => Self::Coalesce(items.iter().map(Into::into).collect()),
            JsonTemplate::Object(fields) => Self::Object(
                fields
                    .iter()
                    .map(|(name, value)| (name.clone(), value.into()))
                    .collect(),
            ),
            JsonTemplate::Array(items) => Self::Array(items.iter().map(Into::into).collect()),
            JsonTemplate::Calc(expr) => Self::Calc(expr.clone()),
        }
    }
}

impl From<Body> for BodyTemplate {
    fn from(value: Body) -> Self {
        match value {
            Body::None => Self::None,
            Body::Json { value } => Self::JsonValue(value.into()),
            Body::JsonTemplate { value } => Self::Json(value.into()),
            Body::Raw { value } => Self::Raw(value.into()),
            Body::UrlEncoded { value } => Self::UrlEncoded(value.into()),
            Body::File { value } => Self::File(value.into()),
        }
    }
}

impl From<&BodyTemplate> for Body {
    fn from(value: &BodyTemplate) -> Self {
        match value {
            BodyTemplate::None => Self::None,
            BodyTemplate::JsonValue(value) => Self::Json {
                value: value.into(),
            },
            BodyTemplate::Json(value) => Self::JsonTemplate {
                value: value.into(),
            },
            BodyTemplate::Raw(value) => Self::Raw {
                value: value.into(),
            },
            BodyTemplate::UrlEncoded(value) => Self::UrlEncoded {
                value: value.into(),
            },
            BodyTemplate::File(value) => Self::File {
                value: value.into(),
            },
        }
    }
}

impl From<Check> for ResponseCheck {
    fn from(value: Check) -> Self {
        match value {
            Check::Status { equals } => Self::StatusEquals(equals),
            Check::Jsonpath { path, equals } => Self::JsonValueEquals {
                path,
                expected: equals.into(),
            },
            Check::HeaderExists { name } => Self::HeaderExists { name },
            Check::HeaderContains { name, equals } => Self::HeaderContains {
                name,
                expected: equals.into(),
            },
            Check::BodyContains { equals } => Self::BodyContains {
                expected: equals.into(),
            },
            Check::Redirects { equals } => Self::RedirectsEquals(equals),
            Check::Error { equals } => Self::ErrorEquals(equals),
        }
    }
}

impl From<&ResponseCheck> for Check {
    fn from(value: &ResponseCheck) -> Self {
        match value {
            ResponseCheck::StatusEquals(equals) => Self::Status { equals: *equals },
            ResponseCheck::JsonValueEquals { path, expected } => Self::Jsonpath {
                path: path.clone(),
                equals: expected.into(),
            },
            ResponseCheck::HeaderExists { name } => Self::HeaderExists { name: name.clone() },
            ResponseCheck::HeaderContains { name, expected } => Self::HeaderContains {
                name: name.clone(),
                equals: expected.into(),
            },
            ResponseCheck::BodyContains { expected } => Self::BodyContains {
                equals: expected.into(),
            },
            ResponseCheck::RedirectsEquals(equals) => Self::Redirects { equals: *equals },
            ResponseCheck::ErrorEquals(equals) => Self::Error { equals: *equals },
        }
    }
}
impl From<Reference> for ValueReference {
    fn from(value: Reference) -> Self {
        match value {
            Reference::Input(name) => ValueReference::Input(name),
            Reference::Output(value) => ValueReference::StepOutput {
                step_id: value.step,
                name: value.name,
            },
            Reference::Literal(v) => ValueReference::Literal(v),
            Reference::Coalesce(items) => {
                ValueReference::Coalesce(items.into_iter().map(Into::into).collect())
            }
        }
    }
}

impl From<&ValueReference> for Reference {
    fn from(value: &ValueReference) -> Self {
        match value {
            ValueReference::Input(name) => Reference::Input(name.clone()),
            ValueReference::StepOutput { step_id, name } => Reference::Output(Ref {
                step: step_id.clone(),
                name: name.clone(),
            }),
            ValueReference::Literal(v) => Reference::Literal(v.clone()),
            ValueReference::Coalesce(items) => {
                Reference::Coalesce(items.iter().map(Into::into).collect())
            }
        }
    }
}

impl From<ConditionWire> for ConditionExpr {
    fn from(wire: ConditionWire) -> Self {
        match wire {
            ConditionWire::Eq(l, r) => Self::Eq(l.into(), r.into()),
            ConditionWire::Ne(l, r) => Self::Ne(l.into(), r.into()),
            ConditionWire::Gt(l, r) => Self::Gt(l.into(), r.into()),
            ConditionWire::Gte(l, r) => Self::Gte(l.into(), r.into()),
            ConditionWire::Lt(l, r) => Self::Lt(l.into(), r.into()),
            ConditionWire::Lte(l, r) => Self::Lte(l.into(), r.into()),
            ConditionWire::In(l, r) => Self::In(l.into(), r.into()),
            ConditionWire::And(items) => Self::And(items.into_iter().map(Into::into).collect()),
            ConditionWire::Or(items) => Self::Or(items.into_iter().map(Into::into).collect()),
            ConditionWire::Not(inner) => Self::Not(Box::new((*inner).into())),
        }
    }
}

impl From<&ConditionExpr> for ConditionWire {
    fn from(expr: &ConditionExpr) -> Self {
        match expr {
            ConditionExpr::Eq(l, r) => Self::Eq(l.into(), r.into()),
            ConditionExpr::Ne(l, r) => Self::Ne(l.into(), r.into()),
            ConditionExpr::Gt(l, r) => Self::Gt(l.into(), r.into()),
            ConditionExpr::Gte(l, r) => Self::Gte(l.into(), r.into()),
            ConditionExpr::Lt(l, r) => Self::Lt(l.into(), r.into()),
            ConditionExpr::Lte(l, r) => Self::Lte(l.into(), r.into()),
            ConditionExpr::In(l, r) => Self::In(l.into(), r.into()),
            ConditionExpr::And(items) => Self::And(items.iter().map(Into::into).collect()),
            ConditionExpr::Or(items) => Self::Or(items.iter().map(Into::into).collect()),
            ConditionExpr::Not(inner) => Self::Not(Box::new((&**inner).into())),
        }
    }
}

impl From<WireAuth> for AuthTemplate {
    fn from(value: WireAuth) -> Self {
        match value {
            WireAuth::HmacSha256 { secret, param } => Self::HmacSha256 {
                secret: secret.into(),
                param,
            },
        }
    }
}

impl From<&AuthTemplate> for WireAuth {
    fn from(value: &AuthTemplate) -> Self {
        match value {
            AuthTemplate::HmacSha256 { secret, param } => Self::HmacSha256 {
                secret: secret.into(),
                param: param.clone(),
            },
        }
    }
}

use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use postman_flow::{
    compile_flow, parse_flow_yaml, write_flow_yaml, CompileEnvironment, FlowDocument,
    HttpRequestSource,
};
use rmcp::{
    handler::server::wrapper::{Json, Parameters},
    model::{Implementation, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router, ServerHandler,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const DSL_REFERENCE: &str = include_str!("../../postman-flow/README.md");
const MINIMAL_EXAMPLE: &str =
    include_str!("../../postman-flow/examples/flows/httpbingo_minimal.http.yml");
const CATALOG_EXAMPLE: &str =
    include_str!("../../postman-flow/examples/flows/httpbingo_catalog.http.yml");
const DEFAULT_MAX_STEPS: usize = 256;

#[derive(Clone)]
pub struct FlowMcpServer {
    root: Arc<PathBuf>,
    max_steps: usize,
}

impl FlowMcpServer {
    pub fn new(root: impl AsRef<Path>, max_steps: Option<usize>) -> Result<Self, String> {
        let root = root.as_ref().canonicalize().map_err(|error| {
            format!(
                "cannot resolve MCP workspace root {}: {error}",
                root.as_ref().display()
            )
        })?;
        if !root.is_dir() {
            return Err(format!(
                "MCP workspace root is not a directory: {}",
                root.display()
            ));
        }
        let max_steps = max_steps.unwrap_or(DEFAULT_MAX_STEPS);
        if max_steps == 0 {
            return Err("max_steps must be greater than zero".to_owned());
        }
        Ok(Self {
            root: Arc::new(root),
            max_steps,
        })
    }

    fn compile(&self, document: &FlowDocument) -> Result<FlowSummary, String> {
        let plan = compile_flow(
            &document.flow,
            &document.apis,
            &CompileEnvironment {
                max_steps: Some(self.max_steps),
            },
        )
        .map_err(|diagnostics| {
            let details = diagnostics
                .into_iter()
                .map(|diagnostic| {
                    format!(
                        "{:?} at {}: {}",
                        diagnostic.code, diagnostic.location.field, diagnostic.message
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!("flow compilation failed:\n{details}")
        })?;

        Ok(FlowSummary {
            name: plan.name().to_owned(),
            step_count: plan.step_count(),
            inputs: document
                .flow
                .inputs
                .iter()
                .map(|input| InputSummary {
                    name: input.name.clone(),
                    required: input.default.is_none(),
                    sensitive: input.sensitive,
                })
                .collect(),
            steps: document
                .flow
                .steps
                .iter()
                .map(|step| StepSummary {
                    id: step.id.clone(),
                    name: step.name.clone(),
                    request_kind: match step.request {
                        HttpRequestSource::Inline(_) => "http",
                        HttpRequestSource::Api(_) => "api",
                    }
                    .to_owned(),
                    conditional: step.when.is_some(),
                    checks: step.checks.len(),
                    exports: step
                        .exports
                        .iter()
                        .map(|export| export.name.clone())
                        .collect(),
                })
                .collect(),
            outputs: plan
                .outputs()
                .iter()
                .map(|output| output.name.clone())
                .collect(),
        })
    }

    fn parse_document_value(
        &self,
        value: &BTreeMap<String, Value>,
    ) -> Result<(FlowDocument, String), String> {
        let source = yaml_serde::to_string(value)
            .map_err(|error| format!("cannot convert document to YAML: {error}"))?;
        self.parse_source(&source)
    }

    fn parse_source(&self, source: &str) -> Result<(FlowDocument, String), String> {
        let document = parse_flow_yaml(source).map_err(|error| error.to_string())?;
        self.compile(&document)?;
        let yaml = write_flow_yaml(&document).map_err(|error| error.to_string())?;
        Ok((document, yaml))
    }

    fn load_source(&self, source: FlowSource) -> Result<(FlowDocument, String), String> {
        let provided = usize::from(source.path.is_some())
            + usize::from(source.document.is_some())
            + usize::from(source.yaml.is_some());
        if provided != 1 {
            return Err("provide exactly one of `path`, `document`, or `yaml`".to_owned());
        }
        if let Some(path) = source.path {
            let path = self.resolve_existing_path(&path)?;
            let source = fs::read_to_string(&path)
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
            self.parse_source(&source)
        } else if let Some(document) = source.document {
            self.parse_document_value(&document)
        } else {
            self.parse_source(source.yaml.as_deref().expect("source count checked"))
        }
    }

    fn validate_relative_path(&self, path: &str) -> Result<PathBuf, String> {
        let relative = Path::new(path);
        if relative.as_os_str().is_empty() || relative.is_absolute() {
            return Err("path must be a non-empty relative path".to_owned());
        }
        if !is_flow_file(relative) {
            return Err(
                "path must end in .http.yml, .http.yaml, .flow.yml, or .flow.yaml".to_owned(),
            );
        }
        for component in relative.components() {
            if !matches!(component, Component::Normal(_)) {
                return Err(
                    "path must not contain `.`, `..`, root, or platform prefixes".to_owned(),
                );
            }
        }
        Ok(self.root.join(relative))
    }

    fn reject_symlink_ancestors(&self, path: &Path) -> Result<(), String> {
        let relative = path
            .strip_prefix(self.root.as_ref())
            .map_err(|_| "path escaped the configured workspace root".to_owned())?;
        let mut current = self.root.as_ref().clone();
        for component in relative.components() {
            current.push(component);
            match fs::symlink_metadata(&current) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(format!(
                        "refusing symlinked path inside MCP workspace: {}",
                        current.display()
                    ));
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(error) => {
                    return Err(format!("cannot inspect {}: {error}", current.display()));
                }
            }
        }
        Ok(())
    }

    fn resolve_existing_path(&self, path: &str) -> Result<PathBuf, String> {
        let path = self.validate_relative_path(path)?;
        self.reject_symlink_ancestors(&path)?;
        let canonical = path
            .canonicalize()
            .map_err(|error| format!("cannot resolve {}: {error}", path.display()))?;
        if !canonical.starts_with(self.root.as_ref()) {
            return Err("path escaped the configured workspace root".to_owned());
        }
        if !canonical.is_file() {
            return Err(format!("flow path is not a file: {}", canonical.display()));
        }
        Ok(canonical)
    }

    fn write_flow(&self, path: &str, yaml: &str, overwrite: bool) -> Result<PathBuf, String> {
        let path = self.validate_relative_path(path)?;
        self.reject_symlink_ancestors(&path)?;
        let parent = path
            .parent()
            .expect("validated relative path has root parent");
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        self.reject_symlink_ancestors(&path)?;

        let mut options = fs::OpenOptions::new();
        options.write(true);
        if overwrite {
            options.create(true).truncate(true);
        } else {
            options.create_new(true);
        }
        let mut file = options.open(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                format!(
                    "{} already exists; pass overwrite=true to replace it",
                    path.display()
                )
            } else {
                format!("cannot write {}: {error}", path.display())
            }
        })?;
        file.write_all(yaml.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("cannot persist {}: {error}", path.display()))?;
        Ok(path)
    }
}

#[tool_router]
impl FlowMcpServer {
    #[tool(
        name = "get_dsl_schema",
        description = "Return the authoritative postman-flow v1 JSON schema and generation rules. Call this before creating a flow."
    )]
    fn get_dsl_schema(&self) -> Json<DslSchemaOutput> {
        Json(DslSchemaOutput {
            schema_version: 1,
            schema: dsl_schema(),
            rules: vec![
                "Use explicit expression objects such as {\"literal\": ...}, {\"input\": \"name\"}, and {\"output\": {\"step\": \"id\", \"name\": \"field\"}}.".to_owned(),
                "A step may reference only inputs and exports from earlier steps.".to_owned(),
                "Never place API keys, private keys, passwords, or wallet secrets in a flow document; use sensitive runtime inputs.".to_owned(),
                "Call validate_flow before create_flow when iterating on a draft.".to_owned(),
            ],
        })
    }

    #[tool(
        name = "get_dsl_reference",
        description = "Return the full postman-flow v1 reference, including expressions, bodies, checks, API catalogs, diagnostics, and limits."
    )]
    fn get_dsl_reference(&self) -> String {
        DSL_REFERENCE.to_owned()
    }

    #[tool(
        name = "list_flow_examples",
        description = "Return validated example flow documents. Use these as patterns, then call validate_flow on generated drafts."
    )]
    fn list_flow_examples(&self) -> Result<Json<ExamplesOutput>, String> {
        let examples = [
            (
                "minimal",
                "Extract and reuse an HTTP response value",
                MINIMAL_EXAMPLE,
            ),
            (
                "api_catalog",
                "Declare and call reusable API definitions",
                CATALOG_EXAMPLE,
            ),
        ]
        .into_iter()
        .map(|(name, description, source)| {
            let (document, yaml) = self.parse_source(source)?;
            let value = yaml_serde::from_str::<Value>(&yaml)
                .map_err(|error| format!("cannot expose built-in example {name}: {error}"))?;
            Ok(FlowExample {
                name: name.to_owned(),
                description: description.to_owned(),
                flow_name: document.flow.name,
                document: value,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
        Ok(Json(ExamplesOutput { examples }))
    }

    #[tool(
        name = "validate_flow",
        description = "Parse and compile a flow from one relative path, structured JSON document, or YAML string without writing files or sending network requests."
    )]
    fn validate_flow(
        &self,
        Parameters(arguments): Parameters<FlowSource>,
    ) -> Result<Json<ValidateFlowOutput>, String> {
        let (document, yaml) = self.load_source(arguments)?;
        let summary = self.compile(&document)?;
        Ok(Json(ValidateFlowOutput {
            valid: true,
            summary,
            canonical_yaml: yaml,
        }))
    }

    #[tool(
        name = "inspect_flow",
        description = "Inspect a valid flow's inputs, ordered steps, checks, exports, and outputs without executing it."
    )]
    fn inspect_flow(
        &self,
        Parameters(arguments): Parameters<FlowSource>,
    ) -> Result<Json<FlowSummary>, String> {
        let (document, _) = self.load_source(arguments)?;
        self.compile(&document).map(Json)
    }

    #[tool(
        name = "create_flow",
        description = "Compile a structured JSON flow document and save canonical YAML under the configured workspace root. Invalid flows are never written."
    )]
    fn create_flow(
        &self,
        Parameters(arguments): Parameters<CreateFlowArguments>,
    ) -> Result<Json<CreateFlowOutput>, String> {
        let (document, yaml) = self.parse_document_value(&arguments.document)?;
        let summary = self.compile(&document)?;
        let path = self.write_flow(&arguments.path, &yaml, arguments.overwrite)?;
        let relative_path = path
            .strip_prefix(self.root.as_ref())
            .expect("resolved path is rooted")
            .display()
            .to_string();
        Ok(Json(CreateFlowOutput {
            created: true,
            path: relative_path,
            summary,
            canonical_yaml: yaml,
        }))
    }
}

#[tool_handler]
impl ServerHandler for FlowMcpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "postman-flow-mcp",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "Call get_dsl_schema or list_flow_examples before generating a document. Use validate_flow while iterating, then create_flow to save canonical YAML. Files are confined to the configured workspace root.",
            )
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FlowSource {
    /// Relative flow path under the configured MCP workspace root.
    #[serde(default)]
    pub path: Option<String>,
    /// Structured JSON representation of a complete schema_version 1 document.
    #[serde(default)]
    pub document: Option<BTreeMap<String, Value>>,
    /// Complete YAML source. Prefer `document` for Agent-generated flows.
    #[serde(default)]
    pub yaml: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateFlowArguments {
    /// Relative destination ending in .http.yml, .http.yaml, .flow.yml, or .flow.yaml.
    pub path: String,
    /// Structured JSON representation of the complete Flow document.
    pub document: BTreeMap<String, Value>,
    /// Replace an existing regular file. Defaults to false.
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DslSchemaOutput {
    pub schema_version: u64,
    pub schema: Value,
    pub rules: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ExamplesOutput {
    pub examples: Vec<FlowExample>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct FlowExample {
    pub name: String,
    pub description: String,
    pub flow_name: String,
    pub document: Value,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ValidateFlowOutput {
    pub valid: bool,
    pub summary: FlowSummary,
    pub canonical_yaml: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct CreateFlowOutput {
    pub created: bool,
    pub path: String,
    pub summary: FlowSummary,
    pub canonical_yaml: String,
}

#[derive(Debug, Serialize, JsonSchema, PartialEq, Eq)]
pub struct FlowSummary {
    pub name: String,
    pub step_count: usize,
    pub inputs: Vec<InputSummary>,
    pub steps: Vec<StepSummary>,
    pub outputs: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema, PartialEq, Eq)]
pub struct InputSummary {
    pub name: String,
    pub required: bool,
    pub sensitive: bool,
}

#[derive(Debug, Serialize, JsonSchema, PartialEq, Eq)]
pub struct StepSummary {
    pub id: String,
    pub name: String,
    pub request_kind: String,
    pub conditional: bool,
    pub checks: usize,
    pub exports: Vec<String>,
}

fn is_flow_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    name.ends_with(".http.yml")
        || name.ends_with(".http.yaml")
        || name.ends_with(".flow.yml")
        || name.ends_with(".flow.yaml")
}

fn dsl_schema() -> Value {
    serde_json::from_str(include_str!("../flow-v1.schema.json"))
        .expect("embedded Flow v1 schema must be valid JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn minimal_document() -> BTreeMap<String, Value> {
        serde_json::from_value(json!({
            "schema_version": 1,
            "flow": {
                "name": "generated",
                "inputs": [{"name": "host", "default": "https://example.com"}],
                "steps": [{
                    "id": "status",
                    "request": {
                        "kind": "http",
                        "method": "GET",
                        "url": {"concat": [
                            {"input": "host"},
                            {"literal": "/status/200"}
                        ]}
                    },
                    "checks": [{"kind": "status", "equals": 200}]
                }]
            }
        }))
        .unwrap()
    }

    #[test]
    fn creates_canonical_yaml_inside_root() {
        let directory = tempdir().unwrap();
        let server = FlowMcpServer::new(directory.path(), None).unwrap();
        let output = server
            .create_flow(Parameters(CreateFlowArguments {
                path: "flows/generated.http.yml".to_owned(),
                document: minimal_document(),
                overwrite: false,
            }))
            .unwrap()
            .0;

        assert!(output.created);
        assert_eq!(output.summary.name, "generated");
        assert_eq!(output.summary.step_count, 1);
        let saved = fs::read_to_string(directory.path().join(&output.path)).unwrap();
        assert_eq!(saved, output.canonical_yaml);
        parse_flow_yaml(&saved).unwrap();
    }

    #[test]
    fn rejects_parent_traversal_and_absolute_paths() {
        let directory = tempdir().unwrap();
        let server = FlowMcpServer::new(directory.path(), None).unwrap();
        for path in ["../escape.http.yml", "/tmp/escape.http.yml"] {
            let result = server.create_flow(Parameters(CreateFlowArguments {
                path: path.to_owned(),
                document: minimal_document(),
                overwrite: false,
            }));
            let error = match result {
                Err(error) => error,
                Ok(_) => panic!("unsafe path should be rejected"),
            };
            assert!(error.contains("relative path") || error.contains("must not contain"));
        }
    }

    #[test]
    fn rejects_invalid_references_without_writing() {
        let directory = tempdir().unwrap();
        let server = FlowMcpServer::new(directory.path(), None).unwrap();
        let mut document = serde_json::to_value(minimal_document()).unwrap();
        document["flow"]["steps"][0]["request"]["url"] =
            json!({"output": {"step": "later", "name": "id"}});
        let document = serde_json::from_value(document).unwrap();
        let result = server.create_flow(Parameters(CreateFlowArguments {
            path: "flows/invalid.http.yml".to_owned(),
            document,
            overwrite: false,
        }));
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("invalid flow should be rejected"),
        };

        assert!(error.contains("UnavailableOutput"));
        assert!(!directory.path().join("flows/invalid.http.yml").exists());
    }

    #[test]
    fn embedded_schema_and_examples_are_available() {
        let directory = tempdir().unwrap();
        let server = FlowMcpServer::new(directory.path(), None).unwrap();
        let schema = server.get_dsl_schema().0;
        assert_eq!(schema.schema_version, 1);
        assert_eq!(schema.schema["properties"]["schema_version"]["const"], 1);

        let examples = server.list_flow_examples().unwrap().0.examples;
        assert_eq!(examples.len(), 2);
        assert!(examples.iter().all(|example| example.document.is_object()));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let directory = tempdir().unwrap();
        let outside = tempdir().unwrap();
        symlink(outside.path(), directory.path().join("flows")).unwrap();
        let server = FlowMcpServer::new(directory.path(), None).unwrap();
        let result = server.create_flow(Parameters(CreateFlowArguments {
            path: "flows/escape.http.yml".to_owned(),
            document: minimal_document(),
            overwrite: false,
        }));
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("symlink escape should be rejected"),
        };

        assert!(error.contains("symlinked path"));
        assert!(!outside.path().join("escape.http.yml").exists());
    }
}

//! Versioned persistence for source documents. Runtime plans and session values are not persisted.
use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ApiCatalog, FlowDefinition};

#[path = "document_wire.rs"]
mod wire;

pub const FLOW_DOCUMENT_VERSION: u64 = 1;
const MAX_DOCUMENT_BYTES: usize = 2 * 1024 * 1024;
const MAX_DOCUMENT_DEPTH: usize = 96;

#[derive(Debug, Clone, PartialEq)]
pub struct FlowDocument {
    pub flow: FlowDefinition,
    pub apis: ApiCatalog,
    pub editor: EditorLayout,
}

impl FlowDocument {
    pub fn new(flow: FlowDefinition) -> Self {
        Self {
            flow,
            apis: ApiCatalog::new(),
            editor: EditorLayout::default(),
        }
    }
}

/// Optional, persistent layout only. It does not change compilation or execution.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorLayout {
    #[serde(default)]
    pub nodes: BTreeMap<String, NodePosition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodePosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentErrorCode {
    Syntax,
    Schema,
    UnsupportedVersion,
    LimitExceeded,
    Serialization,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentError {
    pub code: DocumentErrorCode,
    pub field: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub message: String,
}

impl DocumentError {
    fn new(code: DocumentErrorCode, field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code,
            field: field.into(),
            line: None,
            column: None,
            message: message.into(),
        }
    }
}

impl fmt::Display for DocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.field)?;
        if let Some(line) = self.line {
            write!(f, " at {line}:{}", self.column.unwrap_or(1))?;
        }
        write!(f, ": {}", self.message)
    }
}

impl std::error::Error for DocumentError {}

/// Parse one YAML v1 document. Syntax/shape errors are distinct from compile diagnostics.
/// Unfinished but structurally representable flows can be saved and loaded before compiling.
pub fn parse_flow_yaml(source: &str) -> Result<FlowDocument, DocumentError> {
    if source.len() > MAX_DOCUMENT_BYTES {
        return Err(DocumentError::new(
            DocumentErrorCode::LimitExceeded,
            "$",
            "flow document exceeds 2 MiB",
        ));
    }
    let tree: yaml_serde::Value = yaml_serde::from_str(source).map_err(|error| {
        let location = error.location();
        DocumentError {
            code: DocumentErrorCode::Syntax,
            field: "$".into(),
            line: location.as_ref().map(|value| value.line()),
            column: location.as_ref().map(|value| value.column()),
            message: error.to_string(),
        }
    })?;
    let json = to_json(tree, "$", 0)?;
    match json.get("schema_version").and_then(Value::as_u64) {
        Some(FLOW_DOCUMENT_VERSION) => {}
        Some(_) => {
            return Err(DocumentError::new(
                DocumentErrorCode::UnsupportedVersion,
                "schema_version",
                "unsupported flow document version",
            ))
        }
        None => {
            return Err(DocumentError::new(
                DocumentErrorCode::Schema,
                "schema_version",
                "expected schema_version: 1",
            ))
        }
    }
    let document: wire::Document = serde_path_to_error::deserialize(json).map_err(|error| {
        DocumentError::new(
            DocumentErrorCode::Schema,
            error.path().to_string(),
            error.inner().to_string(),
        )
    })?;
    Ok(document.into())
}

/// Canonical YAML output. Preserves source semantics and layout, not comments or original spacing.
pub fn write_flow_yaml(document: &FlowDocument) -> Result<String, DocumentError> {
    for (id, position) in &document.editor.nodes {
        if !position.x.is_finite() || !position.y.is_finite() {
            return Err(DocumentError::new(
                DocumentErrorCode::Schema,
                format!("editor.nodes[{id:?}]"),
                "node coordinates must be finite",
            ));
        }
    }
    let wire = wire::Document::from(document);
    // JSON-shaped enum maps deliberately avoid YAML-specific tags.
    let value = serde_json::to_value(wire).map_err(|error| {
        DocumentError::new(DocumentErrorCode::Serialization, "$", error.to_string())
    })?;
    let output = yaml_serde::to_string(&value).map_err(|error| {
        DocumentError::new(DocumentErrorCode::Serialization, "$", error.to_string())
    })?;
    // A saved document must satisfy the same format limits as a subsequently loaded one.
    parse_flow_yaml(&output)?;
    Ok(output)
}

fn to_json(value: yaml_serde::Value, field: &str, depth: usize) -> Result<Value, DocumentError> {
    let error = |message| DocumentError::new(DocumentErrorCode::Schema, field, message);
    if depth > MAX_DOCUMENT_DEPTH {
        return Err(DocumentError::new(
            DocumentErrorCode::LimitExceeded,
            field,
            "document nesting exceeds 96 levels",
        ));
    }
    Ok(match value {
        yaml_serde::Value::Null => Value::Null,
        yaml_serde::Value::Bool(value) => Value::Bool(value),
        yaml_serde::Value::String(value) => Value::String(value),
        yaml_serde::Value::Number(value) => {
            if let Some(value) = value.as_u64() {
                Value::from(value)
            } else if let Some(value) = value.as_i64() {
                Value::from(value)
            } else {
                let number = value
                    .as_f64()
                    .and_then(serde_json::Number::from_f64)
                    .ok_or_else(|| error("only finite JSON numbers are supported"))?;
                Value::Number(number)
            }
        }
        yaml_serde::Value::Sequence(items) => Value::Array(
            items
                .into_iter()
                .enumerate()
                .map(|(index, value)| to_json(value, &format!("{field}[{index}]"), depth + 1))
                .collect::<Result<_, _>>()?,
        ),
        yaml_serde::Value::Mapping(fields) => {
            let mut object = serde_json::Map::new();
            for (key, value) in fields {
                let yaml_serde::Value::String(key) = key else {
                    return Err(error("mapping keys must be strings"));
                };
                object.insert(
                    key.clone(),
                    to_json(value, &format!("{field}.{key}"), depth + 1)?,
                );
            }
            Value::Object(object)
        }
        yaml_serde::Value::Tagged(_) => {
            return Err(error(
                "YAML tags are not supported; use explicit expression maps",
            ))
        }
    })
}

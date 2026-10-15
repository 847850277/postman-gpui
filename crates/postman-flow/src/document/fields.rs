//! JSON field editing uses exactly the document's wire types and domain conversions.
use super::{wire, DocumentError, DocumentErrorCode, MAX_DOCUMENT_BYTES};
use crate::{BodyTemplate, ConditionExpr, ResponseCheck, TextTemplate};
use serde::{de::DeserializeOwned, Serialize};

fn parse<T: DeserializeOwned>(source: &str) -> Result<T, DocumentError> {
    if source.len() > MAX_DOCUMENT_BYTES {
        return Err(DocumentError::new(
            DocumentErrorCode::LimitExceeded,
            "$",
            "expression exceeds 2 MiB",
        ));
    }
    let mut deserializer = serde_json::Deserializer::from_str(source);
    let value = serde_path_to_error::deserialize(&mut deserializer).map_err(|error| {
        let inner = error.inner();
        DocumentError {
            code: if inner.is_data() {
                DocumentErrorCode::Schema
            } else {
                DocumentErrorCode::Syntax
            },
            field: error.path().to_string(),
            line: Some(inner.line()),
            column: Some(inner.column()),
            message: inner.to_string(),
        }
    })?;
    deserializer
        .end()
        .map_err(|error| DocumentError::new(DocumentErrorCode::Syntax, "$", error.to_string()))?;
    Ok(value)
}

fn write(value: impl Serialize) -> Result<String, DocumentError> {
    serde_json::to_string_pretty(&value).map_err(|error| {
        DocumentError::new(DocumentErrorCode::Serialization, "$", error.to_string())
    })
}

/// Parse a native text expression, without resolving references or interpreting literal text.
pub fn parse_text_expression(source: &str) -> Result<TextTemplate, DocumentError> {
    parse::<wire::Text>(source).map(Into::into)
}

/// Encode a text template in the Flow document's JSON expression notation.
pub fn write_text_expression(value: &TextTemplate) -> Result<String, DocumentError> {
    write(wire::Text::from(value))
}

/// Parse an ordered array of expression-bearing header pairs. Duplicates are retained.
pub fn parse_header_expressions(
    source: &str,
) -> Result<Vec<(TextTemplate, TextTemplate)>, DocumentError> {
    if source.trim().is_empty() {
        return Ok(Vec::new());
    }
    parse::<Vec<wire::Header>>(source).map(|headers| {
        headers
            .into_iter()
            .map(|h| (h.name.into(), h.value.into()))
            .collect()
    })
}

/// Encode headers without deduplicating or resolving them.
pub fn write_header_expressions(
    value: &[(TextTemplate, TextTemplate)],
) -> Result<String, DocumentError> {
    write(
        value
            .iter()
            .map(|(name, value)| wire::Header {
                name: name.into(),
                value: value.into(),
            })
            .collect::<Vec<_>>(),
    )
}

/// Parse a native body object. An empty editor represents an absent body.
pub fn parse_body_expression(source: &str) -> Result<BodyTemplate, DocumentError> {
    if source.trim().is_empty() {
        return Ok(BodyTemplate::None);
    }
    parse::<wire::Body>(source).map(Into::into)
}

/// Encode a body using the same typed JSON/text distinction as the document codec.
pub fn write_body_expression(value: &BodyTemplate) -> Result<String, DocumentError> {
    write(wire::Body::from(value))
}

/// Parse an optional native condition. Only blank source removes the condition.
pub fn parse_condition_expression(source: &str) -> Result<Option<ConditionExpr>, DocumentError> {
    if source.trim().is_empty() {
        return Ok(None);
    }
    parse::<wire::ConditionWire>(source).map(|value| Some(value.into()))
}

/// Encode a condition using the document's typed JSON operands, without evaluation.
pub fn write_condition_expression(value: &ConditionExpr) -> Result<String, DocumentError> {
    write(wire::ConditionWire::from(value))
}

/// Encode a response-check definition for lossless readonly inspection.
pub fn write_response_check(value: &ResponseCheck) -> Result<String, DocumentError> {
    write(wire::Check::from(value))
}

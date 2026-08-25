//! Versioned, sanitized Request History persistence contract.
//!
//! `HistoryEntry` remains the runtime projection. Versioned snapshots are the only shapes that may
//! cross the repository boundary: conversion removes known credentials before a storage adapter
//! can observe them. V1 contains a replay request only; V2 can additionally contain a bounded,
//! sanitized textual response preview. Neither version contains GPUI, tab, cookie-jar, or
//! pending-request state.

use crate::models::{
    HistoricalResponse, HistoricalResponseBody, HistoryEntry, HttpMethod, MultipartEditorPart,
    MultipartPart, MultipartValue, RedirectPolicy, Request, RequestBody, RequestEditorIntent,
    RequestOptions, MAX_REDIRECT_HOPS,
};
use chrono::{DateTime, Utc};
use reqwest::{
    header::{HeaderName, HeaderValue},
    Url,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fmt, path::PathBuf};
use uuid::Uuid;

pub const HISTORY_SNAPSHOT_VERSION_V1: u64 = 1;
pub const HISTORY_SNAPSHOT_VERSION_V2: u64 = 2;
pub const MAX_HISTORICAL_RESPONSE_PREVIEW_BYTES: usize = 256 * 1024;

/// Errors at the runtime/snapshot and serialized-payload boundaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistorySnapshotError {
    UnsupportedVersion {
        found: u64,
    },
    MalformedPayload {
        message: String,
    },
    Serialization {
        message: String,
    },
    IncompleteHistoryEntry {
        field: &'static str,
    },
    InvalidField {
        field: &'static str,
        reason: &'static str,
    },
    NumericOverflow {
        field: &'static str,
    },
    NonUtf8Path {
        field: &'static str,
    },
    MissingMultipartFile {
        path: PathBuf,
    },
}

impl fmt::Display for HistorySnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion { found } => {
                write!(formatter, "unsupported History snapshot version {found}")
            }
            Self::MalformedPayload { message } => {
                write!(formatter, "malformed History snapshot payload: {message}")
            }
            Self::Serialization { message } => {
                write!(formatter, "failed to serialize History snapshot: {message}")
            }
            Self::IncompleteHistoryEntry { field } => {
                write!(
                    formatter,
                    "History entry is not persistable without {field}"
                )
            }
            Self::InvalidField { field, reason } => {
                write!(
                    formatter,
                    "invalid History snapshot field {field}: {reason}"
                )
            }
            Self::NumericOverflow { field } => {
                write!(
                    formatter,
                    "History field {field} exceeds the persisted numeric range"
                )
            }
            Self::NonUtf8Path { field } => {
                write!(formatter, "History field {field} is not a UTF-8 path")
            }
            Self::MissingMultipartFile { path } => {
                write!(
                    formatter,
                    "multipart replay file is unavailable: {}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for HistorySnapshotError {}

/// Version dispatcher used by repositories. New payload versions must add a new enum variant;
/// unknown versions are rejected before their payload is decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionedHistorySnapshot {
    V1(HistorySnapshotV1),
    V2(HistorySnapshotV2),
}

impl VersionedHistorySnapshot {
    pub fn version(&self) -> u64 {
        match self {
            Self::V1(_) => HISTORY_SNAPSHOT_VERSION_V1,
            Self::V2(_) => HISTORY_SNAPSHOT_VERSION_V2,
        }
    }

    /// Common request/metadata prefix shared by both persisted versions.
    pub fn as_v1(&self) -> &HistorySnapshotV1 {
        match self {
            Self::V1(snapshot) => snapshot,
            Self::V2(snapshot) => snapshot.base(),
        }
    }

    pub fn as_v2(&self) -> Option<&HistorySnapshotV2> {
        match self {
            Self::V1(_) => None,
            Self::V2(snapshot) => Some(snapshot),
        }
    }

    pub fn to_json_bytes(&self) -> Result<Vec<u8>, HistorySnapshotError> {
        let serialized = match self {
            Self::V1(snapshot) => serde_json::to_vec(&HistorySnapshotEnvelope {
                version: HISTORY_SNAPSHOT_VERSION_V1,
                snapshot,
            }),
            Self::V2(snapshot) => serde_json::to_vec(&HistorySnapshotEnvelope {
                version: HISTORY_SNAPSHOT_VERSION_V2,
                snapshot,
            }),
        };
        serialized.map_err(|error| HistorySnapshotError::Serialization {
            message: error.to_string(),
        })
    }

    pub fn to_json_string(&self) -> Result<String, HistorySnapshotError> {
        String::from_utf8(self.to_json_bytes()?).map_err(|error| {
            HistorySnapshotError::Serialization {
                message: error.to_string(),
            }
        })
    }

    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, HistorySnapshotError> {
        let value: Value = serde_json::from_slice(bytes).map_err(malformed_payload)?;
        Self::from_json_value(value)
    }

    pub fn from_json_str(value: &str) -> Result<Self, HistorySnapshotError> {
        Self::from_json_bytes(value.as_bytes())
    }

    fn from_json_value(value: Value) -> Result<Self, HistorySnapshotError> {
        let object = value
            .as_object()
            .ok_or(HistorySnapshotError::MalformedPayload {
                message: "top-level value must be an object".to_string(),
            })?;
        let version = object.get("version").and_then(Value::as_u64).ok_or(
            HistorySnapshotError::MalformedPayload {
                message: "version must be an unsigned integer".to_string(),
            },
        )?;
        if !matches!(
            version,
            HISTORY_SNAPSHOT_VERSION_V1 | HISTORY_SNAPSHOT_VERSION_V2
        ) {
            return Err(HistorySnapshotError::UnsupportedVersion { found: version });
        }
        let snapshot_value =
            object
                .get("snapshot")
                .cloned()
                .ok_or(HistorySnapshotError::MalformedPayload {
                    message: "snapshot is required".to_string(),
                })?;
        match version {
            HISTORY_SNAPSHOT_VERSION_V1 => {
                let raw: RawHistorySnapshotV1 =
                    serde_json::from_value(snapshot_value).map_err(malformed_payload)?;
                let mut snapshot = HistorySnapshotV1::from(raw);
                snapshot.sanitize_and_validate()?;
                Ok(Self::V1(snapshot))
            }
            HISTORY_SNAPSHOT_VERSION_V2 => {
                let raw: RawHistorySnapshotV2 =
                    serde_json::from_value(snapshot_value).map_err(malformed_payload)?;
                let mut snapshot = HistorySnapshotV2::from(raw);
                snapshot.sanitize_and_validate()?;
                Ok(Self::V2(snapshot))
            }
            found => Err(HistorySnapshotError::UnsupportedVersion { found }),
        }
    }

    pub fn validate_replay_files(&self) -> Result<(), HistorySnapshotError> {
        self.as_v1().validate_replay_files()
    }
}

impl TryFrom<&HistoryEntry> for VersionedHistorySnapshot {
    type Error = HistorySnapshotError;

    fn try_from(entry: &HistoryEntry) -> Result<Self, Self::Error> {
        HistorySnapshotV2::try_from(entry).map(Self::V2)
    }
}

impl TryFrom<VersionedHistorySnapshot> for HistoryEntry {
    type Error = HistorySnapshotError;

    fn try_from(snapshot: VersionedHistorySnapshot) -> Result<Self, Self::Error> {
        match snapshot {
            VersionedHistorySnapshot::V1(snapshot) => HistoryEntry::try_from(snapshot),
            VersionedHistorySnapshot::V2(snapshot) => HistoryEntry::try_from(snapshot),
        }
    }
}

#[derive(Serialize)]
struct HistorySnapshotEnvelope<'a, T> {
    version: u64,
    snapshot: &'a T,
}

/// Complete V1 History row. All fields are private so callers cannot bypass the sanitizing
/// conversion and hand an ad-hoc credential-bearing DTO to a repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HistorySnapshotV1 {
    entry_id: String,
    timestamp: String,
    name: String,
    status: u16,
    elapsed_ms: u64,
    response_size: u64,
    request: RequestSnapshotV1,
}

/// Decode-only shape kept private so external callers cannot bypass the version dispatcher and
/// repository-boundary sanitization by deserializing `HistorySnapshotV1` directly.
#[derive(Deserialize)]
struct RawHistorySnapshotV1 {
    entry_id: String,
    timestamp: String,
    name: String,
    status: u16,
    elapsed_ms: u64,
    response_size: u64,
    request: RequestSnapshotV1,
}

impl From<RawHistorySnapshotV1> for HistorySnapshotV1 {
    fn from(raw: RawHistorySnapshotV1) -> Self {
        Self {
            entry_id: raw.entry_id,
            timestamp: raw.timestamp,
            name: raw.name,
            status: raw.status,
            elapsed_ms: raw.elapsed_ms,
            response_size: raw.response_size,
            request: raw.request,
        }
    }
}

impl HistorySnapshotV1 {
    pub fn entry_id(&self) -> &str {
        &self.entry_id
    }

    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn status(&self) -> u16 {
        self.status
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.elapsed_ms
    }

    pub fn response_size(&self) -> u64 {
        self.response_size
    }

    pub fn request(&self) -> &RequestSnapshotV1 {
        &self.request
    }

    /// File contents are intentionally absent from V1. This validation is explicit so startup
    /// hydration can still show the History row and the replay path can report a typed error.
    pub fn validate_replay_files(&self) -> Result<(), HistorySnapshotError> {
        self.request.validate_replay_files()
    }

    fn sanitize_and_validate(&mut self) -> Result<(), HistorySnapshotError> {
        let original_url = self.request.replay_url()?;
        let secret_values = self.request.sanitize_in_place()?;
        let sanitized_url = self.request.replay_url()?;
        self.name =
            sanitize_history_name(&self.name, &original_url, &sanitized_url, &secret_values);
        self.request.remove_empty_editor_placeholders();
        self.validate()
    }

    fn validate(&self) -> Result<(), HistorySnapshotError> {
        if self.entry_id.trim().is_empty() {
            return Err(invalid_field("entry_id", "must not be empty"));
        }
        Uuid::parse_str(&self.entry_id).map_err(|_| invalid_field("entry_id", "must be a UUID"))?;
        DateTime::parse_from_rfc3339(&self.timestamp)
            .map_err(|_| invalid_field("timestamp", "must be RFC 3339"))?;
        if !(100..=599).contains(&self.status) {
            return Err(invalid_field("status", "must be between 100 and 599"));
        }
        self.request.validate()
    }
}

impl TryFrom<&HistoryEntry> for HistorySnapshotV1 {
    type Error = HistorySnapshotError;

    fn try_from(entry: &HistoryEntry) -> Result<Self, Self::Error> {
        if entry.id.trim().is_empty() {
            return Err(invalid_field("entry_id", "must not be empty"));
        }
        Uuid::parse_str(&entry.id).map_err(|_| invalid_field("entry_id", "must be a UUID"))?;
        let status = entry
            .status
            .ok_or(HistorySnapshotError::IncompleteHistoryEntry { field: "status" })?;
        let elapsed_ms = entry
            .elapsed_ms
            .ok_or(HistorySnapshotError::IncompleteHistoryEntry {
                field: "elapsed_ms",
            })?;
        let response_size =
            entry
                .response_size
                .ok_or(HistorySnapshotError::IncompleteHistoryEntry {
                    field: "response_size",
                })?;
        let elapsed_ms =
            u64::try_from(elapsed_ms).map_err(|_| HistorySnapshotError::NumericOverflow {
                field: "elapsed_ms",
            })?;
        let response_size =
            u64::try_from(response_size).map_err(|_| HistorySnapshotError::NumericOverflow {
                field: "response_size",
            })?;

        let sanitized_request = SanitizedRequest::try_from_entry(entry)?;
        let name = sanitize_history_name(
            &entry.name,
            &entry.request.url,
            &sanitized_request.replay_url,
            &sanitized_request.secret_values,
        );
        let snapshot = Self {
            entry_id: entry.id.clone(),
            timestamp: entry.timestamp.to_rfc3339(),
            name,
            status,
            elapsed_ms,
            response_size,
            request: sanitized_request.snapshot,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }
}

impl TryFrom<HistorySnapshotV1> for HistoryEntry {
    type Error = HistorySnapshotError;

    fn try_from(mut snapshot: HistorySnapshotV1) -> Result<Self, Self::Error> {
        snapshot.sanitize_and_validate()?;
        let timestamp = DateTime::parse_from_rfc3339(&snapshot.timestamp)
            .map_err(|_| invalid_field("timestamp", "must be RFC 3339"))?
            .with_timezone(&Utc);
        let response_size = usize::try_from(snapshot.response_size).map_err(|_| {
            HistorySnapshotError::NumericOverflow {
                field: "response_size",
            }
        })?;
        let request = snapshot.request.to_runtime_request()?;
        let editor_intent = snapshot.request.to_runtime_editor_intent();
        let request_options = snapshot.request.options.to_runtime();
        Ok(HistoryEntry {
            id: snapshot.entry_id,
            request,
            editor_intent,
            request_options,
            timestamp,
            name: snapshot.name,
            status: Some(snapshot.status),
            elapsed_ms: Some(u128::from(snapshot.elapsed_ms)),
            response_size: Some(response_size),
            historical_response: None,
        })
    }
}

/// V2 extends the sanitized V1 replay request with optional historical response evidence.
/// Flattening keeps the stable request fields at the same JSON paths while the versioned envelope
/// remains the sole decoder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HistorySnapshotV2 {
    #[serde(flatten)]
    base: HistorySnapshotV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    response: Option<HistoricalResponseSnapshotV2>,
}

#[derive(Deserialize)]
struct RawHistorySnapshotV2 {
    #[serde(flatten)]
    base: RawHistorySnapshotV1,
    response: Option<RawHistoricalResponseSnapshotV2>,
}

impl From<RawHistorySnapshotV2> for HistorySnapshotV2 {
    fn from(raw: RawHistorySnapshotV2) -> Self {
        Self {
            base: raw.base.into(),
            response: raw.response.map(Into::into),
        }
    }
}

impl HistorySnapshotV2 {
    pub fn base(&self) -> &HistorySnapshotV1 {
        &self.base
    }

    pub fn response(&self) -> Option<&HistoricalResponseSnapshotV2> {
        self.response.as_ref()
    }

    fn sanitize_and_validate(&mut self) -> Result<(), HistorySnapshotError> {
        self.base.sanitize_and_validate()?;
        if let Some(response) = &mut self.response {
            response.sanitize_and_validate()?;
            response.validate_against(&self.base)?;
        }
        Ok(())
    }
}

impl TryFrom<&HistoryEntry> for HistorySnapshotV2 {
    type Error = HistorySnapshotError;

    fn try_from(entry: &HistoryEntry) -> Result<Self, Self::Error> {
        let base = HistorySnapshotV1::try_from(entry)?;
        let response = entry
            .historical_response
            .as_ref()
            .map(HistoricalResponseSnapshotV2::try_from)
            .transpose()?;
        let snapshot = Self { base, response };
        if let Some(response) = &snapshot.response {
            response.validate_against(&snapshot.base)?;
        }
        Ok(snapshot)
    }
}

impl TryFrom<HistorySnapshotV2> for HistoryEntry {
    type Error = HistorySnapshotError;

    fn try_from(mut snapshot: HistorySnapshotV2) -> Result<Self, Self::Error> {
        snapshot.sanitize_and_validate()?;
        let response = snapshot
            .response
            .map(HistoricalResponse::try_from)
            .transpose()?;
        let mut entry = HistoryEntry::try_from(snapshot.base)?;
        entry.historical_response = response;
        Ok(entry)
    }
}

/// Explicit persisted body classification. Text is copied exactly from the sanitized preview;
/// unsupported bodies intentionally have no byte payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "preview", rename_all = "snake_case")]
pub enum HistoricalResponseBodySnapshotV2 {
    Empty,
    Text(String),
    TruncatedText(String),
    Unsupported,
}

#[derive(Deserialize)]
#[serde(tag = "kind", content = "preview", rename_all = "snake_case")]
enum RawHistoricalResponseBodySnapshotV2 {
    Empty,
    Text(String),
    TruncatedText(String),
    Unsupported,
}

impl From<RawHistoricalResponseBodySnapshotV2> for HistoricalResponseBodySnapshotV2 {
    fn from(raw: RawHistoricalResponseBodySnapshotV2) -> Self {
        match raw {
            RawHistoricalResponseBodySnapshotV2::Empty => Self::Empty,
            RawHistoricalResponseBodySnapshotV2::Text(preview) => Self::Text(preview),
            RawHistoricalResponseBodySnapshotV2::TruncatedText(preview) => {
                Self::TruncatedText(preview)
            }
            RawHistoricalResponseBodySnapshotV2::Unsupported => Self::Unsupported,
        }
    }
}

impl HistoricalResponseBodySnapshotV2 {
    pub fn preview(&self) -> Option<&str> {
        match self {
            Self::Text(preview) | Self::TruncatedText(preview) => Some(preview),
            Self::Empty | Self::Unsupported => None,
        }
    }

    pub fn is_truncated(&self) -> bool {
        matches!(self, Self::TruncatedText(_))
    }
}

/// Sanitized response subset persisted by V2. The summary fields are intentionally repeated and
/// validated against the History row so a corrupt payload cannot render contradictory evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HistoricalResponseSnapshotV2 {
    status: u16,
    headers: Vec<HeaderSnapshotV1>,
    body: HistoricalResponseBodySnapshotV2,
    media_type: Option<String>,
    elapsed_ms: u64,
    original_size: u64,
    persisted_size: u64,
    truncated: bool,
}

#[derive(Deserialize)]
struct RawHistoricalResponseSnapshotV2 {
    status: u16,
    headers: Vec<HeaderSnapshotV1>,
    body: RawHistoricalResponseBodySnapshotV2,
    media_type: Option<String>,
    elapsed_ms: u64,
    original_size: u64,
    persisted_size: u64,
    truncated: bool,
}

impl From<RawHistoricalResponseSnapshotV2> for HistoricalResponseSnapshotV2 {
    fn from(raw: RawHistoricalResponseSnapshotV2) -> Self {
        Self {
            status: raw.status,
            headers: raw.headers,
            body: raw.body.into(),
            media_type: raw.media_type,
            elapsed_ms: raw.elapsed_ms,
            original_size: raw.original_size,
            persisted_size: raw.persisted_size,
            truncated: raw.truncated,
        }
    }
}

impl HistoricalResponseSnapshotV2 {
    pub fn status(&self) -> u16 {
        self.status
    }

    pub fn headers(&self) -> &[HeaderSnapshotV1] {
        &self.headers
    }

    pub fn body(&self) -> &HistoricalResponseBodySnapshotV2 {
        &self.body
    }

    pub fn media_type(&self) -> Option<&str> {
        self.media_type.as_deref()
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.elapsed_ms
    }

    pub fn original_size(&self) -> u64 {
        self.original_size
    }

    pub fn persisted_size(&self) -> u64 {
        self.persisted_size
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    fn sanitize_and_validate(&mut self) -> Result<(), HistorySnapshotError> {
        self.headers
            .retain(|header| !HistorySensitiveDataPolicy::is_sensitive_header_name(&header.name));
        let is_download = is_download_response(&self.headers);
        let media_type = response_media_type(&self.headers).or_else(|| self.media_type.clone());
        self.media_type = sanitize_media_type(media_type)?;

        let existing_truncation = self.truncated || self.body.is_truncated();
        self.body = match std::mem::replace(
            &mut self.body,
            HistoricalResponseBodySnapshotV2::Unsupported,
        ) {
            HistoricalResponseBodySnapshotV2::Empty => HistoricalResponseBodySnapshotV2::Empty,
            HistoricalResponseBodySnapshotV2::Unsupported => {
                HistoricalResponseBodySnapshotV2::Unsupported
            }
            HistoricalResponseBodySnapshotV2::Text(_preview)
            | HistoricalResponseBodySnapshotV2::TruncatedText(_preview)
                if is_download || !is_textual_response_media_type(self.media_type.as_deref()) =>
            {
                HistoricalResponseBodySnapshotV2::Unsupported
            }
            HistoricalResponseBodySnapshotV2::Text(preview)
            | HistoricalResponseBodySnapshotV2::TruncatedText(preview) => {
                let preview =
                    sanitize_structured_response_body(preview, self.media_type.as_deref());
                let (preview, truncated_now) =
                    truncate_utf8_preview(preview, MAX_HISTORICAL_RESPONSE_PREVIEW_BYTES);
                if existing_truncation || truncated_now {
                    HistoricalResponseBodySnapshotV2::TruncatedText(preview)
                } else if preview.is_empty() {
                    HistoricalResponseBodySnapshotV2::Empty
                } else {
                    HistoricalResponseBodySnapshotV2::Text(preview)
                }
            }
        };
        self.truncated = self.body.is_truncated();
        self.persisted_size =
            u64::try_from(self.body.preview().map_or(0, str::len)).map_err(|_| {
                HistorySnapshotError::NumericOverflow {
                    field: "response.persisted_size",
                }
            })?;
        self.validate()
    }

    fn validate(&self) -> Result<(), HistorySnapshotError> {
        if !(100..=599).contains(&self.status) {
            return Err(invalid_field(
                "response.status",
                "must be between 100 and 599",
            ));
        }
        for header in &self.headers {
            if HistorySensitiveDataPolicy::is_sensitive_header_name(&header.name) {
                return Err(invalid_field(
                    "response.headers.name",
                    "must not be sensitive",
                ));
            }
            HeaderName::from_bytes(header.name.as_bytes()).map_err(|_| {
                invalid_field("response.headers.name", "must be a valid HTTP header name")
            })?;
            HeaderValue::from_str(&header.value).map_err(|_| {
                invalid_field(
                    "response.headers.value",
                    "must be a valid HTTP header value",
                )
            })?;
        }
        if self.truncated != self.body.is_truncated() {
            return Err(invalid_field(
                "response.truncated",
                "must match response.body.kind",
            ));
        }
        let preview_size = self.body.preview().map_or(0, str::len);
        if preview_size > MAX_HISTORICAL_RESPONSE_PREVIEW_BYTES {
            return Err(invalid_field(
                "response.body.preview",
                "must not exceed 256 KiB",
            ));
        }
        if self.persisted_size != preview_size as u64 {
            return Err(invalid_field(
                "response.persisted_size",
                "must equal the persisted UTF-8 preview byte length",
            ));
        }
        if matches!(self.body, HistoricalResponseBodySnapshotV2::Empty) && self.original_size != 0 {
            return Err(invalid_field(
                "response.original_size",
                "must be zero for an empty body",
            ));
        }
        Ok(())
    }

    fn validate_against(&self, base: &HistorySnapshotV1) -> Result<(), HistorySnapshotError> {
        self.validate()?;
        if self.status != base.status {
            return Err(invalid_field(
                "response.status",
                "must match the History summary status",
            ));
        }
        if self.elapsed_ms != base.elapsed_ms {
            return Err(invalid_field(
                "response.elapsed_ms",
                "must match the History summary elapsed time",
            ));
        }
        if self.original_size != base.response_size {
            return Err(invalid_field(
                "response.original_size",
                "must match the History summary response size",
            ));
        }
        Ok(())
    }
}

impl TryFrom<&HistoricalResponse> for HistoricalResponseSnapshotV2 {
    type Error = HistorySnapshotError;

    fn try_from(response: &HistoricalResponse) -> Result<Self, Self::Error> {
        let elapsed_ms = u64::try_from(response.elapsed_ms).map_err(|_| {
            HistorySnapshotError::NumericOverflow {
                field: "response.elapsed_ms",
            }
        })?;
        let original_size = u64::try_from(response.original_size).map_err(|_| {
            HistorySnapshotError::NumericOverflow {
                field: "response.original_size",
            }
        })?;
        let body = match &response.body {
            HistoricalResponseBody::Empty => HistoricalResponseBodySnapshotV2::Empty,
            HistoricalResponseBody::Text(preview) => {
                HistoricalResponseBodySnapshotV2::Text(preview.clone())
            }
            HistoricalResponseBody::TruncatedText(preview) => {
                HistoricalResponseBodySnapshotV2::TruncatedText(preview.clone())
            }
            HistoricalResponseBody::Unsupported => HistoricalResponseBodySnapshotV2::Unsupported,
        };
        let mut snapshot = Self {
            status: response.status,
            headers: response
                .headers
                .iter()
                .map(|(name, value)| HeaderSnapshotV1 {
                    name: name.clone(),
                    value: value.clone(),
                })
                .collect(),
            body,
            media_type: response.media_type.clone(),
            elapsed_ms,
            original_size,
            persisted_size: 0,
            truncated: response.body.is_truncated(),
        };
        snapshot.sanitize_and_validate()?;
        Ok(snapshot)
    }
}

impl TryFrom<HistoricalResponseSnapshotV2> for HistoricalResponse {
    type Error = HistorySnapshotError;

    fn try_from(snapshot: HistoricalResponseSnapshotV2) -> Result<Self, Self::Error> {
        snapshot.validate()?;
        let original_size = usize::try_from(snapshot.original_size).map_err(|_| {
            HistorySnapshotError::NumericOverflow {
                field: "response.original_size",
            }
        })?;
        let persisted_size = usize::try_from(snapshot.persisted_size).map_err(|_| {
            HistorySnapshotError::NumericOverflow {
                field: "response.persisted_size",
            }
        })?;
        let body = match snapshot.body {
            HistoricalResponseBodySnapshotV2::Empty => HistoricalResponseBody::Empty,
            HistoricalResponseBodySnapshotV2::Text(preview) => {
                HistoricalResponseBody::Text(preview)
            }
            HistoricalResponseBodySnapshotV2::TruncatedText(preview) => {
                HistoricalResponseBody::TruncatedText(preview)
            }
            HistoricalResponseBodySnapshotV2::Unsupported => HistoricalResponseBody::Unsupported,
        };
        Ok(Self {
            status: snapshot.status,
            headers: snapshot
                .headers
                .into_iter()
                .map(|header| (header.name, header.value))
                .collect(),
            body,
            media_type: snapshot.media_type,
            elapsed_ms: u128::from(snapshot.elapsed_ms),
            original_size,
            persisted_size,
        })
    }
}

struct SanitizedRequest {
    snapshot: RequestSnapshotV1,
    replay_url: String,
    secret_values: Vec<String>,
}

impl SanitizedRequest {
    fn try_from_entry(entry: &HistoryEntry) -> Result<Self, HistorySnapshotError> {
        let mut url = parse_http_url(&entry.request.url)?;
        let mut secret_values = Vec::new();
        if !url.username().is_empty() {
            secret_values.push(url.username().to_string());
        }
        if let Some(password) = url.password().filter(|value| !value.is_empty()) {
            secret_values.push(password.to_string());
        }

        let query = url
            .query_pairs()
            .filter_map(|(name, value)| {
                if HistorySensitiveDataPolicy::is_sensitive_query_name(&name) {
                    if !value.is_empty() {
                        secret_values.push(value.into_owned());
                    }
                    None
                } else {
                    Some(KeyValueSnapshotV1 {
                        name: name.into_owned(),
                        value: value.into_owned(),
                    })
                }
            })
            .collect();
        clear_url_credentials_and_non_base_parts(&mut url)?;

        let mut headers = Vec::new();
        for (name, value) in &entry.request.headers {
            if HistorySensitiveDataPolicy::is_sensitive_header_name(name) {
                if !value.is_empty() {
                    secret_values.push(value.clone());
                }
            } else {
                headers.push(HeaderSnapshotV1 {
                    name: name.clone(),
                    value: value.clone(),
                });
            }
        }

        let snapshot = RequestSnapshotV1 {
            method: entry.request.method.into(),
            url: url.to_string(),
            query,
            headers,
            body: RequestBodySnapshotV1::try_from(&entry.request.body)?,
            editor_intent: entry
                .editor_intent
                .as_ref()
                .map(RequestEditorIntentSnapshotV1::try_from)
                .transpose()?,
            options: RequestOptionsSnapshotV1::try_from(entry.request_options)?,
        };
        let mut snapshot = snapshot;
        snapshot.remove_empty_editor_placeholders();
        snapshot.validate()?;
        let replay_url = snapshot.replay_url()?;
        Ok(Self {
            snapshot,
            replay_url,
            secret_values,
        })
    }
}

/// Sanitized replay request. Query values are kept separate from the credential-free base URL so
/// storage inspection and policy testing remain deterministic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestSnapshotV1 {
    method: HttpMethodSnapshotV1,
    url: String,
    query: Vec<KeyValueSnapshotV1>,
    headers: Vec<HeaderSnapshotV1>,
    body: RequestBodySnapshotV1,
    editor_intent: Option<RequestEditorIntentSnapshotV1>,
    options: RequestOptionsSnapshotV1,
}

impl RequestSnapshotV1 {
    pub fn method(&self) -> HttpMethod {
        self.method.into()
    }

    pub fn base_url(&self) -> &str {
        &self.url
    }

    pub fn query(&self) -> &[KeyValueSnapshotV1] {
        &self.query
    }

    pub fn headers(&self) -> &[HeaderSnapshotV1] {
        &self.headers
    }

    pub fn body(&self) -> &RequestBodySnapshotV1 {
        &self.body
    }

    pub fn editor_intent(&self) -> Option<&RequestEditorIntentSnapshotV1> {
        self.editor_intent.as_ref()
    }

    pub fn options(&self) -> &RequestOptionsSnapshotV1 {
        &self.options
    }

    pub fn replay_url(&self) -> Result<String, HistorySnapshotError> {
        let mut url = parse_http_url(&self.url)?;
        url.set_query(None);
        if !self.query.is_empty() {
            let mut query = url.query_pairs_mut();
            for pair in &self.query {
                query.append_pair(&pair.name, &pair.value);
            }
        }
        Ok(url.to_string())
    }

    fn sanitize_in_place(&mut self) -> Result<Vec<String>, HistorySnapshotError> {
        let mut url = parse_http_url(&self.url)?;
        let mut secret_values = Vec::new();
        if !url.username().is_empty() {
            secret_values.push(url.username().to_string());
        }
        if let Some(password) = url.password().filter(|value| !value.is_empty()) {
            secret_values.push(password.to_string());
        }
        let url_query = url
            .query_pairs()
            .map(|(name, value)| KeyValueSnapshotV1 {
                name: name.into_owned(),
                value: value.into_owned(),
            })
            .collect::<Vec<_>>();
        clear_url_credentials_and_non_base_parts(&mut url)?;
        self.url = url.to_string();
        self.query.splice(0..0, url_query);
        self.query.retain(|pair| {
            if HistorySensitiveDataPolicy::is_sensitive_query_name(&pair.name) {
                if !pair.value.is_empty() {
                    secret_values.push(pair.value.clone());
                }
                false
            } else {
                true
            }
        });
        self.headers.retain(|header| {
            if HistorySensitiveDataPolicy::is_sensitive_header_name(&header.name) {
                if !header.value.is_empty() {
                    secret_values.push(header.value.clone());
                }
                false
            } else {
                true
            }
        });
        Ok(secret_values)
    }

    fn remove_empty_editor_placeholders(&mut self) {
        if let Some(RequestEditorIntentSnapshotV1::Multipart(parts)) = &mut self.editor_intent {
            parts.retain(|part| !part.is_empty_placeholder());
        }
    }

    fn validate(&self) -> Result<(), HistorySnapshotError> {
        let url = parse_http_url(&self.url)?;
        if url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(invalid_field(
                "request.url",
                "must be a credential-free base URL",
            ));
        }
        for pair in &self.query {
            if pair.name.trim().is_empty() {
                return Err(invalid_field("request.query.name", "must not be empty"));
            }
            if HistorySensitiveDataPolicy::is_sensitive_query_name(&pair.name) {
                return Err(invalid_field("request.query.name", "must not be sensitive"));
            }
        }
        for header in &self.headers {
            if HistorySensitiveDataPolicy::is_sensitive_header_name(&header.name) {
                return Err(invalid_field(
                    "request.headers.name",
                    "must not be sensitive",
                ));
            }
            HeaderName::from_bytes(header.name.as_bytes()).map_err(|_| {
                invalid_field("request.headers.name", "must be a valid HTTP header name")
            })?;
            HeaderValue::from_str(&header.value).map_err(|_| {
                invalid_field("request.headers.value", "must be a valid HTTP header value")
            })?;
        }
        self.body.validate()?;
        if let Some(editor_intent) = &self.editor_intent {
            editor_intent.validate()?;
        }
        self.options.validate()
    }

    fn validate_replay_files(&self) -> Result<(), HistorySnapshotError> {
        self.body.validate_replay_files()?;
        if let Some(editor_intent) = &self.editor_intent {
            editor_intent.validate_replay_files()?;
        }
        Ok(())
    }

    fn to_runtime_request(&self) -> Result<Request, HistorySnapshotError> {
        self.validate()?;
        Ok(Request {
            method: self.method.into(),
            url: self.replay_url()?,
            headers: self
                .headers
                .iter()
                .map(|header| (header.name.clone(), header.value.clone()))
                .collect(),
            body: self.body.to_runtime(),
        })
    }

    fn to_runtime_editor_intent(&self) -> Option<RequestEditorIntent> {
        self.editor_intent
            .as_ref()
            .map(RequestEditorIntentSnapshotV1::to_runtime)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyValueSnapshotV1 {
    name: String,
    value: String,
}

impl KeyValueSnapshotV1 {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn value(&self) -> &str {
        &self.value
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderSnapshotV1 {
    name: String,
    value: String,
}

impl HeaderSnapshotV1 {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn value(&self) -> &str {
        &self.value
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum HttpMethodSnapshotV1 {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

impl From<HttpMethod> for HttpMethodSnapshotV1 {
    fn from(method: HttpMethod) -> Self {
        match method {
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

impl From<HttpMethodSnapshotV1> for HttpMethod {
    fn from(method: HttpMethodSnapshotV1) -> Self {
        match method {
            HttpMethodSnapshotV1::Get => Self::GET,
            HttpMethodSnapshotV1::Post => Self::POST,
            HttpMethodSnapshotV1::Put => Self::PUT,
            HttpMethodSnapshotV1::Delete => Self::DELETE,
            HttpMethodSnapshotV1::Patch => Self::PATCH,
            HttpMethodSnapshotV1::Head => Self::HEAD,
            HttpMethodSnapshotV1::Options => Self::OPTIONS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum RequestBodySnapshotV1 {
    None,
    Json(String),
    Raw(String),
    UrlEncoded(String),
    Multipart(Vec<MultipartPartSnapshotV1>),
}

impl RequestBodySnapshotV1 {
    fn to_runtime(&self) -> RequestBody {
        match self {
            Self::None => RequestBody::None,
            Self::Json(value) => RequestBody::Json(value.clone()),
            Self::Raw(value) => RequestBody::Raw(value.clone()),
            Self::UrlEncoded(value) => RequestBody::UrlEncoded(value.clone()),
            Self::Multipart(parts) => RequestBody::Multipart(
                parts
                    .iter()
                    .map(MultipartPartSnapshotV1::to_runtime)
                    .collect(),
            ),
        }
    }

    fn validate(&self) -> Result<(), HistorySnapshotError> {
        if let Self::Multipart(parts) = self {
            for part in parts {
                if part.name.trim().is_empty() {
                    return Err(invalid_field(
                        "request.body.multipart.name",
                        "must not be empty",
                    ));
                }
                if matches!(&part.value, MultipartValueSnapshotV1::File { path, .. } if path.is_empty())
                {
                    return Err(invalid_field(
                        "request.body.multipart.path",
                        "must not be empty",
                    ));
                }
            }
        }
        Ok(())
    }

    fn validate_replay_files(&self) -> Result<(), HistorySnapshotError> {
        if let Self::Multipart(parts) = self {
            for part in parts {
                part.value.validate_replay_file()?;
            }
        }
        Ok(())
    }
}

impl TryFrom<&RequestBody> for RequestBodySnapshotV1 {
    type Error = HistorySnapshotError;

    fn try_from(body: &RequestBody) -> Result<Self, Self::Error> {
        match body {
            RequestBody::None => Ok(Self::None),
            RequestBody::Json(value) => Ok(Self::Json(value.clone())),
            RequestBody::Raw(value) => Ok(Self::Raw(value.clone())),
            RequestBody::UrlEncoded(value) => Ok(Self::UrlEncoded(value.clone())),
            RequestBody::Multipart(parts) => parts
                .iter()
                .map(MultipartPartSnapshotV1::try_from)
                .collect::<Result<Vec<_>, _>>()
                .map(Self::Multipart),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultipartPartSnapshotV1 {
    name: String,
    value: MultipartValueSnapshotV1,
}

impl MultipartPartSnapshotV1 {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn value(&self) -> &MultipartValueSnapshotV1 {
        &self.value
    }

    fn to_runtime(&self) -> MultipartPart {
        MultipartPart {
            name: self.name.clone(),
            value: self.value.to_runtime(),
        }
    }
}

impl TryFrom<&MultipartPart> for MultipartPartSnapshotV1 {
    type Error = HistorySnapshotError;

    fn try_from(part: &MultipartPart) -> Result<Self, Self::Error> {
        Ok(Self {
            name: part.name.clone(),
            value: MultipartValueSnapshotV1::try_from(&part.value)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MultipartValueSnapshotV1 {
    Text {
        value: String,
    },
    File {
        path: String,
        file_name: Option<String>,
        content_type: Option<String>,
    },
}

impl MultipartValueSnapshotV1 {
    fn to_runtime(&self) -> MultipartValue {
        match self {
            Self::Text { value } => MultipartValue::Text(value.clone()),
            Self::File {
                path,
                file_name,
                content_type,
            } => MultipartValue::File {
                path: PathBuf::from(path),
                file_name: file_name.clone(),
                content_type: content_type.clone(),
            },
        }
    }

    fn validate_replay_file(&self) -> Result<(), HistorySnapshotError> {
        if let Self::File { path, .. } = self {
            let path = PathBuf::from(path);
            if !path.is_file() {
                return Err(HistorySnapshotError::MissingMultipartFile { path });
            }
        }
        Ok(())
    }
}

impl TryFrom<&MultipartValue> for MultipartValueSnapshotV1 {
    type Error = HistorySnapshotError;

    fn try_from(value: &MultipartValue) -> Result<Self, Self::Error> {
        match value {
            MultipartValue::Text(value) => Ok(Self::Text {
                value: value.clone(),
            }),
            MultipartValue::File {
                path,
                file_name,
                content_type,
            } => Ok(Self::File {
                path: path
                    .to_str()
                    .ok_or(HistorySnapshotError::NonUtf8Path {
                        field: "multipart.path",
                    })?
                    .to_string(),
                file_name: file_name.clone(),
                content_type: content_type.clone(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "parts", rename_all = "snake_case")]
pub enum RequestEditorIntentSnapshotV1 {
    Multipart(Vec<MultipartEditorPartSnapshotV1>),
}

impl RequestEditorIntentSnapshotV1 {
    fn to_runtime(&self) -> RequestEditorIntent {
        match self {
            Self::Multipart(parts) => RequestEditorIntent::Multipart(
                parts
                    .iter()
                    .map(MultipartEditorPartSnapshotV1::to_runtime)
                    .collect(),
            ),
        }
    }

    fn validate(&self) -> Result<(), HistorySnapshotError> {
        // Incomplete editor-only rows are retained intentionally. They never enter the effective
        // Request and the explicit replay-file validator reports an enabled missing path.
        Ok(())
    }

    fn validate_replay_files(&self) -> Result<(), HistorySnapshotError> {
        match self {
            Self::Multipart(parts) => {
                for part in parts.iter().filter(|part| part.enabled) {
                    part.value.validate_replay_file()?;
                }
            }
        }
        Ok(())
    }
}

impl TryFrom<&RequestEditorIntent> for RequestEditorIntentSnapshotV1 {
    type Error = HistorySnapshotError;

    fn try_from(intent: &RequestEditorIntent) -> Result<Self, Self::Error> {
        match intent {
            RequestEditorIntent::Multipart(parts) => parts
                .iter()
                .filter(|part| !is_empty_editor_placeholder(part))
                .map(MultipartEditorPartSnapshotV1::try_from)
                .collect::<Result<Vec<_>, _>>()
                .map(Self::Multipart),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultipartEditorPartSnapshotV1 {
    enabled: bool,
    name: String,
    value: MultipartValueSnapshotV1,
}

impl MultipartEditorPartSnapshotV1 {
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn value(&self) -> &MultipartValueSnapshotV1 {
        &self.value
    }

    fn is_empty_placeholder(&self) -> bool {
        self.name.trim().is_empty()
            && match &self.value {
                MultipartValueSnapshotV1::Text { value } => value.is_empty(),
                MultipartValueSnapshotV1::File { path, .. } => path.is_empty(),
            }
    }

    fn to_runtime(&self) -> MultipartEditorPart {
        MultipartEditorPart {
            enabled: self.enabled,
            name: self.name.clone(),
            value: self.value.to_runtime(),
        }
    }
}

impl TryFrom<&MultipartEditorPart> for MultipartEditorPartSnapshotV1 {
    type Error = HistorySnapshotError;

    fn try_from(part: &MultipartEditorPart) -> Result<Self, Self::Error> {
        Ok(Self {
            enabled: part.enabled,
            name: part.name.clone(),
            value: MultipartValueSnapshotV1::try_from(&part.value)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestOptionsSnapshotV1 {
    timeout_ms: Option<u64>,
    redirect_policy: RedirectPolicySnapshotV1,
    max_hops: u32,
}

impl RequestOptionsSnapshotV1 {
    pub fn timeout_ms(&self) -> Option<u64> {
        self.timeout_ms
    }

    pub fn redirect_policy(&self) -> RedirectPolicySnapshotV1 {
        self.redirect_policy
    }

    pub fn max_hops(&self) -> u32 {
        self.max_hops
    }

    fn validate(&self) -> Result<(), HistorySnapshotError> {
        if self.timeout_ms == Some(0) {
            return Err(invalid_field(
                "request.options.timeout_ms",
                "zero must be represented as null",
            ));
        }
        if !(1..=MAX_REDIRECT_HOPS).contains(&self.max_hops) {
            return Err(invalid_field(
                "request.options.max_hops",
                "must be between 1 and 100",
            ));
        }
        Ok(())
    }

    fn to_runtime(self) -> RequestOptions {
        RequestOptions {
            timeout_ms: self.timeout_ms,
            redirect_policy: self.redirect_policy.into(),
            max_redirect_hops: self.max_hops,
        }
    }
}

impl TryFrom<RequestOptions> for RequestOptionsSnapshotV1 {
    type Error = HistorySnapshotError;

    fn try_from(options: RequestOptions) -> Result<Self, Self::Error> {
        let snapshot = Self {
            timeout_ms: options.timeout_ms,
            redirect_policy: options.redirect_policy.into(),
            max_hops: options.max_redirect_hops,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedirectPolicySnapshotV1 {
    Follow,
    DoNotFollow,
}

impl From<RedirectPolicy> for RedirectPolicySnapshotV1 {
    fn from(policy: RedirectPolicy) -> Self {
        match policy {
            RedirectPolicy::Follow => Self::Follow,
            RedirectPolicy::DoNotFollow => Self::DoNotFollow,
        }
    }
}

impl From<RedirectPolicySnapshotV1> for RedirectPolicy {
    fn from(policy: RedirectPolicySnapshotV1) -> Self {
        match policy {
            RedirectPolicySnapshotV1::Follow => Self::Follow,
            RedirectPolicySnapshotV1::DoNotFollow => Self::DoNotFollow,
        }
    }
}

/// Stateless policy applied before every repository boundary and again after decoding.
#[derive(Debug, Clone, Copy, Default)]
pub struct HistorySensitiveDataPolicy;

impl HistorySensitiveDataPolicy {
    pub fn is_sensitive_header_name(name: &str) -> bool {
        let name = compact_name(name);
        matches!(
            name.as_str(),
            "authorization"
                | "proxyauthorization"
                | "cookie"
                | "cookies"
                | "setcookie"
                | "apikey"
                | "xapikey"
                | "xauthkey"
                | "session"
                | "sessionid"
        ) || name.contains("token")
            || name.contains("apikey")
            || name.contains("secret")
            || name.contains("password")
            || name.contains("session")
            || name.contains("credential")
    }

    pub fn is_sensitive_query_name(name: &str) -> bool {
        let name = compact_name(name);
        matches!(
            name.as_str(),
            "auth"
                | "authorization"
                | "apikey"
                | "xapikey"
                | "accesskey"
                | "accesskeyid"
                | "accesstoken"
                | "authtoken"
                | "bearertoken"
                | "clientsecret"
                | "idtoken"
                | "password"
                | "passwd"
                | "refreshtoken"
                | "secret"
                | "session"
                | "sessionid"
                | "token"
        ) || name.ends_with("apikey")
            || name.ends_with("accesstoken")
            || name.ends_with("authtoken")
            || name.ends_with("password")
            || name.ends_with("secret")
            || name.ends_with("signature")
            || name.ends_with("credential")
            || name.ends_with("securitytoken")
    }
}

fn response_media_type(headers: &[HeaderSnapshotV1]) -> Option<String> {
    headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("content-type"))
        .map(|header| header.value.clone())
}

fn sanitize_media_type(media_type: Option<String>) -> Result<Option<String>, HistorySnapshotError> {
    media_type
        .map(|media_type| {
            let media_type = media_type.trim().to_string();
            HeaderValue::from_str(&media_type).map_err(|_| {
                invalid_field("response.media_type", "must be a valid HTTP header value")
            })?;
            Ok(media_type)
        })
        .transpose()
}

fn normalized_media_type(media_type: Option<&str>) -> Option<String> {
    media_type.map(|value| {
        value
            .split(';')
            .next()
            .unwrap_or(value)
            .trim()
            .to_ascii_lowercase()
    })
}

fn is_textual_response_media_type(media_type: Option<&str>) -> bool {
    let Some(media_type) = normalized_media_type(media_type) else {
        // The current transport exposes a UTF-8 String. In the absence of response metadata,
        // treating it as plain text preserves deterministic local/test responses.
        return true;
    };
    if media_type == "text/event-stream" {
        return false;
    }
    media_type.starts_with("text/")
        || media_type == "application/json"
        || media_type.ends_with("+json")
        || media_type == "application/x-ndjson"
        || media_type == "application/xml"
        || media_type.ends_with("+xml")
        || media_type == "application/x-www-form-urlencoded"
        || media_type == "application/javascript"
        || media_type == "application/graphql"
        || media_type == "application/yaml"
        || media_type == "application/x-yaml"
}

fn is_json_response_media_type(media_type: Option<&str>) -> bool {
    normalized_media_type(media_type).is_some_and(|media_type| {
        media_type == "application/json"
            || media_type.ends_with("+json")
            || media_type == "application/x-ndjson"
    })
}

fn is_form_response_media_type(media_type: Option<&str>) -> bool {
    normalized_media_type(media_type)
        .is_some_and(|media_type| media_type == "application/x-www-form-urlencoded")
}

fn is_download_response(headers: &[HeaderSnapshotV1]) -> bool {
    headers.iter().any(|header| {
        header.name.eq_ignore_ascii_case("content-disposition") && {
            let value = header.value.to_ascii_lowercase();
            value.contains("attachment") || value.contains("filename=")
        }
    })
}

fn sanitize_structured_response_body(body: String, media_type: Option<&str>) -> String {
    if is_json_response_media_type(media_type) {
        if let Ok(mut value) = serde_json::from_str::<Value>(&body) {
            redact_json_secrets(&mut value);
            return serde_json::to_string(&value).unwrap_or(body);
        }
    } else if is_form_response_media_type(media_type) {
        let pairs = form_urlencoded::parse(body.as_bytes()).collect::<Vec<_>>();
        let mut serializer = form_urlencoded::Serializer::new(String::new());
        for (name, value) in pairs {
            if is_sensitive_response_field(&name) {
                serializer.append_pair(&name, "[REDACTED]");
            } else {
                serializer.append_pair(&name, &value);
            }
        }
        return serializer.finish();
    }
    // Arbitrary plain/raw text has no reliable field structure. The defined V2 policy stores it
    // verbatim after header sanitization and byte bounding; callers must not infer secret fields.
    body
}

fn redact_json_secrets(value: &mut Value) {
    match value {
        Value::Object(values) => {
            for (name, value) in values {
                if is_sensitive_response_field(name) {
                    *value = Value::String("[REDACTED]".to_string());
                } else {
                    redact_json_secrets(value);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                redact_json_secrets(value);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn is_sensitive_response_field(name: &str) -> bool {
    HistorySensitiveDataPolicy::is_sensitive_query_name(name)
        || HistorySensitiveDataPolicy::is_sensitive_header_name(name)
}

fn truncate_utf8_preview(mut value: String, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value, false);
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value.truncate(boundary);
    (value, true)
}

fn parse_http_url(value: &str) -> Result<Url, HistorySnapshotError> {
    let url = Url::parse(value)
        .map_err(|_| invalid_field("request.url", "must be a valid absolute HTTP or HTTPS URL"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(invalid_field(
            "request.url",
            "must be a valid absolute HTTP or HTTPS URL",
        ));
    }
    Ok(url)
}

fn clear_url_credentials_and_non_base_parts(url: &mut Url) -> Result<(), HistorySnapshotError> {
    url.set_username("")
        .map_err(|()| invalid_field("request.url", "must permit credential sanitization"))?;
    url.set_password(None)
        .map_err(|()| invalid_field("request.url", "must permit credential sanitization"))?;
    url.set_query(None);
    url.set_fragment(None);
    Ok(())
}

fn compact_name(name: &str) -> String {
    name.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn sanitize_history_name(
    name: &str,
    original_url: &str,
    sanitized_url: &str,
    secret_values: &[String],
) -> String {
    if name == original_url {
        return sanitized_url.to_string();
    }
    if let Some(prefix) = name.strip_suffix('…') {
        if original_url.starts_with(prefix) {
            let prefix_length = prefix.chars().count();
            let sanitized_length = sanitized_url.chars().count();
            let mut value = sanitized_url
                .chars()
                .take(prefix_length)
                .collect::<String>();
            if sanitized_length > prefix_length {
                value.push('…');
            }
            return value;
        }
    }

    let mut sanitized = name.to_string();
    let mut values = secret_values
        .iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    values.sort_by_key(|value| std::cmp::Reverse(value.len()));
    values.dedup();
    for value in values {
        sanitized = sanitized.replace(value, "[REDACTED]");
    }
    sanitized
}

fn is_empty_editor_placeholder(part: &MultipartEditorPart) -> bool {
    part.name.trim().is_empty()
        && match &part.value {
            MultipartValue::Text(value) => value.is_empty(),
            MultipartValue::File { path, .. } => path.as_os_str().is_empty(),
        }
}

fn invalid_field(field: &'static str, reason: &'static str) -> HistorySnapshotError {
    HistorySnapshotError::InvalidField { field, reason }
}

fn malformed_payload(error: serde_json::Error) -> HistorySnapshotError {
    HistorySnapshotError::MalformedPayload {
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests;

//! Durable editable request data. This is separate from transport requests and redacted History.
use super::request_draft::RequestDraft;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestDocument {
    version: u32,
    id: String,
    name: String,
    revision: u64,
    draft: RequestDraft,
    pre_request_script: String,
    tests_script: String,
}
impl RequestDocument {
    pub(crate) fn snapshot(
        id: String,
        name: String,
        revision: u64,
        draft: &RequestDraft,
        pre_request_script: &str,
        tests_script: &str,
    ) -> Self {
        Self {
            version: 1,
            id,
            name,
            revision,
            draft: draft.document_snapshot(),
            pre_request_script: pre_request_script.into(),
            tests_script: tests_script.into(),
        }
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn draft(&self) -> &RequestDraft {
        &self.draft
    }
    pub fn pre_request_script(&self) -> &str {
        &self.pre_request_script
    }
    pub fn tests_script(&self) -> &str {
        &self.tests_script
    }
    pub(crate) fn same_content(&self, draft: &RequestDraft, pre: &str, tests: &str) -> bool {
        self.draft.same_document_content(draft)
            && self.pre_request_script == pre
            && self.tests_script == tests
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("Unsupported request document version".into());
        }
        uuid::Uuid::parse_str(&self.id).map_err(|_| "Invalid request identity")?;
        Self::validate_name(&self.name)?;
        if self.revision == 0 || self.revision > i64::MAX as u64 {
            return Err("Invalid request revision".into());
        }
        Ok(())
    }
    pub fn validate_name(name: &str) -> Result<(), String> {
        if name.trim().is_empty()
            || name.chars().count() > 120
            || name.chars().any(char::is_control)
        {
            return Err("Enter a request name of 1–120 characters, without line breaks.".into());
        }
        Ok(())
    }
}

//! Session documents. Selection and filtering never mutate the engine's source document.
use crate::models::HttpMethod;
use postman_flow::{
    FlowDefinition, FlowDocument, HttpRequestSource, HttpRequestTemplate, HttpStepDefinition,
    TemplatePart, TextTemplate,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct FlowId(pub u64);

pub(super) struct FlowDraft {
    id: FlowId,
    document: FlowDocument,
    selected: Option<String>,
    revision: u64,
}

impl FlowDraft {
    pub(super) fn id(&self) -> FlowId {
        self.id
    }
    pub(super) fn document(&self) -> &FlowDocument {
        &self.document
    }
    pub(super) fn revision(&self) -> u64 {
        self.revision
    }
    pub(super) fn title(&self) -> &str {
        if self.document.flow.name.trim().is_empty() {
            "Untitled flow"
        } else {
            &self.document.flow.name
        }
    }
    pub(super) fn selected(&self) -> Option<&HttpStepDefinition> {
        self.document
            .flow
            .steps
            .iter()
            .find(|s| Some(s.id.as_str()) == self.selected.as_deref())
    }
}

#[derive(Default)]
pub(super) struct FlowLibrary {
    drafts: Vec<FlowDraft>,
    active: Option<FlowId>,
    next_id: u64,
}

impl FlowLibrary {
    pub(super) fn drafts(&self) -> &[FlowDraft] {
        &self.drafts
    }
    pub(super) fn active(&self) -> Option<&FlowDraft> {
        self.drafts.iter().find(|d| Some(d.id) == self.active)
    }
    fn active_mut(&mut self) -> Option<&mut FlowDraft> {
        self.drafts.iter_mut().find(|d| Some(d.id) == self.active)
    }
    pub(super) fn create(&mut self, name: &str) -> Result<FlowId, &'static str> {
        validate_name(name)?;
        Ok(self.insert(FlowDocument::new(FlowDefinition::new(name.trim()))))
    }
    pub(super) fn insert(&mut self, document: FlowDocument) -> FlowId {
        self.next_id += 1;
        let id = FlowId(self.next_id);
        let selected = document.flow.steps.first().map(|s| s.id.clone());
        self.drafts.push(FlowDraft {
            id,
            document,
            selected,
            revision: 0,
        });
        self.active = Some(id);
        id
    }
    pub(super) fn select(&mut self, id: FlowId) -> bool {
        if self.drafts.iter().any(|d| d.id == id) {
            self.active = Some(id);
            true
        } else {
            false
        }
    }
    pub(super) fn select_step(&mut self, id: &str) {
        if let Some(draft) = self.active_mut() {
            if draft.document.flow.steps.iter().any(|s| s.id == id) {
                draft.selected = Some(id.to_owned());
            }
        }
    }
    pub(super) fn rename(&mut self, name: String) {
        if let Some(draft) = self.active_mut() {
            if draft.document.flow.name != name {
                draft.document.flow.name = name;
                draft.revision += 1;
            }
        }
    }
    pub(super) fn append_step(&mut self) {
        let Some(draft) = self.active_mut() else {
            return;
        };
        let mut number = draft.document.flow.steps.len() + 1;
        while draft
            .document
            .flow
            .steps
            .iter()
            .any(|s| s.id == format!("request-{number}"))
        {
            number += 1;
        }
        let id = format!("request-{number}");
        draft.document.flow.steps.push(HttpStepDefinition::new(
            &id,
            format!("Request {number}"),
            HttpRequestTemplate::new(HttpMethod::GET, TextTemplate::literal("")),
        ));
        draft.selected = Some(id);
        draft.revision += 1;
    }
    /// Target identity travels with the editor event, including an invalid raw-source edit.
    pub(super) fn edit_step(&mut self, flow: FlowId, id: &str, edit: &StepEdit) {
        let Some(draft) = self.drafts.iter_mut().find(|d| d.id == flow) else {
            return;
        };
        let Some(step) = draft.document.flow.steps.iter_mut().find(|s| s.id == id) else {
            return;
        };
        let changed = match edit {
            StepEdit::Name(value) => replace_if_changed(&mut step.name, value.clone()),
            _ => match &mut step.request {
                HttpRequestSource::Inline(request) => match edit {
                    StepEdit::Method(value) => replace_if_changed(&mut request.method, *value),
                    StepEdit::Url(value) => {
                        request.url = value.clone();
                        true
                    }
                    StepEdit::Headers(value) => {
                        request.headers = value.clone();
                        true
                    }
                    StepEdit::Body(value) => {
                        request.body = value.clone();
                        true
                    }
                    StepEdit::Checks(value) => {
                        step.checks = value.clone();
                        true
                    }
                    StepEdit::Exports(value) => {
                        step.exports = value.clone();
                        true
                    }
                    StepEdit::Condition(value) => {
                        step.when = value.clone();
                        true
                    }
                    StepEdit::Invalid => true,
                    StepEdit::Name(_) => unreachable!(),
                },
                _ => false,
            },
        };
        if changed {
            draft.revision += 1;
        }
    }
}

pub(super) enum StepEdit {
    Name(String),
    Method(HttpMethod),
    Url(TextTemplate),
    Headers(Vec<(TextTemplate, TextTemplate)>),
    Body(postman_flow::BodyTemplate),
    Checks(Vec<postman_flow::ResponseCheck>),
    Exports(Vec<postman_flow::ResponseExport>),
    Condition(Option<postman_flow::ConditionExpr>),
    Invalid,
}

fn replace_if_changed<T: PartialEq>(target: &mut T, value: T) -> bool {
    if *target == value {
        false
    } else {
        *target = value;
        true
    }
}

pub(super) fn validate_name(name: &str) -> Result<(), &'static str> {
    if name.trim().is_empty() {
        Err("Enter a flow name.")
    } else if name.chars().count() > 80 {
        Err("Use at most 80 characters.")
    } else if name.chars().any(char::is_control) {
        Err("Use a single-line name without control characters.")
    } else {
        Ok(())
    }
}

pub(super) fn literal_url(url: &TextTemplate) -> Option<&str> {
    match url.parts.as_slice() {
        [TemplatePart::Literal(value)] => Some(value),
        [] => Some(""),
        _ => None,
    }
}

pub(super) fn request_summary(step: &HttpStepDefinition) -> (Option<HttpMethod>, String) {
    match &step.request {
        HttpRequestSource::Inline(request) => (
            Some(request.method),
            literal_url(&request.url)
                .map(|url| {
                    if url.is_empty() {
                        "No URL yet".into()
                    } else {
                        url.into()
                    }
                })
                .unwrap_or_else(|| "URL expression · preserved".into()),
        ),
        HttpRequestSource::Api(api) => (None, format!("API reference · {}", api.api_id)),
        HttpRequestSource::ForEach(_) => (None, "For each · preserved".into()),
        HttpRequestSource::RepeatUntil(_) => (None, "Repeat until · preserved".into()),
    }
}

#[cfg(test)]
mod tests;

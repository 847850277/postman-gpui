//! Adapt compiler/document draft locations to retained Kit fields, without parsing messages.
use super::super::validation::{indexed, InspectorTarget, Issue, Target};
use super::*;

impl RequestInspector {
    pub(in crate::app::postman_app::flows) fn draft_issues(&self) -> Vec<Issue> {
        let mut issues = Vec::new();
        let mut add = |field: &str, message: &str, target| {
            issues.push(Issue {
                message: format!(
                    "UnappliedDraft · step {} · {field}: {message}",
                    self.step_id
                ),
                target: Target::Step(self.step_id.clone(), target),
            })
        };
        for (field, error) in [
            ("request.url", &self.url_error),
            ("request.headers", &self.headers_error),
            ("request.body", &self.body_error),
            ("when", &self.condition_error),
        ] {
            if let Some(error) = error {
                add(field, error, InspectorTarget::Field(field.into()));
            }
        }
        for row in &self.checks {
            if let Some(error) = &row.error {
                add("checks", error, InspectorTarget::Check(row.id));
            }
        }
        for row in &self.exports {
            if row.error.is_some() || row.value.is_none() {
                add(
                    "exports",
                    row.error
                        .as_deref()
                        .unwrap_or("Complete or remove this export."),
                    InspectorTarget::Export(row.id),
                );
            }
        }
        issues
    }
    pub(in crate::app::postman_app::flows) fn locate(
        &mut self,
        target: &InspectorTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = match target {
            InspectorTarget::Field(field) if self.readonly.is_none() => {
                if let Some((index, _)) = indexed(field, "checks") {
                    self.checks
                        .iter()
                        .filter(|r| r.value.is_some())
                        .nth(index)
                        .map(|r| InspectorTarget::Check(r.id))
                        .unwrap_or_else(|| target.clone())
                } else if let Some((index, _)) = indexed(field, "exports") {
                    self.exports
                        .iter()
                        .filter(|r| r.value.is_some())
                        .nth(index)
                        .map(|r| InspectorTarget::Export(r.id))
                        .unwrap_or_else(|| target.clone())
                } else {
                    target.clone()
                }
            }
            _ => target.clone(),
        };
        match target {
            InspectorTarget::Check(id) => {
                self.tab = 1;
                if let Some(row) = self.checks.iter().find(|r| r.id == id) {
                    row.input
                        .as_ref()
                        .map(|i| i.focus_handle(cx))
                        .unwrap_or_else(|| self.tab_focus[1].clone())
                        .focus(window, cx);
                    row.anchor.scroll_to(window, cx);
                }
            }
            InspectorTarget::Export(id) => {
                self.tab = 2;
                if let Some(row) = self.exports.iter().find(|r| r.id == id) {
                    if postman_flow::validate_definition_name(&row.name.read(cx).value()).is_ok()
                        && postman_flow::validate_json_path(&row.path.read(cx).value()).is_err()
                    {
                        row.path.focus_handle(cx).focus(window, cx);
                    } else {
                        row.name.focus_handle(cx).focus(window, cx);
                    }
                    row.anchor.scroll_to(window, cx);
                }
            }
            InspectorTarget::Field(field) => {
                self.tab = 0;
                if self.readonly.is_some() {
                    self.tab_focus[0].focus(window, cx);
                    self.tab_scroll[0]
                        .set_offset(gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(0.)));
                } else {
                    let (index, focus) = if field.starts_with("when") {
                        self.condition_open = true;
                        (3, self.condition.focus_handle(cx))
                    } else if field.starts_with("request.headers") {
                        self.headers_open = true;
                        (
                            1,
                            if self.expression_headers {
                                self.header_expression.focus_handle(cx)
                            } else {
                                self.headers.focus_handle(cx)
                            },
                        )
                    } else if field.starts_with("request.body") {
                        (2, self.body.focus_handle(cx))
                    } else if field.starts_with("request.url") {
                        (
                            0,
                            if self.expression_url {
                                self.url_expression.focus_handle(cx)
                            } else {
                                self.url.focus_handle(cx)
                            },
                        )
                    } else {
                        (4, self.name.focus_handle(cx))
                    };
                    focus.focus(window, cx);
                    if let Some(anchor) = self.request_anchors.get(index) {
                        anchor.scroll_to(window, cx);
                    } else {
                        self.tab_scroll[0]
                            .set_offset(gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(0.)));
                    }
                }
            }
        }
        cx.notify();
    }
}

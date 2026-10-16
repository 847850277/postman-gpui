//! Document-only validation. Locations capture a revision, never a mutable list index alone.
use super::{console, FlowId, FlowsView};
use gpui::{App, AppContext, Context, Focusable, Window};
use gpui_kit::component::{notification::Notification, WindowExt};
use postman_flow::{compile_flow, CompileEnvironment};

#[derive(Clone, Debug)]
pub(super) enum InspectorTarget {
    Field(String),
    Check(u64),
    Export(u64),
}
#[derive(Clone, Debug)]
pub(super) enum Target {
    Root(String),
    RuntimeInputs,
    Input(u64),
    Step(String, InspectorTarget),
}
#[derive(Clone, Debug)]
pub(super) struct DiagnosticTarget {
    pub(super) flow: FlowId,
    pub(super) revision: u64,
    pub(super) target: Target,
}
pub(super) struct Issue {
    pub(super) message: String,
    pub(super) target: Target,
}

/// Only structured compiler fields enter this parser; display messages are never navigation data.
pub(super) fn indexed<'a>(field: &'a str, prefix: &str) -> Option<(usize, &'a str)> {
    let rest = field.strip_prefix(prefix)?.strip_prefix('[')?;
    let (index, suffix) = rest.split_once(']')?;
    Some((index.parse().ok()?, suffix.trim_start_matches('.')))
}
fn contains_step(step: &postman_flow::HttpStepDefinition, id: &str) -> bool {
    if step.id == id {
        return true;
    }
    let children = match &step.request {
        postman_flow::HttpRequestSource::ForEach(value) => &value.steps,
        postman_flow::HttpRequestSource::RepeatUntil(value) => &value.steps,
        _ => return false,
    };
    children.iter().any(|step| contains_step(step, id))
}
impl FlowsView {
    pub(super) fn draft_issues(&self, flow: FlowId, cx: &App) -> Vec<Issue> {
        let mut issues = Vec::new();
        if let Some(editor) = self.inputs.get(&flow) {
            issues.extend(editor.read(cx).draft_issues());
        }
        // Use document order for deterministic diagnostics; retained, hidden inspectors count too.
        if let Some(draft) = self.library.drafts().iter().find(|d| d.id() == flow) {
            for step in &draft.document().flow.steps {
                if let Some(editor) = self.inspectors.get(&(flow, step.id.clone())) {
                    issues.extend(editor.read(cx).draft_issues());
                }
            }
        }
        issues
    }
    pub(super) fn check_flow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.library.active() else {
            return;
        };
        let (flow, revision) = (draft.id(), draft.revision());
        let issues = self.prepare_plan(flow, cx).err().unwrap_or_default();
        self.console.update(cx, |console, cx| {
            console.report(flow, revision, issues, window, cx)
        });
        cx.notify();
    }
    pub(super) fn prepare_plan(
        &self,
        flow: FlowId,
        cx: &App,
    ) -> Result<postman_flow::FlowPlan, Vec<Issue>> {
        let draft = self.library.get(flow).expect("open Flow");
        let mut issues = self.draft_issues(flow, cx);
        let document = draft.document();
        let compiled = compile_flow(
            &document.flow,
            &document.apis,
            &CompileEnvironment::default(),
        );
        if let Err(errors) = &compiled {
            issues.extend(errors.iter().map(|error| {
                let field = &error.location.field;
                // Nested/API diagnostics reveal their top-level readonly owner. Do not confuse
                // a scoped namesake with another top-level step or invent an editable field.
                let target = indexed(field, "flow.steps")
                    .and_then(|(index, suffix)| {
                        document.flow.steps.get(index).map(|step| {
                            Target::Step(step.id.clone(), InspectorTarget::Field(suffix.into()))
                        })
                    })
                    .or_else(|| {
                        let id = error.location.step_id.as_deref()?;
                        let mut owners = document
                            .flow
                            .steps
                            .iter()
                            .filter(|step| contains_step(step, id));
                        let owner = owners.next()?;
                        owners.next().is_none().then(|| {
                            Target::Step(owner.id.clone(), InspectorTarget::Field(field.clone()))
                        })
                    })
                    .or_else(|| {
                        indexed(field, "flow.inputs").and_then(|(index, _)| {
                            self.inputs
                                .get(&flow)?
                                .read(cx)
                                .definition_row_id(index)
                                .map(Target::Input)
                        })
                    })
                    .unwrap_or_else(|| Target::Root(field.clone()));
                Issue {
                    message: format!("{:?} · {error}", error.code),
                    target,
                }
            }));
        }
        if issues.is_empty() {
            Ok(compiled.expect("compiler errors were collected"))
        } else {
            Err(issues)
        }
    }
    /// Called at document edit boundaries, not during rendering. Hidden YAML is refreshed lazily.
    pub(super) fn document_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(draft) = self.library.active() {
            self.console.update(cx, |console, cx| {
                console.document_changed(draft.id(), draft.revision(), cx)
            });
        }
        if self.yaml_open {
            self.refresh_yaml(window, cx);
        }
    }
    pub(super) fn show_yaml(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.conceal_inputs(cx);
        self.yaml_open = open;
        if open {
            self.refresh_yaml(window, cx);
        }
        cx.notify();
    }
    fn refresh_yaml(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.library.active() else {
            return;
        };
        let flow = draft.id();
        let pending = !self.draft_issues(flow, cx).is_empty();
        let preview = self
            .previews
            .entry(flow)
            .or_insert_with(|| cx.new(|cx| super::yaml::YamlPreview::new(window, cx)));
        preview.update(cx, |preview, cx| {
            preview.refresh(draft.document(), draft.revision(), pending, window, cx)
        });
    }
    pub(super) fn locate_diagnostic(
        &mut self,
        target: &DiagnosticTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self
            .library
            .active()
            .is_some_and(|d| d.id() == target.flow && d.revision() == target.revision)
        {
            window.push_notification(
                Notification::info("This diagnostic is stale. Check the current flow again."),
                cx,
            );
            return;
        }
        self.yaml_open = false;
        self.console.update(cx, |console, cx| {
            console.set_mode(console::ConsoleMode::Normal, cx)
        });
        match &target.target {
            Target::RuntimeInputs => {
                self.open_inputs(window, cx);
                if let Some(editor) = self.inputs.get(&target.flow) {
                    editor.update(cx, |editor, cx| editor.show_runtime(window, cx));
                }
            }
            Target::Input(row) => {
                self.open_inputs(window, cx);
                if let Some(editor) = self.inputs.get(&target.flow) {
                    editor.update(cx, |editor, cx| editor.locate_definition(*row, window, cx));
                }
            }
            Target::Step(step, field) => {
                self.select_step(step, window, cx);
                self.details = true;
                self.reveal_step();
                if let Some(editor) = self.inspector() {
                    editor.update(cx, |editor, cx| editor.locate(field, window, cx));
                }
            }
            Target::Root(field) => {
                self.inputs_open = false;
                self.details = false;
                if field == "flow.name" {
                    self.title.focus_handle(cx).focus(window, cx);
                } else {
                    self.canvas_focus.focus(window, cx);
                    window.push_notification(Notification::info(format!("{field}: document-level configuration; no editable field is available yet.")), cx);
                }
            }
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests;

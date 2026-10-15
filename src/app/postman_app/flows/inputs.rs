//! Flow-owned declaration drafts and run overrides; only definition edits reach the document.
#[cfg(test)]
mod tests;
mod value;
mod view;
use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, ScrollHandle,
    Subscription, Window,
};
use gpui_kit::component::input::{InputEvent, InputState};
use postman_flow::{validate_definition_name, FlowInputSpec, FlowInputs};
use serde_json::Value;
use std::{borrow::Cow, collections::HashMap};
use value::ValueEditor;

pub(super) enum InputsEvent {
    Definitions {
        values: Vec<FlowInputSpec>,
        invalid: bool,
    },
    Back,
}
impl EventEmitter<InputsEvent> for InputsInspector {}

struct Row {
    id: u64,
    name: Entity<InputState>,
    default: Entity<ValueEditor>,
    runtime: Entity<ValueEditor>,
    has_default: bool,
    sensitive: bool,
    overridden: bool,
    applied: Option<FlowInputSpec>,
    error: Option<String>,
    anchor: gpui::ScrollAnchor,
    _subscriptions: Vec<Subscription>,
}
pub(super) struct InputsInspector {
    rows: Vec<Row>,
    next_id: u64,
    definitions: bool,
    scroll: [ScrollHandle; 2],
    add_focus: FocusHandle,
    tab_focus: [FocusHandle; 2],
    environment: Entity<crate::app::environments::Environments>,
    _environment: Subscription,
}
impl Focusable for InputsInspector {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.tab_focus[usize::from(self.definitions)].clone()
    }
}
impl InputsInspector {
    pub(super) fn new(
        values: &[FlowInputSpec],
        environment: Entity<crate::app::environments::Environments>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscription = cx.observe(&environment, |_, _, cx| cx.notify());
        let mut this = Self {
            rows: Vec::new(),
            next_id: 0,
            definitions: false,
            scroll: std::array::from_fn(|_| ScrollHandle::new()),
            add_focus: cx.focus_handle().tab_stop(true),
            tab_focus: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            environment,
            _environment: subscription,
        };
        for value in values {
            let row = this.row(Some(value.clone()), window, cx);
            this.rows.push(row);
        }
        this
    }
    fn row(
        &mut self,
        applied: Option<FlowInputSpec>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Row {
        let id = self.next_id;
        self.next_id += 1;
        let name = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("Input name");
            input.set_value(
                applied.as_ref().map(|v| v.name.clone()).unwrap_or_default(),
                window,
                cx,
            );
            input
        });
        let default = cx.new(|cx| {
            ValueEditor::new(
                format!("flow-input-default-{id}"),
                applied
                    .as_ref()
                    .and_then(|v| v.default.clone())
                    .unwrap_or(Value::String(String::new())),
                window,
                cx,
            )
        });
        let runtime = cx.new(|cx| {
            ValueEditor::new(
                format!("flow-input-runtime-{id}"),
                Value::String(String::new()),
                window,
                cx,
            )
        });
        let sensitive = applied.as_ref().is_some_and(|v| v.sensitive);
        for editor in [&default, &runtime] {
            editor.update(cx, |v, cx| v.sensitive(sensitive, cx));
        }
        let subscriptions = vec![
            cx.subscribe(&name, move |this, _, event, cx| {
                if matches!(event, InputEvent::Change) && this.rows.iter().any(|r| r.id == id) {
                    this.apply(cx);
                }
            }),
            cx.subscribe(&default, move |this, _, _: &value::Changed, cx| {
                if this.rows.iter().any(|r| r.id == id && r.has_default) {
                    this.apply(cx);
                }
            }),
            cx.subscribe(&runtime, |_, _, _: &value::Changed, cx| cx.notify()),
        ];
        Row {
            id,
            name,
            has_default: applied.as_ref().is_some_and(|v| v.default.is_some()),
            default,
            runtime,
            sensitive,
            overridden: false,
            applied,
            error: None,
            anchor: gpui::ScrollAnchor::for_handle(self.scroll[1].clone()),
            _subscriptions: subscriptions,
        }
    }
    fn apply(&mut self, cx: &mut Context<Self>) {
        let candidates = self
            .rows
            .iter()
            .map(|r| {
                let name = r.name.read(cx).value().to_string();
                validate_definition_name(&name).map_err(str::to_owned)?;
                let default = if r.has_default {
                    Some(r.default.read(cx).parsed.clone().map_err(str::to_owned)?)
                } else {
                    None
                };
                Ok(FlowInputSpec {
                    name,
                    default,
                    sensitive: r.sensitive,
                })
            })
            .collect();
        let previous: Vec<_> = self.rows.iter().map(|r| r.applied.clone()).collect();
        for (row, result) in self.rows.iter_mut().zip(reconcile(candidates, &previous)) {
            row.error = result.as_ref().err().cloned();
            if let Ok(value) = result {
                row.applied = Some(value);
            }
            let sensitive = row.sensitive || row.applied.as_ref().is_some_and(|v| v.sensitive);
            for editor in [&row.default, &row.runtime] {
                editor.update(cx, |v, cx| v.sensitive(sensitive, cx));
            }
        }
        cx.emit(InputsEvent::Definitions {
            values: self.rows.iter().filter_map(|r| r.applied.clone()).collect(),
            invalid: self.rows.iter().any(|r| r.error.is_some()),
        });
        cx.notify();
    }
    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.conceal(cx);
        self.definitions = true;
        let row = self.row(None, window, cx);
        row.name.focus_handle(cx).focus(window, cx);
        row.anchor.scroll_to(window, cx);
        self.rows.push(row);
        self.apply(cx);
    }
    fn remove(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.rows.iter().position(|r| r.id == id) else {
            return;
        };
        self.rows.remove(ix);
        self.rows
            .get(ix)
            .or_else(|| self.rows.last())
            .map(|r| r.name.focus_handle(cx))
            .unwrap_or_else(|| self.add_focus.clone())
            .focus(window, cx);
        self.apply(cx);
    }
    pub(super) fn conceal(&self, cx: &mut Context<Self>) {
        for row in &self.rows {
            for editor in [&row.default, &row.runtime] {
                editor.update(cx, |v, cx| v.conceal(cx));
            }
        }
    }
    fn set_definitions(&mut self, definitions: bool, cx: &mut Context<Self>) {
        self.conceal(cx);
        self.definitions = definitions;
        cx.notify();
    }
    fn inherited<'a>(
        &'a self,
        row: &'a Row,
        cx: &'a App,
    ) -> (Option<Cow<'a, Value>>, &'static str) {
        let Some(spec) = row.applied.as_ref() else {
            return (None, "Incomplete definition");
        };
        if spec.name == "base_url" {
            if let Some(base) = self.environment.read(cx).active_base() {
                return (
                    Some(Cow::Owned(Value::String(base.to_owned()))),
                    "Environment",
                );
            }
        }
        (
            spec.default.as_ref().map(Cow::Borrowed),
            if spec.default.is_some() {
                "Default"
            } else {
                "Required / missing"
            },
        )
    }
    fn override_value(
        &mut self,
        id: u64,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self.rows.iter().find(|r| r.id == id) else {
            return;
        };
        if enabled {
            let value = self
                .inherited(row, cx)
                .0
                .map(Cow::into_owned)
                .unwrap_or(Value::String(String::new()));
            row.runtime
                .update(cx, |v, cx| v.set_value(value, window, cx));
        }
        row.runtime.update(cx, |v, cx| v.conceal(cx));
        self.rows
            .iter_mut()
            .find(|r| r.id == id)
            .unwrap()
            .overridden = enabled;
        cx.notify();
    }
    /// Explicit future-run boundary. Rendering never prepares a snapshot. Invalid drafts block it.
    #[allow(dead_code)] // P6 consumes this boundary; exercised through native controls now.
    pub(super) fn prepare(&self, cx: &App) -> Result<FlowInputs, Vec<String>> {
        let mut values = FlowInputs::new();
        let mut errors = Vec::new();
        for row in &self.rows {
            if row.error.is_some() || row.applied.is_none() {
                errors.push("Correct the input definition before preparing parameters.".into());
                continue;
            }
            let spec = row.applied.as_ref().unwrap();
            let value = if row.overridden {
                row.runtime.read(cx).parsed.clone().map_err(str::to_owned)
            } else {
                self.inherited(row, cx)
                    .0
                    .map(Cow::into_owned)
                    .ok_or_else(|| format!("Required input '{}' is missing.", spec.name))
            };
            match value {
                Ok(value) => values.insert(spec.name.clone(), value),
                Err(error) => errors.push(error),
            }
        }
        if errors.is_empty() {
            Ok(values)
        } else {
            Err(errors)
        }
    }
}
fn reconcile(
    mut candidates: Vec<Result<FlowInputSpec, String>>,
    previous: &[Option<FlowInputSpec>],
) -> Vec<Result<FlowInputSpec, String>> {
    loop {
        let mut names: HashMap<&str, Vec<usize>> = HashMap::new();
        for (ix, result) in candidates.iter().enumerate() {
            if let Some(v) = result.as_ref().ok().or(previous[ix].as_ref()) {
                names.entry(&v.name).or_default().push(ix);
            }
        }
        let mut conflicts = Vec::new();
        for (name, rows) in names.into_iter().filter(|(_, rows)| rows.len() > 1) {
            let owners: Vec<_> = rows
                .iter()
                .copied()
                .filter(|ix| previous[*ix].as_ref().is_some_and(|v| v.name == name))
                .collect();
            let owner = (owners.len() == 1).then(|| owners[0]);
            for ix in rows {
                if Some(ix) != owner && candidates[ix].is_ok() {
                    conflicts.push(ix);
                }
            }
        }
        if conflicts.is_empty() {
            return candidates;
        }
        for ix in conflicts {
            candidates[ix] = Err("Input name is already declared.".into());
        }
    }
}

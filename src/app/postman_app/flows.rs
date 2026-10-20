//! Native, session-owned Flow editing. Kit owns input, selection controls and overlays.
mod console;
mod execution;
mod files;
pub(super) use files::SaveKind;
mod controls;
mod inputs;
mod layout;
mod model;
mod request_inspector;
mod request_source;
mod step_actions;
mod transfer;
mod validation;
mod yaml;
pub(super) use transfer::open as open_transfer;
#[cfg(test)]
mod tests;
mod view;

use crate::app::environments::Environments;
use gpui_kit::component::{
    input::{InputEvent, InputState},
    notification::Notification,
    searchable_list::{SearchableListItem, SearchableVec},
    select::{SelectEvent, SelectState},
    WindowExt,
};
use gpui_kit::{
    actions, AppContext, Context, Entity, FocusHandle, Focusable, KeyBinding, SharedString,
    Subscription, UniformListScrollHandle, Window,
};
use layout::{FlowSplit, SplitKind};
pub(super) use model::FlowId;
use model::FlowLibrary;
use request_inspector::RequestInspector;
use std::collections::HashMap;

actions!(
    flow_editor,
    [
        PreviousStep,
        NextStep,
        FirstStep,
        LastStep,
        PreviousFlow,
        NextFlow
    ]
);

#[derive(Clone)]
struct FlowOption {
    id: FlowId,
    name: SharedString,
}
impl SearchableListItem for FlowOption {
    type Value = FlowId;
    fn title(&self) -> SharedString {
        self.name.clone()
    }
    fn value(&self) -> &FlowId {
        &self.id
    }
}

pub(super) enum FlowsEvent {
    AddStep,
    RequestAdded,
    QuitReady,
    CookiesChanged(Vec<(String, String)>),
}
impl gpui_kit::EventEmitter<FlowsEvent> for FlowsView {}

pub(super) struct FlowsView {
    library: FlowLibrary,
    execution: execution::Execution,
    files: files::Files,
    input_subscriptions: HashMap<FlowId, Subscription>,
    console: Entity<console::FlowConsole>,
    environments: Entity<Environments>,
    focus: FocusHandle,
    canvas_focus: FocusHandle,
    library_focus: FocusHandle,
    title: Entity<InputState>,
    projected_title_flow: Option<FlowId>,
    inputs: HashMap<FlowId, Entity<inputs::InputsInspector>>,
    inputs_open: bool,
    yaml_open: bool,
    previews: HashMap<FlowId, Entity<yaml::YamlPreview>>,
    inspectors: HashMap<(FlowId, String), Entity<RequestInspector>>,
    step_subscriptions: HashMap<(FlowId, String), Subscription>,
    add_step_focus: FocusHandle,
    canvas_add_step_focus: FocusHandle,
    remove_step_focus: FocusHandle,
    filter: Entity<InputState>,
    picker: Entity<SelectState<SearchableVec<FlowOption>>>,
    create_name: Entity<InputState>,
    create_open: bool,
    create_attempted: bool,
    details: bool,
    library_scroll: UniformListScrollHandle,
    canvas_scroll: HashMap<FlowId, gpui_kit::ListState>,
    canvas_rem: gpui_kit::Pixels,
    splits: [FlowSplit; 3],
    _subscriptions: Vec<Subscription>,
}

impl FlowsView {
    pub(super) fn new(
        environments: Entity<Environments>,
        executor: crate::http::executor::RequestExecutor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.bind_keys([
            KeyBinding::new("up", PreviousStep, Some("FlowCanvas")),
            KeyBinding::new("down", NextStep, Some("FlowCanvas")),
            KeyBinding::new("home", FirstStep, Some("FlowCanvas")),
            KeyBinding::new("end", LastStep, Some("FlowCanvas")),
            KeyBinding::new("up", PreviousFlow, Some("FlowLibrary")),
            KeyBinding::new("down", NextFlow, Some("FlowLibrary")),
        ]);
        layout::bind_keys(cx);
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Flow name"));
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Find a flow…"));
        let create_name =
            cx.new(|cx| InputState::new(window, cx).placeholder("e.g. API smoke checks"));
        let picker = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(Vec::<FlowOption>::new()),
                None,
                window,
                cx,
            )
        });
        let console = cx.new(|cx| console::FlowConsole::new(window, cx));
        let mut subscriptions = Vec::new();
        subscriptions.push(
            cx.subscribe_in(&console, window, |this, _, event, window, cx| match event {
                console::ConsoleEvent::Run => this.run_flow(window, cx),
                console::ConsoleEvent::Stop => this.stop_active_flow(cx),
                console::ConsoleEvent::Check => this.check_flow(window, cx),
                console::ConsoleEvent::LocateDiagnostic(target) => {
                    this.locate_diagnostic(target, window, cx)
                }
                console::ConsoleEvent::LayoutChanged => {
                    this.conceal_inputs(cx);
                    cx.notify();
                }
                console::ConsoleEvent::LocateStep { flow, step, revision } => {
                    if this.library.active().is_some_and(|d| {
                        d.id() == *flow && revision.is_none_or(|revision| revision == d.revision()) && d.document().flow.steps.iter().any(|s| s.id == *step)
                    }) {
                        this.yaml_open = false;
                        this.select_step(step, window, cx);
                        this.details = true;
                        this.reveal_step();
                        this.focus_inspector(window, cx);
                        cx.notify();
                    } else {
                        window.push_notification(
                            Notification::info(
                                "This result belongs to an earlier Flow revision. Run the current Flow again.",
                            ),
                            cx,
                        );
                    }
                }
            }),
        );
        subscriptions.push(
            cx.subscribe_in(&title, window, |this, input, event, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.library.rename(input.read(cx).value().to_string());
                    this.sync_picker(window, cx);
                    this.document_changed(window, cx);
                    cx.notify();
                }
            }),
        );
        subscriptions.push(cx.subscribe_in(&filter, window, |this, _, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.library_scroll
                    .scroll_to_item(0, gpui_kit::ScrollStrategy::Top);
                cx.notify();
            }
        }));
        subscriptions.push(
            cx.subscribe_in(&picker, window, |this, _, event, window, cx| {
                if let SelectEvent::Confirm(Some(id)) = event {
                    this.select_flow(*id, window, cx);
                }
            }),
        );
        subscriptions.push(
            cx.subscribe_in(&create_name, window, |this, _, event, window, cx| {
                if !this.create_open {
                    return;
                }
                if matches!(event, InputEvent::PressEnter { shift: false, .. }) {
                    window.dispatch_action(
                        Box::new(gpui_kit::component::dialog::Confirm { secondary: false }),
                        cx,
                    );
                }
                cx.notify();
            }),
        );
        Self {
            library: FlowLibrary::default(),
            execution: execution::Execution::new(executor),
            files: files::Files::default(),
            input_subscriptions: HashMap::new(),
            console,
            environments,
            focus: cx.focus_handle(),
            canvas_focus: cx.focus_handle().tab_stop(true),
            library_focus: cx.focus_handle().tab_stop(true),
            title,
            projected_title_flow: None,
            inputs: HashMap::new(),
            inputs_open: false,
            yaml_open: false,
            previews: HashMap::new(),
            inspectors: HashMap::new(),
            step_subscriptions: HashMap::new(),
            add_step_focus: cx.focus_handle().tab_stop(true),
            canvas_add_step_focus: cx.focus_handle().tab_stop(true),
            remove_step_focus: cx.focus_handle().tab_stop(true),
            filter,
            picker,
            create_name,
            create_open: false,
            create_attempted: false,
            details: false,
            library_scroll: UniformListScrollHandle::new(),
            canvas_scroll: HashMap::new(),
            canvas_rem: window.rem_size(),
            splits: [
                FlowSplit::new(SplitKind::Library, cx),
                FlowSplit::new(SplitKind::Inspector, cx),
                FlowSplit::new(SplitKind::Results, cx),
            ],
            _subscriptions: subscriptions,
        }
    }
    pub(super) fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }
    pub(super) fn recent(&self) -> Vec<(FlowId, String, usize)> {
        self.library
            .drafts()
            .iter()
            .rev()
            .take(3)
            .map(|draft| {
                (
                    draft.id(),
                    draft.title().into(),
                    draft.document().flow.steps.len(),
                )
            })
            .collect()
    }
    pub(super) fn select_flow(&mut self, id: FlowId, window: &mut Window, cx: &mut Context<Self>) {
        if self.library.active().is_some_and(|d| d.id() == id) {
            return;
        }
        self.conceal_inputs(cx);
        if self.library.select(id) {
            self.sync_fields(window, cx);
            self.sync_picker(window, cx);
            cx.notify();
        }
    }
    fn sync_picker(&self, window: &mut Window, cx: &mut Context<Self>) {
        let items: Vec<_> = self
            .library
            .drafts()
            .iter()
            .map(|d| FlowOption {
                id: d.id(),
                name: d.title().to_owned().into(),
            })
            .collect();
        self.picker.update(cx, |state, cx| {
            state.set_items(SearchableVec::new(items), window, cx);
            if let Some(draft) = self.library.active() {
                state.set_selected_value(&draft.id(), window, cx);
            }
        });
    }
    /// Retain title editing state within a Flow; rebind it when the Flow identity changes.
    fn sync_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.document_changed(window, cx);
        let active = self.library.active().map(|d| d.id());
        let flow_changed = self.projected_title_flow != active;
        self.projected_title_flow = active;
        self.console
            .update(cx, |console, cx| console.activate(active, window, cx));
        let Some(draft) = self.library.active() else {
            return;
        };
        let flow = draft.id();
        if !self.inputs.contains_key(&flow) {
            let editor = cx.new(|cx| {
                inputs::InputsInspector::new(
                    &draft.document().flow.inputs,
                    self.environments.clone(),
                    window,
                    cx,
                )
            });
            self.input_subscriptions.insert(
                flow,
                cx.subscribe_in(&editor, window, move |this, _, event, window, cx| {
                    match event {
                        inputs::InputsEvent::Definitions { values, invalid } => {
                            this.library.edit_inputs(flow, values, *invalid);
                            this.document_changed(window, cx);
                            if let Some(draft) =
                                this.library.drafts().iter().find(|d| d.id() == flow)
                            {
                                for ((owner, step), inspector) in &this.inspectors {
                                    if *owner == flow {
                                        let variables =
                                            request_source::variables(&draft.document().flow, step);
                                        inspector
                                            .update(cx, |i, cx| i.set_variables(variables, cx));
                                    }
                                }
                            }
                        }
                        inputs::InputsEvent::Back => {
                            this.inputs_open = false;
                            this.focus_inspector(window, cx);
                        }
                    }
                    cx.notify();
                }),
            );
            self.inputs.insert(flow, editor);
        }
        self.canvas_scroll.entry(draft.id()).or_insert_with(|| {
            gpui_kit::ListState::new(
                draft.document().flow.steps.len() + 3,
                gpui_kit::ListAlignment::Top,
                window.rem_size() * 3.,
            )
        });
        self.title.update(cx, |input, cx| {
            let name = &draft.document().flow.name;
            if flow_changed || input.value().as_ref() != name {
                input.set_value(name.clone(), window, cx);
            }
        });
        if let Some(step) = draft.selected() {
            let flow = draft.id();
            let id = step.id.clone();
            let key = (flow, id.clone());
            if !self.inspectors.contains_key(&key) {
                let variables = request_source::variables(&draft.document().flow, &id);
                let inspector = cx.new(|cx| RequestInspector::new(step, variables, window, cx));
                let subscription =
                    cx.subscribe_in(&inspector, window, move |this, _, edit, window, cx| {
                        this.library.edit_step(flow, &id, edit);
                        this.document_changed(window, cx);
                        if matches!(edit, model::StepEdit::Exports(_)) {
                            if let Some(draft) =
                                this.library.drafts().iter().find(|d| d.id() == flow)
                            {
                                for ((owner, step), inspector) in &this.inspectors {
                                    if *owner == flow && step != &id {
                                        let variables =
                                            request_source::variables(&draft.document().flow, step);
                                        inspector.update(cx, |inspector, cx| {
                                            inspector.set_variables(variables, cx)
                                        });
                                    }
                                }
                            }
                        }
                        cx.notify();
                    });
                self.step_subscriptions.insert(key.clone(), subscription);
                self.inspectors.insert(key, inspector);
            }
        }
    }
    fn inspector(&self) -> Option<&Entity<RequestInspector>> {
        let draft = self.library.active()?;
        self.inspectors
            .get(&(draft.id(), draft.selected()?.id.clone()))
    }
    pub(super) fn conceal_inputs(&self, cx: &mut Context<Self>) {
        for editor in self.inputs.values() {
            editor.update(cx, |v, cx| v.conceal(cx));
        }
    }
    fn open_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.yaml_open = false;
        self.inputs_open = true;
        self.details = true;
        if let Some(editor) = self.library.active().and_then(|d| self.inputs.get(&d.id())) {
            editor.focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }
    fn focus_inspector(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(inspector) = self.inspector() {
            inspector.focus_handle(cx).focus(window, cx);
        }
    }

    fn select_step(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.conceal_inputs(cx);
        self.inputs_open = false;
        cx.notify();
        if self
            .library
            .active()
            .and_then(|d| d.selected())
            .is_some_and(|s| s.id == id)
        {
            return;
        }
        self.library.select_step(id);
        self.sync_fields(window, cx);
        cx.notify();
    }
    fn append_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.library.append_step();
        self.finish_append(window, cx);
    }
    fn finish_append(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.yaml_open = false;
        self.conceal_inputs(cx);
        self.inputs_open = false;
        if let Some(draft) = self.library.active() {
            if let Some(scroll) = self.canvas_scroll.get(&draft.id()) {
                let index = draft.document().flow.steps.len();
                scroll.splice(index..index, 1);
            }
        }
        self.sync_fields(window, cx);
        self.sync_picker(window, cx);
        self.details = true;
        self.reveal_step();
        self.focus_inspector(window, cx);
        cx.notify();
    }
    fn reveal_step(&self) {
        if let Some(draft) = self.library.active() {
            if let Some(index) = draft
                .document()
                .flow
                .steps
                .iter()
                .position(|s| Some(&s.id) == draft.selected().map(|s| &s.id))
            {
                if let Some(scroll) = self.canvas_scroll.get(&draft.id()) {
                    // Unmeasured virtual rows have no height yet. Jump by identity instead of
                    // accumulating their unknown heights; measured rows keep nearest-edge scrolling.
                    if scroll.bounds_for_item(index + 1).is_some() {
                        scroll.scroll_to_reveal_item(index + 1);
                    } else {
                        scroll.scroll_to(gpui_kit::ListOffset {
                            item_ix: index + 1,
                            offset_in_item: gpui_kit::px(0.),
                        });
                    }
                }
            }
        }
    }
    fn navigate_step(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.library.active() else {
            return;
        };
        let steps = &draft.document().flow.steps;
        if steps.is_empty() {
            return;
        }
        let index = steps
            .iter()
            .position(|s| Some(&s.id) == draft.selected().map(|s| &s.id))
            .unwrap_or(0);
        let next = index.saturating_add_signed(delta).min(steps.len() - 1);
        let id = steps[next].id.clone();
        self.select_step(&id, window, cx);
        self.reveal_step();
        self.canvas_focus.focus(window, cx);
    }
    fn filtered(&self, cx: &gpui_kit::App) -> Vec<FlowId> {
        let filter = self.filter.read(cx).value().to_lowercase();
        self.library
            .drafts()
            .iter()
            .filter(|d| d.title().to_lowercase().contains(&filter))
            .map(|d| d.id())
            .collect()
    }
    fn move_flow(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let filtered = self.filtered(cx);
        if filtered.is_empty() {
            return;
        }
        let index = filtered
            .iter()
            .position(|id| self.library.active().is_some_and(|d| d.id() == *id));
        let next = index.map_or(0, |i| {
            i.saturating_add_signed(delta).min(filtered.len() - 1)
        });
        self.select_flow(filtered[next], window, cx);
        self.library_scroll
            .scroll_to_item(next, gpui_kit::ScrollStrategy::Nearest);
        self.library_focus.focus(window, cx);
    }
}

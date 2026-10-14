//! Native, session-owned Flow editing. Kit owns input, selection controls and overlays.
mod console;
mod controls;
mod layout;
mod model;
#[cfg(test)]
mod tests;
mod view;

use crate::app::environments::Environments;
use crate::ui::components::kit_controls::MethodState;
use gpui::{
    actions, AppContext, Context, Entity, FocusHandle, Focusable, KeyBinding, SharedString,
    Subscription, UniformListScrollHandle, Window,
};
use gpui_kit::component::{
    input::{InputEvent, InputState},
    searchable_list::{SearchableListItem, SearchableVec},
    select::{SelectEvent, SelectState},
};
use layout::{FlowSplit, SplitKind};
pub(super) use model::FlowId;
use model::{FlowLibrary, StepEdit};
use std::collections::HashMap;

const METHODS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
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

pub(super) struct FlowsView {
    library: FlowLibrary,
    console: Entity<console::FlowConsole>,
    environments: Entity<Environments>,
    focus: FocusHandle,
    canvas_focus: FocusHandle,
    library_focus: FocusHandle,
    title: Entity<InputState>,
    step_name: Entity<InputState>,
    url: Entity<InputState>,
    method: Entity<MethodState>,
    filter: Entity<InputState>,
    picker: Entity<SelectState<SearchableVec<FlowOption>>>,
    create_name: Entity<InputState>,
    create_open: bool,
    create_attempted: bool,
    details: bool,
    library_scroll: UniformListScrollHandle,
    canvas_scroll: HashMap<FlowId, gpui::ListState>,
    inspector_scroll: gpui::ScrollHandle,
    canvas_rem: gpui::Pixels,
    splits: [FlowSplit; 3],
    _subscriptions: Vec<Subscription>,
}

impl FlowsView {
    pub(super) fn new(
        environments: Entity<Environments>,
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
        let step_name = cx.new(|cx| InputState::new(window, cx).placeholder("Step name"));
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("https://httpbingo.org/get"));
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Find a flow…"));
        let create_name =
            cx.new(|cx| InputState::new(window, cx).placeholder("e.g. API smoke checks"));
        let method = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(METHODS.to_vec()),
                Some(gpui_kit::component::IndexPath::new(0)),
                window,
                cx,
            )
        });
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
                console::ConsoleEvent::LayoutChanged => cx.notify(),
                console::ConsoleEvent::LocateStep { flow, step } => {
                    if this.library.active().is_some_and(|d| {
                        d.id() == *flow && d.document().flow.steps.iter().any(|s| s.id == *step)
                    }) {
                        this.select_step(step, window, cx);
                        this.details = true;
                        this.reveal_step();
                        this.step_name.focus_handle(cx).focus(window, cx);
                        cx.notify();
                    }
                }
            }),
        );
        subscriptions.push(
            cx.subscribe_in(&title, window, |this, input, event, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.library.rename(input.read(cx).value().to_string());
                    this.sync_picker(window, cx);
                    cx.notify();
                }
            }),
        );
        for (input, is_url) in [(&step_name, false), (&url, true)] {
            subscriptions.push(
                cx.subscribe_in(input, window, move |this, input, event, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        let value = input.read(cx).value().to_string();
                        this.library.edit_step(if is_url {
                            StepEdit::Url(value)
                        } else {
                            StepEdit::Name(value)
                        });
                        cx.notify();
                    }
                }),
            );
        }
        subscriptions.push(cx.subscribe_in(&method, window, |this, _, event, _, cx| {
            if let SelectEvent::Confirm(Some(method)) = event {
                this.library.edit_step(StepEdit::Method((*method).into()));
                cx.notify();
            }
        }));
        subscriptions.push(cx.subscribe_in(&filter, window, |this, _, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.library_scroll
                    .scroll_to_item(0, gpui::ScrollStrategy::Top);
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
            console,
            environments,
            focus: cx.focus_handle(),
            canvas_focus: cx.focus_handle().tab_stop(true),
            library_focus: cx.focus_handle().tab_stop(true),
            title,
            step_name,
            url,
            method,
            filter,
            picker,
            create_name,
            create_open: false,
            create_attempted: false,
            details: false,
            library_scroll: UniformListScrollHandle::new(),
            canvas_scroll: HashMap::new(),
            inspector_scroll: gpui::ScrollHandle::new(),
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
    /// Projection occurs only on context changes. Kit clears composition/undo even for equal text.
    fn sync_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active = self.library.active().map(|d| d.id());
        self.console
            .update(cx, |console, cx| console.activate(active, window, cx));
        let Some(draft) = self.library.active() else {
            return;
        };
        self.canvas_scroll.entry(draft.id()).or_insert_with(|| {
            gpui::ListState::new(
                draft.document().flow.steps.len() + 3,
                gpui::ListAlignment::Top,
                window.rem_size() * 3.,
            )
        });
        let step = draft.selected();
        let name = step.map_or("", |s| s.name.as_str());
        let request = step.and_then(|s| match &s.request {
            postman_flow::HttpRequestSource::Inline(r) => Some(r),
            _ => None,
        });
        let url = request
            .and_then(|r| model::literal_url(&r.url))
            .unwrap_or("");
        self.title.update(cx, |input, cx| {
            input.set_value(draft.document().flow.name.clone(), window, cx)
        });
        self.step_name
            .update(cx, |input, cx| input.set_value(name.to_owned(), window, cx));
        self.url
            .update(cx, |input, cx| input.set_value(url.to_owned(), window, cx));
        self.method.update(cx, |state, cx| {
            let index =
                request.and_then(|r| METHODS.iter().position(|m| *m == r.method.to_string()));
            state.set_selected_index(index.map(gpui_kit::component::IndexPath::new), window, cx);
        });
        self.inspector_scroll
            .set_offset(gpui::point(gpui::px(0.), gpui::px(0.)));
    }
    fn select_step(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
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
        if let Some(draft) = self.library.active() {
            if let Some(scroll) = self.canvas_scroll.get(&draft.id()) {
                let index = draft.document().flow.steps.len();
                scroll.splice(index..index, 1);
            }
        }
        self.sync_fields(window, cx);
        self.details = true;
        self.reveal_step();
        self.step_name.focus_handle(cx).focus(window, cx);
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
                        scroll.scroll_to(gpui::ListOffset {
                            item_ix: index + 1,
                            offset_in_item: gpui::px(0.),
                        });
                    }
                }
            }
        }
    }
    fn move_step(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
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
    fn filtered(&self, cx: &gpui::App) -> Vec<FlowId> {
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
            .scroll_to_item(next, gpui::ScrollStrategy::Nearest);
        self.library_focus.focus(window, cx);
    }
}

//! Retained drilldown and a native readonly editor; no runtime transformation in render.
use super::super::execution::results::{NamedValue, Results};
use super::*;
use gpui::{ScrollStrategy, UniformListScrollHandle};
use gpui_kit::component::input::EditorState;
mod view;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Row {
    Node(usize),
    Check(usize),
    Value(usize),
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct SelectedValue {
    node: Option<usize>,
    index: usize,
}
pub(super) struct Inspector {
    scope: Option<usize>,
    values: HashMap<bool, SelectedValue>,
    rows: Vec<Row>,
    selected: usize,
    selected_by_scope: HashMap<(bool, Option<usize>), usize>,
    scrolls: HashMap<(bool, Option<usize>), UniformListScrollHandle>,
    focus: FocusHandle,
    editors: [Entity<EditorState>; 2],
    showing_outputs: bool,
}
impl Inspector {
    pub fn new(window: &mut Window, cx: &mut Context<FlowConsole>) -> Self {
        let editors = std::array::from_fn(|_| {
            cx.new(|cx| {
                let mut editor = EditorState::new(window, cx)
                    .language("json")
                    .context_menu(false)
                    .searchable(false)
                    .folding(false)
                    .line_number(true)
                    .scroll_beyond_last_line(Some(0));
                editor.set_readonly(true, cx);
                editor.set_soft_wrap(true, window, cx);
                editor
            })
        });
        Self {
            scope: None,
            values: HashMap::new(),
            rows: Vec::new(),
            selected: 0,
            selected_by_scope: HashMap::new(),
            scrolls: HashMap::new(),
            focus: cx.focus_handle().tab_stop(true),
            editors,
            showing_outputs: false,
        }
    }
    fn editor(&self) -> &Entity<EditorState> {
        &self.editors[usize::from(self.showing_outputs)]
    }
    fn key(&self) -> (bool, Option<usize>) {
        (
            self.showing_outputs,
            if self.showing_outputs {
                None
            } else {
                self.scope
            },
        )
    }
    pub fn refresh(&mut self, results: &Results, outputs: bool) {
        if self.showing_outputs != outputs {
            self.selected_by_scope.insert(self.key(), self.selected);
            self.showing_outputs = outputs;
            self.selected = self
                .selected_by_scope
                .get(&self.key())
                .copied()
                .unwrap_or(0);
        }
        self.rows = if outputs {
            (0..if results.complete {
                results.outputs.len()
            } else {
                0
            })
                .map(Row::Value)
                .collect()
        } else if let Some(node) = self.scope.and_then(|i| results.nodes.get(i)) {
            node.children
                .iter()
                .copied()
                .map(Row::Node)
                .chain((0..node.checks.len()).map(Row::Check))
                .chain((0..node.values.len()).map(Row::Value))
                .collect()
        } else {
            results.roots.iter().copied().map(Row::Node).collect()
        };
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
        self.scrolls.entry(self.key()).or_default();
    }
    fn selected_value<'a>(&self, results: &'a Results) -> Option<&'a NamedValue> {
        let v = *self.values.get(&self.showing_outputs)?;
        if self.showing_outputs {
            if v.node.is_none() {
                results.outputs.get(v.index)
            } else {
                None
            }
        } else if v.node == self.scope {
            results.nodes.get(v.node?)?.values.get(v.index)
        } else {
            None
        }
    }
}
impl FlowConsole {
    pub(super) fn refresh_results(&mut self) {
        for session in self.sessions.values_mut() {
            if let (Some(inspector), Some(run)) = (&mut session.inspector, &session.run) {
                if session.view != View::Logs {
                    inspector.refresh(&run.results, session.view == View::Outputs);
                }
            }
        }
    }
    fn result_move(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(i) = self.current_mut().and_then(|s| s.inspector.as_mut()) {
            if i.values.contains_key(&i.showing_outputs) {
                return;
            }
            i.selected = i
                .selected
                .saturating_add_signed(delta)
                .min(i.rows.len().saturating_sub(1));
            i.scrolls
                .get(&i.key())
                .unwrap()
                .scroll_to_item(i.selected, ScrollStrategy::Top);
            i.focus.focus(window, cx);
            cx.notify();
        }
    }
    fn result_open(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.current_mut() else {
            return;
        };
        let (Some(i), Some(run)) = (&mut session.inspector, &session.run) else {
            return;
        };
        let Some(selected) = i.rows.get(row).copied() else {
            return;
        };
        i.selected = row;
        i.selected_by_scope.insert(i.key(), row);
        match selected {
            Row::Node(id) => {
                i.scope = Some(id);
                i.selected = i.selected_by_scope.get(&i.key()).copied().unwrap_or(0);
                i.values.remove(&i.showing_outputs);
                i.refresh(&run.results, false);
                i.focus.focus(window, cx);
            }
            Row::Value(index) => {
                i.values.insert(
                    i.showing_outputs,
                    SelectedValue {
                        node: if i.showing_outputs { None } else { i.scope },
                        index,
                    },
                );
                if let Some(value) = i.selected_value(&run.results) {
                    let source = value.value.json().unwrap_or_else(|| {
                        "Value unavailable: inspection size limit exceeded.".into()
                    });
                    i.editor()
                        .update(cx, |e, cx| e.set_value(source, window, cx));
                    i.editor().focus_handle(cx).focus(window, cx);
                }
            }
            Row::Check(_) => {
                i.focus.focus(window, cx);
            }
        }
        cx.notify();
    }
    fn result_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.current_mut() else {
            return;
        };
        let (Some(i), Some(run)) = (&mut session.inspector, &session.run) else {
            return;
        };
        if i.selected_value(&run.results).is_some() {
            i.values.remove(&i.showing_outputs);
        } else if !i.showing_outputs {
            i.selected_by_scope.insert(i.key(), i.selected);
            i.scope = i.scope.and_then(|id| run.results.nodes[id].parent);
            i.selected = i.selected_by_scope.get(&i.key()).copied().unwrap_or(0);
        }
        i.refresh(&run.results, i.showing_outputs);
        i.focus.focus(window, cx);
        cx.notify();
    }
}

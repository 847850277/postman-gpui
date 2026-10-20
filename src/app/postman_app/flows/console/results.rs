//! Retained drilldown and a native readonly editor; no runtime transformation in render.
use super::super::execution::results::{HttpPart, HttpTarget, NamedValue, Results};
use super::*;
use gpui_kit::component::input::EditorState;
use gpui_kit::{ScrollStrategy, UniformListScrollHandle};
mod view;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Row {
    Node(usize),
    Check(usize),
    Value(usize),
    Http(HttpPart),
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct SelectedValue {
    node: Option<usize>,
    index: usize,
    http: Option<HttpPart>,
}
pub(super) struct Inspector {
    scope: Option<usize>,
    values: HashMap<bool, SelectedValue>,
    rows: Vec<Row>,
    rows_key: Option<(bool, Option<usize>)>,
    selected: usize,
    selected_by_scope: HashMap<(bool, Option<usize>), usize>,
    scrolls: HashMap<(bool, Option<usize>), UniformListScrollHandle>,
    focus: FocusHandle,
    editors: [Entity<EditorState>; 2],
    showing_outputs: bool,
    // Inspector is replaced on each run; this choice is never persisted.
    show_sensitive: bool,
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
            rows_key: None,
            selected: 0,
            selected_by_scope: HashMap::new(),
            scrolls: HashMap::new(),
            focus: cx.focus_handle().tab_stop(true),
            editors,
            showing_outputs: false,
            show_sensitive: false,
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
        let selected = if self.rows_key == Some(self.key()) {
            self.rows.get(self.selected).copied()
        } else {
            None
        };
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
                .chain(
                    [HttpPart::Request, HttpPart::Response]
                        .into_iter()
                        .filter(|part| node.http[*part as usize].is_some())
                        .map(Row::Http),
                )
                .collect()
        } else {
            results.roots.iter().copied().map(Row::Node).collect()
        };
        self.selected = selected
            .and_then(|row| self.rows.iter().position(|r| *r == row))
            .unwrap_or(self.selected)
            .min(self.rows.len().saturating_sub(1));
        self.rows_key = Some(self.key());
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
            let node = results.nodes.get(v.node?)?;
            if let Some(part) = v.http {
                node.http[part as usize].as_ref()
            } else {
                node.values.get(v.index)
            }
        } else {
            None
        }
    }
    fn showing_original(&self) -> bool {
        self.show_sensitive
            && !self.showing_outputs
            && self.values.get(&false).is_some_and(|v| v.http.is_some())
    }
    fn sync_editor(&self, results: &Results, window: &mut Window, cx: &mut Context<FlowConsole>) {
        if let Some(value) = self.selected_value(results) {
            let source = if self.showing_original() {
                value.value.original_json().map(str::to_owned)
            } else {
                value.value.json()
            }
            .unwrap_or_else(|| "Value unavailable: inspection size limit exceeded.".into());
            self.editor()
                .update(cx, |e, cx| e.set_value(source, window, cx));
        }
    }
}
impl FlowConsole {
    fn set_sensitive_visible(
        &mut self,
        visible: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.current_mut() else {
            return;
        };
        let (Some(i), Some(run)) = (&mut session.inspector, &session.run) else {
            return;
        };
        if i.showing_outputs || !i.values.get(&false).is_some_and(|v| v.http.is_some()) {
            return;
        }
        if visible
            && i.selected_value(&run.results)
                .and_then(|v| v.value.original_json())
                .is_none()
        {
            return;
        }
        i.show_sensitive = visible;
        i.sync_editor(&run.results, window, cx);
        cx.notify();
    }
    pub(super) fn open_http(
        &mut self,
        target: HttpTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.current_mut() else {
            return;
        };
        let (Some(i), Some(run)) = (&mut session.inspector, &session.run) else {
            return;
        };
        if run.id != target.run
            || run
                .results
                .nodes
                .get(target.node)
                .and_then(|n| n.http[target.part as usize].as_ref())
                .is_none()
        {
            return;
        }
        session.view = View::Steps;
        i.scope = Some(target.node);
        i.values.remove(&false);
        i.refresh(&run.results, false);
        let row = i
            .rows
            .iter()
            .position(|row| *row == Row::Http(target.part))
            .unwrap();
        self.result_open(row, window, cx);
    }
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
            Row::Value(_) | Row::Http(_) => {
                let (index, http) = match selected {
                    Row::Value(index) => (index, None),
                    Row::Http(part) => (0, Some(part)),
                    _ => unreachable!(),
                };
                i.values.insert(
                    i.showing_outputs,
                    SelectedValue {
                        node: if i.showing_outputs { None } else { i.scope },
                        index,
                        http,
                    },
                );
                i.sync_editor(&run.results, window, cx);
                i.editor().focus_handle(cx).focus(window, cx);
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

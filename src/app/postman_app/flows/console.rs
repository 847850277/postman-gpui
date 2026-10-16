//! A retained, session-only Console. Documents and execution remain owned elsewhere.
mod model;
#[cfg(test)]
mod tests;
mod view;

use super::FlowId;
use gpui::{
    actions, AppContext, ClipboardItem, Context, Entity, EntityInputHandler, EventEmitter,
    FocusHandle, Focusable, KeyBinding, Keystroke, ListAlignment, ListState, Subscription, Window,
};
use gpui_kit::component::{
    input::{InputEvent, InputState},
    searchable_list::SearchableVec,
    select::{SelectEvent, SelectState},
    IndexPath,
};
use model::{Level, Session, View, COMMANDS};
use std::collections::HashMap;

pub(super) const HEADER_HEIGHT: f32 = 2.75;
pub(super) const TOOLS_HEIGHT: f32 = 2.5;
pub(super) const COMMAND_HEIGHT: f32 = 4.;
const HINT: &str = "Enter to execute · ↑ ↓ history · Tab complete";
const FILTERS: [&str; 6] = [
    "All levels",
    "Commands",
    "Info",
    "Success",
    "Warnings",
    "Errors",
];
const LEVELS: [Option<Level>; 6] = [
    None,
    Some(Level::Command),
    Some(Level::Info),
    Some(Level::Success),
    Some(Level::Warning),
    Some(Level::Error),
];

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) enum ConsoleMode {
    #[default]
    Normal,
    Collapsed,
    Maximized,
}
pub(super) enum ConsoleEvent {
    LayoutChanged,
    Check,
    LocateDiagnostic(super::validation::DiagnosticTarget),
    LocateStep { flow: FlowId, step: String },
}
actions!(
    flow_console,
    [PreviousTab, NextTab, FirstTab, LastTab, ActivateTab]
);

pub(super) struct FlowConsole {
    active: Option<FlowId>,
    sessions: HashMap<FlowId, Session>,
    input: Entity<InputState>,
    filter: Entity<SelectState<SearchableVec<&'static str>>>,
    scroll: ListState,
    visible: Vec<u64>,
    tab_focus: [FocusHandle; 3],
    mode: ConsoleMode,
    hint: String,
    rem: gpui::Pixels,
    _subscriptions: Vec<Subscription>,
}
impl EventEmitter<ConsoleEvent> for FlowConsole {}
impl FlowConsole {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.bind_keys([
            KeyBinding::new("left", PreviousTab, Some("FlowConsoleTabs")),
            KeyBinding::new("right", NextTab, Some("FlowConsoleTabs")),
            KeyBinding::new("home", FirstTab, Some("FlowConsoleTabs")),
            KeyBinding::new("end", LastTab, Some("FlowConsoleTabs")),
            KeyBinding::new("enter", ActivateTab, Some("FlowConsoleTabs")),
            KeyBinding::new("space", ActivateTab, Some("FlowConsoleTabs")),
        ]);
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Type help for commands"));
        let filter = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(FILTERS.to_vec()),
                Some(IndexPath::new(0)),
                window,
                cx,
            )
        });
        let scroll = ListState::new(0, ListAlignment::Top, window.rem_size() * 3.);
        let owner = cx.weak_entity();
        scroll.set_scroll_handler(move |event, window, cx| {
            if event.is_scrolled && !event.is_following_tail {
                let active = owner.upgrade().and_then(|owner| owner.read(cx).active);
                let owner = owner.clone();
                // The list owns its geometry while dispatching; update after it releases its borrow.
                window.defer(cx, move |_, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        if this.active != active {
                            return;
                        }
                        if let Some(session) = this.current_mut() {
                            if session.follow {
                                session.follow = false;
                                this.scroll.set_follow_mode(gpui::FollowMode::Normal);
                                cx.notify();
                            }
                        }
                    });
                });
            }
        });
        let mut subscriptions = vec![
            cx.subscribe_in(&input, window, |this, input, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = input.read(cx).value().to_string();
                    if let Some(session) = this.current_mut() {
                        session.edit(value);
                    }
                    this.hint = HINT.into();
                    cx.notify();
                }
            }),
            cx.subscribe_in(&filter, window, |this, _, event, _, cx| {
                if let SelectEvent::Confirm(Some(value)) = event {
                    if let Some(session) = this.current_mut() {
                        session.filter =
                            LEVELS[FILTERS.iter().position(|v| v == value).unwrap_or(0)];
                    }
                    this.refresh_logs(true);
                    cx.notify();
                }
            }),
        ];
        let owner = cx.weak_entity();
        subscriptions.push(cx.intercept_keystrokes(move |event, window, cx| {
            let _ = owner.update(cx, |this, cx| {
                if this.mode != ConsoleMode::Collapsed
                    && this.input.focus_handle(cx).is_focused(window)
                    && this.handle_key(&event.keystroke, window, cx)
                {
                    cx.stop_propagation();
                }
            });
        }));
        Self {
            active: None,
            sessions: HashMap::new(),
            input,
            filter,
            scroll,
            visible: Vec::new(),
            tab_focus: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            mode: ConsoleMode::Normal,
            hint: HINT.into(),
            rem: window.rem_size(),
            _subscriptions: subscriptions,
        }
    }
    pub(super) fn mode(&self) -> ConsoleMode {
        self.mode
    }
    fn current(&self) -> Option<&Session> {
        self.active.and_then(|id| self.sessions.get(&id))
    }
    fn current_mut(&mut self) -> Option<&mut Session> {
        self.active.and_then(|id| self.sessions.get_mut(&id))
    }
    pub(super) fn activate(
        &mut self,
        id: Option<FlowId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active == id {
            return;
        }
        self.active = id;
        if let Some(id) = id {
            self.sessions.entry(id).or_default();
        }
        let value = self.current().map_or(String::new(), |s| s.draft.clone());
        self.input
            .update(cx, |input, cx| input.set_value(value, window, cx));
        self.sync_filter(window, cx);
        self.hint = HINT.into();
        self.refresh_logs(true);
        cx.notify();
    }
    fn sync_filter(&self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.current().and_then(|s| s.filter);
        let ix = LEVELS.iter().position(|level| *level == value).unwrap_or(0);
        self.filter.update(cx, |filter, cx| {
            filter.set_selected_index(Some(IndexPath::new(ix)), window, cx)
        });
    }
    fn composing(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.input.update(cx, |input, cx| {
            input.marked_text_range(window, cx).is_some()
        })
    }
    fn handle_key(&mut self, key: &Keystroke, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let modified = key.modifiers.control || key.modifiers.platform || key.modifiers.alt;
        let name = key.key.as_str();
        let owned = matches!(name, "enter" | "up" | "down" | "tab" | "escape");
        if self.composing(window, cx) {
            return owned;
        }
        if self.active.is_none() {
            return owned;
        }
        match name {
            "enter" if !key.modifiers.alt => {
                if !key.modifiers.shift {
                    self.submit(None, window, cx);
                }
                true
            }
            "up" | "down" if !modified && !key.modifiers.shift => {
                let value = self.current_mut().unwrap().recall(name == "up").to_owned();
                self.input
                    .update(cx, |input, cx| input.set_value(value, window, cx));
                cx.notify();
                true
            }
            "escape" if !modified => {
                self.current_mut().unwrap().edit(String::new());
                self.input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.hint = HINT.into();
                cx.notify();
                true
            }
            "tab" if !modified && !key.modifiers.shift => {
                let prefix = self.input.read(cx).value().trim().to_ascii_lowercase();
                let matches: Vec<_> = COMMANDS
                    .iter()
                    .filter(|(name, _)| name.starts_with(&prefix))
                    .map(|(name, _)| *name)
                    .collect();
                if matches.len() == 1 && matches[0] != prefix {
                    let value = matches[0].to_owned();
                    self.current_mut().unwrap().edit(value.clone());
                    self.input
                        .update(cx, |input, cx| input.set_value(value, window, cx));
                    self.hint = "Completed command. Enter to execute; Tab to leave.".into();
                    cx.notify();
                    true
                } else {
                    if matches.len() > 1 {
                        self.hint =
                            format!("Matches: {}. Type more to complete.", matches.join(", "));
                        cx.notify();
                    }
                    false
                }
            }
            // Saving belongs to the editor, not this command entry surface.
            "s" if key.modifiers.platform || key.modifiers.control => true,
            _ => false,
        }
    }
    fn submit(&mut self, source: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        if self.active.is_none() || self.composing(window, cx) {
            return;
        }
        let source = source
            .map(str::to_owned)
            .unwrap_or_else(|| self.input.read(cx).value().to_string());
        if source.trim().is_empty() {
            return;
        }
        self.current_mut().unwrap().execute(&source);
        if source.trim().eq_ignore_ascii_case("check") {
            cx.emit(ConsoleEvent::Check);
        }
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.sync_filter(window, cx);
        self.hint = if source.trim().eq_ignore_ascii_case("clear") {
            "Logs cleared. Results and command history kept.".into()
        } else {
            HINT.into()
        };
        if self.mode == ConsoleMode::Collapsed {
            self.set_mode(ConsoleMode::Normal, cx);
        }
        self.refresh_logs(false);
        self.input.focus_handle(cx).focus(window, cx);
        cx.notify();
    }
    fn refresh_logs(&mut self, reset: bool) {
        let next: Vec<_> = self
            .current()
            .map_or(Vec::new(), |s| s.visible().map(|entry| entry.id).collect());
        // Preserve the viewport when only a bounded prefix is evicted or new rows are appended.
        let removed = next
            .first()
            .and_then(|first| self.visible.iter().position(|id| id == first));
        if !reset && removed.is_some_and(|removed| next.starts_with(&self.visible[removed..])) {
            let removed = removed.unwrap();
            self.scroll.splice(0..removed, 0);
            let kept = self.visible.len() - removed;
            self.scroll.splice(kept..kept, next.len() - kept);
        } else {
            self.scroll.reset(next.len());
        }
        self.visible = next;
        let follow = self.current().is_some_and(|s| s.follow);
        self.scroll.set_follow_mode(if follow {
            gpui::FollowMode::Tail
        } else {
            gpui::FollowMode::Normal
        });
    }
    pub(super) fn set_mode(&mut self, mode: ConsoleMode, cx: &mut Context<Self>) {
        if self.mode != mode {
            self.mode = mode;
            cx.emit(ConsoleEvent::LayoutChanged);
            cx.notify();
        }
    }
    fn select_view(&mut self, view: View, cx: &mut Context<Self>) {
        if let Some(session) = self.current_mut() {
            session.view = view;
        }
        if self.mode == ConsoleMode::Collapsed {
            self.set_mode(ConsoleMode::Normal, cx);
        }
        cx.notify();
    }
    fn navigate_tab(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.select_view(View::ALL[ix], cx);
        self.tab_focus[ix].focus(window, cx);
    }
    fn move_tab(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.current().map_or(View::Logs, |s| s.view) as usize;
        self.navigate_tab(
            (current as isize + delta).rem_euclid(3) as usize,
            window,
            cx,
        );
    }
    fn clear(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.current_mut() {
            session.clear();
        }
        self.hint = "Logs cleared. Results and command history kept.".into();
        self.refresh_logs(true);
        cx.notify();
    }
    fn copy(&mut self, cx: &mut Context<Self>) {
        let text = self.current().map_or(String::new(), Session::copy_text);
        if !text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            self.hint = "Visible logs copied.".into();
            cx.notify();
        }
    }
    fn toggle_follow(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.current_mut() {
            session.follow = !session.follow;
        }
        self.scroll
            .set_follow_mode(if self.current().is_some_and(|s| s.follow) {
                gpui::FollowMode::Tail
            } else {
                gpui::FollowMode::Normal
            });
        cx.notify();
    }
}

impl FlowConsole {
    pub(super) fn document_changed(&mut self, flow: FlowId, revision: u64, cx: &mut Context<Self>) {
        let session = self.sessions.entry(flow).or_default();
        if session.revision != revision {
            session.revision = revision;
            cx.notify();
        }
    }
    pub(super) fn check_label(&self) -> &'static str {
        match self.current().and_then(|s| {
            s.check
                .map(|(revision, passed)| (revision == s.revision, passed))
        }) {
            None => "Not checked",
            Some((false, _)) => "Check stale",
            Some((true, true)) => "Check passed",
            Some((true, false)) => "Check failed",
        }
    }
    pub(super) fn report(
        &mut self,
        flow: FlowId,
        revision: u64,
        issues: Vec<super::validation::Issue>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let session = self.sessions.entry(flow).or_default();
        session.revision = revision;
        session.check = Some((revision, issues.is_empty()));
        session.view = View::Logs;
        session.filter = None;
        session.follow = false;
        // Assemble atomically so the history cap cannot truncate a single large report.
        let mut previous = std::mem::take(&mut session.logs);
        let (level, summary) = if issues.is_empty() {
            (Level::Success, format!("Check passed · revision {revision}. Document validation only; no requests sent."))
        } else {
            (
                Level::Error,
                format!(
                    "Check failed · {} diagnostic(s) · revision {revision}. No requests sent.",
                    issues.len()
                ),
            )
        };
        session.push(level, summary, None);
        let mut entries = std::collections::VecDeque::new();
        entries.append(&mut session.logs);
        for issue in issues {
            session.push(Level::Error, issue.message, None);
            session.logs.back_mut().unwrap().diagnostic =
                Some(super::validation::DiagnosticTarget {
                    flow,
                    revision,
                    target: issue.target,
                });
            entries.append(&mut session.logs);
        }
        while !previous.is_empty() && previous.len() + entries.len() > 500 {
            previous.pop_front();
        }
        let report_start = previous.len();
        previous.append(&mut entries);
        session.logs = previous;
        if self.mode == ConsoleMode::Collapsed {
            self.set_mode(ConsoleMode::Normal, cx);
        }
        self.sync_filter(window, cx);
        self.refresh_logs(true);
        self.scroll.scroll_to(gpui::ListOffset {
            item_ix: report_start,
            offset_in_item: gpui::px(0.),
        });
        cx.notify();
    }
}

#[cfg(test)]
impl FlowConsole {
    pub(super) fn diagnostics(&self) -> Vec<(u64, super::validation::DiagnosticTarget)> {
        self.current()
            .unwrap()
            .logs
            .iter()
            .filter_map(|e| {
                e.diagnostic
                    .clone()
                    .filter(|d| {
                        self.current()
                            .unwrap()
                            .check
                            .is_some_and(|(revision, _)| d.revision == revision)
                    })
                    .map(|d| (e.id, d))
            })
            .collect()
    }
}

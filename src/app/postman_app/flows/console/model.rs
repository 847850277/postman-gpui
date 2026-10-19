//! Transient Console state; none of this belongs in a Flow document.
use std::collections::VecDeque;

pub(super) const COMMANDS: [(&str, &str); 8] = [
    ("help", "Show available Flow commands"),
    ("check", "Validate the current Flow document"),
    ("run", "Run the current Flow snapshot"),
    ("stop", "Cancel the active Flow execution"),
    ("clear", "Clear logs; keep command history and results"),
    ("logs", "Show event logs"),
    ("steps", "Inspect steps, checks, extractions and iterations"),
    (
        "outputs",
        "Inspect and copy sanitized final outputs as JSON",
    ),
];

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) enum View {
    #[default]
    Logs,
    Steps,
    Outputs,
}
impl View {
    pub(super) const ALL: [Self; 3] = [Self::Logs, Self::Steps, Self::Outputs];
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Logs => "Logs",
            Self::Steps => "Steps",
            Self::Outputs => "Outputs",
        }
    }
    pub(super) fn id(self) -> &'static str {
        match self {
            Self::Logs => "console-tab-logs",
            Self::Steps => "console-tab-steps",
            Self::Outputs => "console-tab-outputs",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Level {
    Command,
    Info,
    Success,
    Warning,
    Error,
}
impl Level {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Command => "COMMAND",
            Self::Info => "INFO",
            Self::Success => "SUCCESS",
            Self::Warning => "WARNING",
            Self::Error => "ERROR",
        }
    }
}

pub(super) struct LogEntry {
    pub(super) id: u64,
    pub(super) time: String,
    pub(super) level: Level,
    pub(super) message: String,
    pub(super) step: Option<String>,
    pub(super) run_revision: Option<u64>,
    pub(super) diagnostic: Option<super::super::validation::DiagnosticTarget>,
}

pub(super) struct Session {
    pub(super) logs: VecDeque<LogEntry>,
    pub(super) draft: String,
    pub(super) history: VecDeque<String>,
    pub(super) view: View,
    pub(super) filter: Option<Level>,
    pub(super) follow: bool,
    pub(super) cleared: bool,
    pub(super) check: Option<(u64, bool)>,
    pub(super) revision: u64,
    pub(super) run: Option<super::super::execution::Run>,
    pub(super) inspector: Option<super::results::Inspector>,
    cursor: usize,
    stash: String,
    next_id: u64,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            logs: VecDeque::new(),
            draft: String::new(),
            history: VecDeque::new(),
            view: View::Logs,
            filter: None,
            follow: true,
            cleared: false,
            check: None,
            revision: 0,
            run: None,
            inspector: None,
            cursor: 0,
            stash: String::new(),
            next_id: 0,
        }
    }
}
impl Session {
    pub(super) fn push(&mut self, level: Level, message: impl Into<String>, step: Option<String>) {
        self.next_id += 1;
        self.logs.push_back(LogEntry {
            id: self.next_id,
            time: chrono::Local::now().format("%H:%M:%S%.3f").to_string(),
            level,
            message: message.into(),
            step,
            diagnostic: None,
            run_revision: None,
        });
        if self.logs.len() > 500 {
            self.logs.pop_front();
        }
        self.cleared = false;
    }
    pub(super) fn visible(&self) -> impl Iterator<Item = &LogEntry> {
        self.logs
            .iter()
            .filter(|entry| self.filter.is_none_or(|level| entry.level == level))
    }
    pub(super) fn copy_text(&self) -> String {
        if self.view == View::Outputs {
            return self
                .run
                .as_ref()
                .and_then(|r| r.results.outputs_json())
                .unwrap_or_default();
        }
        if self.view != View::Logs {
            return String::new();
        }
        self.visible()
            .map(|entry| {
                format!(
                    "{} {} {}{}",
                    entry.time,
                    entry.level.label(),
                    entry
                        .step
                        .as_ref()
                        .map_or(String::new(), |id| format!("[{id}] ")),
                    entry.message
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    pub(super) fn edit(&mut self, value: String) {
        self.draft = value;
        self.cursor = self.history.len();
        self.stash.clone_from(&self.draft);
    }
    pub(super) fn recall(&mut self, previous: bool) -> &str {
        if previous {
            if self.cursor == self.history.len() {
                self.stash.clone_from(&self.draft);
            }
            self.cursor = self.cursor.saturating_sub(1);
        } else {
            self.cursor = (self.cursor + 1).min(self.history.len());
        }
        self.draft = self.history.get(self.cursor).unwrap_or(&self.stash).clone();
        &self.draft
    }
    pub(super) fn clear(&mut self) {
        self.logs.clear();
        self.cleared = true;
    }
    pub(super) fn execute(&mut self, source: &str) {
        let source = source.trim();
        if source.is_empty() {
            return;
        }
        if self.history.back().is_none_or(|last| last != source) {
            self.history.push_back(source.into());
            if self.history.len() > 50 {
                self.history.pop_front();
            }
        }
        self.edit(String::new());
        self.view = View::Logs;
        self.filter = None;
        self.follow = true;
        let command = source.to_ascii_lowercase();
        if command == "clear" {
            self.clear();
            return;
        }
        self.push(Level::Command, format!("› {source}"), None);
        match command.as_str() {
            "help" => self.push(
                Level::Info,
                COMMANDS
                    .iter()
                    .map(|(name, description)| format!("{name:9} {description}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                None,
            ),
            "check" => {} // The Flow owner compiles the current document and editor drafts.
            "run" | "stop" => {} // The Flow owner owns execution and cancellation.
            "logs" => self.view = View::Logs,
            "steps" => self.view = View::Steps,
            "outputs" => self.view = View::Outputs,
            _ => self.push(
                Level::Error,
                "Unknown Flow command or unsupported arguments. Type help for available commands.",
                None,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn command_history_restores_drafts_and_clear_preserves_history() {
        let mut state = Session::default();
        state.execute(" HELP ");
        state.execute("HELP");
        state.execute("steps");
        state.edit("unfinished".into());
        assert_eq!(state.recall(true), "steps");
        assert_eq!(state.recall(true), "HELP");
        assert_eq!(state.recall(false), "steps");
        assert_eq!(state.recall(false), "unfinished");
        state.execute("clear");
        assert!(state.logs.is_empty());
        assert!(state.cleared);
        assert_eq!(state.history.len(), 3);
        for i in 0..100 {
            state.execute(&format!("unknown-{i}"));
        }
        assert_eq!(state.history.len(), 50);
        assert_eq!(state.recall(true), "unknown-99");
    }
    #[test]
    fn bounded_logs_filter_copy_and_commands_have_no_fake_success() {
        let mut state = Session::default();
        for i in 0..510 {
            state.push(Level::Info, format!("event-{i}"), None);
        }
        assert_eq!(state.logs.len(), 500);
        assert_eq!(state.logs.front().unwrap().id, 11);
        state.push(Level::Error, "Bad field", Some("request-1".into()));
        state.filter = Some(Level::Error);
        assert_eq!(state.visible().count(), 1);
        assert!(state.copy_text().contains("[request-1] Bad field"));
        assert!(!state.copy_text().contains("event-"));
        for command in ["run", "stop"] {
            state.execute(command);
            assert_eq!(state.logs.back().unwrap().level, Level::Command);
        }
        for command in ["run extra", "echo hello", "run; clear"] {
            state.execute(command);
            assert_eq!(state.logs.back().unwrap().level, Level::Error);
        }
        state.execute("outputs");
        assert_eq!(state.view, View::Outputs);
        assert!(state.copy_text().is_empty());
    }
}

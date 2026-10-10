//! Retained text and form editors selected by their owning pane.
//!
//! `BodyInput` projects the pane-selected editor mode and forwards child edit events. Text editing
//! mechanics live in `TextBodyInput`; typed form-row mechanics live in `FormBodyInput`. Request
//! semantics and transport serialization remain authoritative in the workspace ViewModel.

use form_body_input::{FormBodyInput, FormBodyInputEvent};
use gpui::{
    actions, div, App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyBinding, ParentElement, Render, Styled, Subscription,
    Window,
};
use std::path::PathBuf;
use text_body_input::{TextBodyInput, TextBodyInputEvent};

use crate::ui::theme::{CODE_BG, PANEL};

mod form_body_input;
mod text_body_input;

actions!(
    body_input,
    [
        Backspace,
        Delete,
        Enter,
        Escape,
        Tab,
        ShiftTab,
        Left,
        Right,
        WordLeft,
        WordRight,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectWordLeft,
        SelectWordRight,
        SelectUp,
        SelectDown,
        SelectAll,
        Home,
        End,
        Paste,
        Cut,
        Copy,
        Undo,
        Redo,
    ]
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyType {
    Json,
    FormData,
    Raw,
}

#[derive(Debug, Clone)]
pub enum BodyInputEvent {
    ValueChanged(String),
    FormDataChanged(Vec<FormDataEntry>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormDataFile {
    pub path: PathBuf,
    pub file_name: Option<String>,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormDataEntry {
    pub key: String,
    pub value: String,
    pub file: Option<FormDataFile>,
    pub enabled: bool,
}

impl FormDataEntry {
    pub fn text(key: impl Into<String>, value: impl Into<String>, enabled: bool) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            file: None,
            enabled,
        }
    }

    pub fn file(
        key: impl Into<String>,
        path: impl Into<PathBuf>,
        file_name: Option<String>,
        content_type: Option<String>,
        enabled: bool,
    ) -> Self {
        Self {
            key: key.into(),
            value: String::new(),
            file: Some(FormDataFile {
                path: path.into(),
                file_name,
                content_type,
            }),
            enabled,
        }
    }
}

/// Presents the text or form editor selected by the owning pane.
pub struct BodyInput {
    current_type: BodyType,
    text_input: Entity<TextBodyInput>,
    form_input: Entity<FormBodyInput>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<BodyInputEvent> for BodyInput {}

impl Focusable for BodyInput {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self.current_type {
            BodyType::Json | BodyType::Raw => self.text_input.read(cx).focus_handle(cx),
            BodyType::FormData => self.form_input.read(cx).focus_handle(cx),
        }
    }
}

impl BodyInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let text_input = cx.new(TextBodyInput::new);
        let form_input = cx.new(FormBodyInput::new);
        let subscriptions = vec![
            cx.subscribe(&text_input, Self::on_text_event),
            cx.subscribe(&form_input, Self::on_form_event),
        ];

        Self {
            current_type: BodyType::Json,
            text_input,
            form_input,
            _subscriptions: subscriptions,
        }
    }

    fn on_text_event(
        &mut self,
        _input: Entity<TextBodyInput>,
        event: &TextBodyInputEvent,
        cx: &mut Context<Self>,
    ) {
        let TextBodyInputEvent::ValueChanged(value) = event;
        cx.emit(BodyInputEvent::ValueChanged(value.clone()));
        cx.notify();
    }

    fn on_form_event(
        &mut self,
        _input: Entity<FormBodyInput>,
        event: &FormBodyInputEvent,
        cx: &mut Context<Self>,
    ) {
        let FormBodyInputEvent::Changed(entries) = event;
        cx.emit(BodyInputEvent::FormDataChanged(entries.clone()));
        cx.notify();
    }

    /// Change editor presentation without emitting a draft-value event.
    pub fn set_type_silent(&mut self, body_type: BodyType, cx: &mut Context<Self>) {
        if self.current_type != body_type {
            self.current_type = body_type;
            cx.notify();
        }
    }

    pub fn set_form_data_allows_files(&mut self, allows_files: bool, cx: &mut Context<Self>) {
        self.form_input.update(cx, |input, cx| {
            input.set_form_data_allows_files(allows_files, cx)
        });
    }

    /// Content-fit height of the form table and its separate Add field button.
    /// Pass the actual editor width and clamp the result to the available pane height;
    /// rows scroll within a smaller allocation while Add field remains accessible.
    /// Excludes surrounding pane chrome.
    pub fn preferred_form_height(&self, width: gpui::Pixels, cx: &App) -> gpui::Pixels {
        self.form_input.read(cx).preferred_height(width, cx)
    }

    #[cfg(test)]
    fn set_content(&mut self, content: impl Into<String>, cx: &mut Context<Self>) {
        if self.current_type != BodyType::FormData {
            let content = content.into();
            self.text_input
                .update(cx, |input, cx| input.set_content(content, cx));
        }
    }

    /// Projects a ViewModel value into the active editor buffer without emitting an edit event.
    pub fn project_content(&mut self, content: impl Into<String>, cx: &mut Context<Self>) {
        if self.current_type != BodyType::FormData {
            let content = content.into();
            self.text_input
                .update(cx, |input, cx| input.project_content(content, cx));
        }
    }

    #[cfg(test)]
    fn add_form_data_entry(&mut self, cx: &mut Context<Self>) {
        self.form_input
            .update(cx, FormBodyInput::add_form_data_entry);
    }

    #[cfg(test)]
    fn remove_form_data_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        self.form_input
            .update(cx, |input, cx| input.remove_form_data_entry(index, cx));
    }

    #[cfg(test)]
    fn toggle_form_data_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        self.form_input
            .update(cx, |input, cx| input.toggle_form_data_entry(index, cx));
    }

    /// Returns all editor rows, including disabled and blank rows, from the form editor.
    pub fn form_data_entry_count(&self, cx: &App) -> usize {
        self.form_input.read(cx).entries().len()
    }

    #[cfg(test)]
    fn set_form_data_entries(&mut self, entries: Vec<FormDataEntry>, cx: &mut Context<Self>) {
        self.form_input
            .update(cx, |input, cx| input.set_form_data_entries(entries, cx));
    }

    /// Projects parsed form data without turning the projection into a user edit event.
    pub fn project_form_data_entries(
        &mut self,
        entries: Vec<FormDataEntry>,
        cx: &mut Context<Self>,
    ) {
        self.form_input
            .update(cx, |input, cx| input.project_form_data_entries(entries, cx));
    }

    /// Projects a different request tab and starts fresh per-cell selection/composition/history.
    pub(crate) fn project_form_data_entries_with_rebind(
        &mut self,
        entries: Vec<FormDataEntry>,
        cx: &mut Context<Self>,
    ) {
        self.form_input.update(cx, |input, cx| {
            input.project_form_data_entries_with_rebind(entries, true, cx)
        });
    }

    #[cfg(test)]
    fn clear(&mut self, cx: &mut Context<Self>) {
        match self.current_type {
            BodyType::Json | BodyType::Raw => {
                self.text_input.update(cx, TextBodyInput::clear);
            }
            BodyType::FormData => {
                self.form_input.update(cx, FormBodyInput::clear);
            }
        }
    }
}

impl Render for BodyInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current_type = self.current_type;
        div()
            .key_context("BodyInput")
            .flex()
            .flex_col()
            .gap_0()
            .w_full()
            .h_full()
            .min_h_0()
            .bg((if current_type == BodyType::FormData {
                PANEL
            } else {
                CODE_BG
            })
            .resolve(cx))
            .child(match current_type {
                BodyType::Json | BodyType::Raw => self.text_input.clone().into_any_element(),
                BodyType::FormData => self.form_input.clone().into_any_element(),
            })
    }
}

pub fn setup_body_input_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("backspace", Backspace, Some("BodyInput")),
        KeyBinding::new("delete", Delete, Some("BodyInput")),
        KeyBinding::new("enter", Enter, Some("BodyInput")),
        KeyBinding::new("escape", Escape, Some("BodyInput")),
        KeyBinding::new("tab", Tab, Some("BodyInput")),
        KeyBinding::new("shift-tab", ShiftTab, Some("BodyInput")),
        KeyBinding::new("left", Left, Some("BodyInput")),
        KeyBinding::new("right", Right, Some("BodyInput")),
        KeyBinding::new("alt-left", WordLeft, Some("BodyInput")),
        KeyBinding::new("ctrl-left", WordLeft, Some("BodyInput")),
        KeyBinding::new("alt-right", WordRight, Some("BodyInput")),
        KeyBinding::new("ctrl-right", WordRight, Some("BodyInput")),
        KeyBinding::new("up", Up, Some("BodyInput")),
        KeyBinding::new("down", Down, Some("BodyInput")),
        KeyBinding::new("shift-left", SelectLeft, Some("BodyInput")),
        KeyBinding::new("shift-right", SelectRight, Some("BodyInput")),
        KeyBinding::new("alt-shift-left", SelectWordLeft, Some("BodyInput")),
        KeyBinding::new("ctrl-shift-left", SelectWordLeft, Some("BodyInput")),
        KeyBinding::new("alt-shift-right", SelectWordRight, Some("BodyInput")),
        KeyBinding::new("ctrl-shift-right", SelectWordRight, Some("BodyInput")),
        KeyBinding::new("shift-up", SelectUp, Some("BodyInput")),
        KeyBinding::new("shift-down", SelectDown, Some("BodyInput")),
        KeyBinding::new("cmd-a", SelectAll, Some("BodyInput")),
        KeyBinding::new("ctrl-a", SelectAll, Some("BodyInput")),
        KeyBinding::new("cmd-v", Paste, Some("BodyInput")),
        KeyBinding::new("ctrl-v", Paste, Some("BodyInput")),
        KeyBinding::new("cmd-c", Copy, Some("BodyInput")),
        KeyBinding::new("ctrl-c", Copy, Some("BodyInput")),
        KeyBinding::new("cmd-x", Cut, Some("BodyInput")),
        KeyBinding::new("ctrl-x", Cut, Some("BodyInput")),
        KeyBinding::new("cmd-z", Undo, Some("BodyInput")),
        KeyBinding::new("ctrl-z", Undo, Some("BodyInput")),
        KeyBinding::new("cmd-shift-z", Redo, Some("BodyInput")),
        KeyBinding::new("ctrl-shift-z", Redo, Some("BodyInput")),
        KeyBinding::new("ctrl-y", Redo, Some("BodyInput")),
        KeyBinding::new("home", Home, Some("BodyInput")),
        KeyBinding::new("end", End, Some("BodyInput")),
        KeyBinding::new("cmd-left", Home, Some("BodyInput")),
        KeyBinding::new("cmd-right", End, Some("BodyInput")),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext, TestAppContext};

    struct EventRecorder {
        events: Vec<BodyInputEvent>,
        _subscription: Subscription,
    }

    impl EventRecorder {
        fn new(input: Entity<BodyInput>, cx: &mut Context<Self>) -> Self {
            Self {
                events: Vec::new(),
                _subscription: cx.subscribe(&input, Self::on_event),
            }
        }

        fn on_event(
            &mut self,
            input: Entity<BodyInput>,
            event: &BodyInputEvent,
            cx: &mut Context<Self>,
        ) {
            if let BodyInputEvent::FormDataChanged(entries) = event {
                assert_eq!(input.read(cx).form_data_entry_count(cx), entries.len());
            }
            self.events.push(event.clone());
        }
    }

    #[gpui::test]
    fn form_row_count_tracks_edits_including_disabled_and_blank_rows(cx: &mut TestAppContext) {
        let input = cx.new(BodyInput::new);
        let recorder = cx.new(|cx| EventRecorder::new(input.clone(), cx));
        input.update(cx, |input, cx| {
            input.set_type_silent(BodyType::FormData, cx);
            assert_eq!(input.form_data_entry_count(cx), 1);
        });

        let mut check_edit = |edit: fn(&mut BodyInput, &mut Context<BodyInput>), expected| {
            input.update(cx, |input, cx| {
                edit(input, cx);
                assert_eq!(input.form_data_entry_count(cx), expected);
            });
            recorder.update(cx, |recorder, _| {
                assert!(matches!(recorder.events.as_slice(),
                    [BodyInputEvent::FormDataChanged(entries)] if entries.len() == expected
                ));
                recorder.events.clear();
            });
        };
        check_edit(
            |input, cx| {
                input.set_form_data_entries(
                    vec![
                        FormDataEntry::text("disabled", "value", false),
                        FormDataEntry::text("", "", true),
                    ],
                    cx,
                )
            },
            2,
        );
        check_edit(|input, cx| input.toggle_form_data_entry(0, cx), 2);
        check_edit(BodyInput::add_form_data_entry, 3);
        check_edit(|input, cx| input.remove_form_data_entry(1, cx), 2);
        check_edit(BodyInput::clear, 1);
        check_edit(|input, cx| input.remove_form_data_entry(0, cx), 1);
    }

    #[gpui::test]
    fn view_model_projection_preserves_row_count_and_user_events(cx: &mut TestAppContext) {
        let input = cx.new(BodyInput::new);
        let recorder = cx.new(|cx| EventRecorder::new(input.clone(), cx));
        input.update(cx, |input, cx| {
            input.project_content("投影😀", cx);
            input
                .project_form_data_entries(vec![FormDataEntry::text("key", "value", false); 3], cx);
            assert_eq!(input.form_data_entry_count(cx), 3);
            for mode in [BodyType::FormData, BodyType::Raw, BodyType::Json] {
                input.set_type_silent(mode, cx);
                assert_eq!(input.form_data_entry_count(cx), 3);
            }
            input.set_form_data_allows_files(true, cx);
            input.set_form_data_allows_files(false, cx);
            assert_eq!(input.form_data_entry_count(cx), 3);
            input.project_form_data_entries_with_rebind(vec![], cx);
            assert_eq!(input.form_data_entry_count(cx), 1);
        });
        assert!(recorder.read_with(cx, |recorder, _| recorder.events.is_empty()));

        // UI actions mutate the child directly; neither counting nor event forwarding may
        // depend on going through a parent mutation wrapper.
        let form = input.read_with(cx, |input, _| input.form_input.clone());
        form.update(cx, FormBodyInput::add_form_data_entry);
        assert_eq!(
            input.read_with(cx, |input, cx| input.form_data_entry_count(cx)),
            2
        );
        assert!(matches!(
            recorder.read_with(cx, |recorder, _| recorder.events.clone()).as_slice(),
            [BodyInputEvent::FormDataChanged(entries)] if entries.len() == 2
        ));
        recorder.update(cx, |recorder, _| recorder.events.clear());
        input.update(cx, |input, cx| {
            input.set_type_silent(BodyType::Json, cx);
            input.set_content("user edit", cx);
        });
        assert!(matches!(
            recorder.read_with(cx, |recorder, _| recorder.events.clone()).as_slice(),
            [BodyInputEvent::ValueChanged(value)] if value == "user edit"
        ));
    }

    #[test]
    fn test_body_type_enum() {
        assert_eq!(BodyType::Json, BodyType::Json);
        assert_eq!(BodyType::FormData, BodyType::FormData);
        assert_eq!(BodyType::Raw, BodyType::Raw);
        assert_ne!(BodyType::Json, BodyType::FormData);
    }

    #[test]
    fn test_form_data_entry_creation() {
        let entry = FormDataEntry::text("username", "john_doe", true);
        assert_eq!(entry.key, "username");
        assert_eq!(entry.value, "john_doe");
        assert!(entry.enabled);
    }

    #[test]
    fn test_form_data_entry_disabled() {
        let entry = FormDataEntry::text("api_key", "secret123", false);
        assert!(!entry.enabled);
    }
}

//! Compatibility adapter for the two request-body editing surfaces.
//!
//! `BodyInput` owns type selection and forwards the existing public events. Text editing mechanics
//! live in `TextBodyInput`; typed form-row mechanics live in `FormBodyInput`. Neither child owns
//! request semantics or transport serialization—the workspace ViewModel remains authoritative.

use form_body_input::{FormBodyInput, FormBodyInputEvent};
use gpui::{
    actions, div, prelude::FluentBuilder, px, rgb, App, AppContext, Context, Entity, EventEmitter,
    FocusHandle, Focusable, InteractiveElement, IntoElement, KeyBinding, ParentElement, Render,
    Styled, Subscription, Window,
};
use std::path::PathBuf;
use text_body_input::{TextBodyInput, TextBodyInputEvent};

use crate::ui::theme::{CODE_BG, FONT_UI, INFO, PANEL, TEXT};

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

/// Thin compatibility surface for callers that switch between JSON/raw and form body modes.
pub struct BodyInput {
    show_type_tabs: bool,
    current_type: BodyType,
    text_input: Entity<TextBodyInput>,
    form_input: Entity<FormBodyInput>,
    form_entry_count: usize,
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
        let form_entry_count = form_input.read(cx).entries().len();
        let subscriptions = vec![
            cx.subscribe(&text_input, Self::on_text_event),
            cx.subscribe(&form_input, Self::on_form_event),
        ];

        Self {
            show_type_tabs: true,
            current_type: BodyType::Json,
            text_input,
            form_input,
            form_entry_count,
            _subscriptions: subscriptions,
        }
    }

    pub fn with_placeholder(self, _placeholder: &str) -> Self {
        self
    }

    pub fn with_type_tabs(mut self, show_type_tabs: bool) -> Self {
        self.show_type_tabs = show_type_tabs;
        self
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
        self.form_entry_count = entries.len();
        cx.emit(BodyInputEvent::FormDataChanged(entries.clone()));
        cx.notify();
    }

    fn refresh_form_metadata(&mut self, cx: &App) {
        self.form_entry_count = self.form_input.read(cx).entries().len();
    }

    pub fn set_type(&mut self, body_type: BodyType, cx: &mut Context<Self>) {
        if self.current_type == body_type {
            return;
        }

        self.current_type = body_type;
        match body_type {
            BodyType::Json | BodyType::Raw => cx.emit(BodyInputEvent::ValueChanged(
                self.text_input.read(cx).content().to_string(),
            )),
            BodyType::FormData => cx.emit(BodyInputEvent::FormDataChanged(
                self.form_input.read(cx).entries().to_vec(),
            )),
        }
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
        self.refresh_form_metadata(cx);
    }

    pub fn set_content(&mut self, content: impl Into<String>, cx: &mut Context<Self>) {
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

    pub fn add_form_data_entry(&mut self, cx: &mut Context<Self>) {
        self.form_input
            .update(cx, FormBodyInput::add_form_data_entry);
        self.refresh_form_metadata(cx);
    }

    pub fn remove_form_data_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        self.form_input
            .update(cx, |input, cx| input.remove_form_data_entry(index, cx));
        self.refresh_form_metadata(cx);
    }

    pub fn toggle_form_data_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        self.form_input
            .update(cx, |input, cx| input.toggle_form_data_entry(index, cx));
        self.refresh_form_metadata(cx);
    }

    pub fn form_data_entry_count(&self) -> usize {
        self.form_entry_count
    }

    pub fn set_form_data_entries(&mut self, entries: Vec<FormDataEntry>, cx: &mut Context<Self>) {
        self.form_input
            .update(cx, |input, cx| input.set_form_data_entries(entries, cx));
        self.refresh_form_metadata(cx);
    }

    /// Projects parsed form data without turning the projection into a user edit event.
    pub fn project_form_data_entries(
        &mut self,
        entries: Vec<FormDataEntry>,
        cx: &mut Context<Self>,
    ) {
        self.form_input
            .update(cx, |input, cx| input.project_form_data_entries(entries, cx));
        self.refresh_form_metadata(cx);
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
        self.refresh_form_metadata(cx);
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        match self.current_type {
            BodyType::Json | BodyType::Raw => {
                self.text_input.update(cx, TextBodyInput::clear);
            }
            BodyType::FormData => {
                self.form_input.update(cx, FormBodyInput::clear);
                self.refresh_form_metadata(cx);
            }
        }
    }

    pub fn start_editing_key(&mut self, index: usize, cx: &mut Context<Self>) {
        self.form_input
            .update(cx, |input, cx| input.start_editing_key(index, cx));
    }

    pub fn start_editing_value(&mut self, index: usize, cx: &mut Context<Self>) {
        self.form_input
            .update(cx, |input, cx| input.start_editing_value(index, cx));
    }

    pub fn finish_editing(&mut self, cx: &mut Context<Self>) {
        self.form_input.update(cx, FormBodyInput::finish_editing);
    }

    pub fn finish_key_editing_only(&mut self, cx: &mut Context<Self>) {
        self.form_input
            .update(cx, FormBodyInput::finish_key_editing_only);
    }

    pub fn finish_value_editing_only(&mut self, cx: &mut Context<Self>) {
        self.form_input
            .update(cx, FormBodyInput::finish_value_editing_only);
    }

    pub fn cancel_editing(&mut self, cx: &mut Context<Self>) {
        self.form_input.update(cx, FormBodyInput::cancel_editing);
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
            .bg(rgb(if current_type == BodyType::FormData {
                PANEL
            } else {
                CODE_BG
            }))
            .when(self.show_type_tabs, |root| {
                root.child(
                    div()
                        .h(px(40.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap_4()
                        .px_4()
                        .bg(rgb(0x00ff_ffff))
                        .border_b_1()
                        .border_color(rgb(0x00e2_e8f0))
                        .child(
                            div()
                                .cursor_pointer()
                                .font_family(FONT_UI)
                                .text_size(px(12.0))
                                .when(current_type == BodyType::Json, |div| {
                                    div.text_color(rgb(INFO))
                                        .font_weight(gpui::FontWeight::BOLD)
                                })
                                .when(current_type != BodyType::Json, |div| {
                                    div.text_color(rgb(0x0047_5569))
                                        .hover(|style| style.text_color(rgb(TEXT)))
                                })
                                .child("● JSON ▾")
                                .on_mouse_up(
                                    gpui::MouseButton::Left,
                                    cx.listener(|this, _, _, cx| this.set_type(BodyType::Json, cx)),
                                ),
                        )
                        .child(
                            div()
                                .cursor_pointer()
                                .font_family(FONT_UI)
                                .text_size(px(12.0))
                                .when(current_type == BodyType::FormData, |div| {
                                    div.text_color(rgb(INFO))
                                        .font_weight(gpui::FontWeight::BOLD)
                                })
                                .when(current_type != BodyType::FormData, |div| {
                                    div.text_color(rgb(0x0047_5569))
                                        .hover(|style| style.text_color(rgb(TEXT)))
                                })
                                .child("○ form-data")
                                .on_mouse_up(
                                    gpui::MouseButton::Left,
                                    cx.listener(|this, _, _, cx| {
                                        this.set_type(BodyType::FormData, cx)
                                    }),
                                ),
                        )
                        .child(
                            div()
                                .cursor_pointer()
                                .font_family(FONT_UI)
                                .text_size(px(12.0))
                                .when(current_type == BodyType::Raw, |div| {
                                    div.text_color(rgb(INFO))
                                        .font_weight(gpui::FontWeight::BOLD)
                                })
                                .when(current_type != BodyType::Raw, |div| {
                                    div.text_color(rgb(0x0047_5569))
                                        .hover(|style| style.text_color(rgb(TEXT)))
                                })
                                .child("○ raw")
                                .on_mouse_up(
                                    gpui::MouseButton::Left,
                                    cx.listener(|this, _, _, cx| this.set_type(BodyType::Raw, cx)),
                                ),
                        ),
                )
            })
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
            _input: Entity<BodyInput>,
            event: &BodyInputEvent,
            _cx: &mut Context<Self>,
        ) {
            self.events.push(event.clone());
        }
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

    #[gpui::test]
    fn view_model_projection_is_silent_but_user_updates_are_forwarded(cx: &mut TestAppContext) {
        let input = cx.new(BodyInput::new);
        let recorder = cx.new(|cx| EventRecorder::new(input.clone(), cx));

        input.update(cx, |input, cx| {
            input.project_content("投影😀", cx);
            input.set_type_silent(BodyType::FormData, cx);
            input.project_form_data_entries(
                vec![FormDataEntry::text("projected", "value", false)],
                cx,
            );
        });
        assert!(recorder.read_with(cx, |recorder, _| recorder.events.is_empty()));

        input.update(cx, |input, cx| {
            input.set_type_silent(BodyType::Json, cx);
            input.set_content("user edit", cx);
        });
        assert!(matches!(
            recorder.read_with(cx, |recorder, _| recorder.events.clone()).as_slice(),
            [BodyInputEvent::ValueChanged(value)] if value == "user edit"
        ));
    }
}

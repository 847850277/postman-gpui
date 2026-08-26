use super::{
    Backspace, Copy, Cut, Delete, End, Enter, Escape, FormDataEntry, FormDataFile, Home, Left,
    Paste, Redo, Right, SelectAll, SelectLeft, SelectRight, SelectWordLeft, SelectWordRight,
    ShiftTab, Tab, Undo, WordLeft, WordRight,
};
use crate::ui::{
    components::common::edit_context_menu::{
        edit_context_menu, EditContextAction, EDITABLE_ACTIONS,
    },
    components::common::keyboard::ActivateControl,
    components::common::scrollbar::{scrollbar_geometry, ScrollbarGeometry},
    components::input::edit_history::{next_word_boundary, previous_word_boundary, EditHistory},
    theme::{
        ACCENT_SOFT, FONT_MONO, FONT_UI, INFO, INFO_SOFT, LINE, MUTED, OK, OK_SOFT, PANEL,
        PANEL_ALT, SUBTEXT, TEXT,
    },
};
use gpui::{
    div, fill, point, prelude::FluentBuilder, px, relative, rgb, rgba, size, App, Bounds,
    ClipboardItem, Context, CursorStyle, Element, ElementId, Entity, EventEmitter, FocusHandle,
    Focusable, GlobalElementId, InteractiveElement, IntoElement, KeyDownEvent, LayoutId,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, ParentElement, Pixels,
    Point, Render, Role, ScrollHandle, ShapedLine, SharedString, StatefulInteractiveElement, Style,
    Styled, TextAlign, TextRun, Window,
};
use std::{ops::Range, path::PathBuf};
use unicode_segmentation::UnicodeSegmentation;

const FORM_DATA_ROW_HEIGHT: f32 = 38.0;
const FORM_DATA_ROW_GAP: f32 = 8.0;
const FORM_DATA_ROWS_PADDING: f32 = 16.0;
const FORM_DATA_MAX_VISIBLE_ROWS: usize = 6;

#[derive(Clone, Debug)]
pub(super) enum FormBodyInputEvent {
    Changed(Vec<FormDataEntry>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FormEditSnapshot {
    entries: Vec<FormDataEntry>,
    editing_key_index: Option<usize>,
    editing_value_index: Option<usize>,
    temp_key_value: String,
    temp_value_value: String,
    key_selection: Range<usize>,
    key_selection_reversed: bool,
    value_selection: Range<usize>,
    value_selection_reversed: bool,
}

/// Stateful URL-encoded/multipart body editor. It owns row editing mechanics and file-picker state;
/// typed request-body drafts remain authoritative in the shared workspace ViewModel.
pub(super) struct FormBodyInput {
    focus_handle: FocusHandle,
    form_data_allows_files: bool,
    form_data_scroll: ScrollHandle,
    form_data_entries: Vec<FormDataEntry>,
    editing_key_index: Option<usize>,
    editing_value_index: Option<usize>,
    temp_key_value: String,
    temp_value_value: String,
    form_key_selected_range: Range<usize>,
    form_key_selection_reversed: bool,
    form_key_is_selecting: bool,
    form_value_selected_range: Range<usize>,
    form_value_selection_reversed: bool,
    form_value_is_selecting: bool,
    form_key_last_layout: Option<ShapedLine>,
    form_key_last_bounds: Option<Bounds<Pixels>>,
    form_value_last_layout: Option<ShapedLine>,
    form_value_last_bounds: Option<Bounds<Pixels>>,
    context_menu_position: Option<Point<Pixels>>,
    edit_history: EditHistory<FormEditSnapshot>,
    row_toggle_focus_handles: Vec<FocusHandle>,
    row_type_focus_handles: Vec<FocusHandle>,
    row_file_focus_handles: Vec<FocusHandle>,
    row_delete_focus_handles: Vec<FocusHandle>,
    add_row_focus_handle: FocusHandle,
}

impl EventEmitter<FormBodyInputEvent> for FormBodyInput {}

impl Focusable for FormBodyInput {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl FormBodyInput {
    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            form_data_allows_files: false,
            form_data_scroll: ScrollHandle::new(),
            form_data_entries: vec![FormDataEntry::text("", "", true)],
            editing_key_index: None,
            editing_value_index: None,
            temp_key_value: String::new(),
            temp_value_value: String::new(),
            form_key_selected_range: 0..0,
            form_key_selection_reversed: false,
            form_key_is_selecting: false,
            form_value_selected_range: 0..0,
            form_value_selection_reversed: false,
            form_value_is_selecting: false,
            form_key_last_layout: None,
            form_key_last_bounds: None,
            form_value_last_layout: None,
            form_value_last_bounds: None,
            context_menu_position: None,
            edit_history: EditHistory::default(),
            row_toggle_focus_handles: vec![cx.focus_handle().tab_index(0).tab_stop(true)],
            row_type_focus_handles: vec![cx.focus_handle().tab_index(0).tab_stop(true)],
            row_file_focus_handles: vec![cx.focus_handle().tab_index(0).tab_stop(true)],
            row_delete_focus_handles: vec![cx.focus_handle().tab_index(0).tab_stop(true)],
            add_row_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
        }
    }

    pub(super) fn entries(&self) -> &[FormDataEntry] {
        &self.form_data_entries
    }

    pub(super) fn set_form_data_allows_files(
        &mut self,
        allows_files: bool,
        cx: &mut Context<Self>,
    ) {
        if self.form_data_allows_files != allows_files {
            self.form_data_allows_files = allows_files;
            if !allows_files {
                for entry in &mut self.form_data_entries {
                    if let Some(file) = entry.file.take() {
                        entry.value = file.path.display().to_string();
                    }
                }
            }
            cx.notify();
        }
    }

    pub(super) fn add_form_data_entry(&mut self, cx: &mut Context<Self>) {
        self.record_edit();
        self.form_data_entries
            .push(FormDataEntry::text("", "", true));
        self.ensure_control_focus_handles(cx);
        self.form_data_scroll.scroll_to_bottom();
        self.emit_form_data_changed(cx);
        cx.notify();
    }

    pub(super) fn remove_form_data_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.form_data_entries.len() {
            self.record_edit();
            self.form_data_entries.remove(index);
            self.remove_control_focus_handles(index);
            self.editing_key_index = adjusted_editing_index(self.editing_key_index, index);
            self.editing_value_index = adjusted_editing_index(self.editing_value_index, index);
            if self.form_data_entries.is_empty() {
                self.form_data_entries
                    .push(FormDataEntry::text("", "", true));
            }
            self.ensure_control_focus_handles(cx);
            self.emit_form_data_changed(cx);
            cx.notify();
        }
    }

    pub(super) fn toggle_form_data_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.form_data_entries.len() {
            self.record_edit();
            let entry = &mut self.form_data_entries[index];
            entry.enabled = !entry.enabled;
            self.emit_form_data_changed(cx);
            cx.notify();
        }
    }

    fn emit_form_data_changed(&self, cx: &mut Context<Self>) {
        cx.emit(FormBodyInputEvent::Changed(self.form_data_entries.clone()));
    }

    /// Persists the active editor buffer immediately so the ViewModel always reflects what the
    /// user can currently see, even before Enter, Tab, or a focus change.
    fn persist_active_form_edit(&mut self, cx: &mut Context<Self>) {
        let changed = if let Some(index) = self.editing_key_index {
            self.form_data_entries.get_mut(index).is_some_and(|entry| {
                if entry.key == self.temp_key_value {
                    false
                } else {
                    entry.key.clone_from(&self.temp_key_value);
                    true
                }
            })
        } else if let Some(index) = self.editing_value_index {
            self.form_data_entries.get_mut(index).is_some_and(|entry| {
                if entry.value == self.temp_value_value {
                    false
                } else {
                    entry.value.clone_from(&self.temp_value_value);
                    true
                }
            })
        } else {
            false
        };

        if changed {
            self.emit_form_data_changed(cx);
        }
    }

    fn toggle_form_data_value_kind(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.form_data_allows_files {
            return;
        }
        if index >= self.form_data_entries.len() {
            return;
        }
        self.record_edit();
        let entry = &mut self.form_data_entries[index];
        if let Some(file) = entry.file.take() {
            entry.value = file.path.display().to_string();
        } else {
            entry.value.clear();
            entry.file = Some(FormDataFile {
                path: PathBuf::new(),
                file_name: None,
                content_type: None,
            });
        }
        self.cancel_editing(cx);
        self.emit_form_data_changed(cx);
        cx.notify();
    }

    fn choose_form_data_file(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.form_data_allows_files
            || self
                .form_data_entries
                .get(index)
                .is_none_or(|entry| entry.file.is_none())
        {
            return;
        }

        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select multipart file".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let path = match paths.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            let Some(path) = path else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                if index < this.form_data_entries.len() {
                    this.record_edit();
                    let entry = &mut this.form_data_entries[index];
                    let content_type = mime_guess::from_path(&path).first_raw().map(str::to_string);
                    entry.file = Some(FormDataFile {
                        file_name: path
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned()),
                        path,
                        content_type,
                    });
                    this.emit_form_data_changed(cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn set_form_data_entries(
        &mut self,
        entries: Vec<FormDataEntry>,
        cx: &mut Context<Self>,
    ) {
        self.record_edit();
        self.form_data_entries = entries;
        if self.form_data_entries.is_empty() {
            self.form_data_entries
                .push(FormDataEntry::text("", "", true));
        }
        self.ensure_control_focus_handles(cx);
        self.emit_form_data_changed(cx);
        cx.notify();
    }

    /// Projects parsed form data without turning the projection into a user edit event.
    pub(super) fn project_form_data_entries(
        &mut self,
        mut entries: Vec<FormDataEntry>,
        cx: &mut Context<Self>,
    ) {
        self.edit_history.clear();
        if entries.is_empty() {
            entries.push(FormDataEntry::text("", "", true));
        }
        if self.form_data_entries != entries {
            self.form_data_entries = entries;
            self.editing_key_index = None;
            self.editing_value_index = None;
            cx.notify();
        }
        self.ensure_control_focus_handles(cx);
    }

    pub(super) fn start_editing_key(&mut self, index: usize, cx: &mut Context<Self>) {
        self.edit_history.break_typing_group();
        // 首先完成任何现有的编辑
        if self.editing_value_index.is_some() {
            self.finish_value_editing_only(cx);
        }

        if let Some(entry) = self.form_data_entries.get(index) {
            self.editing_key_index = Some(index);
            self.editing_value_index = None;
            self.temp_key_value = entry.key.clone();
            // 初始化光标位置到文本末尾
            let len = self.temp_key_value.len();
            self.form_key_selected_range = len..len;
            self.form_key_selection_reversed = false;
            self.form_key_is_selecting = false;
            cx.notify();
        }
    }

    pub(super) fn start_editing_value(&mut self, index: usize, cx: &mut Context<Self>) {
        self.edit_history.break_typing_group();
        // 首先完成任何现有的编辑
        if self.editing_key_index.is_some() {
            self.finish_key_editing_only(cx);
        }

        if let Some(entry) = self.form_data_entries.get(index) {
            if entry.file.is_some() {
                return;
            }
            self.editing_value_index = Some(index);
            self.editing_key_index = None;
            self.temp_value_value = entry.value.clone();
            // 初始化光标位置到文本末尾
            let len = self.temp_value_value.len();
            self.form_value_selected_range = len..len;
            self.form_value_selection_reversed = false;
            self.form_value_is_selecting = false;
            cx.notify();
        }
    }

    pub(super) fn finish_editing(&mut self, cx: &mut Context<Self>) {
        self.edit_history.break_typing_group();
        self.editing_key_index = None;
        self.editing_value_index = None;
        self.temp_key_value.clear();
        self.temp_value_value.clear();
        cx.notify();
    }

    pub(super) fn finish_key_editing_only(&mut self, cx: &mut Context<Self>) {
        if self.editing_key_index.is_some() {
            self.editing_key_index = None;
            self.temp_key_value.clear();
        }
        cx.notify();
    }

    pub(super) fn finish_value_editing_only(&mut self, cx: &mut Context<Self>) {
        if self.editing_value_index.is_some() {
            self.editing_value_index = None;
            self.temp_value_value.clear();
        }
        cx.notify();
    }

    pub(super) fn cancel_editing(&mut self, cx: &mut Context<Self>) {
        self.edit_history.break_typing_group();
        self.editing_key_index = None;
        self.editing_value_index = None;
        self.temp_key_value.clear();
        self.temp_value_value.clear();
        cx.notify();
    }

    pub(super) fn clear(&mut self, cx: &mut Context<Self>) {
        if self.form_data_entries == [FormDataEntry::text("", "", true)] {
            return;
        }
        self.record_edit();
        self.form_data_entries = vec![FormDataEntry::text("", "", true)];
        self.ensure_control_focus_handles(cx);
        self.cancel_editing(cx);
        self.emit_form_data_changed(cx);
        cx.notify();
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        let before = self.snapshot();
        if let Some(_index) = self.editing_key_index {
            if self.form_key_selected_range.is_empty() {
                // 没有选择，删除光标前的一个字符
                let cursor = self.form_key_cursor_offset();
                if cursor > 0 {
                    let prev = self.form_key_previous_boundary(cursor);
                    self.temp_key_value.replace_range(prev..cursor, "");
                    self.form_key_selected_range = prev..prev;
                    self.form_key_selection_reversed = false;
                }
            } else {
                // 有选择，删除选中的文本
                self.temp_key_value
                    .replace_range(self.form_key_selected_range.clone(), "");
                let start = self.form_key_selected_range.start;
                self.form_key_selected_range = start..start;
                self.form_key_selection_reversed = false;
            }
            self.persist_active_form_edit(cx);
            cx.notify();
        } else if let Some(_index) = self.editing_value_index {
            if self.form_value_selected_range.is_empty() {
                let cursor = self.form_value_cursor_offset();
                if cursor > 0 {
                    let prev = self.form_value_previous_boundary(cursor);
                    self.temp_value_value.replace_range(prev..cursor, "");
                    self.form_value_selected_range = prev..prev;
                    self.form_value_selection_reversed = false;
                }
            } else {
                self.temp_value_value
                    .replace_range(self.form_value_selected_range.clone(), "");
                let start = self.form_value_selected_range.start;
                self.form_value_selected_range = start..start;
                self.form_value_selection_reversed = false;
            }
            self.persist_active_form_edit(cx);
            cx.notify();
        }
        if self.snapshot() != before {
            self.edit_history.record(before);
        }
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        let before = self.snapshot();
        if self.editing_key_index.is_some() {
            if self.form_key_selected_range.is_empty() {
                let cursor = self.form_key_cursor_offset();
                let next = self.form_key_next_boundary(cursor);
                self.temp_key_value.replace_range(cursor..next, "");
                self.form_key_selected_range = cursor..cursor;
            } else {
                let start = self.form_key_selected_range.start;
                self.temp_key_value
                    .replace_range(self.form_key_selected_range.clone(), "");
                self.form_key_selected_range = start..start;
            }
            self.form_key_selection_reversed = false;
            self.persist_active_form_edit(cx);
            cx.notify();
        } else if self.editing_value_index.is_some() {
            if self.form_value_selected_range.is_empty() {
                let cursor = self.form_value_cursor_offset();
                let next = self.form_value_next_boundary(cursor);
                self.temp_value_value.replace_range(cursor..next, "");
                self.form_value_selected_range = cursor..cursor;
            } else {
                let start = self.form_value_selected_range.start;
                self.temp_value_value
                    .replace_range(self.form_value_selected_range.clone(), "");
                self.form_value_selected_range = start..start;
            }
            self.form_value_selection_reversed = false;
            self.persist_active_form_edit(cx);
            cx.notify();
        }
        if self.snapshot() != before {
            self.edit_history.record(before);
        }
    }

    fn enter(&mut self, _: &Enter, _: &mut Window, cx: &mut Context<Self>) {
        if self.editing_key_index.is_some() || self.editing_value_index.is_some() {
            self.finish_editing(cx);
        }
    }

    fn escape(&mut self, _: &Escape, _: &mut Window, cx: &mut Context<Self>) {
        if self.context_menu_position.take().is_some() {
            cx.notify();
            return;
        }
        if self.editing_key_index.is_some() || self.editing_value_index.is_some() {
            self.cancel_editing(cx);
        }
    }

    fn tab(&mut self, _: &Tab, window: &mut Window, cx: &mut Context<Self>) {
        // Tab 键在 FormData 条目之间导航
        if let Some(index) = self.editing_key_index {
            // 从 key 切换到 value - start_editing_value 会自动完成 key 编辑
            self.start_editing_value(index, cx);
        } else if let Some(index) = self.editing_value_index {
            // 从 value 切换到下一行的 key，或者添加新行
            if index + 1 < self.form_data_entries.len() {
                self.start_editing_key(index + 1, cx);
            } else {
                self.add_form_data_entry(cx);
                self.start_editing_key(self.form_data_entries.len() - 1, cx);
            }
        } else {
            window.focus_next(cx);
        }
    }

    fn shift_tab(&mut self, _: &ShiftTab, window: &mut Window, cx: &mut Context<Self>) {
        // Shift+Tab 键反向导航
        if let Some(index) = self.editing_value_index {
            // 从 value 切换到 key - start_editing_key 会自动完成 value 编辑
            self.start_editing_key(index, cx);
        } else if let Some(index) = self.editing_key_index {
            // 从 key 切换到上一行的 value
            if index > 0 {
                self.start_editing_value(index - 1, cx);
            }
        } else {
            window.focus_prev(cx);
        }
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.editing_key_index.is_some() {
            if self.form_key_selected_range.is_empty() {
                self.form_key_move_to(
                    self.form_key_previous_boundary(self.form_key_cursor_offset()),
                    cx,
                );
            } else {
                self.form_key_move_to(self.form_key_selected_range.start, cx);
            }
        } else if self.editing_value_index.is_some() {
            if self.form_value_selected_range.is_empty() {
                self.form_value_move_to(
                    self.form_value_previous_boundary(self.form_value_cursor_offset()),
                    cx,
                );
            } else {
                self.form_value_move_to(self.form_value_selected_range.start, cx);
            }
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.editing_key_index.is_some() {
            if self.form_key_selected_range.is_empty() {
                self.form_key_move_to(
                    self.form_key_next_boundary(self.form_key_cursor_offset()),
                    cx,
                );
            } else {
                self.form_key_move_to(self.form_key_selected_range.end, cx);
            }
        } else if self.editing_value_index.is_some() {
            if self.form_value_selected_range.is_empty() {
                self.form_value_move_to(
                    self.form_value_next_boundary(self.form_value_cursor_offset()),
                    cx,
                );
            } else {
                self.form_value_move_to(self.form_value_selected_range.end, cx);
            }
        }
    }

    fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        if self.editing_key_index.is_some() {
            let offset =
                previous_word_boundary(&self.temp_key_value, self.form_key_cursor_offset());
            self.form_key_move_to(offset, cx);
        } else if self.editing_value_index.is_some() {
            let offset =
                previous_word_boundary(&self.temp_value_value, self.form_value_cursor_offset());
            self.form_value_move_to(offset, cx);
        }
    }

    fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        if self.editing_key_index.is_some() {
            let offset = next_word_boundary(&self.temp_key_value, self.form_key_cursor_offset());
            self.form_key_move_to(offset, cx);
        } else if self.editing_value_index.is_some() {
            let offset =
                next_word_boundary(&self.temp_value_value, self.form_value_cursor_offset());
            self.form_value_move_to(offset, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        if self.editing_key_index.is_some() {
            self.form_key_select_to(
                self.form_key_previous_boundary(self.form_key_cursor_offset()),
                cx,
            );
        } else if self.editing_value_index.is_some() {
            self.form_value_select_to(
                self.form_value_previous_boundary(self.form_value_cursor_offset()),
                cx,
            );
        }
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        if self.editing_key_index.is_some() {
            self.form_key_select_to(
                self.form_key_next_boundary(self.form_key_cursor_offset()),
                cx,
            );
        } else if self.editing_value_index.is_some() {
            self.form_value_select_to(
                self.form_value_next_boundary(self.form_value_cursor_offset()),
                cx,
            );
        }
    }

    fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        if self.editing_key_index.is_some() {
            let offset =
                previous_word_boundary(&self.temp_key_value, self.form_key_cursor_offset());
            self.form_key_select_to(offset, cx);
        } else if self.editing_value_index.is_some() {
            let offset =
                previous_word_boundary(&self.temp_value_value, self.form_value_cursor_offset());
            self.form_value_select_to(offset, cx);
        }
    }

    fn select_word_right(&mut self, _: &SelectWordRight, _: &mut Window, cx: &mut Context<Self>) {
        if self.editing_key_index.is_some() {
            let offset = next_word_boundary(&self.temp_key_value, self.form_key_cursor_offset());
            self.form_key_select_to(offset, cx);
        } else if self.editing_value_index.is_some() {
            let offset =
                next_word_boundary(&self.temp_value_value, self.form_value_cursor_offset());
            self.form_value_select_to(offset, cx);
        }
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        if self.editing_key_index.is_some() {
            self.form_key_move_to(0, cx);
            self.form_key_select_to(self.temp_key_value.len(), cx);
        } else if self.editing_value_index.is_some() {
            self.form_value_move_to(0, cx);
            self.form_value_select_to(self.temp_value_value.len(), cx);
        }
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        let current = self.snapshot();
        if let Some(previous) = self.edit_history.undo(current) {
            self.restore_snapshot(previous, cx);
        }
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        let current = self.snapshot();
        if let Some(next) = self.edit_history.redo(current) {
            self.restore_snapshot(next, cx);
        }
    }

    fn form_paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let single_line = text
                .chars()
                .filter(|character| !matches!(character, '\r' | '\n'))
                .collect::<String>();
            self.replace_form_selection(&single_line, false, cx);
        }
    }

    fn form_copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        let selected_text = if self.editing_key_index.is_some()
            && !self.form_key_selected_range.is_empty()
        {
            Some(self.temp_key_value[self.form_key_selected_range.clone()].to_string())
        } else if self.editing_value_index.is_some() && !self.form_value_selected_range.is_empty() {
            Some(self.temp_value_value[self.form_value_selected_range.clone()].to_string())
        } else {
            None
        };

        if let Some(selected_text) = selected_text {
            cx.write_to_clipboard(ClipboardItem::new_string(selected_text));
        }
    }

    fn form_cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        let has_selection = (self.editing_key_index.is_some()
            && !self.form_key_selected_range.is_empty())
            || (self.editing_value_index.is_some() && !self.form_value_selected_range.is_empty());
        if has_selection {
            self.form_copy(&Copy, window, cx);
            self.replace_form_selection("", false, cx);
        }
    }

    fn replace_form_selection(
        &mut self,
        text: &str,
        coalesce_typing: bool,
        cx: &mut Context<Self>,
    ) {
        let before = self.snapshot();
        if self.editing_key_index.is_some() {
            let range = self.form_key_selected_range.clone();
            self.temp_key_value.replace_range(range.clone(), text);
            let cursor = range.start + text.len();
            self.form_key_selected_range = cursor..cursor;
            self.form_key_selection_reversed = false;
            self.persist_active_form_edit(cx);
            cx.notify();
        } else if self.editing_value_index.is_some() {
            let range = self.form_value_selected_range.clone();
            self.temp_value_value.replace_range(range.clone(), text);
            let cursor = range.start + text.len();
            self.form_value_selected_range = cursor..cursor;
            self.form_value_selection_reversed = false;
            self.persist_active_form_edit(cx);
            cx.notify();
        }
        if self.snapshot() != before {
            if coalesce_typing {
                self.edit_history.record_typing(before);
            } else {
                self.edit_history.record(before);
            }
        }
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        if self.editing_key_index.is_some() {
            self.form_key_move_to(0, cx);
        } else if self.editing_value_index.is_some() {
            self.form_value_move_to(0, cx);
        }
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        if self.editing_key_index.is_some() {
            self.form_key_move_to(self.temp_key_value.len(), cx);
        } else if self.editing_value_index.is_some() {
            self.form_value_move_to(self.temp_value_value.len(), cx);
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        // 只在编辑模式下处理字符输入
        if self.editing_key_index.is_none() && self.editing_value_index.is_none() {
            return;
        }

        // 处理普通字符输入
        if let Some(key_char) = &event.keystroke.key_char {
            // GPUI may deliver a Unicode character (or an IME commit) as multiple UTF-8 bytes.
            // Accept the complete text payload rather than treating byte length as character
            // length; special keys are still excluded because they have no printable key_char.
            if !key_char.is_empty() && !key_char.chars().any(char::is_control) {
                self.replace_form_selection(key_char, true, cx);
            }
        }
    }

    // JSON input action handlers
    fn form_key_cursor_offset(&self) -> usize {
        if self.form_key_selection_reversed {
            self.form_key_selected_range.start
        } else {
            self.form_key_selected_range.end
        }
    }

    fn form_key_move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.edit_history.break_typing_group();
        self.form_key_selected_range = offset..offset;
        self.form_key_selection_reversed = false;
        cx.notify();
    }

    fn form_key_select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.edit_history.break_typing_group();
        if self.form_key_selection_reversed {
            self.form_key_selected_range.start = offset;
        } else {
            self.form_key_selected_range.end = offset;
        }

        if self.form_key_selected_range.end < self.form_key_selected_range.start {
            self.form_key_selection_reversed = !self.form_key_selection_reversed;
            self.form_key_selected_range =
                self.form_key_selected_range.end..self.form_key_selected_range.start;
        }
        cx.notify();
    }

    fn form_key_previous_boundary(&self, offset: usize) -> usize {
        self.temp_key_value
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn form_key_next_boundary(&self, offset: usize) -> usize {
        self.temp_key_value
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.temp_key_value.len())
    }

    // FormData value helper methods
    fn form_value_cursor_offset(&self) -> usize {
        if self.form_value_selection_reversed {
            self.form_value_selected_range.start
        } else {
            self.form_value_selected_range.end
        }
    }

    fn form_value_move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.edit_history.break_typing_group();
        self.form_value_selected_range = offset..offset;
        self.form_value_selection_reversed = false;
        cx.notify();
    }

    fn form_value_select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.edit_history.break_typing_group();
        if self.form_value_selection_reversed {
            self.form_value_selected_range.start = offset;
        } else {
            self.form_value_selected_range.end = offset;
        }

        if self.form_value_selected_range.end < self.form_value_selected_range.start {
            self.form_value_selection_reversed = !self.form_value_selection_reversed;
            self.form_value_selected_range =
                self.form_value_selected_range.end..self.form_value_selected_range.start;
        }
        cx.notify();
    }

    fn form_value_previous_boundary(&self, offset: usize) -> usize {
        self.temp_value_value
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn form_value_next_boundary(&self, offset: usize) -> usize {
        self.temp_value_value
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.temp_value_value.len())
    }

    fn snapshot(&self) -> FormEditSnapshot {
        FormEditSnapshot {
            entries: self.form_data_entries.clone(),
            editing_key_index: self.editing_key_index,
            editing_value_index: self.editing_value_index,
            temp_key_value: self.temp_key_value.clone(),
            temp_value_value: self.temp_value_value.clone(),
            key_selection: self.form_key_selected_range.clone(),
            key_selection_reversed: self.form_key_selection_reversed,
            value_selection: self.form_value_selected_range.clone(),
            value_selection_reversed: self.form_value_selection_reversed,
        }
    }

    fn record_edit(&mut self) {
        self.edit_history.record(self.snapshot());
    }

    fn restore_snapshot(&mut self, snapshot: FormEditSnapshot, cx: &mut Context<Self>) {
        self.form_data_entries = snapshot.entries;
        self.editing_key_index = snapshot.editing_key_index;
        self.editing_value_index = snapshot.editing_value_index;
        self.temp_key_value = snapshot.temp_key_value;
        self.temp_value_value = snapshot.temp_value_value;
        self.form_key_selected_range = snapshot.key_selection;
        self.form_key_selection_reversed = snapshot.key_selection_reversed;
        self.form_value_selected_range = snapshot.value_selection;
        self.form_value_selection_reversed = snapshot.value_selection_reversed;
        self.context_menu_position = None;
        self.ensure_control_focus_handles(cx);
        self.emit_form_data_changed(cx);
        cx.notify();
    }

    fn ensure_control_focus_handles(&mut self, cx: &mut Context<Self>) {
        let target = self.form_data_entries.len();
        while self.row_toggle_focus_handles.len() < target {
            self.row_toggle_focus_handles
                .push(cx.focus_handle().tab_index(0).tab_stop(true));
            self.row_type_focus_handles
                .push(cx.focus_handle().tab_index(0).tab_stop(true));
            self.row_file_focus_handles
                .push(cx.focus_handle().tab_index(0).tab_stop(true));
            self.row_delete_focus_handles
                .push(cx.focus_handle().tab_index(0).tab_stop(true));
        }
        self.row_toggle_focus_handles.truncate(target);
        self.row_type_focus_handles.truncate(target);
        self.row_file_focus_handles.truncate(target);
        self.row_delete_focus_handles.truncate(target);
    }

    fn remove_control_focus_handles(&mut self, index: usize) {
        if index < self.row_toggle_focus_handles.len() {
            self.row_toggle_focus_handles.remove(index);
            self.row_type_focus_handles.remove(index);
            self.row_file_focus_handles.remove(index);
            self.row_delete_focus_handles.remove(index);
        }
    }

    fn focus_after_row_removal(
        &self,
        removed_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(focus) = self
            .row_toggle_focus_handles
            .get(removed_index)
            .or_else(|| self.row_toggle_focus_handles.last())
        {
            focus.focus(window, cx);
        } else {
            self.add_row_focus_handle.focus(window, cx);
        }
    }

    // FormData key mouse event handlers
    fn form_key_on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editing_key_index.is_none() {
            return;
        }
        self.context_menu_position = None;
        self.form_key_is_selecting = true;

        // 简单的文本索引估算（基于固定宽度字符）
        // 在实际应用中，可能需要更精确的文本布局信息
        let index = self.form_key_estimate_index_for_position(event.position);

        if event.modifiers.shift {
            self.form_key_select_to(index, cx);
        } else {
            self.form_key_move_to(index, cx);
        }
    }

    fn form_key_on_mouse_up(
        &mut self,
        _: &MouseUpEvent,
        _window: &mut Window,
        _: &mut Context<Self>,
    ) {
        if self.editing_key_index.is_none() {
            return;
        }
        self.form_key_is_selecting = false;
    }

    fn form_key_on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editing_key_index.is_none() {
            return;
        }
        if self.form_key_is_selecting {
            let index = self.form_key_estimate_index_for_position(event.position);
            self.form_key_select_to(index, cx);
        }
    }

    fn form_key_estimate_index_for_position(&self, position: Point<Pixels>) -> usize {
        // 使用保存的文本布局信息进行精确计算
        if self.temp_key_value.is_empty() {
            return 0;
        }

        let Some(bounds) = self.form_key_last_bounds.as_ref() else {
            return self.form_key_cursor_offset();
        };

        let Some(layout) = self.form_key_last_layout.as_ref() else {
            return self.form_key_cursor_offset();
        };

        // 计算相对于文本框的 x 位置
        let x_in_text = position.x - bounds.left();

        // 使用 ShapedLine 的 closest_index_for_x 方法获取精确索引
        layout.closest_index_for_x(x_in_text)
    }

    // FormData value mouse event handlers
    fn form_value_on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editing_value_index.is_none() {
            return;
        }
        self.context_menu_position = None;
        self.form_value_is_selecting = true;

        let index = self.form_value_estimate_index_for_position(event.position);

        if event.modifiers.shift {
            self.form_value_select_to(index, cx);
        } else {
            self.form_value_move_to(index, cx);
        }
    }

    fn form_value_on_mouse_up(
        &mut self,
        _: &MouseUpEvent,
        _window: &mut Window,
        _: &mut Context<Self>,
    ) {
        if self.editing_value_index.is_none() {
            return;
        }
        self.form_value_is_selecting = false;
    }

    fn form_value_on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editing_value_index.is_none() {
            return;
        }
        if self.form_value_is_selecting {
            let index = self.form_value_estimate_index_for_position(event.position);
            self.form_value_select_to(index, cx);
        }
    }

    fn form_value_estimate_index_for_position(&self, position: Point<Pixels>) -> usize {
        // 使用保存的文本布局信息进行精确计算
        if self.temp_value_value.is_empty() {
            return 0;
        }

        let Some(bounds) = self.form_value_last_bounds.as_ref() else {
            return self.form_value_cursor_offset();
        };

        let Some(layout) = self.form_value_last_layout.as_ref() else {
            return self.form_value_cursor_offset();
        };

        // 计算相对于文本框的 x 位置
        let x_in_text = position.x - bounds.left();

        // 使用 ShapedLine 的 closest_index_for_x 方法获取精确索引
        layout.closest_index_for_x(x_in_text)
    }

    fn open_form_context_menu(
        &mut self,
        index: usize,
        is_key: bool,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        if is_key {
            if self.editing_key_index != Some(index) {
                self.start_editing_key(index, cx);
            }
            self.form_key_is_selecting = false;
        } else {
            if self.editing_value_index != Some(index) {
                self.start_editing_value(index, cx);
            }
            self.form_value_is_selecting = false;
        }
        self.context_menu_position = Some(event.position);
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn handle_context_menu_action(
        &mut self,
        action: EditContextAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            EditContextAction::Dismiss => {}
            EditContextAction::Undo => self.undo(&Undo, window, cx),
            EditContextAction::Redo => self.redo(&Redo, window, cx),
            EditContextAction::Cut => self.form_cut(&Cut, window, cx),
            EditContextAction::Copy => self.form_copy(&Copy, window, cx),
            EditContextAction::Paste => self.form_paste(&Paste, window, cx),
            EditContextAction::SelectAll => self.select_all(&SelectAll, window, cx),
        }
        self.context_menu_position = None;
        cx.notify();
    }
}

struct FormTextElement {
    input: Entity<FormBodyInput>,
    is_key: bool, // true for key, false for value
}

struct FormPrepaintState {
    shaped_line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for FormTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for FormTextElement {
    type RequestLayoutState = ();
    type PrepaintState = FormPrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        _cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let line_height = window.line_height();
        style.size.height = line_height.into();

        (window.request_layout(style, [], _cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let (content, selected_range) = if self.is_key {
            (&input.temp_key_value, &input.form_key_selected_range)
        } else {
            (&input.temp_value_value, &input.form_value_selected_range)
        };

        if content.is_empty() {
            return FormPrepaintState {
                shaped_line: None,
                cursor: None,
                selection: None,
            };
        }

        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_str: SharedString = content.clone().into();

        let run = TextRun {
            len: line_str.len(),
            font: style.font(),
            color: style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        let shaped_line = window
            .text_system()
            .shape_line(line_str, font_size, &[run], None);

        let line_height = window.line_height();

        // Calculate cursor or selection
        let (selection_quad, cursor_quad) = if selected_range.is_empty() {
            // Show cursor
            let cursor_offset = if self.is_key {
                input.form_key_cursor_offset()
            } else {
                input.form_value_cursor_offset()
            };
            let cursor_x = shaped_line.x_for_index(cursor_offset);

            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_x, bounds.top()),
                        size(px(2.), line_height),
                    ),
                    rgb(INFO),
                )),
            )
        } else {
            // Show selection
            let start_x = shaped_line.x_for_index(selected_range.start);
            let end_x = shaped_line.x_for_index(selected_range.end);

            (
                Some(fill(
                    Bounds::from_corners(
                        point(bounds.left() + start_x, bounds.top()),
                        point(bounds.left() + end_x, bounds.top() + line_height),
                    ),
                    rgba(0x3366_ff33),
                )),
                None,
            )
        };

        FormPrepaintState {
            shaped_line: Some(shaped_line),
            cursor: cursor_quad,
            selection: selection_quad,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        // Paint selection if any
        if let Some(selection) = &prepaint.selection {
            window.paint_quad(selection.clone());
        }

        // Paint text
        if let Some(shaped_line) = &prepaint.shaped_line {
            let line_height = window.line_height();
            let _ = shaped_line.paint(
                point(bounds.left(), bounds.top()),
                line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            );
        }

        // Paint cursor if any and focused
        let focus_handle = self.input.read(cx).focus_handle.clone();
        if focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }

        // Save layout and bounds for mouse interaction
        self.input.update(cx, |input, _cx| {
            if self.is_key {
                input.form_key_last_layout = prepaint.shaped_line.clone();
                input.form_key_last_bounds = Some(bounds);
            } else {
                input.form_value_last_layout = prepaint.shaped_line.clone();
                input.form_value_last_bounds = Some(bounds);
            }
        });
    }
}
fn adjusted_editing_index(editing_index: Option<usize>, removed_index: usize) -> Option<usize> {
    match editing_index {
        Some(index) if index == removed_index => None,
        Some(index) if index > removed_index => Some(index - 1),
        other => other,
    }
}

fn form_data_scrollbar_geometry(
    row_count: usize,
    offset_y: f32,
    max_offset_y: f32,
) -> Option<ScrollbarGeometry> {
    if row_count <= FORM_DATA_MAX_VISIBLE_ROWS && max_offset_y <= 0.0 {
        return None;
    }

    let content_height = FORM_DATA_ROWS_PADDING
        + FORM_DATA_ROW_HEIGHT * row_count as f32
        + FORM_DATA_ROW_GAP * row_count.saturating_sub(1) as f32;
    let visible_fraction = if max_offset_y > 0.0 && content_height > 0.0 {
        (content_height - max_offset_y) / content_height
    } else {
        FORM_DATA_MAX_VISIBLE_ROWS as f32 / row_count as f32
    };

    Some(scrollbar_geometry(visible_fraction, offset_y, max_offset_y))
}
impl Render for FormBodyInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let form_data_entries = self.form_data_entries.clone();
        let form_data_allows_files = self.form_data_allows_files;
        let context_menu_position = self.context_menu_position;
        let row_toggle_focus_handles = self.row_toggle_focus_handles.clone();
        let row_type_focus_handles = self.row_type_focus_handles.clone();
        let row_file_focus_handles = self.row_file_focus_handles.clone();
        let row_delete_focus_handles = self.row_delete_focus_handles.clone();
        let form_data_scrollbar = form_data_scrollbar_geometry(
            form_data_entries.len(),
            self.form_data_scroll.offset().y.as_f32(),
            self.form_data_scroll.max_offset().y.as_f32(),
        );
        let editor = div()
            .debug_selector(|| "body-form-editor".into())
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_0()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(if self.focus_handle.is_focused(window) {
                rgb(INFO)
            } else {
                rgb(LINE)
            })
            .track_focus(&self.focus_handle(cx))
            .key_context("BodyInput")
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::escape))
            .on_action(cx.listener(Self::tab))
            .on_action(cx.listener(Self::shift_tab))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::form_paste))
            .on_action(cx.listener(Self::form_cut))
            .on_action(cx.listener(Self::form_copy))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_key_down(cx.listener(Self::on_key_down))
            .child(
                div()
                    .debug_selector(|| "body-form-table-header".into())
                    .h(px(30.0))
                    .flex_none()
                    .flex()
                    .gap_2()
                    .items_center()
                    .px_3()
                    .bg(rgb(PANEL_ALT))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .w(px(18.0))
                            .font_family(FONT_UI)
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_size(px(9.0))
                            .text_color(rgb(SUBTEXT))
                            .child("✓"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .font_family(FONT_UI)
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_size(px(9.0))
                            .text_color(rgb(SUBTEXT))
                            .child("KEY"),
                    )
                    .when(form_data_allows_files, |header| {
                        header.child(
                            div()
                                .w(px(64.0))
                                .font_family(FONT_UI)
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_size(px(9.0))
                                .text_color(rgb(SUBTEXT))
                                .child("TYPE"),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .font_family(FONT_UI)
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_size(px(9.0))
                            .text_color(rgb(SUBTEXT))
                            .child("VALUE"),
                    )
                    .child(
                        div()
                            .w(px(58.0))
                            .font_family(FONT_UI)
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_size(px(9.0))
                            .text_color(rgb(SUBTEXT))
                            .text_align(gpui::TextAlign::Center)
                            .child("STATE"),
                    )
                    .child(
                        div()
                            .w(px(44.0))
                            .font_family(FONT_UI)
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_size(px(9.0))
                            .text_color(rgb(SUBTEXT))
                            .text_align(gpui::TextAlign::Center)
                            .child("ACTION"),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .relative()
                    .child(
                        div()
                            .id("body-form-scroll")
                            .debug_selector(|| "body-form-scroll".into())
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .p_2()
                            .when(form_data_scrollbar.is_some(), |rows| rows.pr(px(20.0)))
                            .overflow_y_scroll()
                            .track_scroll(&self.form_data_scroll)
                            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
                            .children(form_data_entries.iter().enumerate().map(
                                |(index, entry)| {
                                    let entry_key = entry.key.clone();
                                    let entry_value = entry.value.clone();
                                    let entry_is_file = entry.file.is_some();
                                    let entry_file_name = entry.file.as_ref().and_then(|file| {
                                        if file.path.as_os_str().is_empty() {
                                            None
                                        } else {
                                            file.file_name.clone().or_else(|| {
                                                file.path.file_name().map(|name| {
                                                    name.to_string_lossy().into_owned()
                                                })
                                            })
                                        }
                                    });
                                    let entry_file_content_type =
                                        entry.file.as_ref().and_then(|file| {
                                            (!file.path.as_os_str().is_empty()).then(|| {
                                                file.content_type.clone().unwrap_or_else(|| {
                                                    "content type: automatic".to_string()
                                                })
                                            })
                                        });
                                    let entry_enabled = entry.enabled;
                                    let toggle_focus = row_toggle_focus_handles[index].clone();
                                    let mouse_toggle_focus = toggle_focus.clone();
                                    let toggle_focused = toggle_focus.is_focused(window);
                                    let type_focus = row_type_focus_handles[index].clone();
                                    let mouse_type_focus = type_focus.clone();
                                    let type_focused = type_focus.is_focused(window);
                                    let file_focus = row_file_focus_handles[index].clone();
                                    let mouse_file_focus = file_focus.clone();
                                    let file_focused = file_focus.is_focused(window);
                                    let delete_focus = row_delete_focus_handles[index].clone();
                                    let mouse_delete_focus = delete_focus.clone();
                                    let delete_focused = delete_focus.is_focused(window);

                                    div()
                                        .debug_selector(move || format!("body-form-row-{index}"))
                                        .h(px(38.0))
                                        .flex_none()
                                        .flex()
                                        .gap_2()
                                        .items_center()
                                        .bg(rgb(if entry_enabled { PANEL } else { PANEL_ALT }))
                                        .child(
                                            // Checkbox
                                            div()
                                                .id(("body-form-toggle", index))
                                                .debug_selector(move || {
                                                    format!("body-form-toggle-{index}")
                                                })
                                                .track_focus(&toggle_focus)
                                                .key_context("KeyboardButton")
                                                .role(Role::CheckBox)
                                                .aria_label(format!(
                                                    "{} form body row {}",
                                                    if entry_enabled { "Disable" } else { "Enable" },
                                                    index + 1
                                                ))
                                                .aria_selected(entry_enabled)
                                                .size(px(18.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .border_1()
                                                .border_color(rgb(if entry_enabled {
                                                    INFO
                                                } else {
                                                    LINE
                                                }))
                                                .rounded_sm()
                                                .bg(rgb(if entry_enabled { INFO } else { PANEL }))
                                                .text_color(rgb(PANEL))
                                                .font_family(FONT_UI)
                                                .font_weight(gpui::FontWeight::BOLD)
                                                .text_size(px(10.0))
                                                .cursor_pointer()
                                                .when(toggle_focused, |control| {
                                                    control.border_2().border_color(rgb(ACCENT_SOFT))
                                                })
                                                .child(if entry_enabled { "✓" } else { "" })
                                                .on_action(cx.listener(
                                                    move |this,
                                                          _: &ActivateControl,
                                                          _,
                                                          cx| {
                                                        this.toggle_form_data_entry(index, cx);
                                                    },
                                                ))
                                                .on_mouse_up(
                                                    gpui::MouseButton::Left,
                                                    cx.listener(
                                                        move |this, _event, window, cx| {
                                                            mouse_toggle_focus.focus(window, cx);
                                                            this.toggle_form_data_entry(index, cx);
                                                        },
                                                    ),
                                                ),
                                        )
                                        .child(
                                            // Key input - 可点击编辑
                                            div()
                                                .debug_selector(move || {
                                                    format!("body-form-key-{index}")
                                                })
                                                .flex_1()
                                                .px_2()
                                                .py_1()
                                                .bg(rgb(if entry_enabled { PANEL } else { PANEL_ALT }))
                                                .border_1()
                                                .border_color(
                                                    if self.editing_key_index == Some(index) {
                                                        rgb(INFO)
                                                    } else {
                                                        rgb(LINE)
                                                    },
                                                )
                                                .rounded_md()
                                                .font_family(FONT_MONO)
                                                .text_size(px(12.0))
                                                .cursor(CursorStyle::IBeam)
                                                .when(
                                                    self.editing_key_index == Some(index),
                                                    |div| {
                                                        div.child(FormTextElement {
                                                            input: cx.entity().clone(),
                                                            is_key: true,
                                                        })
                                                        .on_mouse_down(
                                                            MouseButton::Left,
                                                            cx.listener(
                                                                Self::form_key_on_mouse_down,
                                                            ),
                                                        )
                                                        .on_mouse_up(
                                                            MouseButton::Left,
                                                            cx.listener(Self::form_key_on_mouse_up),
                                                        )
                                                        .on_mouse_up_out(
                                                            MouseButton::Left,
                                                            cx.listener(Self::form_key_on_mouse_up),
                                                        )
                                                        .on_mouse_move(
                                                            cx.listener(
                                                                Self::form_key_on_mouse_move,
                                                            ),
                                                        )
                                                    },
                                                )
                                                .when(
                                                    self.editing_key_index != Some(index),
                                                    |div| {
                                                        div.when(entry_key.is_empty(), |div| {
                                                            div.text_color(rgb(SUBTEXT))
                                                                .child("Key")
                                                        })
                                                        .when(!entry_key.is_empty(), |div| {
                                                            div.text_color(rgb(if entry_enabled {
                                                                TEXT
                                                            } else {
                                                                SUBTEXT
                                                            }))
                                                                .child(entry_key.clone())
                                                        })
                                                        .on_mouse_up(
                                                            gpui::MouseButton::Left,
                                                            cx.listener(
                                                                move |this, _event, window, cx| {
                                                                    this.start_editing_key(
                                                                        index, cx,
                                                                    );
                                                                    this.focus_handle
                                                                        .focus(window, cx);
                                                                },
                                                            ),
                                                        )
                                                    },
                                                )
                                                .on_mouse_down(
                                                    MouseButton::Right,
                                                    cx.listener(move |this, event, window, cx| {
                                                        this.open_form_context_menu(
                                                            index, true, event, window, cx,
                                                        );
                                                    }),
                                                ),
                                        )
                                        .when(form_data_allows_files, |row| {
                                            row.child(
                                                div()
                                                    .id(("body-form-type", index))
                                                    .debug_selector(move || {
                                                        format!("body-form-type-{index}")
                                                    })
                                                    .track_focus(&type_focus)
                                                    .key_context("KeyboardButton")
                                                    .role(Role::Button)
                                                    .aria_label(format!(
                                                        "Use {} value for form row {}",
                                                        if entry_is_file { "text" } else { "file" },
                                                        index + 1
                                                    ))
                                                    .w(px(64.0))
                                                    .px_2()
                                                    .py_2()
                                                    .flex_none()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .bg(rgb(if !entry_enabled {
                                                        PANEL_ALT
                                                    } else if entry_is_file {
                                                        0x00df_eafe
                                                    } else {
                                                        0x00f8_f9fa
                                                    }))
                                                    .border_1()
                                                    .border_color(rgb(LINE))
                                                    .rounded_md()
                                                    .text_size(px(12.0))
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .text_color(rgb(if !entry_enabled {
                                                        MUTED
                                                    } else if entry_is_file {
                                                        0x001d_4ed8
                                                    } else {
                                                        0x0047_5569
                                                    }))
                                                    .cursor_pointer()
                                                    .when(type_focused, |control| {
                                                        control.border_2().border_color(rgb(INFO))
                                                    })
                                                    .child(if entry_is_file {
                                                        "File"
                                                    } else {
                                                        "Text"
                                                    })
                                                    .on_action(cx.listener(
                                                        move |this,
                                                              _: &ActivateControl,
                                                              _,
                                                              cx| {
                                                            this.toggle_form_data_value_kind(
                                                                index, cx,
                                                            );
                                                        },
                                                    ))
                                                    .on_mouse_up(
                                                        gpui::MouseButton::Left,
                                                        cx.listener(move |this, _, window, cx| {
                                                            mouse_type_focus.focus(window, cx);
                                                            this.toggle_form_data_value_kind(
                                                                index, cx,
                                                            );
                                                        }),
                                                    ),
                                            )
                                        })
                                        .child(
                                            // Value input - 可点击编辑
                                            div()
                                                .id(("body-form-value", index))
                                                .debug_selector(move || {
                                                    format!("body-form-value-{index}")
                                                })
                                                .flex_1()
                                                .px_2()
                                                .py_1()
                                                .bg(rgb(if entry_enabled { PANEL } else { PANEL_ALT }))
                                                .border_1()
                                                .border_color(
                                                    if self.editing_value_index == Some(index) {
                                                        rgb(INFO)
                                                    } else {
                                                        rgb(LINE)
                                                    },
                                                )
                                                .rounded_md()
                                                .font_family(FONT_MONO)
                                                .text_size(px(12.0))
                                                .cursor(if entry_is_file {
                                                    CursorStyle::PointingHand
                                                } else {
                                                    CursorStyle::IBeam
                                                })
                                                .when(
                                                    !entry_is_file
                                                        && self.editing_value_index == Some(index),
                                                    |div| {
                                                        div.child(FormTextElement {
                                                            input: cx.entity().clone(),
                                                            is_key: false,
                                                        })
                                                        .on_mouse_down(
                                                            MouseButton::Left,
                                                            cx.listener(
                                                                Self::form_value_on_mouse_down,
                                                            ),
                                                        )
                                                        .on_mouse_up(
                                                            MouseButton::Left,
                                                            cx.listener(
                                                                Self::form_value_on_mouse_up,
                                                            ),
                                                        )
                                                        .on_mouse_up_out(
                                                            MouseButton::Left,
                                                            cx.listener(
                                                                Self::form_value_on_mouse_up,
                                                            ),
                                                        )
                                                        .on_mouse_move(cx.listener(
                                                            Self::form_value_on_mouse_move,
                                                        ))
                                                    },
                                                )
                                                .when(
                                                    !entry_is_file
                                                        && self.editing_value_index != Some(index),
                                                    |div| {
                                                        div.when(entry_value.is_empty(), |div| {
                                                            div.text_color(rgb(SUBTEXT))
                                                                .child("Value")
                                                        })
                                                        .when(!entry_value.is_empty(), |div| {
                                                            div.text_color(rgb(if entry_enabled {
                                                                TEXT
                                                            } else {
                                                                SUBTEXT
                                                            }))
                                                                .child(entry_value.clone())
                                                        })
                                                        .on_mouse_up(
                                                            gpui::MouseButton::Left,
                                                            cx.listener(
                                                                move |this, _event, window, cx| {
                                                                    this.start_editing_value(
                                                                        index, cx,
                                                                    );
                                                                    this.focus_handle
                                                                        .focus(window, cx);
                                                                },
                                                            ),
                                                        )
                                                    },
                                                )
                                                .when(entry_is_file, |file_cell| {
                                                    file_cell
                                                    .debug_selector(move || {
                                                        format!("body-form-file-{index}")
                                                    })
                                                    .track_focus(&file_focus)
                                                    .key_context("KeyboardButton")
                                                    .role(Role::Button)
                                                    .aria_label(format!(
                                                        "Choose file for form row {}",
                                                        index + 1
                                                    ))
                                                    .when(file_focused, |control| {
                                                        control.border_2().border_color(rgb(INFO))
                                                    })
                                                    .flex()
                                                    .flex_col()
                                                    .justify_center()
                                                    .text_color(rgb(if !entry_enabled {
                                                        SUBTEXT
                                                    } else if entry_file_name.is_none() {
                                                        0x006c_757d
                                                    } else {
                                                        0x0021_2529
                                                    }))
                                                    .child(
                                                        div()
                                                            .debug_selector(move || {
                                                                format!(
                                                                    "body-form-file-name-{index}"
                                                                )
                                                            })
                                                            .child(
                                                                entry_file_name.clone().unwrap_or_else(
                                                                    || "Choose file…".to_string(),
                                                                ),
                                                            ),
                                                    )
                                                    .when_some(
                                                        entry_file_content_type.clone(),
                                                        |file, content_type| {
                                                            file.child(
                                                                div()
                                                                    .debug_selector(move || {
                                                                        format!(
                                                                            "body-form-file-metadata-{index}"
                                                                        )
                                                                    })
                                                                    .font_family(FONT_UI)
                                                                    .text_size(px(8.0))
                                                                    .text_color(rgb(SUBTEXT))
                                                                    .child(content_type),
                                                            )
                                                        },
                                                    )
                                                    .on_action(cx.listener(
                                                        move |this,
                                                              _: &ActivateControl,
                                                              window,
                                                              cx| {
                                                            this.choose_form_data_file(
                                                                index, window, cx,
                                                            );
                                                        },
                                                    ))
                                                    .on_mouse_up(
                                                        gpui::MouseButton::Left,
                                                        cx.listener(move |this, _, window, cx| {
                                                            mouse_file_focus.focus(window, cx);
                                                            this.choose_form_data_file(
                                                                index, window, cx,
                                                            );
                                                        }),
                                                    )
                                                })
                                                .when(!entry_is_file, |div| {
                                                    div.on_mouse_down(
                                                        MouseButton::Right,
                                                        cx.listener(
                                                            move |this, event, window, cx| {
                                                                this.open_form_context_menu(
                                                                    index, false, event, window, cx,
                                                                );
                                                            },
                                                        ),
                                                    )
                                                }),
                                        )
                                        .child(
                                            div()
                                                .debug_selector(move || {
                                                    format!("body-form-state-{index}")
                                                })
                                                .w(px(58.0))
                                                .h(px(30.0))
                                                .flex_none()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .child(
                                                    div()
                                                        .debug_selector(move || {
                                                            format!(
                                                                "body-form-{}-{index}",
                                                                if entry_enabled {
                                                                    "ready"
                                                                } else {
                                                                    "omitted"
                                                                }
                                                            )
                                                        })
                                                        .px_2()
                                                        .py_1()
                                                        .rounded_lg()
                                                        .bg(rgb(if entry_enabled {
                                                            OK_SOFT
                                                        } else {
                                                            PANEL_ALT
                                                        }))
                                                        .font_family(FONT_UI)
                                                        .font_weight(gpui::FontWeight::BOLD)
                                                        .text_size(px(7.0))
                                                        .text_color(rgb(if entry_enabled {
                                                            OK
                                                        } else {
                                                            MUTED
                                                        }))
                                                        .child(if entry_enabled {
                                                            "READY"
                                                        } else {
                                                            "OMITTED"
                                                        }),
                                                ),
                                        )
                                        .child(
                                            // Delete button
                                            div()
                                                .id(("body-form-delete", index))
                                                .debug_selector(move || {
                                                    format!("body-form-delete-{index}")
                                                })
                                                .track_focus(&delete_focus)
                                                .key_context("KeyboardButton")
                                                .role(Role::Button)
                                                .aria_label(format!(
                                                    "Delete form body row {}",
                                                    index + 1
                                                ))
                                                .w(px(44.0))
                                                .h(px(30.0))
                                                .flex_none()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .bg(rgb(if entry_enabled { PANEL } else { PANEL_ALT }))
                                                .text_color(rgb(SUBTEXT))
                                                .border_1()
                                                .border_color(rgb(LINE))
                                                .rounded_md()
                                                .cursor_pointer()
                                                .hover(|style| style.bg(rgb(ACCENT_SOFT)))
                                                .when(delete_focused, |control| {
                                                    control.border_2().border_color(rgb(INFO))
                                                })
                                                .child("×")
                                                .text_size(px(15.0))
                                                .on_action(cx.listener(
                                                    move |this,
                                                          _: &ActivateControl,
                                                          window,
                                                          cx| {
                                                        this.remove_form_data_entry(index, cx);
                                                        this.focus_after_row_removal(
                                                            index, window, cx,
                                                        );
                                                    },
                                                ))
                                                .on_mouse_up(
                                                    gpui::MouseButton::Left,
                                                    cx.listener(
                                                        move |this, _event, window, cx| {
                                                            mouse_delete_focus.focus(window, cx);
                                                            this.remove_form_data_entry(index, cx);
                                                            this.focus_after_row_removal(
                                                                index, window, cx,
                                                            );
                                                        },
                                                    ),
                                                ),
                                        )
                                },
                            )),
                    )
                    .when_some(form_data_scrollbar, |viewport, scrollbar| {
                        viewport.child(
                            div()
                                .debug_selector(|| "body-form-scrollbar".into())
                                .absolute()
                                .top(px(8.0))
                                .right(px(5.0))
                                .bottom(px(8.0))
                                .w(px(8.0))
                                .rounded_full()
                                .bg(rgb(PANEL_ALT))
                                .border_1()
                                .border_color(rgb(LINE))
                                .child(
                                    div()
                                        .debug_selector(|| "body-form-scrollbar-thumb".into())
                                        .absolute()
                                        .top(relative(scrollbar.thumb_top))
                                        .w_full()
                                        .h(relative(scrollbar.thumb_height))
                                        .rounded_full()
                                        .bg(rgb(INFO)),
                                ),
                        )
                    }),
            )
            .child(
                div()
                    .id("body-form-add-row")
                    .debug_selector(|| "body-form-add-row".into())
                    .track_focus(&self.add_row_focus_handle)
                    .key_context("KeyboardButton")
                    .role(Role::Button)
                    .aria_label("Add form body row")
                    .h(px(34.0))
                    .mx_2()
                    .mb_2()
                    .px_3()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .bg(rgb(INFO_SOFT))
                    .text_color(rgb(INFO))
                    .border_1()
                    .border_color(rgb(LINE))
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(PANEL_ALT)))
                    .when(self.add_row_focus_handle.is_focused(window), |button| {
                        button.border_2().border_color(rgb(INFO))
                    })
                    .child("+ Add form field")
                    .child(
                        div()
                            .debug_selector(|| "body-form-add-row-hint".into())
                            .text_color(rgb(SUBTEXT))
                            .child("one click = one row · no limit"),
                    )
                    .font_family(FONT_UI)
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_size(px(11.0))
                    .on_action(cx.listener(
                        |this, _: &ActivateControl, _window, cx| {
                            this.add_form_data_entry(cx);
                        },
                    ))
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _event, window, cx| {
                            this.add_row_focus_handle.focus(window, cx);
                            this.add_form_data_entry(cx);
                        }),
                    ),
            );
        editor.when_some(context_menu_position, |root, position| {
            root.child(edit_context_menu(
                position,
                "body-edit-menu",
                EDITABLE_ACTIONS,
                Self::handle_context_menu_action,
                window,
                cx,
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::FormBodyInput;
    use crate::ui::components::input::body_input::FormDataEntry;
    use gpui::{AppContext, TestAppContext};
    use std::path::PathBuf;

    #[gpui::test]
    fn row_insertion_removal_and_enabled_state_preserve_order(cx: &mut TestAppContext) {
        let input = cx.new(FormBodyInput::new);
        input.update(cx, |input, cx| {
            input.project_form_data_entries(
                vec![
                    FormDataEntry::text("duplicate", "first", true),
                    FormDataEntry::text("duplicate", "second", false),
                ],
                cx,
            );
            input.add_form_data_entry(cx);
            input.toggle_form_data_entry(1, cx);
        });

        input.read_with(cx, |input, _| {
            assert_eq!(
                input.entries(),
                &[
                    FormDataEntry::text("duplicate", "first", true),
                    FormDataEntry::text("duplicate", "second", true),
                    FormDataEntry::text("", "", true),
                ]
            );
        });

        input.update(cx, |input, cx| {
            input.remove_form_data_entry(0, cx);
            input.remove_form_data_entry(1, cx);
            input.remove_form_data_entry(0, cx);
        });
        assert_eq!(
            input.read_with(cx, |input, _| input.entries().to_vec()),
            vec![FormDataEntry::text("", "", true)]
        );
    }

    #[gpui::test]
    fn text_and_file_transitions_retain_typed_metadata_and_enabled_state(cx: &mut TestAppContext) {
        let input = cx.new(FormBodyInput::new);
        let path = PathBuf::from("/tmp/issue-101-upload.txt");
        input.update(cx, |input, cx| {
            input.set_form_data_allows_files(true, cx);
            input.project_form_data_entries(
                vec![FormDataEntry::file(
                    "upload",
                    path.clone(),
                    Some("renamed.txt".to_string()),
                    Some("text/plain".to_string()),
                    false,
                )],
                cx,
            );
            input.toggle_form_data_value_kind(0, cx);
        });

        assert_eq!(
            input.read_with(cx, |input, _| input.entries().to_vec()),
            vec![FormDataEntry::text(
                "upload",
                path.display().to_string(),
                false,
            )]
        );

        input.update(cx, |input, cx| input.toggle_form_data_value_kind(0, cx));
        input.read_with(cx, |input, _| {
            let entry = &input.entries()[0];
            assert_eq!(entry.key, "upload");
            assert!(!entry.enabled);
            assert!(entry.value.is_empty());
            let file = entry.file.as_ref().expect("row should switch back to file");
            assert!(file.path.as_os_str().is_empty());
            assert_eq!(file.file_name, None);
            assert_eq!(file.content_type, None);
        });
    }
}

use super::{FormDataEntry, FormDataFile};
use crate::ui::{
    components::{
        common::{
            keyboard::ActivateControl,
            scrollbar::{scrollbar_geometry, ScrollbarGeometry},
        },
        input::table_cell_input::{
            TableCellColumn, TableCellId, TableCellInput, TableCellInputEvent, TableCellTraversal,
            TableRowId,
        },
    },
    theme::{
        ACCENT_SOFT, FONT_UI, INFO, INFO_SOFT, LINE, MUTED, OK, OK_SOFT, PANEL, PANEL_ALT, SUBTEXT,
    },
};
use gpui::{
    div, prelude::FluentBuilder, px, relative, rgb, App, AppContext, Context, CursorStyle, Entity,
    EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Render,
    Role, ScrollHandle, StatefulInteractiveElement, Styled, Subscription, Window,
};
use std::path::PathBuf;

const FORM_DATA_ROW_HEIGHT: f32 = 38.0;
const FORM_DATA_ROW_GAP: f32 = 8.0;
const FORM_DATA_ROWS_PADDING: f32 = 16.0;
const FORM_DATA_MAX_VISIBLE_ROWS: usize = 6;

#[derive(Clone, Debug)]
pub(super) enum FormBodyInputEvent {
    Changed(Vec<FormDataEntry>),
}

struct FormRowEditor {
    row_id: TableRowId,
    key_input: Entity<TableCellInput>,
    value_input: Entity<TableCellInput>,
    _subscriptions: Vec<Subscription>,
}

impl FormRowEditor {
    fn cell(&self, column: TableCellColumn) -> Entity<TableCellInput> {
        match column {
            TableCellColumn::Key => self.key_input.clone(),
            TableCellColumn::Value => self.value_input.clone(),
        }
    }
}

enum PendingFormFocus {
    Cell(TableCellId),
    Control(FocusHandle),
    WindowPrevious,
}

/// Stateful URL-encoded/multipart adapter. Request-body values, enablement, file metadata, and
/// serialization remain outside the shared text core; each text cell delegates cursor, selection,
/// Unicode/IME, clipboard, and independent Undo/Redo state to TableCellInput.
pub(super) struct FormBodyInput {
    form_data_allows_files: bool,
    form_data_scroll: ScrollHandle,
    form_data_entries: Vec<FormDataEntry>,
    row_editors: Vec<FormRowEditor>,
    row_toggle_focus_handles: Vec<FocusHandle>,
    row_type_focus_handles: Vec<FocusHandle>,
    row_file_focus_handles: Vec<FocusHandle>,
    row_delete_focus_handles: Vec<FocusHandle>,
    add_row_focus_handle: FocusHandle,
    pending_focus: Option<PendingFormFocus>,
}

impl EventEmitter<FormBodyInputEvent> for FormBodyInput {}

impl Focusable for FormBodyInput {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.row_editors
            .first()
            .map(|row| row.key_input.read(cx).focus_handle(cx))
            .unwrap_or_else(|| self.add_row_focus_handle.clone())
    }
}

impl FormBodyInput {
    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        let entry = FormDataEntry::text("", "", true);
        let row_editor = Self::new_row_editor(&entry, cx);
        Self {
            form_data_allows_files: false,
            form_data_scroll: ScrollHandle::new(),
            form_data_entries: vec![entry],
            row_editors: vec![row_editor],
            row_toggle_focus_handles: vec![cx.focus_handle().tab_index(0).tab_stop(true)],
            row_type_focus_handles: vec![cx.focus_handle().tab_index(0).tab_stop(true)],
            row_file_focus_handles: vec![cx.focus_handle().tab_index(0).tab_stop(true)],
            row_delete_focus_handles: vec![cx.focus_handle().tab_index(0).tab_stop(true)],
            add_row_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            pending_focus: None,
        }
    }

    fn new_row_editor(entry: &FormDataEntry, cx: &mut Context<Self>) -> FormRowEditor {
        let row_id = TableRowId::next();
        let key_input = cx.new(|cx| {
            let mut input =
                TableCellInput::new(TableCellId::new(row_id, TableCellColumn::Key), "Key", cx)
                    .with_context_menu_id("body-edit-menu");
            input.project_content(entry.key.clone(), cx);
            input
        });
        let value_input = cx.new(|cx| {
            let mut input = TableCellInput::new(
                TableCellId::new(row_id, TableCellColumn::Value),
                "Value",
                cx,
            )
            .with_context_menu_id("body-edit-menu");
            input.project_content(entry.value.clone(), cx);
            input
        });
        let subscriptions = vec![
            cx.subscribe(&key_input, Self::on_cell_event),
            cx.subscribe(&value_input, Self::on_cell_event),
        ];
        FormRowEditor {
            row_id,
            key_input,
            value_input,
            _subscriptions: subscriptions,
        }
    }

    pub(super) fn entries(&self) -> &[FormDataEntry] {
        &self.form_data_entries
    }

    fn emit_form_data_changed(&self, cx: &mut Context<Self>) {
        cx.emit(FormBodyInputEvent::Changed(self.form_data_entries.clone()));
    }

    fn entry_index(&self, row_id: TableRowId) -> Option<usize> {
        self.row_editors
            .iter()
            .position(|editor| editor.row_id == row_id)
    }

    fn cell_entity(&self, cell: TableCellId) -> Option<Entity<TableCellInput>> {
        self.row_editors
            .iter()
            .find(|editor| editor.row_id == cell.row())
            .map(|editor| editor.cell(cell.column()))
    }

    fn push_control_focus_handles(&mut self, cx: &mut Context<Self>) {
        self.row_toggle_focus_handles
            .push(cx.focus_handle().tab_index(0).tab_stop(true));
        self.row_type_focus_handles
            .push(cx.focus_handle().tab_index(0).tab_stop(true));
        self.row_file_focus_handles
            .push(cx.focus_handle().tab_index(0).tab_stop(true));
        self.row_delete_focus_handles
            .push(cx.focus_handle().tab_index(0).tab_stop(true));
    }

    fn push_blank_entry(&mut self, cx: &mut Context<Self>) -> TableRowId {
        let entry = FormDataEntry::text("", "", true);
        let editor = Self::new_row_editor(&entry, cx);
        let row_id = editor.row_id;
        self.form_data_entries.push(entry);
        self.row_editors.push(editor);
        self.push_control_focus_handles(cx);
        row_id
    }

    fn rebuild_row_editors(&mut self, cx: &mut Context<Self>) {
        let entries = self.form_data_entries.clone();
        self.row_editors = entries
            .iter()
            .map(|entry| Self::new_row_editor(entry, cx))
            .collect();
        self.row_toggle_focus_handles = (0..entries.len())
            .map(|_| cx.focus_handle().tab_index(0).tab_stop(true))
            .collect();
        self.row_type_focus_handles = (0..entries.len())
            .map(|_| cx.focus_handle().tab_index(0).tab_stop(true))
            .collect();
        self.row_file_focus_handles = (0..entries.len())
            .map(|_| cx.focus_handle().tab_index(0).tab_stop(true))
            .collect();
        self.row_delete_focus_handles = (0..entries.len())
            .map(|_| cx.focus_handle().tab_index(0).tab_stop(true))
            .collect();
        self.pending_focus = None;
    }

    fn editor_text_matches(&self, entries: &[FormDataEntry], cx: &App) -> bool {
        self.row_editors.len() == entries.len()
            && self.row_editors.iter().zip(entries).all(|(editor, entry)| {
                editor.key_input.read(cx).content() == entry.key
                    && editor.value_input.read(cx).content() == entry.value
            })
    }

    pub(super) fn set_form_data_allows_files(
        &mut self,
        allows_files: bool,
        cx: &mut Context<Self>,
    ) {
        if self.form_data_allows_files == allows_files {
            return;
        }
        self.form_data_allows_files = allows_files;
        if !allows_files {
            let mut projections = Vec::new();
            for (index, entry) in self.form_data_entries.iter_mut().enumerate() {
                if let Some(file) = entry.file.take() {
                    entry.value = file.path.display().to_string();
                    projections.push((
                        self.row_editors[index].value_input.clone(),
                        entry.value.clone(),
                    ));
                }
            }
            for (input, value) in projections {
                input.update(cx, |input, cx| input.project_content(value, cx));
            }
        }
        cx.notify();
    }

    pub(super) fn add_form_data_entry(&mut self, cx: &mut Context<Self>) {
        self.push_blank_entry(cx);
        self.form_data_scroll.scroll_to_bottom();
        self.emit_form_data_changed(cx);
        cx.notify();
    }

    pub(super) fn remove_form_data_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.form_data_entries.len() {
            return;
        }
        self.form_data_entries.remove(index);
        self.row_editors.remove(index);
        self.row_toggle_focus_handles.remove(index);
        self.row_type_focus_handles.remove(index);
        self.row_file_focus_handles.remove(index);
        self.row_delete_focus_handles.remove(index);
        if self.form_data_entries.is_empty() {
            self.push_blank_entry(cx);
        }
        self.emit_form_data_changed(cx);
        cx.notify();
    }

    pub(super) fn toggle_form_data_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(entry) = self.form_data_entries.get_mut(index) {
            entry.enabled = !entry.enabled;
            self.emit_form_data_changed(cx);
            cx.notify();
        }
    }

    fn toggle_form_data_value_kind(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.form_data_allows_files || index >= self.form_data_entries.len() {
            return;
        }
        let value = {
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
            entry.value.clone()
        };
        self.row_editors[index]
            .value_input
            .update(cx, |input, cx| input.project_content(value, cx));
        self.emit_form_data_changed(cx);
        cx.notify();
    }

    fn choose_form_data_file(
        &mut self,
        row_id: TableRowId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.entry_index(row_id) else {
            return;
        };
        if !self.form_data_allows_files || self.form_data_entries[index].file.is_none() {
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
                let Some(index) = this.entry_index(row_id) else {
                    return;
                };
                let Some(entry) = this.form_data_entries.get_mut(index) else {
                    return;
                };
                if entry.file.is_none() {
                    return;
                }
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
            });
        })
        .detach();
    }

    pub(super) fn set_form_data_entries(
        &mut self,
        mut entries: Vec<FormDataEntry>,
        cx: &mut Context<Self>,
    ) {
        if entries.is_empty() {
            entries.push(FormDataEntry::text("", "", true));
        }
        self.form_data_entries = entries;
        self.rebuild_row_editors(cx);
        self.emit_form_data_changed(cx);
        cx.notify();
    }

    /// Project domain rows without emitting a user edit. Stable cell entities are retained when
    /// the same logical table is projected again; callers force a rebind when changing requests.
    pub(super) fn project_form_data_entries(
        &mut self,
        entries: Vec<FormDataEntry>,
        cx: &mut Context<Self>,
    ) {
        self.project_form_data_entries_with_rebind(entries, false, cx);
    }

    pub(super) fn project_form_data_entries_with_rebind(
        &mut self,
        mut entries: Vec<FormDataEntry>,
        force_rebind: bool,
        cx: &mut Context<Self>,
    ) {
        if entries.is_empty() {
            entries.push(FormDataEntry::text("", "", true));
        }
        let text_matches = self.editor_text_matches(&entries, cx);
        self.form_data_entries = entries;
        if force_rebind || !text_matches {
            self.rebuild_row_editors(cx);
        }
        cx.notify();
    }

    pub(super) fn start_editing_key(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(editor) = self.row_editors.get(index) {
            self.pending_focus = Some(PendingFormFocus::Cell(TableCellId::new(
                editor.row_id,
                TableCellColumn::Key,
            )));
            cx.notify();
        }
    }

    pub(super) fn start_editing_value(&mut self, index: usize, cx: &mut Context<Self>) {
        if self
            .form_data_entries
            .get(index)
            .is_some_and(|entry| entry.file.is_none())
        {
            let row_id = self.row_editors[index].row_id;
            self.pending_focus = Some(PendingFormFocus::Cell(TableCellId::new(
                row_id,
                TableCellColumn::Value,
            )));
            cx.notify();
        }
    }

    pub(super) fn finish_editing(&mut self, _cx: &mut Context<Self>) {}

    pub(super) fn finish_key_editing_only(&mut self, _cx: &mut Context<Self>) {}

    pub(super) fn finish_value_editing_only(&mut self, _cx: &mut Context<Self>) {}

    pub(super) fn cancel_editing(&mut self, _cx: &mut Context<Self>) {}

    pub(super) fn clear(&mut self, cx: &mut Context<Self>) {
        if self.form_data_entries == [FormDataEntry::text("", "", true)] {
            return;
        }
        self.form_data_entries = vec![FormDataEntry::text("", "", true)];
        self.rebuild_row_editors(cx);
        self.emit_form_data_changed(cx);
        cx.notify();
    }

    fn on_cell_event(
        &mut self,
        _input: Entity<TableCellInput>,
        event: &TableCellInputEvent,
        cx: &mut Context<Self>,
    ) {
        let cell = match event {
            TableCellInputEvent::ValueChanged { cell, .. }
            | TableCellInputEvent::SubmitRequested { cell }
            | TableCellInputEvent::TraversalRequested { cell, .. } => *cell,
        };
        let Some(index) = self.entry_index(cell.row()) else {
            return;
        };

        match event {
            TableCellInputEvent::ValueChanged { value, .. } => {
                let entry = &mut self.form_data_entries[index];
                let changed = match cell.column() {
                    TableCellColumn::Key if entry.key != *value => {
                        entry.key.clone_from(value);
                        true
                    }
                    TableCellColumn::Value if entry.file.is_none() && entry.value != *value => {
                        entry.value.clone_from(value);
                        true
                    }
                    _ => false,
                };
                if changed {
                    self.emit_form_data_changed(cx);
                }
            }
            TableCellInputEvent::SubmitRequested { .. } => {}
            TableCellInputEvent::TraversalRequested { direction, .. } => {
                self.queue_traversal(cell, index, *direction, cx);
            }
        }
    }

    fn queue_traversal(
        &mut self,
        cell: TableCellId,
        index: usize,
        direction: TableCellTraversal,
        cx: &mut Context<Self>,
    ) {
        let target = match (cell.column(), direction) {
            (TableCellColumn::Key, TableCellTraversal::Forward) => {
                if self.form_data_entries[index].file.is_some() {
                    PendingFormFocus::Control(self.row_type_focus_handles[index].clone())
                } else {
                    PendingFormFocus::Cell(TableCellId::new(cell.row(), TableCellColumn::Value))
                }
            }
            (TableCellColumn::Value, TableCellTraversal::Backward) => {
                PendingFormFocus::Cell(TableCellId::new(cell.row(), TableCellColumn::Key))
            }
            (TableCellColumn::Value, TableCellTraversal::Forward) => {
                let next_row_id = if index + 1 < self.row_editors.len() {
                    self.row_editors[index + 1].row_id
                } else {
                    let row_id = self.push_blank_entry(cx);
                    self.form_data_scroll.scroll_to_bottom();
                    self.emit_form_data_changed(cx);
                    row_id
                };
                PendingFormFocus::Cell(TableCellId::new(next_row_id, TableCellColumn::Key))
            }
            (TableCellColumn::Key, TableCellTraversal::Backward) if index == 0 => {
                PendingFormFocus::WindowPrevious
            }
            (TableCellColumn::Key, TableCellTraversal::Backward) => {
                let previous = index - 1;
                if self.form_data_entries[previous].file.is_some() {
                    PendingFormFocus::Control(self.row_file_focus_handles[previous].clone())
                } else {
                    PendingFormFocus::Cell(TableCellId::new(
                        self.row_editors[previous].row_id,
                        TableCellColumn::Value,
                    ))
                }
            }
        };
        self.pending_focus = Some(target);
        cx.notify();
    }

    fn apply_pending_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.pending_focus.take() else {
            return;
        };
        match target {
            PendingFormFocus::Cell(cell) => {
                if let Some(input) = self.cell_entity(cell) {
                    input.read(cx).focus_handle(cx).focus(window, cx);
                }
            }
            PendingFormFocus::Control(focus) => focus.focus(window, cx),
            PendingFormFocus::WindowPrevious => window.focus_prev(cx),
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
}

fn form_data_scrollbar_geometry(
    row_count: usize,
    offset_y: f32,
    max_offset_y: f32,
) -> Option<ScrollbarGeometry> {
    if row_count <= FORM_DATA_MAX_VISIBLE_ROWS {
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
        self.apply_pending_focus(window, cx);
        let form_data_entries = self.form_data_entries.clone();
        let form_data_allows_files = self.form_data_allows_files;
        let row_cells = self
            .row_editors
            .iter()
            .map(|row| (row.row_id, row.key_input.clone(), row.value_input.clone()))
            .collect::<Vec<_>>();
        let row_toggle_focus_handles = self.row_toggle_focus_handles.clone();
        let row_type_focus_handles = self.row_type_focus_handles.clone();
        let row_file_focus_handles = self.row_file_focus_handles.clone();
        let row_delete_focus_handles = self.row_delete_focus_handles.clone();
        let form_data_scrollbar = form_data_scrollbar_geometry(
            form_data_entries.len(),
            self.form_data_scroll.offset().y.as_f32(),
            self.form_data_scroll.max_offset().y.as_f32(),
        );

        div()
            .debug_selector(|| "body-form-editor".into())
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(LINE))
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
                            .children(
                                form_data_entries
                                    .iter()
                                    .zip(row_cells)
                                    .enumerate()
                                    .map(|(index, (entry, (row_id, key_input, value_input)))| {
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
                                        let toggle_focus =
                                            row_toggle_focus_handles[index].clone();
                                        let mouse_toggle_focus = toggle_focus.clone();
                                        let toggle_focused = toggle_focus.is_focused(window);
                                        let type_focus = row_type_focus_handles[index].clone();
                                        let mouse_type_focus = type_focus.clone();
                                        let type_focused = type_focus.is_focused(window);
                                        let file_focus = row_file_focus_handles[index].clone();
                                        let mouse_file_focus = file_focus.clone();
                                        let file_focused = file_focus.is_focused(window);
                                        let delete_focus =
                                            row_delete_focus_handles[index].clone();
                                        let mouse_delete_focus = delete_focus.clone();
                                        let delete_focused = delete_focus.is_focused(window);

                                        div()
                                            .debug_selector(move || {
                                                format!("body-form-row-{index}")
                                            })
                                            .h(px(FORM_DATA_ROW_HEIGHT))
                                            .flex_none()
                                            .flex()
                                            .gap_2()
                                            .items_center()
                                            .bg(rgb(if entry_enabled {
                                                PANEL
                                            } else {
                                                PANEL_ALT
                                            }))
                                            .child(
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
                                                        if entry_enabled {
                                                            "Disable"
                                                        } else {
                                                            "Enable"
                                                        },
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
                                                    .bg(rgb(if entry_enabled {
                                                        INFO
                                                    } else {
                                                        PANEL
                                                    }))
                                                    .text_color(rgb(PANEL))
                                                    .font_family(FONT_UI)
                                                    .font_weight(gpui::FontWeight::BOLD)
                                                    .text_size(px(10.0))
                                                    .cursor_pointer()
                                                    .when(toggle_focused, |control| {
                                                        control
                                                            .border_2()
                                                            .border_color(rgb(ACCENT_SOFT))
                                                    })
                                                    .child(if entry_enabled { "✓" } else { "" })
                                                    .on_action(cx.listener(
                                                        move |this,
                                                              _: &ActivateControl,
                                                              _,
                                                              cx| {
                                                            this.toggle_form_data_entry(
                                                                index, cx,
                                                            );
                                                        },
                                                    ))
                                                    .on_mouse_up(
                                                        gpui::MouseButton::Left,
                                                        cx.listener(
                                                            move |this, _, window, cx| {
                                                                mouse_toggle_focus
                                                                    .focus(window, cx);
                                                                this.toggle_form_data_entry(
                                                                    index, cx,
                                                                );
                                                            },
                                                        ),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .debug_selector(move || {
                                                        format!("body-form-key-{index}")
                                                    })
                                                    .h_full()
                                                    .min_w_0()
                                                    .flex_1()
                                                    .flex()
                                                    .child(key_input),
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
                                                            if entry_is_file {
                                                                "text"
                                                            } else {
                                                                "file"
                                                            },
                                                            index + 1
                                                        ))
                                                        .w(px(64.0))
                                                        .h(px(32.0))
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
                                                        .font_weight(
                                                            gpui::FontWeight::SEMIBOLD,
                                                        )
                                                        .text_color(rgb(if !entry_enabled {
                                                            MUTED
                                                        } else if entry_is_file {
                                                            0x001d_4ed8
                                                        } else {
                                                            0x0047_5569
                                                        }))
                                                        .cursor_pointer()
                                                        .when(type_focused, |control| {
                                                            control
                                                                .border_2()
                                                                .border_color(rgb(INFO))
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
                                                            cx.listener(
                                                                move |this, _, window, cx| {
                                                                    mouse_type_focus
                                                                        .focus(window, cx);
                                                                    this.toggle_form_data_value_kind(
                                                                        index, cx,
                                                                    );
                                                                },
                                                            ),
                                                        ),
                                                )
                                            })
                                            .child(
                                                div()
                                                    .id(("body-form-value", index))
                                                    .debug_selector(move || {
                                                        format!("body-form-value-{index}")
                                                    })
                                                    .h_full()
                                                    .min_w_0()
                                                    .flex_1()
                                                    .flex()
                                                    .when(!entry_is_file, |cell| {
                                                        cell.child(value_input)
                                                    })
                                                    .when(entry_is_file, |file_cell| {
                                                        file_cell
                                                            .debug_selector(move || {
                                                                format!(
                                                                    "body-form-file-{index}"
                                                                )
                                                            })
                                                            .track_focus(&file_focus)
                                                            .key_context("KeyboardButton")
                                                            .role(Role::Button)
                                                            .aria_label(format!(
                                                                "Choose file for form row {}",
                                                                index + 1
                                                            ))
                                                            .px_2()
                                                            .border_1()
                                                            .border_color(rgb(LINE))
                                                            .rounded_md()
                                                            .cursor(CursorStyle::PointingHand)
                                                            .when(file_focused, |control| {
                                                                control
                                                                    .border_2()
                                                                    .border_color(rgb(INFO))
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
                                                                        entry_file_name
                                                                            .clone()
                                                                            .unwrap_or_else(|| {
                                                                                "Choose file…"
                                                                                    .to_string()
                                                                            }),
                                                                    ),
                                                            )
                                                            .when_some(
                                                                entry_file_content_type.clone(),
                                                                |file, content_type| {
                                                                    file.child(
                                                                        div()
                                                                            .debug_selector(
                                                                                move || {
                                                                                    format!(
                                                                                        "body-form-file-metadata-{index}"
                                                                                    )
                                                                                },
                                                                            )
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
                                                                        row_id, window, cx,
                                                                    );
                                                                },
                                                            ))
                                                            .on_mouse_up(
                                                                gpui::MouseButton::Left,
                                                                cx.listener(
                                                                    move |this, _, window, cx| {
                                                                        mouse_file_focus
                                                                            .focus(window, cx);
                                                                        this.choose_form_data_file(
                                                                            row_id, window, cx,
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
                                                            .font_weight(
                                                                gpui::FontWeight::BOLD,
                                                            )
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
                                                    .bg(rgb(if entry_enabled {
                                                        PANEL
                                                    } else {
                                                        PANEL_ALT
                                                    }))
                                                    .text_color(rgb(SUBTEXT))
                                                    .border_1()
                                                    .border_color(rgb(LINE))
                                                    .rounded_md()
                                                    .cursor_pointer()
                                                    .hover(|style| {
                                                        style.bg(rgb(ACCENT_SOFT))
                                                    })
                                                    .when(delete_focused, |control| {
                                                        control
                                                            .border_2()
                                                            .border_color(rgb(INFO))
                                                    })
                                                    .child("×")
                                                    .text_size(px(15.0))
                                                    .on_action(cx.listener(
                                                        move |this,
                                                              _: &ActivateControl,
                                                              window,
                                                              cx| {
                                                            this.remove_form_data_entry(
                                                                index, cx,
                                                            );
                                                            this.focus_after_row_removal(
                                                                index, window, cx,
                                                            );
                                                        },
                                                    ))
                                                    .on_mouse_up(
                                                        gpui::MouseButton::Left,
                                                        cx.listener(
                                                            move |this, _, window, cx| {
                                                                mouse_delete_focus
                                                                    .focus(window, cx);
                                                                this.remove_form_data_entry(
                                                                    index, cx,
                                                                );
                                                                this.focus_after_row_removal(
                                                                    index, window, cx,
                                                                );
                                                            },
                                                        ),
                                                    ),
                                            )
                                    }),
                            ),
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
                                        .debug_selector(|| {
                                            "body-form-scrollbar-thumb".into()
                                        })
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
                        cx.listener(|this, _, window, cx| {
                            this.add_row_focus_handle.focus(window, cx);
                            this.add_form_data_entry(cx);
                        }),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::FormBodyInput;
    use crate::ui::components::input::{
        body_input::FormDataEntry,
        table_cell_input::{TableCellColumn, TableCellId, TableCellInputEvent},
    };
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
    fn duplicate_rows_keep_identity_across_append_and_neighbor_removal(cx: &mut TestAppContext) {
        let input = cx.new(FormBodyInput::new);
        input.update(cx, |input, cx| {
            input.project_form_data_entries(
                vec![
                    FormDataEntry::text("duplicate", "first", true),
                    FormDataEntry::text("duplicate", "second", false),
                ],
                cx,
            );
        });
        let ids = input.read_with(cx, |input, _| {
            input
                .row_editors
                .iter()
                .map(|row| row.row_id)
                .collect::<Vec<_>>()
        });

        input.update(cx, |input, cx| {
            input.add_form_data_entry(cx);
            assert_eq!(input.row_editors[0].row_id, ids[0]);
            assert_eq!(input.row_editors[1].row_id, ids[1]);
            input.remove_form_data_entry(0, cx);
            assert_eq!(input.row_editors[0].row_id, ids[1]);
        });
    }

    #[gpui::test]
    fn same_tab_projection_retains_cells_but_request_rebind_resets_them(cx: &mut TestAppContext) {
        let input = cx.new(FormBodyInput::new);
        let entries = vec![FormDataEntry::text("same", "value", true)];
        input.update(cx, |input, cx| {
            input.project_form_data_entries(entries.clone(), cx);
        });
        let original = input.read_with(cx, |input, _| input.row_editors[0].row_id);

        input.update(cx, |input, cx| {
            input.project_form_data_entries(entries.clone(), cx);
            assert_eq!(input.row_editors[0].row_id, original);
            input.project_form_data_entries_with_rebind(entries, true, cx);
            assert_ne!(input.row_editors[0].row_id, original);
        });
    }

    #[gpui::test]
    fn stable_cell_event_updates_the_same_logical_row_after_deletion(cx: &mut TestAppContext) {
        let input = cx.new(FormBodyInput::new);
        input.update(cx, |input, cx| {
            input.project_form_data_entries(
                vec![
                    FormDataEntry::text("first", "one", true),
                    FormDataEntry::text("second", "two", true),
                ],
                cx,
            );
            let second = input.row_editors[1].row_id;
            input.remove_form_data_entry(0, cx);
            input.on_cell_event(
                input.row_editors[0].key_input.clone(),
                &TableCellInputEvent::ValueChanged {
                    cell: TableCellId::new(second, TableCellColumn::Key),
                    value: "still-second".to_string(),
                },
                cx,
            );
        });
        assert_eq!(
            input.read_with(cx, |input, _| input.entries()[0].key.clone()),
            "still-second"
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

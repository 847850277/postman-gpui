use super::{FormDataEntry, FormDataFile};
use crate::ui::components::input::table_cell_input::{
    TableCellColumn, TableCellId, TableCellInput, TableCellInputEvent, TableCellTraversal,
    TableRowId,
};
use gpui::{
    px, App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, ScrollHandle,
    Subscription, Window,
};
use std::path::PathBuf;

mod layout;

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
    viewport_width: gpui::Pixels,
    has_overflow: bool,
    focused_row: Option<TableRowId>,
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
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let entry = FormDataEntry::text("", "", true);
        let row_editor = Self::new_row_editor(&entry, window, cx);
        Self {
            form_data_allows_files: false,
            form_data_scroll: ScrollHandle::new(),
            viewport_width: px(0.),
            has_overflow: false,
            focused_row: None,
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

    fn new_row_editor(
        entry: &FormDataEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> FormRowEditor {
        let row_id = TableRowId::next();
        let key_input = cx.new(|cx| {
            let mut input = TableCellInput::new(
                TableCellId::new(row_id, TableCellColumn::Key),
                "Key",
                window,
                cx,
            );
            input.project_content(entry.key.clone(), window, cx);
            input
        });
        let value_input = cx.new(|cx| {
            let mut input = TableCellInput::new(
                TableCellId::new(row_id, TableCellColumn::Value),
                "Value",
                window,
                cx,
            );
            input.project_content(entry.value.clone(), window, cx);
            input
        });
        let subscriptions = vec![
            cx.subscribe_in(&key_input, window, Self::on_cell_event),
            cx.subscribe_in(&value_input, window, Self::on_cell_event),
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

    fn push_blank_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) -> TableRowId {
        let entry = FormDataEntry::text("", "", true);
        let editor = Self::new_row_editor(&entry, window, cx);
        let row_id = editor.row_id;
        self.form_data_entries.push(entry);
        self.row_editors.push(editor);
        self.push_control_focus_handles(cx);
        row_id
    }

    fn rebuild_row_editors(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let entries = self.form_data_entries.clone();
        self.row_editors = entries
            .iter()
            .map(|entry| Self::new_row_editor(entry, window, cx))
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
        self.focused_row = None;
    }

    fn editor_text_matches(&self, entries: &[FormDataEntry], cx: &App) -> bool {
        self.row_editors.len() == entries.len()
            && self.row_editors.iter().zip(entries).all(|(editor, entry)| {
                editor.key_input.read(cx).content(cx) == entry.key
                    && editor.value_input.read(cx).content(cx) == entry.value
            })
    }

    pub(super) fn set_form_data_allows_files(
        &mut self,
        allows_files: bool,
        window: &mut Window,
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
                input.update(cx, |input, cx| input.project_content(value, window, cx));
            }
        }
        cx.notify();
    }

    pub(super) fn add_form_data_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let row_id = self.push_blank_entry(window, cx);
        self.pending_focus = Some(PendingFormFocus::Cell(TableCellId::new(
            row_id,
            TableCellColumn::Key,
        )));
        self.form_data_scroll.scroll_to_bottom();
        self.emit_form_data_changed(cx);
        cx.notify();
    }

    pub(super) fn remove_form_data_entry(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
            self.push_blank_entry(window, cx);
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

    fn toggle_form_data_value_kind(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
            .update(cx, |input, cx| input.project_content(value, window, cx));
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

    #[cfg(test)]
    pub(super) fn set_form_data_entries(
        &mut self,
        mut entries: Vec<FormDataEntry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if entries.is_empty() {
            entries.push(FormDataEntry::text("", "", true));
        }
        self.form_data_entries = entries;
        self.rebuild_row_editors(window, cx);
        self.emit_form_data_changed(cx);
        cx.notify();
    }

    /// Project domain rows without emitting a user edit. Stable cell entities are retained when
    /// the same logical table is projected again; callers force a rebind when changing requests.
    pub(super) fn project_form_data_entries(
        &mut self,
        entries: Vec<FormDataEntry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_form_data_entries_with_rebind(entries, false, window, cx);
    }

    pub(super) fn project_form_data_entries_with_rebind(
        &mut self,
        mut entries: Vec<FormDataEntry>,
        force_rebind: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if entries.is_empty() {
            entries.push(FormDataEntry::text("", "", true));
        }
        let text_matches = self.editor_text_matches(&entries, cx);
        self.form_data_entries = entries;
        if force_rebind || !text_matches {
            self.rebuild_row_editors(window, cx);
        }
        cx.notify();
    }

    #[cfg(test)]
    pub(super) fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.form_data_entries == [FormDataEntry::text("", "", true)] {
            return;
        }
        self.form_data_entries = vec![FormDataEntry::text("", "", true)];
        self.rebuild_row_editors(window, cx);
        self.emit_form_data_changed(cx);
        cx.notify();
    }

    fn on_cell_event(
        &mut self,
        _input: &Entity<TableCellInput>,
        event: &TableCellInputEvent,
        window: &mut Window,
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
                self.queue_traversal(cell, index, *direction, window, cx);
            }
        }
    }

    fn queue_traversal(
        &mut self,
        cell: TableCellId,
        index: usize,
        direction: TableCellTraversal,
        window: &mut Window,
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
                    let row_id = self.push_blank_entry(window, cx);
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

#[cfg(test)]
mod tests {
    use super::FormBodyInput;
    use crate::ui::components::input::body_input::FormDataEntry;
    use gpui::{Focusable, TestAppContext};
    use std::path::PathBuf;

    #[gpui::test]
    fn row_insertion_removal_and_enabled_state_preserve_order(cx: &mut TestAppContext) {
        cx.update(crate::ui::kit::init);
        let (input, cx) = cx.add_window_view(FormBodyInput::new);
        input.update_in(cx, |input, window, cx| {
            input.project_form_data_entries(
                vec![
                    FormDataEntry::text("duplicate", "first", true),
                    FormDataEntry::text("duplicate", "second", false),
                ],
                window,
                cx,
            );
            input.add_form_data_entry(window, cx);
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

        input.update_in(cx, |input, window, cx| {
            input.remove_form_data_entry(0, window, cx);
            input.remove_form_data_entry(1, window, cx);
            input.remove_form_data_entry(0, window, cx);
        });
        assert_eq!(
            input.read_with(cx, |input, _| input.entries().to_vec()),
            vec![FormDataEntry::text("", "", true)]
        );
    }

    #[gpui::test]
    fn duplicate_rows_keep_identity_across_append_and_neighbor_removal(cx: &mut TestAppContext) {
        cx.update(crate::ui::kit::init);
        let (input, cx) = cx.add_window_view(FormBodyInput::new);
        input.update_in(cx, |input, window, cx| {
            input.project_form_data_entries(
                vec![
                    FormDataEntry::text("duplicate", "first", true),
                    FormDataEntry::text("duplicate", "second", false),
                ],
                window,
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

        input.update_in(cx, |input, window, cx| {
            input.add_form_data_entry(window, cx);
            assert_eq!(input.row_editors[0].row_id, ids[0]);
            assert_eq!(input.row_editors[1].row_id, ids[1]);
            input.remove_form_data_entry(0, window, cx);
            assert_eq!(input.row_editors[0].row_id, ids[1]);
        });
    }

    #[gpui::test]
    fn same_tab_projection_retains_cells_but_request_rebind_resets_them(cx: &mut TestAppContext) {
        cx.update(crate::ui::kit::init);
        let (input, cx) = cx.add_window_view(FormBodyInput::new);
        let entries = vec![FormDataEntry::text("same", "value", true)];
        input.update_in(cx, |input, window, cx| {
            input.project_form_data_entries(entries.clone(), window, cx);
        });
        let original = input.read_with(cx, |input, _| input.row_editors[0].row_id);

        input.update_in(cx, |input, window, cx| {
            input.project_form_data_entries(entries.clone(), window, cx);
            assert_eq!(input.row_editors[0].row_id, original);
            input.project_form_data_entries_with_rebind(entries, true, window, cx);
            assert_ne!(input.row_editors[0].row_id, original);
        });
    }

    #[gpui::test]
    fn stable_cell_event_updates_the_same_logical_row_after_deletion(cx: &mut TestAppContext) {
        cx.update(crate::ui::kit::init);
        let (input, visual) = cx.add_window_view(FormBodyInput::new);
        let second = input.update_in(visual, |input, window, cx| {
            input.project_form_data_entries(
                vec![
                    FormDataEntry::text("first", "one", true),
                    FormDataEntry::text("second", "two", true),
                ],
                window,
                cx,
            );
            let second = input.row_editors[1].key_input.clone();
            input.remove_form_data_entry(0, window, cx);
            second.read(cx).focus_handle(cx).focus(window, cx);
            second
        });
        visual.simulate_keystrokes("ctrl-a");
        visual.simulate_input("still-second");
        assert_eq!(
            input.read_with(visual, |input, _| input.entries()[0].key.clone()),
            "still-second"
        );
        assert_eq!(
            input.read_with(visual, |input, _| input.row_editors[0]
                .key_input
                .entity_id()),
            second.entity_id()
        );
    }

    #[gpui::test]
    fn kit_cell_tab_traverses_and_appends_form_rows(cx: &mut TestAppContext) {
        cx.update(crate::ui::kit::init);
        let (input, visual) = cx.add_window_view(FormBodyInput::new);
        input.update_in(visual, |input, window, cx| {
            input.focus_handle(cx).focus(window, cx)
        });
        visual.simulate_input("duplicate");
        visual.simulate_keystrokes("tab");
        visual.simulate_input("first😀");
        visual.simulate_keystrokes("tab");
        visual.simulate_input("duplicate");
        visual.simulate_keystrokes("tab");
        visual.simulate_input("second中");
        visual.simulate_keystrokes("shift-tab");
        input.update_in(visual, |input, window, cx| {
            assert_eq!(
                input.entries(),
                &[
                    FormDataEntry::text("duplicate", "first😀", true),
                    FormDataEntry::text("duplicate", "second中", true),
                ]
            );
            assert!(input.row_editors[1]
                .key_input
                .read(cx)
                .focus_handle(cx)
                .is_focused(window));
        });
    }

    #[gpui::test]
    fn text_and_file_transitions_retain_typed_metadata_and_enabled_state(cx: &mut TestAppContext) {
        cx.update(crate::ui::kit::init);
        let (input, cx) = cx.add_window_view(FormBodyInput::new);
        let path = PathBuf::from("/tmp/issue-101-upload.txt");
        input.update_in(cx, |input, window, cx| {
            input.set_form_data_allows_files(true, window, cx);
            input.project_form_data_entries(
                vec![FormDataEntry::file(
                    "upload",
                    path.clone(),
                    Some("renamed.txt".to_string()),
                    Some("text/plain".to_string()),
                    false,
                )],
                window,
                cx,
            );
            input.toggle_form_data_value_kind(0, window, cx);
        });

        assert_eq!(
            input.read_with(cx, |input, _| input.entries().to_vec()),
            vec![FormDataEntry::text(
                "upload",
                path.display().to_string(),
                false,
            )]
        );

        input.update_in(cx, |input, window, cx| {
            input.toggle_form_data_value_kind(0, window, cx)
        });
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

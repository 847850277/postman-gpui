use super::super::layout::RequestPanelLayout;
use crate::{
    app::{KeyValueRow, RequestPane, RequestTabId, RequestViewModel, WorkspaceViewModel},
    ui::{
        components::input::header_input::{HeaderInput, HeaderInputEvent},
        components::input::table_cell_input::{
            TableCellColumn, TableCellId, TableCellInput, TableCellInputEvent, TableCellTraversal,
            TableRowId,
        },
        theme::{FONT_MONO, LINE, MUTED, PANEL_ALT, TEXT},
    },
};
use gpui::{
    div, prelude::FluentBuilder, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, ScrollHandle,
    StatefulInteractiveElement, Styled, Subscription, Window,
};
use gpui_kit::{
    base::ElementExt,
    component::scroll::{Scrollbar, ScrollbarMode},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app::postman_app::request_workspace) enum KeyValueRowsKind {
    Params,
    Headers,
}

#[derive(Clone, Debug)]
pub(in crate::app::postman_app::request_workspace) enum PersistentRowEditorEvent {
    Cell(TableCellInputEvent),
    Description { row: TableRowId, value: String },
}

/// Editing buffers for one persistent Params or Headers row. Business values remain in the
/// ViewModel; these entities only retain cursor and selection state.
pub(in crate::app::postman_app::request_workspace) struct PersistentRowEditor {
    kind: KeyValueRowsKind,
    index: usize,
    row_id: TableRowId,
    key_input: Entity<TableCellInput>,
    value_input: Entity<TableCellInput>,
    description_input: Entity<HeaderInput>,
    description: String,
    show_description: bool,
    _subscriptions: Vec<Subscription>,
}

impl PersistentRowEditor {
    fn new(kind: KeyValueRowsKind, index: usize, row: KeyValueRow, cx: &mut Context<Self>) -> Self {
        let KeyValueRow {
            key,
            value,
            description,
            ..
        } = row;
        let row_id = TableRowId::next();
        let (key_placeholder, value_placeholder) = match kind {
            KeyValueRowsKind::Params => ("Key", "Value"),
            KeyValueRowsKind::Headers => ("Header name", "Header value"),
        };
        let key_input = cx.new(|cx| {
            let mut input = TableCellInput::new(
                TableCellId::new(row_id, TableCellColumn::Key),
                key_placeholder,
                cx,
            );
            input.project_content(key, cx);
            input
        });
        let value_input = cx.new(|cx| {
            let mut input = TableCellInput::new(
                TableCellId::new(row_id, TableCellColumn::Value),
                value_placeholder,
                cx,
            );
            input.project_content(value, cx);
            input
        });
        let description_input = cx.new(|cx| {
            let mut input = HeaderInput::new(cx)
                .with_placeholder("Description")
                .with_font_family(crate::ui::theme::FONT_UI)
                .with_embedded_chrome(true);
            input.project_content(description.clone(), cx);
            input
        });
        let subscriptions = vec![
            cx.subscribe(&key_input, Self::on_cell_event),
            cx.subscribe(&value_input, Self::on_cell_event),
            cx.subscribe(
                &description_input,
                |this, _, event: &HeaderInputEvent, cx| {
                    if let HeaderInputEvent::ValueChanged(value) = event {
                        this.description = value.clone();
                        cx.emit(PersistentRowEditorEvent::Description {
                            row: this.row_id,
                            value: value.clone(),
                        });
                    }
                },
            ),
        ];
        Self {
            kind,
            index,
            row_id,
            key_input,
            value_input,
            description_input,
            description,
            show_description: true,
            _subscriptions: subscriptions,
        }
    }

    fn on_cell_event(
        &mut self,
        _input: Entity<TableCellInput>,
        event: &TableCellInputEvent,
        cx: &mut Context<Self>,
    ) {
        cx.emit(PersistentRowEditorEvent::Cell(event.clone()));
    }

    fn row_id(&self) -> TableRowId {
        self.row_id
    }

    fn set_index(&mut self, index: usize) {
        self.index = index;
    }

    fn cell(&self, column: TableCellColumn) -> Entity<TableCellInput> {
        match column {
            TableCellColumn::Key => self.key_input.clone(),
            TableCellColumn::Value => self.value_input.clone(),
        }
    }
}

impl EventEmitter<PersistentRowEditorEvent> for PersistentRowEditor {}

impl Render for PersistentRowEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (key_cell_selector, key_input_selector, value_cell_selector, value_input_selector) =
            match self.kind {
                KeyValueRowsKind::Params => (
                    format!("param-row-key-input-{}", self.index),
                    None,
                    format!("param-row-value-input-{}", self.index),
                    None,
                ),
                KeyValueRowsKind::Headers => (
                    format!("header-row-key-{}", self.index),
                    Some(format!("header-row-key-input-{}", self.index)),
                    format!("header-row-value-{}", self.index),
                    Some(format!("header-row-value-input-{}", self.index)),
                ),
            };
        let description_selector = format!(
            "{}-row-description-input-{}",
            if self.kind == KeyValueRowsKind::Params {
                "param"
            } else {
                "header"
            },
            self.index
        );
        div()
            .h_full()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .child(
                div()
                    .debug_selector(move || key_cell_selector.clone())
                    .h_full()
                    .border_l_1()
                    .border_color(LINE.resolve(cx))
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .when_some(key_input_selector, |this, selector| {
                                this.debug_selector(move || selector.clone())
                            })
                            .h_full()
                            .child(self.key_input.clone()),
                    ),
            )
            .child(
                div()
                    .debug_selector(move || value_cell_selector.clone())
                    .h_full()
                    .border_l_1()
                    .border_color(LINE.resolve(cx))
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .when_some(value_input_selector, |this, selector| {
                                this.debug_selector(move || selector.clone())
                            })
                            .h_full()
                            .child(self.value_input.clone()),
                    ),
            )
            .when(self.show_description, |row| {
                row.child(
                    div()
                        .debug_selector(move || description_selector.clone())
                        .h_full()
                        .flex_1()
                        .min_w_0()
                        .border_l_1()
                        .border_color(LINE.resolve(cx))
                        .child(div().h_full().px_3().child(self.description_input.clone())),
                )
            })
    }
}

#[derive(Clone, Debug)]
pub(in crate::app::postman_app::request_workspace) enum KeyValueRowsPaneEvent {
    EffectiveUrlChanged,
}

/// Shared stateful Params/Headers row surface. Row values, ordering, and enabled flags stay in the
/// shared WorkspaceViewModel; this entity owns only controls, subscriptions, and scrolling.
pub(in crate::app::postman_app::request_workspace) struct KeyValueRowsPane {
    view_model: Entity<WorkspaceViewModel>,
    panel_layout: Entity<RequestPanelLayout>,
    kind: KeyValueRowsKind,
    projected_tab_id: Option<RequestTabId>,
    row_editors: Vec<Entity<PersistentRowEditor>>,
    row_subscriptions: Vec<Subscription>,
    row_toggle_focus_handles: Vec<FocusHandle>,
    row_delete_focus_handles: Vec<FocusHandle>,
    rows_scroll_handle: ScrollHandle,
    rows_have_overflow: bool,
    draft_row_id: TableRowId,
    draft_key_input: Entity<TableCellInput>,
    draft_value_input: Entity<TableCellInput>,
    draft_description_input: Entity<HeaderInput>,
    draft_subscriptions: Vec<Subscription>,
    draft_toggle_focus_handle: FocusHandle,
    draft_delete_focus_handle: FocusHandle,
    add_row_focus_handle: FocusHandle,
    pending_focus: Option<PendingTableFocus>,
    _panel_layout_subscription: Subscription,
    _description_subscription: Subscription,
}

enum PendingTableFocus {
    Cell(TableCellId),
    Control(FocusHandle),
    WindowNext,
    WindowPrevious,
}

impl EventEmitter<KeyValueRowsPaneEvent> for KeyValueRowsPane {}

impl KeyValueRowsKind {
    fn request_pane(self) -> RequestPane {
        match self {
            Self::Params => RequestPane::Params,
            Self::Headers => RequestPane::Headers,
        }
    }
}

impl KeyValueRowsPane {
    pub(in crate::app::postman_app::request_workspace) fn new(
        view_model: Entity<WorkspaceViewModel>,
        panel_layout: Entity<RequestPanelLayout>,
        kind: KeyValueRowsKind,
        cx: &mut Context<Self>,
    ) -> Self {
        let (key_placeholder, value_placeholder) = match kind {
            KeyValueRowsKind::Params => ("Key", "Value"),
            KeyValueRowsKind::Headers => ("Header name", "Header value"),
        };
        let draft_row_id = TableRowId::next();
        let draft_key_input = cx.new(|cx| {
            TableCellInput::new(
                TableCellId::new(draft_row_id, TableCellColumn::Key),
                key_placeholder,
                cx,
            )
        });
        let draft_value_input = cx.new(|cx| {
            TableCellInput::new(
                TableCellId::new(draft_row_id, TableCellColumn::Value),
                value_placeholder,
                cx,
            )
        });
        let draft_subscriptions = vec![
            cx.subscribe(&draft_key_input, Self::on_draft_cell_event),
            cx.subscribe(&draft_value_input, Self::on_draft_cell_event),
        ];
        let panel_layout_subscription = cx.observe(&panel_layout, |_, _, cx| cx.notify());
        let draft_description_input = cx.new(|cx| {
            HeaderInput::new(cx)
                .with_placeholder("Description")
                .with_font_family(crate::ui::theme::FONT_UI)
                .with_embedded_chrome(true)
        });
        let description_subscription = cx.subscribe(
            &draft_description_input,
            |this, _, event: &HeaderInputEvent, cx| {
                if let HeaderInputEvent::ValueChanged(value) = event {
                    let pane = this.kind.request_pane();
                    this.update_active_request(cx, |request| {
                        request.set_row_description(pane, None, value.clone())
                    });
                }
            },
        );
        let mut pane = Self {
            view_model,
            panel_layout,
            kind,
            projected_tab_id: None,
            row_editors: Vec::new(),
            row_subscriptions: Vec::new(),
            row_toggle_focus_handles: Vec::new(),
            row_delete_focus_handles: Vec::new(),
            rows_scroll_handle: ScrollHandle::new(),
            rows_have_overflow: false,
            draft_row_id,
            draft_key_input,
            draft_value_input,
            draft_description_input,
            draft_subscriptions,
            _description_subscription: description_subscription,
            draft_toggle_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            draft_delete_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            add_row_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            pending_focus: None,
            _panel_layout_subscription: panel_layout_subscription,
        };
        pane.project_active_request(cx);
        pane
    }

    fn update_active_request<R>(
        &self,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut RequestViewModel) -> R,
    ) -> Option<R> {
        let result = self.view_model.update(cx, |view_model, cx| {
            let result = view_model.update_active_request(update);
            cx.notify();
            result
        });
        cx.notify();
        result
    }

    fn emit_effective_url_changed(&self, cx: &mut Context<Self>) {
        if self.kind == KeyValueRowsKind::Params {
            cx.emit(KeyValueRowsPaneEvent::EffectiveUrlChanged);
        }
    }

    fn rebuild_row_editors_from(&mut self, rows: &[KeyValueRow], cx: &mut Context<Self>) {
        self.row_editors.clear();
        self.row_subscriptions.clear();
        self.row_toggle_focus_handles.clear();
        self.row_delete_focus_handles.clear();
        for (index, row) in rows.iter().cloned().enumerate() {
            self.push_row_editor(index, row, cx);
        }
    }

    fn push_row_editor(&mut self, index: usize, row: KeyValueRow, cx: &mut Context<Self>) {
        self.row_toggle_focus_handles
            .push(cx.focus_handle().tab_index(0).tab_stop(true));
        self.row_delete_focus_handles
            .push(cx.focus_handle().tab_index(0).tab_stop(true));
        let kind = self.kind;
        let editor = cx.new(|cx| PersistentRowEditor::new(kind, index, row, cx));
        let subscription = cx.subscribe(&editor, Self::on_persistent_row_event);
        self.row_editors.push(editor);
        self.row_subscriptions.push(subscription);
    }

    fn row_editors_match(&self, rows: &[KeyValueRow], cx: &gpui::App) -> bool {
        self.row_editors.len() == rows.len()
            && self.row_editors.iter().zip(rows).all(|(editor, row)| {
                let editor = editor.read(cx);
                editor.key_input.read(cx).content() == row.key
                    && editor.value_input.read(cx).content() == row.value
                    && editor.description == row.description
            })
    }

    /// Retain existing cell entities whenever the logical prefix is unchanged. Appending a row
    /// must not clear cursor, selection, or Undo history in neighboring cells.
    fn sync_row_editors(
        &mut self,
        rows: &[KeyValueRow],
        force_rebind: bool,
        cx: &mut Context<Self>,
    ) {
        let prefix_matches = !force_rebind
            && self.row_editors.len() <= rows.len()
            && self.row_editors.iter().zip(rows).all(|(editor, row)| {
                let editor = editor.read(cx);
                editor.key_input.read(cx).content() == row.key
                    && editor.value_input.read(cx).content() == row.value
                    && editor.description == row.description
            });
        if !prefix_matches {
            self.rebuild_row_editors_from(rows, cx);
            return;
        }
        for (index, row) in rows
            .iter()
            .cloned()
            .enumerate()
            .skip(self.row_editors.len())
        {
            self.push_row_editor(index, row, cx);
        }
    }

    fn remove_row_editor(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.row_editors.len() {
            return;
        }
        self.row_editors.remove(index);
        let _ = self.row_subscriptions.remove(index);
        self.row_toggle_focus_handles.remove(index);
        self.row_delete_focus_handles.remove(index);
        for (new_index, editor) in self.row_editors.iter().enumerate().skip(index) {
            editor.update(cx, |editor, _| editor.set_index(new_index));
        }
    }

    fn row_index(&self, row_id: TableRowId, cx: &gpui::App) -> Option<usize> {
        self.row_editors
            .iter()
            .position(|editor| editor.read(cx).row_id() == row_id)
    }

    fn cell_entity(&self, cell: TableCellId, cx: &gpui::App) -> Option<Entity<TableCellInput>> {
        if cell.row() == self.draft_row_id {
            return Some(match cell.column() {
                TableCellColumn::Key => self.draft_key_input.clone(),
                TableCellColumn::Value => self.draft_value_input.clone(),
            });
        }
        self.row_editors.iter().find_map(|editor| {
            let editor = editor.read(cx);
            (editor.row_id() == cell.row()).then(|| editor.cell(cell.column()))
        })
    }

    fn reset_draft_inputs(&mut self, cx: &mut Context<Self>) {
        self.draft_description_input
            .update(cx, |input, cx| input.project_content("", cx));
        let (key_placeholder, value_placeholder) = match self.kind {
            KeyValueRowsKind::Params => ("Key", "Value"),
            KeyValueRowsKind::Headers => ("Header name", "Header value"),
        };
        let row_id = TableRowId::next();
        let key_input = cx.new(|cx| {
            TableCellInput::new(
                TableCellId::new(row_id, TableCellColumn::Key),
                key_placeholder,
                cx,
            )
        });
        let value_input = cx.new(|cx| {
            TableCellInput::new(
                TableCellId::new(row_id, TableCellColumn::Value),
                value_placeholder,
                cx,
            )
        });
        self.draft_subscriptions = vec![
            cx.subscribe(&key_input, Self::on_draft_cell_event),
            cx.subscribe(&value_input, Self::on_draft_cell_event),
        ];
        self.draft_row_id = row_id;
        self.draft_key_input = key_input;
        self.draft_value_input = value_input;
        self.project_draft(cx);
    }

    fn active_projection(&self, cx: &gpui::App) -> (Option<RequestTabId>, Vec<KeyValueRow>) {
        let view_model = self.view_model.read(cx);
        let Some(request) = view_model.active_request() else {
            return (None, Vec::new());
        };
        let rows = match self.kind {
            KeyValueRowsKind::Params => request.params(),
            KeyValueRowsKind::Headers => request.headers(),
        };
        (Some(request.tab_id()), rows.to_vec())
    }

    fn on_persistent_row_event(
        &mut self,
        _editor: Entity<PersistentRowEditor>,
        event: &PersistentRowEditorEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            PersistentRowEditorEvent::Cell(event) => self.handle_cell_event(event, false, cx),
            PersistentRowEditorEvent::Description { row, value } => {
                if let Some(index) = self.row_index(*row, cx) {
                    let pane = self.kind.request_pane();
                    self.update_active_request(cx, |request| {
                        request.set_row_description(pane, Some(index), value.clone())
                    });
                }
            }
        }
    }

    fn on_draft_cell_event(
        &mut self,
        _input: Entity<TableCellInput>,
        event: &TableCellInputEvent,
        cx: &mut Context<Self>,
    ) {
        self.handle_cell_event(event, true, cx);
    }

    fn handle_cell_event(
        &mut self,
        event: &TableCellInputEvent,
        draft: bool,
        cx: &mut Context<Self>,
    ) {
        let cell = match event {
            TableCellInputEvent::ValueChanged { cell, .. }
            | TableCellInputEvent::SubmitRequested { cell }
            | TableCellInputEvent::TraversalRequested { cell, .. } => *cell,
        };
        let valid_cell = if draft {
            cell.row() == self.draft_row_id
        } else {
            self.row_index(cell.row(), cx).is_some()
        };
        if !valid_cell {
            return;
        }

        match event {
            TableCellInputEvent::ValueChanged { value, .. } if draft => {
                let pane = self.kind.request_pane();
                match cell.column() {
                    TableCellColumn::Key => {
                        self.update_active_request(cx, |request| {
                            request.set_row_draft_key(pane, value)
                        });
                    }
                    TableCellColumn::Value => {
                        self.update_active_request(cx, |request| {
                            request.set_row_draft_value(pane, value)
                        });
                    }
                }
                self.emit_effective_url_changed(cx);
            }
            TableCellInputEvent::ValueChanged { value, .. } => {
                let Some(index) = self.row_index(cell.row(), cx) else {
                    return;
                };
                match (self.kind, cell.column()) {
                    (KeyValueRowsKind::Params, TableCellColumn::Key) => {
                        self.update_active_request(cx, |request| {
                            request.set_param_key(index, value.clone())
                        });
                        self.emit_effective_url_changed(cx);
                    }
                    (KeyValueRowsKind::Params, TableCellColumn::Value) => {
                        self.update_active_request(cx, |request| {
                            request.set_param_value(index, value.clone())
                        });
                        self.emit_effective_url_changed(cx);
                    }
                    (KeyValueRowsKind::Headers, TableCellColumn::Key) => {
                        self.update_active_request(cx, |request| {
                            request.set_header_key(index, value.clone())
                        });
                    }
                    (KeyValueRowsKind::Headers, TableCellColumn::Value) => {
                        self.update_active_request(cx, |request| {
                            request.set_header_value(index, value.clone())
                        });
                    }
                }
            }
            TableCellInputEvent::SubmitRequested { .. } => self.append_row(cx),
            TableCellInputEvent::TraversalRequested { direction, .. } => {
                self.queue_traversal(cell, *direction, cx);
            }
        }
    }

    fn queue_traversal(
        &mut self,
        cell: TableCellId,
        direction: TableCellTraversal,
        cx: &mut Context<Self>,
    ) {
        if cell.column() == TableCellColumn::Value
            && direction == TableCellTraversal::Forward
            && self.panel_layout.read(cx).show_descriptions()
        {
            let input = if cell.row() == self.draft_row_id {
                self.draft_description_input.clone()
            } else if let Some(index) = self.row_index(cell.row(), cx) {
                self.row_editors[index].read(cx).description_input.clone()
            } else {
                return;
            };
            self.pending_focus = Some(PendingTableFocus::Control(input.read(cx).focus_handle(cx)));
            cx.notify();
            return;
        }
        self.pending_focus = if cell.row() == self.draft_row_id {
            match (self.kind, cell.column(), direction) {
                (_, TableCellColumn::Key, TableCellTraversal::Forward) => {
                    Some(PendingTableFocus::Cell(TableCellId::new(
                        self.draft_row_id,
                        TableCellColumn::Value,
                    )))
                }
                (_, TableCellColumn::Value, TableCellTraversal::Backward) => {
                    Some(PendingTableFocus::Cell(TableCellId::new(
                        self.draft_row_id,
                        TableCellColumn::Key,
                    )))
                }
                (KeyValueRowsKind::Headers, TableCellColumn::Key, TableCellTraversal::Backward) => {
                    Some(PendingTableFocus::Control(
                        self.draft_toggle_focus_handle.clone(),
                    ))
                }
                (
                    KeyValueRowsKind::Headers,
                    TableCellColumn::Value,
                    TableCellTraversal::Forward,
                ) => Some(PendingTableFocus::Control(
                    self.draft_delete_focus_handle.clone(),
                )),
                (_, TableCellColumn::Key, TableCellTraversal::Backward) => {
                    Some(PendingTableFocus::WindowPrevious)
                }
                (_, TableCellColumn::Value, TableCellTraversal::Forward) => {
                    Some(PendingTableFocus::WindowNext)
                }
            }
        } else {
            let Some(index) = self.row_index(cell.row(), cx) else {
                return;
            };
            match (cell.column(), direction) {
                (TableCellColumn::Key, TableCellTraversal::Forward) => Some(
                    PendingTableFocus::Cell(TableCellId::new(cell.row(), TableCellColumn::Value)),
                ),
                (TableCellColumn::Value, TableCellTraversal::Backward) => Some(
                    PendingTableFocus::Cell(TableCellId::new(cell.row(), TableCellColumn::Key)),
                ),
                (TableCellColumn::Key, TableCellTraversal::Backward) => Some(
                    PendingTableFocus::Control(self.row_toggle_focus_handles[index].clone()),
                ),
                (TableCellColumn::Value, TableCellTraversal::Forward) => Some(
                    PendingTableFocus::Control(self.row_delete_focus_handles[index].clone()),
                ),
            }
        };
        cx.notify();
    }

    fn apply_pending_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.pending_focus.take() else {
            return;
        };
        match target {
            PendingTableFocus::Cell(cell) => {
                if let Some(input) = self.cell_entity(cell, cx) {
                    input.read(cx).focus_handle(cx).focus(window, cx);
                }
            }
            PendingTableFocus::Control(focus) => focus.focus(window, cx),
            PendingTableFocus::WindowNext => window.focus_next(cx),
            PendingTableFocus::WindowPrevious => window.focus_prev(cx),
        }
    }

    fn append_row(&mut self, cx: &mut Context<Self>) {
        let appended = match self.kind {
            KeyValueRowsKind::Params => {
                let appended = self
                    .update_active_request(cx, RequestViewModel::append_param_row)
                    .is_some();
                if appended {
                    self.emit_effective_url_changed(cx);
                }
                appended
            }
            KeyValueRowsKind::Headers => self
                .update_active_request(cx, RequestViewModel::append_header_row)
                .is_some(),
        };
        if appended {
            let (_, rows) = self.active_projection(cx);
            self.sync_row_editors(&rows, false, cx);
            self.reset_draft_inputs(cx);
            self.rows_scroll_handle.scroll_to_bottom();
        }
    }

    fn add_current_row(&mut self, cx: &mut Context<Self>) {
        self.append_row(cx);
    }

    fn toggle_param(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_active_request(cx, |request| request.toggle_param(index));
        self.emit_effective_url_changed(cx);
    }

    fn remove_param(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_active_request(cx, |request| request.remove_param(index));
        self.remove_row_editor(index, cx);
        self.emit_effective_url_changed(cx);
    }

    fn toggle_header(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_active_request(cx, |request| request.toggle_header(index));
    }

    fn toggle_header_draft(&mut self, cx: &mut Context<Self>) {
        let appended = self.update_active_request(cx, |request| {
            let index = request.headers().len();
            request.append_header_row();
            request.toggle_header(index);
        });
        if appended.is_some() {
            let (_, rows) = self.active_projection(cx);
            self.sync_row_editors(&rows, false, cx);
            self.reset_draft_inputs(cx);
            self.rows_scroll_handle.scroll_to_bottom();
        }
    }

    fn remove_header(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_active_request(cx, |request| request.remove_header(index));
        self.remove_row_editor(index, cx);
    }

    fn clear_header_draft(&mut self, cx: &mut Context<Self>) {
        self.update_active_request(cx, RequestViewModel::clear_header_draft);
        self.reset_draft_inputs(cx);
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

    fn project_draft(&self, cx: &mut Context<Self>) {
        let description = self
            .view_model
            .read(cx)
            .active_request()
            .map(|r| {
                r.request_draft()
                    .row_description(self.kind == KeyValueRowsKind::Headers, None)
                    .to_string()
            })
            .unwrap_or_default();
        self.draft_description_input
            .update(cx, |input, cx| input.project_content(description, cx));

        let (key, value) = {
            let view_model = self.view_model.read(cx);
            view_model
                .active_request()
                .and_then(|request| request.row_draft(self.kind.request_pane()))
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .unwrap_or_default()
        };
        self.draft_key_input.update(cx, |input, cx| {
            input.project_content(key, cx);
        });
        self.draft_value_input.update(cx, |input, cx| {
            input.project_content(value, cx);
        });
    }

    pub(in crate::app::postman_app::request_workspace) fn project_active_request(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let (tab_id, rows) = self.active_projection(cx);
        let tab_changed = self.projected_tab_id != tab_id;
        if tab_changed || !self.row_editors_match(&rows, cx) {
            self.sync_row_editors(&rows, tab_changed, cx);
        }
        if tab_changed {
            self.reset_draft_inputs(cx);
        } else {
            self.project_draft(cx);
        }
        self.projected_tab_id = tab_id;
        cx.notify();
    }

    fn render_rows_editor(&self, panel_height: f32, cx: &mut Context<Self>) -> gpui::AnyElement {
        use crate::ui::{components::kit_controls, theme::metrics as m};
        use gpui_kit::{assets::IconName, component::Icon};
        let headers = self.kind == KeyValueRowsKind::Headers;
        let show_description = self.panel_layout.read(cx).show_descriptions();
        let compact = panel_height < 300.;
        let prefix = if headers { "header" } else { "param" };
        let plural = if headers { "headers" } else { "params" };
        let (_, rows) = self.active_projection(cx);
        let model = self.view_model.read(cx);
        let Some(request) = model.active_request() else {
            return div().into_any_element();
        };
        let (draft_key, draft_value) = request
            .row_draft(self.kind.request_pane())
            .unwrap_or_default();
        let draft_enabled =
            !draft_key.trim().is_empty() && (!headers || !draft_value.trim().is_empty());
        // Size the grid to its rows, with remaining space below Add rather than
        // inside an empty table. Overflow stays local to the row viewport.
        let section_height = if compact { 32. } else { 55. };
        let table_available = (panel_height - 2. - section_height - 40. - 12.).max(34.);
        let table_height = (34. + (rows.len() + 1) as f32 * 40.).min(table_available);
        let scroll_selector = format!("{plural}-rows-scroll");
        let mut table_rows = div()
            .id((plural, 0usize))
            .debug_selector(move || scroll_selector.clone())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.rows_scroll_handle)
            .when(self.rows_have_overflow, |rows| rows.pr_4())
            .flex()
            .flex_col();
        for (index, row) in rows.iter().enumerate() {
            let row_selector = format!("{prefix}-row-{index}");
            let toggle_selector = format!("{prefix}-row-toggle-{index}");
            let delete_selector = format!("{prefix}-row-delete-{index}");
            let stable_id = self.row_editors[index].entity_id();
            let on_toggle = cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                if headers {
                    this.toggle_header(index, cx);
                } else {
                    this.toggle_param(index, cx);
                }
            });
            table_rows = table_rows.child(
                div()
                    .id(("row", stable_id))
                    .debug_selector(move || row_selector.clone())
                    .h(m::TABLE_ROW)
                    .flex_none()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(LINE.resolve(cx))
                    .child(
                        div().w_10().flex_none().flex().justify_center().child(
                            kit_controls::parameter_checkbox(
                                ("enabled", stable_id),
                                row.enabled,
                                cx,
                            )
                            .debug_selector(move || toggle_selector.clone())
                            .track_focus(&self.row_toggle_focus_handles[index])
                            .accessibility_label(format!("Enable {prefix} {}", index + 1))
                            .on_change(move |_, event, window, cx| on_toggle(event, window, cx)),
                        ),
                    )
                    .child(self.row_editors[index].clone())
                    .child(
                        div().w_10().flex_none().flex().justify_center().child(
                            kit_controls::editor_button(("delete", stable_id), "", cx)
                                .accessibility_label("Remove row")
                                .size_8()
                                .p_0()
                                .child(Icon::new(IconName::X).size(m::SMALL_ICON))
                                .debug_selector(move || delete_selector.clone())
                                .track_focus(&self.row_delete_focus_handles[index])
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if headers {
                                        this.remove_header(index, cx);
                                    } else {
                                        this.remove_param(index, cx);
                                    }
                                    this.focus_after_row_removal(index, window, cx);
                                })),
                        ),
                    ),
            );
        }
        let index = rows.len();
        let draft_row_selector = format!("{prefix}-row-{index}");
        let draft_toggle_selector = if headers {
            format!("header-row-toggle-{index}")
        } else {
            "params-draft-toggle".into()
        };
        let on_draft_toggle =
            cx.listener(|this, _: &gpui::ClickEvent, _, cx| this.toggle_header_draft(cx));
        let mut draft = div()
            .id((plural, 1usize))
            .debug_selector(move || draft_row_selector.clone())
            .h(m::TABLE_ROW)
            .flex_none()
            .flex()
            .items_center()
            .border_b_1()
            .border_color(LINE.resolve(cx))
            .child(
                div().w_10().flex_none().flex().justify_center().child(
                    kit_controls::parameter_checkbox((plural, 2usize), draft_enabled, cx)
                        .debug_selector(move || draft_toggle_selector.clone())
                        .track_focus(&self.draft_toggle_focus_handle)
                        .accessibility_label(if headers {
                            "Enable new header"
                        } else {
                            "New parameter is enabled when it has a key"
                        })
                        .disabled(!headers)
                        .on_change(move |_, event, window, cx| on_draft_toggle(event, window, cx)),
                ),
            );
        for (column, input) in [
            ("key", self.draft_key_input.clone()),
            ("value", self.draft_value_input.clone()),
        ] {
            let selector = format!("{prefix}-row-{column}-input-{index}");
            let legacy = format!("row-{column}-input");
            let cell_selector = format!("{prefix}-row-{column}-{index}");
            draft = draft.child(
                div()
                    .debug_selector(move || cell_selector.clone())
                    .h_full()
                    .flex_1()
                    .min_w_0()
                    .border_l_1()
                    .border_color(LINE.resolve(cx))
                    .child(
                        div()
                            .debug_selector(move || selector.clone())
                            .h_full()
                            .child(
                                div()
                                    .debug_selector(move || legacy.clone())
                                    .h_full()
                                    .child(input),
                            ),
                    ),
            );
        }
        if show_description {
            let selector = format!("{prefix}-row-description-input-{index}");
            draft = draft.child(
                div()
                    .debug_selector(move || selector.clone())
                    .h_full()
                    .flex_1()
                    .min_w_0()
                    .border_l_1()
                    .border_color(LINE.resolve(cx))
                    .child(
                        div()
                            .h_full()
                            .px_3()
                            .child(self.draft_description_input.clone()),
                    ),
            );
        }
        let delete_selector = format!("header-row-delete-{index}");
        draft = draft.child(div().w_10().flex_none().when(headers, |cell| {
            cell.child(
                kit_controls::editor_button((plural, 3usize), "", cx)
                    .accessibility_label("Clear new header")
                    .size_8()
                    .p_0()
                    .child(Icon::new(IconName::X).size(m::SMALL_ICON))
                    .debug_selector(move || delete_selector.clone())
                    .track_focus(&self.draft_delete_focus_handle)
                    .on_click(cx.listener(|this, _, _, cx| this.clear_header_draft(cx))),
            )
        }));
        table_rows = table_rows.child(draft);
        let count_selector = format!("{plural}-enabled-count");
        let scrollbar_selector = format!("{plural}-scrollbar");

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .px_7()
            .pb_3()
            .child(
                div()
                    .h(gpui::rems(if compact { 2. } else { 3.4375 }))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(m::LABEL)
                    .text_color(TEXT.resolve(cx))
                    .child(if headers {
                        "Request headers"
                    } else {
                        "Query parameters"
                    })
                    .child(
                        div()
                            .debug_selector(move || count_selector.clone())
                            .text_size(m::CAPTION)
                            .text_color(MUTED.resolve(cx))
                            .child(if headers {
                                "Sent with this request".to_string()
                            } else {
                                "Synced with URL".to_string()
                            }),
                    ),
            )
            .child(
                div()
                    .h(gpui::px(table_height))
                    .flex_none()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .border_1()
                    .border_color(LINE.resolve(cx))
                    .rounded(m::RADIUS)
                    .overflow_hidden()
                    .child(
                        div()
                            .h(m::TABLE_HEADER)
                            .when(self.rows_have_overflow, |head| head.pr_4())
                            .flex_none()
                            .flex()
                            .items_center()
                            .bg(PANEL_ALT.resolve(cx))
                            .text_size(gpui::rems(9. / 16.))
                            .font_family(FONT_MONO)
                            .text_color(MUTED.resolve(cx))
                            .child(div().w_10().flex_none())
                            .child(div().flex_1().min_w_0().px_3().child("KEY"))
                            .child(
                                div()
                                    .flex_1()
                                    .px_3()
                                    .border_l_1()
                                    .border_color(LINE.resolve(cx))
                                    .child("VALUE"),
                            )
                            .when(show_description, |head| {
                                head.child(
                                    div()
                                        .flex_1()
                                        .px_3()
                                        .border_l_1()
                                        .border_color(LINE.resolve(cx))
                                        .child("DESCRIPTION"),
                                )
                            })
                            .child(div().w_10().flex_none()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .relative()
                            .child(table_rows)
                            .on_prepaint({
                                let this = cx.weak_entity();
                                let scroll = self.rows_scroll_handle.clone();
                                let previous = self.rows_have_overflow;
                                move |_, window, cx| {
                                    let has_overflow = scroll.max_offset().y > gpui::Pixels::ZERO;
                                    if has_overflow != previous {
                                        window.defer(cx, move |_, cx| {
                                            let _ = this.update(cx, |this, cx| {
                                                if this.rows_have_overflow != has_overflow {
                                                    this.rows_have_overflow = has_overflow;
                                                    cx.notify();
                                                }
                                            });
                                        });
                                    }
                                }
                            })
                            .when(self.rows_have_overflow, |area| {
                                area.child(
                                    div()
                                        .debug_selector(move || scrollbar_selector.clone())
                                        .absolute()
                                        .top_0()
                                        .right_0()
                                        .bottom_0()
                                        .w(Scrollbar::width())
                                        .child(
                                            Scrollbar::vertical(&self.rows_scroll_handle)
                                                .id((plural, 4usize))
                                                .mode(ScrollbarMode::Always),
                                        ),
                                )
                            }),
                    ),
            )
            .child(
                div().h_10().flex_none().flex().items_center().child(
                    kit_controls::editor_button(
                        "add-row-button",
                        if headers {
                            "+ Add header"
                        } else {
                            "+ Add parameter"
                        },
                        cx,
                    )
                    .debug_selector(|| "add-row-button".into())
                    .track_focus(&self.add_row_focus_handle)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.add_row_focus_handle.focus(window, cx);
                        this.add_current_row(cx);
                    })),
                ),
            )
            .child(div().flex_1().min_h_0())
            .into_any_element()
    }
}

impl Render for KeyValueRowsPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (_, rows) = self.active_projection(cx);
        if !self.row_editors_match(&rows, cx) {
            self.sync_row_editors(&rows, false, cx);
        }
        let visible = self.panel_layout.read(cx).show_descriptions();
        for editor in &self.row_editors {
            if editor.read(cx).show_description != visible {
                editor.update(cx, |editor, cx| {
                    editor.show_description = visible;
                    cx.notify();
                });
            }
        }
        self.apply_pending_focus(window, cx);
        let panel_height = self.panel_layout.read(cx).height();
        self.render_rows_editor(panel_height, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext, TestAppContext};

    #[gpui::test]
    fn duplicate_param_rows_keep_identity_history_owners_across_append_and_removal(
        cx: &mut TestAppContext,
    ) {
        let workspace = cx.new(|_| WorkspaceViewModel::new());
        let panel_layout = cx.new(|_| RequestPanelLayout::default());
        let pane = cx.new(|cx| {
            KeyValueRowsPane::new(
                workspace.clone(),
                panel_layout,
                KeyValueRowsKind::Params,
                cx,
            )
        });

        pane.update(cx, |pane, cx| {
            pane.append_row(cx);
            pane.append_row(cx);
        });
        let ids = pane.read_with(cx, |pane, cx| {
            pane.row_editors
                .iter()
                .map(|editor| editor.read(cx).row_id())
                .collect::<Vec<_>>()
        });
        assert_eq!(ids.len(), 2);

        pane.update(cx, |pane, cx| {
            pane.toggle_param(1, cx);
            pane.project_active_request(cx);
            assert_eq!(pane.row_editors[0].read(cx).row_id(), ids[0]);
            assert_eq!(pane.row_editors[1].read(cx).row_id(), ids[1]);

            pane.remove_param(0, cx);
            assert_eq!(pane.row_editors[0].read(cx).row_id(), ids[1]);
        });
        workspace.read_with(cx, |workspace, _| {
            let rows = workspace.active_request().unwrap().params();
            assert_eq!(rows.len(), 1);
            assert!(!rows[0].enabled);
            assert!(rows[0].key.is_empty());
            assert!(rows[0].value.is_empty());
        });
    }

    #[gpui::test]
    fn table_traversal_resolves_from_stable_cell_identity(cx: &mut TestAppContext) {
        let workspace = cx.new(|_| WorkspaceViewModel::new());
        let panel_layout = cx.new(|_| RequestPanelLayout::default());
        let pane = cx.new(|cx| {
            KeyValueRowsPane::new(workspace, panel_layout, KeyValueRowsKind::Headers, cx)
        });
        pane.update(cx, |pane, cx| {
            pane.append_row(cx);
            let row_id = pane.row_editors[0].read(cx).row_id();
            pane.queue_traversal(
                TableCellId::new(row_id, TableCellColumn::Key),
                TableCellTraversal::Forward,
                cx,
            );
            assert!(matches!(
                pane.pending_focus,
                Some(PendingTableFocus::Cell(cell))
                    if cell == TableCellId::new(row_id, TableCellColumn::Value)
            ));
        });
    }
}

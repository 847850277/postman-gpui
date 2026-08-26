use super::super::layout::{
    adaptive_request_panel_height, header_row_complete, row_scrollbar_geometry,
    visible_row_capacity,
};
use crate::{
    app::{ActivateControl, KeyValueRow, RequestPane, WorkspaceViewModel},
    ui::{
        components::input::header_input::{HeaderInput, HeaderInputEvent},
        theme::{
            ACCENT, ACCENT_SOFT, ERROR, FONT_MONO, FONT_UI, INFO, INFO_SOFT, LINE, MUTED, OK,
            OK_SOFT, PANEL, PANEL_ALT, SUBTEXT, TEXT,
        },
    },
};
use gpui::{
    div, prelude::FluentBuilder, px, relative, rgb, AppContext, Context, Entity, EventEmitter,
    FocusHandle, FontWeight, InteractiveElement, IntoElement, ParentElement, Render, Role,
    ScrollHandle, StatefulInteractiveElement, Styled, Subscription, Window,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app::postman_app::request_workspace) enum KeyValueRowsKind {
    Params,
    Headers,
}

#[derive(Clone, Debug)]
pub(in crate::app::postman_app::request_workspace) enum PersistentRowEditorEvent {
    KeyChanged {
        kind: KeyValueRowsKind,
        index: usize,
        value: String,
    },
    ValueChanged {
        kind: KeyValueRowsKind,
        index: usize,
        value: String,
    },
    SubmitRequested,
}

/// Editing buffers for one persistent Params or Headers row. Business values remain in the
/// ViewModel; these entities only retain cursor and selection state.
pub(in crate::app::postman_app::request_workspace) struct PersistentRowEditor {
    kind: KeyValueRowsKind,
    index: usize,
    key_input: Entity<HeaderInput>,
    value_input: Entity<HeaderInput>,
    _subscriptions: Vec<Subscription>,
}

impl PersistentRowEditor {
    fn new(kind: KeyValueRowsKind, index: usize, row: KeyValueRow, cx: &mut Context<Self>) -> Self {
        let KeyValueRow { key, value, .. } = row;
        let (key_placeholder, value_placeholder) = match kind {
            KeyValueRowsKind::Params => ("Key", "Value"),
            KeyValueRowsKind::Headers => ("Header name", "Header value"),
        };
        let key_input = cx.new(|cx| {
            let mut input = HeaderInput::new(cx).with_placeholder(key_placeholder);
            input.project_content(key, cx);
            input
        });
        let value_input = cx.new(|cx| {
            let mut input = HeaderInput::new(cx).with_placeholder(value_placeholder);
            input.project_content(value, cx);
            input
        });
        let subscriptions = vec![
            cx.subscribe(&key_input, Self::on_key_event),
            cx.subscribe(&value_input, Self::on_value_event),
        ];
        Self {
            kind,
            index,
            key_input,
            value_input,
            _subscriptions: subscriptions,
        }
    }

    fn on_key_event(
        &mut self,
        _input: Entity<HeaderInput>,
        event: &HeaderInputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            HeaderInputEvent::ValueChanged(value) => {
                cx.emit(PersistentRowEditorEvent::KeyChanged {
                    kind: self.kind,
                    index: self.index,
                    value: value.clone(),
                })
            }
            HeaderInputEvent::SubmitRequested => cx.emit(PersistentRowEditorEvent::SubmitRequested),
        }
    }

    fn on_value_event(
        &mut self,
        _input: Entity<HeaderInput>,
        event: &HeaderInputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            HeaderInputEvent::ValueChanged(value) => {
                cx.emit(PersistentRowEditorEvent::ValueChanged {
                    kind: self.kind,
                    index: self.index,
                    value: value.clone(),
                })
            }
            HeaderInputEvent::SubmitRequested => cx.emit(PersistentRowEditorEvent::SubmitRequested),
        }
    }
}

impl EventEmitter<PersistentRowEditorEvent> for PersistentRowEditor {}

impl Render for PersistentRowEditor {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
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
        div()
            .h_full()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .debug_selector(move || key_cell_selector.clone())
                    .h_full()
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
    kind: KeyValueRowsKind,
    row_editors: Vec<Entity<PersistentRowEditor>>,
    row_subscriptions: Vec<Subscription>,
    row_toggle_focus_handles: Vec<FocusHandle>,
    row_delete_focus_handles: Vec<FocusHandle>,
    rows_scroll_handle: ScrollHandle,
    draft_key_input: Entity<HeaderInput>,
    draft_value_input: Entity<HeaderInput>,
    draft_toggle_focus_handle: FocusHandle,
    draft_delete_focus_handle: FocusHandle,
    add_row_focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
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
        kind: KeyValueRowsKind,
        cx: &mut Context<Self>,
    ) -> Self {
        let (key_placeholder, value_placeholder) = match kind {
            KeyValueRowsKind::Params => ("Key", "Value"),
            KeyValueRowsKind::Headers => ("Header name", "Header value"),
        };
        let draft_key_input = cx.new(|cx| HeaderInput::new(cx).with_placeholder(key_placeholder));
        let draft_value_input =
            cx.new(|cx| HeaderInput::new(cx).with_placeholder(value_placeholder));
        let subscriptions = vec![
            cx.subscribe(&draft_key_input, Self::on_draft_key_event),
            cx.subscribe(&draft_value_input, Self::on_draft_value_event),
        ];
        let mut pane = Self {
            view_model,
            kind,
            row_editors: Vec::new(),
            row_subscriptions: Vec::new(),
            row_toggle_focus_handles: Vec::new(),
            row_delete_focus_handles: Vec::new(),
            rows_scroll_handle: ScrollHandle::new(),
            draft_key_input,
            draft_value_input,
            draft_toggle_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            draft_delete_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            add_row_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            _subscriptions: subscriptions,
        };
        pane.project_active_request(cx);
        pane
    }

    fn update_view_model<R>(
        &self,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut WorkspaceViewModel) -> R,
    ) -> R {
        let result = self.view_model.update(cx, |view_model, cx| {
            let result = update(view_model);
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

    fn rebuild_row_editors(&mut self, cx: &mut Context<Self>) {
        let rows = {
            let view_model = self.view_model.read(cx);
            match self.kind {
                KeyValueRowsKind::Params => view_model.params(),
                KeyValueRowsKind::Headers => view_model.headers(),
            }
            .to_vec()
        };
        self.row_editors.clear();
        self.row_subscriptions.clear();
        while self.row_toggle_focus_handles.len() < rows.len() {
            self.row_toggle_focus_handles
                .push(cx.focus_handle().tab_index(0).tab_stop(true));
            self.row_delete_focus_handles
                .push(cx.focus_handle().tab_index(0).tab_stop(true));
        }
        self.row_toggle_focus_handles.truncate(rows.len());
        self.row_delete_focus_handles.truncate(rows.len());
        for (index, row) in rows.into_iter().enumerate() {
            let kind = self.kind;
            let editor = cx.new(|cx| PersistentRowEditor::new(kind, index, row, cx));
            let subscription = cx.subscribe(&editor, Self::on_persistent_row_event);
            self.row_editors.push(editor);
            self.row_subscriptions.push(subscription);
        }
    }

    fn on_persistent_row_event(
        &mut self,
        _editor: Entity<PersistentRowEditor>,
        event: &PersistentRowEditorEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            PersistentRowEditorEvent::KeyChanged { kind, index, value } => match kind {
                KeyValueRowsKind::Params => {
                    self.update_view_model(cx, |view_model| {
                        view_model.set_param_key(*index, value.clone())
                    });
                    self.emit_effective_url_changed(cx);
                }
                KeyValueRowsKind::Headers => {
                    self.update_view_model(cx, |view_model| {
                        view_model.set_header_key(*index, value.clone())
                    });
                }
            },
            PersistentRowEditorEvent::ValueChanged { kind, index, value } => match kind {
                KeyValueRowsKind::Params => {
                    self.update_view_model(cx, |view_model| {
                        view_model.set_param_value(*index, value.clone())
                    });
                    self.emit_effective_url_changed(cx);
                }
                KeyValueRowsKind::Headers => {
                    self.update_view_model(cx, |view_model| {
                        view_model.set_header_value(*index, value.clone())
                    });
                }
            },
            PersistentRowEditorEvent::SubmitRequested => self.append_row(cx),
        }
    }

    fn on_draft_key_event(
        &mut self,
        _input: Entity<HeaderInput>,
        event: &HeaderInputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            HeaderInputEvent::ValueChanged(key) => {
                let pane = self.kind.request_pane();
                self.update_view_model(cx, |view_model| view_model.set_row_draft_key(pane, key));
                self.emit_effective_url_changed(cx);
            }
            HeaderInputEvent::SubmitRequested => self.append_row(cx),
        }
    }

    fn on_draft_value_event(
        &mut self,
        _input: Entity<HeaderInput>,
        event: &HeaderInputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            HeaderInputEvent::ValueChanged(value) => {
                let pane = self.kind.request_pane();
                self.update_view_model(cx, |view_model| {
                    view_model.set_row_draft_value(pane, value)
                });
                self.emit_effective_url_changed(cx);
            }
            HeaderInputEvent::SubmitRequested => self.append_row(cx),
        }
    }

    fn append_row(&mut self, cx: &mut Context<Self>) {
        match self.kind {
            KeyValueRowsKind::Params => {
                self.update_view_model(cx, |view_model| view_model.append_param_row());
                self.rebuild_row_editors(cx);
                self.rows_scroll_handle.scroll_to_bottom();
                self.emit_effective_url_changed(cx);
            }
            KeyValueRowsKind::Headers => {
                self.update_view_model(cx, |view_model| view_model.append_header_row());
                self.rebuild_row_editors(cx);
                self.rows_scroll_handle.scroll_to_bottom();
            }
        }
        self.project_draft(cx);
    }

    fn add_current_row(&mut self, cx: &mut Context<Self>) {
        self.append_row(cx);
    }

    fn toggle_param(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.toggle_param(index));
        self.emit_effective_url_changed(cx);
    }

    fn remove_param(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.remove_param(index));
        if index < self.row_toggle_focus_handles.len() {
            self.row_toggle_focus_handles.remove(index);
            self.row_delete_focus_handles.remove(index);
        }
        self.rebuild_row_editors(cx);
        self.emit_effective_url_changed(cx);
    }

    fn toggle_header(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.toggle_header(index));
    }

    fn toggle_header_draft(&mut self, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| {
            let index = view_model.headers().len();
            view_model.append_header_row();
            view_model.toggle_header(index);
        });
        self.rebuild_row_editors(cx);
        self.rows_scroll_handle.scroll_to_bottom();
        self.project_draft(cx);
    }

    fn remove_header(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.remove_header(index));
        if index < self.row_toggle_focus_handles.len() {
            self.row_toggle_focus_handles.remove(index);
            self.row_delete_focus_handles.remove(index);
        }
        self.rebuild_row_editors(cx);
    }

    fn clear_header_draft(&mut self, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.clear_header_draft());
        self.project_draft(cx);
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
        let (key, value, key_placeholder, value_placeholder) = {
            let view_model = self.view_model.read(cx);
            let (key, value) = view_model
                .row_draft(self.kind.request_pane())
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .unwrap_or_default();
            let placeholders = match self.kind {
                KeyValueRowsKind::Params => ("Key", "Value"),
                KeyValueRowsKind::Headers => ("Header name", "Header value"),
            };
            (key, value, placeholders.0, placeholders.1)
        };
        self.draft_key_input.update(cx, |input, cx| {
            input.project_placeholder(key_placeholder, cx);
            input.project_content(key, cx);
        });
        self.draft_value_input.update(cx, |input, cx| {
            input.project_placeholder(value_placeholder, cx);
            input.project_content(value, cx);
        });
    }

    pub(in crate::app::postman_app::request_workspace) fn project_active_request(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.rebuild_row_editors(cx);
        self.project_draft(cx);
        cx.notify();
    }

    fn render_params_editor(
        &self,
        panel_height: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let row_editors = self.row_editors.clone();
        let toggle_focus_handles = self.row_toggle_focus_handles.clone();
        let delete_focus_handles = self.row_delete_focus_handles.clone();
        let (rows, draft_key, visible_row_count, enabled_count, effective_url) = {
            let view_model = self.view_model.read(cx);
            let (draft_key, _) = view_model
                .row_draft(RequestPane::Params)
                .unwrap_or_default();
            (
                view_model.params().to_vec(),
                draft_key.to_string(),
                view_model.visible_param_row_count(),
                view_model.enabled_param_count(),
                view_model.effective_url(),
            )
        };
        let draft_enabled = !draft_key.trim().is_empty();
        let draft_index = visible_row_count - 1;
        let draft_row_selector = format!("param-row-{draft_index}");
        let draft_key_selector = format!("param-row-key-input-{draft_index}");
        let draft_value_selector = format!("param-row-value-input-{draft_index}");
        let visible_capacity = visible_row_capacity(RequestPane::Params, panel_height);
        let show_scrollbar = visible_row_count > visible_capacity;
        let scrollbar = row_scrollbar_geometry(
            visible_row_count,
            visible_capacity,
            self.rows_scroll_handle.offset().y.as_f32(),
            self.rows_scroll_handle.max_offset().y.as_f32(),
        );

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .child(
                div()
                    .h(px(42.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .px_3()
                    .font_family(FONT_UI)
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(12.0))
                                    .text_color(rgb(TEXT))
                                    .child("Query parameters"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(rgb(SUBTEXT))
                                    .child("Synchronized with the URL query string"),
                            ),
                    )
                    .child(
                        div()
                            .debug_selector(|| "params-enabled-count".into())
                            .h(px(24.0))
                            .px_2()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap_1()
                            .rounded_lg()
                            .bg(rgb(OK_SOFT))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(10.0))
                            .text_color(rgb(OK))
                            .child("●")
                            .child(format!("{enabled_count} enabled")),
                    ),
            )
            .child(
                div()
                    .h(px(32.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .bg(rgb(PANEL_ALT))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .font_family(FONT_UI)
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(10.0))
                    .text_color(rgb(SUBTEXT))
                    .child(div().w(px(18.0)))
                    .child(div().flex_1().child("KEY"))
                    .child(div().flex_1().child("VALUE"))
                    .child(
                        div()
                            .w(px(56.0))
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
                            .id("params-rows-scroll")
                            .debug_selector(|| "params-rows-scroll".into())
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .p_3()
                            .when(show_scrollbar, |this| this.pr(px(22.0)))
                            .overflow_y_scroll()
                            .track_scroll(&self.rows_scroll_handle)
                            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
                            .children(rows.into_iter().zip(row_editors).enumerate().map(
                                |(index, (row, row_editor))| {
                                    let is_enabled = row.enabled;
                                    let row_selector = format!("param-row-{index}");
                                    let toggle_selector = format!("param-row-toggle-{index}");
                                    let delete_selector = format!("param-row-delete-{index}");
                                    let toggle_focus = toggle_focus_handles[index].clone();
                                    let mouse_toggle_focus = toggle_focus.clone();
                                    let toggle_focused = toggle_focus.is_focused(window);
                                    let delete_focus = delete_focus_handles[index].clone();
                                    let mouse_delete_focus = delete_focus.clone();
                                    let delete_focused = delete_focus.is_focused(window);
                                    div()
                                        .debug_selector(move || row_selector.clone())
                                        .h(px(38.0))
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .font_family(FONT_MONO)
                                        .text_size(px(12.0))
                                        .child(
                                            div()
                                                .id(("param-row-toggle", index))
                                                .debug_selector(move || toggle_selector.clone())
                                                .track_focus(&toggle_focus)
                                                .key_context("KeyboardButton")
                                                .role(Role::CheckBox)
                                                .aria_label(format!(
                                                    "{} parameter row {}",
                                                    if is_enabled { "Disable" } else { "Enable" },
                                                    index + 1
                                                ))
                                                .aria_selected(is_enabled)
                                                .size(px(18.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded_sm()
                                                .border_1()
                                                .border_color(rgb(if is_enabled {
                                                    INFO
                                                } else {
                                                    LINE
                                                }))
                                                .bg(rgb(if is_enabled { INFO } else { PANEL }))
                                                .text_color(rgb(PANEL))
                                                .cursor_pointer()
                                                .when(toggle_focused, |control| {
                                                    control.border_2().border_color(rgb(ACCENT))
                                                })
                                                .child(if is_enabled { "✓" } else { "" })
                                                .on_action(cx.listener(
                                                    move |this, _: &ActivateControl, _, cx| {
                                                        this.toggle_param(index, cx)
                                                    },
                                                ))
                                                .on_mouse_up(
                                                    gpui::MouseButton::Left,
                                                    cx.listener(move |this, _, window, cx| {
                                                        mouse_toggle_focus.focus(window, cx);
                                                        this.toggle_param(index, cx)
                                                    }),
                                                ),
                                        )
                                        .child(row_editor)
                                        .child(
                                            div()
                                                .id(("param-row-delete", index))
                                                .debug_selector(move || delete_selector.clone())
                                                .track_focus(&delete_focus)
                                                .key_context("KeyboardButton")
                                                .role(Role::Button)
                                                .aria_label(format!(
                                                    "Delete parameter row {}",
                                                    index + 1
                                                ))
                                                .w(px(56.0))
                                                .h(px(32.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded_lg()
                                                .cursor_pointer()
                                                .text_color(rgb(MUTED))
                                                .hover(|style| {
                                                    style
                                                        .bg(rgb(ACCENT_SOFT))
                                                        .text_color(rgb(ERROR))
                                                })
                                                .when(delete_focused, |control| {
                                                    control.border_1().border_color(rgb(ACCENT))
                                                })
                                                .child("×")
                                                .on_action(cx.listener(
                                                    move |this, _: &ActivateControl, window, cx| {
                                                        this.remove_param(index, cx);
                                                        this.focus_after_row_removal(
                                                            index, window, cx,
                                                        );
                                                    },
                                                ))
                                                .on_mouse_up(
                                                    gpui::MouseButton::Left,
                                                    cx.listener(move |this, _, window, cx| {
                                                        mouse_delete_focus.focus(window, cx);
                                                        this.remove_param(index, cx);
                                                        this.focus_after_row_removal(
                                                            index, window, cx,
                                                        );
                                                    }),
                                                ),
                                        )
                                },
                            ))
                            .child(
                                div()
                                    .debug_selector(move || draft_row_selector.clone())
                                    .h(px(38.0))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .debug_selector(|| "params-draft-toggle".into())
                                            .size(px(18.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_sm()
                                            .border_1()
                                            .border_color(rgb(if draft_enabled {
                                                INFO
                                            } else {
                                                LINE
                                            }))
                                            .bg(rgb(if draft_enabled { INFO } else { PANEL }))
                                            .text_color(rgb(PANEL))
                                            .child(if draft_enabled { "✓" } else { "" }),
                                    )
                                    .child(
                                        div()
                                            .debug_selector(move || draft_key_selector.clone())
                                            .h_full()
                                            .flex_1()
                                            .child(
                                                div()
                                                    .debug_selector(|| "row-key-input".into())
                                                    .h_full()
                                                    .child(self.draft_key_input.clone()),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .debug_selector(move || draft_value_selector.clone())
                                            .h_full()
                                            .flex_1()
                                            .child(
                                                div()
                                                    .debug_selector(|| "row-value-input".into())
                                                    .h_full()
                                                    .child(self.draft_value_input.clone()),
                                            ),
                                    )
                                    .child(div().w(px(56.0)).h(px(32.0))),
                            ),
                    )
                    .when_some(scrollbar, |this, scrollbar| {
                        this.child(
                            div()
                                .debug_selector(|| "params-scrollbar".into())
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
                                        .debug_selector(|| "params-scrollbar-thumb".into())
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
                    .h(px(44.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .px_3()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(PANEL))
                    .child(
                        div()
                            .id("params-add-row-button")
                            .debug_selector(|| "add-row-button".into())
                            .track_focus(&self.add_row_focus_handle)
                            .key_context("KeyboardButton")
                            .role(Role::Button)
                            .aria_label("Add parameter row")
                            .h(px(32.0))
                            .w_full()
                            .px_3()
                            .flex()
                            .items_center()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(LINE))
                            .bg(rgb(PANEL_ALT))
                            .text_color(rgb(SUBTEXT))
                            .font_family(FONT_UI)
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .hover(|style| {
                                style
                                    .bg(rgb(INFO_SOFT))
                                    .border_color(rgb(INFO))
                                    .text_color(rgb(INFO))
                            })
                            .when(self.add_row_focus_handle.is_focused(window), |button| {
                                button.border_color(rgb(ACCENT)).text_color(rgb(ACCENT))
                            })
                            .child("＋ Add parameter")
                            .on_action(cx.listener(|this, _: &ActivateControl, _window, cx| {
                                this.add_current_row(cx)
                            }))
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    this.add_row_focus_handle.focus(window, cx);
                                    this.add_current_row(cx);
                                }),
                            ),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "effective-url-preview".into())
                    .h(px(64.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .px_3()
                    .bg(rgb(INFO_SOFT))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .font_family(FONT_UI)
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(10.0))
                                    .text_color(rgb(INFO))
                                    .child("↗  EFFECTIVE URL"),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "effective-url-value".into())
                                    .overflow_hidden()
                                    .font_family(FONT_MONO)
                                    .text_size(px(11.0))
                                    .text_color(rgb(TEXT))
                                    .child(effective_url),
                            ),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .flex_none()
                            .rounded_lg()
                            .bg(rgb(PANEL))
                            .font_family(FONT_UI)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(10.0))
                            .text_color(rgb(INFO))
                            .child("encoded"),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "params-ready-indicator".into())
                    .h(px(34.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .font_family(FONT_UI)
                    .text_size(px(10.0))
                    .text_color(rgb(SUBTEXT))
                    .child(div().text_color(rgb(OK)).child("✓"))
                    .child("Ready to send — the active value is already in the ViewModel"),
            )
            .into_any_element()
    }
    fn render_headers_editor(
        &self,
        panel_height: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let row_editors = self.row_editors.clone();
        let toggle_focus_handles = self.row_toggle_focus_handles.clone();
        let delete_focus_handles = self.row_delete_focus_handles.clone();
        let (rows, draft_key, draft_value, visible_row_count, enabled_count) = {
            let view_model = self.view_model.read(cx);
            let (draft_key, draft_value) = view_model
                .row_draft(RequestPane::Headers)
                .unwrap_or_default();
            (
                view_model.headers().to_vec(),
                draft_key.to_string(),
                draft_value.to_string(),
                view_model.visible_header_row_count(),
                view_model.enabled_header_count(),
            )
        };
        let disabled_count = rows
            .iter()
            .filter(|row| header_row_complete(row) && !row.enabled)
            .count();
        let draft_complete = !draft_key.trim().is_empty() && !draft_value.trim().is_empty();
        let draft_index = visible_row_count - 1;
        let draft_row_selector = format!("header-row-{draft_index}");
        let draft_toggle_selector = format!("header-row-toggle-{draft_index}");
        let draft_key_selector = format!("header-row-key-{draft_index}");
        let draft_key_input_selector = format!("header-row-key-input-{draft_index}");
        let draft_value_selector = format!("header-row-value-{draft_index}");
        let draft_value_input_selector = format!("header-row-value-input-{draft_index}");
        let draft_status_selector = format!("header-row-status-{draft_index}");
        let draft_delete_selector = format!("header-row-delete-{draft_index}");
        let draft_toggle_focus = self.draft_toggle_focus_handle.clone();
        let mouse_draft_toggle_focus = draft_toggle_focus.clone();
        let draft_toggle_focused = draft_toggle_focus.is_focused(window);
        let draft_delete_focus = self.draft_delete_focus_handle.clone();
        let mouse_draft_delete_focus = draft_delete_focus.clone();
        let draft_delete_focused = draft_delete_focus.is_focused(window);
        let visible_capacity = visible_row_capacity(RequestPane::Headers, panel_height);
        let show_scrollbar = visible_row_count > visible_capacity;
        let scrollbar = row_scrollbar_geometry(
            visible_row_count,
            visible_capacity,
            self.rows_scroll_handle.offset().y.as_f32(),
            self.rows_scroll_handle.max_offset().y.as_f32(),
        );

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .child(
                div()
                    .debug_selector(|| "headers-summary".into())
                    .h(px(42.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .px_3()
                    .font_family(FONT_UI)
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_none()
                                    .text_color(rgb(TEXT))
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(12.0))
                                    .child("Request headers"),
                            )
                            .child(
                                div()
                                    .overflow_hidden()
                                    .text_size(px(11.0))
                                    .text_color(rgb(SUBTEXT))
                                    .child("Disabled rows stay saved but are excluded from Send"),
                            ),
                    )
                    .child(
                        div()
                            .debug_selector(|| "headers-enabled-count".into())
                            .h(px(24.0))
                            .px_2()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap_1()
                            .rounded_lg()
                            .bg(rgb(OK_SOFT))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(10.0))
                            .text_color(rgb(OK))
                            .child("●")
                            .child(format!(
                                "{enabled_count} enabled · {disabled_count} disabled"
                            )),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "headers-table-header".into())
                    .h(px(32.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .bg(rgb(PANEL_ALT))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .font_family(FONT_UI)
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(10.0))
                    .text_color(rgb(SUBTEXT))
                    .child(div().w(px(18.0)))
                    .child(div().flex_1().child("KEY"))
                    .child(div().flex_1().child("VALUE"))
                    .child(
                        div()
                            .w(px(112.0))
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
                            .id("headers-rows-scroll")
                            .debug_selector(|| "headers-rows-scroll".into())
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .p_3()
                            .when(show_scrollbar, |this| this.pr(px(22.0)))
                            .overflow_y_scroll()
                            .track_scroll(&self.rows_scroll_handle)
                            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
                            .children(rows.into_iter().zip(row_editors).enumerate().map(
                                |(index, (row, row_editor))| {
                                    let is_complete = header_row_complete(&row);
                                    let row_enabled = row.enabled;
                                    let is_sent = row.enabled && is_complete;
                                    let (status, status_bg, status_color) = if !is_complete {
                                        ("DRAFT", PANEL_ALT, SUBTEXT)
                                    } else if row.enabled {
                                        ("SENT", OK_SOFT, OK)
                                    } else {
                                        ("EXCLUDED", ACCENT_SOFT, ACCENT)
                                    };
                                    let row_selector = format!("header-row-{index}");
                                    let toggle_selector = format!("header-row-toggle-{index}");
                                    let status_selector = format!("header-row-status-{index}");
                                    let delete_selector = format!("header-row-delete-{index}");
                                    let toggle_focus = toggle_focus_handles[index].clone();
                                    let mouse_toggle_focus = toggle_focus.clone();
                                    let toggle_focused = toggle_focus.is_focused(window);
                                    let delete_focus = delete_focus_handles[index].clone();
                                    let mouse_delete_focus = delete_focus.clone();
                                    let delete_focused = delete_focus.is_focused(window);

                                    div()
                                        .debug_selector(move || row_selector.clone())
                                        .h(px(40.0))
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .font_family(FONT_MONO)
                                        .text_size(px(12.0))
                                        .child(
                                            div()
                                                .id(("header-row-toggle", index))
                                                .debug_selector(move || toggle_selector.clone())
                                                .track_focus(&toggle_focus)
                                                .key_context("KeyboardButton")
                                                .role(Role::CheckBox)
                                                .aria_label(format!(
                                                    "{} header row {}",
                                                    if row_enabled { "Disable" } else { "Enable" },
                                                    index + 1
                                                ))
                                                .aria_selected(row_enabled)
                                                .size(px(18.0))
                                                .flex_none()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded_sm()
                                                .border_1()
                                                .border_color(rgb(if is_sent {
                                                    INFO
                                                } else {
                                                    LINE
                                                }))
                                                .bg(rgb(if is_sent { INFO } else { PANEL }))
                                                .text_color(rgb(PANEL))
                                                .cursor_pointer()
                                                .when(toggle_focused, |control| {
                                                    control.border_2().border_color(rgb(ACCENT))
                                                })
                                                .child(if is_sent { "✓" } else { "" })
                                                .on_action(cx.listener(
                                                    move |this,
                                                          _: &ActivateControl,
                                                          _,
                                                          cx| {
                                                        this.toggle_header(index, cx)
                                                    },
                                                ))
                                                .on_mouse_up(
                                                    gpui::MouseButton::Left,
                                                    cx.listener(move |this, _, window, cx| {
                                                        mouse_toggle_focus.focus(window, cx);
                                                        this.toggle_header(index, cx)
                                                    }),
                                                ),
                                        )
                                        .child(row_editor)
                                        .child(
                                            div()
                                                .w(px(112.0))
                                                .flex_none()
                                                .flex()
                                                .items_center()
                                                .justify_between()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .debug_selector(move || {
                                                            status_selector.clone()
                                                        })
                                                        .h(px(24.0))
                                                        .w(px(76.0))
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .rounded_lg()
                                                        .bg(rgb(status_bg))
                                                        .font_family(FONT_UI)
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .text_size(px(9.0))
                                                        .text_color(rgb(status_color))
                                                        .child(status),
                                                )
                                                .child(
                                                    div()
                                                        .id(("header-row-delete", index))
                                                        .debug_selector(move || {
                                                            delete_selector.clone()
                                                        })
                                                        .track_focus(&delete_focus)
                                                        .key_context("KeyboardButton")
                                                        .role(Role::Button)
                                                        .aria_label(format!(
                                                            "Delete header row {}",
                                                            index + 1
                                                        ))
                                                        .size(px(28.0))
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .rounded_lg()
                                                        .cursor_pointer()
                                                        .text_color(rgb(MUTED))
                                                        .hover(|style| {
                                                            style
                                                                .bg(rgb(ACCENT_SOFT))
                                                                .text_color(rgb(ERROR))
                                                        })
                                                        .when(delete_focused, |control| {
                                                            control
                                                                .border_1()
                                                                .border_color(rgb(ACCENT))
                                                        })
                                                        .child("×")
                                                        .on_action(cx.listener(
                                                            move |this,
                                                                  _: &ActivateControl,
                                                                  window,
                                                                  cx| {
                                                                this.remove_header(index, cx);
                                                                this.focus_after_row_removal(
                                                                    index, window, cx,
                                                                );
                                                            },
                                                        ))
                                                        .on_mouse_up(
                                                            gpui::MouseButton::Left,
                                                            cx.listener(move |this, _, window, cx| {
                                                                mouse_delete_focus.focus(window, cx);
                                                                this.remove_header(index, cx);
                                                                this.focus_after_row_removal(
                                                                    index, window, cx,
                                                                );
                                                            }),
                                                        ),
                                                ),
                                        )
                                },
                            ))
                            .child(
                                div()
                                    .debug_selector(move || draft_row_selector.clone())
                                    .h(px(40.0))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .font_family(FONT_MONO)
                                    .text_size(px(12.0))
                                    .child(
                                        div()
                                            .id("header-draft-toggle")
                                            .debug_selector(move || draft_toggle_selector.clone())
                                            .track_focus(&draft_toggle_focus)
                                            .key_context("KeyboardButton")
                                            .role(Role::CheckBox)
                                            .aria_label(if draft_complete {
                                                "Commit and disable draft header row"
                                            } else {
                                                "Complete the draft header before toggling"
                                            })
                                            .aria_selected(draft_complete)
                                            .size(px(18.0))
                                            .flex_none()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_sm()
                                            .border_1()
                                            .border_color(rgb(if draft_complete {
                                                INFO
                                            } else {
                                                LINE
                                            }))
                                            .bg(rgb(if draft_complete { INFO } else { PANEL }))
                                            .text_color(rgb(PANEL))
                                            .when(draft_toggle_focused, |control| {
                                                control.border_2().border_color(rgb(ACCENT))
                                            })
                                            .child(if draft_complete { "✓" } else { "" })
                                            .on_action(cx.listener(
                                                move |this,
                                                      _: &ActivateControl,
                                                      window,
                                                      cx| {
                                                    if draft_complete {
                                                        this.toggle_header_draft(cx);
                                                        if let Some(focus) = this
                                                            .row_toggle_focus_handles
                                                            .last()
                                                        {
                                                            focus.focus(window, cx);
                                                        }
                                                    }
                                                },
                                            ))
                                            .when(draft_complete, |this| {
                                                this.cursor_pointer().on_mouse_up(
                                                    gpui::MouseButton::Left,
                                                    cx.listener(move |this, _, window, cx| {
                                                        mouse_draft_toggle_focus.focus(window, cx);
                                                        this.toggle_header_draft(cx);
                                                        if let Some(focus) = this
                                                            .row_toggle_focus_handles
                                                            .last()
                                                        {
                                                            focus.focus(window, cx);
                                                        }
                                                    }),
                                                )
                                            }),
                                    )
                                    .child(
                                        div()
                                            .debug_selector(move || draft_key_selector.clone())
                                            .h_full()
                                            .min_w_0()
                                            .flex_1()
                                            .child(
                                                div()
                                                    .debug_selector(move || {
                                                        draft_key_input_selector.clone()
                                                    })
                                                    .h_full()
                                                    .child(
                                                        div()
                                                            .debug_selector(|| {
                                                                "row-key-input".into()
                                                            })
                                                            .h_full()
                                                            .child(self.draft_key_input.clone()),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .debug_selector(move || draft_value_selector.clone())
                                            .h_full()
                                            .min_w_0()
                                            .flex_1()
                                            .child(
                                                div()
                                                    .debug_selector(move || {
                                                        draft_value_input_selector.clone()
                                                    })
                                                    .h_full()
                                                    .child(
                                                        div()
                                                            .debug_selector(|| {
                                                                "row-value-input".into()
                                                            })
                                                            .h_full()
                                                            .child(self.draft_value_input.clone()),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .w(px(112.0))
                                            .flex_none()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .debug_selector(move || {
                                                        draft_status_selector.clone()
                                                    })
                                                    .h(px(24.0))
                                                    .w(px(76.0))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .rounded_lg()
                                                    .bg(rgb(if draft_complete {
                                                        OK_SOFT
                                                    } else {
                                                        PANEL_ALT
                                                    }))
                                                    .font_family(FONT_UI)
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_size(px(9.0))
                                                    .text_color(rgb(if draft_complete {
                                                        OK
                                                    } else {
                                                        SUBTEXT
                                                    }))
                                                    .child(if draft_complete {
                                                        "SENT"
                                                    } else {
                                                        "DRAFT"
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .id("header-draft-delete")
                                                    .debug_selector(move || {
                                                        draft_delete_selector.clone()
                                                    })
                                                    .track_focus(&draft_delete_focus)
                                                    .key_context("KeyboardButton")
                                                    .role(Role::Button)
                                                    .aria_label("Clear draft header row")
                                                    .size(px(28.0))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .rounded_lg()
                                                    .cursor_pointer()
                                                    .text_color(rgb(MUTED))
                                                    .hover(|style| {
                                                        style
                                                            .bg(rgb(ACCENT_SOFT))
                                                            .text_color(rgb(ERROR))
                                                    })
                                                    .when(draft_delete_focused, |control| {
                                                        control
                                                            .border_1()
                                                            .border_color(rgb(ACCENT))
                                                    })
                                                    .child("×")
                                                    .on_action(cx.listener(
                                                        |this, _: &ActivateControl, _, cx| {
                                                            this.clear_header_draft(cx)
                                                        },
                                                    ))
                                                    .on_mouse_up(
                                                        gpui::MouseButton::Left,
                                                        cx.listener(move |this, _, window, cx| {
                                                            mouse_draft_delete_focus
                                                                .focus(window, cx);
                                                            this.clear_header_draft(cx)
                                                        }),
                                                    ),
                                            ),
                                    ),
                            ),
                    )
                    .when_some(scrollbar, |this, scrollbar| {
                        this.child(
                            div()
                                .debug_selector(|| "headers-scrollbar".into())
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
                                        .debug_selector(|| "headers-scrollbar-thumb".into())
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
                    .h(px(44.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .px_3()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(INFO_SOFT))
                    .child(
                        div()
                            .id("headers-add-row-button")
                            .debug_selector(|| "add-row-button".into())
                            .track_focus(&self.add_row_focus_handle)
                            .key_context("KeyboardButton")
                            .role(Role::Button)
                            .aria_label("Add header row")
                            .h(px(32.0))
                            .w_full()
                            .px_3()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(LINE))
                            .bg(rgb(PANEL_ALT))
                            .text_color(rgb(SUBTEXT))
                            .font_family(FONT_UI)
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .hover(|style| {
                                style
                                    .bg(rgb(INFO_SOFT))
                                    .border_color(rgb(INFO))
                                    .text_color(rgb(INFO))
                            })
                            .when(self.add_row_focus_handle.is_focused(window), |button| {
                                button.border_color(rgb(ACCENT)).text_color(rgb(ACCENT))
                            })
                            .child("＋ Add another header row")
                            .child(
                                div()
                                    .font_weight(FontWeight::NORMAL)
                                    .text_color(rgb(MUTED))
                                    .child("Click repeatedly — rows are unlimited"),
                            )
                            .on_action(cx.listener(
                                |this, _: &ActivateControl, _window, cx| {
                                    this.add_current_row(cx)
                                },
                            ))
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    this.add_row_focus_handle.focus(window, cx);
                                    this.add_current_row(cx);
                                }),
                            ),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "headers-ready-indicator".into())
                    .h(px(54.0))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap_2()
                    .px_3()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .font_family(FONT_UI)
                    .text_size(px(10.0))
                    .text_color(rgb(SUBTEXT))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().text_color(rgb(OK)).child("✓"))
                            .child("Ready to send — active values are already in the ViewModel"),
                    )
                    .child(
                        div().font_family(FONT_MONO).text_color(rgb(INFO)).child(
                            "Only complete, checked rows participate in request construction",
                        ),
                    ),
            )
            .into_any_element()
    }
}

impl Render for KeyValueRowsPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pane = self.kind.request_pane();
        let visible_rows = {
            let view_model = self.view_model.read(cx);
            match self.kind {
                KeyValueRowsKind::Params => view_model.visible_param_row_count(),
                KeyValueRowsKind::Headers => view_model.visible_header_row_count(),
            }
        };
        let panel_height = adaptive_request_panel_height(
            pane,
            visible_rows,
            window.viewport_size().height.as_f32(),
        );
        match self.kind {
            KeyValueRowsKind::Params => self.render_params_editor(panel_height, window, cx),
            KeyValueRowsKind::Headers => self.render_headers_editor(panel_height, window, cx),
        }
    }
}

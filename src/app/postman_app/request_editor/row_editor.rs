use super::RequestEditor;
use crate::{
    app::{KeyValueRow, RequestPane},
    ui::components::header_input::{HeaderInput, HeaderInputEvent},
};
use gpui::{
    div, prelude::FluentBuilder, AppContext, Context, Entity, EventEmitter, InteractiveElement,
    IntoElement, ParentElement, Render, Styled, Subscription, Window,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PersistentRowKind {
    Params,
    Headers,
}

#[derive(Clone, Debug)]
pub(super) enum PersistentRowEditorEvent {
    KeyChanged {
        kind: PersistentRowKind,
        index: usize,
        value: String,
    },
    ValueChanged {
        kind: PersistentRowKind,
        index: usize,
        value: String,
    },
    SubmitRequested {
        kind: PersistentRowKind,
    },
}

/// Editing buffers for one persistent Params or Headers row. Business values remain in the
/// ViewModel; these entities only retain cursor and selection state.
pub(super) struct PersistentRowEditor {
    kind: PersistentRowKind,
    index: usize,
    key_input: Entity<HeaderInput>,
    value_input: Entity<HeaderInput>,
    _subscriptions: Vec<Subscription>,
}

impl PersistentRowEditor {
    fn new(
        kind: PersistentRowKind,
        index: usize,
        row: KeyValueRow,
        cx: &mut Context<Self>,
    ) -> Self {
        let KeyValueRow { key, value, .. } = row;
        let (key_placeholder, value_placeholder) = match kind {
            PersistentRowKind::Params => ("Key", "Value"),
            PersistentRowKind::Headers => ("Header name", "Header value"),
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
            HeaderInputEvent::SubmitRequested => {
                cx.emit(PersistentRowEditorEvent::SubmitRequested { kind: self.kind })
            }
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
            HeaderInputEvent::SubmitRequested => {
                cx.emit(PersistentRowEditorEvent::SubmitRequested { kind: self.kind })
            }
        }
    }
}

impl EventEmitter<PersistentRowEditorEvent> for PersistentRowEditor {}

impl Render for PersistentRowEditor {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let (key_cell_selector, key_input_selector, value_cell_selector, value_input_selector) =
            match self.kind {
                PersistentRowKind::Params => (
                    format!("param-row-key-input-{}", self.index),
                    None,
                    format!("param-row-value-input-{}", self.index),
                    None,
                ),
                PersistentRowKind::Headers => (
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

impl RequestEditor {
    pub(super) fn rebuild_param_row_editors(&mut self, cx: &mut Context<Self>) {
        let rows = self.view_model.read(cx).params().to_vec();
        self.param_row_editors.clear();
        self.param_row_subscriptions.clear();
        for (index, row) in rows.into_iter().enumerate() {
            let editor =
                cx.new(|cx| PersistentRowEditor::new(PersistentRowKind::Params, index, row, cx));
            let subscription = cx.subscribe(&editor, Self::on_persistent_row_event);
            self.param_row_editors.push(editor);
            self.param_row_subscriptions.push(subscription);
        }
    }

    pub(super) fn rebuild_header_row_editors(&mut self, cx: &mut Context<Self>) {
        let rows = self.view_model.read(cx).headers().to_vec();
        self.header_row_editors.clear();
        self.header_row_subscriptions.clear();
        for (index, row) in rows.into_iter().enumerate() {
            let editor =
                cx.new(|cx| PersistentRowEditor::new(PersistentRowKind::Headers, index, row, cx));
            let subscription = cx.subscribe(&editor, Self::on_persistent_row_event);
            self.header_row_editors.push(editor);
            self.header_row_subscriptions.push(subscription);
        }
    }

    pub(super) fn on_persistent_row_event(
        &mut self,
        _editor: Entity<PersistentRowEditor>,
        event: &PersistentRowEditorEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            PersistentRowEditorEvent::KeyChanged { kind, index, value } => match kind {
                PersistentRowKind::Params => {
                    self.update_view_model(cx, |view_model| {
                        view_model.set_param_key(*index, value.clone())
                    });
                    self.project_url(cx);
                }
                PersistentRowKind::Headers => {
                    self.update_view_model(cx, |view_model| {
                        view_model.set_header_key(*index, value.clone())
                    });
                }
            },
            PersistentRowEditorEvent::ValueChanged { kind, index, value } => match kind {
                PersistentRowKind::Params => {
                    self.update_view_model(cx, |view_model| {
                        view_model.set_param_value(*index, value.clone())
                    });
                    self.project_url(cx);
                }
                PersistentRowKind::Headers => {
                    self.update_view_model(cx, |view_model| {
                        view_model.set_header_value(*index, value.clone())
                    });
                }
            },
            PersistentRowEditorEvent::SubmitRequested { kind } => {
                self.append_row(
                    match kind {
                        PersistentRowKind::Params => RequestPane::Params,
                        PersistentRowKind::Headers => RequestPane::Headers,
                    },
                    cx,
                );
            }
        }
    }

    pub(super) fn on_row_key_event(
        &mut self,
        _input: Entity<HeaderInput>,
        event: &HeaderInputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            HeaderInputEvent::ValueChanged(key) => {
                let pane = self.view_model.read(cx).request_pane();
                self.update_view_model(cx, |view_model| view_model.set_row_draft_key(pane, key));
                if pane == RequestPane::Params {
                    self.project_url(cx);
                }
            }
            HeaderInputEvent::SubmitRequested => self.add_current_row(cx),
        }
    }

    pub(super) fn on_row_value_event(
        &mut self,
        _input: Entity<HeaderInput>,
        event: &HeaderInputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            HeaderInputEvent::ValueChanged(value) => {
                let pane = self.view_model.read(cx).request_pane();
                self.update_view_model(cx, |view_model| {
                    view_model.set_row_draft_value(pane, value)
                });
                if pane == RequestPane::Params {
                    self.project_url(cx);
                }
            }
            HeaderInputEvent::SubmitRequested => self.add_current_row(cx),
        }
    }

    pub(super) fn add_current_row(&mut self, cx: &mut Context<Self>) {
        let request_pane = self.view_model.read(cx).request_pane();
        self.append_row(request_pane, cx);
    }

    pub(super) fn append_row(&mut self, request_pane: RequestPane, cx: &mut Context<Self>) {
        match request_pane {
            RequestPane::Params => {
                self.update_view_model(cx, |view_model| view_model.append_param_row());
                self.rebuild_param_row_editors(cx);
                self.param_rows_scroll_handle.scroll_to_bottom();
                self.project_url(cx);
            }
            RequestPane::Headers => {
                self.update_view_model(cx, |view_model| view_model.append_header_row());
                self.rebuild_header_row_editors(cx);
                self.header_rows_scroll_handle.scroll_to_bottom();
            }
            RequestPane::Authorization
            | RequestPane::Body
            | RequestPane::Scripts
            | RequestPane::Tests => return,
        }
        self.project_row_draft(cx);
    }

    pub(super) fn toggle_param(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.toggle_param(index));
        self.project_url(cx);
    }

    pub(super) fn remove_param(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.remove_param(index));
        self.rebuild_param_row_editors(cx);
        self.project_url(cx);
    }

    pub(super) fn toggle_header(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.toggle_header(index));
    }

    pub(super) fn toggle_header_draft(&mut self, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| {
            let index = view_model.headers().len();
            view_model.append_header_row();
            view_model.toggle_header(index);
        });
        self.rebuild_header_row_editors(cx);
        self.header_rows_scroll_handle.scroll_to_bottom();
        self.project_row_draft(cx);
    }

    pub(super) fn remove_header(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.remove_header(index));
        self.rebuild_header_row_editors(cx);
    }

    pub(super) fn clear_header_draft(&mut self, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.clear_header_draft());
        self.project_row_draft(cx);
    }
}

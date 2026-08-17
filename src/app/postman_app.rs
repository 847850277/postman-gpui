use crate::{
    app::{
        request_runner::RequestRunner, AuthorizationKind, BodyKind, KeyValueRow, PendingRequest,
        RequestPane, SendId, WorkspaceViewModel,
    },
    models::{MultipartPart, MultipartValue, Request, RequestBody},
    ui::{
        components::{
            body_input::{
                setup_body_input_key_bindings, BodyInput, BodyInputEvent, BodyType, FormDataEntry,
            },
            header_input::{setup_header_input_key_bindings, HeaderInput, HeaderInputEvent},
            history_list::{HistoryList, HistoryListEvent},
            method_selector::{MethodSelector, MethodSelectorEvent},
            response_viewer::{setup_response_viewer_key_bindings, ResponseViewer},
            url_input::{setup_url_input_key_bindings, UrlInput, UrlInputEvent},
        },
        theme::{
            method_color, ACCENT, ACCENT_DARK, ACCENT_INK, ACCENT_SOFT, ACCENT_VIVID, BG, CODE_BG,
            CODE_PANEL, CODE_TEXT, ERROR, FONT_HEADING, FONT_MONO, FONT_UI, INFO, INFO_SOFT, LINE,
            MUTED, OK, OK_SOFT, PANEL, PANEL_ALT, SUBTEXT, TEXT,
        },
    },
};
use gpui::{
    div, prelude::FluentBuilder, px, rgb, AppContext, Context, Entity, EventEmitter, FontWeight,
    InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled,
    Subscription, Window,
};

mod chrome;
mod request_editor;

use request_editor::{RequestEditor, RequestEditorEvent};

/// Application composition root. Feature-specific controls and task lifetimes live in child
/// entities; this type only wires the shell together.
pub struct PostmanApp {
    view_model: Entity<WorkspaceViewModel>,
    request_editor: Entity<RequestEditor>,
    request_runner: Entity<RequestRunner>,
    history_list: Entity<HistoryList>,
    _subscriptions: Vec<Subscription>,
}

impl PostmanApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let view_model = cx.new(|_| WorkspaceViewModel::new());
        Self::with_view_model(view_model, cx)
    }

    /// Dependency-injected constructor used by app hosts and black-box UI tests that need to
    /// observe the ViewModel without mutating the View through a second command surface.
    pub fn with_view_model(view_model: Entity<WorkspaceViewModel>, cx: &mut Context<Self>) -> Self {
        let request_editor = cx.new(|cx| RequestEditor::new(view_model.clone(), cx));
        let request_runner = cx.new(|_| RequestRunner::new());
        let history_list = cx.new(|cx| HistoryList::new(view_model.clone(), cx));
        let subscriptions = vec![
            cx.subscribe(&request_editor, Self::on_request_editor_event),
            cx.subscribe(&history_list, Self::on_history_selected),
        ];

        Self {
            view_model,
            request_editor,
            request_runner,
            history_list,
            _subscriptions: subscriptions,
        }
    }

    fn on_request_editor_event(
        &mut self,
        _editor: Entity<RequestEditor>,
        event: &RequestEditorEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            RequestEditorEvent::Execute(pending) => {
                let pending = pending.clone();
                let view_model = self.view_model.clone();
                self.request_runner
                    .update(cx, |runner, cx| runner.execute(pending, view_model, cx));
            }
            RequestEditorEvent::Abort(send_id) => {
                self.request_runner
                    .update(cx, |runner, _| runner.abort(*send_id));
            }
        }
    }

    fn on_history_selected(
        &mut self,
        _list: Entity<HistoryList>,
        event: &HistoryListEvent,
        cx: &mut Context<Self>,
    ) {
        let HistoryListEvent::RequestSelected(request) = event;
        self.request_editor
            .update(cx, |editor, cx| editor.load_request(request, cx));
    }

    fn new_request(&mut self, cx: &mut Context<Self>) {
        self.request_editor.update(cx, RequestEditor::new_request);
    }
}

impl Render for PostmanApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("main-container")
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .font_family(FONT_UI)
            .child(self.render_top_header())
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.render_left_rail(cx))
                    .child(self.history_list.clone())
                    .child(self.request_editor.clone()),
            )
    }
}

fn request_pane_selector(pane: RequestPane) -> &'static str {
    match pane {
        RequestPane::Params => "request-pane-params",
        RequestPane::Authorization => "request-pane-authorization",
        RequestPane::Headers => "request-pane-headers",
        RequestPane::Body => "request-pane-body",
        RequestPane::Scripts => "request-pane-scripts",
        RequestPane::Tests => "request-pane-tests",
    }
}

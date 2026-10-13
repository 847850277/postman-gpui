//! Request-document commands and dialog lifecycle. Kit owns focus, inputs and modal behavior;
//! the ViewModel owns editor values and the repository owns durable commits.
use super::{RequestTabId, WorkspaceViewModel};
use crate::{
    persistence::request_documents::DocumentRepository,
    ui::theme::{metrics as m, ERROR, MUTED},
};
use gpui::{
    div, prelude::FluentBuilder, AppContext, Context, Entity, EventEmitter, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, Styled, Subscription, Window,
};
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    Disableable, Sizable, WindowExt,
};
use std::{collections::HashSet, path::PathBuf};

#[derive(Clone, Copy)]
pub(super) enum DocumentEvent {
    CloseTab(RequestTabId),
    Quit,
}
#[derive(Clone, Copy)]
pub(super) enum CloseIntent {
    Tab(RequestTabId),
    Quit,
}
#[derive(Clone, Copy)]
struct SaveDialog {
    tab: RequestTabId,
    closing: bool,
}

pub(super) struct SavedRequests {
    model: Entity<WorkspaceViewModel>,
    repository: Option<DocumentRepository>,
    name: Entity<InputState>,
    dialog: Option<SaveDialog>,
    close_intent: Option<CloseIntent>,
    discarded: HashSet<RequestTabId>,
    error: Option<String>,
    _name_subscription: Subscription,
}
impl EventEmitter<DocumentEvent> for SavedRequests {}
impl SavedRequests {
    pub(super) fn new(
        model: Entity<WorkspaceViewModel>,
        path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Request name"));
        let name_subscription = cx.subscribe_in(&name, window, |this, _, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { shift: false, .. }) {
                this.submit(window, cx);
            }
        });
        let mut this = Self {
            model,
            repository: path.map(DocumentRepository::new),
            name,
            dialog: None,
            close_intent: None,
            discarded: HashSet::new(),
            error: None,
            _name_subscription: name_subscription,
        };
        this.reload(cx);
        this
    }
    pub(super) fn reload(&mut self, cx: &mut Context<Self>) {
        if self.model.read(cx).documents_loading() || self.model.read(cx).saving_request().is_some()
        {
            return;
        }
        let repository = self.repository.clone();
        self.model.update(cx, |model, cx| {
            model.set_document_loading();
            cx.notify();
        });
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    repository
                        .ok_or_else(|| {
                            "Local storage directory is unavailable. Restore access and reload."
                                .to_string()
                        })?
                        .load()
                        .map(|load| (load.documents, load.skipped))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.model.update(cx, |model, cx| {
                    model.complete_document_load(result);
                    cx.notify();
                });
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn save_active(
        &mut self,
        rename: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) || self.model.read(cx).saving_request().is_some() {
            return;
        }
        let Some(request) = self.model.read(cx).active_request() else {
            return;
        };
        let tab = request.tab_id();
        if rename || request.saved_document_id().is_none() {
            self.open_dialog(tab, false, window, cx);
        } else {
            let name = request.tab_title();
            self.save(tab, name, window, cx);
        }
    }
    pub(super) fn request_close(
        &mut self,
        intent: CloseIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) || self.close_intent.is_some() {
            return;
        }
        self.close_intent = Some(intent);
        self.discarded.clear();
        self.advance_close(window, cx);
    }
    fn advance_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A pending commit must finish before deciding whether a tab can be closed.
        if self.model.read(cx).saving_request().is_some() {
            return;
        }
        let Some(intent) = self.close_intent else {
            return;
        };
        let model = self.model.read(cx);
        let next = model
            .tabs()
            .iter()
            .find(|r| {
                let included = match intent {
                    CloseIntent::Tab(id) => r.tab_id() == id,
                    CloseIntent::Quit => true,
                };
                included && r.is_dirty() && !self.discarded.contains(&r.tab_id())
            })
            .map(|r| r.tab_id());
        if let Some(tab) = next {
            self.open_dialog(tab, true, window, cx);
        } else {
            self.close_intent = None;
            self.discarded.clear();
            cx.emit(match intent {
                CloseIntent::Tab(id) => DocumentEvent::CloseTab(id),
                CloseIntent::Quit => DocumentEvent::Quit,
            });
        }
    }
    fn open_dialog(
        &mut self,
        tab: RequestTabId,
        closing: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(request) = self.model.read(cx).request_for_tab(tab) else {
            return;
        };
        let title = request.tab_title();
        self.name
            .update(cx, |input, cx| input.set_value(title, window, cx));
        self.dialog = Some(SaveDialog { tab, closing });
        self.error = None;
        let entity = cx.entity();
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            let state = entity.read(cx);
            let busy = state.model.read(cx).saving_request().is_some();
            let closing = state.dialog.is_some_and(|d| d.closing);
            let submit = owner.clone();
            let close = owner.clone();
            let content = entity.clone();
            dialog
                .title(if closing {
                    "Save changes before closing?"
                } else {
                    "Save request"
                })
                .w((window.rem_size() * 32.)
                    .min(window.viewport_size().width - window.rem_size() * 2.))
                .close_button(false)
                .keyboard(!busy)
                .overlay_closable(false)
                .content(move |content_box, _, _| content_box.child(content.clone()))
                .on_ok(move |_, window, cx| {
                    let _ = submit.update(cx, |this, cx| this.submit(window, cx));
                    false
                })
                .on_close(move |_, _, cx| {
                    let _ = close.update(cx, |this, cx| {
                        if this.dialog.is_some() {
                            this.cancel(cx);
                        }
                    });
                })
        });
        self.name.read(cx).focus_handle(cx).focus(window, cx);
        cx.notify();
    }
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(dialog) = self.dialog {
            let name = self.name.read(cx).value().to_string();
            self.save(dialog.tab, name, window, cx);
        }
    }
    fn save(
        &mut self,
        tab: RequestTabId,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.model.read(cx).saving_request().is_some() {
            return;
        }
        let snapshot = self
            .model
            .read(cx)
            .request_for_tab(tab)
            .map(|r| r.document_snapshot(&name));
        let snapshot = match snapshot {
            Some(Ok(snapshot)) => snapshot,
            Some(Err(error)) => {
                self.error = Some(error);
                cx.notify();
                return;
            }
            None => return,
        };
        let Some(repository) = self.repository.clone() else {
            self.error = Some("Local request storage is unavailable.".into());
            cx.notify();
            return;
        };
        if !self.model.update(cx, |model, cx| {
            let started = model.begin_document_save(tab, &snapshot);
            cx.notify();
            started
        }) {
            self.error = Some(
                "Saved requests are not ready. Reload saved requests from Home, then retry.".into(),
            );
            cx.notify();
            return;
        }
        self.error = None;
        cx.spawn_in(window, async move |this, cx| {
            let write = snapshot.clone();
            let result = cx
                .background_executor()
                .spawn(async move { repository.save(&write) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                let successful = result.is_ok();
                this.error = result.as_ref().err().cloned();
                this.model.update(cx, |model, cx| {
                    model.complete_document_save(tab, snapshot, result);
                    cx.notify();
                });
                if successful {
                    if this.dialog.take().is_some() {
                        window.close_dialog(cx);
                    }
                    // Defer so Kit finishes removing the old overlay and restoring focus first.
                    cx.defer_in(window, |this, window, cx| this.advance_close(window, cx));
                } else if this.dialog.is_none() {
                    this.close_intent = None;
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn cancel(&mut self, cx: &mut Context<Self>) {
        self.dialog = None;
        self.close_intent = None;
        self.discarded.clear();
        self.error = None;
        cx.notify();
    }
    fn discard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(dialog) = self.dialog.take() {
            self.discarded.insert(dialog.tab);
        }
        window.close_dialog(cx);
        cx.defer_in(window, |this, window, cx| this.advance_close(window, cx));
    }
}
impl Render for SavedRequests {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let closing = self.dialog.is_some_and(|d| d.closing);
        let saving = self.model.read(cx).saving_request().is_some();
        let selector = if closing {
            "unsaved-request-dialog"
        } else {
            "save-request-dialog"
        };
        div().debug_selector(move || selector.into()).flex().flex_col().gap_4().text_size(m::LABEL)
            .when(closing, |d| d.child("This request has unsaved changes. Discard keeps its last saved version."))
            .child(div().text_color(MUTED.resolve(cx)).child("Request name"))
            .child(div().debug_selector(|| "saved-request-name".into()).child(Input::new(&self.name).disabled(saving)))
            .child(div().text_size(m::CAPTION).text_color(MUTED.resolve(cx)).child("Saved locally, including entered tokens, passwords, headers and body values. Storage is not encrypted. Files are saved as references."))
            .when_some(self.error.clone(), |d, error| d.child(div().debug_selector(|| "request-save-error".into()).text_color(ERROR.resolve(cx)).child(error)))
            .child(div().flex().justify_end().gap_2()
                .child(Button::new("request-save-cancel").debug_selector(|| "request-save-cancel".into()).small().label("Cancel").disabled(saving).on_click(cx.listener(|this, _, window, cx| { this.cancel(cx); window.close_dialog(cx); })))
                .when(closing, |d| d.child(Button::new("request-save-discard").debug_selector(|| "request-save-discard".into()).small().label("Discard changes").disabled(saving).on_click(cx.listener(|this, _, window, cx| this.discard(window, cx)))))
                .child(Button::new("request-save-confirm").debug_selector(|| "request-save-confirm".into()).small().primary().label(if saving { "Saving…" } else { "Save" }).disabled(saving).on_click(cx.listener(|this, _, window, cx| this.submit(window, cx)))))
    }
}

use super::*;
use crate::ui::theme::{metrics as m, ERROR, MUTED};
use gpui::{
    div, prelude::FluentBuilder, AnyElement, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    menu::{DropdownMenu, PopupMenuItem},
    Disableable, Sizable,
};

impl FlowsView {
    pub(in crate::app::postman_app::flows) fn render_file_actions(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let busy = self.file_operation_pending();
        let active = self.library.active().is_some();
        let owner = cx.weak_entity();
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(
                Button::new("flow-open")
                    .debug_selector(|| "flow-open".into())
                    .small()
                    .label("Open…")
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, w, cx| this.open_flow(w, cx))),
            )
            .child(
                Button::new("flow-save")
                    .debug_selector(|| "flow-save".into())
                    .small()
                    .tooltip("Save plaintext YAML, including entered credentials and defaults")
                    .label(if self.files.busy_flow.is_some() {
                        "Saving…"
                    } else {
                        "Save"
                    })
                    .disabled(busy || !active)
                    .on_click(cx.listener(|this, _, w, cx| this.save_flow(SaveKind::Save, w, cx))),
            )
            .child(
                Button::new("flow-file-menu")
                    .debug_selector(|| "flow-file-menu".into())
                    .small()
                    .ghost()
                    .label("⋯")
                    .accessibility_label("Flow file actions")
                    .disabled(busy)
                    .dropdown_menu(move |menu, _, _| {
                        let save = owner.clone();
                        let recent = owner.clone();
                        let close = owner.clone();
                        menu.item(PopupMenuItem::new("Save as…").disabled(!active).on_click(
                            move |_, w, cx| {
                                let _ = save
                                    .update(cx, |this, cx| this.save_flow(SaveKind::SaveAs, w, cx));
                            },
                        ))
                        .item(
                            PopupMenuItem::new("Recent flows…").on_click(move |_, w, cx| {
                                let _ = recent.update(cx, |this, cx| this.open_recent(w, cx));
                            }),
                        )
                        .item(
                            PopupMenuItem::new("Close flow").disabled(!active).on_click(
                                move |_, w, cx| {
                                    let _ = close.update(cx, |this, cx| this.close_flow(w, cx));
                                },
                            ),
                        )
                    }),
            )
            .into_any_element()
    }
    pub(in crate::app::postman_app::flows) fn render_file_feedback(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div().flex_none().when_some(self.files.error.clone(), |d, error| {
            d.child(div().debug_selector(|| "flow-file-error".into()).px_4().py_2().text_size(m::LABEL).text_color(ERROR.resolve(cx)).child(error))
        }).when(self.files.recent_error.is_some(), |d| {
            d.child(div().px_4().py_1().text_size(m::CAPTION).text_color(MUTED.resolve(cx)).child("Recent-file storage needs attention. Open Recent flows to reload; document files are independent."))
        }).into_any_element()
    }
    pub(in crate::app::postman_app::flows) fn file_status(&self) -> String {
        let Some(draft) = self.library.active() else {
            return "Flows".into();
        };
        let label = draft
            .saved()
            .map(|s| s.path.display().to_string())
            .unwrap_or_else(|| "Untitled file".into());
        format!("Flows / {label}")
    }
    pub(in crate::app::postman_app::flows) fn file_state(&self, cx: &App) -> &'static str {
        let Some(draft) = self.library.active() else {
            return "";
        };
        if self.files.busy_flow == Some(draft.id()) {
            "Saving…"
        } else if self.is_dirty(draft.id(), cx) {
            "Unsaved changes"
        } else {
            "Saved"
        }
    }
    pub(super) fn open_close_dialog(
        &mut self,
        flow: FlowId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.files.error = None;
        self.files.dialog = Some(DialogKind::Close(flow));
        self.files.return_focus = window.focused(cx);
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, w, cx| {
            let Some(entity) = owner.upgrade() else { return dialog; };
            let view = entity.read(cx);
            let busy = view.files.busy;
            let name = view.library.get(flow).map(|d| d.title()).unwrap_or("Flow").to_owned();
            let save = owner.clone(); let discard = owner.clone(); let cancel = owner.clone(); let dismiss = owner.clone(); let review = owner.clone();
            dialog.title(format!("Save changes to “{name}”?"))
                .w((w.rem_size()*34.).min(w.viewport_size().width-w.rem_size()*2.))
                .close_button(false).keyboard(!busy).overlay_closable(false)
                .on_ok(|_, _, cx| { cx.propagate(); false })
                .on_close(move |_, w, cx| { let _ = dismiss.update(cx, |this, cx| {
                    if this.files.dialog == Some(DialogKind::Close(flow)) { this.file_dialog_closed(w, cx); }
                }); })
                .child(div().debug_selector(|| "flow-close-dialog".into()).flex().flex_col().gap_3().text_size(m::LABEL)
                    .child("Discard changes leaves the last saved file unchanged.")
                    .when(view.running(flow), |d| d.child("This Flow is running. Closing it also requires confirmation to stop execution."))
                    .child(div().text_size(m::CAPTION).text_color(MUTED.resolve(cx)).child("YAML includes entered credentials and defaults as plaintext. Body files remain references."))
                    .when_some(view.files.error.clone(), |d, e| d.child(div().debug_selector(|| "flow-close-error".into()).text_color(ERROR.resolve(cx)).child(e)))
                    .when(!view.draft_issues(flow, cx).is_empty(), |d| d.child(Button::new("flow-review-edits").small().label("Review edits").disabled(busy).on_click(move |_, w, cx| {
                        let _ = review.update(cx, |this, cx| {
                            this.files.return_focus = None;
                            this.cancel_file_dialog(w, cx);
                            this.select_flow(flow, w, cx);
                            this.check_flow(w, cx);
                        });
                    }))))
                .footer(div().flex().flex_wrap().justify_end().gap_2()
                    .child(Button::new("flow-close-cancel").debug_selector(|| "flow-close-cancel".into()).label("Cancel").disabled(busy).on_click(move |_, w, cx| { let _ = cancel.update(cx, |this, cx| this.cancel_file_dialog(w, cx)); }))
                    .child(Button::new("flow-close-discard").debug_selector(|| "flow-close-discard".into()).label("Discard changes").disabled(busy).on_click(move |_, w, cx| { let _ = discard.update(cx, |this, cx| this.discard_flow(flow, w, cx)); }))
                    .child(Button::new("flow-close-save").debug_selector(|| "flow-close-save".into()).primary().label(if busy { "Saving…" } else { "Save" }).disabled(busy).on_click(move |_, w, cx| { let _ = save.update(cx, |this, cx| this.save_document(flow, SaveKind::Save, w, cx)); })))
        });
        cx.notify();
    }
    pub(super) fn open_stop_dialog(
        &mut self,
        flow: FlowId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.files.dialog = Some(DialogKind::Stop(flow));
        if self.files.return_focus.is_none() {
            self.files.return_focus = window.focused(cx);
        }
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, w, cx| {
            let Some(entity) = owner.upgrade() else { return dialog; };
            let name = entity.read(cx).library.get(flow).map(|d| d.title()).unwrap_or("Flow").to_owned();
            let cancel = owner.clone(); let confirm = owner.clone(); let dismiss = owner.clone();
            dialog.title(format!("Stop running “{name}” and close?"))
                .w((w.rem_size()*34.).min(w.viewport_size().width-w.rem_size()*2.))
                .close_button(false).overlay_closable(false)
                .on_ok(|_, _, cx| { cx.propagate(); false })
                .on_close(move |_, w, cx| { let _ = dismiss.update(cx, |this, cx| {
                    if this.files.dialog == Some(DialogKind::Stop(flow)) { this.file_dialog_closed(w, cx); }
                }); })
                .child(div().debug_selector(|| "flow-stop-close-dialog".into()).child("Closing stops the active request or polling wait and prevents later steps. Requests already received by a server cannot be undone. Cancel keeps the Flow open and running."))
                .footer(div().flex().flex_wrap().justify_end().gap_2()
                    .child(Button::new("flow-stop-close-cancel").label("Cancel").on_click(move |_, w, cx| { let _ = cancel.update(cx, |this, cx| this.cancel_file_dialog(w, cx)); }))
                    .child(Button::new("flow-stop-close-confirm").danger().label("Stop and close").on_click(move |_, w, cx| { let _ = confirm.update(cx, |this, cx| {
                        if let Some(run) = this.execution.runs.get(&flow) { this.files.stop_approved.insert(flow, run.id); }
                        this.dismiss_file_dialog(w, cx);
                        cx.defer_in(w, |this, w, cx| this.advance_file_close(w, cx));
                    }); })))
        });
        cx.notify();
    }
    pub(super) fn open_conflict(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.files.conflict.is_none() {
            return;
        }
        self.files.dialog = Some(DialogKind::Conflict);
        if self.files.return_focus.is_none() {
            self.files.return_focus = window.focused(cx);
        }
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, w, cx| {
            let Some(entity) = owner.upgrade() else { return dialog; };
            let view = entity.read(cx);
            let path = view.files.conflict.as_ref().map(|s| s.path.display().to_string()).unwrap_or_default();
            let cancel = owner.clone(); let replace = owner.clone(); let dismiss = owner.clone();
            dialog.title("Replace file?").w((w.rem_size()*34.).min(w.viewport_size().width-w.rem_size()*2.))
                .overlay_closable(false).on_ok(|_, _, cx| { cx.propagate(); false })
                .on_close(move |_, w, cx| { let _ = dismiss.update(cx, |this, cx| {
                    if this.files.dialog == Some(DialogKind::Conflict) { this.file_dialog_closed(w, cx); }
                }); })
                .child(div().debug_selector(|| "flow-conflict-dialog".into()).flex().flex_col().gap_3().text_size(m::LABEL)
                    .child(path).child("This destination already exists or changed since it was opened or saved. Replace it with the captured Flow, or cancel and choose Save as to keep both versions."))
                .footer(div().flex().justify_end().gap_2()
                    .child(Button::new("flow-conflict-cancel").debug_selector(|| "flow-conflict-cancel".into()).label("Cancel").on_click(move |_, w, cx| { let _ = cancel.update(cx, |this, cx| this.cancel_file_dialog(w, cx)); }))
                    .child(Button::new("flow-conflict-replace").debug_selector(|| "flow-conflict-replace".into()).danger().label("Replace file").on_click(move |_, w, cx| { let _ = replace.update(cx, |this, cx| {
                        let Some(write) = this.files.conflict.take() else { return; };
                        this.dismiss_file_dialog(w, cx);
                        this.write_document(write, w, cx);
                    }); })))
        });
        cx.notify();
    }
    pub(in crate::app::postman_app::flows) fn open_recent(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.files_available(window, cx) {
            return;
        }
        self.files.dialog = Some(DialogKind::Recent);
        self.files.return_focus = window.focused(cx);
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, w, cx| {
            let Some(entity) = owner.upgrade() else {
                return dialog;
            };
            let view = entity.read(cx);
            let busy = view.files.busy;
            let dismiss = owner.clone();
            let reload = owner.clone();
            let rows = view.files.recent.iter().enumerate().map(|(ix, file)| {
                let open = owner.clone();
                let remove = owner.clone();
                let path = file.path.clone();
                let remove_path = path.clone();
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new(gpui::SharedString::from(format!(
                            "flow-recent-{}",
                            file.path.display()
                        )))
                        .debug_selector(move || format!("flow-recent-{ix}"))
                        .tooltip(file.path.display().to_string())
                        .accessibility_label(format!(
                            "Open {} ({})",
                            file.name,
                            file.path.display()
                        ))
                        .ghost()
                        .disabled(busy)
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .min_w_0()
                                .w_full()
                                .flex()
                                .flex_col()
                                .items_start()
                                .gap_1()
                                .child(div().truncate().w_full().child(file.name.clone()))
                                .child(
                                    div()
                                        .truncate()
                                        .w_full()
                                        .text_size(m::CAPTION)
                                        .text_color(MUTED.resolve(cx))
                                        .child(path.display().to_string()),
                                ),
                        )
                        .on_click(move |_, w, cx| {
                            let _ = open.update(cx, |this, cx| {
                                this.dismiss_file_dialog(w, cx);
                                this.open_path(path.clone(), w, cx);
                            });
                        }),
                    )
                    .child(
                        Button::new(gpui::SharedString::from(format!(
                            "flow-forget-{}",
                            file.path.display()
                        )))
                        .debug_selector(move || format!("flow-forget-{ix}"))
                        .small()
                        .ghost()
                        .label("Remove")
                        .disabled(busy)
                        .accessibility_label(format!(
                            "Remove {} from recent flows",
                            file.path.display()
                        ))
                        .on_click(move |_, w, cx| {
                            let _ = remove.update(cx, |this, cx| {
                                this.files.recent.retain(|r| r.path != remove_path);
                                this.files.busy = true;
                                this.persist_recent(w, cx);
                                cx.notify();
                            });
                        }),
                    )
            });
            dialog
                .title("Recent flows")
                .w((w.rem_size() * 40.).min(w.viewport_size().width - w.rem_size() * 2.))
                .keyboard(!busy)
                .on_ok(|_, _, cx| {
                    cx.propagate();
                    false
                })
                .on_close(move |_, w, cx| {
                    let _ = dismiss.update(cx, |this, cx| {
                        if this.files.dialog == Some(DialogKind::Recent) {
                            this.file_dialog_closed(w, cx);
                        }
                    });
                })
                .child(
                    div()
                        .debug_selector(|| "flow-recent-dialog".into())
                        .flex()
                        .flex_col()
                        .gap_3()
                        .when_some(view.files.recent_error.clone(), |d, e| {
                            d.child(
                                div()
                                    .text_color(ERROR.resolve(cx))
                                    .text_size(m::LABEL)
                                    .child(e),
                            )
                        })
                        .when(view.files.recent.is_empty(), |d| {
                            d.child(
                                "No recent files. Save or open a Flow to keep its location here.",
                            )
                        })
                        .child(
                            div()
                                .id("flow-recent-list")
                                .max_h(w.rem_size() * 20.)
                                .overflow_y_scroll()
                                .children(rows),
                        )
                        .child(
                            div()
                                .text_size(m::CAPTION)
                                .text_color(MUTED.resolve(cx))
                                .child("Removing an entry keeps its file and open edits."),
                        ),
                )
                .footer(
                    Button::new("flow-recent-reload")
                        .small()
                        .label("Reload")
                        .disabled(busy)
                        .on_click(move |_, _, cx| {
                            let _ = reload.update(cx, |this, cx| {
                                this.configure_files(this.files.index.clone(), cx)
                            });
                        }),
                )
        });
        cx.notify();
    }
}

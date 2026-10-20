//! Structural edits belong to the Flow owner, not to retained field editors.
use super::{model::StepDirection, FlowId, FlowsView};
use crate::ui::theme::{metrics as m, ERROR, FONT_MONO, LINE, MUTED, PANEL};
use gpui_kit::{
    assets::IconName,
    component::{
        button::{Button, ButtonVariants},
        Disableable, Sizable, WindowExt,
    },
};
use gpui_kit::{
    div, AnyElement, Context, InteractiveElement, IntoElement, ParentElement, Styled, Window,
};

impl FlowsView {
    pub(super) fn render_step_actions(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(draft) = self.library.active() else {
            return div().into_any_element();
        };
        let Some(step) = draft.selected() else {
            return div().into_any_element();
        };
        let flow = draft.id();
        let index = draft
            .document()
            .flow
            .steps
            .iter()
            .position(|s| s.id == step.id)
            .unwrap();
        let up = step.id.clone();
        let down = step.id.clone();
        let remove = step.id.clone();
        div()
            .flex_none()
            .min_w_0()
            .px_4()
            .py_2()
            .border_t_1()
            .border_color(LINE.resolve(cx))
            .bg(PANEL.resolve(cx))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        Button::new("flow-move-step-up")
                            .icon(IconName::ArrowUp)
                            .ghost()
                            .small()
                            .accessibility_label("Move step up")
                            .tooltip("Move step up. References stay unchanged; check them after editing the order.")
                            .disabled(index == 0)
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.reorder_step(flow, &up, StepDirection::Up, w, cx)
                            })),
                    )
                    .child(
                        Button::new("flow-move-step-down")
                            .icon(IconName::ArrowDown)
                            .ghost()
                            .small()
                            .accessibility_label("Move step down")
                            .tooltip("Move step down. References stay unchanged; check them after editing the order.")
                            .disabled(index + 1 == draft.document().flow.steps.len())
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.reorder_step(flow, &down, StepDirection::Down, w, cx)
                            })),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(m::CAPTION)
                            .text_color(MUTED.resolve(cx))
                            .child("Execution order"),
                    )
                    .child(
                        Button::new("flow-remove-step")
                            .icon(IconName::Trash)
                            .ghost()
                            .text_color(ERROR.resolve(cx))
                            .small()
                            .track_focus(&self.remove_step_focus)
                            .accessibility_label("Remove step")
                            .tooltip("Remove step")
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.open_remove_step(flow, &remove, w, cx)
                            })),
                    ),
            )
            .into_any_element()
    }

    fn refresh_step_variables(&self, flow: FlowId, cx: &mut Context<Self>) {
        let Some(draft) = self.library.drafts().iter().find(|d| d.id() == flow) else {
            return;
        };
        for ((owner, step), inspector) in &self.inspectors {
            if *owner == flow {
                let variables = super::request_source::variables(&draft.document().flow, step);
                inspector.update(cx, |i, cx| i.set_variables(variables, cx));
            }
        }
    }

    fn reorder_step(
        &mut self,
        flow: FlowId,
        id: &str,
        direction: StepDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.library.active().is_some_and(|d| d.id() == flow) {
            return;
        }
        let Some(range) = self.library.reorder_step(flow, id, direction) else {
            return;
        };
        if let Some(scroll) = self.canvas_scroll.get(&flow) {
            // Invalidate only the moved rows; retain the viewport and every editor entity.
            scroll.splice(range.start + 1..range.end + 1, range.len());
        }
        self.refresh_step_variables(flow, cx);
        self.document_changed(window, cx);
        self.reveal_step();
        cx.notify();
    }

    fn remove_step(&mut self, flow: FlowId, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !self.library.active().is_some_and(|d| d.id() == flow) {
            return;
        }
        let Some(removed) = self.library.remove_step(flow, id) else {
            return;
        };
        // Only top-level steps have cached inspectors. Preserve a surviving scoped namesake.
        let survives = |step: &str| {
            self.library
                .active()
                .unwrap()
                .document()
                .flow
                .steps
                .iter()
                .any(|s| s.id == step)
        };
        self.inspectors.retain(|(owner, step), _| {
            *owner != flow || !removed.ids.contains(step) || survives(step)
        });
        self.step_subscriptions.retain(|(owner, step), _| {
            *owner != flow || !removed.ids.contains(step) || survives(step)
        });
        if let Some(scroll) = self.canvas_scroll.get(&flow) {
            scroll.splice(removed.index + 1..removed.index + 2, 0);
        }
        self.sync_fields(window, cx);
        self.refresh_step_variables(flow, cx);
        self.document_changed(window, cx);
        self.reveal_step();
        if self.inspector().is_some() {
            self.focus_inspector(window, cx);
        } else {
            self.add_step_focus.focus(window, cx);
        }
        cx.notify();
    }

    fn open_remove_step(
        &mut self,
        flow: FlowId,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) {
            return;
        }
        let Some(step) = self
            .library
            .active()
            .filter(|d| d.id() == flow)
            .and_then(|d| d.document().flow.steps.iter().find(|s| s.id == id))
        else {
            return;
        };
        let name = if step.name.trim().is_empty() {
            "Untitled step"
        } else {
            &step.name
        }
        .to_owned();
        let nested = matches!(
            step.request,
            postman_flow::HttpRequestSource::ForEach(_)
                | postman_flow::HttpRequestSource::RepeatUntil(_)
        );
        let id = id.to_owned();
        let owner = cx.weak_entity();
        self.remove_step_focus.focus(window, cx);
        window.open_dialog(cx, move |dialog, window, cx| {
            let submit = owner.clone();
            let id = id.clone();
            dialog.title("Remove step?")
                .w((window.rem_size() * 30.).min(window.viewport_size().width - window.rem_size() * 2.))
                .on_ok(|_, _, cx| {
                    // Let the focused Kit button handle Enter; the dialog itself has no default
                    // destructive action. This also preserves Enter on Cancel/Close.
                    cx.propagate();
                    false
                })
                .child(div().debug_selector(|| "flow-remove-dialog".into()).flex().flex_col().gap_3()
                    .child(div().text_size(m::BODY).font_weight(m::SEMIBOLD).child(name.clone()))
                    .child(div().font_family(FONT_MONO).text_size(m::LABEL).text_color(MUTED.resolve(cx)).child(id.clone()))
                    .child(div().text_size(m::LABEL).child(if nested {
                        "This step, its contained steps, and their unfinished edits will be removed. References to them will remain unchanged."
                    } else {
                        "This step and its unfinished edits will be removed. References to it will remain unchanged."
                    })))
                .footer(div().flex().justify_end().gap_2()
                    .child(Button::new("flow-remove-cancel").label("Cancel").on_click(|_, w, cx| w.close_dialog(cx)))
                    .child(Button::new("flow-remove-confirm").label("Remove step").danger()
                        .on_click(move |_, w, cx| {
                            w.close_dialog(cx);
                            let _ = submit.update(cx, |this, cx| this.remove_step(flow, &id, w, cx));
                        })))

        });
    }
}

#[cfg(test)]
mod tests;

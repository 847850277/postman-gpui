use super::*;
use crate::ui::theme::{metrics as m, CODE_BG, ERROR, LINE, MUTED, PANEL};
use gpui::{
    div, prelude::FluentBuilder, AnyElement, InteractiveElement, IntoElement, ParentElement,
    Render, StatefulInteractiveElement, Styled,
};
use gpui_kit::{
    assets::IconName,
    component::{
        button::{Button, ButtonVariants},
        checkbox::Checkbox,
        input::Input,
        Disableable, Selectable, Sizable,
    },
};

pub(super) fn hint(text: impl Into<gpui::SharedString>, error: bool, cx: &App) -> AnyElement {
    div()
        .min_w_0()
        .text_size(m::COMPACT_LABEL)
        .text_color(if error { ERROR } else { MUTED }.resolve(cx))
        .child(text.into())
        .into_any_element()
}
impl InputsInspector {
    fn definition_row(&self, row: &Row, cx: &mut Context<Self>) -> AnyElement {
        let id = row.id;
        div()
            .id(("flow-input-row", id))
            .anchor_scroll(Some(row.anchor.clone()))
            .flex()
            .flex_col()
            .min_w_0()
            .gap_3()
            .pb_4()
            .border_b_1()
            .border_color(LINE.resolve(cx))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Input::new(&row.name)
                            .id(("flow-input-name", id))
                            .aria_label("Input name")
                            .w_full(),
                    )
                    .child(
                        Button::new(("flow-input-remove", id))
                            .icon(IconName::Trash)
                            .accessibility_label("Remove input")
                            .ghost()
                            .small()
                            .on_click(cx.listener(move |this, _, w, cx| this.remove(id, w, cx))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_4()
                    .child(
                        Checkbox::new(("flow-input-has-default", id))
                            .label("Default value")
                            .checked(row.has_default)
                            .small()
                            .on_change(cx.listener(move |this, checked, _, cx| {
                                if let Some(r) = this.rows.iter_mut().find(|r| r.id == id) {
                                    r.has_default = *checked;
                                }
                                this.apply(cx);
                            })),
                    )
                    .child(
                        Checkbox::new(("flow-input-sensitive", id))
                            .label("Sensitive")
                            .checked(row.sensitive)
                            .small()
                            .on_change(cx.listener(move |this, checked, _, cx| {
                                if let Some(r) = this.rows.iter_mut().find(|r| r.id == id) {
                                    r.sensitive = *checked;
                                }
                                this.apply(cx);
                            })),
                    ),
            )
            .when(row.has_default, |d| d.child(row.default.clone()))
            .when(!row.has_default, |d| {
                d.child(hint("Required unless a run value is supplied.", false, cx))
            })
            .when_some(row.error.clone(), |d, e| {
                d.child(hint(
                    format!("{e} Draft retained; last valid definition kept."),
                    true,
                    cx,
                ))
            })
            .into_any_element()
    }
    fn run_row(&self, row: &Row, cx: &mut Context<Self>) -> AnyElement {
        let id = row.id;
        let (inherited, source) = self.inherited(row, cx);
        let sensitive = row.sensitive || row.applied.as_ref().is_some_and(|v| v.sensitive);
        let label = row
            .applied
            .as_ref()
            .map(|v| v.name.clone())
            .unwrap_or_else(|| "Unnamed input".into());
        div()
            .id(("flow-run-input-row", id))
            .flex()
            .flex_col()
            .gap_2()
            .min_w_0()
            .pb_4()
            .border_b_1()
            .border_color(LINE.resolve(cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .child(div().flex_1().min_w_0().child(label))
                    .child(hint(
                        if row.overridden { "Override" } else { source },
                        false,
                        cx,
                    )),
            )
            .when(row.overridden, |d| d.child(row.runtime.clone()))
            .when(!row.overridden, |d| {
                d.child(
                    div()
                        .p_3()
                        .bg(CODE_BG.resolve(cx))
                        .rounded(m::RADIUS)
                        .min_w_0()
                        .child(hint(
                            if sensitive {
                                "Sensitive value hidden".to_owned()
                            } else {
                                inherited
                                    .as_ref()
                                    .map(|v| {
                                        v.as_str()
                                            .map(str::to_owned)
                                            .unwrap_or_else(|| v.to_string())
                                    })
                                    .unwrap_or_else(|| "No value supplied".into())
                            },
                            inherited.is_none(),
                            cx,
                        )),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Checkbox::new(("flow-input-override", id))
                            .label("Override for this session")
                            .checked(row.overridden)
                            .disabled(row.applied.is_none())
                            .small()
                            .on_change(cx.listener(move |this, checked, w, cx| {
                                this.override_value(id, *checked, w, cx)
                            })),
                    )
                    .when(row.overridden, |d| {
                        d.child(
                            Button::new(("flow-input-reset", id))
                                .label("Reset")
                                .ghost()
                                .small()
                                .on_click(cx.listener(move |this, _, w, cx| {
                                    this.override_value(id, false, w, cx)
                                })),
                        )
                    }),
            )
            .when(row.error.is_some(), |d| {
                d.child(hint(
                    "Correct this input in Definitions before preparing a run.",
                    true,
                    cx,
                ))
            })
            .into_any_element()
    }
}
impl Render for InputsInspector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let definitions = self.definitions;
        let heading = div()
            .flex_none()
            .px_4()
            .pt_4()
            .pb_3()
            .flex()
            .items_center()
            .gap_2()
            .child(div().flex_1().font_weight(m::SEMIBOLD).child("Flow inputs"))
            .child(
                Button::new("flow-inputs-back")
                    .label("Step details")
                    .ghost()
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.conceal(cx);
                        cx.emit(InputsEvent::Back);
                    })),
            );
        let tabs = div()
            .flex_none()
            .px_4()
            .pb_3()
            .flex()
            .gap_1()
            .child(
                Button::new("flow-inputs-run-tab")
                    .label("Run inputs")
                    .track_focus(&self.tab_focus[0])
                    .small()
                    .ghost()
                    .selected(!definitions)
                    .on_click(cx.listener(|this, _, _, cx| this.set_definitions(false, cx))),
            )
            .child(
                Button::new("flow-inputs-definitions-tab")
                    .label("Definitions")
                    .track_focus(&self.tab_focus[1])
                    .small()
                    .ghost()
                    .selected(definitions)
                    .on_click(cx.listener(|this, _, _, cx| this.set_definitions(true, cx))),
            );
        let introduction = if definitions {
            "Declare names, defaults, and sensitivity. No default means a required input."
        } else {
            "Prepare values for a future run. Overrides stay separate from document defaults. Environment supplies base_url when declared."
        };
        let footer = if definitions {
            "Renaming or removing inputs leaves existing references unchanged. Sensitive defaults remain in the document; the flag does not encrypt them."
        } else {
            "Session only. Run captures these values; they stay out of saved YAML."
        };
        let fields = div()
            .p_4()
            .flex()
            .flex_col()
            .gap_4()
            .min_w_0()
            .child(hint(introduction, false, cx))
            .when(self.rows.is_empty(), |d| {
                d.child(hint(
                    "No inputs declared. Add an input to make this flow configurable.",
                    false,
                    cx,
                ))
            })
            .children(self.rows.iter().map(|r| {
                if definitions {
                    self.definition_row(r, cx)
                } else {
                    self.run_row(r, cx)
                }
            }))
            .child(
                div().flex().child(
                    Button::new("flow-input-add")
                        .label("Add input")
                        .icon(IconName::Plus)
                        .ghost()
                        .small()
                        .track_focus(&self.add_focus)
                        .on_click(cx.listener(|this, _, w, cx| this.add(w, cx))),
                ),
            )
            .child(hint(footer, false, cx));
        div()
            .id("flow-inputs-inspector")
            .debug_selector(|| "flow-inputs-inspector".into())
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(PANEL.resolve(cx))
            .text_size(m::LABEL)
            .child(heading)
            .child(tabs)
            .child(
                div()
                    .id(("flow-inputs-fields", usize::from(definitions)))
                    .debug_selector(|| "flow-inputs-scroll".into())
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll[usize::from(definitions)])
                    .child(fields),
            )
    }
}

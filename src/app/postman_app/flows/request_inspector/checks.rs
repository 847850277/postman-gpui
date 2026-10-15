//! Stable status-check rows; untouched native checks keep their position and exact value.
use super::view::{field, hint};
use super::*;
use crate::ui::theme::{metrics as m, LINE, MUTED, PANEL_ALT};
use gpui::{
    div, prelude::FluentBuilder, AnyElement, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use gpui_kit::{
    assets::IconName,
    component::{
        button::{Button, ButtonVariants},
        input::Input,
        Sizable,
    },
};
use postman_flow::{validate_response_status, write_response_check, ResponseCheck};

pub(super) struct CheckRow {
    pub(super) id: u64,
    anchor: gpui::ScrollAnchor,
    input: Option<Entity<InputState>>,
    value: Option<ResponseCheck>,
    error: Option<String>,
    readonly_source: String,
    _subscription: Option<Subscription>,
}

impl RequestInspector {
    pub(super) fn check_row(
        id: u64,
        value: Option<ResponseCheck>,
        scroll: &ScrollHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> CheckRow {
        let editable = value
            .as_ref()
            .is_none_or(|v| matches!(v, ResponseCheck::StatusEquals(_)));
        let input = editable.then(|| {
            cx.new(|cx| {
                let mut input = InputState::new(window, cx).placeholder("No status check");
                if let Some(ResponseCheck::StatusEquals(code)) = &value {
                    input.set_value(code.to_string(), window, cx);
                }
                input
            })
        });
        let subscription = input.as_ref().map(|input| {
            cx.subscribe(input, move |this, input, event, cx| {
                if this.readonly.is_some() || !matches!(event, InputEvent::Change) {
                    return;
                }
                let Some(row) = this.checks.iter_mut().find(|r| r.id == id) else {
                    return;
                };
                let source = input.read(cx).value();
                let parsed = parse_status(&source);
                row.error = parsed.as_ref().err().cloned();
                match parsed {
                    Ok(value) => {
                        row.value = value.map(ResponseCheck::StatusEquals);
                        this.emit_checks(cx);
                    }
                    Err(_) => cx.emit(StepEdit::Invalid),
                }
                cx.notify();
            })
        });
        let readonly_source = value
            .as_ref()
            .map(|v| write_response_check(v).expect("serializable check"))
            .unwrap_or_default();
        CheckRow {
            id,
            anchor: gpui::ScrollAnchor::for_handle(scroll.clone()),
            input,
            value,
            error: None,
            readonly_source,
            _subscription: subscription,
        }
    }
    fn emit_checks(&self, cx: &mut Context<Self>) {
        cx.emit(StepEdit::Checks(
            self.checks.iter().filter_map(|r| r.value.clone()).collect(),
        ));
    }
    fn add_check(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.readonly.is_some() {
            return;
        }
        let id = self.allocate_row();
        let row = Self::check_row(id, None, &self.tab_scroll[1], window, cx);
        row.input
            .as_ref()
            .unwrap()
            .focus_handle(cx)
            .focus(window, cx);
        row.anchor.scroll_to(window, cx);
        self.checks.push(row);
        cx.emit(StepEdit::Invalid);
        cx.notify();
    }
    fn remove_check(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.readonly.is_some() {
            return;
        }
        let Some(ix) = self
            .checks
            .iter()
            .position(|r| r.id == id && r.input.is_some())
        else {
            return;
        };
        self.checks.remove(ix);
        let next = self
            .checks
            .iter()
            .skip(ix)
            .chain(self.checks.iter().take(ix).rev())
            .find_map(|r| r.input.as_ref());
        next.map(|input| input.focus_handle(cx))
            .unwrap_or_else(|| self.add_check_focus.clone())
            .focus(window, cx);
        self.emit_checks(cx);
        cx.notify();
    }
    pub(super) fn checks_content(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_4()
            .min_w_0()
            .child(hint(
                "Configure response checks for this step. Checks are evaluated when the flow runs.",
                false,
                cx,
            ))
            .children(self.checks.iter().map(|row| {
                let id = row.id;
                div()
                    .id(("flow-check-row", id))
                    .anchor_scroll(Some(row.anchor.clone()))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .border_1()
                    .border_color(LINE.resolve(cx))
                    .bg(PANEL_ALT.resolve(cx))
                    .rounded(m::RADIUS)
                    .min_w_0()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().flex_1().text_size(m::LABEL).child(
                                if row.input.is_some() {
                                    "Response status"
                                } else {
                                    "Native check · Readonly"
                                },
                            ))
                            .when(row.input.is_some() && self.readonly.is_none(), |d| {
                                d.child(
                                    Button::new(("flow-remove-check", id))
                                        .icon(IconName::Trash)
                                        .ghost()
                                        .small()
                                        .accessibility_label("Remove status check")
                                        .on_click(cx.listener(move |this, _, w, cx| {
                                            this.remove_check(id, w, cx)
                                        })),
                                )
                            }),
                    )
                    .when_some(
                        row.input.as_ref().filter(|_| self.readonly.is_none()),
                        |d, input| {
                            d.child(field(
                                "Expected status code",
                                Input::new(input)
                                    .id(("flow-check-status", id))
                                    .aria_label("Expected status code")
                                    .w_full(),
                                cx,
                            ))
                        },
                    )
                    .when(row.input.is_none() || self.readonly.is_some(), |d| {
                        d.child(hint(row.readonly_source.clone(), false, cx))
                    })
                    .when_some(row.error.clone(), |d, e| {
                        d.child(hint(
                            format!("{e} Draft retained; check not applied."),
                            true,
                            cx,
                        ))
                    })
            }))
            .when(self.readonly.is_none(), |d| {
                d.child(
                    div().flex().items_start().child(
                        Button::new("flow-add-check")
                            .label("Add status check")
                            .icon(IconName::Plus)
                            .ghost()
                            .small()
                            .track_focus(&self.add_check_focus)
                            .on_click(cx.listener(|this, _, w, cx| this.add_check(w, cx))),
                    ),
                )
            })
            .child(
                div()
                    .text_size(m::CAPTION)
                    .text_color(MUTED.resolve(cx))
                    .child("Other check kinds are preserved as readonly definitions."),
            )
            .into_any_element()
    }
}

fn parse_status(source: &str) -> Result<Option<u16>, String> {
    if source.is_empty() {
        return Ok(None);
    }
    if !source.bytes().all(|c| c.is_ascii_digit()) {
        return Err("Use a whole decimal status code from 100 to 599.".into());
    }
    let value = source
        .parse::<u16>()
        .map_err(|_| "Use a status code from 100 to 599.".to_string())?;
    validate_response_status(value).map_err(str::to_owned)?;
    Ok(Some(value))
}

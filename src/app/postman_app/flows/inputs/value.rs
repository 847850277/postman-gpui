//! Retained, explicitly typed value editing. Concealment never changes the source buffer.
use crate::ui::{
    components::input::multiline_input::{MultilineInput, MultilineInputEvent},
    theme::{metrics as m, MUTED},
};
use gpui::{
    div, prelude::FluentBuilder, AppContext, Context, Entity, EventEmitter, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, SharedString, Styled, Subscription,
    Window,
};
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    Disableable, Selectable, Sizable,
};
use serde_json::Value;

pub(super) struct Changed;
impl EventEmitter<Changed> for ValueEditor {}

pub(super) struct ValueEditor {
    pub(super) input: Entity<MultilineInput>,
    key: SharedString,
    json: bool,
    pub(super) parsed: Result<Value, &'static str>,
    sensitive: bool,
    revealed: bool,
    _subscription: Subscription,
}
impl ValueEditor {
    pub(super) fn new(
        key: String,
        value: Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let json = !value.is_string();
        let source = encode(&value, json);
        let input = cx.new(|cx| {
            let mut input = MultilineInput::new("Input value", "Value", window, cx);
            input.project_content(source, window, cx);
            input
        });
        let subscription = cx.subscribe(&input, |this, _, event, cx| {
            let MultilineInputEvent::ValueChanged(source) = event;
            this.parsed = parse(source, this.json);
            cx.emit(Changed);
            cx.notify();
        });
        Self {
            input,
            key: key.into(),
            json,
            parsed: Ok(value),
            sensitive: false,
            revealed: false,
            _subscription: subscription,
        }
    }
    pub(super) fn set_value(&mut self, value: Value, window: &mut Window, cx: &mut Context<Self>) {
        self.json = !value.is_string();
        self.input.update(cx, |i, cx| {
            i.project_content(encode(&value, self.json), window, cx)
        });
        self.parsed = Ok(value);
        cx.notify();
    }
    pub(super) fn sensitive(&mut self, sensitive: bool, cx: &mut Context<Self>) {
        if self.sensitive != sensitive {
            self.sensitive = sensitive;
            self.conceal(cx);
        }
    }
    pub(super) fn conceal(&mut self, cx: &mut Context<Self>) {
        self.revealed = false;
        cx.notify();
    }
    fn switch(&mut self, json: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.json == json {
            return;
        }
        let Ok(value) = &self.parsed else {
            return;
        };
        if !json && !value.is_string() {
            return;
        }
        self.input.update(cx, |i, cx| {
            i.project_content(encode(value, json), window, cx)
        });
        self.json = json;
        cx.notify();
    }
}
fn encode(value: &Value, json: bool) -> String {
    if json {
        serde_json::to_string_pretty(value).expect("JSON value")
    } else {
        value.as_str().unwrap_or_default().to_owned()
    }
}
pub(super) fn parse(source: &str, json: bool) -> Result<Value, &'static str> {
    if source.len() > 2 * 1024 * 1024 {
        return Err("Value exceeds 2 MiB.");
    }
    if json {
        // Same bounded serde JSON parser as the document codec. Never echo source in diagnostics.
        postman_flow::parse_input_value(source)
            .map_err(|_| "Enter a valid JSON value. Correct JSON before changing modes.")
    } else {
        Ok(Value::String(source.to_owned()))
    }
}
impl Render for ValueEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let hidden = self.sensitive && !self.revealed;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .min_w_0()
            .child(
                div()
                    .flex()
                    .gap_1()
                    .items_center()
                    .child(
                        Button::new(SharedString::from(format!("{}-text", self.key)))
                            .label("Text")
                            .xsmall()
                            .ghost()
                            .selected(!self.json)
                            .tooltip(
                                "Text mode requires a JSON string; other JSON types keep JSON mode",
                            )
                            .disabled(
                                hidden || self.parsed.as_ref().map_or(true, |v| !v.is_string()),
                            )
                            .on_click(cx.listener(|this, _, w, cx| this.switch(false, w, cx))),
                    )
                    .child(
                        Button::new(SharedString::from(format!("{}-json", self.key)))
                            .label("JSON")
                            .xsmall()
                            .ghost()
                            .selected(self.json)
                            .disabled(hidden || self.parsed.is_err())
                            .on_click(cx.listener(|this, _, w, cx| this.switch(true, w, cx))),
                    )
                    .child(div().flex_1())
                    .when(self.sensitive, |d| {
                        d.child(
                            Button::new(SharedString::from(format!("{}-reveal", self.key)))
                                .label(if hidden { "Reveal / edit" } else { "Hide" })
                                .small()
                                .ghost()
                                .on_click(cx.listener(|this, _, w, cx| {
                                    this.revealed = !this.revealed;
                                    if this.revealed {
                                        this.input.focus_handle(cx).focus(w, cx);
                                    }
                                    cx.notify();
                                })),
                        )
                    }),
            )
            .when(hidden, |d| {
                d.child(
                    div()
                        .py_2()
                        .text_color(MUTED.resolve(cx))
                        .child("Sensitive value hidden"),
                )
            })
            .when(!hidden, |d| {
                d.child(
                    div()
                        .id(self.key.clone())
                        .debug_selector({
                            let key = self.key.clone();
                            move || key.to_string()
                        })
                        .h_24()
                        .min_w_0()
                        .child(self.input.clone())
                        .test_support(),
                )
            })
            .when_some(self.parsed.as_ref().err(), |d, error| {
                d.child(super::view::hint(*error, true, cx))
            })
            .text_size(m::LABEL)
    }
}

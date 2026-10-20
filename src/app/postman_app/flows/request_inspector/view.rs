use super::*;
use crate::ui::theme::{
    metrics as m, ACCENT, ACCENT_SOFT, ERROR, FONT_MONO, MUTED, PANEL, SUBTEXT,
};
use gpui::{
    div, prelude::FluentBuilder, rems, AnyElement, InteractiveElement, IntoElement, ParentElement,
    Render, StatefulInteractiveElement, Styled,
};
use gpui_kit::{
    assets::IconName,
    component::{
        button::{Button, ButtonVariants},
        collapsible::Collapsible,
        input::Input,
        select::Select,
        Disableable, Icon, Selectable, Sizable,
    },
};

pub(super) fn field(label: &'static str, control: impl IntoElement, cx: &App) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .min_w_0()
        .child(
            div()
                .text_size(m::COMPACT_LABEL)
                .text_color(SUBTEXT.resolve(cx))
                .child(label),
        )
        .child(control)
        .into_any_element()
}
pub(super) fn hint(text: impl Into<gpui::SharedString>, error: bool, cx: &App) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
        .text_size(m::COMPACT_LABEL)
        .text_color(if error { ERROR } else { MUTED }.resolve(cx))
        .child(text.into())
        .into_any_element()
}
fn source(id: &'static str, editor: &Entity<MultilineInput>, height: f32) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .w_full()
        .min_w_0()
        .h(rems(height))
        .flex_none()
        .child(editor.clone())
        .into_any_element()
}

impl RequestInspector {
    fn url_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let modes = div()
            .flex()
            .gap_1()
            .child(
                Button::new("flow-url-literal")
                    .label("Literal")
                    .xsmall()
                    .ghost()
                    .selected(!self.expression_url)
                    .disabled(self.expression_url && !self.simple_url_available)
                    .on_click(cx.listener(|this, _, w, cx| this.switch_url(false, w, cx))),
            )
            .child(
                Button::new("flow-url-expression")
                    .label("Expression")
                    .xsmall()
                    .ghost()
                    .selected(self.expression_url)
                    .on_click(cx.listener(|this, _, w, cx| this.switch_url(true, w, cx))),
            );
        // Leave room for OPTIONS, the caret/checkmark, and Kit's control/menu padding.
        let method = div().w(rems(7.5)).flex_none().child(field(
            "Method",
            div().debug_selector(|| "flow-step-method".into()).child(
                Select::new(&self.method)
                    .id("flow-step-method")
                    .accessibility_label("Step HTTP method")
                    .w_full(),
            ),
            cx,
        ));
        let url = div()
            .w_full()
            .min_w_0()
            .debug_selector(|| "flow-step-url".into())
            .child(field(
                "URL",
                Input::new(&self.url)
                    .id("flow-step-url")
                    .aria_label("Step URL")
                    .font_family(FONT_MONO)
                    .text_size(m::CODE)
                    .w_full(),
                cx,
            ));
        div()
            .flex()
            .flex_col()
            .gap_2()
            .min_w_0()
            // The inspector can be only 16.25rem wide. Sharing a row with
            // Method leaves too little room to read the URL's scheme/host.
            .child(method)
            .when(!self.expression_url, |d| d.child(url))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(hint(
                        if self.expression_url {
                            "URL expression"
                        } else {
                            "URL source"
                        },
                        false,
                        cx,
                    ))
                    .child(modes),
            )
            .when(self.expression_url, |d| {
                d.child(source("flow-url-source", &self.url_expression, 7.))
                    .child(hint(
                        "Native text expression. References are kept unresolved.",
                        false,
                        cx,
                    ))
            })
            .when(!self.expression_url, |d| {
                let value = self.url.read(cx).value();
                let invalid = !value.is_empty()
                    && !reqwest::Url::parse(&value).is_ok_and(|u| {
                        matches!(u.scheme(), "http" | "https") && u.host_str().is_some()
                    });
                d.when(invalid, |d| {
                    d.child(hint(
                        "Enter a complete HTTP or HTTPS URL. This draft is kept as typed.",
                        true,
                        cx,
                    ))
                })
            })
            .when_some(self.url_error.clone(), |d, error| {
                d.child(hint(
                    format!("{error} Draft retained; URL not applied."),
                    true,
                    cx,
                ))
            })
            .when(
                self.expression_url && !self.simple_url_available && self.url_error.is_none(),
                |d| d.child(hint("This expression requires Expression mode.", false, cx)),
            )
            .into_any_element()
    }
    fn headers_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let content = div()
            .pt_2()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        Button::new("flow-headers-literal")
                            .label("Literal")
                            .xsmall()
                            .ghost()
                            .selected(!self.expression_headers)
                            .disabled(self.expression_headers && !self.simple_headers_available)
                            .on_click(
                                cx.listener(|this, _, w, cx| this.switch_headers(false, w, cx)),
                            ),
                    )
                    .child(
                        Button::new("flow-headers-expression")
                            .label("Expression")
                            .xsmall()
                            .ghost()
                            .selected(self.expression_headers)
                            .disabled(self.headers_error.is_some())
                            .on_click(
                                cx.listener(|this, _, w, cx| this.switch_headers(true, w, cx)),
                            ),
                    ),
            )
            .child(source(
                "flow-headers-source",
                if self.expression_headers {
                    &self.header_expression
                } else {
                    &self.headers
                },
                8.,
            ))
            .child(hint(
                if self.expression_headers {
                    "Ordered {name, value} pairs using native text expressions."
                } else {
                    "One Name: value per line. Duplicates and empty values are kept."
                },
                false,
                cx,
            ))
            .when_some(self.headers_error.clone(), |d, error| {
                d.child(hint(
                    format!("{error} Draft retained; headers not applied."),
                    true,
                    cx,
                ))
            });
        Collapsible::new()
            .open(self.headers_open)
            .child(
                div().flex().items_start().child(
                    Button::new("flow-headers-toggle")
                        .label(format!(
                            "Headers · {}{}",
                            self.header_count,
                            if self.headers_error.is_some() {
                                " · Draft error"
                            } else {
                                ""
                            }
                        ))
                        .icon(if self.headers_open {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .xsmall()
                        .ghost()
                        .accessibility_label(if self.headers_open {
                            "Collapse headers"
                        } else {
                            "Expand headers"
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.headers_open = !this.headers_open;
                            cx.notify();
                        })),
                ),
            )
            .content(content)
            .into_any_element()
    }
}

impl RequestInspector {
    fn request_content(&self, cx: &mut Context<Self>) -> AnyElement {
        div().flex().flex_col().gap_5().min_w_0()
            .child(field("Step name", div().debug_selector(|| "flow-step-name".into()).child(Input::new(&self.name).id("flow-step-name").aria_label("Step name").w_full()), cx))
            .when(self.name.read(cx).value().trim().is_empty(), |d| d.child(hint("Enter a step name. The unfinished draft is retained.", true, cx)))
            .child(div().debug_selector(|| "flow-step-id".into()).text_size(m::CAPTION).font_family(FONT_MONO).text_color(MUTED.resolve(cx)).child(self.step_id.clone()))
            .when_some(self.readonly.clone(), |d, description| d.child(div().debug_selector(|| "flow-step-readonly".into()).child(field("Request source", hint(description, false, cx), cx)))
                .child(hint("Configuration is read-only for API references and loop steps. Their contents are preserved.", false, cx)))
            .when(self.readonly.is_none(), |d| d
                .child(div().id(("flow-field-anchor", 0usize)).anchor_scroll(Some(self.request_anchors[0].clone())).child(self.url_control(cx)))
                .child(div().flex().flex_col().gap_2().child(hint("AVAILABLE VARIABLES", false, cx))
                    .when(self.variables.is_empty(), |d| d.child(hint("No declared inputs or preceding step exports.", false, cx)))
                    .child(div().flex().flex_wrap().gap_1().children(self.variables.iter().map(|name| div().px_2().py_1().rounded(m::RADIUS).bg(ACCENT_SOFT.resolve(cx)).text_color(ACCENT.resolve(cx)).font_family(FONT_MONO).text_size(m::CAPTION).child(name.clone())))))
                .child(div().id(("flow-field-anchor", 1usize)).anchor_scroll(Some(self.request_anchors[1].clone())).child(self.headers_control(cx)))
                .child(div().id(("flow-field-anchor", 2usize)).anchor_scroll(Some(self.request_anchors[2].clone())).child(field("BODY EXPRESSION", source("flow-body-source", &self.body, 11.), cx)))
                .when_some(self.body_error.clone(), |d, error| d.child(hint(format!("{error} Draft retained; body not applied."), true, cx)))
                .child(hint("JSON expressions preserve types and references. Empty means no body. Shape checks do not compile or run the flow.", false, cx)))
            .child(div().id(("flow-field-anchor", 3usize)).anchor_scroll(Some(self.request_anchors[3].clone())).child(self.condition_control(cx))).into_any_element()
    }
    fn condition_control(&self, cx: &mut Context<Self>) -> AnyElement {
        Collapsible::new().open(self.condition_open)
            .child(div().flex().items_start().child(Button::new("flow-condition-toggle")
                .label(format!("Run condition · {}{}", if self.condition_enabled { "Enabled" } else { "Optional" }, if self.condition_error.is_some() { " · Draft error" } else { "" }))
                .icon(if self.condition_open { IconName::ChevronDown } else { IconName::ChevronRight })
                .ghost().xsmall().accessibility_label("Run condition")
                .on_click(cx.listener(|this, _, _, cx| { this.condition_open = !this.condition_open; cx.notify(); }))))
            .content(div().pt_2().flex().flex_col().gap_2().min_w_0()
                .when(self.readonly.is_none(), |d| d.child(source("flow-condition-source", &self.condition, 9.)))
                .when(self.readonly.is_some(), |d| d.child(hint(self.condition.read(cx).value(), false, cx)))
                .when_some(self.condition_error.clone(), |d, error| d.child(hint(format!("{error} Draft retained; condition not applied."), true, cx)))
                .child(hint("Native condition expression. Blank always runs the step. References and operand types are checked when compiling the flow.", false, cx)))
            .into_any_element()
    }
}

impl Render for RequestInspector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let fields = match self.tab {
            1 => self.checks_content(cx),
            2 => self.exports_content(cx),
            _ => self.request_content(cx),
        };
        div()
            .id("flow-inspector")
            .debug_selector(|| "flow-inspector".into())
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(PANEL.resolve(cx))
            .child(
                div()
                    .flex_none()
                    .px_4()
                    .pt_4()
                    .pb_3()
                    .flex()
                    .gap_2()
                    .text_size(m::LABEL)
                    .font_weight(m::SEMIBOLD)
                    .child(Icon::new(IconName::Terminal).size(m::SMALL_ICON))
                    .child("Step configuration"),
            )
            .child(self.tabs(cx))
            .child(
                div()
                    .id(("flow-inspector-fields", self.tab))
                    .debug_selector(|| "flow-inspector-scroll".into())
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.tab_scroll[self.tab])
                    .child(div().p_4().child(fields)),
            )
    }
}

//! Native presentation of the retained result inspector.
use super::*;
use crate::ui::theme::{metrics as m, ERROR, LINE, MUTED, OK, PANEL_ALT, WARNING};
use gpui_kit::{
    assets::IconName,
    component::{
        button::{Button, ButtonVariants},
        input::Editor,
        switch::Switch,
        Disableable, Selectable, Sizable,
    },
};
use gpui_kit::{
    div, prelude::FluentBuilder, AnyElement, InteractiveElement, IntoElement, ParentElement, Styled,
};

impl FlowConsole {
    pub(in super::super) fn run_results(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.current().unwrap();
        let run = session.run.as_ref().unwrap();
        let Some(i) = &session.inspector else {
            return div().into_any_element();
        };
        let results = &run.results;
        let outputs = session.view == View::Outputs;
        let value = i.selected_value(results);
        let original = i.showing_original();
        let http = i
            .values
            .get(&false)
            .filter(|_| !outputs)
            .and_then(|v| v.http);
        let scope = if outputs {
            None
        } else {
            i.scope.and_then(|id| results.nodes.get(id))
        };
        let title = if let Some(value) = value {
            format!(
                "{}{} · {}{}",
                if http.is_some() {
                    scope.map_or(String::new(), |n| format!("{} · ", n.path))
                } else {
                    String::new()
                },
                value.name,
                if original { "JSON" } else { value.value.kind() },
                if original {
                    " · Original"
                } else if value.value.redacted_paths().is_empty() {
                    ""
                } else {
                    " · Redacted"
                }
            )
        } else if let Some(node) = scope {
            format!("{} · {}", node.path, node.status.label())
        } else if outputs {
            "Final outputs".into()
        } else {
            "Steps".into()
        };
        let description = if let Some(value) = value {
            let paths = value.value.redacted_paths();
            format!(
                "{}{}",
                match http {
                    Some(HttpPart::Request) => "Readonly JSON · Resolved request, before automatic headers, cookies and redirects. Query shown separately.",
                    Some(HttpPart::Response) => "Readonly JSON · Final response; large bodies and binary contents omitted.",
                    None => "Readonly JSON · Copy value includes only this sanitized value.",
                },
                if original {
                    if value.value.original_json().is_some() {
                        " · Sensitive values shown for this run".into()
                    } else {
                        " · Original unavailable (size limit)".into()
                    }
                } else if paths.is_empty() {
                    String::new()
                } else if paths.iter().any(String::is_empty) {
                    " · Whole value redacted".into()
                } else {
                    format!(
                        " · Redacted JSON paths: {}{}",
                        paths
                            .iter()
                            .take(3)
                            .map(|s| s.chars().take(80).collect::<String>())
                            .collect::<Vec<_>>()
                            .join(", "),
                        if paths.len() > 3 {
                            format!(" (+{} more)", paths.len() - 3)
                        } else {
                            String::new()
                        }
                    )
                }
            )
        } else if let Some(n) = scope {
            format!(
                "{} · Checks {}/{} · Extractions {}/{}{}",
                format_args!(
                    "{}{}",
                    n.summary(),
                    match i.rows.get(i.selected) {
                        Some(Row::Check(index)) => format!(
                            " · Check {} · {} · {}",
                            index + 1,
                            if n.checks[*index].success {
                                "Passed"
                            } else {
                                "Failed"
                            },
                            n.checks[*index].detail
                        ),
                        _ => String::new(),
                    }
                ),
                n.check_count,
                n.expected_checks,
                n.value_count,
                n.expected_values,
                if n.http_omitted
                    || n.check_count > n.checks.len()
                    || n.value_count > n.values.len()
                    || results.total_rows > results.nodes.len()
                {
                    " · Some details omitted (retention limit)"
                } else {
                    ""
                }
            )
        } else if outputs {
            if run.status.active() {
                "Run in progress; final outputs are not available yet.".into()
            } else if !results.complete {
                "No final returns. Partial extractions remain in Steps.".into()
            } else {
                format!(
                    "{} declared output(s) · {} retained{}",
                    results.output_count,
                    results.outputs.len(),
                    if results.output_count > results.outputs.len() {
                        " · Retention limit; complete copy unavailable"
                    } else {
                        ""
                    }
                )
            }
        } else {
            format!(
                "{} of {} observed result rows retained{}",
                results.nodes.len(),
                results.total_rows,
                if results.total_rows > results.nodes.len() {
                    " · Retention limit; later details omitted"
                } else {
                    ""
                }
            )
        };
        let back = value.is_some() || scope.is_some();
        let copy = value.and_then(|v| v.value.json());
        let original_json = value.and_then(|v| v.value.original_json());
        let original_available = original_json.is_some();
        let original_copy = original
            .then_some(original_json)
            .flatten()
            .map(str::to_owned);
        let flow = self.active.unwrap();
        let revision = Some(run.revision);
        div()
            .id("flow-run-results")
            .size_full()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .key_context("FlowResultInspector")
            .track_focus(&i.focus)
            .on_action(cx.listener(|this, _: &ResultBack, w, cx| this.result_back(w, cx)))
            .on_action(cx.listener(|this, _: &ResultPrevious, w, cx| this.result_move(-1, w, cx)))
            .on_action(cx.listener(|this, _: &ResultNext, w, cx| this.result_move(1, w, cx)))
            .on_action(cx.listener(|this, _: &ResultOpen, w, cx| {
                if let Some(i) = this.current().and_then(|s| s.inspector.as_ref()) {
                    let index = i.selected;
                    this.result_open(index, w, cx);
                }
            }))
            .child(
                div()
                    .flex_none()
                    .px_4()
                    .py_1()
                    .border_b_1()
                    .border_color(LINE.resolve(cx))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .min_w_0()
                            .child(
                                Button::new("result-back")
                                    .ghost()
                                    .small()
                                    .icon(IconName::ChevronLeft)
                                    .label("Back")
                                    .disabled(!back)
                                    .on_click(
                                        cx.listener(|this, _, w, cx| this.result_back(w, cx)),
                                    ),
                            )
                            .child(div().flex_1().min_w_0().truncate().child(title))
                            .when_some(scope, |d, n| {
                                let owner = n.owner.clone();
                                d.child(
                                    Button::new("result-locate")
                                        .ghost()
                                        .small()
                                        .label("Locate")
                                        .disabled(run.revision != session.revision)
                                        .on_click(cx.listener(move |_, _, _, cx| {
                                            cx.emit(ConsoleEvent::LocateStep {
                                                flow,
                                                revision,
                                                step: owner.clone(),
                                            })
                                        })),
                                )
                            })
                            .when(value.is_some(), |d| {
                                d.child(
                                    Button::new("result-copy-value")
                                        .ghost()
                                        .small()
                                        .label(if http.is_some() { "Copy sanitized" } else { "Copy value" })
                                        .disabled(copy.is_none())
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(copy) = &copy {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    copy.clone(),
                                                ));
                                                this.hint =
                                                    "Selected value copied as sanitized JSON."
                                                        .into();
                                                cx.notify();
                                            }
                                        })),
                                )
                            }),
                    )
                    .when(http.is_some() && value.is_some(), |d| {
                        d.child(
                            div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap_2()
                                .min_h_8()
                                .py_1()
                                .child(
                                    Switch::new("result-show-sensitive")
                                        .small()
                                        .label("Show sensitive values")
                                        .checked(i.show_sensitive)
                                        .disabled(!original_available && !i.show_sensitive)
                                        .tooltip(if original_available {
                                            "Applies to HTTP details in this run; resets on the next run"
                                        } else {
                                            "Original unavailable: inspection size limit exceeded"
                                        })
                                        .on_change(cx.listener(|this, visible, w, cx| {
                                            this.set_sensitive_visible(*visible, w, cx);
                                        })),
                                )
                                .when(original, |d| {
                                    d.child(
                                        Button::new("result-copy-original")
                                            .ghost()
                                            .small()
                                            .label("Copy original")
                                            .disabled(!original_available)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                if let Some(copy) = &original_copy {
                                                    cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()));
                                                    this.hint = "Original HTTP details copied, including sensitive values.".into();
                                                    cx.notify();
                                                }
                                            })),
                                    )
                                }),
                        )
                    })
                    .child(
                        div()
                            .text_size(m::CAPTION)
                            .text_color(MUTED.resolve(cx))
                            .whitespace_normal()
                            .child(description),
                    ),
            )
            .map(|d| {
                if value.is_some() {
                    d.child(
                        div().flex_1().min_h_0().min_w_0().child(
                            Editor::new(i.editor())
                                .readonly(true)
                                .aria_label(if original { "Original HTTP JSON" } else { "Sanitized result JSON" })
                                .size_full(),
                        ),
                    )
                } else if i.rows.is_empty() {
                    d.child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(MUTED.resolve(cx))
                            .child(if scope.is_some() {
                                "No child results available."
                            } else if outputs && results.complete {
                                "No declared outputs."
                            } else {
                                "No results available."
                            }),
                    )
                } else {
                    d.child(
                        div()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .min_w_0()
                            .child(
                                gpui_kit::uniform_list(
                                    "result-list",
                                    i.rows.len(),
                                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                                        range
                                            .map(|index| this.result_row(index, cx))
                                            .collect::<Vec<_>>()
                                    }),
                                )
                                .track_scroll(i.scrolls.get(&i.key()).unwrap())
                                .size_full(),
                            )
                            .child(gpui_kit::component::scroll::Scrollbar::vertical(
                                i.scrolls.get(&i.key()).unwrap(),
                            )),
                    )
                }
            })
            .into_any_element()
    }
    fn result_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let session = self.current().unwrap();
        let run = session.run.as_ref().unwrap();
        let i = session.inspector.as_ref().unwrap();
        let (title, state, detail, color) = match i.rows[index] {
            Row::Node(id) => {
                let n = &run.results.nodes[id];
                (
                    n.name.clone(),
                    n.status.label().to_owned(),
                    n.summary(),
                    super::super::super::execution::status_color(n.status),
                )
            }
            Row::Check(id) => {
                let c = &run.results.nodes[i.scope.unwrap()].checks[id];
                (
                    format!("Check {}", id + 1),
                    if c.success { "Passed" } else { "Failed" }.into(),
                    c.detail.clone(),
                    if c.success { OK } else { ERROR },
                )
            }
            Row::Value(_) | Row::Http(_) => {
                let v = match i.rows[index] {
                    Row::Value(id) if i.showing_outputs => &run.results.outputs[id],
                    Row::Value(id) => &run.results.nodes[i.scope.unwrap()].values[id],
                    Row::Http(part) => run.results.nodes[i.scope.unwrap()].http[part as usize]
                        .as_ref()
                        .unwrap(),
                    _ => unreachable!(),
                };
                (
                    v.name.clone(),
                    v.value.kind().into(),
                    if v.value.is_truncated() {
                        "Unavailable · Size limit"
                    } else if v.value.redacted_paths().is_empty() {
                        "Inspect value"
                    } else {
                        "Inspect sanitized value · Redacted"
                    }
                    .into(),
                    if v.value.is_truncated() || !v.value.redacted_paths().is_empty() {
                        WARNING
                    } else {
                        MUTED
                    },
                )
            }
        };
        div()
            .h_10()
            .min_w_0()
            .px_4()
            .border_b_1()
            .border_color(LINE.resolve(cx))
            .when(i.selected == index, |d| d.bg(PANEL_ALT.resolve(cx)))
            .child(
                Button::new(gpui_kit::SharedString::from(format!("result-row-{index}")))
                    .ghost()
                    .small()
                    .w_full()
                    .h_full()
                    .justify_start()
                    .selected(i.selected == index)
                    .accessibility_label(format!("{title} · {state} · {detail}"))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .w_full()
                            .min_w_0()
                            .child(div().flex_1().min_w_0().truncate().child(title))
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(m::CAPTION)
                                    .text_color(color.resolve(cx))
                                    .child(state),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(MUTED.resolve(cx))
                                    .child(detail),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, w, cx| this.result_open(index, w, cx))),
            )
            .into_any_element()
    }
}

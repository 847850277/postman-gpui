use super::*;
use crate::ui::theme::{
    metrics as m, ACCENT, ERROR, FONT_MONO, INFO, LINE, MUTED, OK, PANEL, PANEL_ALT, TEXT, WARNING,
};
use gpui::{
    div, list, prelude::FluentBuilder, rems, AnyElement, InteractiveElement, IntoElement,
    ParentElement, Render, Role, StatefulInteractiveElement, Styled,
};
use gpui_kit::{
    assets::IconName,
    base::{Tab, Tabs},
    component::{
        button::{Button, ButtonVariants},
        input::Input,
        select::Select,
        Disableable, Icon, Selectable, Sizable,
    },
};

impl FlowConsole {
    fn heading(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.current().map_or(View::Logs, |s| s.view);
        let tabs = Tabs::new("console-tabs")
            .flex()
            .h_full()
            .gap_4()
            .ml_2()
            .aria_label("Console view")
            .children(View::ALL.into_iter().enumerate().map(|(ix, view)| {
                Tab::new(view.id())
                    .selected(selected == view)
                    .set_position(ix + 1, 3)
                    .accessibility_label(view.label())
                    .track_focus(&self.tab_focus[ix])
                    .key_context("FlowConsoleTabs")
                    .h_full()
                    .flex()
                    .items_center()
                    .border_b_2()
                    .border_color(if selected == view {
                        ACCENT.resolve(cx)
                    } else {
                        gpui::rgba(0)
                    })
                    .text_size(m::COMPACT_LABEL)
                    .text_color(if selected == view {
                        ACCENT.resolve(cx)
                    } else {
                        MUTED.resolve(cx)
                    })
                    .focus_visible(|d| d.border_1().border_color(ACCENT.resolve(cx)))
                    .on_click(cx.listener(move |this, _, w, c| this.navigate_tab(ix, w, c)))
                    .on_action(cx.listener(|this, _: &PreviousTab, w, c| this.move_tab(-1, w, c)))
                    .on_action(cx.listener(|this, _: &NextTab, w, c| this.move_tab(1, w, c)))
                    .on_action(cx.listener(|this, _: &FirstTab, w, c| this.navigate_tab(0, w, c)))
                    .on_action(cx.listener(|this, _: &LastTab, w, c| this.navigate_tab(2, w, c)))
                    .on_action(
                        cx.listener(move |this, _: &ActivateTab, w, c| this.navigate_tab(ix, w, c)),
                    )
                    .child(view.label())
            }));
        div()
            .debug_selector(|| "console-heading".into())
            .h(rems(HEADER_HEIGHT))
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .border_b_1()
            .border_color(LINE.resolve(cx))
            .child(Icon::new(IconName::Terminal).size(m::SMALL_ICON))
            .child(div().font_weight(m::SEMIBOLD).child("Console"))
            .child(
                div()
                    .text_size(m::CAPTION)
                    .text_color(MUTED.resolve(cx))
                    .child("Flow commands"),
            )
            .child(tabs)
            .child(div().flex_1())
            .child(
                div()
                    .text_size(m::CAPTION)
                    .text_color(MUTED.resolve(cx))
                    .child("Not run"),
            )
            .child(
                Button::new("console-maximize")
                    .accessibility_label(if self.mode == ConsoleMode::Maximized {
                        "Restore console size"
                    } else {
                        "Maximize console"
                    })
                    .ghost()
                    .small()
                    .icon(if self.mode == ConsoleMode::Maximized {
                        IconName::Minimize
                    } else {
                        IconName::Maximize
                    })
                    .tooltip(if self.mode == ConsoleMode::Maximized {
                        "Restore console size"
                    } else {
                        "Maximize console"
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.set_mode(
                            if this.mode == ConsoleMode::Maximized {
                                ConsoleMode::Normal
                            } else {
                                ConsoleMode::Maximized
                            },
                            cx,
                        )
                    })),
            )
            .child(
                Button::new("console-toggle")
                    .accessibility_label(if self.mode == ConsoleMode::Collapsed {
                        "Expand console"
                    } else {
                        "Collapse console"
                    })
                    .ghost()
                    .small()
                    .icon(if self.mode == ConsoleMode::Collapsed {
                        IconName::ChevronUp
                    } else {
                        IconName::ChevronDown
                    })
                    .tooltip(if self.mode == ConsoleMode::Collapsed {
                        "Expand console"
                    } else {
                        "Collapse console"
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.mode != ConsoleMode::Collapsed {
                            let selected = this.current().map_or(View::Logs, |s| s.view);
                            this.tab_focus[selected as usize].focus(window, cx);
                        }
                        this.set_mode(
                            if this.mode == ConsoleMode::Collapsed {
                                ConsoleMode::Normal
                            } else {
                                ConsoleMode::Collapsed
                            },
                            cx,
                        )
                    })),
            )
            .into_any_element()
    }
    fn tools(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.current();
        let logs = session.is_none_or(|s| s.view == View::Logs);
        let count = session.map_or(0, |s| s.logs.len());
        div()
            .debug_selector(|| "console-tools".into())
            .h(rems(TOOLS_HEIGHT))
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .border_b_1()
            .border_color(LINE.resolve(cx))
            .when(logs, |d| {
                d.child(
                    div().w_32().h_6().flex_none().child(
                        Select::new(&self.filter)
                            .id("console-level")
                            .accessibility_label("Log level")
                            .small()
                            .disabled(session.is_none()),
                    ),
                )
                .child(
                    div()
                        .text_size(m::CAPTION)
                        .text_color(MUTED.resolve(cx))
                        .child(format!("{} of {count} events", self.visible.len())),
                )
            })
            .when(!logs, |d| {
                d.child(div().text_color(MUTED.resolve(cx)).child("Not run"))
            })
            .child(div().flex_1())
            .when(logs, |d| {
                d.child(
                    Button::new("console-follow")
                        .ghost()
                        .small()
                        .label("Auto-scroll")
                        .icon(IconName::ArrowDown)
                        .selected(session.is_some_and(|s| s.follow))
                        .disabled(session.is_none())
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_follow(cx))),
                )
            })
            .child(
                Button::new("console-copy")
                    .accessibility_label("Copy visible logs")
                    .ghost()
                    .small()
                    .icon(IconName::Copy)
                    .tooltip("Copy visible logs")
                    .disabled(!logs || self.visible.is_empty())
                    .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
            )
            .when(logs, |d| {
                d.child(
                    Button::new("console-clear")
                        .ghost()
                        .small()
                        .label("Clear")
                        .disabled(count == 0)
                        .on_click(cx.listener(|this, _, _, cx| this.clear(cx))),
                )
            })
            .into_any_element()
    }
    fn content(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.current();
        let view = session.map_or(View::Logs, |s| s.view);
        let (title, description) = match session {
            None => (
                "Create a flow to use Console",
                "Your commands and logs stay with each flow during this session.",
            ),
            Some(_) if view == View::Steps => (
                "No step results yet",
                "Flow execution is not available yet.",
            ),
            Some(_) if view == View::Outputs => (
                "No flow outputs yet",
                "Outputs will appear here when execution is available.",
            ),
            Some(s) if s.cleared => ("Logs cleared", "Step results and command history are kept."),
            Some(s) if !s.logs.is_empty() && self.visible.is_empty() => (
                "No matching events",
                "Choose another log level to see more events.",
            ),
            _ => (
                "Ready for Flow commands",
                "Type help to explore available commands. Flow execution is not available yet.",
            ),
        };
        div()
            .id("console-content")
            .debug_selector(|| "console-content".into())
            .role(Role::TabPanel)
            .aria_label(view.label())
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .map(|d| {
                if view == View::Logs && !self.visible.is_empty() {
                    d.child(
                        list(
                            self.scroll.clone(),
                            cx.processor(|this, ix, _, cx| this.log_row(ix, cx)),
                        )
                        .size_full(),
                    )
                } else {
                    d.child(
                        div()
                            .size_full()
                            .flex()
                            .flex_col()
                            .justify_center()
                            .items_center()
                            .gap_1()
                            .px_4()
                            .child(div().child(title))
                            .child(
                                div()
                                    .text_size(m::COMPACT_LABEL)
                                    .text_color(MUTED.resolve(cx))
                                    .child(description),
                            ),
                    )
                }
            })
            .into_any_element()
    }
    fn log_row(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(entry) = self
            .visible
            .get(ix)
            .and_then(|id| self.current()?.logs.iter().find(|entry| entry.id == *id))
        else {
            return div().into_any_element();
        };
        let color = match entry.level {
            Level::Error => ERROR,
            Level::Warning => WARNING,
            Level::Success => OK,
            Level::Info => INFO,
            Level::Command => MUTED,
        };
        div()
            .id(gpui::SharedString::from(format!(
                "console-event-{}-{}",
                self.active.unwrap().0,
                entry.id
            )))
            .w_full()
            .flex()
            .items_start()
            .gap_3()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(LINE.resolve(cx))
            .min_w_0()
            .font_family(FONT_MONO)
            .text_size(m::COMPACT_LABEL)
            .line_height(gpui::relative(1.6))
            .child(
                div()
                    .w_24()
                    .flex_none()
                    .text_color(MUTED.resolve(cx))
                    .child(entry.time.clone()),
            )
            .child(
                div()
                    .w_16()
                    .flex_none()
                    .text_size(m::CAPTION)
                    .text_color(color.resolve(cx))
                    .child(entry.level.label()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .when_some(entry.step.clone().zip(self.active), |d, (step, flow)| {
                        d.child(
                            Button::new(gpui::SharedString::from(format!(
                                "console-step-{}-{}",
                                flow.0, entry.id
                            )))
                            .ghost()
                            .small()
                            .label(step.clone())
                            .icon(IconName::ArrowRight)
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if this.mode == ConsoleMode::Maximized {
                                        this.set_mode(ConsoleMode::Normal, cx);
                                    }
                                    cx.emit(ConsoleEvent::LocateStep {
                                        flow,
                                        step: step.clone(),
                                    });
                                },
                            )),
                        )
                    })
                    .child(entry.message.clone()),
            )
            .into_any_element()
    }
    fn command_bar(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let disabled = self.active.is_none();
        let focused = self.input.focus_handle(cx).is_focused(window);
        div()
            .debug_selector(|| "console-command-bar".into())
            .relative()
            .h(rems(COMMAND_HEIGHT))
            .flex_none()
            .flex()
            .flex_col()
            .justify_center()
            .px_4()
            .border_t_1()
            .border_color(LINE.resolve(cx))
            .when(focused, |d| {
                d.bg(PANEL_ALT.resolve(cx)).child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(rems(0.125))
                        .bg(ACCENT.resolve(cx)),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .child(
                        div()
                            .font_family(FONT_MONO)
                            .text_color(ACCENT.resolve(cx))
                            .child("flow ›"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .debug_selector(|| "console-command".into())
                            .child(
                                Input::new(&self.input)
                                    .id("console-command")
                                    .aria_label("Flow command")
                                    .font_family(FONT_MONO)
                                    .text_size(m::CODE)
                                    .appearance(false)
                                    .small()
                                    .w_full()
                                    .disabled(disabled),
                            ),
                    )
                    .child(
                        Button::new("console-execute")
                            .accessibility_label("Execute Flow command")
                            .ghost()
                            .small()
                            .icon(IconName::ArrowRight)
                            .tooltip("Execute Flow command")
                            .disabled(disabled || self.input.read(cx).value().trim().is_empty())
                            .on_click(cx.listener(|this, _, w, cx| this.submit(None, w, cx))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(m::CAPTION)
                    .text_color(MUTED.resolve(cx))
                    .child(div().flex_1().min_w_0().truncate().child(self.hint.clone()))
                    .child(
                        Button::new("console-help")
                            .ghost()
                            .xsmall()
                            .label("help")
                            .font_family(FONT_MONO)
                            .text_size(m::CAPTION)
                            .text_color(ACCENT.resolve(cx))
                            .disabled(disabled)
                            .on_click(
                                cx.listener(|this, _, w, cx| this.submit(Some("help"), w, cx)),
                            ),
                    ),
            )
            .into_any_element()
    }
}
impl Render for FlowConsole {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.rem != window.rem_size() {
            self.rem = window.rem_size();
            self.scroll.remeasure();
        }
        div()
            .id("flow-console")
            .debug_selector(|| "flow-results".into())
            .size_full()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .bg(PANEL.resolve(cx))
            .text_color(TEXT.resolve(cx))
            .text_size(m::COMPACT_LABEL)
            .child(self.heading(cx))
            .when(self.mode != ConsoleMode::Collapsed, |d| {
                d.child(self.tools(cx))
                    .child(self.content(cx))
                    .child(self.command_bar(window, cx))
            })
    }
}

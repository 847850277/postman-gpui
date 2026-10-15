use super::{
    layout::SplitKind,
    model::{self, request_summary},
    FirstStep, FlowsView, LastStep, NextFlow, NextStep, PreviousFlow, PreviousStep,
};
use crate::ui::{
    components::kit_controls,
    theme::{
        self, metrics as m, ACCENT, ACCENT_SOFT, ERROR, FONT_MONO, INFO, LINE, LINE_STRONG, MUTED,
        PANEL, PANEL_ALT, SIDEBAR, SUBTEXT, TEXT,
    },
};
use gpui::Focusable;
use gpui::{
    div, list, prelude::FluentBuilder, rems, uniform_list, AnyElement, Context, InteractiveElement,
    IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled, Window,
};
use gpui_kit::{
    assets::IconName,
    component::{
        button::{Button, ButtonVariants},
        input::Input,
        select::Select,
        Disableable, Icon, Selectable, Sizable,
    },
};
use postman_flow::HttpRequestSource;

impl FlowsView {
    fn render_topbar(&self, compact: bool, cx: &mut Context<Self>) -> AnyElement {
        let active = self.library.active();
        let invalid_name = active.and_then(|d| model::validate_name(&d.document().flow.name).err());
        div()
            .debug_selector(|| "flow-topbar".into())
            .flex_none()
            .flex()
            .items_center()
            .gap_5()
            .px_6()
            .py_4()
            .border_b_1()
            .border_color(LINE.resolve(cx))
            .when(compact, |d| d.flex_col().items_stretch().gap_2())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(m::CAPTION)
                            .text_color(MUTED.resolve(cx))
                            .child(if active.is_some() {
                                "Flows / Session draft"
                            } else {
                                "Flows"
                            }),
                    )
                    .map(|d| {
                        if active.is_some() {
                            d.child(
                                div().debug_selector(|| "flow-title".into()).child(
                                    Input::new(&self.title)
                                        .id("flow-title")
                                        .px_0()
                                        .aria_label("Flow name")
                                        .appearance(false)
                                        .h_9()
                                        .w_full()
                                        .text_size(rems(22. / 16.))
                                        .font_weight(m::SEMIBOLD),
                                ),
                            )
                        } else {
                            d.child(
                                div()
                                    .text_size(m::TITLE)
                                    .font_weight(m::SEMIBOLD)
                                    .child("Your flows"),
                            )
                        }
                    })
                    .when_some(invalid_name, |d, error| {
                        d.child(
                            div()
                                .text_size(m::CAPTION)
                                .text_color(ERROR.resolve(cx))
                                .child(error),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .flex_none()
                    .child(self.environments.clone())
                    .child(
                        Button::new("flow-export")
                            .small()
                            .label("Export")
                            .disabled(true)
                            .tooltip("Flow file export is not available yet"),
                    )
                    .child(
                        Button::new("flow-check")
                            .small()
                            .label("Check")
                            .disabled(true)
                            .tooltip("Flow validation is not available yet"),
                    )
                    .child(
                        Button::new("flow-run")
                            .small()
                            .label("Run")
                            .icon(IconName::Play)
                            .disabled(true)
                            .tooltip("Flow execution is not available yet"),
                    ),
            )
            .into_any_element()
    }
    fn render_library(&self, cx: &mut Context<Self>) -> AnyElement {
        let filtered = self.filtered(cx);
        let empty = self.library.drafts().is_empty();
        let count = filtered.len();
        div().id("flow-library").debug_selector(|| "flow-library".into()).size_full().min_w_0().min_h_0().flex().flex_col().bg(SIDEBAR.resolve(cx))
            .child(div().px_3().pt_3().pb_2().flex().items_center().justify_between()
                .child(div().text_size(m::LABEL).font_weight(m::SEMIBOLD).child("Your flows"))
                .child(kit_controls::icon_button("new-flow",IconName::Plus,"New flow…").on_click(cx.listener(|this,_,w,c| this.open_create(w,c)))))
            .child(div().px_3().pb_3().debug_selector(|| "flow-filter".into()).child(Input::new(&self.filter).id("flow-filter").small().aria_label("Find a flow").prefix(IconName::Search).w_full()))
            .child(div().id("flow-library-list").debug_selector(|| "flow-library-list".into()).flex_1().min_h_0().min_w_0().track_focus(&self.library_focus).key_context("FlowLibrary")
                .on_action(cx.listener(|this,_:&PreviousFlow,w,c| this.move_flow(-1,w,c)))
                .on_action(cx.listener(|this,_:&NextFlow,w,c| this.move_flow(1,w,c)))
                .map(|d| if count==0 {
                    d.child(div().p_4().text_size(m::LABEL).text_color(MUTED.resolve(cx)).child(if empty { "No flows yet. Create one to start." } else { "No matching flows." }))
                } else {
                    d.child(uniform_list("flow-list", count, cx.processor(move |this, range:std::ops::Range<usize>, _, cx| {
                        range.filter_map(|i| {
                            let id = *filtered.get(i)?;
                            let draft = this.library.drafts().iter().find(|d|d.id()==id)?;
                            let selected=this.library.active().is_some_and(|d|d.id()==id);
                            let title=draft.title().to_owned();
                            let subtitle=format!("{} steps{}",draft.document().flow.steps.len(),if draft.revision()>0 { " · Edited" } else { " · Draft" });
                            Some(div().h_16().px_2().py_1().child(Button::new(gpui::SharedString::from(format!("flow-row-{}",id.0))).debug_selector(move||format!("flow-row-{}",id.0)).accessibility_label(format!("Open flow {title}")).ghost().selected(selected).w_full().h_full().min_w_0().px_2()
                                .when(selected,|b|b.bg(ACCENT_SOFT.resolve(cx)))
                                .on_click(cx.listener(move|this,_,w,c|{this.select_flow(id,w,c);this.library_focus.focus(w,c);} ))
                                .child(div().w_full().min_w_0().flex().items_center().gap_2()
                                    .child(Icon::new(IconName::Workflow).size(m::SMALL_ICON).text_color(INFO.resolve(cx)))
                                    .child(div().flex_1().min_w_0().flex().flex_col().gap_1()
                                        .child(div().w_full().truncate().text_size(m::LABEL).child(title))
                                        .child(div().w_full().truncate().text_size(m::CAPTION).text_color(MUTED.resolve(cx)).child(subtitle))))))
                        }).collect::<Vec<_>>()
                    })).track_scroll(&self.library_scroll).size_full())
                }))
            .child(div().p_3().border_t_1().border_color(LINE.resolve(cx)).text_size(m::CAPTION).text_color(MUTED.resolve(cx)).child("Drafts last for this session"))
            .into_any_element()
    }
    fn render_toolbar(
        &self,
        show_library: bool,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.library.active();
        div()
            .debug_selector(|| "flow-toolbar".into())
            .flex_none()
            .min_h(rems(51. / 16.))
            .px_4()
            .py_2()
            .flex()
            .items_center()
            .gap_3()
            .border_b_1()
            .border_color(LINE.resolve(cx))
            .when(!show_library, |d| {
                d.child(
                    div()
                        .w(rems(if compact { 10. } else { 14. }))
                        .min_w_0()
                        .debug_selector(|| "flow-picker".into())
                        .child(
                            Select::new(&self.picker)
                                .id("flow-picker")
                                .placeholder("Choose flow")
                                .accessibility_label("Choose flow")
                                .small()
                                .w_full(),
                        ),
                )
                .child(
                    kit_controls::icon_button("new-flow-compact", IconName::Plus, "New flow…")
                        .on_click(cx.listener(|this, _, w, c| this.open_create(w, c))),
                )
            })
            .when(!compact, |d| {
                d.child(
                    Button::new("flow-canvas-tab")
                        .small()
                        .label("Canvas")
                        .selected(true),
                )
                .child(
                    Button::new("flow-yaml-tab")
                        .small()
                        .ghost()
                        .label("YAML")
                        .disabled(true)
                        .tooltip("YAML preview is not available yet"),
                )
            })
            .when(show_library, |d| {
                d.child(
                    div()
                        .text_size(m::CAPTION)
                        .text_color(MUTED.resolve(cx))
                        .child(format!(
                            "{} steps · Sequential",
                            active.map_or(0, |d| d.document().flow.steps.len())
                        )),
                )
            })
            .child(div().flex_1())
            .when(!compact, |d| {
                d.child(
                    Button::new("flow-inputs")
                        .small()
                        .ghost()
                        .label("Inputs")
                        .disabled(true)
                        .tooltip("Input configuration is not available yet"),
                )
            })
            .child(
                Button::new("add-step")
                    .small()
                    .label("Add step…")
                    .icon(IconName::Plus)
                    .disabled(active.is_none())
                    .on_click(cx.listener(|this, _, w, c| this.open_add_step(w, c))),
            )
            .when(compact, |d| {
                d.child(
                    Button::new("flow-toggle-details")
                        .small()
                        .label(if self.details { "Canvas" } else { "Details" })
                        .disabled(active.is_none())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.details = !this.details;
                            cx.notify();
                        })),
                )
            })
            .into_any_element()
    }
    fn render_canvas(&self, cx: &mut Context<Self>) -> AnyElement {
        let dot_color = LINE_STRONG.resolve(cx).opacity(0.45);
        let root = div()
            .relative()
            .id("flow-canvas")
            .debug_selector(|| "flow-canvas".into())
            .track_focus(&self.canvas_focus)
            .key_context("FlowCanvas")
            .size_full()
            .min_h_0()
            .min_w_0()
            .bg(PANEL_ALT.resolve(cx))
            .child(
                gpui::canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let spacing = window.rem_size() * 1.5;
                        let dot = window.rem_size() / 16.;
                        let mut y = bounds.top() + spacing / 2.;
                        while y < bounds.bottom() {
                            let mut x = bounds.left() + spacing / 2.;
                            while x < bounds.right() {
                                window.paint_quad(gpui::fill(
                                    gpui::Bounds::new(gpui::point(x, y), gpui::size(dot, dot)),
                                    dot_color,
                                ));
                                x += spacing;
                            }
                            y += spacing;
                        }
                    },
                )
                .absolute()
                .inset_0()
                .size_full(),
            )
            .on_action(cx.listener(|this, _: &PreviousStep, w, c| this.move_step(-1, w, c)))
            .on_action(cx.listener(|this, _: &NextStep, w, c| this.move_step(1, w, c)))
            .on_action(cx.listener(|this, _: &FirstStep, w, c| this.move_step(isize::MIN, w, c)))
            .on_action(cx.listener(|this, _: &LastStep, w, c| this.move_step(isize::MAX, w, c)));
        let Some(draft) = self.library.active() else {
            return root
                .child(
                    div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap_3()
                        .p_6()
                        .child(
                            Icon::new(IconName::Workflow)
                                .size(rems(2.))
                                .text_color(MUTED.resolve(cx)),
                        )
                        .child(
                            div()
                                .text_size(m::BODY)
                                .font_weight(m::SEMIBOLD)
                                .child("Create your first flow"),
                        )
                        .child(
                            div()
                                .text_size(m::LABEL)
                                .text_color(MUTED.resolve(cx))
                                .child("Arrange HTTP requests into a sequence."),
                        )
                        .child(
                            Button::new("flow-empty-create")
                                .label("New flow…")
                                .on_click(cx.listener(|this, _, w, c| this.open_create(w, c))),
                        ),
                )
                .into_any_element();
        };
        let scroll = self.canvas_scroll[&draft.id()].clone();
        root.child(
            list(
                scroll,
                cx.processor(|this, index, _, cx| this.render_chain_item(index, cx)),
            )
            .size_full(),
        )
        .into_any_element()
    }
    fn render_chain_item(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(draft) = self.library.active() else {
            return div().into_any_element();
        };
        let steps = &draft.document().flow.steps;
        let content = if index == 0 {
            div()
                .pt_6()
                .child(chain_cap(
                    "Flow inputs",
                    draft.document().flow.inputs.len(),
                    cx,
                ))
                .child(connector(cx))
                .into_any_element()
        } else if index <= steps.len() {
            let step = &steps[index - 1];
            let id = step.id.clone();
            let flow = draft.id();
            let selected = draft.selected().is_some_and(|s| s.id == step.id);
            let (method, url) = request_summary(step);
            let name = if step.name.trim().is_empty() {
                "Untitled step"
            } else {
                &step.name
            };
            div()
                .child(
                    Button::new(gpui::SharedString::from(format!(
                        "flow-node-{}-{}",
                        flow.0, id
                    )))
                    .debug_selector({
                        let id = id.clone();
                        move || format!("flow-node-{}-{}", flow.0, id)
                    })
                    .accessibility_label(format!("Select step {}: {name}", index))
                    .tooltip(format!("{name}\n{url}"))
                    .selected(selected)
                    .w_full()
                    .min_w_0()
                    .h(rems(112. / 16.))
                    .p_4()
                    .rounded_lg()
                    .bg(PANEL.resolve(cx))
                    .border_1()
                    .border_color(if selected { ACCENT } else { LINE_STRONG }.resolve(cx))
                    .on_click(cx.listener(move |this, _, w, c| {
                        this.select_step(&id, w, c);
                        this.details = true;
                        if w.viewport_size().width / w.rem_size() <= 50. {
                            this.step_name.focus_handle(c).focus(w, c);
                        } else {
                            this.canvas_focus.focus(w, c);
                        }
                        c.notify();
                    }))
                    .child(
                        div()
                            .size_full()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .items_stretch()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .size(rems(22. / 16.))
                                            .flex_none()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded(m::RADIUS)
                                            .border_1()
                                            .border_color(LINE.resolve(cx))
                                            .font_family(FONT_MONO)
                                            .text_size(m::CAPTION)
                                            .text_color(MUTED.resolve(cx))
                                            .child(format!("{index:02}")),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .text_size(m::LABEL)
                                            .font_weight(m::SEMIBOLD)
                                            .child(name.to_owned()),
                                    )
                                    .child(
                                        div()
                                            .text_size(m::CAPTION)
                                            .text_color(MUTED.resolve(cx))
                                            .child(if method.is_some() { "HTTP" } else { "DSL" }),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .min_w_0()
                                    .font_family(FONT_MONO)
                                    .text_size(m::CAPTION)
                                    .when_some(method, |d, method| {
                                        d.child(
                                            div()
                                                .flex_none()
                                                .text_color(theme::method_color(method).resolve(cx))
                                                .child(method.to_string()),
                                        )
                                    })
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .text_color(MUTED.resolve(cx))
                                            .child(url),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(m::CAPTION)
                                    .text_color(MUTED.resolve(cx))
                                    .child(format!(
                                        "{} checks · {} exports{}",
                                        step.checks.len(),
                                        step.exports.len(),
                                        if step.when.is_some() {
                                            " · Conditional"
                                        } else {
                                            ""
                                        }
                                    )),
                            ),
                    ),
                )
                .child(connector(cx))
                .into_any_element()
        } else if index == steps.len() + 1 {
            div()
                .child(
                    Button::new("canvas-add-step")
                        .ghost()
                        .label("Add a step…")
                        .icon(IconName::Plus)
                        .w_full()
                        .border_1()
                        .border_color(LINE.resolve(cx))
                        .on_click(cx.listener(|this, _, w, c| this.open_add_step(w, c))),
                )
                .child(connector(cx))
                .into_any_element()
        } else {
            div()
                .child(chain_cap(
                    "Flow outputs",
                    draft.document().flow.outputs.len(),
                    cx,
                ))
                .pb_6()
                .into_any_element()
        };
        div()
            .px_5()
            .w_full()
            .child(div().max_w(rems(22.5)).mx_auto().child(content))
            .into_any_element()
    }
    fn render_inspector(&self, cx: &mut Context<Self>) -> AnyElement {
        let step = self.library.active().and_then(|d| d.selected());
        let root = div()
            .id("flow-inspector")
            .debug_selector(|| "flow-inspector".into())
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.inspector_scroll)
            .bg(PANEL.resolve(cx));
        let Some(step) = step else {
            return root
                .child(
                    div()
                        .p_4()
                        .text_size(m::LABEL)
                        .text_color(MUTED.resolve(cx))
                        .child("Add a request to start your flow."),
                )
                .into_any_element();
        };
        let request = match &step.request {
            HttpRequestSource::Inline(r) => Some(r),
            _ => None,
        };
        let literal = request.and_then(|r| model::literal_url(&r.url));
        let url_error = literal.filter(|url| !url.is_empty()).and_then(|url| {
            let valid = reqwest::Url::parse(url).is_ok_and(|url| {
                matches!(url.scheme(), "http" | "https") && url.host_str().is_some()
            });
            (!valid).then_some("Enter a complete HTTP or HTTPS URL. This draft is kept as typed.")
        });
        let method = div().w(rems(5.75)).flex_none().child(field(
            "Method",
            div()
                .debug_selector(|| "flow-step-method".into())
                .child(
                    Select::new(&self.method)
                        .id("flow-step-method")
                        .accessibility_label("Step HTTP method")
                        .disabled(request.is_none())
                        .w_full(),
                )
                .into_any_element(),
            cx,
        ));
        let source = if literal.is_some() {
            div()
                .flex_1()
                .min_w_0()
                .child(field(
                    "URL",
                    div()
                        .debug_selector(|| "flow-step-url".into())
                        .child(
                            Input::new(&self.url)
                                .id("flow-step-url")
                                .aria_label("Step URL")
                                .font_family(FONT_MONO)
                                .text_size(m::CODE)
                                .w_full(),
                        )
                        .into_any_element(),
                    cx,
                ))
                .into_any_element()
        } else {
            div()
                .flex_1()
                .min_w_0()
                .debug_selector(|| "flow-step-readonly".into())
                .child(field(
                    "Request source",
                    div()
                        .font_family(FONT_MONO)
                        .text_size(m::CAPTION)
                        .child(
                            request
                                .map(|r| model::url_expression(&r.url))
                                .unwrap_or_else(|| request_summary(step).1),
                        )
                        .into_any_element(),
                    cx,
                ))
                .into_any_element()
        };
        let request_fields = div().p_4().flex().flex_col().gap_4()
            .child(field("Step name",
                div().debug_selector(|| "flow-step-name".into()).child(
                    Input::new(&self.step_name).id("flow-step-name").aria_label("Step name").w_full()
                ).into_any_element(), cx,
            ))
            .child(div().debug_selector(|| "flow-step-id".into()).font_family(FONT_MONO)
                .text_size(m::CAPTION).text_color(MUTED.resolve(cx)).child(step.id.clone()))
            .when(step.name.trim().is_empty(), |d| d.child(hint("Enter a step name. The unfinished draft is retained.", true, cx)))
            .child(div().flex().items_start().gap_3().min_w_0().child(method).child(source))
            .when_some(url_error, |d,e| d.child(hint(e, true, cx)))
            .when(literal.is_none(), |d| d.child(hint("This request source is preserved. Expression and advanced request editing are not available yet.", false, cx)))
            .child(hint("Body, headers, conditions, checks and exports are preserved. Their editors are not available yet.", false, cx));
        root.child(
            div()
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
        .child(
            div()
                .px_4()
                .flex()
                .border_b_1()
                .border_color(LINE.resolve(cx))
                .child(
                    gpui_kit::base::Tab::new("flow-request-tab")
                        .selected(true)
                        .h_10()
                        .flex()
                        .items_center()
                        .border_b_2()
                        .border_color(ACCENT.resolve(cx))
                        .text_color(ACCENT.resolve(cx))
                        .text_size(m::LABEL)
                        .child("Request"),
                ),
        )
        .child(request_fields)
        .into_any_element()
    }
}
impl Render for FlowsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The Flow surface fills the shell beside its fixed rail. Scale the prototype breakpoints
        // with rem so controls retain their minimum usable widths at application zoom.
        if self.canvas_rem != window.rem_size() {
            self.canvas_rem = window.rem_size();
            for scroll in self.canvas_scroll.values() {
                scroll.remeasure();
            }
        }
        let width = window.viewport_size().width / window.rem_size() - 4.5;
        let compact = width <= 45.5;
        let show_library = width > 67.375;
        let canvas = self.render_canvas(cx);
        let inspector = self.render_inspector(cx);
        let center = if compact {
            if self.details {
                inspector
            } else {
                canvas
            }
        } else {
            self.render_panel_split(SplitKind::Inspector, canvas, inspector, cx)
        };
        let results = self.console.clone().into_any_element();
        let stage = match self.console.read(cx).mode() {
            super::console::ConsoleMode::Normal => {
                self.render_panel_split(SplitKind::Results, center, results, cx)
            }
            super::console::ConsoleMode::Maximized => results,
            super::console::ConsoleMode::Collapsed => div()
                .size_full()
                .min_h_0()
                .flex()
                .flex_col()
                .child(div().flex_1().min_h_0().child(center))
                .child(
                    div()
                        .h(rems(super::console::HEADER_HEIGHT))
                        .flex_none()
                        .child(results),
                )
                .into_any_element(),
        };
        let work = div()
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.render_toolbar(show_library, compact, cx))
            .child(div().flex_1().min_h_0().min_w_0().child(stage))
            .into_any_element();
        let layout = if show_library {
            let library = self.render_library(cx);
            self.render_panel_split(SplitKind::Library, library, work, cx)
        } else {
            work
        };
        div()
            .id("flows-screen")
            .debug_selector(|| "flows-screen".into())
            .track_focus(&self.focus)
            .key_context("Flows")
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(PANEL.resolve(cx))
            .text_color(TEXT.resolve(cx))
            .child(self.render_topbar(compact, cx))
            .child(div().flex_1().min_w_0().min_h_0().child(layout))
    }
}
fn field(label: &'static str, control: AnyElement, cx: &gpui::App) -> AnyElement {
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
fn hint(text: &'static str, error: bool, cx: &gpui::App) -> AnyElement {
    div()
        .text_size(m::COMPACT_LABEL)
        .text_color(if error { ERROR } else { MUTED }.resolve(cx))
        .child(text)
        .into_any_element()
}
fn connector(cx: &gpui::App) -> AnyElement {
    div()
        .h_7()
        .flex()
        .items_center()
        .justify_center()
        .text_color(LINE_STRONG.resolve(cx))
        .child(Icon::new(IconName::ArrowDown).size(m::SMALL_ICON))
        .into_any_element()
}
fn chain_cap(label: &'static str, count: usize, cx: &gpui::App) -> AnyElement {
    div()
        .flex()
        .justify_center()
        .child(
            div()
                .px_3()
                .py_2()
                .rounded_full()
                .border_1()
                .border_color(LINE.resolve(cx))
                .bg(PANEL.resolve(cx))
                .font_family(FONT_MONO)
                .text_size(m::CAPTION)
                .text_color(MUTED.resolve(cx))
                .child(format!("{label}  {count}")),
        )
        .into_any_element()
}

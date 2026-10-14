//! Kit owns drag geometry. This owner adds per-region preferences and keyboard parity.
use super::FlowsView;
use crate::ui::theme::{metrics as m, ACCENT, MUTED};
use gpui::{
    actions, div, prelude::FluentBuilder, px, Along, AnyElement, AppContext, Axis, Bounds, Context,
    Entity, FocusHandle, InteractiveElement, IntoElement, KeyBinding, MouseButton, ParentElement,
    Pixels, Role, StatefulInteractiveElement, Styled, Window,
};
use gpui_kit::{
    base::ElementExt,
    component::{
        button::{Button, ButtonVariants},
        input::{InputState, NumberInput},
        resizable::{
            h_resizable, resizable_panel, resize_handle_appearance, v_resizable, ResizableState,
        },
        WindowExt,
    },
};
use std::{cell::Cell, rc::Rc};

actions!(
    flow_split,
    [Shrink, Grow, ShrinkFast, GrowFast, Minimum, Maximum, Cancel, Sizes]
);
#[derive(Clone, Copy)]
pub(super) enum SplitKind {
    Library,
    Inspector,
    Results,
}
impl SplitKind {
    fn ix(self) -> usize {
        self as usize
    }
    fn id(self) -> &'static str {
        match self {
            Self::Library => "flow-library-split",
            Self::Inspector => "flow-inspector-split",
            Self::Results => "flow-results-split",
        }
    }
    fn handle(self) -> &'static str {
        match self {
            Self::Library => "flow-library-divider",
            Self::Inspector => "flow-inspector-divider",
            Self::Results => "flow-results-divider",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Library => "Flow library width",
            Self::Inspector => "Step details width",
            Self::Results => "Console height",
        }
    }
    fn default_size(self) -> f32 {
        match self {
            Self::Library => 12.25,
            Self::Inspector => 18.75,
            Self::Results => 15.,
        }
    }
    fn axis(self) -> Axis {
        if matches!(self, Self::Results) {
            Axis::Vertical
        } else {
            Axis::Horizontal
        }
    }
    fn is_trailing(self) -> bool {
        !matches!(self, Self::Library)
    }
}

pub(super) fn bind_keys(cx: &mut Context<FlowsView>) {
    cx.bind_keys([
        KeyBinding::new("left", Shrink, Some("FlowSplit")),
        KeyBinding::new("up", Shrink, Some("FlowSplit")),
        KeyBinding::new("right", Grow, Some("FlowSplit")),
        KeyBinding::new("down", Grow, Some("FlowSplit")),
        KeyBinding::new("shift-left", ShrinkFast, Some("FlowSplit")),
        KeyBinding::new("shift-up", ShrinkFast, Some("FlowSplit")),
        KeyBinding::new("shift-right", GrowFast, Some("FlowSplit")),
        KeyBinding::new("shift-down", GrowFast, Some("FlowSplit")),
        KeyBinding::new("home", Minimum, Some("FlowSplit")),
        KeyBinding::new("end", Maximum, Some("FlowSplit")),
        KeyBinding::new("escape", Cancel, Some("FlowSplit")),
        KeyBinding::new("enter", Sizes, Some("FlowSplit")),
    ]);
}

pub(super) struct FlowSplit {
    state: Entity<ResizableState>,
    kind: SplitKind,
    preferred: f32,
    rem: f32,
    bounds: Bounds<Pixels>,
    restore: bool,
    focus: FocusHandle,
    drag_origin: Option<f32>,
    handle_bounds: Rc<Cell<Bounds<Pixels>>>,
}
impl FlowSplit {
    pub(super) fn new(kind: SplitKind, cx: &mut Context<FlowsView>) -> Self {
        Self {
            state: cx.new(|_| ResizableState::default()),
            kind,
            preferred: kind.default_size(),
            rem: 16.,
            bounds: Bounds::default(),
            restore: false,
            focus: cx.focus_handle().tab_stop(true),
            drag_origin: None,
            handle_bounds: Rc::new(Cell::new(Bounds::default())),
        }
    }
    fn total(&self) -> f32 {
        (self.bounds.size.along(self.kind.axis()).as_f32() / self.rem).max(1.)
    }
    fn limits(&self) -> (f32, f32) {
        let (min, max, other) = match self.kind {
            SplitKind::Library => (10.5_f32, 21.25_f32, 38.75_f32),
            SplitKind::Inspector => (16.25, f32::MAX, 18.75),
            SplitKind::Results => {
                let chrome = super::console::HEADER_HEIGHT
                    + super::console::TOOLS_HEIGHT
                    + super::console::COMMAND_HEIGHT;
                let min = (chrome + 3.).min(self.total() * 0.6);
                return (
                    min,
                    (self.total() - 11.25_f32.min(self.total() * 0.4)).max(min),
                );
            }
        };
        let min = min.min(self.total() * 0.4);
        (
            min,
            max.min(self.total() - other.min(self.total() * 0.55))
                .max(min),
        )
    }
    fn displayed(&self, cx: &gpui::App) -> f32 {
        self.state
            .read(cx)
            .sizes()
            .get(usize::from(self.kind.is_trailing()))
            .map_or(self.preferred, |n| n.as_f32() / self.rem)
    }
    fn apply(&self, window: &mut Window, cx: &mut Context<FlowsView>) {
        let (min, max) = self.limits();
        let size = self.preferred.clamp(min, max);
        let first = if self.kind.is_trailing() {
            self.total() - size
        } else {
            size
        };
        self.state.update(cx, |state, cx| {
            state.resize_panel(0, px(first * self.rem), window, cx)
        });
    }
}

impl FlowsView {
    fn set_panel(
        &mut self,
        kind: SplitKind,
        value: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let split = &mut self.splits[kind.ix()];
        let (min, max) = split.limits();
        split.preferred = value.clamp(min, max);
        split.apply(window, cx);
        cx.notify();
    }
    fn adjust_panel(
        &mut self,
        kind: SplitKind,
        delta: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let split = &self.splits[kind.ix()];
        let sign = if kind.is_trailing() { -1. } else { 1. };
        self.set_panel(kind, split.displayed(cx) + delta * sign, window, cx);
    }
    fn panel_sizes(&mut self, kind: SplitKind, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        let split = &self.splits[kind.ix()];
        let (min, max) = split.limits();
        let rem = split.rem;
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(format!("{:.0}", split.displayed(cx) * rem))
                .min((min * rem) as f64)
                .max((max * rem) as f64)
                .step(8.)
        });
        let owner = cx.weak_entity();
        let focus = split.focus.clone();
        window.open_dialog(cx, move |dialog, window, cx| {
            let reset = owner.clone();
            let reset_input = input.clone();
            let submit = owner.clone();
            let value = input.clone();
            let focus = focus.clone();
            dialog
                .title(kind.label())
                .w((window.rem_size() * 26.)
                    .min(window.viewport_size().width - window.rem_size() * 2.))
                .child(
                    div()
                        .debug_selector(|| "flow-size-input".into())
                        .child(NumberInput::new(&input).w_full()),
                )
                .child(
                    div()
                        .text_size(m::LABEL)
                        .text_color(MUTED.resolve(cx))
                        .child(
                        "Size in logical pixels. Arrow keys adjust; Shift changes a larger amount.",
                    ),
                )
                .child(
                    Button::new("flow-size-reset")
                        .label("Reset to default")
                        .on_click(move |_, w, c| {
                            let _ = reset.update(c, |this, cx| {
                                this.set_panel(kind, kind.default_size(), w, cx);
                            });
                            reset_input.update(c, |input, c| {
                                input.set_value(format!("{:.0}", kind.default_size() * rem), w, c)
                            });
                        }),
                )
                .footer(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(Button::new("flow-size-cancel").label("Cancel").on_click(
                            |_, w, c| {
                                w.close_dialog(c);
                            },
                        ))
                        .child(
                            Button::new("flow-size-apply")
                                .primary()
                                .label("Apply")
                                .on_click(|_, w, c| {
                                    w.dispatch_action(
                                        Box::new(gpui_kit::component::dialog::Confirm {
                                            secondary: false,
                                        }),
                                        c,
                                    )
                                }),
                        ),
                )
                .on_ok(move |_, window, cx| {
                    let Ok(n) = value.read(cx).value().parse::<f32>() else {
                        return false;
                    };
                    if !n.is_finite() {
                        return false;
                    }
                    let _ = submit.update(cx, |this, cx| this.set_panel(kind, n / rem, window, cx));
                    true
                })
                .on_close(move |_, w, c| focus.focus(w, c))
        });
    }
    pub(super) fn render_panel_split(
        &self,
        kind: SplitKind,
        first: AnyElement,
        second: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let split = &self.splits[kind.ix()];
        let (min, max) = split.limits();
        let total = split.total();
        let rem = split.rem;
        let focus = split.focus.clone();
        let handle_bounds = split.handle_bounds.clone();
        let appearance = resize_handle_appearance();
        let (first_min, first_max) = if kind.is_trailing() {
            (total - max, total - min)
        } else {
            (min, max)
        };
        let group = if matches!(kind, SplitKind::Results) {
            v_resizable(kind.id())
        } else {
            h_resizable(kind.id())
        };
        let content = group
            .with_state(&split.state)
            .with_handle_appearance(Rc::new(move |context, window, cx| {
                let bounds = handle_bounds.clone();
                Some(
                    div()
                        .id(kind.handle())
                        .debug_selector(move || kind.handle().into())
                        .track_focus(&focus)
                        .key_context("FlowSplit")
                        .role(Role::Splitter)
                        .aria_label(kind.label())
                        .relative()
                        .when(matches!(kind, SplitKind::Results), |d| d.h(px(1.)).w_full())
                        .when(!matches!(kind, SplitKind::Results), |d| {
                            d.w(px(1.)).h_full()
                        })
                        .on_prepaint(move |b, _, _| bounds.set(b))
                        .children(appearance(context, window, cx))
                        .when(focus.is_focused(window), |d| {
                            d.child(div().absolute().inset_0().bg(ACCENT.resolve(cx)))
                        })
                        .into_any_element(),
                )
            }))
            .on_resize(cx.listener(move |this, _, _, cx| {
                let split = &mut this.splits[kind.ix()];
                if split.drag_origin.take().is_some() {
                    split.preferred = split.displayed(cx);
                }
                cx.notify();
            }))
            .child(
                resizable_panel()
                    .size_range(px(first_min * rem)..px(first_max * rem))
                    .child(first),
            )
            .child(
                resizable_panel()
                    .size_range(px((total - first_max) * rem)..px((total - first_min) * rem))
                    .child(second),
            );
        let previous = split.bounds;
        let previous_rem = split.rem;
        let restore = split.restore;
        div()
            .id((kind.id(), 0usize))
            .relative()
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .on_prepaint({
                let owner = cx.weak_entity();
                move |bounds, window, cx| {
                    let rem = window.rem_size().as_f32();
                    if previous != bounds || previous_rem != rem || restore {
                        let owner = owner.clone();
                        window.defer(cx, move |window, cx| {
                            let _ = owner.update(cx, |this, cx| {
                                let split = &mut this.splits[kind.ix()];
                                if split.bounds != bounds || split.rem != rem {
                                    split.bounds = bounds;
                                    split.rem = rem;
                                    split.restore = true;
                                    cx.notify();
                                } else if split.restore {
                                    split.restore = false;
                                    split.apply(window, cx);
                                    cx.notify();
                                }
                            });
                        });
                    }
                }
            })
            .child(
                gpui::canvas(|_, _, _| (), {
                    let owner = cx.weak_entity();
                    move |bounds, _, window, _| {
                        let owner = owner.clone();
                        window.on_mouse_event(
                            move |event: &gpui::MouseDownEvent, phase, window, cx| {
                                if !phase.capture()
                                    || event.button != MouseButton::Left
                                    || !bounds.contains(&event.position)
                                    || window.has_active_dialog(cx)
                                {
                                    return;
                                }
                                let _ = owner.update(cx, |this, cx| {
                                    let split = &mut this.splits[kind.ix()];
                                    if split
                                        .handle_bounds
                                        .get()
                                        .dilate(px(4.))
                                        .contains(&event.position)
                                    {
                                        split.focus.focus(window, cx);
                                        if event.click_count >= 2 {
                                            split.drag_origin = None;
                                            this.set_panel(kind, kind.default_size(), window, cx);
                                        } else {
                                            split.drag_origin = Some(split.preferred);
                                        }
                                    }
                                });
                            },
                        );
                    }
                })
                .absolute()
                .inset_0()
                .size_full(),
            )
            .on_action(
                cx.listener(move |this, _: &Shrink, w, c| this.adjust_panel(kind, -0.5, w, c)),
            )
            .on_action(cx.listener(move |this, _: &Grow, w, c| this.adjust_panel(kind, 0.5, w, c)))
            .on_action(
                cx.listener(move |this, _: &ShrinkFast, w, c| this.adjust_panel(kind, -2., w, c)),
            )
            .on_action(
                cx.listener(move |this, _: &GrowFast, w, c| this.adjust_panel(kind, 2., w, c)),
            )
            .on_action(cx.listener(move |this, _: &Minimum, w, c| this.set_panel(kind, 0., w, c)))
            .on_action(
                cx.listener(move |this, _: &Maximum, w, c| this.set_panel(kind, f32::MAX, w, c)),
            )
            .on_action(cx.listener(move |this, _: &Sizes, w, c| this.panel_sizes(kind, w, c)))
            .on_action(cx.listener(move |this, _: &Cancel, w, c| {
                let split = &mut this.splits[kind.ix()];
                if let Some(value) = split.drag_origin.take() {
                    c.stop_active_drag(w);
                    split.preferred = value;
                    split.apply(w, c);
                    c.notify();
                }
            }))
            .child(content)
            .into_any_element()
    }
}

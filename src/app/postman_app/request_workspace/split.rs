//! Kit owns panel sizing and pointer dragging; this owner supplies product preferences,
//! responsive constraints and the keyboard/numeric paths missing from Kit 0.7.1.
use super::RequestWorkspace;
use crate::app::http_layout::{HttpLayout, HttpLayoutPreferences, DEFAULT_SHARE};
use crate::ui::theme::{metrics as m, ACCENT, LINE, PANEL, SUBTEXT};
use gpui::{
    actions, div, prelude::FluentBuilder, px, Along, AppContext, Axis, Bounds, Context, Entity,
    FocusHandle, InteractiveElement, IntoElement, KeyBinding, MouseButton, ParentElement, Pixels,
    Role, StatefulInteractiveElement, Styled, Window,
};
use gpui_kit::base::ElementExt;
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    dialog::Confirm,
    input::{InputState, NumberInput},
    resizable::{
        h_resizable, resizable_panel, resize_handle_appearance, v_resizable, ResizableState,
    },
    Disableable, WindowExt,
};
use std::{cell::Cell, rc::Rc};

actions!(
    http_split,
    [Shrink, Grow, ShrinkFast, GrowFast, Minimum, Maximum, Reset, Cancel, Sizes]
);

pub(super) struct HttpSplit {
    horizontal: Entity<ResizableState>,
    vertical: Entity<ResizableState>,
    preferences: HttpLayout,
    bounds: Bounds<Pixels>,
    stacked: bool,
    restore_pending: bool,
    focus: FocusHandle,
    drag_origin: Option<f32>,
    divider_bounds: Rc<Cell<Bounds<Pixels>>>,
}
impl HttpSplit {
    pub(super) fn new(cx: &mut Context<RequestWorkspace>) -> Self {
        cx.bind_keys([
            KeyBinding::new("left", Shrink, Some("HttpSplit")),
            KeyBinding::new("up", Shrink, Some("HttpSplit")),
            KeyBinding::new("right", Grow, Some("HttpSplit")),
            KeyBinding::new("down", Grow, Some("HttpSplit")),
            KeyBinding::new("shift-left", ShrinkFast, Some("HttpSplit")),
            KeyBinding::new("shift-up", ShrinkFast, Some("HttpSplit")),
            KeyBinding::new("shift-right", GrowFast, Some("HttpSplit")),
            KeyBinding::new("shift-down", GrowFast, Some("HttpSplit")),
            KeyBinding::new("home", Minimum, Some("HttpSplit")),
            KeyBinding::new("end", Maximum, Some("HttpSplit")),
            KeyBinding::new("escape", Cancel, Some("HttpSplit")),
            KeyBinding::new("enter", Sizes, Some("HttpSplit")),
            KeyBinding::new("space", Sizes, Some("HttpSplit")),
        ]);
        Self {
            horizontal: cx.new(|_| ResizableState::default()),
            vertical: cx.new(|_| ResizableState::default()),
            preferences: HttpLayoutPreferences::current(cx),
            bounds: Bounds::default(),
            stacked: false,
            restore_pending: false,
            focus: cx.focus_handle().tab_stop(true),
            drag_origin: None,
            divider_bounds: Rc::new(Cell::new(Bounds::default())),
        }
    }
    fn axis(&self) -> Axis {
        if self.stacked {
            Axis::Vertical
        } else {
            Axis::Horizontal
        }
    }
    fn state(&self) -> Entity<ResizableState> {
        if self.stacked {
            self.vertical.clone()
        } else {
            self.horizontal.clone()
        }
    }
    fn total(&self) -> f32 {
        self.bounds.size.along(self.axis()).as_f32().max(1.)
    }
    fn limits(&self) -> (f32, f32) {
        // Prototype limits. In a short native window both regions retain scrollable content.
        let total = self.total();
        let minimum = (if self.stacked { 180_f32 } else { 360. }).min(total * 0.45);
        let response = (if self.stacked { 210_f32 } else { 360. }).min(total * 0.45);
        (minimum, (total - response).max(minimum))
    }
    fn displayed(&self, cx: &gpui::App) -> f32 {
        self.state()
            .read(cx)
            .sizes()
            .first()
            .map_or(self.total() * DEFAULT_SHARE, |v| v.as_f32())
    }
}
impl RequestWorkspace {
    fn restore_split(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let size = self.split.preferences.share(self.split.stacked) * self.split.total();
        let (min, max) = self.split.limits();
        self.split.state().update(cx, |state, cx| {
            state.resize_panel(0, px(size.clamp(min, max)), window, cx)
        });
        cx.notify();
    }
    fn set_split_size(&mut self, size: f32, window: &mut Window, cx: &mut Context<Self>) {
        let (min, max) = self.split.limits();
        let size = size.clamp(min, max);
        self.split
            .preferences
            .set_share(self.split.stacked, size / self.split.total());
        self.restore_split(window, cx);
        HttpLayoutPreferences::save(self.split.preferences, cx);
    }
    fn adjust_split(&mut self, delta: f32, window: &mut Window, cx: &mut Context<Self>) {
        self.set_split_size(
            self.split.displayed(cx) + self.split.total() * delta,
            window,
            cx,
        );
    }
    fn reset_split(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.split
            .preferences
            .set_share(self.split.stacked, DEFAULT_SHARE);
        self.split.drag_origin = None;
        self.restore_split(window, cx);
        HttpLayoutPreferences::save(self.split.preferences, cx);
    }
    fn cancel_split_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(share) = self.split.drag_origin.take() {
            cx.stop_active_drag(window);
            self.split.preferences.set_share(self.split.stacked, share);
            self.restore_split(window, cx);
        }
    }
    pub(super) fn toggle_split(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_split_drag(window, cx);
        // A constrained stack can ask for automatic layout; widening then restores columns.
        self.split.preferences.stacked = !self.split.stacked;
        self.update_split_bounds(self.split.bounds, window, cx);
        HttpLayoutPreferences::save(self.split.preferences, cx);
    }
    fn update_split_bounds(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let stacked = self.split.preferences.stacked || bounds.size.width < px(900.);
        self.split.bounds = bounds;
        if stacked != self.split.stacked {
            self.cancel_split_drag(window, cx);
            self.split.stacked = stacked;
        }
        self.response_viewer.update(cx, |viewer, cx| {
            viewer.set_stacked(stacked, cx);
        });
        self.split.restore_pending = true;
        cx.notify();
    }
    pub(super) fn open_split_sizes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = (self.split.displayed(cx) / self.split.total() * 100.).round();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(format!("{value:.0}"))
                .step(2.)
                .min(1.)
                .max(99.)
        });
        let this = cx.weak_entity();
        let focus = self.split.focus.clone();
        let (min, max) = self.split.limits();
        let limits = format!(
            "Request share: {:.0}–{:.0}%. Each arrangement keeps its own size.",
            min / self.split.total() * 100.,
            max / self.split.total() * 100.
        );
        window.open_dialog(cx, move |dialog, window, cx| {
            let valid = input
                .read(cx)
                .value()
                .parse::<f32>()
                .is_ok_and(|n| n.is_finite());
            let owner = this.clone();
            let field = input.clone();
            let reset_owner = this.clone();
            let reset_field = input.clone();
            let close_focus = focus.clone();
            dialog
                .title("Panel sizes")
                .w(window.rem_size() * 27.)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child("Request size (%)")
                        .child(
                            div()
                                .debug_selector(|| "http-size-input".into())
                                .child(NumberInput::new(&input).w_full()),
                        )
                        .child(
                            div()
                                .text_size(m::LABEL)
                                .text_color(SUBTEXT.resolve(cx))
                                .child(limits.clone()),
                        ),
                )
                .footer(
                    div()
                        .flex()
                        .justify_between()
                        .w_full()
                        .child(
                            Button::new("http-reset-sizes")
                                .label("Reset to default")
                                .on_click(move |_, window, cx| {
                                    let _ = reset_owner
                                        .update(cx, |this, cx| this.reset_split(window, cx));
                                    reset_field
                                        .update(cx, |input, cx| input.set_value("46", window, cx));
                                }),
                        )
                        .child(
                            Button::new("http-apply-sizes")
                                .primary()
                                .label("Apply")
                                .disabled(!valid)
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(
                                        Box::new(Confirm { secondary: false }),
                                        cx,
                                    );
                                }),
                        ),
                )
                .on_ok(move |_, window, cx| {
                    let Ok(value) = field.read(cx).value().parse::<f32>() else {
                        return false;
                    };
                    if !value.is_finite() {
                        return false;
                    }
                    owner
                        .update(cx, |this, cx| {
                            this.set_split_size(this.split.total() * value / 100., window, cx);
                        })
                        .is_ok()
                })
                .on_close(move |_, window, cx| {
                    close_focus.focus(window, cx);
                })
        });
    }
    pub(super) fn render_split(
        &self,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let stacked = self.split.stacked;
        let state = self.split.state();
        let (min, max) = self.split.limits();
        let appearance = resize_handle_appearance();
        let focus = self.split.focus.clone();
        let divider_bounds = self.split.divider_bounds.clone();
        let group = if stacked {
            v_resizable("http-panels-stacked")
        } else {
            h_resizable("http-panels-columns")
        };
        let request = div()
            .debug_selector(|| "request-container".into())
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .when(!stacked, |d| {
                d.child(
                    div()
                        .h(gpui::rems(50. / 16.))
                        .flex_none()
                        .px_7()
                        .flex()
                        .items_center()
                        .text_size(m::LABEL)
                        .text_color(SUBTEXT.resolve(cx))
                        .child("Request"),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child(self.composer.clone()),
            );
        let response = div()
            .id("response-container")
            .debug_selector(|| "response-container".into())
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(self.response_viewer.clone());
        let content = group
            .with_state(&state)
            .with_handle_appearance(Rc::new(move |context, window, cx| {
                let handle = div()
                    .id("http-split-handle")
                    .debug_selector(|| "response-resize-handle".into())
                    .track_focus(&focus)
                    .key_context("HttpSplit")
                    .role(Role::Splitter)
                    .on_prepaint({
                        let bounds = divider_bounds.clone();
                        move |value, _, _| bounds.set(value)
                    })
                    .aria_label(
                        "Request and response divider. Arrow keys resize; Enter opens panel sizes",
                    )
                    .when(stacked, |d| d.h(px(1.)).w_full())
                    .when(!stacked, |d| d.w(px(1.)).h_full())
                    .relative()
                    .children(appearance(context, window, cx))
                    .when(focus.is_focused(window), |d| {
                        d.child(div().absolute().inset_0().bg(ACCENT.resolve(cx)))
                    });
                Some(handle.into_any_element())
            }))
            .on_resize(cx.listener(|this, _, _, cx| {
                if this.split.drag_origin.take().is_some() {
                    let share = this.split.displayed(cx) / this.split.total();
                    this.split.preferences.set_share(this.split.stacked, share);
                    HttpLayoutPreferences::save(this.split.preferences, cx);
                }
                cx.notify();
            }))
            .child(
                resizable_panel()
                    .size_range(px(min)..px(max))
                    .child(request),
            )
            .child(
                resizable_panel()
                    .size_range(px(self.split.total() - max)..px(self.split.total() - min))
                    .child(response),
            );
        div()
            .id("http-split")
            .debug_selector(|| "http-split".into())
            .relative()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .bg(PANEL.resolve(cx))
            .border_t_1()
            .border_color(LINE.resolve(cx))
            .on_prepaint({
                let this = cx.weak_entity();
                let previous = self.split.bounds;
                let restore = self.split.restore_pending;
                move |bounds, window, cx| {
                    if bounds != previous || restore {
                        window.defer(cx, move |window, cx| {
                            let _ = this.update(cx, |this, cx| {
                                if bounds != this.split.bounds {
                                    this.update_split_bounds(bounds, window, cx);
                                } else if this.split.restore_pending {
                                    this.split.restore_pending = false;
                                    this.restore_split(window, cx);
                                }
                            });
                        });
                    }
                }
            })
            // Kit's grab band occludes ancestor hitboxes. Observe its pointer-down
            // in the window capture phase; Kit still owns the complete drag itself.
            .child(
                gpui::canvas(|_, _, _| (), {
                    let this = cx.weak_entity();
                    move |bounds, _, window, _| {
                        window.on_mouse_event(
                            move |event: &gpui::MouseDownEvent, phase, window, cx| {
                                if window.has_active_dialog(cx)
                                    || !phase.capture()
                                    || event.button != MouseButton::Left
                                    || !bounds.contains(&event.position)
                                {
                                    return;
                                }
                                let _ = this.update(cx, |this, cx| {
                                    if this
                                        .split
                                        .divider_bounds
                                        .get()
                                        .dilate(px(4.))
                                        .contains(&event.position)
                                    {
                                        this.split.focus.focus(window, cx);
                                        this.split.drag_origin =
                                            Some(this.split.preferences.share(this.split.stacked));
                                        if event.click_count >= 2 {
                                            this.reset_split(window, cx);
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
            .on_action(cx.listener(|this, _: &Shrink, w, c| this.adjust_split(-0.02, w, c)))
            .on_action(cx.listener(|this, _: &Grow, w, c| this.adjust_split(0.02, w, c)))
            .on_action(cx.listener(|this, _: &ShrinkFast, w, c| this.adjust_split(-0.1, w, c)))
            .on_action(cx.listener(|this, _: &GrowFast, w, c| this.adjust_split(0.1, w, c)))
            .on_action(cx.listener(|this, _: &Minimum, w, c| this.set_split_size(0., w, c)))
            .on_action(cx.listener(|this, _: &Maximum, w, c| this.set_split_size(f32::MAX, w, c)))
            .on_action(cx.listener(|this, _: &Reset, w, c| this.reset_split(w, c)))
            .on_action(cx.listener(|this, _: &Cancel, w, c| this.cancel_split_drag(w, c)))
            .on_action(cx.listener(|this, _: &Sizes, w, c| this.open_split_sizes(w, c)))
            .child(content)
    }
}

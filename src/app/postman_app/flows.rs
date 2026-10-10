//! Retained boundary for the future flow editor. No sample runs or editable-looking fake controls.
use crate::ui::theme::{metrics as m, INFO, LINE, MUTED, PANEL, PANEL_ALT, TEXT};
use gpui::{
    div, App, Context, FocusHandle, InteractiveElement, IntoElement, ParentElement, Render,
    ScrollHandle, StatefulInteractiveElement, Styled, Window,
};
use gpui_kit::{assets::IconName, component::Icon};

pub(super) struct FlowsView {
    focus: FocusHandle,
    scroll: ScrollHandle,
}

impl FlowsView {
    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
        }
    }
    pub(super) fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }
}

pub(super) fn empty_message(cx: &App) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .text_size(m::LABEL)
        .text_color(MUTED.resolve(cx))
        .child("No flows yet")
        .child("Flow editing and execution are not available in this version.")
}

impl Render for FlowsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("flows-screen")
            .debug_selector(|| "flows-screen".into())
            .track_focus(&self.focus)
            .key_context("Flows")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .h_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .bg(PANEL.resolve(cx))
            .text_color(TEXT.resolve(cx))
            .child(
                div()
                    .p_8()
                    .flex()
                    .flex_col()
                    .gap_6()
                    .child(
                        div()
                            .text_size(m::TITLE)
                            .font_weight(m::SEMIBOLD)
                            .child("Flows"),
                    )
                    .child(
                        div()
                            .text_size(m::BODY)
                            .text_color(MUTED.resolve(cx))
                            .child("Connect HTTP requests into a repeatable sequence."),
                    )
                    .child(
                        div()
                            .p_8()
                            .border_1()
                            .border_color(LINE.resolve(cx))
                            .rounded(m::DIALOG_RADIUS)
                            .bg(PANEL_ALT.resolve(cx))
                            .flex()
                            .flex_col()
                            .gap_4()
                            .child(
                                Icon::new(IconName::Workflow)
                                    .size(m::ICON)
                                    .text_color(INFO.resolve(cx)),
                            )
                            .child(empty_message(cx)),
                    ),
            )
    }
}

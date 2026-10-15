use super::*;
use crate::ui::theme::{metrics as m, ACCENT, LINE, MUTED};
use gpui::{
    actions, AnyElement, InteractiveElement, IntoElement, KeyBinding, ParentElement,
    StatefulInteractiveElement, Styled,
};
use gpui_kit::base::{Tab, Tabs};

actions!(
    flow_inspector,
    [PreviousTab, NextTab, FirstTab, LastTab, ActivateTab]
);

pub(super) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("left", PreviousTab, Some("FlowInspectorTabs")),
        KeyBinding::new("right", NextTab, Some("FlowInspectorTabs")),
        KeyBinding::new("home", FirstTab, Some("FlowInspectorTabs")),
        KeyBinding::new("end", LastTab, Some("FlowInspectorTabs")),
        KeyBinding::new("enter", ActivateTab, Some("FlowInspectorTabs")),
        KeyBinding::new("space", ActivateTab, Some("FlowInspectorTabs")),
    ]);
}
impl RequestInspector {
    fn navigate_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.tab = index;
        self.tab_focus[index].focus(window, cx);
        cx.notify();
    }
    pub(super) fn tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        Tabs::new("flow-inspector-tabs")
            .aria_label("Step configuration")
            .flex_none()
            .mx_4()
            .flex()
            .gap_4()
            .border_b_1()
            .border_color(LINE.resolve(cx))
            .children(
                [
                    ("flow-request-tab", "Request"),
                    ("flow-checks-tab", "Checks"),
                    ("flow-exports-tab", "Exports"),
                ]
                .into_iter()
                .enumerate()
                .map(|(ix, (id, label))| {
                    Tab::new(id)
                        .selected(self.tab == ix)
                        .set_position(ix + 1, 3)
                        .accessibility_label(label)
                        .track_focus(&self.tab_focus[ix])
                        .key_context("FlowInspectorTabs")
                        .h_10()
                        .flex()
                        .items_center()
                        .border_b_2()
                        .border_color(if self.tab == ix {
                            ACCENT.resolve(cx)
                        } else {
                            gpui::rgba(0)
                        })
                        .text_color(if self.tab == ix { ACCENT } else { MUTED }.resolve(cx))
                        .text_size(m::LABEL)
                        .focus_visible(|d| d.border_1().border_color(ACCENT.resolve(cx)))
                        .on_click(cx.listener(move |this, _, w, cx| this.navigate_tab(ix, w, cx)))
                        .on_action(cx.listener(|this, _: &PreviousTab, w, cx| {
                            this.navigate_tab((this.tab + 2) % 3, w, cx)
                        }))
                        .on_action(cx.listener(|this, _: &NextTab, w, cx| {
                            this.navigate_tab((this.tab + 1) % 3, w, cx)
                        }))
                        .on_action(
                            cx.listener(|this, _: &FirstTab, w, cx| this.navigate_tab(0, w, cx)),
                        )
                        .on_action(
                            cx.listener(|this, _: &LastTab, w, cx| this.navigate_tab(2, w, cx)),
                        )
                        .on_action(cx.listener(move |this, _: &ActivateTab, w, cx| {
                            this.navigate_tab(ix, w, cx)
                        }))
                        .child(label)
                }),
            )
            .into_any_element()
    }
}

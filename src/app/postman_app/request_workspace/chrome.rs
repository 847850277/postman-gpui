use super::RequestWorkspace;
use crate::{
    app::ActivateControl,
    ui::theme::{
        method_color, ACCENT, ACCENT_DARK, ACCENT_SOFT, FONT_HEADING, FONT_UI, LINE, MUTED, PANEL,
        PANEL_ALT, SUBTEXT,
    },
};
use gpui::{
    actions, div, prelude::FluentBuilder, px, rgb, Context, FontWeight, InteractiveElement,
    IntoElement, KeyBinding, MouseButton, ParentElement, Role, StatefulInteractiveElement, Styled,
    Window,
};
use std::collections::HashSet;

actions!(
    request_tabs,
    [
        ActivateRequestTab,
        FocusNextRequestTab,
        FocusPreviousRequestTab,
        ActivateNextRequestTab,
        ActivatePreviousRequestTab
    ]
);

pub(super) fn setup_request_tab_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("enter", ActivateRequestTab, Some("RequestTab")),
        KeyBinding::new("space", ActivateRequestTab, Some("RequestTab")),
        KeyBinding::new("tab", FocusNextRequestTab, Some("RequestTab")),
        KeyBinding::new("shift-tab", FocusPreviousRequestTab, Some("RequestTab")),
        KeyBinding::new("right", ActivateNextRequestTab, Some("RequestTab")),
        KeyBinding::new("down", ActivateNextRequestTab, Some("RequestTab")),
        KeyBinding::new("left", ActivatePreviousRequestTab, Some("RequestTab")),
        KeyBinding::new("up", ActivatePreviousRequestTab, Some("RequestTab")),
    ]
}

impl RequestWorkspace {
    pub(super) fn render_request_tabs_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let tabs: Vec<_> = {
            let view_model = self.view_model.read(cx);
            let active_index = view_model.active_tab_index();
            view_model
                .tabs()
                .iter()
                .enumerate()
                .map(|(index, request)| {
                    (
                        index,
                        request.tab_id(),
                        request.method(),
                        request.tab_title(),
                        request.is_dirty(),
                        index == active_index,
                    )
                })
                .collect()
        };
        let retained_tab_ids = tabs
            .iter()
            .map(|(_, tab_id, _, _, _, _)| *tab_id)
            .collect::<HashSet<_>>();
        self.tab_focus_handles
            .retain(|tab_id, _| retained_tab_ids.contains(tab_id));
        self.tab_close_focus_handles
            .retain(|tab_id, _| retained_tab_ids.contains(tab_id));

        let tab_elements = tabs
            .into_iter()
            .map(|(index, tab_id, method, title, dirty, active)| {
                let focus_handle = self
                    .tab_focus_handles
                    .entry(tab_id)
                    .or_insert_with(|| cx.focus_handle().tab_index(0).tab_stop(true))
                    .clone();
                let mouse_focus_handle = focus_handle.clone();
                let focused = focus_handle.is_focused(window);
                let mouse_tab_id = tab_id;
                let keyboard_tab_id = tab_id;
                let close_tab_id = tab_id;
                let keyboard_close_tab_id = tab_id;
                let close_focus_handle = self
                    .tab_close_focus_handles
                    .entry(tab_id)
                    .or_insert_with(|| cx.focus_handle())
                    .clone();
                let mouse_close_focus_handle = close_focus_handle.clone();
                let close_focused = close_focus_handle.is_focused(window);
                let close_label = format!("Close {title} request tab");

                div()
                    .id(format!("request-tab-id-{tab_id}"))
                    .debug_selector(move || format!("request-tab-{index}"))
                    .track_focus(&focus_handle)
                    .key_context("RequestTab")
                    .role(Role::Tab)
                    .aria_label(format!("{method} request tab {title}"))
                    .aria_selected(active)
                    .h_full()
                    .max_w(px(280.0))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .bg(rgb(if active { PANEL } else { PANEL_ALT }))
                    .rounded_t_lg()
                    .font_family(FONT_UI)
                    .text_size(px(12.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(PANEL)))
                    .when(focused, |tab| tab.border_1().border_color(rgb(ACCENT)))
                    .child(
                        div()
                            .debug_selector(move || format!("request-tab-method-{index}"))
                            .text_color(rgb(method_color(method)))
                            .font_weight(FontWeight::BOLD)
                            .child(method.to_string()),
                    )
                    .child(
                        div()
                            .max_w(px(180.0))
                            .overflow_hidden()
                            .text_color(rgb(if active { SUBTEXT } else { MUTED }))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .when(dirty, |tab| {
                        tab.child(div().size(px(6.0)).rounded_full().bg(rgb(ACCENT)))
                    })
                    .child(
                        div()
                            .id(("request-tab-close", index))
                            .debug_selector(move || format!("close-tab-{index}"))
                            .track_focus(&close_focus_handle)
                            .key_context("KeyboardButton")
                            .role(Role::Button)
                            .aria_label(close_label)
                            .size(px(20.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_md()
                            .text_color(rgb(MUTED))
                            .hover(|style| style.bg(rgb(ACCENT_SOFT)).text_color(rgb(ACCENT_DARK)))
                            .when(close_focused, |button| {
                                button.border_1().border_color(rgb(ACCENT))
                            })
                            .child("×")
                            .on_action(cx.listener(move |this, _: &ActivateControl, window, cx| {
                                cx.stop_propagation();
                                this.close_request_tab(keyboard_close_tab_id, cx);
                                this.focus_active_request_tab(window, cx);
                            }))
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    mouse_close_focus_handle.focus(window, cx);
                                    this.close_request_tab(close_tab_id, cx);
                                    this.focus_active_request_tab(window, cx);
                                }),
                            ),
                    )
                    .on_action(
                        cx.listener(move |this, _: &ActivateRequestTab, _window, cx| {
                            this.activate_request_tab(keyboard_tab_id, cx);
                        }),
                    )
                    .on_action(cx.listener(Self::focus_next_request_tab))
                    .on_action(cx.listener(Self::focus_previous_request_tab))
                    .on_action(cx.listener(Self::activate_next_request_tab))
                    .on_action(cx.listener(Self::activate_previous_request_tab))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            mouse_focus_handle.focus(window, cx);
                            this.activate_request_tab(mouse_tab_id, cx);
                        }),
                    )
            })
            .collect::<Vec<_>>();

        div()
            .debug_selector(|| "request-tabs-bar".into())
            .h(px(54.0))
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .bg(rgb(PANEL))
            .border_b_1()
            .border_color(rgb(LINE))
            .children(tab_elements)
            .child(
                div()
                    .id("new-tab-button")
                    .debug_selector(|| "new-tab-button".into())
                    .track_focus(&self.new_tab_focus_handle)
                    .key_context("KeyboardButton")
                    .role(Role::Button)
                    .aria_label("New request tab")
                    .size(px(32.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_lg()
                    .bg(rgb(PANEL_ALT))
                    .text_color(rgb(SUBTEXT))
                    .font_family(FONT_HEADING)
                    .text_size(px(20.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(ACCENT_SOFT)).text_color(rgb(ACCENT_DARK)))
                    .when(self.new_tab_focus_handle.is_focused(window), |button| {
                        button.border_1().border_color(rgb(ACCENT))
                    })
                    .child("+")
                    .on_action(cx.listener(|this, _: &ActivateControl, window, cx| {
                        this.new_request(cx);
                        this.focus_active_request_tab(window, cx);
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.new_request(cx);
                            this.focus_active_request_tab(window, cx);
                        }),
                    ),
            )
    }

    fn focus_next_request_tab(
        &mut self,
        _: &FocusNextRequestTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus_next(cx);
    }

    fn focus_previous_request_tab(
        &mut self,
        _: &FocusPreviousRequestTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus_prev(cx);
    }

    fn activate_next_request_tab(
        &mut self,
        _: &ActivateNextRequestTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activate_focused_relative_request(1, window, cx);
    }

    fn activate_previous_request_tab(
        &mut self,
        _: &ActivatePreviousRequestTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activate_focused_relative_request(-1, window, cx);
    }

    fn activate_focused_relative_request(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (tab_ids, active_index) = {
            let view_model = self.view_model.read(cx);
            (
                view_model
                    .tabs()
                    .iter()
                    .map(|tab| tab.tab_id())
                    .collect::<Vec<_>>(),
                view_model.active_tab_index(),
            )
        };
        if tab_ids.is_empty() {
            return;
        }
        let current_index = tab_ids
            .iter()
            .position(|tab_id| {
                self.tab_focus_handles
                    .get(tab_id)
                    .is_some_and(|focus| focus.is_focused(window))
            })
            .unwrap_or(active_index);
        let next = (current_index as isize + delta).rem_euclid(tab_ids.len() as isize) as usize;
        self.activate_request_tab(tab_ids[next], cx);
        self.focus_active_request_tab(window, cx);
    }
}

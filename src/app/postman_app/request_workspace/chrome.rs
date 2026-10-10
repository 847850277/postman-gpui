use super::RequestWorkspace;
use crate::{
    app::RequestTabId,
    ui::{
        components::kit_controls,
        theme::{method_color, metrics as m, ACCENT, FONT_MONO, LINE, MUTED, PANEL, SIDEBAR, TEXT},
    },
};
use gpui::{
    actions, canvas, div, prelude::FluentBuilder, rems, Context, InteractiveElement, IntoElement,
    KeyBinding, ParentElement, StatefulInteractiveElement, Styled, Window,
};
use gpui_kit::{assets::IconName, base::Tab};
use std::collections::HashSet;

actions!(
    request_tabs,
    [
        ActivateRequestTab,
        ActivateNextRequestTab,
        ActivatePreviousRequestTab,
        ActivateTabAbove,
        ActivateTabBelow,
        ActivateFirstRequestTab,
        ActivateLastRequestTab
    ]
);
pub(super) fn setup_request_tab_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("enter", ActivateRequestTab, Some("RequestTab")),
        KeyBinding::new("space", ActivateRequestTab, Some("RequestTab")),
        KeyBinding::new("right", ActivateNextRequestTab, Some("RequestTab")),
        KeyBinding::new("left", ActivatePreviousRequestTab, Some("RequestTab")),
        KeyBinding::new("up", ActivateTabAbove, Some("RequestTab")),
        KeyBinding::new("down", ActivateTabBelow, Some("RequestTab")),
        KeyBinding::new("home", ActivateFirstRequestTab, Some("RequestTab")),
        KeyBinding::new("end", ActivateLastRequestTab, Some("RequestTab")),
    ]
}
impl RequestWorkspace {
    fn tab_columns(&self, window: &Window) -> usize {
        let width = if self.tab_bar_width > gpui::px(0.) {
            self.tab_bar_width
        } else {
            window.viewport_size().width - rems(4.5).to_pixels(window.rem_size())
        };
        (((width - rems(2.75).to_pixels(window.rem_size())) / window.rem_size()) / 10.)
            .floor()
            .max(1.) as usize
    }
    pub(super) fn render_request_tabs_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let model = self.view_model.read(cx);
        let active = model.active_tab_id();
        let tabs = model.request_tag_projections();
        let ids = tabs.iter().map(|t| t.tab_id).collect::<HashSet<_>>();
        self.tab_focus_handles.retain(|id, _| ids.contains(id));
        self.tab_close_focus_handles
            .retain(|id, _| ids.contains(id));
        let count = tabs.len();
        let columns = self.tab_columns(window);
        let row_height = rems(38. / 16.);
        // Limit the strip in short windows so wrapped tabs cannot consume the response.
        let max_rows = if window.viewport_size().height < gpui::px(800.) {
            2.
        } else {
            6.
        };
        let reveal_key = active.map(|id| (id, columns, max_rows as usize));
        if self.tab_reveal_key != reveal_key {
            if let Some(index) = tabs.iter().position(|tab| Some(tab.tab_id) == active) {
                self.tab_scroll.scroll_to_item(index);
            }
            self.tab_reveal_key = reveal_key;
        }
        let weak = cx.entity().downgrade();
        div()
            .id("request-tabs-bar")
            .debug_selector(|| "request-tabs-bar".into())
            .relative()
            .flex_none()
            .flex()
            .items_start()
            .min_w_0()
            .bg(SIDEBAR.resolve(cx))
            .border_b_1()
            .border_color(LINE.resolve(cx))
            .child(
                canvas(
                    move |bounds, _, cx| {
                        cx.defer(move |cx| {
                            let _ = weak.update(cx, |this, cx| {
                                if (this.tab_bar_width - bounds.size.width).abs() > gpui::px(0.5) {
                                    this.tab_bar_width = bounds.size.width;
                                    cx.notify();
                                }
                            });
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .id("request-tabs-scroll")
                    .flex_1()
                    .min_w_0()
                    .max_h(rems(38. / 16. * max_rows))
                    .overflow_y_scroll()
                    .track_scroll(&self.tab_scroll)
                    .grid()
                    .grid_cols(columns.min(u16::MAX as usize) as u16)
                    .children(tabs.into_iter().enumerate().map(|(index, tag)| {
                        let id = tag.tab_id;
                        let selected = Some(id) == active;
                        let focus = self
                            .tab_focus_handles
                            .entry(id)
                            .or_insert_with(|| cx.focus_handle().tab_index(0).tab_stop(true))
                            .clone();
                        let close_focus = self
                            .tab_close_focus_handles
                            .entry(id)
                            .or_insert_with(|| cx.focus_handle().tab_index(0).tab_stop(true))
                            .clone();
                        let mouse_focus = focus.clone();
                        let title = tag.display_name.clone();
                        div()
                            .id(("request-tab-container", id.0))
                            .relative()
                            .min_w_0()
                            .h(row_height)
                            .child(
                                Tab::new(("request-tab", id.0))
                                    .debug_selector(move || format!("request-tab-{index}"))
                                    .selected(selected)
                                    .set_position(index + 1, count)
                                    .accessibility_label(format!(
                                        "{} request tab {}",
                                        tag.method, title
                                    ))
                                    .track_focus(&focus)
                                    .key_context("RequestTab")
                                    .size_full()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .pl_3()
                                    .pr_7()
                                    .border_r_1()
                                    .border_b_1()
                                    .border_color(LINE.resolve(cx))
                                    .bg(if selected { PANEL } else { SIDEBAR }.resolve(cx))
                                    .focus_visible(|t| {
                                        t.bg(crate::ui::theme::ACCENT_SOFT.resolve(cx))
                                    })
                                    .child(
                                        div()
                                            .debug_selector(move || {
                                                format!("request-tab-method-{index}")
                                            })
                                            .flex_none()
                                            .font_family(FONT_MONO)
                                            .text_size(m::CAPTION)
                                            .text_color(method_color(tag.method).resolve(cx))
                                            .child(tag.method.to_string()),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .text_size(rems(11. / 16.))
                                            .text_color(
                                                if selected { TEXT } else { MUTED }.resolve(cx),
                                            )
                                            .child(title),
                                    )
                                    .when(tag.dirty, |t| {
                                        t.child(
                                            div()
                                                .size(rems(5. / 16.))
                                                .flex_none()
                                                .rounded_full()
                                                .bg(ACCENT.resolve(cx)),
                                        )
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        mouse_focus.focus(window, cx);
                                        this.activate_request_tab(id, window, cx);
                                    }))
                                    .on_action(cx.listener(
                                        move |this, _: &ActivateRequestTab, window, cx| {
                                            this.activate_request_tab(id, window, cx)
                                        },
                                    ))
                                    .on_action(cx.listener(
                                        |this, _: &ActivateNextRequestTab, window, cx| {
                                            this.activate_relative_tab(1, window, cx)
                                        },
                                    ))
                                    .on_action(cx.listener(
                                        |this, _: &ActivatePreviousRequestTab, window, cx| {
                                            this.activate_relative_tab(-1, window, cx)
                                        },
                                    ))
                                    .on_action(cx.listener(
                                        move |this, _: &ActivateTabAbove, window, cx| {
                                            this.activate_relative_tab(
                                                -(columns as isize),
                                                window,
                                                cx,
                                            )
                                        },
                                    ))
                                    .on_action(cx.listener(
                                        move |this, _: &ActivateTabBelow, window, cx| {
                                            this.activate_relative_tab(columns as isize, window, cx)
                                        },
                                    ))
                                    .on_action(cx.listener(
                                        |this, _: &ActivateFirstRequestTab, window, cx| {
                                            this.activate_boundary_tab(false, window, cx)
                                        },
                                    ))
                                    .on_action(cx.listener(
                                        |this, _: &ActivateLastRequestTab, window, cx| {
                                            this.activate_boundary_tab(true, window, cx)
                                        },
                                    )),
                            )
                            .when(selected, |t| {
                                t.child(
                                    div()
                                        .absolute()
                                        .top_0()
                                        .left_0()
                                        .right_0()
                                        .h(rems(2. / 16.))
                                        .bg(ACCENT.resolve(cx)),
                                )
                            })
                            .child(
                                div().absolute().right_1().top(rems(7. / 16.)).child(
                                    gpui_kit::base::Button::new(("request-close", id.0))
                                        .debug_selector(move || format!("close-tab-{index}"))
                                        .accessibility_label(format!(
                                            "Close {} request tab",
                                            tag.display_name
                                        ))
                                        .track_focus(&close_focus)
                                        .size_6()
                                        .rounded(m::RADIUS)
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_color(MUTED.resolve(cx))
                                        .hover(|s| s.bg(crate::ui::theme::ACCENT_SOFT.resolve(cx)))
                                        .focus_visible(|s| {
                                            s.border_1().border_color(ACCENT.resolve(cx))
                                        })
                                        .child(
                                            gpui_kit::component::Icon::new(IconName::X)
                                                .size(m::SMALL_ICON),
                                        )
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.close_request_tab(id, window, cx);
                                            this.focus_active_request_tab(window, cx);
                                        })),
                                ),
                            )
                    })),
            )
            .child(
                div()
                    .w(rems(2.75))
                    .h(row_height)
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        kit_controls::icon_button(
                            "new-tab-button",
                            IconName::Plus,
                            "New request tab",
                        )
                        .debug_selector(|| "new-tab-button".into())
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.new_request(window, cx);
                            this.focus_active_request_tab(window, cx);
                        })),
                    ),
            )
    }
    fn activate_relative_tab(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        self.activate_relative_request(delta, window, cx);
    }
    fn activate_boundary_tab(&mut self, last: bool, window: &mut Window, cx: &mut Context<Self>) {
        let model = self.view_model.read(cx);
        let tab = if last {
            model.tabs().last()
        } else {
            model.tabs().first()
        };
        let id: Option<RequestTabId> = tab.map(|t| t.tab_id());
        if let Some(id) = id {
            self.activate_request_tab(id, window, cx);
            self.focus_active_request_tab(window, cx);
        }
    }
}

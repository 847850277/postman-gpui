use super::{navigation::AppRoute, PostmanApp};
use crate::{
    app::{NewRequest, ToggleShortcutHelp},
    ui::{
        components::kit_controls,
        theme::{metrics as m, ACCENT, ACCENT_SOFT, BG, LINE, MUTED, SIDEBAR, SUBTEXT, TEXT},
    },
};
use gpui::{
    div, prelude::FluentBuilder, App, Context, InteractiveElement, IntoElement, ParentElement,
    Styled, Window,
};
use gpui_kit::{
    assets::IconName,
    component::{
        button::{Button, ButtonVariants},
        Icon, TitleBar,
    },
};

fn rail_button(
    id: &'static str,
    label: &'static str,
    icon: IconName,
    selected: bool,
    cx: &App,
) -> Button {
    Button::new(id)
        .ghost()
        .debug_selector(move || id.into())
        .accessibility_label(label)
        .toggled(selected)
        .w_full()
        .h(gpui::rems(3.5))
        .p_1()
        .bg(if selected { ACCENT_SOFT } else { SIDEBAR }.resolve(cx))
        .text_color(if selected { ACCENT } else { MUTED }.resolve(cx))
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .child(Icon::new(icon).size(m::ICON))
                .child(div().text_size(m::CAPTION).child(label)),
        )
}

impl PostmanApp {
    pub(super) fn render_top_header(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .debug_selector(|| "top-header".into())
            .flex_none()
            .child(
                TitleBar::new()
                    .h(m::TITLEBAR)
                    .bg(BG.resolve(cx))
                    .border_color(LINE.resolve(cx))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(m::BODY)
                            .text_color(TEXT.resolve(cx))
                            .child(div().font_weight(m::SEMIBOLD).child("Postman"))
                            .child(
                                div()
                                    .px_1()
                                    .border_1()
                                    .border_color(LINE.resolve(cx))
                                    .rounded_sm()
                                    .text_size(m::CAPTION)
                                    .text_color(SUBTEXT.resolve(cx))
                                    .child("GPUI"),
                            )
                            .child(
                                div()
                                    .ml_3()
                                    .pl_4()
                                    .border_l_1()
                                    .border_color(LINE.resolve(cx))
                                    .text_size(m::LABEL)
                                    .text_color(MUTED.resolve(cx))
                                    .child(self.route.label()),
                            ),
                    )
                    .child(div().mr_3().child(self.render_global_search(window, cx))),
            )
    }

    pub(super) fn render_left_rail(
        &self,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("left-rail")
            .debug_selector(|| "left-rail".into())
            .w(m::RAIL)
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .p_2()
            .bg(SIDEBAR.resolve(cx))
            .border_r_1()
            .border_color(LINE.resolve(cx))
            .children(
                [
                    (AppRoute::Home, "nav-home", "Home", IconName::House),
                    (AppRoute::Http, "nav-http", "HTTP", IconName::Terminal),
                    (AppRoute::Flows, "nav-flows", "Flows", IconName::Workflow),
                ]
                .into_iter()
                .map(|(route, id, label, icon)| {
                    rail_button(id, label, icon, self.route == route, cx).on_click(
                        cx.listener(move |this, _, window, cx| this.navigate(route, window, cx)),
                    )
                }),
            )
            .when(self.route == AppRoute::Http, |rail| {
                rail.child(
                    div()
                        .w_6()
                        .my_2()
                        .border_t_1()
                        .border_color(LINE.resolve(cx)),
                )
                .child(
                    kit_controls::icon_button("rail-new-request", IconName::Plus, "New request")
                        .debug_selector(|| "rail-new-request".into())
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.new_request_command(&NewRequest, window, cx)
                        })),
                )
                .child(
                    rail_button(
                        "rail-history",
                        "History",
                        IconName::RotateCcw,
                        self.history_panel_open,
                        cx,
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.history_panel_open = !this.history_panel_open;
                        if this.history_panel_open {
                            this.history_list
                                .update(cx, |history, cx| history.focus_search(window, cx));
                        } else {
                            this.app_focus_handle.focus(window, cx);
                        }
                        cx.notify();
                    })),
                )
                .child(
                    rail_button("rail-search", "Search", IconName::Search, false, cx).on_click(
                        cx.listener(|this, _, window, cx| {
                            this.begin_global_search_focus(window, cx)
                        }),
                    ),
                )
                .child(
                    gpui_kit::base::Button::new("cookie-jar-trigger")
                        .debug_selector(|| "cookie-jar-trigger".into())
                        .accessibility_label("Cookies")
                        .track_focus(&self.cookie_trigger_focus)
                        .key_context("OverlayTrigger")
                        .w_full()
                        .h(gpui::rems(3.5))
                        .p_1()
                        .rounded(m::RADIUS)
                        .bg(if self.cookie_jar_open {
                            ACCENT_SOFT
                        } else {
                            SIDEBAR
                        }
                        .resolve(cx))
                        .text_color(MUTED.resolve(cx))
                        .hover(|style| style.bg(ACCENT_SOFT.resolve(cx)))
                        .focus_visible(|style| style.border_1().border_color(ACCENT.resolve(cx)))
                        .child(
                            div()
                                .size_full()
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .gap_1()
                                .child(Icon::new(IconName::Cookie).size(m::ICON))
                                .child(div().text_size(m::CAPTION).child("Cookies")),
                        )
                        .on_click(
                            cx.listener(|this, _, window, cx| this.toggle_cookie_jar(window, cx)),
                        ),
                )
            })
            .child(div().flex_1().min_h_0())
            .child(
                crate::app::appearance::button("appearance-toggle", cx)
                    .debug_selector(|| "appearance-toggle".into()),
            )
            .child(
                div()
                    .w_6()
                    .my_2()
                    .border_t_1()
                    .border_color(LINE.resolve(cx)),
            )
            .child(
                kit_controls::icon_button(
                    "shortcut-help-button",
                    IconName::Keyboard,
                    "Keyboard shortcuts",
                )
                .debug_selector(|| "shortcut-help-button".into())
                .on_click(cx.listener(|this, _, window, cx| {
                    this.toggle_shortcut_help(&ToggleShortcutHelp, window, cx)
                })),
            )
    }

    pub(super) fn render_status_bar(&self, cx: &Context<Self>) -> impl IntoElement {
        let model = self.view_model.read(cx);
        let running = model.tabs().iter().filter(|tab| tab.is_sending()).count();
        let status = if running > 0 {
            format!("Sending {running} request(s)…")
        } else {
            match self.route {
                AppRoute::Home => "Ready when you are",
                AppRoute::Http => "Requests stay open in this session",
                AppRoute::Flows => "Flow editing is not available yet",
            }
            .into()
        };
        div()
            .debug_selector(|| "status-bar".into())
            .h(m::STATUSBAR)
            .flex_none()
            .px_3()
            .flex()
            .items_center()
            .gap_3()
            .border_t_1()
            .border_color(LINE.resolve(cx))
            .bg(BG.resolve(cx))
            .text_size(m::CAPTION)
            .text_color(MUTED.resolve(cx))
            .child(status)
            .when_some(
                crate::app::appearance::Appearance::error(cx)
                    .or_else(|| crate::app::http_layout::HttpLayoutPreferences::error(cx))
                    .map(str::to_owned),
                |bar, error| {
                    bar.child(
                        div()
                            .text_color(crate::ui::theme::ERROR.resolve(cx))
                            .child(error),
                    )
                },
            )
            .child(div().flex_1())
            .when(self.route == AppRoute::Http, |bar| {
                bar.child("⌘/Ctrl ↵  Send request")
            })
    }
}

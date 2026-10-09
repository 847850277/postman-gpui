use super::{PostmanApp, LEFT_RAIL_WIDTH};
use crate::{
    app::{ActivateControl, NewRequest, ToggleShortcutHelp},
    ui::theme::{
        ACCENT, ACCENT_DARK, ACCENT_SOFT, ACCENT_VIVID, FONT_HEADING, FONT_UI, INFO, INFO_SOFT,
        LINE, PANEL, PANEL_ALT, SUBTEXT, TEXT,
    },
};
use gpui::{
    div, prelude::FluentBuilder, px, Context, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Role, StatefulInteractiveElement, Styled, Window,
};

impl PostmanApp {
    pub(super) fn render_top_header(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let cookie_count = self.view_model.read(cx).cookie_count();
        let cookie_jar_open = self.cookie_jar_open;
        div()
            .debug_selector(|| "top-header".into())
            .h(px(72.0))
            .flex_none()
            .flex()
            .items_center()
            .px_5()
            .bg(PANEL.resolve(cx))
            .border_b_1()
            .border_color(LINE.resolve(cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .size(px(20.0))
                            .rounded_full()
                            .bg(ACCENT_VIVID.resolve(cx)),
                    )
                    .child(
                        div()
                            .font_family(FONT_HEADING)
                            .text_size(px(22.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(TEXT.resolve(cx))
                            .child("Postman GPUI"),
                    ),
            )
            .child(div().flex_1())
            .child(self.render_global_search(window, cx))
            .when(
                cx.try_global::<gpui_kit::component::Theme>().is_some(),
                |header| header.child(crate::app::appearance::button("appearance-toggle", cx)),
            )
            .when_some(
                crate::app::appearance::Appearance::error(cx).map(str::to_owned),
                |header, error| {
                    header.child(
                        div()
                            .text_size(px(11.))
                            .text_color(crate::ui::theme::ERROR.resolve(cx))
                            .child(error),
                    )
                },
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("cookie-jar-trigger")
                    .debug_selector(|| "cookie-jar-trigger".into())
                    .track_focus(&self.cookie_trigger_focus)
                    .key_context("KeyboardButton OverlayTrigger")
                    .role(Role::Button)
                    .aria_label(format!("Cookie Jar, {cookie_count} stored"))
                    .h(px(34.0))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded_lg()
                    .border_1()
                    .border_color((if cookie_jar_open { INFO } else { LINE }).resolve(cx))
                    .bg(INFO_SOFT.resolve(cx))
                    .font_family(FONT_UI)
                    .text_size(px(11.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(INFO.resolve(cx))
                    .cursor_pointer()
                    .hover(|style| {
                        style
                            .border_color(INFO.resolve(cx))
                            .bg(PANEL_ALT.resolve(cx))
                    })
                    .when(self.cookie_trigger_focus.is_focused(window), |button| {
                        button.border_1().border_color(ACCENT.resolve(cx))
                    })
                    .child("◫")
                    .child(format!("Cookie Jar · {cookie_count} stored"))
                    .on_action(cx.listener(|this, _: &ActivateControl, window, cx| {
                        this.toggle_cookie_jar(window, cx)
                    }))
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.cookie_trigger_focus.focus(window, cx);
                            this.toggle_cookie_jar(window, cx);
                        }),
                    ),
            )
            .child(
                div()
                    .id("shortcut-help-button")
                    .debug_selector(|| "shortcut-help-button".into())
                    .track_focus(&self.shortcut_help_button_focus)
                    .key_context("KeyboardButton")
                    .role(Role::Button)
                    .aria_label("Keyboard shortcuts")
                    .ml_2()
                    .size(px(34.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_lg()
                    .border_1()
                    .border_color(LINE.resolve(cx))
                    .bg(PANEL_ALT.resolve(cx))
                    .font_family(FONT_UI)
                    .font_weight(FontWeight::BOLD)
                    .text_color(SUBTEXT.resolve(cx))
                    .cursor_pointer()
                    .hover(|style| {
                        style
                            .bg(ACCENT_SOFT.resolve(cx))
                            .text_color(ACCENT_DARK.resolve(cx))
                    })
                    .when(
                        self.shortcut_help_button_focus.is_focused(window),
                        |button| {
                            button
                                .border_color(ACCENT.resolve(cx))
                                .text_color(ACCENT.resolve(cx))
                        },
                    )
                    .child("⌘")
                    .on_action(cx.listener(|this, _: &ActivateControl, window, cx| {
                        this.toggle_shortcut_help(&ToggleShortcutHelp, window, cx)
                    }))
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.shortcut_help_button_focus.focus(window, cx);
                            this.toggle_shortcut_help(&ToggleShortcutHelp, window, cx);
                        }),
                    ),
            )
    }

    pub(super) fn render_left_rail(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let passive_slots = ["?"];
        div()
            .debug_selector(|| "left-rail".into())
            .w(px(LEFT_RAIL_WIDTH))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap_4()
            .px_2()
            .py_3()
            .bg(PANEL.resolve(cx))
            .border_r_1()
            .border_color(LINE.resolve(cx))
            .child(
                div()
                    .id(("rail-slot", 0usize))
                    .debug_selector(|| "rail-new-request".into())
                    .track_focus(&self.new_request_focus)
                    .key_context("KeyboardButton")
                    .role(Role::Button)
                    .aria_label("New request")
                    .size(px(40.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_lg()
                    .bg(ACCENT_SOFT.resolve(cx))
                    .text_color(ACCENT_DARK.resolve(cx))
                    .font_family(FONT_UI)
                    .text_size(px(22.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .hover(|style| style.bg(crate::ui::theme::ACCENT_SOFT.resolve(cx)))
                    .when(self.new_request_focus.is_focused(window), |button| {
                        button.border_1().border_color(ACCENT.resolve(cx))
                    })
                    .child("+")
                    .on_action(cx.listener(|this, _: &ActivateControl, window, cx| {
                        this.new_request_command(&NewRequest, window, cx)
                    }))
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.new_request_focus.focus(window, cx);
                            this.new_request_command(&NewRequest, window, cx);
                        }),
                    ),
            )
            .children(passive_slots.into_iter().enumerate().map(|(index, label)| {
                div()
                    .id(("rail-slot", index + 1))
                    .size(px(40.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_lg()
                    .bg(PANEL_ALT.resolve(cx))
                    .text_color(SUBTEXT.resolve(cx))
                    .font_family(FONT_UI)
                    .text_size(px(16.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(label)
            }))
    }
}

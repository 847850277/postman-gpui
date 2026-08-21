use super::PostmanApp;
use crate::ui::theme::{
    ACCENT_DARK, ACCENT_SOFT, ACCENT_VIVID, FONT_HEADING, FONT_UI, INFO, INFO_SOFT, LINE, PANEL,
    PANEL_ALT, SUBTEXT, TEXT,
};
use gpui::{
    div, px, rgb, Context, FontWeight, InteractiveElement, IntoElement, ParentElement, Styled,
};

impl PostmanApp {
    pub(super) fn render_top_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let cookie_count = self.view_model.read(cx).cookie_count();
        let cookie_jar_open = self.cookie_jar_open;
        div()
            .debug_selector(|| "top-header".into())
            .h(px(72.0))
            .flex_none()
            .flex()
            .items_center()
            .px_5()
            .bg(rgb(PANEL))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().size(px(20.0)).rounded_full().bg(rgb(ACCENT_VIVID)))
                    .child(
                        div()
                            .font_family(FONT_HEADING)
                            .text_size(px(22.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(TEXT))
                            .child("Postman GPUI"),
                    ),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("cookie-jar-trigger")
                    .debug_selector(|| "cookie-jar-trigger".into())
                    .h(px(34.0))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(if cookie_jar_open { INFO } else { LINE }))
                    .bg(rgb(INFO_SOFT))
                    .font_family(FONT_UI)
                    .text_size(px(11.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(INFO))
                    .cursor_pointer()
                    .hover(|style| style.border_color(rgb(INFO)).bg(rgb(PANEL_ALT)))
                    .child("◫")
                    .child(format!("Cookie Jar · {cookie_count} stored"))
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.toggle_cookie_jar(cx)),
                    ),
            )
    }

    pub(super) fn render_left_rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let passive_slots = ["↻", "◫", "◇", "⌘", "⚙", "?"];
        div()
            .debug_selector(|| "left-rail".into())
            .w(px(72.0))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap_4()
            .px_2()
            .py_3()
            .bg(rgb(PANEL))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .id(("rail-slot", 0usize))
                    .debug_selector(|| "rail-new-request".into())
                    .size(px(40.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_lg()
                    .bg(rgb(ACCENT_SOFT))
                    .text_color(rgb(ACCENT_DARK))
                    .font_family(FONT_UI)
                    .text_size(px(22.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(0x00ff_e4d5)))
                    .child("+")
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.new_request(cx)),
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
                    .bg(rgb(PANEL_ALT))
                    .text_color(rgb(SUBTEXT))
                    .font_family(FONT_UI)
                    .text_size(px(16.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(label)
            }))
    }
}

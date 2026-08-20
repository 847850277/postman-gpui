use super::RequestWorkspace;
use crate::ui::theme::{
    method_color, ACCENT, ACCENT_DARK, ACCENT_SOFT, FONT_HEADING, FONT_UI, LINE, MUTED, PANEL,
    PANEL_ALT, SUBTEXT,
};
use gpui::{
    div, prelude::FluentBuilder, px, rgb, Context, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Styled,
};

impl RequestWorkspace {
    pub(super) fn render_request_tabs_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
                        request.method(),
                        request.tab_title(),
                        request.is_dirty(),
                        index == active_index,
                    )
                })
                .collect()
        };

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
            .children(
                tabs.into_iter()
                    .map(|(index, method, title, dirty, active)| {
                        div()
                            .debug_selector(move || format!("request-tab-{index}"))
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
                                    .debug_selector(move || format!("close-tab-{index}"))
                                    .size(px(20.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .text_color(rgb(MUTED))
                                    .hover(|style| {
                                        style.bg(rgb(ACCENT_SOFT)).text_color(rgb(ACCENT_DARK))
                                    })
                                    .child("×")
                                    .on_mouse_up(
                                        gpui::MouseButton::Left,
                                        cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.close_request_tab(index, cx);
                                        }),
                                    ),
                            )
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    this.select_request_tab(index, cx)
                                }),
                            )
                    }),
            )
            .child(
                div()
                    .debug_selector(|| "new-tab-button".into())
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
                    .child("+")
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.new_request(cx)),
                    ),
            )
    }
}

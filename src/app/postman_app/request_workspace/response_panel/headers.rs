use crate::ui::theme::{
    CODE_BG, CODE_TEXT, FONT_HEADING, FONT_MONO, FONT_UI, INFO, LINE, MUTED, PANEL_ALT, SUBTEXT,
    TEXT,
};
use gpui::{
    div, prelude::FluentBuilder, px, rgb, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Role, StatefulInteractiveElement, Styled,
};

pub(super) fn render_response_headers(headers: &[(String, String)]) -> impl IntoElement {
    let header_count = headers.len();
    let rows = headers.to_vec();

    div()
        .id("response-content")
        .debug_selector(|| "response-content".into())
        .role(Role::TabPanel)
        .aria_label("Response headers")
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .bg(rgb(CODE_BG))
        .child(
            div()
                .debug_selector(|| "response-headers-summary".into())
                .h(px(40.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_between()
                .px_4()
                .border_b_1()
                .border_color(rgb(LINE))
                .bg(rgb(PANEL_ALT))
                .child(
                    div()
                        .font_family(FONT_UI)
                        .font_weight(FontWeight::BOLD)
                        .text_size(px(12.0))
                        .text_color(rgb(TEXT))
                        .child("Response headers"),
                )
                .child(
                    div()
                        .debug_selector(|| "response-headers-count".into())
                        .font_family(FONT_MONO)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_size(px(10.0))
                        .text_color(rgb(SUBTEXT))
                        .child(format!("{header_count} rows")),
                ),
        )
        .when(header_count == 0, |panel| {
            panel.child(
                div()
                    .debug_selector(|| "response-headers-empty".into())
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .p_4()
                    .child(
                        div()
                            .font_family(FONT_HEADING)
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(16.0))
                            .text_color(rgb(TEXT))
                            .child("No response headers"),
                    )
                    .child(
                        div()
                            .font_family(FONT_UI)
                            .text_size(px(11.0))
                            .text_color(rgb(SUBTEXT))
                            .child(
                                "The response body remains available. Send another request to inspect headers.",
                            ),
                    ),
            )
        })
        .when(header_count > 0, |panel| {
            panel.child(
                div()
                    .debug_selector(|| "response-headers-table".into())
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(
                        div()
                            .debug_selector(|| "response-headers-table-labels".into())
                            .h(px(30.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .px_4()
                            .bg(rgb(PANEL_ALT))
                            .border_b_1()
                            .border_color(rgb(LINE))
                            .font_family(FONT_UI)
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(9.0))
                            .text_color(rgb(MUTED))
                            .child(div().w_1_3().child("HEADER"))
                            .child(div().flex_1().min_w_0().child("VALUE")),
                    )
                    .child(
                        div()
                            .id("response-headers-rows")
                            .debug_selector(|| "response-headers-rows".into())
                            .flex_1()
                            .min_h_0()
                            .overflow_scroll()
                            .children(rows.into_iter().enumerate().map(
                                |(index, (name, value))| {
                                    div()
                                        .debug_selector(move || {
                                            format!("response-header-row-{index}")
                                        })
                                        .min_h(px(38.0))
                                        .flex()
                                        .items_start()
                                        .px_4()
                                        .py_2()
                                        .border_b_1()
                                        .border_color(rgb(LINE))
                                        .when(index % 2 == 1, |row| row.bg(rgb(PANEL_ALT)))
                                        .child(
                                            div()
                                                .debug_selector(move || {
                                                    format!("response-header-name-{index}")
                                                })
                                                .w_1_3()
                                                .pr_3()
                                                .font_family(FONT_MONO)
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_size(px(11.0))
                                                .text_color(rgb(INFO))
                                                .child(name),
                                        )
                                        .child(
                                            div()
                                                .debug_selector(move || {
                                                    format!("response-header-value-{index}")
                                                })
                                                .flex_1()
                                                .min_w_0()
                                                .font_family(FONT_MONO)
                                                .text_size(px(11.0))
                                                .text_color(rgb(CODE_TEXT))
                                                .child(value),
                                        )
                                },
                            )),
                    ),
            )
        })
}

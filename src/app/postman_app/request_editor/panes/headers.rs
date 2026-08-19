use super::super::{
    layout::{header_row_complete, row_scrollbar_geometry, visible_row_capacity},
    RequestEditor,
};
use crate::{
    app::RequestPane,
    ui::theme::{
        ACCENT, ACCENT_SOFT, ERROR, FONT_MONO, FONT_UI, INFO, INFO_SOFT, LINE, MUTED, OK, OK_SOFT,
        PANEL, PANEL_ALT, SUBTEXT, TEXT,
    },
};
use gpui::{
    div, prelude::FluentBuilder, px, relative, rgb, Context, FontWeight, InteractiveElement,
    IntoElement, ParentElement, StatefulInteractiveElement, Styled,
};

impl RequestEditor {
    pub(in crate::app::postman_app::request_editor) fn render_headers_editor(
        &self,
        panel_height: f32,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let row_editors = self.header_row_editors.clone();
        let (rows, draft_key, draft_value, visible_row_count, enabled_count) = {
            let view_model = self.view_model.read(cx);
            let (draft_key, draft_value) = view_model
                .row_draft(RequestPane::Headers)
                .unwrap_or_default();
            (
                view_model.headers().to_vec(),
                draft_key.to_string(),
                draft_value.to_string(),
                view_model.visible_header_row_count(),
                view_model.enabled_header_count(),
            )
        };
        let disabled_count = rows
            .iter()
            .filter(|row| header_row_complete(row) && !row.enabled)
            .count();
        let draft_complete = !draft_key.trim().is_empty() && !draft_value.trim().is_empty();
        let draft_index = visible_row_count - 1;
        let draft_row_selector = format!("header-row-{draft_index}");
        let draft_toggle_selector = format!("header-row-toggle-{draft_index}");
        let draft_key_selector = format!("header-row-key-{draft_index}");
        let draft_key_input_selector = format!("header-row-key-input-{draft_index}");
        let draft_value_selector = format!("header-row-value-{draft_index}");
        let draft_value_input_selector = format!("header-row-value-input-{draft_index}");
        let draft_status_selector = format!("header-row-status-{draft_index}");
        let draft_delete_selector = format!("header-row-delete-{draft_index}");
        let visible_capacity = visible_row_capacity(RequestPane::Headers, panel_height);
        let show_scrollbar = visible_row_count > visible_capacity;
        let scrollbar = row_scrollbar_geometry(
            visible_row_count,
            visible_capacity,
            self.header_rows_scroll_handle.offset().y.as_f32(),
            self.header_rows_scroll_handle.max_offset().y.as_f32(),
        );

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .child(
                div()
                    .debug_selector(|| "headers-summary".into())
                    .h(px(42.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .px_3()
                    .font_family(FONT_UI)
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_none()
                                    .text_color(rgb(TEXT))
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(12.0))
                                    .child("Request headers"),
                            )
                            .child(
                                div()
                                    .overflow_hidden()
                                    .text_size(px(11.0))
                                    .text_color(rgb(SUBTEXT))
                                    .child("Disabled rows stay saved but are excluded from Send"),
                            ),
                    )
                    .child(
                        div()
                            .debug_selector(|| "headers-enabled-count".into())
                            .h(px(24.0))
                            .px_2()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap_1()
                            .rounded_lg()
                            .bg(rgb(OK_SOFT))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(10.0))
                            .text_color(rgb(OK))
                            .child("●")
                            .child(format!(
                                "{enabled_count} enabled · {disabled_count} disabled"
                            )),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "headers-table-header".into())
                    .h(px(32.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .bg(rgb(PANEL_ALT))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .font_family(FONT_UI)
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(10.0))
                    .text_color(rgb(SUBTEXT))
                    .child(div().w(px(18.0)))
                    .child(div().flex_1().child("KEY"))
                    .child(div().flex_1().child("VALUE"))
                    .child(
                        div()
                            .w(px(112.0))
                            .text_align(gpui::TextAlign::Center)
                            .child("ACTION"),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .relative()
                    .child(
                        div()
                            .id("headers-rows-scroll")
                            .debug_selector(|| "headers-rows-scroll".into())
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .p_3()
                            .when(show_scrollbar, |this| this.pr(px(22.0)))
                            .overflow_y_scroll()
                            .track_scroll(&self.header_rows_scroll_handle)
                            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
                            .children(rows.into_iter().zip(row_editors).enumerate().map(
                                |(index, (row, row_editor))| {
                                    let is_complete = header_row_complete(&row);
                                    let is_sent = row.enabled && is_complete;
                                    let (status, status_bg, status_color) = if !is_complete {
                                        ("DRAFT", PANEL_ALT, SUBTEXT)
                                    } else if row.enabled {
                                        ("SENT", OK_SOFT, OK)
                                    } else {
                                        ("EXCLUDED", ACCENT_SOFT, ACCENT)
                                    };
                                    let row_selector = format!("header-row-{index}");
                                    let toggle_selector = format!("header-row-toggle-{index}");
                                    let status_selector = format!("header-row-status-{index}");
                                    let delete_selector = format!("header-row-delete-{index}");

                                    div()
                                        .debug_selector(move || row_selector.clone())
                                        .h(px(40.0))
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .font_family(FONT_MONO)
                                        .text_size(px(12.0))
                                        .child(
                                            div()
                                                .debug_selector(move || toggle_selector.clone())
                                                .size(px(18.0))
                                                .flex_none()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded_sm()
                                                .border_1()
                                                .border_color(rgb(if is_sent {
                                                    INFO
                                                } else {
                                                    LINE
                                                }))
                                                .bg(rgb(if is_sent { INFO } else { PANEL }))
                                                .text_color(rgb(PANEL))
                                                .cursor_pointer()
                                                .child(if is_sent { "✓" } else { "" })
                                                .on_mouse_up(
                                                    gpui::MouseButton::Left,
                                                    cx.listener(move |this, _, _, cx| {
                                                        this.toggle_header(index, cx)
                                                    }),
                                                ),
                                        )
                                        .child(row_editor)
                                        .child(
                                            div()
                                                .w(px(112.0))
                                                .flex_none()
                                                .flex()
                                                .items_center()
                                                .justify_between()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .debug_selector(move || {
                                                            status_selector.clone()
                                                        })
                                                        .h(px(24.0))
                                                        .w(px(76.0))
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .rounded_lg()
                                                        .bg(rgb(status_bg))
                                                        .font_family(FONT_UI)
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .text_size(px(9.0))
                                                        .text_color(rgb(status_color))
                                                        .child(status),
                                                )
                                                .child(
                                                    div()
                                                        .debug_selector(move || {
                                                            delete_selector.clone()
                                                        })
                                                        .size(px(28.0))
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .rounded_lg()
                                                        .cursor_pointer()
                                                        .text_color(rgb(MUTED))
                                                        .hover(|style| {
                                                            style
                                                                .bg(rgb(ACCENT_SOFT))
                                                                .text_color(rgb(ERROR))
                                                        })
                                                        .child("×")
                                                        .on_mouse_up(
                                                            gpui::MouseButton::Left,
                                                            cx.listener(move |this, _, _, cx| {
                                                                this.remove_header(index, cx)
                                                            }),
                                                        ),
                                                ),
                                        )
                                },
                            ))
                            .child(
                                div()
                                    .debug_selector(move || draft_row_selector.clone())
                                    .h(px(40.0))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .font_family(FONT_MONO)
                                    .text_size(px(12.0))
                                    .child(
                                        div()
                                            .debug_selector(move || draft_toggle_selector.clone())
                                            .size(px(18.0))
                                            .flex_none()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_sm()
                                            .border_1()
                                            .border_color(rgb(if draft_complete {
                                                INFO
                                            } else {
                                                LINE
                                            }))
                                            .bg(rgb(if draft_complete { INFO } else { PANEL }))
                                            .text_color(rgb(PANEL))
                                            .child(if draft_complete { "✓" } else { "" })
                                            .when(draft_complete, |this| {
                                                this.cursor_pointer().on_mouse_up(
                                                    gpui::MouseButton::Left,
                                                    cx.listener(|this, _, _, cx| {
                                                        this.toggle_header_draft(cx)
                                                    }),
                                                )
                                            }),
                                    )
                                    .child(
                                        div()
                                            .debug_selector(move || draft_key_selector.clone())
                                            .h_full()
                                            .min_w_0()
                                            .flex_1()
                                            .child(
                                                div()
                                                    .debug_selector(move || {
                                                        draft_key_input_selector.clone()
                                                    })
                                                    .h_full()
                                                    .child(
                                                        div()
                                                            .debug_selector(|| {
                                                                "row-key-input".into()
                                                            })
                                                            .h_full()
                                                            .child(self.row_key_input.clone()),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .debug_selector(move || draft_value_selector.clone())
                                            .h_full()
                                            .min_w_0()
                                            .flex_1()
                                            .child(
                                                div()
                                                    .debug_selector(move || {
                                                        draft_value_input_selector.clone()
                                                    })
                                                    .h_full()
                                                    .child(
                                                        div()
                                                            .debug_selector(|| {
                                                                "row-value-input".into()
                                                            })
                                                            .h_full()
                                                            .child(self.row_value_input.clone()),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .w(px(112.0))
                                            .flex_none()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .debug_selector(move || {
                                                        draft_status_selector.clone()
                                                    })
                                                    .h(px(24.0))
                                                    .w(px(76.0))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .rounded_lg()
                                                    .bg(rgb(if draft_complete {
                                                        OK_SOFT
                                                    } else {
                                                        PANEL_ALT
                                                    }))
                                                    .font_family(FONT_UI)
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_size(px(9.0))
                                                    .text_color(rgb(if draft_complete {
                                                        OK
                                                    } else {
                                                        SUBTEXT
                                                    }))
                                                    .child(if draft_complete {
                                                        "SENT"
                                                    } else {
                                                        "DRAFT"
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .debug_selector(move || {
                                                        draft_delete_selector.clone()
                                                    })
                                                    .size(px(28.0))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .rounded_lg()
                                                    .cursor_pointer()
                                                    .text_color(rgb(MUTED))
                                                    .hover(|style| {
                                                        style
                                                            .bg(rgb(ACCENT_SOFT))
                                                            .text_color(rgb(ERROR))
                                                    })
                                                    .child("×")
                                                    .on_mouse_up(
                                                        gpui::MouseButton::Left,
                                                        cx.listener(|this, _, _, cx| {
                                                            this.clear_header_draft(cx)
                                                        }),
                                                    ),
                                            ),
                                    ),
                            ),
                    )
                    .when_some(scrollbar, |this, scrollbar| {
                        this.child(
                            div()
                                .debug_selector(|| "headers-scrollbar".into())
                                .absolute()
                                .top(px(8.0))
                                .right(px(5.0))
                                .bottom(px(8.0))
                                .w(px(8.0))
                                .rounded_full()
                                .bg(rgb(PANEL_ALT))
                                .border_1()
                                .border_color(rgb(LINE))
                                .child(
                                    div()
                                        .debug_selector(|| "headers-scrollbar-thumb".into())
                                        .absolute()
                                        .top(relative(scrollbar.thumb_top))
                                        .w_full()
                                        .h(relative(scrollbar.thumb_height))
                                        .rounded_full()
                                        .bg(rgb(INFO)),
                                ),
                        )
                    }),
            )
            .child(
                div()
                    .h(px(44.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .px_3()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(INFO_SOFT))
                    .child(
                        div()
                            .debug_selector(|| "add-row-button".into())
                            .h(px(32.0))
                            .w_full()
                            .px_3()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(LINE))
                            .bg(rgb(PANEL_ALT))
                            .text_color(rgb(SUBTEXT))
                            .font_family(FONT_UI)
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .hover(|style| {
                                style
                                    .bg(rgb(INFO_SOFT))
                                    .border_color(rgb(INFO))
                                    .text_color(rgb(INFO))
                            })
                            .child("＋ Add another header row")
                            .child(
                                div()
                                    .font_weight(FontWeight::NORMAL)
                                    .text_color(rgb(MUTED))
                                    .child("Click repeatedly — rows are unlimited"),
                            )
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.add_current_row(cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "headers-ready-indicator".into())
                    .h(px(54.0))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap_2()
                    .px_3()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .font_family(FONT_UI)
                    .text_size(px(10.0))
                    .text_color(rgb(SUBTEXT))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().text_color(rgb(OK)).child("✓"))
                            .child("Ready to send — active values are already in the ViewModel"),
                    )
                    .child(
                        div().font_family(FONT_MONO).text_color(rgb(INFO)).child(
                            "Only complete, checked rows participate in request construction",
                        ),
                    ),
            )
            .into_any_element()
    }
}

use super::super::{
    layout::{row_scrollbar_geometry, visible_row_capacity},
    RequestEditor,
};
use crate::{
    app::RequestPane,
    ui::theme::{
        ACCENT_SOFT, ERROR, FONT_MONO, FONT_UI, INFO, INFO_SOFT, LINE, MUTED, OK, OK_SOFT, PANEL,
        PANEL_ALT, SUBTEXT, TEXT,
    },
};
use gpui::{
    div, prelude::FluentBuilder, px, relative, rgb, Context, FontWeight, InteractiveElement,
    IntoElement, ParentElement, StatefulInteractiveElement, Styled,
};

impl RequestEditor {
    pub(in crate::app::postman_app::request_editor) fn render_params_editor(
        &self,
        panel_height: f32,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let row_editors = self.param_row_editors.clone();
        let (rows, draft_key, visible_row_count, enabled_count, effective_url) = {
            let view_model = self.view_model.read(cx);
            let (draft_key, _) = view_model
                .row_draft(RequestPane::Params)
                .unwrap_or_default();
            (
                view_model.params().to_vec(),
                draft_key.to_string(),
                view_model.visible_param_row_count(),
                view_model.enabled_param_count(),
                view_model.effective_url(),
            )
        };
        let draft_enabled = !draft_key.trim().is_empty();
        let draft_index = visible_row_count - 1;
        let draft_row_selector = format!("param-row-{draft_index}");
        let draft_key_selector = format!("param-row-key-input-{draft_index}");
        let draft_value_selector = format!("param-row-value-input-{draft_index}");
        let visible_capacity = visible_row_capacity(RequestPane::Params, panel_height);
        let show_scrollbar = visible_row_count > visible_capacity;
        let scrollbar = row_scrollbar_geometry(
            visible_row_count,
            visible_capacity,
            self.param_rows_scroll_handle.offset().y.as_f32(),
            self.param_rows_scroll_handle.max_offset().y.as_f32(),
        );

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .child(
                div()
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
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(12.0))
                                    .text_color(rgb(TEXT))
                                    .child("Query parameters"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(rgb(SUBTEXT))
                                    .child("Synchronized with the URL query string"),
                            ),
                    )
                    .child(
                        div()
                            .debug_selector(|| "params-enabled-count".into())
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
                            .child(format!("{enabled_count} enabled")),
                    ),
            )
            .child(
                div()
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
                            .w(px(56.0))
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
                            .id("params-rows-scroll")
                            .debug_selector(|| "params-rows-scroll".into())
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .p_3()
                            .when(show_scrollbar, |this| this.pr(px(22.0)))
                            .overflow_y_scroll()
                            .track_scroll(&self.param_rows_scroll_handle)
                            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
                            .children(rows.into_iter().zip(row_editors).enumerate().map(
                                |(index, (row, row_editor))| {
                                    let is_enabled = row.enabled;
                                    let row_selector = format!("param-row-{index}");
                                    let toggle_selector = format!("param-row-toggle-{index}");
                                    let delete_selector = format!("param-row-delete-{index}");
                                    div()
                                        .debug_selector(move || row_selector.clone())
                                        .h(px(38.0))
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
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded_sm()
                                                .border_1()
                                                .border_color(rgb(if is_enabled {
                                                    INFO
                                                } else {
                                                    LINE
                                                }))
                                                .bg(rgb(if is_enabled { INFO } else { PANEL }))
                                                .text_color(rgb(PANEL))
                                                .cursor_pointer()
                                                .child(if is_enabled { "✓" } else { "" })
                                                .on_mouse_up(
                                                    gpui::MouseButton::Left,
                                                    cx.listener(move |this, _, _, cx| {
                                                        this.toggle_param(index, cx)
                                                    }),
                                                ),
                                        )
                                        .child(row_editor)
                                        .child(
                                            div()
                                                .debug_selector(move || delete_selector.clone())
                                                .w(px(56.0))
                                                .h(px(32.0))
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
                                                        this.remove_param(index, cx)
                                                    }),
                                                ),
                                        )
                                },
                            ))
                            .child(
                                div()
                                    .debug_selector(move || draft_row_selector.clone())
                                    .h(px(38.0))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .debug_selector(|| "params-draft-toggle".into())
                                            .size(px(18.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_sm()
                                            .border_1()
                                            .border_color(rgb(if draft_enabled {
                                                INFO
                                            } else {
                                                LINE
                                            }))
                                            .bg(rgb(if draft_enabled { INFO } else { PANEL }))
                                            .text_color(rgb(PANEL))
                                            .child(if draft_enabled { "✓" } else { "" }),
                                    )
                                    .child(
                                        div()
                                            .debug_selector(move || draft_key_selector.clone())
                                            .h_full()
                                            .flex_1()
                                            .child(
                                                div()
                                                    .debug_selector(|| "row-key-input".into())
                                                    .h_full()
                                                    .child(self.row_key_input.clone()),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .debug_selector(move || draft_value_selector.clone())
                                            .h_full()
                                            .flex_1()
                                            .child(
                                                div()
                                                    .debug_selector(|| "row-value-input".into())
                                                    .h_full()
                                                    .child(self.row_value_input.clone()),
                                            ),
                                    )
                                    .child(div().w(px(56.0)).h(px(32.0))),
                            ),
                    )
                    .when_some(scrollbar, |this, scrollbar| {
                        this.child(
                            div()
                                .debug_selector(|| "params-scrollbar".into())
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
                                        .debug_selector(|| "params-scrollbar-thumb".into())
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
                    .bg(rgb(PANEL))
                    .child(
                        div()
                            .debug_selector(|| "add-row-button".into())
                            .h(px(32.0))
                            .w_full()
                            .px_3()
                            .flex()
                            .items_center()
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
                            .child("＋ Add parameter")
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.add_current_row(cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "effective-url-preview".into())
                    .h(px(64.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .px_3()
                    .bg(rgb(INFO_SOFT))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .font_family(FONT_UI)
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(10.0))
                                    .text_color(rgb(INFO))
                                    .child("↗  EFFECTIVE URL"),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "effective-url-value".into())
                                    .overflow_hidden()
                                    .font_family(FONT_MONO)
                                    .text_size(px(11.0))
                                    .text_color(rgb(TEXT))
                                    .child(effective_url),
                            ),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .flex_none()
                            .rounded_lg()
                            .bg(rgb(PANEL))
                            .font_family(FONT_UI)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(10.0))
                            .text_color(rgb(INFO))
                            .child("encoded"),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "params-ready-indicator".into())
                    .h(px(34.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .font_family(FONT_UI)
                    .text_size(px(10.0))
                    .text_color(rgb(SUBTEXT))
                    .child(div().text_color(rgb(OK)).child("✓"))
                    .child("Ready to send — the active value is already in the ViewModel"),
            )
            .into_any_element()
    }
}

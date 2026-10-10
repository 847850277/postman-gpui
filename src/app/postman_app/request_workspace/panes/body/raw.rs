use crate::{
    app::{EffectiveHeader, EffectiveHeaderSource},
    models::HttpMethod,
    ui::theme::{FONT_MONO, FONT_UI, INFO, INFO_SOFT, LINE, OK, OK_SOFT, PANEL, SUBTEXT, TEXT},
};
use gpui::{
    div, prelude::FluentBuilder, px, FontWeight, InteractiveElement, IntoElement, ParentElement,
    ScrollHandle, StatefulInteractiveElement, Styled,
};
use gpui_kit::{
    base::ElementExt,
    component::scroll::{Scrollbar, ScrollbarMode},
};

struct RawSemanticsRow {
    selector: &'static str,
    value_selector: &'static str,
    mark: &'static str,
    key: &'static str,
    value: String,
    state: &'static str,
    success: bool,
}

pub(super) fn render_raw_request_semantics(
    body: &str,
    method: HttpMethod,
    effective_url: &str,
    effective_headers: Vec<EffectiveHeader>,
    scroll_handle: &ScrollHandle,
    has_overflow: bool,
    cx: &mut gpui::Context<super::BodyPane>,
) -> gpui::AnyElement {
    let generated_count = effective_headers
        .iter()
        .filter(|header| header.source == EffectiveHeaderSource::Generated)
        .count();
    let content_type = effective_headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("content-type"));
    let has_content_type = content_type.is_some();
    let (content_type_mark, content_type_value, content_type_state) = match content_type {
        Some(header) => (
            "i",
            header.value.clone(),
            if header.source == EffectiveHeaderSource::Generated {
                "AUTOMATIC"
            } else {
                "USER ROW"
            },
        ),
        None => ("∅", "not generated".to_string(), "ABSENT"),
    };
    let byte_count = body.len();
    let body_preview = if body.is_empty() {
        "(empty body)".to_string()
    } else {
        body.to_string()
    };

    div()
        .debug_selector(|| "body-raw-effective-request".into())
        .w_full()
        .flex_none()
        .min_h_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded_lg()
        .border_1()
        .border_color(LINE.resolve(cx))
        .bg(INFO_SOFT.resolve(cx))
        .child(
            div()
                .h(px(46.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_between()
                .px_3()
                .border_b_1()
                .border_color(LINE.resolve(cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .font_family(FONT_UI)
                                .font_weight(FontWeight::BOLD)
                                .text_size(px(12.0))
                                .text_color(TEXT.resolve(cx))
                                .child("Effective raw request"),
                        )
                        .child(
                            div()
                                .font_family(FONT_UI)
                                .text_size(px(9.0))
                                .text_color(SUBTEXT.resolve(cx))
                                .child(
                                    "Raw format supplies Content-Type unless Headers overrides it.",
                                ),
                        ),
                )
                .child(
                    div()
                        .debug_selector(|| "body-raw-generated-header-count".into())
                        .h(px(24.0))
                        .px_2()
                        .flex()
                        .items_center()
                        .rounded_lg()
                        .bg(PANEL.resolve(cx))
                        .font_family(FONT_UI)
                        .font_weight(FontWeight::BOLD)
                        .text_size(px(9.0))
                        .text_color(INFO.resolve(cx))
                        .child(format!("{generated_count} GENERATED")),
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
                        .id("body-raw-semantics-scroll")
                        .debug_selector(|| "body-raw-semantics-scroll".into())
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .when(has_overflow, |rows| rows.pr_4())
                        .overflow_y_scroll()
                        .track_scroll(scroll_handle)
                        .children([
                            render_raw_semantics_row(
                                RawSemanticsRow {
                                    selector: "body-raw-content-type-state",
                                    value_selector: "body-raw-content-type-value",
                                    mark: content_type_mark,
                                    key: "Content-Type",
                                    value: content_type_value,
                                    state: content_type_state,
                                    success: !has_content_type,
                                },
                                cx,
                            ),
                            render_raw_semantics_row(
                                RawSemanticsRow {
                                    selector: "body-raw-exact-bytes",
                                    value_selector: "body-raw-effective-body",
                                    mark: "✓",
                                    key: "Body bytes",
                                    value: body_preview,
                                    state: "EXACT",
                                    success: true,
                                },
                                cx,
                            ),
                            render_raw_semantics_row(
                                RawSemanticsRow {
                                    selector: "body-raw-ready-indicator",
                                    value_selector: "body-raw-request-target",
                                    mark: "✓",
                                    key: "Effective request",
                                    value: raw_request_target(method, effective_url),
                                    state: "READY",
                                    success: true,
                                },
                                cx,
                            ),
                        ]),
                )
                .on_prepaint({
                    let this = cx.weak_entity();
                    let scroll = scroll_handle.clone();
                    let previous = has_overflow;
                    move |_, window, cx| {
                        let has_overflow = scroll.max_offset().y > gpui::Pixels::ZERO;
                        if has_overflow != previous {
                            window.defer(cx, move |_, cx| {
                                let _ = this.update(cx, |this, cx| {
                                    if this.raw_semantics_have_overflow != has_overflow {
                                        this.raw_semantics_have_overflow = has_overflow;
                                        cx.notify();
                                    }
                                });
                            });
                        }
                    }
                })
                .when(has_overflow, |viewport| {
                    viewport.child(
                        div()
                            .debug_selector(|| "body-raw-scrollbar".into())
                            .absolute()
                            .top_0()
                            .right_0()
                            .bottom_0()
                            .w(Scrollbar::width())
                            .child(
                                Scrollbar::vertical(scroll_handle)
                                    .id("body-raw-scrollbar-control")
                                    .mode(ScrollbarMode::Always),
                            ),
                    )
                }),
        )
        .child(
            div()
                .debug_selector(|| "body-raw-semantics-footer".into())
                .h(px(28.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .px_3()
                .font_family(FONT_UI)
                .text_size(px(8.0))
                .text_color(SUBTEXT.resolve(cx))
                .child(if has_content_type {
                    "Effective Content-Type shown above · exact body bytes remain unchanged."
                } else {
                    "No Content-Type set · body bytes remain unchanged."
                })
                .child(
                    div()
                        .flex_none()
                        .font_family(FONT_MONO)
                        .text_color(TEXT.resolve(cx))
                        .child(format!("{byte_count} UTF-8 bytes")),
                ),
        )
        .into_any_element()
}

fn render_raw_semantics_row(row: RawSemanticsRow, cx: &gpui::App) -> gpui::AnyElement {
    let RawSemanticsRow {
        selector,
        value_selector,
        mark,
        key,
        value,
        state,
        success,
    } = row;

    div()
        .debug_selector(move || selector.into())
        .h(px(48.0))
        .flex_none()
        .flex()
        .items_center()
        .gap_3()
        .px_3()
        .border_b_1()
        .border_color(LINE.resolve(cx))
        .child(raw_semantics_mark(mark, success, cx))
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
                        .text_color(TEXT.resolve(cx))
                        .child(key),
                )
                .child(
                    div()
                        .debug_selector(move || value_selector.into())
                        .min_w_0()
                        .overflow_hidden()
                        .font_family(FONT_MONO)
                        .text_size(px(9.0))
                        .text_color(SUBTEXT.resolve(cx))
                        .child(value),
                ),
        )
        .child(raw_semantics_state(state, success, cx))
        .into_any_element()
}

fn raw_request_target(method: HttpMethod, effective_url: &str) -> String {
    let target = effective_url
        .split_once("://")
        .map(|(_, authority_and_path)| {
            authority_and_path
                .find('/')
                .map(|index| &authority_and_path[index..])
                .unwrap_or("/")
        })
        .unwrap_or_else(|| {
            if effective_url.is_empty() {
                "(URL not set)"
            } else {
                effective_url
            }
        });
    format!("{method} {target}")
}

fn raw_semantics_mark(label: &'static str, success: bool, cx: &gpui::App) -> gpui::AnyElement {
    div()
        .size(px(26.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_lg()
        .bg((if success { OK_SOFT } else { PANEL }).resolve(cx))
        .font_family(FONT_UI)
        .font_weight(FontWeight::BOLD)
        .text_size(px(11.0))
        .text_color((if success { OK } else { INFO }).resolve(cx))
        .child(label)
        .into_any_element()
}

fn raw_semantics_state(label: &'static str, success: bool, cx: &gpui::App) -> gpui::AnyElement {
    div()
        .h(px(22.0))
        .px_2()
        .flex_none()
        .flex()
        .items_center()
        .rounded_lg()
        .bg((if success { OK_SOFT } else { PANEL }).resolve(cx))
        .font_family(FONT_UI)
        .font_weight(FontWeight::BOLD)
        .text_size(px(8.0))
        .text_color((if success { OK } else { INFO }).resolve(cx))
        .child(label)
        .into_any_element()
}

use crate::{
    app::{EffectiveHeader, EffectiveHeaderSource},
    models::HttpMethod,
    ui::theme::{FONT_MONO, FONT_UI, INFO, INFO_SOFT, LINE, OK, OK_SOFT, PANEL, SUBTEXT, TEXT},
};
use gpui::{div, px, rgb, FontWeight, InteractiveElement, IntoElement, ParentElement, Styled};

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
        Some(header) => ("i", header.value.clone(), "USER ROW"),
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
        .w(px(360.0))
        .flex_none()
        .min_h_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded_lg()
        .border_1()
        .border_color(rgb(LINE))
        .bg(rgb(INFO_SOFT))
        .child(
            div()
                .h(px(46.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_between()
                .px_3()
                .border_b_1()
                .border_color(rgb(LINE))
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
                                .text_color(rgb(TEXT))
                                .child("Effective raw request"),
                        )
                        .child(
                            div()
                                .font_family(FONT_UI)
                                .text_size(px(9.0))
                                .text_color(rgb(SUBTEXT))
                                .child("Raw never synthesizes a Content-Type header."),
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
                        .bg(rgb(PANEL))
                        .font_family(FONT_UI)
                        .font_weight(FontWeight::BOLD)
                        .text_size(px(9.0))
                        .text_color(rgb(INFO))
                        .child(format!("{generated_count} GENERATED")),
                ),
        )
        .child(render_raw_semantics_row(RawSemanticsRow {
            selector: "body-raw-content-type-state",
            value_selector: "body-raw-content-type-value",
            mark: content_type_mark,
            key: "Content-Type",
            value: content_type_value,
            state: content_type_state,
            success: !has_content_type,
        }))
        .child(render_raw_semantics_row(RawSemanticsRow {
            selector: "body-raw-exact-bytes",
            value_selector: "body-raw-effective-body",
            mark: "✓",
            key: "Body bytes",
            value: body_preview,
            state: "EXACT",
            success: true,
        }))
        .child(render_raw_semantics_row(RawSemanticsRow {
            selector: "body-raw-ready-indicator",
            value_selector: "body-raw-request-target",
            mark: "✓",
            key: "Effective request",
            value: raw_request_target(method, effective_url),
            state: "READY",
            success: true,
        }))
        .child(
            div()
                .h(px(28.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .px_3()
                .font_family(FONT_UI)
                .text_size(px(8.0))
                .text_color(rgb(SUBTEXT))
                .child(if has_content_type {
                    "Manual Content-Type preserved · exact body bytes remain unchanged."
                } else {
                    "No Content-Type generated · exact body bytes will be sent."
                })
                .child(
                    div()
                        .flex_none()
                        .font_family(FONT_MONO)
                        .text_color(rgb(TEXT))
                        .child(format!("{byte_count} UTF-8 bytes")),
                ),
        )
        .into_any_element()
}

fn render_raw_semantics_row(row: RawSemanticsRow) -> gpui::AnyElement {
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
        .border_color(rgb(LINE))
        .child(raw_semantics_mark(mark, success))
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
                        .text_color(rgb(TEXT))
                        .child(key),
                )
                .child(
                    div()
                        .debug_selector(move || value_selector.into())
                        .min_w_0()
                        .overflow_hidden()
                        .font_family(FONT_MONO)
                        .text_size(px(9.0))
                        .text_color(rgb(SUBTEXT))
                        .child(value),
                ),
        )
        .child(raw_semantics_state(state, success))
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

fn raw_semantics_mark(label: &'static str, success: bool) -> gpui::AnyElement {
    div()
        .size(px(26.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_lg()
        .bg(rgb(if success { OK_SOFT } else { PANEL }))
        .font_family(FONT_UI)
        .font_weight(FontWeight::BOLD)
        .text_size(px(11.0))
        .text_color(rgb(if success { OK } else { INFO }))
        .child(label)
        .into_any_element()
}

fn raw_semantics_state(label: &'static str, success: bool) -> gpui::AnyElement {
    div()
        .h(px(22.0))
        .px_2()
        .flex_none()
        .flex()
        .items_center()
        .rounded_lg()
        .bg(rgb(if success { OK_SOFT } else { PANEL }))
        .font_family(FONT_UI)
        .font_weight(FontWeight::BOLD)
        .text_size(px(8.0))
        .text_color(rgb(if success { OK } else { INFO }))
        .child(label)
        .into_any_element()
}

use crate::{
    app::{
        BodyKind, EffectiveHeader, EffectiveHeaderSource, KeyValueRow, MultipartDraftPart,
        MultipartDraftValue, RequestBodyDraft, WorkspaceViewModel,
    },
    ui::{
        components::body_input::{BodyInput, BodyInputEvent, BodyType, FormDataEntry},
        theme::{
            ACCENT, ACCENT_INK, ACCENT_SOFT, ACCENT_VIVID, FONT_MONO, FONT_UI, INFO, INFO_SOFT,
            LINE, MUTED, OK, OK_SOFT, PANEL, PANEL_ALT, SUBTEXT, TEXT,
        },
    },
};
use gpui::{
    div, prelude::FluentBuilder, px, rgb, AppContext, Context, Entity, FontWeight,
    InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled,
    Subscription, Window,
};

/// BodyPane owns BodyInput's text/form editing state. Complete body drafts remain authoritative in
/// the shared WorkspaceViewModel and are projected only on request or pane changes.
pub(in crate::app::postman_app::request_workspace) struct BodyPane {
    view_model: Entity<WorkspaceViewModel>,
    body_input: Entity<BodyInput>,
    _subscriptions: Vec<Subscription>,
}

impl BodyPane {
    pub(in crate::app::postman_app::request_workspace) fn new(
        view_model: Entity<WorkspaceViewModel>,
        cx: &mut Context<Self>,
    ) -> Self {
        let body_input = cx.new(|cx| {
            BodyInput::new(cx)
                .with_placeholder("Enter request body (JSON, form data, etc.)")
                .with_type_tabs(false)
        });
        let subscriptions = vec![cx.subscribe(&body_input, Self::on_body_event)];
        let mut pane = Self {
            view_model,
            body_input,
            _subscriptions: subscriptions,
        };
        pane.project_active_request(cx);
        pane
    }

    fn update_view_model<R>(
        &self,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut WorkspaceViewModel) -> R,
    ) -> R {
        let result = self.view_model.update(cx, |view_model, cx| {
            let result = update(view_model);
            cx.notify();
            result
        });
        cx.notify();
        result
    }

    fn on_body_event(
        &mut self,
        _input: Entity<BodyInput>,
        event: &BodyInputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            BodyInputEvent::ValueChanged(value) => {
                self.update_view_model(cx, |view_model| view_model.set_body(value));
            }
            BodyInputEvent::FormDataChanged(entries) => {
                let entries = entries.clone();
                self.update_view_model(cx, |view_model| match view_model.body_kind() {
                    BodyKind::UrlEncoded => view_model.set_url_encoded_rows(
                        entries
                            .into_iter()
                            .map(|entry| KeyValueRow {
                                enabled: entry.enabled,
                                key: entry.key,
                                value: entry.value,
                            })
                            .collect(),
                    ),
                    BodyKind::Multipart => {
                        let parts = entries
                            .into_iter()
                            .map(|entry| {
                                let value = match entry.file {
                                    Some(file) => MultipartDraftValue::File {
                                        path: file.path,
                                        file_name: file.file_name,
                                        content_type: file.content_type,
                                    },
                                    None => MultipartDraftValue::Text(entry.value),
                                };
                                MultipartDraftPart {
                                    enabled: entry.enabled,
                                    name: entry.key,
                                    value,
                                }
                            })
                            .collect();
                        view_model.set_multipart_draft_parts(parts);
                    }
                    BodyKind::None | BodyKind::Json | BodyKind::Raw => {}
                });
            }
        }
    }

    fn set_body_kind(&mut self, kind: BodyKind, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| {
            let current = view_model.body_kind();
            let current_is_form = matches!(current, BodyKind::UrlEncoded | BodyKind::Multipart);
            let next_is_form = matches!(kind, BodyKind::UrlEncoded | BodyKind::Multipart);
            if current != kind && current_is_form != next_is_form {
                view_model.clear_body();
            }
            view_model.set_body_kind(kind);
        });
        self.project_active_request(cx);
    }

    fn use_sample_json(&mut self, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| {
            view_model.set_body_kind(BodyKind::Json);
            view_model.set_body(
                r#"{
  "name": "Ada Lovelace",
  "email": "ada@example.com",
  "active": true
}"#,
            );
        });
        self.project_active_request(cx);
    }

    fn clear_body(&mut self, cx: &mut Context<Self>) {
        self.update_view_model(cx, |view_model| view_model.clear_body());
        self.project_active_request(cx);
    }

    pub(in crate::app::postman_app::request_workspace) fn input_entity(&self) -> Entity<BodyInput> {
        self.body_input.clone()
    }

    pub(in crate::app::postman_app::request_workspace) fn project_active_request(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let (body_draft, body_kind) = {
            let view_model = self.view_model.read(cx);
            (view_model.body_draft().clone(), view_model.body_kind())
        };
        self.body_input.update(cx, |input, cx| {
            input.set_type_silent(body_type_from_kind(body_kind), cx);
            input.set_form_data_allows_files(body_kind == BodyKind::Multipart, cx);
            match body_draft {
                RequestBodyDraft::None => input.project_content("", cx),
                RequestBodyDraft::Json(body) | RequestBodyDraft::Raw(body) => {
                    input.project_content(body, cx)
                }
                RequestBodyDraft::UrlEncoded(rows) => {
                    let entries = rows
                        .into_iter()
                        .map(|row| FormDataEntry::text(row.key, row.value, row.enabled))
                        .collect();
                    input.project_form_data_entries(entries, cx);
                }
                RequestBodyDraft::Multipart(parts) => {
                    let entries = parts
                        .into_iter()
                        .map(|part| match part.value {
                            MultipartDraftValue::Text(value) => {
                                FormDataEntry::text(part.name, value, part.enabled)
                            }
                            MultipartDraftValue::File {
                                path,
                                file_name,
                                content_type,
                            } => FormDataEntry::file(
                                part.name,
                                path,
                                file_name,
                                content_type,
                                part.enabled,
                            ),
                        })
                        .collect();
                    input.project_form_data_entries(entries, cx);
                }
            }
        });
        cx.notify();
    }

    fn render_body_editor(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let (kind, body, effective_headers) = {
            let view_model = self.view_model.read(cx);
            (
                view_model.body_kind(),
                view_model.body().to_string(),
                view_model.effective_headers(),
            )
        };
        let is_json = kind == BodyKind::Json;
        let is_url_encoded = kind == BodyKind::UrlEncoded;
        let body_len = body.chars().count();
        let form_row_count = self.body_input.read(cx).form_data_entry_count();

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .child(
                div()
                    .debug_selector(|| "body-kind-selector".into())
                    .h(px(44.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .bg(rgb(PANEL))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .mr_1()
                            .font_family(FONT_UI)
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(9.0))
                            .text_color(rgb(SUBTEXT))
                            .child("BODY TYPE"),
                    )
                    .child(self.body_kind_option("none", BodyKind::None, kind, cx))
                    .child(self.body_kind_option("form-data", BodyKind::Multipart, kind, cx))
                    .child(self.body_kind_option(
                        "x-www-form-urlencoded",
                        BodyKind::UrlEncoded,
                        kind,
                        cx,
                    ))
                    .child(self.body_kind_option("raw", BodyKind::Raw, kind, cx))
                    .child(self.body_kind_option("JSON ✓", BodyKind::Json, kind, cx))
                    .when(is_json, |row| {
                        row.child(
                            div()
                                .debug_selector(|| "body-live-saved".into())
                                .h(px(24.0))
                                .px_2()
                                .flex()
                                .items_center()
                                .rounded_lg()
                                .bg(rgb(OK_SOFT))
                                .font_family(FONT_UI)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(px(9.0))
                                .text_color(rgb(OK))
                                .child("LIVE · SAVED"),
                        )
                    })
                    .when(is_url_encoded, |row| {
                        row.child(
                            div()
                                .debug_selector(|| "body-url-encoded-live-saved".into())
                                .h(px(24.0))
                                .px_2()
                                .flex()
                                .items_center()
                                .rounded_lg()
                                .bg(rgb(OK_SOFT))
                                .font_family(FONT_UI)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(px(9.0))
                                .text_color(rgb(OK))
                                .child("LIVE · SAVED"),
                        )
                        .child(
                            div()
                                .debug_selector(|| "body-url-encoded-row-count".into())
                                .h(px(24.0))
                                .px_2()
                                .flex()
                                .items_center()
                                .rounded_lg()
                                .bg(rgb(PANEL_ALT))
                                .font_family(FONT_UI)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(px(9.0))
                                .text_color(rgb(SUBTEXT))
                                .child(format!("{form_row_count} rows")),
                        )
                    }),
            )
            .child(if is_url_encoded {
                self.render_url_encoded_body(body, effective_headers)
            } else {
                self.render_text_body(body_len, is_json, effective_headers, cx)
            })
            .into_any_element()
    }

    fn render_text_body(
        &self,
        body_len: usize,
        is_json: bool,
        effective_headers: Vec<EffectiveHeader>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .gap_3()
            .p_3()
            .bg(rgb(PANEL_ALT))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .debug_selector(|| "body-editor-shell".into())
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(if is_json { INFO } else { LINE }))
                            .bg(rgb(PANEL))
                            .child(
                                div()
                                    .h(px(32.0))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .px_3()
                                    .border_b_1()
                                    .border_color(rgb(LINE))
                                    .font_family(FONT_UI)
                                    .child(
                                        div()
                                            .debug_selector(|| "body-editor-title".into())
                                            .font_weight(FontWeight::BOLD)
                                            .text_size(px(9.0))
                                            .text_color(rgb(INFO))
                                            .child(if is_json {
                                                "JSON · ACTIVE INPUT"
                                            } else {
                                                "BODY · ACTIVE INPUT"
                                            }),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .text_size(px(9.0))
                                            .text_color(rgb(MUTED))
                                            .child(format!("{body_len} chars"))
                                            .child(
                                                div()
                                                    .debug_selector(|| "body-sample-json".into())
                                                    .px_2()
                                                    .py_1()
                                                    .rounded_md()
                                                    .bg(rgb(INFO_SOFT))
                                                    .text_color(rgb(INFO))
                                                    .cursor_pointer()
                                                    .child("Sample JSON")
                                                    .on_mouse_up(
                                                        gpui::MouseButton::Left,
                                                        cx.listener(|this, _, _, cx| {
                                                            this.use_sample_json(cx)
                                                        }),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .debug_selector(|| "body-clear-button".into())
                                                    .px_2()
                                                    .py_1()
                                                    .rounded_md()
                                                    .bg(rgb(PANEL_ALT))
                                                    .text_color(rgb(SUBTEXT))
                                                    .cursor_pointer()
                                                    .child("Clear")
                                                    .on_mouse_up(
                                                        gpui::MouseButton::Left,
                                                        cx.listener(|this, _, _, cx| {
                                                            this.clear_body(cx)
                                                        }),
                                                    ),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "body-input".into())
                                    .flex_1()
                                    .min_h_0()
                                    .child(self.body_input.clone()),
                            ),
                    )
                    .child(
                        div()
                            .debug_selector(|| "body-source-of-truth".into())
                            .h(px(46.0))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .justify_center()
                            .gap_1()
                            .px_3()
                            .rounded_lg()
                            .bg(rgb(OK_SOFT))
                            .font_family(FONT_UI)
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(9.0))
                                    .text_color(rgb(OK))
                                    .child("SINGLE SOURCE OF TRUTH"),
                            )
                            .child(
                                div()
                                    .font_family(FONT_MONO)
                                    .text_size(px(9.0))
                                    .text_color(rgb(TEXT))
                                    .child(
                                        "The active value already lives in the ViewModel draft; Send performs no backfill",
                                    ),
                            ),
                    ),
            )
            .when(is_json, |content| {
                content.child(self.render_effective_headers(effective_headers))
            })
            .into_any_element()
    }

    fn render_url_encoded_body(
        &self,
        body: String,
        effective_headers: Vec<EffectiveHeader>,
    ) -> gpui::AnyElement {
        let field_count = form_urlencoded::parse(body.as_bytes()).count();
        let request_headers = effective_headers
            .into_iter()
            .filter(|header| {
                header.name.eq_ignore_ascii_case("content-type")
                    || header.name.eq_ignore_ascii_case("accept")
            })
            .collect::<Vec<_>>();

        div()
            .debug_selector(|| "body-url-encoded-editor".into())
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .child(
                div()
                    .debug_selector(|| "body-input".into())
                    .flex_1()
                    .min_h_0()
                    .child(self.body_input.clone()),
            )
            .child(
                div()
                    .debug_selector(|| "body-url-encoded-effective-request".into())
                    .h(px(68.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .bg(rgb(INFO_SOFT))
                    .border_t_1()
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
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .font_family(FONT_UI)
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(9.0))
                                    .text_color(rgb(INFO))
                                    .child("↗ EFFECTIVE REQUEST BODY")
                                    .child(
                                        div()
                                            .debug_selector(|| {
                                                "body-url-encoded-field-count".into()
                                            })
                                            .px_2()
                                            .py_1()
                                            .rounded_lg()
                                            .bg(rgb(PANEL))
                                            .text_color(rgb(SUBTEXT))
                                            .child(format!("{field_count} fields")),
                                    ),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "body-url-encoded-effective-body".into())
                                    .min_w_0()
                                    .overflow_hidden()
                                    .font_family(FONT_MONO)
                                    .text_size(px(10.0))
                                    .text_color(rgb(TEXT))
                                    .child(if body.is_empty() {
                                        "(empty body)".to_string()
                                    } else {
                                        body
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .debug_selector(|| "body-url-encoded-effective-headers".into())
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap_2()
                            .children(
                                request_headers
                                    .into_iter()
                                    .map(render_url_encoded_header_chip),
                            ),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "body-url-encoded-ready-indicator".into())
                    .h(px(28.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .font_family(FONT_UI)
                    .text_size(px(9.0))
                    .text_color(rgb(OK))
                    .child("✓")
                    .child(
                        "Ready to send — active values are already saved in the ViewModel draft",
                    ),
            )
            .into_any_element()
    }

    fn render_effective_headers(&self, headers: Vec<EffectiveHeader>) -> gpui::AnyElement {
        let count = headers.len();
        div()
            .debug_selector(|| "body-effective-headers".into())
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
                                    .child("Effective request headers"),
                            )
                            .child(
                                div()
                                    .font_family(FONT_UI)
                                    .text_size(px(9.0))
                                    .text_color(rgb(SUBTEXT))
                                    .child("Generated defaults and user rows merge once."),
                            ),
                    )
                    .child(
                        div()
                            .debug_selector(|| "body-effective-header-count".into())
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
                            .child(format!("{count} SENT")),
                    ),
            )
            .child(
                div()
                    .id("body-effective-headers-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .px_2()
                    .pb_2()
                    .when(count == 0, |list| {
                        list.child(
                            div()
                                .flex_1()
                                .flex()
                                .items_center()
                                .justify_center()
                                .font_family(FONT_UI)
                                .text_size(px(10.0))
                                .text_color(rgb(MUTED))
                                .child("No enabled request headers"),
                        )
                    })
                    .children(headers.into_iter().map(render_effective_header)),
            )
            .child(
                div()
                    .h(px(28.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .px_3()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .font_family(FONT_UI)
                    .text_size(px(8.0))
                    .text_color(rgb(SUBTEXT))
                    .child("This projection is read from the same typed Request used by Send."),
            )
            .into_any_element()
    }

    fn body_kind_option(
        &self,
        label: &'static str,
        option: BodyKind,
        selected: BodyKind,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = option == selected;
        let debug_selector = match option {
            BodyKind::None => "body-kind-none",
            BodyKind::Multipart => "body-kind-form-data",
            BodyKind::UrlEncoded => "body-kind-url-encoded",
            BodyKind::Raw => "body-kind-raw",
            BodyKind::Json => "body-kind-json",
        };
        let element = div()
            .debug_selector(move || debug_selector.into())
            .h(px(28.0))
            .px_2()
            .flex()
            .items_center()
            .gap_1()
            .rounded_lg()
            .border_1()
            .border_color(rgb(if active { ACCENT } else { LINE }))
            .bg(rgb(if active { ACCENT_SOFT } else { PANEL }))
            .font_family(FONT_UI)
            .text_size(px(12.0))
            .font_weight(if active {
                FontWeight::BOLD
            } else {
                FontWeight::NORMAL
            })
            .text_color(rgb(if active { ACCENT_INK } else { SUBTEXT }))
            .child(
                div()
                    .text_color(rgb(if active { ACCENT_VIVID } else { MUTED }))
                    .child(if active { "●" } else { "○" }),
            )
            .child(label);
        element.cursor_pointer().on_mouse_up(
            gpui::MouseButton::Left,
            cx.listener(move |this, _, _, cx| this.set_body_kind(option, cx)),
        )
    }
}

impl Render for BodyPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_body_editor(cx)
    }
}

fn body_type_from_kind(kind: BodyKind) -> BodyType {
    match kind {
        BodyKind::Json => BodyType::Json,
        BodyKind::UrlEncoded | BodyKind::Multipart => BodyType::FormData,
        BodyKind::None | BodyKind::Raw => BodyType::Raw,
    }
}

fn render_effective_header(header: EffectiveHeader) -> gpui::AnyElement {
    let selector = body_effective_header_selector(&header.name);
    let generated = header.source == EffectiveHeaderSource::Generated;
    div()
        .debug_selector(move || selector.clone())
        .h(px(48.0))
        .flex_none()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .rounded_lg()
        .border_1()
        .border_color(rgb(LINE))
        .bg(rgb(PANEL))
        .child(
            div()
                .size(px(20.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .bg(rgb(OK_SOFT))
                .font_family(FONT_UI)
                .font_weight(FontWeight::BOLD)
                .text_size(px(10.0))
                .text_color(rgb(OK))
                .child("✓"),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .items_center()
                .gap_2()
                .font_family(FONT_MONO)
                .text_size(px(10.0))
                .child(
                    div()
                        .flex_none()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(TEXT))
                        .child(header.name),
                )
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_color(rgb(SUBTEXT))
                        .child(header.value),
                ),
        )
        .child(
            div()
                .h(px(22.0))
                .px_2()
                .flex_none()
                .flex()
                .items_center()
                .rounded_lg()
                .bg(rgb(if generated { INFO_SOFT } else { ACCENT_SOFT }))
                .font_family(FONT_UI)
                .font_weight(FontWeight::BOLD)
                .text_size(px(8.0))
                .text_color(rgb(if generated { INFO } else { ACCENT_INK }))
                .child(if generated { "GENERATED" } else { "USER ROW" }),
        )
        .into_any_element()
}

fn render_url_encoded_header_chip(header: EffectiveHeader) -> gpui::AnyElement {
    let selector = body_effective_header_selector(&header.name);
    let generated = header.source == EffectiveHeaderSource::Generated;
    div()
        .debug_selector(move || selector.clone())
        .h(px(28.0))
        .flex_none()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .rounded_lg()
        .border_1()
        .border_color(rgb(LINE))
        .bg(rgb(PANEL))
        .font_family(FONT_UI)
        .text_size(px(8.0))
        .text_color(rgb(SUBTEXT))
        .child(
            div()
                .font_family(FONT_MONO)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT))
                .child(format!("{}: {}", header.name, header.value)),
        )
        .child(
            div()
                .px_1()
                .py_1()
                .rounded_md()
                .bg(rgb(if generated { INFO_SOFT } else { ACCENT_SOFT }))
                .text_color(rgb(if generated { INFO } else { ACCENT_INK }))
                .child(if generated { "GENERATED" } else { "USER" }),
        )
        .into_any_element()
}

fn body_effective_header_selector(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    format!("body-effective-header-{slug}")
}

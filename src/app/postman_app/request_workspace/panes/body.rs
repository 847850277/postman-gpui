mod raw;

use super::super::layout::RequestPanelLayout;

use crate::{
    app::{
        BodyKind, EffectiveHeader, EffectiveHeaderSource, KeyValueRow, MultipartDraftPart,
        MultipartDraftValue, RequestBodyDraft, RequestTabId, RequestViewModel, ResponseState,
        WorkspaceViewModel,
    },
    models::{HttpMethod, MultipartPart, MultipartValue, RequestBody},
    ui::{
        components::{
            common::scrollbar::{scrollbar_geometry, vertical_scrollbar, ScrollbarGeometry},
            input::body_input::{BodyInput, BodyInputEvent, BodyType, FormDataEntry},
        },
        theme::{
            ACCENT, ACCENT_INK, ACCENT_SOFT, FONT_MONO, FONT_UI, INFO, INFO_SOFT, LINE, MUTED, OK,
            OK_SOFT, PANEL, PANEL_ALT, SUBTEXT, TEXT,
        },
    },
};
use gpui::{
    actions, div, prelude::FluentBuilder, px, AppContext, Context, Entity, FocusHandle, FontWeight,
    InteractiveElement, IntoElement, KeyBinding, ParentElement, Render, ScrollHandle,
    StatefulInteractiveElement, Styled, Subscription, Window,
};
use raw::render_raw_request_semantics;

actions!(body_kind, [NextBodyKind, PreviousBodyKind]);

fn setup_body_kind_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("right", NextBodyKind, Some("BodyKind")),
        KeyBinding::new("down", NextBodyKind, Some("BodyKind")),
        KeyBinding::new("left", PreviousBodyKind, Some("BodyKind")),
        KeyBinding::new("up", PreviousBodyKind, Some("BodyKind")),
    ]
}

/// BodyPane owns BodyInput's text/form editing state. Complete body drafts remain authoritative in
/// the shared WorkspaceViewModel and are projected only on request or pane changes.
pub(in crate::app::postman_app::request_workspace) struct BodyPane {
    view_model: Entity<WorkspaceViewModel>,
    panel_layout: Entity<RequestPanelLayout>,
    body_input: Entity<BodyInput>,
    projected_tab_id: Option<RequestTabId>,
    effective_headers_scroll: ScrollHandle,
    raw_semantics_scroll: ScrollHandle,
    kind_focus_handles: Vec<FocusHandle>,
    sample_focus_handle: FocusHandle,
    clear_focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl BodyPane {
    pub(in crate::app::postman_app::request_workspace) fn new(
        view_model: Entity<WorkspaceViewModel>,
        panel_layout: Entity<RequestPanelLayout>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.bind_keys(setup_body_kind_key_bindings());
        let body_input = cx.new(|cx| {
            BodyInput::new(cx)
                .with_placeholder("Enter request body (JSON, form data, etc.)")
                .with_type_tabs(false)
        });
        let subscriptions = vec![cx.subscribe(&body_input, Self::on_body_event)];
        let mut pane = Self {
            view_model,
            panel_layout,
            body_input,
            projected_tab_id: None,
            effective_headers_scroll: ScrollHandle::new(),
            raw_semantics_scroll: ScrollHandle::new(),
            kind_focus_handles: (0..5)
                .map(|_| cx.focus_handle().tab_index(0).tab_stop(true))
                .collect(),
            sample_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            clear_focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            _subscriptions: subscriptions,
        };
        pane.project_active_request(cx);
        pane
    }

    fn update_active_request<R>(
        &self,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut RequestViewModel) -> R,
    ) -> Option<R> {
        let result = self.view_model.update(cx, |view_model, cx| {
            let result = view_model.update_active_request(update);
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
                self.update_active_request(cx, |request| request.set_body(value));
            }
            BodyInputEvent::FormDataChanged(entries) => {
                let entries = entries.clone();
                self.update_active_request(cx, |request| match request.body_kind() {
                    BodyKind::UrlEncoded => request.set_url_encoded_rows(
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
                        request.set_multipart_draft_parts(parts);
                    }
                    BodyKind::None | BodyKind::Json | BodyKind::Raw => {}
                });
            }
        }
    }

    fn set_body_kind(&mut self, kind: BodyKind, cx: &mut Context<Self>) {
        self.update_active_request(cx, |request| {
            let current = request.body_kind();
            let current_is_form = matches!(current, BodyKind::UrlEncoded | BodyKind::Multipart);
            let next_is_form = matches!(kind, BodyKind::UrlEncoded | BodyKind::Multipart);
            if current != kind && current_is_form != next_is_form {
                request.clear_body();
            }
            request.set_body_kind(kind);
        });
        self.project_active_request(cx);
    }

    fn use_sample_json(&mut self, cx: &mut Context<Self>) {
        self.update_active_request(cx, |request| {
            request.set_body_kind(BodyKind::Json);
            request.set_body(
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
        self.update_active_request(cx, RequestViewModel::clear_body);
        self.project_active_request(cx);
    }

    pub(in crate::app::postman_app::request_workspace) fn input_entity(&self) -> Entity<BodyInput> {
        self.body_input.clone()
    }

    pub(in crate::app::postman_app::request_workspace) fn project_active_request(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let (tab_id, body_draft, body_kind) = {
            let view_model = self.view_model.read(cx);
            view_model.active_request().map_or(
                (None, RequestBodyDraft::None, BodyKind::None),
                |request| {
                    (
                        Some(request.tab_id()),
                        request.body_draft().clone(),
                        request.body_kind(),
                    )
                },
            )
        };
        let tab_changed = self.projected_tab_id != tab_id;
        self.body_input.update(cx, |input, cx| {
            input.set_type_silent(body_type_from_kind(body_kind), cx);
            input.set_form_data_allows_files(body_kind == BodyKind::Multipart, cx);
            match body_draft {
                RequestBodyDraft::None => {
                    if tab_changed {
                        input.project_form_data_entries_with_rebind(Vec::new(), cx);
                    }
                    input.project_content("", cx);
                }
                RequestBodyDraft::Json(body) | RequestBodyDraft::Raw(body) => {
                    if tab_changed {
                        input.project_form_data_entries_with_rebind(Vec::new(), cx);
                    }
                    input.project_content(body, cx)
                }
                RequestBodyDraft::UrlEncoded(rows) => {
                    let entries = rows
                        .into_iter()
                        .map(|row| FormDataEntry::text(row.key, row.value, row.enabled))
                        .collect();
                    if tab_changed {
                        input.project_form_data_entries_with_rebind(entries, cx);
                    } else {
                        input.project_form_data_entries(entries, cx);
                    }
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
                    if tab_changed {
                        input.project_form_data_entries_with_rebind(entries, cx);
                    } else {
                        input.project_form_data_entries(entries, cx);
                    }
                }
            }
        });
        self.projected_tab_id = tab_id;
        cx.notify();
    }

    fn render_body_editor(&self, window: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let (
            kind,
            body,
            request_body,
            method,
            effective_url,
            effective_headers,
            multipart_omitted,
            multipart_error,
        ) = {
            let view_model = self.view_model.read(cx);
            let Some(request) = view_model.active_request() else {
                return div().into_any_element();
            };
            let multipart_omitted = match request.body_draft() {
                RequestBodyDraft::Multipart(parts) => {
                    parts.iter().filter(|part| !part.enabled).count()
                }
                _ => 0,
            };
            let multipart_error = match request.response() {
                ResponseState::Error { message }
                    if message.contains("failed to read multipart file") =>
                {
                    Some(message.clone())
                }
                _ => None,
            };
            let construction = request.request_construction();
            (
                request.body_kind(),
                request.body().to_string(),
                construction.request().body.clone(),
                construction.request().method,
                construction.request().url.clone(),
                construction.effective_headers().to_vec(),
                multipart_omitted,
                multipart_error,
            )
        };
        let is_json = kind == BodyKind::Json;
        let is_raw = kind == BodyKind::Raw;
        let is_url_encoded = kind == BodyKind::UrlEncoded;
        let is_multipart = kind == BodyKind::Multipart;
        let form_row_count = self.body_input.read(cx).form_data_entry_count(cx);
        let panel_height = self.panel_layout.read(cx).height();

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(PANEL.resolve(cx))
            .child(
                div()
                    .debug_selector(|| "body-kind-selector".into())
                    .h(px(44.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .bg(PANEL.resolve(cx))
                    .border_b_1()
                    .border_color(LINE.resolve(cx))
                    .child(
                        div()
                            .mr_1()
                            .font_family(FONT_UI)
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(9.0))
                            .text_color(SUBTEXT.resolve(cx))
                            .child("Body type"),
                    )
                    .child(self.body_kind_option("none", BodyKind::None, kind, window, cx))
                    .child(self.body_kind_option(
                        "form-data",
                        BodyKind::Multipart,
                        kind,
                        window,
                        cx,
                    ))
                    .child(self.body_kind_option(
                        "x-www-form-urlencoded",
                        BodyKind::UrlEncoded,
                        kind,
                        window,
                        cx,
                    ))
                    .child(self.body_kind_option("raw", BodyKind::Raw, kind, window, cx))
                    .child(self.body_kind_option("JSON", BodyKind::Json, kind, window, cx))
                    .when(is_json, |row| {
                        row.child(
                            div()
                                .debug_selector(|| "body-live-saved".into())
                                .h(px(24.0))
                                .px_2()
                                .flex()
                                .items_center()
                                .rounded_lg()
                                .bg(OK_SOFT.resolve(cx))
                                .font_family(FONT_UI)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(px(9.0))
                                .text_color(OK.resolve(cx))
                                .child("Edited"),
                        )
                    })
                    .when(is_raw, |row| {
                        row.child(
                            div()
                                .debug_selector(|| "body-raw-live-saved".into())
                                .h(px(24.0))
                                .px_2()
                                .flex()
                                .items_center()
                                .rounded_lg()
                                .bg(OK_SOFT.resolve(cx))
                                .font_family(FONT_UI)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(px(9.0))
                                .text_color(OK.resolve(cx))
                                .child("Edited"),
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
                                .bg(OK_SOFT.resolve(cx))
                                .font_family(FONT_UI)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(px(9.0))
                                .text_color(OK.resolve(cx))
                                .child("Edited"),
                        )
                        .child(
                            div()
                                .debug_selector(|| "body-url-encoded-row-count".into())
                                .h(px(24.0))
                                .px_2()
                                .flex()
                                .items_center()
                                .rounded_lg()
                                .bg(PANEL_ALT.resolve(cx))
                                .font_family(FONT_UI)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(px(9.0))
                                .text_color(SUBTEXT.resolve(cx))
                                .child(format!("{form_row_count} rows")),
                        )
                    })
                    .when(is_multipart, |row| {
                        row.child(
                            div()
                                .debug_selector(|| "body-multipart-live-saved".into())
                                .h(px(24.0))
                                .px_2()
                                .flex()
                                .items_center()
                                .rounded_lg()
                                .bg(OK_SOFT.resolve(cx))
                                .font_family(FONT_UI)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(px(9.0))
                                .text_color(OK.resolve(cx))
                                .child("Edited"),
                        )
                        .child(
                            div()
                                .debug_selector(|| "body-multipart-row-count".into())
                                .h(px(24.0))
                                .px_2()
                                .flex()
                                .items_center()
                                .rounded_lg()
                                .bg(PANEL_ALT.resolve(cx))
                                .font_family(FONT_UI)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(px(9.0))
                                .text_color(SUBTEXT.resolve(cx))
                                .child(format!("{form_row_count} rows")),
                        )
                    }),
            )
            .child(if is_url_encoded {
                self.render_url_encoded_body(body, effective_headers, cx)
            } else if is_multipart {
                self.render_multipart_body(request_body, multipart_omitted, multipart_error, cx)
            } else {
                self.render_text_body(
                    body,
                    kind,
                    (method, effective_url, effective_headers),
                    panel_height,
                    window,
                    cx,
                )
            })
            .into_any_element()
    }

    fn render_text_body(
        &self,
        body: String,
        kind: BodyKind,
        request_projection: (HttpMethod, String, Vec<EffectiveHeader>),
        panel_height: f32,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        use crate::ui::{components::kit_controls, theme::metrics as m};
        let (method, effective_url, effective_headers) = request_projection;
        let is_json = kind == BodyKind::Json;
        let is_raw = kind == BodyKind::Raw;
        let side_height = (panel_height - 128.).max(0.);
        let side_panel = if is_json {
            Some(self.render_effective_headers(effective_headers, side_height, cx))
        } else if is_raw {
            Some(render_raw_request_semantics(
                &body,
                method,
                &effective_url,
                effective_headers,
                &self.raw_semantics_scroll,
                side_height,
                cx,
            ))
        } else {
            None
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .gap_4()
            .px_7()
            .pb_3()
            .child(
                div()
                    .debug_selector(|| "body-editor-shell".into())
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h_10()
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .debug_selector(|| "body-editor-title".into())
                                    .text_size(m::LABEL)
                                    .text_color(TEXT.resolve(cx))
                                    .child(if is_json {
                                        "JSON"
                                    } else if is_raw {
                                        "Raw body"
                                    } else {
                                        "Request body"
                                    }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .text_size(m::CAPTION)
                                    .text_color(MUTED.resolve(cx))
                                    .child(format!("{} chars", body.chars().count()))
                                    .when(!is_raw, |a| {
                                        a.child(
                                            kit_controls::editor_button(
                                                "body-sample-json",
                                                "Sample JSON",
                                                cx,
                                            )
                                            .debug_selector(|| "body-sample-json".into())
                                            .track_focus(&self.sample_focus_handle)
                                            .on_click(
                                                cx.listener(|this, _, _, cx| {
                                                    this.use_sample_json(cx)
                                                }),
                                            ),
                                        )
                                    })
                                    .child(
                                        kit_controls::editor_button(
                                            "body-clear-button",
                                            "Clear",
                                            cx,
                                        )
                                        .debug_selector(|| "body-clear-button".into())
                                        .track_focus(&self.clear_focus_handle)
                                        .on_click(
                                            cx.listener(|this, _, _, cx| this.clear_body(cx)),
                                        ),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .debug_selector(|| "body-input".into())
                            .flex_1()
                            .min_h_0()
                            .border_1()
                            .border_color(LINE.resolve(cx))
                            .rounded(m::RADIUS)
                            .overflow_hidden()
                            .child(self.body_input.clone()),
                    ),
            )
            .when_some(side_panel, |p, side| p.child(side))
            .into_any_element()
    }

    fn render_url_encoded_body(
        &self,
        body: String,
        effective_headers: Vec<EffectiveHeader>,
        cx: &gpui::App,
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
            .bg(PANEL.resolve(cx))
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
                    .bg(INFO_SOFT.resolve(cx))
                    .border_t_1()
                    .border_color(LINE.resolve(cx))
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
                                    .text_color(INFO.resolve(cx))
                                    .child("↗ EFFECTIVE REQUEST BODY")
                                    .child(
                                        div()
                                            .debug_selector(|| {
                                                "body-url-encoded-field-count".into()
                                            })
                                            .px_2()
                                            .py_1()
                                            .rounded_lg()
                                            .bg(PANEL.resolve(cx))
                                            .text_color(SUBTEXT.resolve(cx))
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
                                    .text_color(TEXT.resolve(cx))
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
                                    .map(|item| render_url_encoded_header_chip(item, cx)),
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
                    .border_color(LINE.resolve(cx))
                    .font_family(FONT_UI)
                    .text_size(px(9.0))
                    .text_color(OK.resolve(cx))
                    .child("✓")
                    .child("Enabled fields are encoded and included when you send."),
            )
            .into_any_element()
    }

    fn render_multipart_body(
        &self,
        request_body: RequestBody,
        omitted_count: usize,
        file_error: Option<String>,
        cx: &gpui::App,
    ) -> gpui::AnyElement {
        let parts = match request_body {
            RequestBody::Multipart(parts) => parts,
            _ => Vec::new(),
        };
        let part_count = parts.len();
        let parts_preview = multipart_parts_preview(&parts);
        let has_file_error = file_error.is_some();

        div()
            .debug_selector(|| "body-multipart-editor".into())
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(PANEL.resolve(cx))
            .child(
                div()
                    .debug_selector(|| "body-input".into())
                    .flex_1()
                    .min_h_0()
                    .child(self.body_input.clone()),
            )
            .when_some(file_error, |editor, message| {
                editor.child(
                    div()
                        .debug_selector(|| "body-multipart-file-error".into())
                        .h(px(38.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .bg(ACCENT_SOFT.resolve(cx))
                        .border_t_1()
                        .border_color(ACCENT.resolve(cx))
                        .font_family(FONT_UI)
                        .text_size(px(9.0))
                        .text_color(ACCENT_INK.resolve(cx))
                        .child("!")
                        .child(
                            div()
                                .debug_selector(|| "body-multipart-file-error-message".into())
                                .min_w_0()
                                .overflow_hidden()
                                .child(message),
                        ),
                )
            })
            .child(
                div()
                    .debug_selector(|| "body-multipart-effective-request".into())
                    .h(px(72.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .bg(INFO_SOFT.resolve(cx))
                    .border_t_1()
                    .border_color(LINE.resolve(cx))
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
                                    .text_color(INFO.resolve(cx))
                                    .child("↗ EFFECTIVE MULTIPART PARTS")
                                    .child(
                                        div()
                                            .debug_selector(|| "body-multipart-part-count".into())
                                            .px_2()
                                            .py_1()
                                            .rounded_lg()
                                            .bg(PANEL.resolve(cx))
                                            .text_color(SUBTEXT.resolve(cx))
                                            .child(format!("{part_count} parts")),
                                    )
                                    .child(
                                        div()
                                            .debug_selector(|| {
                                                "body-multipart-omitted-count".into()
                                            })
                                            .px_2()
                                            .py_1()
                                            .rounded_lg()
                                            .bg(PANEL.resolve(cx))
                                            .text_color(
                                                (if omitted_count > 0 { ACCENT } else { SUBTEXT })
                                                    .resolve(cx),
                                            )
                                            .child(format!("{omitted_count} disabled omitted")),
                                    ),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "body-multipart-effective-parts".into())
                                    .min_w_0()
                                    .overflow_hidden()
                                    .font_family(FONT_MONO)
                                    .text_size(px(10.0))
                                    .text_color(TEXT.resolve(cx))
                                    .child(parts_preview),
                            ),
                    )
                    .child(
                        div()
                            .debug_selector(|| "body-multipart-boundary".into())
                            .flex_none()
                            .px_2()
                            .py_1()
                            .rounded_lg()
                            .bg(PANEL.resolve(cx))
                            .font_family(FONT_MONO)
                            .text_size(px(9.0))
                            .text_color(SUBTEXT.resolve(cx))
                            .child("multipart/form-data; boundary=<generated>"),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "body-multipart-ready-indicator".into())
                    .h(px(28.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .border_t_1()
                    .border_color(LINE.resolve(cx))
                    .font_family(FONT_UI)
                    .text_size(px(9.0))
                    .text_color((if has_file_error { ACCENT } else { OK }).resolve(cx))
                    .child(if has_file_error { "!" } else { "✓" })
                    .child(if has_file_error {
                        "Selected file needs correction — no successful History entry was added"
                    } else {
                        "Ready to send — boundary generation remains transport-owned"
                    }),
            )
            .into_any_element()
    }

    fn render_effective_headers(
        &self,
        headers: Vec<EffectiveHeader>,
        viewport_height: f32,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let count = headers.len();
        let scrollbar = effective_headers_scrollbar_geometry(
            count,
            viewport_height,
            &self.effective_headers_scroll,
        );
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
                                    .child("Effective request headers"),
                            )
                            .child(
                                div()
                                    .font_family(FONT_UI)
                                    .text_size(px(9.0))
                                    .text_color(SUBTEXT.resolve(cx))
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
                            .bg(PANEL.resolve(cx))
                            .font_family(FONT_UI)
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(9.0))
                            .text_color(INFO.resolve(cx))
                            .child(format!("{count} SENT")),
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
                            .id("body-effective-headers-scroll")
                            .debug_selector(|| "body-effective-headers-scroll".into())
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.effective_headers_scroll)
                            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
                            .flex()
                            .flex_col()
                            .gap_2()
                            .px_2()
                            .pb_2()
                            .when(scrollbar.is_some(), |list| list.pr(px(20.0)))
                            .when(count == 0, |list| {
                                list.child(
                                    div()
                                        .flex_1()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .font_family(FONT_UI)
                                        .text_size(px(10.0))
                                        .text_color(MUTED.resolve(cx))
                                        .child("No enabled request headers"),
                                )
                            })
                            .children(
                                headers
                                    .into_iter()
                                    .map(|item| render_effective_header(item, cx)),
                            ),
                    )
                    .when_some(scrollbar, |viewport, scrollbar| {
                        viewport.child(vertical_scrollbar(
                            "body-effective-headers-scrollbar",
                            "body-effective-headers-scrollbar-thumb",
                            scrollbar,
                            cx,
                        ))
                    }),
            )
            .child(
                div()
                    .h(px(28.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .px_3()
                    .border_t_1()
                    .border_color(LINE.resolve(cx))
                    .font_family(FONT_UI)
                    .text_size(px(8.0))
                    .text_color(SUBTEXT.resolve(cx))
                    .child("Headers applied when sending this request."),
            )
            .into_any_element()
    }

    fn body_kind_option(
        &self,
        label: &'static str,
        option: BodyKind,
        selected: BodyKind,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = option == selected;
        let index = body_kind_index(option);
        let focus_handle = self.kind_focus_handles[index].clone();
        let debug_selector = match option {
            BodyKind::None => "body-kind-none",
            BodyKind::Multipart => "body-kind-form-data",
            BodyKind::UrlEncoded => "body-kind-url-encoded",
            BodyKind::Raw => "body-kind-raw",
            BodyKind::Json => "body-kind-json",
        };
        let on_select =
            cx.listener(move |this, _: &gpui::ClickEvent, _, cx| this.set_body_kind(option, cx));
        gpui_kit::base::Radio::new(debug_selector)
            .debug_selector(move || debug_selector.into())
            .checked(active)
            .accessibility_label(label)
            .set_position(index + 1, BODY_KINDS.len())
            .track_focus(&focus_handle)
            .key_context("BodyKind")
            .h(gpui::rems(2.))
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .rounded(crate::ui::theme::metrics::RADIUS)
            .border_1()
            .border_color((if active { ACCENT } else { LINE }).resolve(cx))
            .bg((if active { ACCENT_SOFT } else { PANEL }).resolve(cx))
            .text_size(crate::ui::theme::metrics::LABEL)
            .text_color((if active { ACCENT } else { SUBTEXT }).resolve(cx))
            .focus_visible(|s| s.border_color(ACCENT.resolve(cx)))
            .child(label)
            .on_change(move |_, event, window, cx| on_select(event, window, cx))
            .on_action(cx.listener(move |this, _: &NextBodyKind, window, cx| {
                this.select_relative_body_kind(option, 1, window, cx)
            }))
            .on_action(cx.listener(move |this, _: &PreviousBodyKind, window, cx| {
                this.select_relative_body_kind(option, -1, window, cx)
            }))
    }

    fn select_relative_body_kind(
        &mut self,
        kind: BodyKind,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = (body_kind_index(kind) as isize + delta).rem_euclid(5) as usize;
        let kind = BODY_KINDS[next];
        self.kind_focus_handles[next].focus(window, cx);
        self.set_body_kind(kind, cx);
    }
}

impl Render for BodyPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_body_editor(window, cx)
    }
}

const EFFECTIVE_HEADER_FALLBACK_VISIBLE_ROWS: usize = 3;
const EFFECTIVE_HEADER_ROW_HEIGHT: f32 = 48.0;
const EFFECTIVE_HEADER_ROW_GAP: f32 = 8.0;
const EFFECTIVE_HEADER_LIST_BOTTOM_PADDING: f32 = 8.0;

fn effective_headers_scrollbar_geometry(
    header_count: usize,
    viewport_height: f32,
    scroll_handle: &ScrollHandle,
) -> Option<ScrollbarGeometry> {
    if header_count == 0 {
        return None;
    }

    let content_height = EFFECTIVE_HEADER_ROW_HEIGHT * header_count as f32
        + EFFECTIVE_HEADER_ROW_GAP * header_count.saturating_sub(1) as f32
        + EFFECTIVE_HEADER_LIST_BOTTOM_PADDING;
    let max_offset_y = scroll_handle.max_offset().y.as_f32();
    let overflows = max_offset_y > 0.0
        || (viewport_height > 0.0 && content_height > viewport_height)
        || (viewport_height <= 0.0 && header_count > EFFECTIVE_HEADER_FALLBACK_VISIBLE_ROWS);
    if !overflows {
        return None;
    }

    let visible_fraction = if viewport_height > 0.0 {
        let measured_content_height = if max_offset_y > 0.0 {
            viewport_height + max_offset_y
        } else {
            content_height
        };
        viewport_height / measured_content_height.max(viewport_height)
    } else {
        EFFECTIVE_HEADER_FALLBACK_VISIBLE_ROWS as f32 / header_count as f32
    };
    Some(scrollbar_geometry(
        visible_fraction,
        scroll_handle.offset().y.as_f32(),
        max_offset_y,
    ))
}

const BODY_KINDS: [BodyKind; 5] = [
    BodyKind::None,
    BodyKind::Multipart,
    BodyKind::UrlEncoded,
    BodyKind::Raw,
    BodyKind::Json,
];

fn body_kind_index(kind: BodyKind) -> usize {
    BODY_KINDS
        .iter()
        .position(|candidate| *candidate == kind)
        .expect("all body kinds are represented in keyboard order")
}

fn body_type_from_kind(kind: BodyKind) -> BodyType {
    match kind {
        BodyKind::Json => BodyType::Json,
        BodyKind::UrlEncoded | BodyKind::Multipart => BodyType::FormData,
        BodyKind::None | BodyKind::Raw => BodyType::Raw,
    }
}

fn multipart_parts_preview(parts: &[MultipartPart]) -> String {
    if parts.is_empty() {
        return "(no complete parts)".to_string();
    }
    parts
        .iter()
        .map(|part| match &part.value {
            MultipartValue::Text(value) => format!("Text({} = {value})", part.name),
            MultipartValue::File {
                path,
                file_name,
                content_type,
            } => {
                let display_name = file_name.clone().unwrap_or_else(|| {
                    path.file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "selected file".to_string())
                });
                let content_type = content_type.as_deref().unwrap_or("content type: automatic");
                format!("File({} = {display_name}; {content_type})", part.name)
            }
        })
        .collect::<Vec<_>>()
        .join("  ·  ")
}

fn render_effective_header(header: EffectiveHeader, cx: &gpui::App) -> gpui::AnyElement {
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
        .border_color(LINE.resolve(cx))
        .bg(PANEL.resolve(cx))
        .child(
            div()
                .size(px(20.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .bg(OK_SOFT.resolve(cx))
                .font_family(FONT_UI)
                .font_weight(FontWeight::BOLD)
                .text_size(px(10.0))
                .text_color(OK.resolve(cx))
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
                        .text_color(TEXT.resolve(cx))
                        .child(header.name),
                )
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_color(SUBTEXT.resolve(cx))
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
                .bg((if generated { INFO_SOFT } else { ACCENT_SOFT }).resolve(cx))
                .font_family(FONT_UI)
                .font_weight(FontWeight::BOLD)
                .text_size(px(8.0))
                .text_color((if generated { INFO } else { ACCENT_INK }).resolve(cx))
                .child(if generated { "GENERATED" } else { "USER ROW" }),
        )
        .into_any_element()
}

fn render_url_encoded_header_chip(header: EffectiveHeader, cx: &gpui::App) -> gpui::AnyElement {
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
        .border_color(LINE.resolve(cx))
        .bg(PANEL.resolve(cx))
        .font_family(FONT_UI)
        .text_size(px(8.0))
        .text_color(SUBTEXT.resolve(cx))
        .child(
            div()
                .font_family(FONT_MONO)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(TEXT.resolve(cx))
                .child(format!("{}: {}", header.name, header.value)),
        )
        .child(
            div()
                .px_1()
                .py_1()
                .rounded_md()
                .bg((if generated { INFO_SOFT } else { ACCENT_SOFT }).resolve(cx))
                .text_color((if generated { INFO } else { ACCENT_INK }).resolve(cx))
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

#[cfg(test)]
mod tests {
    use super::multipart_parts_preview;
    use crate::models::{MultipartPart, MultipartValue};
    use std::path::PathBuf;

    #[test]
    fn multipart_preview_preserves_typed_text_part_order() {
        assert_eq!(
            multipart_parts_preview(&[
                MultipartPart::text("note", "hello multipart"),
                MultipartPart::text("category", "gpui"),
            ]),
            "Text(note = hello multipart)  ·  Text(category = gpui)"
        );
    }

    #[test]
    fn multipart_preview_renders_file_name_and_content_type_without_an_absolute_path() {
        assert_eq!(
            multipart_parts_preview(&[
                MultipartPart::text("note", "hello multipart"),
                MultipartPart {
                    name: "upload".to_string(),
                    value: MultipartValue::File {
                        path: PathBuf::from("/private/repository/tests/fixtures/upload.txt"),
                        file_name: Some("httpbingo-upload.txt".to_string()),
                        content_type: Some("text/plain".to_string()),
                    },
                },
            ]),
            "Text(note = hello multipart)  ·  File(upload = httpbingo-upload.txt; text/plain)"
        );
    }
}

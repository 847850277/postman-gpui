use gpui_kit::base::ElementExt;
mod editor;
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
        components::input::body_input::{BodyInput, BodyInputEvent, BodyType, FormDataEntry},
        theme::{
            ACCENT, ACCENT_INK, ACCENT_SOFT, FONT_MONO, FONT_UI, INFO, INFO_SOFT, LINE, MUTED, OK,
            OK_SOFT, PANEL, PANEL_ALT, SUBTEXT, TEXT,
        },
    },
};
use gpui::{
    div, prelude::FluentBuilder, px, AppContext, Context, Entity, FontWeight, InteractiveElement,
    IntoElement, ParentElement, Render, ScrollHandle, StatefulInteractiveElement, Styled,
    Subscription, Window,
};
use raw::render_raw_request_semantics;

use crate::ui::components::kit_controls::MethodState;
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    menu::DropdownMenu,
    scroll::{Scrollbar, ScrollbarMode},
    searchable_list::SearchableVec,
    select::{Select, SelectEvent, SelectState},
    IndexPath,
};

/// BodyPane owns BodyInput's text/form editing state. Complete body drafts remain authoritative in
/// the shared WorkspaceViewModel and are projected only on request or pane changes.
pub(in crate::app::postman_app::request_workspace) struct BodyPane {
    view_model: Entity<WorkspaceViewModel>,
    panel_layout: Entity<RequestPanelLayout>,
    body_input: Entity<BodyInput>,
    projected_tab_id: Option<RequestTabId>,
    effective_headers_scroll: ScrollHandle,
    raw_semantics_scroll: ScrollHandle,
    effective_headers_have_overflow: bool,
    raw_semantics_have_overflow: bool,
    kind_selector: Entity<MethodState>,
    raw_selector: Entity<MethodState>,
    editor_scroll: ScrollHandle,
    editor_error: Option<String>,
    projected_kind: Option<BodyKind>,
    projected_input_kind: Option<BodyKind>,
    projected_raw_format: Option<crate::models::request_draft::RawBodyFormat>,
    show_details: bool,
    _subscriptions: Vec<Subscription>,
}

impl BodyPane {
    pub(in crate::app::postman_app::request_workspace) fn new(
        view_model: Entity<WorkspaceViewModel>,
        panel_layout: Entity<RequestPanelLayout>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let kind_selector = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(BODY_LABELS.to_vec()),
                Some(IndexPath::new(0)),
                window,
                cx,
            )
        });
        let raw_selector = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(editor::RAW_LABELS.to_vec()),
                Some(IndexPath::new(0)),
                window,
                cx,
            )
        });
        let body_input = cx.new(|cx| BodyInput::new(window, cx));
        let subscriptions = vec![
            cx.subscribe(
                &raw_selector,
                |this, _, event: &SelectEvent<SearchableVec<&'static str>>, cx| {
                    if let SelectEvent::Confirm(Some(label)) = event {
                        if let Some(index) =
                            editor::RAW_LABELS.iter().position(|item| item == label)
                        {
                            this.update_active_request(cx, |request| {
                                request.set_raw_body_format(editor::RAW_FORMATS[index])
                            });
                        }
                    }
                },
            ),
            cx.subscribe(&body_input, Self::on_body_event),
            cx.subscribe_in(
                &kind_selector,
                window,
                |this, _, event: &SelectEvent<SearchableVec<&'static str>>, window, cx| {
                    if let SelectEvent::Confirm(Some(label)) = event {
                        if let Some(index) =
                            BODY_LABELS.iter().position(|candidate| candidate == label)
                        {
                            this.set_body_kind(BODY_KINDS[index], window, cx);
                        }
                    }
                },
            ),
        ];
        let mut pane = Self {
            view_model,
            panel_layout,
            body_input,
            projected_tab_id: None,
            effective_headers_scroll: ScrollHandle::new(),
            raw_semantics_scroll: ScrollHandle::new(),
            effective_headers_have_overflow: false,
            raw_semantics_have_overflow: false,
            kind_selector,
            raw_selector,
            editor_scroll: ScrollHandle::new(),
            editor_error: None,
            projected_kind: None,
            projected_input_kind: None,
            projected_raw_format: None,
            show_details: false,
            _subscriptions: subscriptions,
        };
        pane.project_active_request(window, cx);
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
        self.editor_error = None;
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
                                description: String::new(),
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
                    BodyKind::None | BodyKind::Json | BodyKind::Raw | BodyKind::Binary => {}
                });
            }
        }
    }

    fn set_body_kind(&mut self, kind: BodyKind, window: &mut Window, cx: &mut Context<Self>) {
        self.show_details = false;
        self.editor_error = None;
        self.editor_scroll.set_offset(gpui::point(px(0.), px(0.)));
        self.update_active_request(cx, |request| request.set_body_kind(kind));
        self.project_active_request(window, cx);
    }

    fn use_sample_json(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        self.project_active_request(window, cx);
    }

    fn clear_body(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor_error = None;
        self.update_active_request(cx, RequestViewModel::clear_body);
        self.project_active_request(window, cx);
    }

    pub(in crate::app::postman_app::request_workspace) fn input_entity(&self) -> Entity<BodyInput> {
        self.body_input.clone()
    }

    pub(in crate::app::postman_app::request_workspace) fn project_active_request(
        &mut self,
        window: &mut Window,
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
        if tab_changed {
            self.editor_error = None;
            self.show_details = false;
        }
        let projection_changed = tab_changed || self.projected_input_kind != Some(body_kind);
        self.projected_input_kind = Some(body_kind);
        self.body_input.update(cx, |input, cx| {
            input.set_type_silent(body_type_from_kind(body_kind), cx);
            input.set_form_data_allows_files(body_kind == BodyKind::Multipart, window, cx);
            match body_draft {
                RequestBodyDraft::None | RequestBodyDraft::Binary(_) => {
                    if projection_changed {
                        input.project_form_data_entries_with_rebind(Vec::new(), window, cx);
                    }
                    input.project_content("", cx);
                }
                RequestBodyDraft::Json(body) | RequestBodyDraft::Raw(body) => {
                    if projection_changed {
                        input.project_form_data_entries_with_rebind(Vec::new(), window, cx);
                    }
                    input.project_content(body, cx)
                }
                RequestBodyDraft::UrlEncoded(rows) => {
                    let entries = rows
                        .into_iter()
                        .map(|row| FormDataEntry::text(row.key, row.value, row.enabled))
                        .collect();
                    if projection_changed {
                        input.project_form_data_entries_with_rebind(entries, window, cx);
                    } else {
                        input.project_form_data_entries(entries, window, cx);
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
                    if projection_changed {
                        input.project_form_data_entries_with_rebind(entries, window, cx);
                    } else {
                        input.project_form_data_entries(entries, window, cx);
                    }
                }
            }
        });
        self.projected_tab_id = tab_id;
        cx.notify();
    }

    fn render_body_editor(&self, window: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        if !self.show_details {
            return self.render_prototype_editor(cx);
        }
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
        let is_url_encoded = kind == BodyKind::UrlEncoded;
        let is_multipart = kind == BodyKind::Multipart;
        let panel_height = self.panel_layout.read(cx).height();

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(PANEL.resolve(cx))
            .when(self.show_details, |pane| {
                pane.debug_selector(|| "body-details".into())
            })
            .child(self.render_body_toolbar(kind, cx))
            .when(is_multipart && !self.show_details, |pane| {
                pane.when_some(multipart_error.clone(), |pane, error| {
                    pane.child(
                        div()
                            .debug_selector(|| "body-multipart-file-error".into())
                            .flex_none()
                            .px_7()
                            .pb_2()
                            .text_size(crate::ui::theme::metrics::LABEL)
                            .text_color(crate::ui::theme::ERROR.resolve(cx))
                            .child(error),
                    )
                })
            })
            .child(if (is_url_encoded || is_multipart) && !self.show_details {
                div()
                    .debug_selector(|| "body-input".into())
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .px_7()
                    .pb_2()
                    .flex()
                    .flex_col()
                    .child(self.body_input.clone())
                    .into_any_element()
            } else if is_url_encoded {
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
        let (method, effective_url, effective_headers) = request_projection;
        let is_json = kind == BodyKind::Json;
        let is_raw = kind == BodyKind::Raw;
        let side_panel = if is_json {
            Some(self.render_effective_headers(effective_headers, cx))
        } else if is_raw {
            Some(render_raw_request_semantics(
                &body,
                method,
                &effective_url,
                effective_headers,
                &self.raw_semantics_scroll,
                self.raw_semantics_have_overflow,
                cx,
            ))
        } else {
            None
        };
        // At the minimum stacked height, one surface must own the available viewport.
        if self.show_details && panel_height < 330. {
            if let Some(side) = side_panel {
                return div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .flex()
                    .px_7()
                    .pb_2()
                    .child(side)
                    .into_any_element();
            }
        }
        div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .px_7()
            .pb_2()
            .gap_2()
            .child(
                div()
                    .debug_selector(|| "body-editor-shell".into())
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .debug_selector(|| "body-input".into())
                            .flex_1()
                            .min_h_0()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(self.body_input.clone()),
                    ),
            )
            .when(self.show_details, |pane| {
                pane.when_some(side_panel, |pane, side| {
                    pane.child(div().flex_1().min_h_0().min_w_0().flex().child(side))
                })
            })
            .into_any_element()
    }

    fn render_body_toolbar(&self, kind: BodyKind, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::ui::{components::kit_controls, theme::metrics as m};
        let selector = BODY_SELECTORS[body_kind_index(kind)];
        div()
            .debug_selector(|| "body-kind-selector".into())
            .h(gpui::rems(55. / 16.))
            .flex_none()
            .px_7()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex_none()
                    .text_size(gpui::rems(11. / 16.))
                    .text_color(TEXT.resolve(cx))
                    .child("Request body"),
            )
            .child(
                div()
                    .debug_selector(move || selector.into())
                    .flex_none()
                    .child(
                        Select::new(&self.kind_selector)
                            .id("body-kind-select")
                            .accessibility_label("Request body format")
                            .h(gpui::rems(28. / 16.))
                            .w(gpui::rems(match kind {
                                BodyKind::Multipart => 6.5,
                                BodyKind::UrlEncoded => 8.,
                                _ => 5.5,
                            }))
                            .menu_width(gpui::rems(12.))
                            .text_size(gpui::rems(11. / 16.))
                            .bg(PANEL_ALT.resolve(cx))
                            .rounded(m::RADIUS),
                    ),
            )
            .child(div().flex_1())
            .child(
                kit_controls::editor_button(
                    "body-details-toggle",
                    if self.show_details {
                        "Editor"
                    } else {
                        "Details"
                    },
                    cx,
                )
                .debug_selector(|| "body-details-toggle".into())
                .accessibility_label("Toggle effective request details")
                .disabled(kind == BodyKind::None)
                .px_1()
                .text_size(m::CAPTION)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show_details = !this.show_details;
                    cx.notify();
                })),
            )
            .child(
                Button::new("body-actions")
                    .ghost()
                    .label("⋯")
                    .debug_selector(|| "body-actions".into())
                    .accessibility_label("Body actions")
                    .w_6()
                    .p_0()
                    .dropdown_menu({
                        let owner = cx.entity().downgrade();
                        move |menu, _, _| {
                            let sample = owner.clone();
                            let clear = owner.clone();
                            menu.item(
                                gpui_kit::component::menu::PopupMenuItem::new("Sample JSON")
                                    .on_click(move |_, window, cx| {
                                        let _ = sample.update(cx, |this, cx| {
                                            this.use_sample_json(window, cx)
                                        });
                                    }),
                            )
                            .item(
                                gpui_kit::component::menu::PopupMenuItem::new("Clear body")
                                    .on_click(move |_, window, cx| {
                                        let _ = clear
                                            .update(cx, |this, cx| this.clear_body(window, cx));
                                    }),
                            )
                        }
                    }),
            )
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
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let count = headers.len();
        div()
            .debug_selector(|| "body-effective-headers".into())
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
                            .when(self.effective_headers_have_overflow, |list| list.pr_5())
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
                    .on_prepaint({
                        let this = cx.weak_entity();
                        let scroll = self.effective_headers_scroll.clone();
                        let previous = self.effective_headers_have_overflow;
                        move |_, window, cx| {
                            let has_overflow = scroll.max_offset().y > gpui::Pixels::ZERO;
                            if has_overflow != previous {
                                window.defer(cx, move |_, cx| {
                                    let _ = this.update(cx, |this, cx| {
                                        if this.effective_headers_have_overflow != has_overflow {
                                            this.effective_headers_have_overflow = has_overflow;
                                            cx.notify();
                                        }
                                    });
                                });
                            }
                        }
                    })
                    .when(self.effective_headers_have_overflow, |viewport| {
                        viewport.child(
                            div()
                                .debug_selector(|| "body-effective-headers-scrollbar".into())
                                .absolute()
                                .top_0()
                                .right_0()
                                .bottom_0()
                                .w(Scrollbar::width())
                                .child(
                                    Scrollbar::vertical(&self.effective_headers_scroll)
                                        .id("body-effective-headers-scrollbar-control")
                                        .mode(ScrollbarMode::Always),
                                ),
                        )
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
}

impl Render for BodyPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let kind = self
            .view_model
            .read(cx)
            .active_request()
            .map_or(BodyKind::None, |r| r.body_kind());
        let label = BODY_LABELS[body_kind_index(kind)];
        if self.projected_kind != Some(kind) {
            self.projected_kind = Some(kind);
            self.kind_selector
                .update(cx, |state, cx| state.set_selected_value(&label, window, cx));
        }
        let raw_format = self
            .view_model
            .read(cx)
            .active_request()
            .map(|request| request.raw_body_format())
            .unwrap_or_default();
        let raw_label = editor::RAW_LABELS[editor::RAW_FORMATS
            .iter()
            .position(|format| *format == raw_format)
            .unwrap_or(0)];
        if self.projected_raw_format != Some(raw_format) {
            self.projected_raw_format = Some(raw_format);
            self.raw_selector.update(cx, |state, cx| {
                state.set_selected_value(&raw_label, window, cx)
            });
        }
        self.render_body_editor(window, cx)
    }
}

const BODY_LABELS: [&str; 6] = ["None", "Form-data", "URL encoded", "Raw", "JSON", "Binary"];
const BODY_SELECTORS: [&str; 6] = [
    "body-kind-none",
    "body-kind-form-data",
    "body-kind-url-encoded",
    "body-kind-raw",
    "body-kind-json",
    "body-kind-binary",
];
const BODY_KINDS: [BodyKind; 6] = [
    BodyKind::None,
    BodyKind::Multipart,
    BodyKind::UrlEncoded,
    BodyKind::Raw,
    BodyKind::Json,
    BodyKind::Binary,
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
        BodyKind::None | BodyKind::Raw | BodyKind::Binary => BodyType::Raw,
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

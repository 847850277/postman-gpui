//! Primary Body surface matching prototypes/body-editor.js. Drafts live in RequestDraft;
//! this view only owns selection controls, transient validation and file-picker coordination.
use super::*;
use crate::ui::components::kit_controls::editor_button;
use crate::{
    app::RequestPane,
    models::request_draft::RawBodyFormat,
    ui::theme::{metrics as m, ERROR},
};
use gpui::{ExternalPaths, Focusable};
use gpui_kit::component::scroll::{ScrollableElement, ScrollbarAxis};
use gpui_kit::{assets::IconName, component::Icon};
use std::path::PathBuf;

pub(super) const RAW_LABELS: [&str; 4] = ["Text", "XML", "HTML", "JavaScript"];
pub(super) const RAW_FORMATS: [RawBodyFormat; 4] = [
    RawBodyFormat::Text,
    RawBodyFormat::Xml,
    RawBodyFormat::Html,
    RawBodyFormat::JavaScript,
];
const TYPES: [(BodyKind, &str, &str); 6] = [
    (BodyKind::None, "None", "body-kind-none"),
    (BodyKind::Json, "JSON", "body-kind-json"),
    (BodyKind::Raw, "Raw", "body-kind-raw"),
    (BodyKind::UrlEncoded, "URL encoded", "body-kind-url-encoded"),
    (BodyKind::Multipart, "Form-data", "body-kind-form-data"),
    (BodyKind::Binary, "Binary", "body-kind-binary"),
];

impl BodyPane {
    pub(in crate::app::postman_app::request_workspace) fn validate_before_send(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        self.editor_error = self
            .view_model
            .read(cx)
            .active_request()
            .and_then(|request| request.body_validation_error());
        if self.editor_error.is_some() {
            self.show_details = false;
            self.update_active_request(cx, |request| request.set_request_pane(RequestPane::Body));
            self.project_active_request(cx);
            cx.notify();
            return false;
        }
        true
    }

    fn format_json(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let body = self
            .view_model
            .read(cx)
            .active_request()
            .map(|r| r.body())
            .unwrap_or_default();
        match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(value) => {
                self.editor_error = None;
                self.update_active_request(cx, |request| {
                    request.set_body(
                        serde_json::to_string_pretty(&value).expect("JSON value serializes"),
                    )
                });
                self.project_active_request(cx);
            }
            Err(error) => self.editor_error = Some(format!("Invalid JSON: {error}")),
        }
        self.body_input.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    fn choose_binary_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((tab_id, revision)) = self.binary_file_target(cx) else {
            return;
        };
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select binary body".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            if let Some(path) = paths.into_iter().next() {
                let _ = this.update(cx, |this, cx| {
                    this.load_binary_file(tab_id, revision, path, cx)
                });
            }
        })
        .detach();
    }

    fn binary_file_target(&self, cx: &Context<Self>) -> Option<(RequestTabId, u64)> {
        let request = self.view_model.read(cx).active_request()?;
        (request.body_kind() == BodyKind::Binary)
            .then(|| (request.tab_id(), request.draft_revision()))
    }

    fn binary_file_target_is_current(
        &self,
        tab_id: RequestTabId,
        revision: u64,
        cx: &Context<Self>,
    ) -> bool {
        self.view_model
            .read(cx)
            .request_for_tab(tab_id)
            .is_some_and(|request| {
                request.body_kind() == BodyKind::Binary && request.draft_revision() == revision
            })
    }

    fn load_binary_file(
        &mut self,
        tab_id: RequestTabId,
        revision: u64,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        // No filesystem work in render or on the UI executor. A completed picker/drop may
        // only update the original draft, even if History replaces it in the same tab.
        if !self.binary_file_target_is_current(tab_id, revision, cx) {
            return;
        }
        let metadata = cx.background_executor().spawn({
            let path = path.clone();
            async move { std::fs::metadata(path) }
        });
        cx.spawn(async move |this, cx| {
            let result = metadata.await;
            let _ = this.update(cx, |this, cx| {
                if !this.binary_file_target_is_current(tab_id, revision, cx) {
                    return;
                }
                let size = match result {
                    Ok(metadata) if metadata.is_file() => metadata.len(),
                    _ => {
                        if this.view_model.read(cx).active_tab_id() == Some(tab_id) {
                            this.editor_error =
                                Some("Choose a readable file for the binary body.".into());
                            cx.notify();
                        }
                        return;
                    }
                };
                this.view_model.update(cx, |model, cx| {
                    model.update_request_for_tab(tab_id, |request| {
                        request.set_binary_file(path, Some(size));
                    });
                    cx.notify();
                });
                if this.view_model.read(cx).active_tab_id() == Some(tab_id) {
                    this.editor_error = None;
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn render_actions(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let details_available = self
            .view_model
            .read(cx)
            .active_request()
            .is_some_and(|request| {
                !matches!(request.body_kind(), BodyKind::None | BodyKind::Binary)
            });
        Button::new("body-actions")
            .ghost()
            .label("⋯")
            .debug_selector(|| "body-actions".into())
            .accessibility_label("Body actions")
            .size(px(28.))
            .dropdown_menu({
                let owner = cx.entity().downgrade();
                move |menu, _, _| {
                    let sample = owner.clone();
                    let clear = owner.clone();
                    let details = owner.clone();
                    menu.item(
                        gpui_kit::component::menu::PopupMenuItem::new("Sample JSON").on_click(
                            move |_, _, cx| {
                                let _ = sample.update(cx, |this, cx| this.use_sample_json(cx));
                            },
                        ),
                    )
                    .item(
                        gpui_kit::component::menu::PopupMenuItem::new("Clear body").on_click(
                            move |_, _, cx| {
                                let _ = clear.update(cx, |this, cx| this.clear_body(cx));
                            },
                        ),
                    )
                    .item(
                        gpui_kit::component::menu::PopupMenuItem::new("Request details")
                            .disabled(!details_available)
                            .on_click(move |_, _, cx| {
                                let _ = details.update(cx, |this, cx| {
                                    this.show_details = true;
                                    cx.notify();
                                });
                            }),
                    )
                }
            })
    }

    pub(super) fn render_prototype_editor(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let model = self.view_model.read(cx);
        let Some(request) = model.active_request() else {
            return div().into_any_element();
        };
        let kind = request.body_kind();
        let body = request.body();
        let draft = request.body_draft().clone();
        let size = request.binary_size();
        let method = request.method();
        let construction = request.request_construction();
        let header = construction
            .effective_headers()
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case("content-type"));
        let (content_type, source) = match header {
            Some(header) => (
                header.value.clone(),
                if header.source == EffectiveHeaderSource::User {
                    "Set in Headers"
                } else {
                    "Automatic"
                },
            ),
            None if kind == BodyKind::Multipart && method.allows_body() => (
                "multipart/form-data · boundary generated when sending".into(),
                "Automatic",
            ),
            None => ("Not set".into(), "Add in Headers"),
        };
        let send_error = match request.response() {
            ResponseState::Error { message }
                if matches!(kind, BodyKind::Multipart | BodyKind::Binary) =>
            {
                Some(message.clone())
            }
            _ => None,
        };
        let validation = self.editor_error.clone().or(send_error);
        let panel_height = self.panel_layout.read(cx).height();
        let is_text = matches!(kind, BodyKind::Json | BodyKind::Raw);
        div().flex_1().min_h_0().min_w_0().flex().flex_col().relative()
            .child(div().id("body-editor-scroll").debug_selector(|| "body-editor-scroll".into()).flex_1().min_h_0().min_w_0()
                .overflow_y_scroll().track_scroll(&self.editor_scroll)
                .child(div().debug_selector(|| "body-primary-editor".into()).min_h(px(330.)).h(px(panel_height.max(330.))).flex().flex_col().px_7().pt_4().pb_4().gap_3()
                    .child(div().flex_none().flex().items_center().child(div().text_size(m::LABEL).child("Body type")).child(div().flex_1()).child(self.render_actions(cx)))
                    .child(div().debug_selector(|| "body-types".into()).flex_none().flex().flex_wrap().gap_1().children(TYPES.into_iter().map(|(item, label, id)| {
                        editor_button(id, label, cx).selected(kind == item).debug_selector(move || id.into()).accessibility_label(format!("{label} body"))
                            .h(px(30.)).px(px(10.)).text_size(m::LABEL).font_weight(FontWeight::NORMAL).rounded(m::RADIUS).border_1()
                            .border_color(if kind == item { LINE.resolve(cx) } else { gpui::rgba(0) })
                            .bg(if kind == item { PANEL_ALT.resolve(cx) } else { PANEL.resolve(cx) })
                            .on_click(cx.listener(move |this, _, _, cx| this.set_body_kind(item, cx)))
                    })))
                    .child(div().debug_selector(|| "body-content-type".into()).flex_none().flex().flex_wrap().items_center().gap_2().text_size(m::CAPTION).text_color(SUBTEXT.resolve(cx))
                        .child("Content-Type").child(div().font_family(FONT_MONO).child(content_type))
                        .child(editor_button("body-header-source", source, cx).debug_selector(|| "body-header-source".into()).h(px(22.)).px_1().text_size(m::CAPTION).text_color(ACCENT.resolve(cx))
                            .on_click(cx.listener(|this, _, _, cx| { this.update_active_request(cx, |request| request.set_request_pane(RequestPane::Headers)); }))))
                    .when(!method.allows_body() && kind != BodyKind::None, |pane| pane.child(div().text_size(m::CAPTION).text_color(SUBTEXT.resolve(cx)).child(format!("{method} sends no body. Your draft is kept when you change the method."))))
                    .child(match kind {
                        BodyKind::None => div().debug_selector(|| "body-empty".into()).flex_1().min_h(px(150.)).flex().flex_col().justify_center().items_center().gap_3().text_color(SUBTEXT.resolve(cx))
                            .child(Icon::new(IconName::Code).size(px(20.)))
                            .child(div().text_color(TEXT.resolve(cx)).child("This request has no body"))
                            .child(div().text_size(m::LABEL).child("Choose a body type to include data with this request.")).into_any_element(),
                        BodyKind::Json | BodyKind::Raw => self.render_primary_text(kind, &body, cx),
                        BodyKind::UrlEncoded | BodyKind::Multipart => self.render_primary_form(kind, &body, cx),
                        BodyKind::Binary => self.render_binary(draft, size, cx),
                    })
                    .when_some(validation, |pane, message| pane.child(div().debug_selector(|| "body-validation-error".into()).flex_none().text_size(m::LABEL).text_color(ERROR.resolve(cx)).child(message)))
                    .when(!is_text, |pane| pane.child(div().flex_1().min_h_0()))
                )).scrollbar(&self.editor_scroll, ScrollbarAxis::Vertical).into_any_element()
    }

    fn render_primary_text(
        &self,
        kind: BodyKind,
        body: &str,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let json = kind == BodyKind::Json;
        let valid = json && serde_json::from_str::<serde_json::Value>(body).is_ok();
        div()
            .flex_1()
            .min_h(px(195.))
            .min_w_0()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_3()
                    .h(px(30.))
                    .text_size(m::LABEL)
                    .child(if json { "JSON body" } else { "Format" })
                    .when(json, |toolbar| toolbar.child(div().flex_1()))
                    .when(json, |toolbar| {
                        toolbar.child(
                            editor_button("body-format-json", "Format JSON", cx)
                                .debug_selector(|| "body-format-json".into())
                                .h(px(28.))
                                .text_size(m::LABEL)
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.format_json(window, cx)),
                                ),
                        )
                    })
                    .when(!json, |toolbar| {
                        toolbar.child(
                            div().w(px(128.)).flex_none().child(
                                Select::new(&self.raw_selector)
                                    .id("body-raw-format")
                                    .accessibility_label("Raw format")
                                    .menu_width(px(160.))
                                    .w_full()
                                    .h(px(28.))
                                    .text_size(m::LABEL),
                            ),
                        )
                    }),
            )
            .child(
                div()
                    .debug_selector(|| "body-input".into())
                    .flex_1()
                    .min_h(px(150.))
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(self.body_input.clone()),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .justify_between()
                    .text_size(m::CAPTION)
                    .text_color(SUBTEXT.resolve(cx))
                    .child(
                        div()
                            .debug_selector(|| "body-text-status".into())
                            .child(if json {
                                if valid {
                                    "Valid JSON"
                                } else {
                                    "JSON"
                                }
                            } else {
                                "Plain text editor"
                            }),
                    )
                    .child(
                        div()
                            .debug_selector(|| "body-byte-count".into())
                            .child(format!("{} B", body.len())),
                    ),
            )
            .into_any_element()
    }

    fn render_primary_form(
        &self,
        kind: BodyKind,
        body: &str,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let encoded = kind == BodyKind::UrlEncoded;
        let available = (self.panel_layout.read(cx).height() - if encoded { 310. } else { 260. })
            .clamp(130., 360.);
        let width = (self.panel_layout.read(cx).width() - 56.).max(0.);
        let form_height = self
            .body_input
            .read(cx)
            .preferred_form_height(px(width), cx)
            .as_f32()
            .min(available);
        div().min_h_0().min_w_0().flex().flex_col().gap_3()
            .child(div().flex_none().text_size(m::CAPTION).text_color(SUBTEXT.resolve(cx)).child(if encoded {
                "Keys and values are URL encoded automatically. Repeated keys are preserved."
            } else { "Combine text fields and files in one request." }))
            .child(div().debug_selector(|| "body-input".into()).h(px(form_height)).min_w_0().flex().flex_col().child(self.body_input.clone()))
            .when(encoded, |pane| pane.child(div().text_size(m::CAPTION).text_color(SUBTEXT.resolve(cx)).child("Encoded body"))
                .child(div().debug_selector(|| "body-encoded-preview".into()).min_h(px(40.)).px_3().py_2().border_1().border_color(LINE.resolve(cx)).rounded(m::RADIUS).bg(PANEL_ALT.resolve(cx)).font_family(FONT_MONO).text_size(m::LABEL).text_color(SUBTEXT.resolve(cx)).child(if body.is_empty() { "No enabled fields".to_string() } else { body.to_string() })))
            .when(!encoded, |pane| pane.child(div().text_size(m::CAPTION).text_color(SUBTEXT.resolve(cx)).child("Files are read from disk when you send this request.")))
            .into_any_element()
    }

    fn render_binary(
        &self,
        draft: RequestBodyDraft,
        size: Option<u64>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let path = match draft {
            RequestBodyDraft::Binary(path) => path,
            _ => PathBuf::new(),
        };
        let selected = !path.as_os_str().is_empty();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Choose a file or drop it here".into());
        let caption = if selected {
            format!(
                "{} · {}",
                size.map(|size| format!("{size} B"))
                    .unwrap_or_else(|| "File on disk".into()),
                mime_guess::from_path(&path).first_or_octet_stream()
            )
        } else {
            "Send a single file as the entire request body.".into()
        };
        div()
            .id("body-file-drop")
            .debug_selector(|| "body-file-drop".into())
            .min_h(px(180.))
            .flex_none()
            .p_6()
            .rounded(m::RADIUS)
            .border_1()
            .border_dashed()
            .border_color(LINE.resolve(cx))
            .bg(PANEL_ALT.resolve(cx))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .child(Icon::new(IconName::Upload).size(px(20.)))
            .child(
                div()
                    .debug_selector(|| "body-binary-file-name".into())
                    .text_size(m::LABEL)
                    .child(name),
            )
            .child(
                div()
                    .text_size(m::CAPTION)
                    .text_color(SUBTEXT.resolve(cx))
                    .child(caption),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        editor_button(
                            "body-choose-file",
                            if selected {
                                "Change file…"
                            } else {
                                "Choose file…"
                            },
                            cx,
                        )
                        .border_1()
                        .border_color(LINE.resolve(cx))
                        .bg(PANEL.resolve(cx))
                        .debug_selector(|| "body-choose-file".into())
                        .h(px(30.))
                        .text_size(m::LABEL)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.choose_binary_file(window, cx)),
                        ),
                    )
                    .when(selected, |actions| {
                        actions.child(
                            editor_button("body-remove-file", "Remove file", cx)
                                .debug_selector(|| "body-remove-file".into())
                                .h(px(30.))
                                .text_size(m::LABEL)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.editor_error = None;
                                    this.update_active_request(cx, |request| {
                                        request.set_binary_file(PathBuf::new(), None)
                                    });
                                })),
                        )
                    }),
            )
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                if paths.0.len() != 1 {
                    this.editor_error = Some(
                        "Choose one file for a binary body. Use Form-data for multiple files."
                            .into(),
                    );
                    cx.notify();
                    return;
                }
                if let Some((tab, revision)) = this.binary_file_target(cx) {
                    this.load_binary_file(tab, revision, paths.0[0].clone(), cx);
                }
            }))
            .into_any_element()
    }
}

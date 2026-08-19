use super::RequestEditor;
use crate::{
    app::{BodyKind, MultipartDraftValue, RequestBodyDraft, RequestPane},
    ui::components::body_input::{BodyType, FormDataEntry},
};
use gpui::Context;

impl RequestEditor {
    /// One-way VM -> editor projection. Editor buffers retain cursor/selection state, but they
    /// never participate in request construction.
    pub(super) fn project_active_request(&mut self, cx: &mut Context<Self>) {
        self.project_method(cx);
        self.project_url(cx);
        self.rebuild_param_row_editors(cx);
        self.rebuild_header_row_editors(cx);
        self.project_row_draft(cx);
        self.project_body(cx);
        self.project_authorization(cx);
        self.project_scripts(cx);
    }

    pub(super) fn project_method(&self, cx: &mut Context<Self>) {
        let method = self.view_model.read(cx).method();
        self.method_selector
            .update(cx, |selector, cx| selector.project_method(method, cx));
    }

    pub(super) fn project_url(&self, cx: &mut Context<Self>) {
        let url = self.view_model.read(cx).url().to_string();
        self.url_input
            .update(cx, |input, cx| input.project_url(url, cx));
    }

    pub(super) fn project_row_draft(&self, cx: &mut Context<Self>) {
        let (key, value, key_placeholder, value_placeholder) = {
            let view_model = self.view_model.read(cx);
            let (key, value) = view_model
                .row_draft(view_model.request_pane())
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .unwrap_or_default();
            let placeholders = match view_model.request_pane() {
                RequestPane::Headers => ("Header name", "Header value"),
                RequestPane::Params
                | RequestPane::Authorization
                | RequestPane::Body
                | RequestPane::Scripts
                | RequestPane::Tests => ("Key", "Value"),
            };
            (key, value, placeholders.0, placeholders.1)
        };
        self.row_key_input.update(cx, |input, cx| {
            input.project_placeholder(key_placeholder, cx);
            input.project_content(key, cx);
        });
        self.row_value_input.update(cx, |input, cx| {
            input.project_placeholder(value_placeholder, cx);
            input.project_content(value, cx);
        });
    }

    pub(super) fn project_body(&self, cx: &mut Context<Self>) {
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
    }

    pub(super) fn project_authorization(&self, cx: &mut Context<Self>) {
        let (bearer_token, basic_username, basic_password) = {
            let view_model = self.view_model.read(cx);
            (
                view_model.bearer_token().to_string(),
                view_model.basic_username().to_string(),
                view_model.basic_password().to_string(),
            )
        };
        self.authorization_input
            .update(cx, |input, cx| input.project_content(bearer_token, cx));
        self.basic_username_input
            .update(cx, |input, cx| input.project_content(basic_username, cx));
        self.basic_password_input
            .update(cx, |input, cx| input.project_content(basic_password, cx));
    }

    pub(super) fn project_scripts(&self, cx: &mut Context<Self>) {
        let (pre_request_script, tests_script) = {
            let view_model = self.view_model.read(cx);
            (
                view_model.pre_request_script().to_string(),
                view_model.tests_script().to_string(),
            )
        };
        self.script_input.update(cx, |input, cx| {
            input.set_type_silent(BodyType::Raw, cx);
            input.project_content(pre_request_script, cx);
        });

        self.tests_input.update(cx, |input, cx| {
            input.set_type_silent(BodyType::Raw, cx);
            input.project_content(tests_script, cx);
        });
    }
}
fn body_type_from_kind(kind: BodyKind) -> BodyType {
    match kind {
        BodyKind::Json => BodyType::Json,
        BodyKind::UrlEncoded | BodyKind::Multipart => BodyType::FormData,
        BodyKind::None | BodyKind::Raw => BodyType::Raw,
    }
}

//! Version-one document codec for transport types; transport remains independent of storage.
use super::*;
#[derive(Serialize, Deserialize)]
#[serde(remote = "HttpMethod")]
#[allow(clippy::upper_case_acronyms)]
pub(super) enum Method {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
    HEAD,
    OPTIONS,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "RedirectPolicy")]
enum Redirect {
    Follow,
    DoNotFollow,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "RequestOptions")]
pub(super) struct Options {
    timeout_ms: Option<u64>,
    #[serde(with = "Redirect")]
    redirect_policy: RedirectPolicy,
    max_redirect_hops: u32,
}

impl RequestDraft {
    /// Snapshot file references as absolute paths without reading the referenced files.
    /// No response/runtime credentials participate in the editable draft.
    pub(crate) fn document_snapshot(&self) -> Self {
        let mut draft = self.clone();
        draft.binary_size = None;
        if let Ok(base) = std::env::current_dir() {
            for body in std::iter::once(&mut draft.body).chain(&mut draft.inactive_bodies) {
                let absolute = |path: &mut PathBuf| {
                    if !path.as_os_str().is_empty() && path.is_relative() {
                        *path = base.join(&*path);
                    }
                };
                match body {
                    RequestBodyDraft::Binary(path) => absolute(path),
                    RequestBodyDraft::Multipart(parts) => {
                        for part in parts {
                            if let MultipartDraftValue::File { path, .. } = &mut part.value {
                                absolute(path);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        draft
    }
}

impl RequestDraft {
    /// Compare editor values without allocating the payload or including cached file metadata.
    pub(crate) fn same_document_content(&self, other: &Self) -> bool {
        self.method == other.method
            && self.url == other.url
            && self.params == other.params
            && self.param_draft == other.param_draft
            && self.headers == other.headers
            && self.header_draft == other.header_draft
            && same_body(&self.body, &other.body)
            && self.inactive_bodies.len() == other.inactive_bodies.len()
            && self
                .inactive_bodies
                .iter()
                .zip(&other.inactive_bodies)
                .all(|(a, b)| same_body(a, b))
            && self.raw_format == other.raw_format
            && self.content_type_source == other.content_type_source
            && self.accept_source == other.accept_source
            && self.authorization_kind == other.authorization_kind
            && self.bearer_token == other.bearer_token
            && self.basic_username == other.basic_username
            && self.basic_password == other.basic_password
            && self.request_options == other.request_options
    }
}
fn same_path(a: &std::path::Path, b: &std::path::Path) -> bool {
    a == b
        || std::env::current_dir().is_ok_and(|base| {
            !a.as_os_str().is_empty() && !b.as_os_str().is_empty() && base.join(a) == base.join(b)
        })
}
fn same_body(a: &RequestBodyDraft, b: &RequestBodyDraft) -> bool {
    match (a, b) {
        (RequestBodyDraft::Binary(a), RequestBodyDraft::Binary(b)) => same_path(a, b),
        (RequestBodyDraft::Multipart(a), RequestBodyDraft::Multipart(b)) => {
            a.len() == b.len()
                && a.iter().zip(b).all(|(a, b)| {
                    a.name == b.name
                        && a.enabled == b.enabled
                        && match (&a.value, &b.value) {
                            (
                                MultipartDraftValue::File {
                                    path: a,
                                    file_name: an,
                                    content_type: at,
                                },
                                MultipartDraftValue::File {
                                    path: b,
                                    file_name: bn,
                                    content_type: bt,
                                },
                            ) => same_path(a, b) && an == bn && at == bt,
                            (a, b) => a == b,
                        }
                })
        }
        (a, b) => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::request_document::RequestDocument;
    #[test]
    fn version_one_preserves_every_body_and_incomplete_editor_intent() {
        let bodies = vec![
            RequestBodyDraft::None,
            RequestBodyDraft::Json("{ unfinished".into()),
            RequestBodyDraft::Raw(String::new()),
            RequestBodyDraft::UrlEncoded(vec![
                KeyValueRow::enabled("tag", "one"),
                KeyValueRow {
                    enabled: false,
                    key: "tag".into(),
                    value: "two".into(),
                    description: "Keep disabled row".into(),
                },
            ]),
            RequestBodyDraft::Multipart(vec![
                MultipartDraftPart::text("", "unfinished", false),
                MultipartDraftPart::file(
                    "upload",
                    "missing/upload.bin",
                    Some("custom.bin".into()),
                    Some("application/octet-stream".into()),
                    true,
                ),
            ]),
            RequestBodyDraft::Binary("missing/binary.bin".into()),
        ];
        for body in &bodies {
            let mut draft = RequestDraft::new();
            draft.method = HttpMethod::PATCH;
            draft.url = "incomplete URL ?tag=a&tag=b%2f".into();
            draft.params = vec![
                KeyValueRow::enabled("tag", "a"),
                KeyValueRow::enabled("tag", "b"),
            ];
            draft.param_draft = KeyValueDraft {
                key: "unfinished".into(),
                value: "".into(),
                description: "note".into(),
            };
            draft.headers = vec![
                KeyValueRow::enabled("X-Repeat", "1"),
                KeyValueRow::enabled("X-Repeat", "2"),
            ];
            draft.headers[1].enabled = false;
            draft.header_draft = KeyValueDraft {
                key: "".into(),
                value: "partial".into(),
                description: "keep".into(),
            };
            draft.body = body.clone();
            draft.inactive_bodies = bodies.clone();
            draft.raw_format = RawBodyFormat::Xml;
            draft.content_type_source = ManagedHeaderSource::User;
            draft.accept_source = ManagedHeaderSource::Automatic;
            draft.authorization_kind = AuthorizationKind::Basic;
            draft.bearer_token = "explicit-token".into();
            draft.basic_username = "user".into();
            draft.basic_password = "explicit-password".into();
            draft.request_options = RequestOptions {
                timeout_ms: Some(1500),
                redirect_policy: RedirectPolicy::DoNotFollow,
                max_redirect_hops: 4,
            };
            draft.binary_size = Some(999);
            let doc = RequestDocument::snapshot(
                uuid::Uuid::new_v4().to_string(),
                "Draft 用户".into(),
                1,
                &draft,
                "pre()",
                "test()",
            );
            let json = serde_json::to_string(&doc).unwrap();
            let restored: RequestDocument = serde_json::from_str(&json).unwrap();
            assert!(restored == doc);
            assert!(restored.same_content(&draft, "pre()", "test()"));
            assert!(restored.draft().binary_size.is_none());
            assert!(json.contains("explicit-password")); // Explicit Save retains user input; history has its own redaction policy.
            assert!(!json.contains("binary_size"));
        }
    }
}

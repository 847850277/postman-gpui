use super::*;
use crate::models::{
    AuthorizationKind, BodyKind, HttpMethod, KeyValueRow, RedirectPolicy, Request, RequestOptions,
};
use futures::TryStreamExt;
use postman_flow::*;
use postman_http::{HttpError, HttpResponse, HttpTransport};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<(Request, RequestOptions)>>>);
impl HttpTransport for Recorder {
    async fn execute(
        &self,
        request: Request,
        options: RequestOptions,
    ) -> Result<HttpResponse, HttpError> {
        self.0.lock().unwrap().push((request, options));
        Ok(HttpResponse::new(200, vec![], "{}".into()))
    }
}

async fn replay(draft: &RequestDraft) {
    let original = draft.clone();
    let expected = draft.construct();
    let mut document = FlowDocument::new(FlowDefinition::new("Copied request"));
    document.flow.steps.push(HttpStepDefinition::new(
        "copied",
        "Copy",
        convert(draft, "", "").unwrap(),
    ));
    let yaml = write_flow_yaml(&document).unwrap();
    let parsed = parse_flow_yaml(&yaml).unwrap();
    assert_eq!(document, parsed);
    let plan = compile_flow(
        &parsed.flow,
        &ApiCatalog::new(),
        &CompileEnvironment::default(),
    )
    .unwrap();
    let transport = Recorder::default();
    let events: Vec<_> = execute_flow(
        plan,
        transport.clone(),
        FlowSessionEnvironment::default().with_request_options(RequestOptions {
            timeout_ms: Some(999),
            redirect_policy: RedirectPolicy::DoNotFollow,
            max_redirect_hops: 1,
        }),
    )
    .unwrap()
    .try_collect()
    .await
    .unwrap();
    assert!(matches!(
        events.last(),
        Some(FlowEvent::FlowFinished { success: true, .. })
    ));
    let recorded = transport.0.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(&recorded[0].0, expected.request());
    let mut options = expected.request_options();
    options.timeout_ms = Some(options.timeout_ms.unwrap_or(0));
    assert_eq!(recorded[0].1, options);
    assert_eq!(draft, &original);
}

#[tokio::test]
async fn native_codec_and_runtime_preserve_every_body_kind_and_literal_bytes() {
    for body in [
        RequestBody::None,
        RequestBody::Raw(String::new()),
        RequestBody::UrlEncoded(String::new()),
        RequestBody::Json("{ \"name\": \"{{secret}} 中文\", \"n\": 1 }\n".into()),
        RequestBody::Raw("{{inputs.secret}}\nquotes: '\" 空格".into()),
        RequestBody::UrlEncoded("x=a%2Bb&x=%E4%B8%AD%E6%96%87&empty=&literal=%7B%7Bx%7D%7D".into()),
        RequestBody::File(std::env::temp_dir().join("missing-228-{{literal}}.bin")),
    ] {
        let request = Request {
            method: HttpMethod::POST,
            url: "https://httpbingo.org/anything/{{literal}}?x=a%2Bb&x=2&empty=&secret=hidden"
                .into(),
            headers: vec![
                ("X-Repeat".into(), "{{literal}}".into()),
                ("X-Repeat".into(), "two".into()),
                ("X-Empty".into(), "".into()),
                ("Content-Type".into(), "custom/type".into()),
                ("Authorization".into(), "Custom secret".into()),
            ],
            body,
        };
        replay(&RequestDraft::from_request(&request)).await;
    }
}

#[tokio::test]
async fn managed_auth_headers_options_and_live_rows_use_send_construction() {
    for auth in [AuthorizationKind::Bearer, AuthorizationKind::Basic] {
        for timeout in [None, Some(0), Some(1234)] {
            let mut draft = RequestDraft::new();
            draft.set_method(HttpMethod::POST);
            draft.set_url("https://httpbingo.org/post?x=1&x=2");
            draft.toggle_param(0);
            draft.set_param_draft_key("draft");
            draft.set_param_draft_value("a+b 中文");
            draft.set_header_draft_key("X-Live");
            draft.set_header_draft_value("{{inputs.token}}");
            draft.set_authorization_kind(auth);
            draft.set_bearer_token("secret");
            draft.set_basic_username("中文:user");
            draft.set_basic_password("{{secret}}");
            draft.set_request_options(RequestOptions {
                timeout_ms: timeout,
                redirect_policy: RedirectPolicy::DoNotFollow,
                max_redirect_hops: 7,
            });
            replay(&draft).await;
            draft.set_body_kind(BodyKind::UrlEncoded);
            let mut disabled = KeyValueRow::enabled("disabled", "secret");
            disabled.enabled = false;
            draft.set_url_encoded_rows(vec![
                KeyValueRow::enabled("x", "a+b"),
                KeyValueRow::enabled("x", "中文"),
                disabled,
            ]);
            replay(&draft).await;
        }
    }
}

#[tokio::test]
async fn bodyless_methods_and_inactive_drafts_keep_send_semantics() {
    for method in [
        HttpMethod::GET,
        HttpMethod::HEAD,
        HttpMethod::OPTIONS,
        HttpMethod::PUT,
        HttpMethod::PATCH,
        HttpMethod::DELETE,
    ] {
        let mut draft = RequestDraft::new();
        draft.set_url("https://httpbingo.org/anything");
        draft.set_method(method);
        draft.set_body_kind(BodyKind::Raw);
        draft.set_body("retained body");
        replay(&draft).await;
        draft.set_body_kind(BodyKind::None);
        replay(&draft).await;
    }
}

#[test]
fn unsupported_sources_fail_without_mutation_or_sensitive_diagnostics() {
    let mut draft = RequestDraft::new();
    assert!(convert(&draft, "", "").is_err());
    draft.set_url("https://httpbingo.org/post?token=secret");
    draft.set_method(HttpMethod::POST);
    for (pre, tests) in [("secret()", ""), ("", "secret()")] {
        let error = convert(&draft, pre, tests).unwrap_err().message();
        assert!(!error.contains("secret"));
    }
    draft.set_body_kind(BodyKind::Multipart);
    draft.set_body("x=secret");
    let original = draft.clone();
    assert!(convert(&draft, "", "")
        .unwrap_err()
        .message()
        .contains("Form-data"));
    assert_eq!(draft, original);
    draft.set_binary_file("relative/secret.bin".into(), None);
    assert!(convert(&draft, "", "")
        .unwrap_err()
        .message()
        .contains("absolute Unicode"));
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        draft.set_binary_file(
            std::ffi::OsString::from_vec(b"/tmp/\xff".to_vec()).into(),
            None,
        );
        assert!(convert(&draft, "", "").is_err());
    }
    draft.set_body_kind(BodyKind::Json);
    draft.set_body("invalid secret");
    assert!(!convert(&draft, "", "")
        .unwrap_err()
        .message()
        .contains("secret"));
}

#[tokio::test]
async fn managed_auth_replaces_manual_duplicates_and_disabled_headers_stay_in_http() {
    let request = Request {
        method: HttpMethod::POST,
        url: "https://httpbingo.org/post?x=&x=%2B".into(),
        headers: vec![
            ("Authorization".into(), "manual one".into()),
            ("authorization".into(), "manual two".into()),
            ("Accept".into(), "custom/type".into()),
            ("X-Disabled".into(), "secret".into()),
        ],
        body: RequestBody::Raw(String::new()),
    };
    let mut draft = RequestDraft::from_request(&request);
    draft.toggle_header(3);
    replay(&draft).await;
    draft.set_authorization_kind(AuthorizationKind::Bearer);
    draft.set_bearer_token("literal-secret");
    let constructed = draft.construct();
    assert_eq!(
        constructed
            .request()
            .headers
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case("authorization"))
            .count(),
        1
    );
    assert_eq!(
        constructed.request().headers.last().unwrap().1,
        "Bearer literal-secret"
    );
    replay(&draft).await;
}

#[test]
fn conversion_failures_identify_fields_without_echoing_invalid_values() {
    let mut request = Request::new(HttpMethod::GET, "secret:not-http");
    assert_eq!(
        convert(&RequestDraft::from_request(&request), "", ""),
        Err(ConversionError::Url)
    );
    request.url = "https://httpbingo.org/get".into();
    request
        .headers
        .push(("X-Test".into(), "secret\r\ninjection".into()));
    assert_eq!(
        convert(&RequestDraft::from_request(&request), "", ""),
        Err(ConversionError::Headers)
    );
}

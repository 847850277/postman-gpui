use super::*;

fn completed_entry(method: HttpMethod, body: RequestBody) -> HistoryEntry {
    let mut request = Request::new(method, "https://api.example.com/v1/items?tag=rust&tag=gpui");
    request.headers = vec![("X-Trace".to_string(), "trace-128".to_string())];
    request.body = body;
    let mut entry = HistoryEntry::completed_with_intent_and_options(
        request,
        "Issue 128 replay".to_string(),
        201,
        42,
        128,
        None,
        RequestOptions {
            timeout_ms: Some(1_250),
            redirect_policy: RedirectPolicy::DoNotFollow,
            max_redirect_hops: 3,
        },
    );
    entry.id = "00000000-0000-4000-8000-000000000128".to_string();
    entry.timestamp = DateTime::parse_from_rfc3339("2026-08-24T12:34:56.789Z")
        .unwrap()
        .with_timezone(&Utc);
    entry
}

fn round_trip(entry: &HistoryEntry) -> HistoryEntry {
    let versioned = VersionedHistorySnapshot::try_from(entry).unwrap();
    assert_eq!(versioned.version(), HISTORY_SNAPSHOT_VERSION_V2);
    let bytes = versioned.to_json_bytes().unwrap();
    let decoded = VersionedHistorySnapshot::from_json_bytes(&bytes).unwrap();
    HistoryEntry::try_from(decoded).unwrap()
}

#[test]
fn every_supported_method_round_trips() {
    for method in HttpMethod::all() {
        let entry = completed_entry(method, RequestBody::None);
        let restored = round_trip(&entry);
        assert_eq!(restored.id, entry.id);
        assert_eq!(restored.request.method, method);
        assert_eq!(restored.request.url, entry.request.url);
        assert_eq!(restored.request.headers, entry.request.headers);
        assert_eq!(restored.request_options, entry.request_options);
        assert_eq!(restored.timestamp, entry.timestamp);
        assert_eq!(restored.status, entry.status);
        assert_eq!(restored.elapsed_ms, entry.elapsed_ms);
        assert_eq!(restored.response_size, entry.response_size);
    }
}

#[test]
fn url_without_query_does_not_gain_a_trailing_question_mark_on_replay() {
    let mut entry = completed_entry(HttpMethod::GET, RequestBody::None);
    entry.request.url = "https://api.example.com/v1/items".to_string();

    let restored = round_trip(&entry);

    assert_eq!(restored.request.url, entry.request.url);
}

#[test]
fn every_supported_body_kind_round_trips() {
    let bodies = vec![
        RequestBody::None,
        RequestBody::Json(r#"{"message":"hello"}"#.to_string()),
        RequestBody::Raw("plain text".to_string()),
        RequestBody::UrlEncoded("name=Ada+Lovelace&tag=rust&tag=gpui".to_string()),
        RequestBody::Multipart(vec![
            MultipartPart::text("note", "hello"),
            MultipartPart {
                name: "upload".to_string(),
                value: MultipartValue::File {
                    path: PathBuf::from("/tmp/issue-128-upload.txt"),
                    file_name: Some("upload.txt".to_string()),
                    content_type: Some("text/plain".to_string()),
                },
            },
        ]),
    ];

    for body in bodies {
        let entry = completed_entry(HttpMethod::POST, body.clone());
        let restored = round_trip(&entry);
        assert_eq!(restored.request.body, body);
    }
}

#[test]
fn multipart_editor_intent_preserves_real_disabled_rows_but_drops_placeholders() {
    let mut entry = completed_entry(
        HttpMethod::POST,
        RequestBody::Multipart(vec![MultipartPart::text("active", "sent")]),
    );
    entry.editor_intent = Some(RequestEditorIntent::Multipart(vec![
        MultipartEditorPart {
            enabled: true,
            name: String::new(),
            value: MultipartValue::Text(String::new()),
        },
        MultipartEditorPart {
            enabled: false,
            name: "disabled".to_string(),
            value: MultipartValue::Text("draft-only".to_string()),
        },
        MultipartEditorPart {
            enabled: true,
            name: "active".to_string(),
            value: MultipartValue::Text("sent".to_string()),
        },
        MultipartEditorPart {
            enabled: true,
            name: "unfinished-file".to_string(),
            value: MultipartValue::File {
                path: PathBuf::new(),
                file_name: None,
                content_type: None,
            },
        },
    ]));

    let restored = round_trip(&entry);
    assert_eq!(
        restored.request.body,
        RequestBody::Multipart(vec![MultipartPart::text("active", "sent")])
    );
    let Some(RequestEditorIntent::Multipart(parts)) = restored.editor_intent else {
        panic!("multipart editor intent should round-trip");
    };
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[0].name, "disabled");
    assert!(!parts[0].enabled);
    assert_eq!(parts[1].name, "active");
    assert!(parts[1].enabled);
    assert_eq!(parts[2].name, "unfinished-file");
}

#[test]
fn credentials_are_absent_from_serialized_payload_case_insensitively() {
    let original_url = "https://url-user:url-password@api.example.com/v1/items?tag=rust&API_Key=query-secret&access_token=access-secret&X-Amz-Signature=signature-secret";
    let mut request = Request::new(HttpMethod::POST, original_url);
    request.headers = vec![
        (
            "aUtHoRiZaTiOn".to_string(),
            "Bearer bearer-secret".to_string(),
        ),
        (
            "PrOxY-AuThOrIzAtIoN".to_string(),
            "Basic proxy-secret".to_string(),
        ),
        ("COOKIE".to_string(), "session=cookie-secret".to_string()),
        (
            "sEt-CoOkIe".to_string(),
            "response-cookie=secret".to_string(),
        ),
        ("X-API-Key".to_string(), "header-api-secret".to_string()),
        (
            "X-Auth-Token".to_string(),
            "header-token-secret".to_string(),
        ),
        ("X-Trace".to_string(), "safe-trace".to_string()),
    ];
    request.body = RequestBody::Json(
        r#"{"password":"body-secret-is-user-authored","message":"persist me"}"#.to_string(),
    );
    let mut entry = HistoryEntry::completed(request, String::new(), 200, 5, 64);
    entry.name = "Replay query-secret with header-api-secret".to_string();
    let snapshot = VersionedHistorySnapshot::try_from(&entry).unwrap();
    let serialized = snapshot.to_json_string().unwrap();

    for denied in [
        "url-user",
        "url-password",
        "query-secret",
        "access-secret",
        "signature-secret",
        "Bearer bearer-secret",
        "Basic proxy-secret",
        "session=cookie-secret",
        "response-cookie=secret",
        "header-api-secret",
        "header-token-secret",
    ] {
        assert!(
            !serialized.contains(denied),
            "serialized snapshot leaked {denied}"
        );
    }
    assert!(serialized.contains("safe-trace"));
    assert!(serialized.contains("body-secret-is-user-authored"));
    assert!(snapshot.as_v1().name().contains("[REDACTED]"));
    assert_eq!(snapshot.as_v1().request().query().len(), 1);
    assert_eq!(snapshot.as_v1().request().query()[0].name(), "tag");
    assert_eq!(snapshot.as_v1().request().headers().len(), 1);
    assert_eq!(snapshot.as_v1().request().headers()[0].name(), "X-Trace");

    let restored = HistoryEntry::try_from(snapshot).unwrap();
    assert_eq!(
        restored.request.url,
        "https://api.example.com/v1/items?tag=rust"
    );
    assert_eq!(
        restored.request.headers,
        vec![("X-Trace".to_string(), "safe-trace".to_string())]
    );
}

#[test]
fn decoded_v1_payload_is_sanitized_again_before_use() {
    let entry = completed_entry(HttpMethod::GET, RequestBody::None);
    let snapshot = VersionedHistorySnapshot::V1(HistorySnapshotV1::try_from(&entry).unwrap());
    let mut value: Value = serde_json::from_slice(&snapshot.to_json_bytes().unwrap()).unwrap();
    value["snapshot"]["request"]["headers"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "name": "Authorization",
            "value": "decoded-header-secret"
        }));
    value["snapshot"]["request"]["query"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "name": "token",
            "value": "decoded-query-secret"
        }));
    value["snapshot"]["name"] =
        Value::String("decoded-header-secret decoded-query-secret".to_string());

    let decoded = VersionedHistorySnapshot::from_json_bytes(
        &serde_json::to_vec(&value).expect("mutated fixture should serialize"),
    )
    .unwrap();
    let serialized = decoded.to_json_string().unwrap();
    assert!(!serialized.contains("decoded-header-secret"));
    assert!(!serialized.contains("decoded-query-secret"));
    assert_eq!(decoded.as_v1().request().headers().len(), 1);
    assert_eq!(decoded.as_v1().request().headers()[0].name(), "X-Trace");
    assert_eq!(decoded.as_v1().request().query().len(), 2);
}

#[test]
fn unknown_version_is_typed_before_payload_decoding() {
    let error = VersionedHistorySnapshot::from_json_str(r#"{"version":99}"#).unwrap_err();
    assert_eq!(
        error,
        HistorySnapshotError::UnsupportedVersion { found: 99 }
    );
}

#[test]
fn malformed_and_missing_fields_return_typed_errors() {
    let error = VersionedHistorySnapshot::from_json_str("not-json").unwrap_err();
    assert!(matches!(
        error,
        HistorySnapshotError::MalformedPayload { .. }
    ));

    let entry = completed_entry(HttpMethod::POST, RequestBody::Raw("hello".to_string()));
    let snapshot = VersionedHistorySnapshot::try_from(&entry).unwrap();
    let mut value: Value = serde_json::from_slice(&snapshot.to_json_bytes().unwrap()).unwrap();
    value["snapshot"]
        .as_object_mut()
        .unwrap()
        .remove("entry_id");
    let error = VersionedHistorySnapshot::from_json_bytes(&serde_json::to_vec(&value).unwrap())
        .unwrap_err();
    assert!(matches!(
        error,
        HistorySnapshotError::MalformedPayload { .. }
    ));
}

#[test]
fn malformed_enum_variant_returns_a_typed_error() {
    let entry = completed_entry(HttpMethod::POST, RequestBody::Raw("hello".to_string()));
    let snapshot = VersionedHistorySnapshot::try_from(&entry).unwrap();
    let mut value: Value = serde_json::from_slice(&snapshot.to_json_bytes().unwrap()).unwrap();
    value["snapshot"]["request"]["body"]["kind"] = Value::String("future_body_kind".to_string());
    let error = VersionedHistorySnapshot::from_json_bytes(&serde_json::to_vec(&value).unwrap())
        .unwrap_err();
    assert!(matches!(
        error,
        HistorySnapshotError::MalformedPayload { .. }
    ));
}

#[test]
fn incomplete_and_out_of_range_runtime_entries_are_rejected() {
    let request = Request::new(HttpMethod::GET, "https://api.example.com/");
    let incomplete = HistoryEntry::new(request, "not completed".to_string());
    assert_eq!(
        VersionedHistorySnapshot::try_from(&incomplete).unwrap_err(),
        HistorySnapshotError::IncompleteHistoryEntry { field: "status" }
    );

    let mut overflow = completed_entry(HttpMethod::GET, RequestBody::None);
    overflow.elapsed_ms = Some(u128::from(u64::MAX) + 1);
    assert_eq!(
        VersionedHistorySnapshot::try_from(&overflow).unwrap_err(),
        HistorySnapshotError::NumericOverflow {
            field: "elapsed_ms"
        }
    );

    let mut invalid_hops = completed_entry(HttpMethod::GET, RequestBody::None);
    invalid_hops.request_options.max_redirect_hops = 0;
    assert_eq!(
        VersionedHistorySnapshot::try_from(&invalid_hops).unwrap_err(),
        HistorySnapshotError::InvalidField {
            field: "request.options.max_hops",
            reason: "must be between 1 and 100",
        }
    );

    let mut zero_timeout = completed_entry(HttpMethod::GET, RequestBody::None);
    zero_timeout.request_options.timeout_ms = Some(0);
    assert_eq!(
        VersionedHistorySnapshot::try_from(&zero_timeout).unwrap_err(),
        HistorySnapshotError::InvalidField {
            field: "request.options.timeout_ms",
            reason: "zero must be represented as null",
        }
    );
}

#[test]
fn missing_multipart_file_has_an_explicit_replay_error_without_embedded_bytes() {
    let path = std::env::temp_dir().join(format!(
        "postman-gpui-issue-128-missing-{}.bin",
        uuid::Uuid::new_v4()
    ));
    assert!(!path.exists());
    let body = RequestBody::Multipart(vec![MultipartPart::file("upload", &path)]);
    let entry = completed_entry(HttpMethod::POST, body);
    let snapshot = VersionedHistorySnapshot::try_from(&entry).unwrap();
    let serialized = snapshot.to_json_string().unwrap();
    assert!(serialized.contains(path.to_str().unwrap()));
    assert!(!serialized.contains("file_bytes"));
    assert_eq!(
        snapshot.validate_replay_files().unwrap_err(),
        HistorySnapshotError::MissingMultipartFile { path }
    );
}

#[test]
fn common_sensitive_name_policy_is_case_and_separator_insensitive() {
    for name in [
        "Authorization",
        "proxy_authorization",
        "COOKIE",
        "Set-Cookie",
        "X_API_KEY",
        "x-auth-token",
    ] {
        assert!(HistorySensitiveDataPolicy::is_sensitive_header_name(name));
    }
    for name in [
        "api_key",
        "Access-Token",
        "client_secret",
        "X-Amz-Credential",
        "x_amz_signature",
    ] {
        assert!(HistorySensitiveDataPolicy::is_sensitive_query_name(name));
    }
    assert!(!HistorySensitiveDataPolicy::is_sensitive_header_name(
        "Content-Type"
    ));
    assert!(!HistorySensitiveDataPolicy::is_sensitive_query_name("tag"));
}

#[test]
fn v1_options_include_timeout_and_redirect_behavior() {
    let entry = completed_entry(HttpMethod::GET, RequestBody::None);
    let snapshot = VersionedHistorySnapshot::V1(HistorySnapshotV1::try_from(&entry).unwrap());
    let options = snapshot.as_v1().request().options();
    assert_eq!(options.timeout_ms(), Some(1_250));
    assert_eq!(
        options.redirect_policy(),
        RedirectPolicySnapshotV1::DoNotFollow
    );
    assert_eq!(options.max_hops(), 3);
}

#[test]
fn current_defaults_are_stable_in_v1() {
    let mut entry = completed_entry(HttpMethod::GET, RequestBody::None);
    entry.request_options = RequestOptions::default();
    let snapshot = VersionedHistorySnapshot::V1(HistorySnapshotV1::try_from(&entry).unwrap());
    let options = snapshot.as_v1().request().options();
    assert_eq!(options.timeout_ms(), None);
    assert_eq!(options.redirect_policy(), RedirectPolicySnapshotV1::Follow);
    assert_eq!(options.max_hops(), crate::models::DEFAULT_MAX_REDIRECT_HOPS);
}

fn entry_with_response(
    status: u16,
    headers: Vec<(String, String)>,
    body: impl Into<String>,
) -> HistoryEntry {
    let body = body.into();
    let mut entry = completed_entry(HttpMethod::GET, RequestBody::None);
    entry.status = Some(status);
    entry.elapsed_ms = Some(37);
    entry.response_size = Some(body.len());
    entry.historical_response = Some(HistoricalResponse::completed(status, headers, body, 37));
    entry
}

#[test]
fn v2_json_response_round_trip_sanitizes_headers_and_structured_secret_fields() {
    let entry = entry_with_response(
        418,
        vec![
            ("Content-Type".to_string(), "application/json".to_string()),
            ("X-Trace".to_string(), "trace-safe".to_string()),
            ("Set-Cookie".to_string(), "sid=cookie-secret".to_string()),
            (
                "Authorization".to_string(),
                "Bearer auth-secret".to_string(),
            ),
            ("X-API-Key".to_string(), "api-key-secret".to_string()),
            ("X-Session-Id".to_string(), "session-secret".to_string()),
        ],
        r#"{"message":"safe","password":"json-password","nested":{"access_token":"json-token"}}"#,
    );

    let snapshot = VersionedHistorySnapshot::try_from(&entry).unwrap();
    assert_eq!(snapshot.version(), HISTORY_SNAPSHOT_VERSION_V2);
    let response = snapshot.as_v2().unwrap().response().unwrap();
    assert_eq!(response.status(), 418);
    assert_eq!(response.headers().len(), 2);
    assert!(response
        .headers()
        .iter()
        .any(|header| header.name() == "X-Trace"));
    let serialized = snapshot.to_json_string().unwrap();
    for denied in [
        "cookie-secret",
        "auth-secret",
        "api-key-secret",
        "session-secret",
        "json-password",
        "json-token",
    ] {
        assert!(!serialized.contains(denied), "V2 leaked {denied}");
    }
    assert!(serialized.contains("trace-safe"));
    assert!(serialized.contains("safe"));
    assert!(serialized.contains("[REDACTED]"));

    let restored = HistoryEntry::try_from(snapshot).unwrap();
    let response = restored.historical_response.unwrap();
    assert_eq!(response.status, 418);
    assert_eq!(response.original_size, entry.response_size.unwrap());
    assert!(matches!(response.body, HistoricalResponseBody::Text(_)));
    assert!(response
        .headers
        .iter()
        .all(|(name, _)| !HistorySensitiveDataPolicy::is_sensitive_header_name(name)));
}

#[test]
fn v2_form_fields_are_redacted_but_arbitrary_plain_text_is_stored_verbatim() {
    let form = entry_with_response(
        200,
        vec![(
            "content-type".to_string(),
            "application/x-www-form-urlencoded".to_string(),
        )],
        "name=Ada&password=form-secret&session_id=session-secret",
    );
    let form_json = VersionedHistorySnapshot::try_from(&form)
        .unwrap()
        .to_json_string()
        .unwrap();
    assert!(form_json.contains("name=Ada"));
    assert!(!form_json.contains("form-secret"));
    assert!(!form_json.contains("session-secret"));

    // Raw/plain response text has no reliable field boundary. V2's defined policy bounds it but
    // does not guess which arbitrary substrings are credentials.
    let plain = entry_with_response(
        200,
        vec![("Content-Type".to_string(), "text/plain".to_string())],
        "opaque plain payload: arbitrary-secret-value",
    );
    let plain_json = VersionedHistorySnapshot::try_from(&plain)
        .unwrap()
        .to_json_string()
        .unwrap();
    assert!(plain_json.contains("arbitrary-secret-value"));
}

#[test]
fn v2_preview_truncates_on_a_utf8_boundary_and_keeps_both_sizes() {
    let body = "好".repeat(MAX_HISTORICAL_RESPONSE_PREVIEW_BYTES / 3 + 10);
    let entry = entry_with_response(
        200,
        vec![("Content-Type".to_string(), "text/plain".to_string())],
        body.clone(),
    );

    let snapshot = VersionedHistorySnapshot::try_from(&entry).unwrap();
    let response = snapshot.as_v2().unwrap().response().unwrap();
    let preview = response.body().preview().unwrap();
    assert!(response.truncated());
    assert!(preview.len() <= MAX_HISTORICAL_RESPONSE_PREVIEW_BYTES);
    assert!(preview.is_char_boundary(preview.len()));
    assert_eq!(response.original_size(), body.len() as u64);
    assert_eq!(response.persisted_size(), preview.len() as u64);
    assert!(matches!(
        response.body(),
        HistoricalResponseBodySnapshotV2::TruncatedText(_)
    ));
    let preview_len = preview.len();

    let restored = HistoryEntry::try_from(snapshot).unwrap();
    let response = restored.historical_response.unwrap();
    assert_eq!(response.original_size, body.len());
    assert_eq!(response.persisted_size, preview_len);
    assert!(matches!(
        response.body,
        HistoricalResponseBody::TruncatedText(_)
    ));
}

#[test]
fn v2_binary_download_and_stream_bodies_store_metadata_but_no_body_bytes() {
    for (content_type, extra_headers) in [
        ("application/octet-stream", Vec::new()),
        (
            "text/plain",
            vec![(
                "Content-Disposition".to_string(),
                "attachment; filename=report.txt".to_string(),
            )],
        ),
        ("text/event-stream", Vec::new()),
    ] {
        let mut headers = vec![("Content-Type".to_string(), content_type.to_string())];
        headers.extend(extra_headers);
        let entry = entry_with_response(200, headers, "body-bytes-must-not-be-stored");
        let snapshot = VersionedHistorySnapshot::try_from(&entry).unwrap();
        let serialized = snapshot.to_json_string().unwrap();
        assert!(!serialized.contains("body-bytes-must-not-be-stored"));
        let response = snapshot.as_v2().unwrap().response().unwrap();
        assert_eq!(response.persisted_size(), 0);
        assert!(matches!(
            response.body(),
            HistoricalResponseBodySnapshotV2::Unsupported
        ));
    }
}

#[test]
fn v2_empty_204_is_valid_and_v1_remains_response_unavailable() {
    let empty = entry_with_response(204, Vec::new(), String::new());
    let restored =
        HistoryEntry::try_from(VersionedHistorySnapshot::try_from(&empty).unwrap()).unwrap();
    assert!(matches!(
        restored.historical_response.unwrap().body,
        HistoricalResponseBody::Empty
    ));

    let entry = completed_entry(HttpMethod::GET, RequestBody::None);
    let v1 = VersionedHistorySnapshot::V1(HistorySnapshotV1::try_from(&entry).unwrap());
    let decoded = VersionedHistorySnapshot::from_json_bytes(&v1.to_json_bytes().unwrap()).unwrap();
    assert_eq!(decoded.version(), HISTORY_SNAPSHOT_VERSION_V1);
    assert!(HistoryEntry::try_from(decoded)
        .unwrap()
        .historical_response
        .is_none());
}

#[test]
fn decoded_v2_response_is_sanitized_again_before_runtime_use() {
    let entry = entry_with_response(
        200,
        vec![("Content-Type".to_string(), "application/json".to_string())],
        r#"{"message":"safe"}"#,
    );
    let snapshot = VersionedHistorySnapshot::try_from(&entry).unwrap();
    let mut value: Value = serde_json::from_slice(&snapshot.to_json_bytes().unwrap()).unwrap();
    value["snapshot"]["response"]["headers"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "name": "Set-Cookie",
            "value": "sid=decoded-cookie-secret"
        }));
    value["snapshot"]["response"]["body"]["preview"] =
        Value::String(r#"{"password":"decoded-json-secret"}"#.to_string());
    value["snapshot"]["response"]["persisted_size"] =
        Value::from(r#"{"password":"decoded-json-secret"}"#.len() as u64);

    let decoded =
        VersionedHistorySnapshot::from_json_bytes(&serde_json::to_vec(&value).unwrap()).unwrap();
    let serialized = decoded.to_json_string().unwrap();
    assert!(!serialized.contains("decoded-cookie-secret"));
    assert!(!serialized.contains("decoded-json-secret"));
    assert!(serialized.contains("[REDACTED]"));
}

#[test]
fn decoded_v2_uses_the_sanitized_content_type_header_as_body_classification_truth() {
    let entry = entry_with_response(
        200,
        vec![("Content-Type".to_string(), "text/plain".to_string())],
        "untrusted-stored-body",
    );
    let snapshot = VersionedHistorySnapshot::try_from(&entry).unwrap();
    let mut value: Value = serde_json::from_slice(&snapshot.to_json_bytes().unwrap()).unwrap();
    value["snapshot"]["response"]["headers"][0]["value"] =
        Value::String("application/octet-stream".to_string());
    value["snapshot"]["response"]["media_type"] = Value::String("text/plain".to_string());

    let decoded =
        VersionedHistorySnapshot::from_json_bytes(&serde_json::to_vec(&value).unwrap()).unwrap();
    let serialized = decoded.to_json_string().unwrap();
    assert!(!serialized.contains("untrusted-stored-body"));
    assert!(matches!(
        decoded.as_v2().unwrap().response().unwrap().body(),
        HistoricalResponseBodySnapshotV2::Unsupported
    ));
}

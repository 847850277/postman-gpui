use super::*;
use crate::models::{HttpMethod, MultipartPart, MultipartValue, RedirectPolicy, RequestBody};
fn effective(command: &str) -> Request {
    parse(command)
        .unwrap()
        .draft()
        .construct()
        .request()
        .clone()
}
#[test]
fn minimal_and_options() {
    let parsed = parse("/usr/bin/curl.exe -sSg --follow --post301 --post302 -XPATCH --url=https://example.com -H'X-Test: yes' --data-raw=value --max-time=1.025 --max-redirs 3").unwrap();
    let c = parsed.draft().construct();
    assert_eq!(c.request().method, HttpMethod::PATCH);
    assert_eq!(
        c.request_options(),
        RequestOptions {
            timeout_ms: Some(1025),
            redirect_policy: RedirectPolicy::Follow,
            max_redirect_hops: 3
        }
    );
    let plain = parse("curl -- https://example.com")
        .unwrap()
        .draft()
        .construct();
    assert_eq!(
        plain.request().headers,
        vec![("Accept".into(), "*/*".into())]
    );
    assert_eq!(plain.request().body, RequestBody::None);
    assert_eq!(
        plain.request_options().redirect_policy,
        RedirectPolicy::DoNotFollow
    );
}
#[test]
fn exact_shell_literals_and_continuations() {
    let body = "O'Connor\n用户 $HOME `literal` \\";
    let command = format!(
        "curl https://example.com \\\r\n -XPATCH --data-raw {}",
        shell::quote(body).unwrap()
    );
    assert_eq!(effective(&command).body.as_text(), Some(body));
    assert_eq!(
        effective("curl https://example.com --data-raw \"a\\$b\\`c\\\\d\"")
            .body
            .as_text(),
        Some("a$b`c\\d")
    );
    assert_eq!(
        effective("curl https://example.com --data-raw '@literal'")
            .body
            .as_text(),
        Some("@literal")
    );
    assert_eq!(
        effective("curl https://example.com -d tail\\ ")
            .body
            .as_text(),
        Some("tail ")
    );
    assert_eq!(
        effective("curl https://example.com -d a\u{a0}b\n")
            .body
            .as_text(),
        Some("a\u{a0}b")
    );
    assert_eq!(
        effective("curl https://example.com --data-binary 'a\nb'")
            .body
            .as_text(),
        Some("a\nb")
    );
}
#[test]
fn forms_json_queries_and_empty() {
    let r = effective("curl -G 'https://example.com/find?x=%2f&x=2#section' --data-urlencode 'name=Maya Chen' --data-urlencode 'tag=a&b'");
    assert_eq!(
        r.url,
        "https://example.com/find?x=%2f&x=2&name=Maya%20Chen&tag=a%26b#section"
    );
    assert!(r.body.is_none());
    let get_json = effective("curl --get --json '{}' https://example.com");
    assert_eq!(get_json.url, "https://example.com?{}");
    assert!(get_json.body.is_none());
    assert!(get_json
        .headers
        .contains(&("Content-Type".into(), "application/json".into())));
    assert!(get_json
        .headers
        .contains(&("Accept".into(), "application/json".into())));
    assert_eq!(
        effective("curl https://example.com -d a=1 -d b=2").body,
        RequestBody::UrlEncoded("a=1&b=2".into())
    );
    assert_eq!(
        effective("curl https://example.com --json '{\"a\":' --json '1}'").body,
        RequestBody::Json("{\"a\":1}".into())
    );
    assert_eq!(
        effective("curl https://example.com --data-raw ''").body,
        RequestBody::Raw("".into())
    );
    assert_eq!(
        effective("curl -I https://example.com").method,
        HttpMethod::HEAD
    );
}
#[test]
fn explicit_methods_keep_inline_bodies_through_import_and_export() {
    for method in [
        HttpMethod::DELETE,
        HttpMethod::GET,
        HttpMethod::HEAD,
        HttpMethod::OPTIONS,
    ] {
        for (option, body) in [
            (
                r#"--json '{"ids":[1,2]}'"#,
                RequestBody::Json(r#"{"ids":[1,2]}"#.into()),
            ),
            ("--data-raw ''", RequestBody::Raw(String::new())),
        ] {
            let command = format!("curl -X {method} {option} https://example.test/items");
            let imported = parse(&command).unwrap();
            let request = imported.draft().construct();
            assert_eq!(request.request().method, method);
            assert_eq!(request.request().body, body);
            assert_eq!(request.request().url, "https://example.test/items");
            let code = export(imported.draft()).unwrap();
            assert_eq!(effective(code.text()), *request.request());

            let mut draft = imported.into_draft();
            draft.set_request_options(RequestOptions::default());
            let code = export(&draft).unwrap();
            assert!(code.text().contains("--post301 --post302"));
            assert_eq!(
                code.text().contains("--post303"),
                method == HttpMethod::HEAD
            );
            assert_eq!(
                parse(code.text()).unwrap().draft().construct(),
                draft.construct()
            );
        }
    }
}

#[test]
fn redirect_flags_must_preserve_the_body_on_each_status() {
    for flags in [
        "-L -XGET",
        "-L -XGET --post301",
        "-L -XGET --post302",
        "--location -XGET --post303",
        "-L -XGET --post301 --post302 --post303",
        "-L -XHEAD",
        "-L -XHEAD --post301",
        "-L -XHEAD --post302",
        "-L -XHEAD --post301 --post302",
        "-L -XHEAD --post301 --post303",
        "-L -XHEAD --post302 --post303",
        "--follow -XDELETE --post301",
        "--follow -XDELETE --post302",
        "--follow -XDELETE --post301 --post302 --post303",
        "--follow -L -XDELETE --post301 --post302",
        "--follow --post302",
    ] {
        for data in ["payload", "''"] {
            let command = format!("curl {flags} -d {data} https://example.test/items");
            assert!(parse(&command).is_err(), "{command}");
        }
    }
    let query = effective("curl -L -G -d id=42 https://example.test/items");
    assert_eq!(query.url, "https://example.test/items?id=42");
    assert!(query.body.is_none());
}

#[test]
fn bodyless_custom_methods_cannot_promise_native_redirects() {
    for method in [
        HttpMethod::POST,
        HttpMethod::PUT,
        HttpMethod::PATCH,
        HttpMethod::DELETE,
        HttpMethod::OPTIONS,
    ] {
        for query in ["", "--get --data-raw 'id=42'"] {
            let command = format!("curl --follow -X {method} {query} https://example.test/items");
            assert!(parse(&command)
                .unwrap_err()
                .to_string()
                .contains("without a body"));
        }
        let mut draft = RequestDraft::from_request(&Request::new(method, "https://example.test"));
        assert!(export(&draft)
            .unwrap_err()
            .to_string()
            .contains("without a body"));
        draft.set_request_options(RequestOptions {
            redirect_policy: RedirectPolicy::DoNotFollow,
            ..RequestOptions::default()
        });
        let code = export(&draft).unwrap();
        assert!(parse(code.text())
            .unwrap()
            .draft()
            .construct()
            .request()
            .body
            .is_none());
        draft.set_request_options(RequestOptions::default());
        draft.set_body_kind(crate::models::BodyKind::Raw);
        draft.set_body("");
        let code = export(&draft).unwrap();
        assert_eq!(
            parse(code.text()).unwrap().draft().construct(),
            draft.construct()
        );
    }
    for method in [HttpMethod::GET, HttpMethod::HEAD] {
        let draft = RequestDraft::from_request(&Request::new(method, "https://example.test"));
        assert!(export(&draft).is_ok());
        assert!(parse(&format!("curl --follow -X {method} https://example.test")).is_ok());
    }
}

/// Run explicitly with POSTMAN_TEST_CURL pointing to cURL 8.16+ with compression.
#[tokio::test]
#[ignore = "requires an explicit cURL 8.16+ executable; see docs/release-smoke-test.md"]
async fn exported_commands_match_native_redirects_with_real_curl() {
    use postman_http::HttpTransport;
    use postman_request::RequestClient;

    let binary = std::env::var_os("POSTMAN_TEST_CURL").expect("set POSTMAN_TEST_CURL");
    let version = std::process::Command::new(&binary)
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    println!("{}", String::from_utf8_lossy(&version.stdout));
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("upload file.bin");
    let bytes = b"file\x00\xff\r\n";
    std::fs::write(&file, bytes).unwrap();
    let empty_file = directory.path().join("empty file.bin");
    std::fs::write(&empty_file, []).unwrap();
    let client = RequestClient::try_new("curl-release-smoke").unwrap();
    let mut checked = 0;
    for method in [
        HttpMethod::GET,
        HttpMethod::HEAD,
        HttpMethod::POST,
        HttpMethod::PUT,
        HttpMethod::PATCH,
        HttpMethod::DELETE,
        HttpMethod::OPTIONS,
    ] {
        for body in [
            RequestBody::None,
            RequestBody::Raw(String::new()),
            RequestBody::Raw("body 中文\n".into()),
            RequestBody::File(file.clone()),
            RequestBody::File(empty_file.clone()),
        ] {
            if body.is_none() && !matches!(method, HttpMethod::GET | HttpMethod::HEAD) {
                continue; // Rejected at conversion time, covered by the non-ignored test.
            }
            let sent = match &body {
                RequestBody::Raw(value) => value.as_bytes().to_vec(),
                RequestBody::File(path) => std::fs::read(path).unwrap(),
                _ => Vec::new(),
            };
            for status in [301, 302, 303, 307, 308] {
                let mut server = mockito::Server::new_async().await;
                let first = server
                    .mock(&method.to_string(), "/start")
                    .match_body(sent.clone())
                    .with_status(status)
                    .with_header("location", "/end")
                    .with_header("content-length", "0")
                    .expect(2)
                    .create_async()
                    .await;
                let drops_body = (status == 303 && method != HttpMethod::HEAD)
                    || ([301, 302].contains(&status) && method == HttpMethod::POST);
                let next_method = if drops_body { HttpMethod::GET } else { method };
                let next_body = if drops_body { &[][..] } else { sent.as_slice() };
                let last = server
                    .mock(&next_method.to_string(), "/end")
                    .match_body(next_body.to_vec())
                    .with_status(200)
                    .with_header("content-length", "0")
                    .expect(2)
                    .create_async()
                    .await;
                let mut request = Request::new(method, format!("{}/start", server.url()));
                request.body = body.clone();
                let mut draft = RequestDraft::from_request(&request);
                draft.set_request_options(RequestOptions {
                    timeout_ms: Some(5_000),
                    ..RequestOptions::default()
                });
                let effective = draft.construct();
                let native = client
                    .execute(effective.request().clone(), effective.request_options())
                    .await
                    .unwrap();
                assert_eq!(native.status, 200);
                let code = export(&draft).unwrap();
                let words = shell::words(code.text()).unwrap();
                // Execute argv directly: no shell, user configuration, or ambient proxy.
                let result = std::process::Command::new(&binary)
                    .args(["-q", "--silent", "--show-error", "--noproxy", "*"])
                    .args(&words[1..])
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{code:?}: {}",
                    String::from_utf8_lossy(&result.stderr)
                );
                first.assert_async().await;
                last.assert_async().await;
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 150);
    println!(
        "Compared {checked} exported cURL/native redirect cases, including empty and binary files."
    );
}

#[tokio::test]
async fn imported_redirects_send_the_expected_method_and_bytes_on_both_hops() {
    use postman_http::HttpTransport;
    use postman_request::RequestClient;

    let client = RequestClient::try_new("curl-import-test").unwrap();
    for (method, flags) in [
        ("GET", "-L --post301 --post302"),
        ("HEAD", "-L --post301 --post302 --post303"),
        ("DELETE", "-L --follow --post301 --post302"),
    ] {
        for status in [301, 302, 303] {
            let mut server = mockito::Server::new_async().await;
            let first = server
                .mock(method, "/start")
                .match_body("payload")
                .with_status(status)
                .with_header("location", "/end")
                .create_async()
                .await;
            let (next_method, next_body) = if status == 303 && method != "HEAD" {
                ("GET", "")
            } else {
                (method, "payload")
            };
            let last = server
                .mock(next_method, "/end")
                .match_body(next_body)
                .with_status(204)
                .create_async()
                .await;
            let command = format!("curl {flags} -X {method} -d payload {}/start", server.url());
            let request = parse(&command).unwrap().draft().construct();
            let response = client
                .execute(request.request().clone(), request.request_options())
                .await
                .unwrap();
            assert_eq!(response.status, 204, "{command} with {status}");
            first.assert_async().await;
            last.assert_async().await;
        }
    }
}

#[test]
fn duplicate_auth_and_explicit_precedence() {
    let r = effective("curl https://example.com -u user:password -H 'Authorization: Bearer a' -H 'Authorization: Bearer b' -H 'X-Tag: one' -H 'X-Tag: two'");
    assert_eq!(r.headers.len(), 5);
    assert_eq!(r.headers[0].1, "Bearer a");
    assert_eq!(r.headers[1].1, "Bearer b");
    for auth in [
        "bearer lower",
        "Bearer  spaces",
        "Basic Og==",
        "Custom value",
    ] {
        let command = format!(
            "curl https://example.com -H {}",
            shell::quote(&format!("Authorization: {auth}")).unwrap()
        );
        assert_eq!(effective(&command).headers[0].1, auth);
    }
    assert_eq!(
        effective("curl https://example.com -u '用户:päss'")
            .headers
            .iter()
            .find(|(n, _)| n == "Authorization")
            .unwrap()
            .1,
        "Basic 55So5oi3OnDDpHNz"
    );
}
#[test]
fn suppression_and_round_trip_preserve_effective_bytes_and_options() {
    for command in [
        "curl https://example.com",
        "curl -I https://example.com",
        "curl -XOPTIONS https://example.com",
        "curl -XDELETE https://example.com",
        "curl -L https://example.com --max-time 1.250 --max-redirs 7",
        "curl https://example.com -H 'Content-Type:' -H 'Accept:' --data-raw '@literal'",
        "curl https://example.com --json '{\"name\":\"用户\"}'",
        "curl https://example.com --data-urlencode 'hello world'",
        "curl https://example.com --data-raw ''",
        "curl https://example.com -d 'x=%2f&x=a+b'",
        "curl -G 'https://example.com/?x=%2f#f' --data-urlencode 'x=two words'",
        "curl https://example.com --oauth2-bearer token",
        "curl https://example.com -u 'name:p:a:ss'",
    ] {
        let initial = parse(command).unwrap();
        let exported = export(initial.draft()).unwrap();
        let again = parse(exported.text()).unwrap();
        let a = initial.draft().construct();
        let b = again.draft().construct();
        assert_eq!(a.request().method, b.request().method, "{command}");
        assert_eq!(a.request().url, b.request().url, "{command}");
        assert_eq!(a.request().headers, b.request().headers, "{command}");
        assert_eq!(
            a.request().body.as_text(),
            b.request().body.as_text(),
            "{command}"
        );
        assert_eq!(
            a.request().body.is_none(),
            b.request().body.is_none(),
            "{command}"
        );
        assert_eq!(a.request_options(), b.request_options(), "{command}");
    }
}
#[test]
fn rejects_unsafe_unsupported_or_lossy_input() {
    for command in [
        "",
        "wget https://example.com",
        "curl",
        "curl example.com",
        "curl https:example.com",
        "curl https://example.com\n--data-raw value",
        "curl https://example.com --compressed -H 'Range: bytes=0-10'",
        "curl ftp://example.com",
        "curl https://a.com https://b.com",
        "curl 'https://example.com",
        "curl https://example.com \\",
        "curl https://example.com -H",
        "curl https://example.com -H 'Bad header'",
        "curl https://example.com -H 'X: first\nsecond'",
        "curl https://example.com -H 'User-Agent:'",
        "curl https://example.com -H 'X-Empty;'",
        "curl https://example.com -F a=b",
        "curl https://example.com -d @file",
        "curl https://example.com --data-urlencode x@file",
        "curl https://example.com -b cookies.txt",
        "curl https://example.com -u username",
        "curl https://example.com -k",
        "curl https://example.com -XPROPFIND",
        "curl https://example.com -I -d a",
        "curl -L --post301 https://example.com -d a",
        "curl --follow --post303 -XGET https://example.com -d a",
        "curl https://example.com --json '{}' -d a",
        "curl https://example.com | sh",
        "curl https://example.com; echo hello",
        "curl https://example.com/$TOKEN",
        "curl https://example.com -d \"$(id)\"",
        "curl https://example.com -d $'a'",
        "curl 'https://example.com/[1-3]'",
        "curl https://example.com --max-time NaN",
        "curl https://example.com --max-time 999999999999999999",
        "curl https://example.com --max-time .0001",
        "curl https://example.com --max-redirs 0",
        "curl https://example.com --max-redirs 101",
        "curl https://example.com --location=true",
    ] {
        assert!(parse(command).is_err(), "accepted: {command}");
    }
}
#[test]
fn exports_files_without_reading_them_and_without_shell_injection() {
    let mut request = Request::new(HttpMethod::POST, "https://example.com");
    request.body = RequestBody::Multipart(vec![
        MultipartPart::text("name", "@literal; type=x $HOME"),
        MultipartPart {
            name: "file".into(),
            value: MultipartValue::File {
                path: "missing,;\"O'Connor\\.txt".into(),
                file_name: Some("quoted;\"name.txt".into()),
                content_type: Some("text/plain".into()),
            },
        },
    ]);
    let code = export(&RequestDraft::from_request(&request)).unwrap();
    assert!(code.references_files());
    let words = shell::words(code.text()).unwrap();
    assert!(words.iter().any(|v| v == "name=@literal; type=x $HOME"));
    assert!(words.iter().any(|v| v == "file=@\"./missing,;\\\"O'Connor\\\\.txt\";type=text/plain;filename=\"quoted;\\\"name.txt\""));
    assert!(parse(code.text()).is_err());
    request.body = RequestBody::File("-".into());
    let code = export(&RequestDraft::from_request(&request)).unwrap();
    assert!(shell::words(code.text())
        .unwrap()
        .contains(&"@./-".to_string()));
}

#[test]
fn single_managed_auth_allows_header_case_and_distinct_name_reordering() {
    let r=effective("curl https://example.com -H 'authorization: Bearer token' -H 'X-First: one' -H 'X-First: two'");
    assert!(r
        .headers
        .iter()
        .any(|(n, v)| n.eq_ignore_ascii_case("authorization") && v == "Bearer token"));
    assert_eq!(
        r.headers
            .iter()
            .filter(|(n, _)| n == "X-First")
            .map(|(_, v)| v.as_str())
            .collect::<Vec<_>>(),
        vec!["one", "two"]
    );
}

#[test]
fn shell_quote_round_trips_literal_combinations() {
    for prefix in [
        "", "'", "\"", "用户", "\\", "$HOME", "`uname`", "\n", "\r\n", "@", ";()|&<>",
    ] {
        for suffix in ["", "tail", "'\\\"", "\n", "用户"] {
            let value = format!("{prefix}{suffix}");
            let r = effective(&format!(
                "curl https://example.com --data-raw {}",
                shell::quote(&value).unwrap()
            ));
            assert_eq!(r.body.as_text(), Some(value.as_str()));
            assert_eq!(
                shell::words(&shell::quote(&value).unwrap()).unwrap(),
                vec![value]
            );
        }
    }
}

#[test]
fn export_uses_send_normalization_omits_disabled_rows_and_rejects_invalid_drafts() {
    let mut draft = RequestDraft::new();
    draft.set_url("https://example.com");
    draft.set_method(HttpMethod::POST);
    draft.upsert_header("X-Disabled", "hidden");
    let ix = draft
        .headers()
        .iter()
        .position(|h| h.key == "X-Disabled")
        .unwrap();
    draft.toggle_header(ix);
    draft.set_bearer_token(" Bearer token ");
    draft.set_body_kind(crate::models::BodyKind::Raw);
    draft.set_body("$quoted ' body");
    let before = draft.clone();
    let code = export(&draft).unwrap();
    assert_eq!(draft, before);
    assert!(!code.text().contains("X-Disabled"));
    assert!(!code.requires_modern_curl());
    assert!(code.text().contains("--location"));
    assert!(!code.text().contains("--request POST"));
    let mut patch = draft.clone();
    patch.set_method(HttpMethod::PATCH);
    assert!(export(&patch).unwrap().requires_modern_curl());
    let imported = parse(code.text()).unwrap();
    assert!(imported
        .draft()
        .construct()
        .request()
        .headers
        .iter()
        .any(|(n, v)| n == "Authorization" && v == "Bearer token"));
    draft.set_body_kind(crate::models::BodyKind::Json);
    draft.set_body("invalid json");
    assert!(export(&draft).is_err());
    assert!(export(&RequestDraft::new()).is_err());
}

#[test]
fn rejects_unrepresentable_redirect_overrides_and_file_metadata() {
    assert!(parse("curl -L -XPOST https://example.com -d a=1")
        .unwrap_err()
        .to_string()
        .contains("--follow"));
    let mut request = Request::new(HttpMethod::POST, "https://example.com");
    request.body = RequestBody::Multipart(vec![MultipartPart::file("file", "missing.txt")]);
    assert!(export(&RequestDraft::from_request(&request))
        .unwrap_err()
        .to_string()
        .contains("content type"));
}

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
    let parsed = parse("/usr/bin/curl.exe -sSg --follow -XPATCH --url=https://example.com -H'X-Test: yes' --data-raw=value --max-time=1.025 --max-redirs 3").unwrap();
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
        "curl https://example.com -XGET -d ''",
        "curl https://example.com -XDELETE -d a",
        "curl https://example.com -I -d a",
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

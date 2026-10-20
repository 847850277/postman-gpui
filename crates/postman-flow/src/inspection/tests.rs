use super::*;
use serde_json::json;

#[test]
fn typed_values_and_redaction_metadata_are_distinct() {
    let r = Redactor::default();
    for value in [
        json!(null),
        json!(false),
        json!(0),
        json!(""),
        json!([]),
        json!({}),
        json!({"nested":[false,0,null]}),
    ] {
        let inspected = r.inspect("value", &value, false);
        assert_eq!(inspected.value(), Some(&value));
        assert_eq!(
            serde_json::from_str::<Value>(&inspected.json().unwrap()).unwrap(),
            value
        );
        assert!(inspected.redacted_paths().is_empty());
    }
    let literal = r.inspect("value", &json!("[REDACTED]"), false);
    assert_eq!(literal.kind(), "string");
    let secret = r.inspect("value", &json!({"hidden_shape":[1,2,3]}), true);
    assert_eq!(secret.kind(), "redacted");
    assert_eq!(secret.redacted_paths(), [""]);
    assert_eq!(secret.json().unwrap(), "\"[REDACTED]\"");
}

#[test]
fn credentials_echoes_and_encoded_values_never_reach_copy_or_debug() {
    let mut r = Redactor::default();
    r.headers(&[
        ("Authorization".into(), "Bearer auth-SYNTH".into()),
        (
            "Set-Cookie".into(),
            "sid=cookie-SYNTH; Path=/; HttpOnly".into(),
        ),
    ]);
    r.url("https://user-SYNTH:pass-SYNTH@example.test/?token=query-SYNTH#fragment-SYNTH");
    r.discover(&json!({"nested":{"api_key":"key-SYNTH"}}));
    r.remember(&json!("quote\"SYNTH"));
    let value = json!({"safe":{"zero":0,"false":false,"path":"/"}, "nested":{"password":"new-SYNTH"}, "echo":["auth-SYNTH","cookie-SYNTH","query-SYNTH","key-SYNTH","user-SYNTH","pass-SYNTH","fragment-SYNTH","quote\\\"SYNTH"], "encoded":"{\"token\":\"encoded-SYNTH\"}"});
    let inspected = r.inspect("value", &value, false);
    let copy = inspected.json().unwrap();
    assert!(!copy.contains("SYNTH"), "{copy}");
    assert!(!format!("{inspected:?}").contains("SYNTH"));
    assert_eq!(inspected.value().unwrap()["safe"], value["safe"]);
    assert!(!inspected.redacted_paths().is_empty());
    assert!(!format!("{r:?}").contains("SYNTH"));
}

#[test]
fn size_node_depth_and_secret_budgets_are_explicit() {
    let r = Redactor::default();
    for v in [
        json!("x".repeat(MAX_BYTES + 1)),
        json!(vec![0; MAX_NODES + 1]),
        json!("\u{0}".repeat(MAX_BYTES / 2)),
    ] {
        let inspected = r.inspect("value", &v, false);
        assert!(inspected.is_truncated());
        assert!(inspected.json().is_none());
    }
    let mut exhausted = r;
    exhausted.remember(&json!("secret".repeat(MAX_SECRET_BYTES)));
    assert_eq!(
        exhausted.inspect("value", &json!(true), false).kind(),
        "redacted"
    );
}

#[test]
fn form_and_multipart_credentials_and_literal_redaction_paths() {
    use postman_http::request::{MultipartPart, RequestBody};
    let mut r = Redactor::default();
    r.body(&RequestBody::UrlEncoded(
        "password=form-SYNTH&ok=123".into(),
    ));
    r.body(&RequestBody::Multipart(vec![MultipartPart::text(
        "api-key",
        "multipart-SYNTH",
    )]));
    let v = r.inspect(
        "body",
        &json!({"echo":"form-SYNTH multipart-SYNTH","literal":"[REDACTED]","zero":0}),
        false,
    );
    assert_eq!(v.redacted_paths(), ["/echo"]);
    assert!(!v.json().unwrap().contains("SYNTH"));
    assert_eq!(v.value().unwrap()["zero"], json!(0));
}

#[test]
fn ordinary_query_values_do_not_mask_output_values_or_keys() {
    let mut r = Redactor::default();
    r.url("https://example.test/?page=1&user=id&q=widget");
    let value = json!({"id":1,"width":10,"name":"order-123","kind":"widget"});
    let inspected = r.inspect("result", &value, false);
    assert_eq!(inspected.value(), Some(&value));
    assert!(inspected.redacted_paths().is_empty());
    // Short, explicitly sensitive credentials must still be protected.
    r.url("https://example.test/?token=1");
    assert_eq!(r.inspect("result", &json!(1), false).kind(), "redacted");
}

#[test]
fn opaque_header_padding_is_not_a_cookie_secret() {
    let mut r = Redactor::default();
    r.headers(&[
        ("Authorization".into(), "Basic dXNlcjE6cA==".into()),
        ("X-Api-Key".into(), "opaque==".into()),
        ("Cookie".into(), "sid=cookie-SYNTH; csrf=csrf-SYNTH".into()),
    ]);
    let inspected = r.inspect(
        "result",
        &json!({
            "formula":"a=b", "padding":"==", "basic":"dXNlcjE6cA==",
            "api":"opaque==", "sid":"cookie-SYNTH", "csrf":"csrf-SYNTH"
        }),
        false,
    );
    assert_eq!(inspected.value().unwrap()["formula"], "a=b");
    assert_eq!(inspected.value().unwrap()["padding"], "==");
    assert_eq!(
        inspected.redacted_paths(),
        ["/api", "/basic", "/csrf", "/sid"]
    );
}

#[test]
fn credential_names_do_not_include_unrelated_words_or_metadata() {
    let mut r = Redactor::default();
    let value = json!({"token_count":42,"secretary":"Ada","tokenizer":"word",
        "password_policy":"strong","clientSecretLength":32});
    r.discover(&value);
    assert_eq!(r.inspect("result", &value, false).value(), Some(&value));
    assert_eq!(
        r.inspect("token_count", &json!(42), false).value(),
        Some(&json!(42))
    );
    for name in [
        "token",
        "access_token",
        "refreshToken",
        "X-Auth-Token",
        "client_secret",
        "clientSecret",
        "password",
        "dbPassword",
        "API_KEY",
        "apiKey",
        "X-API-Key",
        "Authorization",
        "Proxy-Authorization",
        "Cookie",
        "Set-Cookie",
        "session_id",
        "secret_key",
        "clientSecretKey",
        "awsSecretAccessKey",
        "passwordHash",
        "accessTokenValue",
        "secretValue",
    ] {
        assert!(sensitive_name(name), "{name}");
        assert_eq!(
            r.inspect(name, &json!("SYNTH"), false).kind(),
            "redacted",
            "{name}"
        );
    }
}

#[test]
fn url_credentials_are_masked_in_raw_decoded_and_encoded_forms() {
    let mut r = Redactor::default();
    r.url("https://user%40example:p%40ss%20word@example.test/?token=hello%20world&api_key=a%2bb#fragment%20secret");
    for value in [
        "user@example",
        "user%40example",
        "p@ss word",
        "p%40ss%20word",
        "p%40ss+word",
        "hello world",
        "hello%20world",
        "hello+world",
        "a+b",
        "a%2bb",
        "a%2Bb",
        "fragment secret",
        "fragment%20secret",
    ] {
        let inspected = r.inspect("echo", &json!(format!("echo: {value}")), false);
        assert_eq!(inspected.kind(), "redacted", "{value}");
        assert!(!inspected.json().unwrap().contains(value));
    }
    // A literal plus in userinfo must not be decoded as a form space.
    let mut plus = Redactor::default();
    plus.url("https://user:p+ss@example.test/");
    assert_eq!(
        plus.inspect("echo", &json!("p+ss"), false).kind(),
        "redacted"
    );
    assert_eq!(
        plus.inspect("echo", &json!("p ss"), false).value(),
        Some(&json!("p ss"))
    );
}

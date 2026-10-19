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
    r.url("https://user-SYNTH:pass-SYNTH@example.test/?q=query-SYNTH#fragment-SYNTH");
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

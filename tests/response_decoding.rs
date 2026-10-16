//! HTTP character decoding must preserve the response bytes and History preview.
use postman_gpui::{models::HistoricalResponse, persistence::HistoricalResponseSnapshotV2};
use postman_http::{
    request::{HttpMethod, Request, RequestOptions},
    HttpResponse, HttpTransport,
};
use postman_request::RequestClient;

fn fetch(content_type: &str, bytes: &[u8]) -> HttpResponse {
    let mut server = mockito::Server::new();
    let mock = server
        .mock("GET", "/response")
        .with_header("content-type", content_type)
        .with_body(bytes)
        .create();
    let client = RequestClient::try_new("response-decoding-test/1.0").unwrap();
    let response = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(client.execute(
            Request::new(HttpMethod::GET, format!("{}/response", server.url())),
            RequestOptions::default(),
        ))
        .unwrap();
    mock.assert();
    assert_eq!(response.body_bytes(), bytes);
    response
}

#[test]
fn yaml_media_types_retain_text_and_persisted_history() {
    let text = "title: demo\nitems:\n  - 中文\n";
    for content_type in [
        "application/yaml",
        "application/x-yaml",
        "application/yaml; charset=utf-8",
        "text/yaml",
    ] {
        let response = fetch(content_type, text.as_bytes());
        assert!(!response.is_binary(), "{content_type}");
        assert_eq!(response.body, text);
        let history = HistoricalResponse::from_response(&response);
        let snapshot = HistoricalResponseSnapshotV2::try_from(&history).unwrap();
        assert_eq!(snapshot.body().preview(), Some(text), "{content_type}");
        assert_eq!(snapshot.persisted_size(), text.len() as u64);
        assert_eq!(snapshot.original_size(), text.len() as u64);
    }
}

#[test]
fn declared_charset_wins_over_another_encodings_bom() {
    for (bytes, expected) in [
        (b"caf\xe9".as_slice(), "café"),
        (b"\xef\xbb\xbf\xe9".as_slice(), "ï»¿é"),
        (b"\xff\xfeAB".as_slice(), "ÿþAB"),
        (b"\xfe\xffAB".as_slice(), "þÿAB"),
    ] {
        let response = fetch("text/plain; charset=windows-1252", bytes);
        assert!(!response.is_binary());
        assert_eq!(response.body, expected);
        let history = HistoricalResponse::from_response(&response);
        let snapshot = HistoricalResponseSnapshotV2::try_from(&history).unwrap();
        assert_eq!(snapshot.body().preview(), Some(expected));
        assert_eq!(snapshot.original_size(), bytes.len() as u64);
    }
}

#[test]
fn matching_boms_are_removed_without_changing_encoding() {
    for (content_type, bytes) in [
        ("text/plain; charset=utf-8", b"\xef\xbb\xbfAB".as_slice()),
        ("text/plain; charset=utf-16le", b"\xff\xfeA\0B\0".as_slice()),
        ("text/plain; charset=utf-16be", b"\xfe\xff\0A\0B".as_slice()),
    ] {
        let response = fetch(content_type, bytes);
        assert!(!response.is_binary());
        assert_eq!(response.body, "AB");
    }
    // Without a declared encoding, the transport's UTF-8 default stays authoritative.
    let json = fetch("application/json", b"\xff\xfe{\0}\0");
    assert!(json.is_binary());
    assert!(json.body.is_empty());
    let utf8_json = fetch("application/json", b"\xef\xbb\xbf{}");
    assert_eq!(utf8_json.body, "{}");
}

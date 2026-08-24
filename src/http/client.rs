use crate::errors::AppError;
use crate::http::response::HttpResponse;
use crate::models::{
    HttpMethod, MultipartPart, MultipartValue, Request, RequestBody, DEFAULT_MAX_REDIRECT_HOPS,
};
use reqwest::{
    cookie::{CookieStore, Jar},
    header::HeaderValue,
    multipart, Client, ClientBuilder, RequestBuilder, Url,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, RwLock},
};

const DEFAULT_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

tokio::task_local! {
    static RESPONSE_COOKIE_CAPTURE: Arc<Mutex<Vec<(String, String)>>>;
}

#[derive(Clone)]
pub(super) struct HttpClient {
    client: Client,
    range_client: Client,
    cookie_jar: Arc<ApplicationCookieJar>,
}

impl HttpClient {
    pub(super) fn new() -> Self {
        let cookie_jar = Arc::new(ApplicationCookieJar::default());
        let client = base_client_builder(cookie_jar.clone())
            // Keep response negotiation explicit. Reqwest adds one Accept-Encoding value only
            // when the user did not supply one, then transparently decodes the response and
            // removes the stale wire encoding/length headers.
            .gzip(true)
            .deflate(true)
            .brotli(true)
            .build()
            .expect("the built-in HTTP client configuration should be valid");
        // Reqwest 0.12 currently adds Accept-Encoding even when Range is present. Keep a second
        // connection pool with the same cookie provider for that one negotiation-suppressed path.
        let range_client = base_client_builder(cookie_jar.clone())
            .no_gzip()
            .no_deflate()
            .no_brotli()
            .build()
            .expect("the range HTTP client configuration should be valid");
        HttpClient {
            client,
            range_client,
            cookie_jar,
        }
    }

    pub(super) fn cookie_snapshot(&self) -> Vec<(String, String)> {
        self.cookie_jar.snapshot()
    }

    pub(super) fn clear_cookies(&self) -> usize {
        self.cookie_jar.clear()
    }

    /// Executes the complete request command without rebuilding or coercing its semantics.
    pub(super) async fn execute(&self, request: Request) -> Result<HttpResponse, AppError> {
        let stored_cookies = Arc::new(Mutex::new(Vec::new()));
        let response = RESPONSE_COOKIE_CAPTURE
            .scope(
                stored_cookies.clone(),
                self.execute_with_cookie_capture(request),
            )
            .await?;
        let mut stored_cookies = stored_cookies
            .lock()
            .expect("response cookie capture lock should not be poisoned")
            .clone();
        stored_cookies.sort();
        stored_cookies.dedup();
        Ok(response.with_stored_cookies(stored_cookies))
    }

    async fn execute_with_cookie_capture(
        &self,
        request: Request,
    ) -> Result<HttpResponse, AppError> {
        let Request {
            method,
            url,
            headers,
            body,
        } = request;
        let has_accept_encoding = headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("accept-encoding"));
        let has_range = headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("range"));
        let client = if has_range && !has_accept_encoding {
            &self.range_client
        } else {
            &self.client
        };
        let mut request = match method {
            HttpMethod::GET => client.get(&url),
            HttpMethod::POST => client.post(&url),
            HttpMethod::PUT => client.put(&url),
            HttpMethod::DELETE => client.delete(&url),
            HttpMethod::PATCH => client.patch(&url),
            HttpMethod::HEAD => client.head(&url),
            HttpMethod::OPTIONS => client.request(reqwest::Method::OPTIONS, &url),
        };

        for (key, value) in headers {
            request = request.header(key, value);
        }

        request = match body {
            RequestBody::None => request,
            RequestBody::Json(body) | RequestBody::Raw(body) | RequestBody::UrlEncoded(body) => {
                request.body(body)
            }
            RequestBody::Multipart(parts) => {
                let mut form = multipart::Form::new();
                for part in parts {
                    let MultipartPart { name, value } = part;
                    form = match value {
                        MultipartValue::Text(value) => form.text(name, value),
                        MultipartValue::File {
                            path,
                            file_name,
                            content_type,
                        } => {
                            let bytes = tokio::fs::read(&path).await.map_err(|error| {
                                AppError::ValidationError(format!(
                                    "failed to read multipart file for field `{name}` at {}: {error}",
                                    path.display()
                                ))
                            })?;
                            let inferred_name = path
                                .file_name()
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_else(|| "upload.bin".to_string());
                            let mut file_part = multipart::Part::bytes(bytes)
                                .file_name(file_name.unwrap_or(inferred_name));
                            if let Some(content_type) = content_type {
                                file_part = file_part.mime_str(&content_type)?;
                            }
                            form.part(name, file_part)
                        }
                    };
                }
                request.multipart(form)
            }
        };

        Self::send(request).await
    }

    async fn send(request: RequestBuilder) -> Result<HttpResponse, AppError> {
        let response = request.send().await?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (name.to_string(), value.to_string()))
            })
            .collect();
        let body = response.text().await?;
        Ok(HttpResponse::new(status, headers, body))
    }
}

fn base_client_builder(cookie_jar: Arc<ApplicationCookieJar>) -> ClientBuilder {
    Client::builder()
        .user_agent(DEFAULT_USER_AGENT)
        // Redirect following is part of the application's request contract. Keep the policy
        // explicit so a dependency default cannot silently change that behavior.
        .redirect(reqwest::redirect::Policy::limited(
            DEFAULT_MAX_REDIRECT_HOPS as usize,
        ))
        // Both connection pools belong to one application session. The observable provider
        // retains cookies from intermediate redirects and adds them to later requests.
        .cookie_provider(cookie_jar)
}

/// Reqwest's built-in Jar deliberately hides its contents. This wrapper keeps the wire behavior
/// delegated to that implementation while exposing only cookie names and origins to the UI. It
/// never copies sensitive values into application state or logs.
#[derive(Debug, Default)]
struct ApplicationCookieJar {
    jar: RwLock<Jar>,
    cookies_by_origin: RwLock<BTreeMap<String, Vec<String>>>,
}

impl ApplicationCookieJar {
    fn snapshot(&self) -> Vec<(String, String)> {
        self.cookies_by_origin
            .read()
            .expect("cookie snapshot lock should not be poisoned")
            .iter()
            .flat_map(|(origin, names)| {
                names.iter().map(move |name| (origin.clone(), name.clone()))
            })
            .collect()
    }

    fn clear(&self) -> usize {
        let mut jar = self
            .jar
            .write()
            .expect("cookie jar lock should not be poisoned");
        let mut snapshots = self
            .cookies_by_origin
            .write()
            .expect("cookie snapshot lock should not be poisoned");
        let cleared = snapshots.values().map(Vec::len).sum();
        snapshots.clear();
        *jar = Jar::default();
        cleared
    }

    fn refresh_origin_snapshot(&self, url: &Url, jar: &Jar) {
        let origin = cookie_origin(url);
        let mut names = jar
            .cookies(url)
            .and_then(|header| header.to_str().ok().map(str::to_string))
            .into_iter()
            .flat_map(|header| {
                header
                    .split(';')
                    .filter_map(|pair| pair.trim().split_once('=').map(|(name, _)| name.trim()))
                    .filter(|name| !name.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        names.sort();
        names.dedup();

        let mut snapshots = self
            .cookies_by_origin
            .write()
            .expect("cookie snapshot lock should not be poisoned");
        if names.is_empty() {
            snapshots.remove(&origin);
        } else {
            snapshots.insert(origin, names);
        }
    }
}

impl CookieStore for ApplicationCookieJar {
    fn set_cookies(&self, cookie_headers: &mut dyn Iterator<Item = &HeaderValue>, url: &Url) {
        let headers = cookie_headers.cloned().collect::<Vec<_>>();
        let origin = cookie_origin(url);
        let captured = headers
            .iter()
            .filter_map(|header| header.to_str().ok())
            .filter_map(set_cookie_name)
            .map(|name| (origin.clone(), name))
            .collect::<Vec<_>>();
        let _ = RESPONSE_COOKIE_CAPTURE.try_with(|capture| {
            capture
                .lock()
                .expect("response cookie capture lock should not be poisoned")
                .extend(captured);
        });
        let jar = self
            .jar
            .read()
            .expect("cookie jar lock should not be poisoned");
        jar.set_cookies(&mut headers.iter(), url);
        self.refresh_origin_snapshot(url, &jar);
    }

    fn cookies(&self, url: &Url) -> Option<HeaderValue> {
        self.jar
            .read()
            .expect("cookie jar lock should not be poisoned")
            .cookies(url)
    }
}

fn set_cookie_name(value: &str) -> Option<String> {
    value
        .split(';')
        .next()?
        .split_once('=')
        .map(|(name, _)| name.trim())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

fn cookie_origin(url: &Url) -> String {
    let host = url.host_str().unwrap_or("unknown-host");
    match url.port() {
        Some(port) => format!("{}://{host}:{port}", url.scheme()),
        None => format!("{}://{host}", url.scheme()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MultipartPart, MultipartValue};
    use flate2::{
        write::{GzEncoder, ZlibEncoder},
        Compression,
    };
    use mockito::{Matcher, Server};
    use std::io::Write;

    const DEFAULT_ACCEPT_ENCODING: &str = "gzip,deflate,br";

    fn gzip(body: &str) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(body.as_bytes())
            .expect("gzip test payload should be writable");
        encoder
            .finish()
            .expect("gzip test payload should be encodable")
    }

    fn deflate(body: &str) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(body.as_bytes())
            .expect("deflate test payload should be writable");
        encoder
            .finish()
            .expect("deflate test payload should be encodable")
    }

    fn brotli(body: &str) -> Vec<u8> {
        let mut encoded = Vec::new();
        {
            let mut encoder = brotli::CompressorWriter::new(&mut encoded, 4_096, 5, 22);
            encoder
                .write_all(body.as_bytes())
                .expect("Brotli test payload should be writable");
        }
        encoded
    }

    #[tokio::test]
    async fn default_client_sends_product_user_agent() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/headers")
            .match_header("user-agent", DEFAULT_USER_AGENT)
            .with_status(200)
            .with_body("ok")
            .create_async()
            .await;

        let response = HttpClient::new()
            .execute(Request::new(
                HttpMethod::GET,
                format!("{}/headers", server.url()),
            ))
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), 200);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn compression_negotiation_decodes_supported_formats_and_sanitizes_headers() {
        let mut server = Server::new_async().await;
        let cases = [
            (
                "/gzip",
                "gzip",
                r#"{"gzipped":true}"#,
                gzip as fn(&str) -> Vec<u8>,
            ),
            (
                "/deflate",
                "deflate",
                r#"{"deflated":true}"#,
                deflate as fn(&str) -> Vec<u8>,
            ),
            (
                "/brotli",
                "br",
                r#"{"brotli":true}"#,
                brotli as fn(&str) -> Vec<u8>,
            ),
        ];
        let client = HttpClient::new();

        for (path, encoding, decoded, encode) in cases {
            let response = encode(decoded);
            let request = server
                .mock("GET", path)
                .match_header("accept-encoding", DEFAULT_ACCEPT_ENCODING)
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_header("content-encoding", encoding)
                .with_body(response)
                .create_async()
                .await;

            let response = client
                .execute(Request::new(
                    HttpMethod::GET,
                    format!("{}{path}", server.url()),
                ))
                .await
                .unwrap_or_else(|error| panic!("{encoding} response should decode: {error}"));

            assert_eq!(response.status(), 200);
            assert_eq!(response.body(), decoded);
            assert!(response.headers().iter().any(|(name, value)| {
                name.eq_ignore_ascii_case("content-type") && value == "application/json"
            }));
            assert!(response.headers().iter().all(|(name, _)| {
                !name.eq_ignore_ascii_case("content-encoding")
                    && !name.eq_ignore_ascii_case("content-length")
            }));
            request.assert_async().await;
        }
    }

    #[tokio::test]
    async fn user_accept_encoding_wins_and_range_suppresses_automatic_negotiation() {
        let mut server = Server::new_async().await;
        let user_header = server
            .mock("GET", "/user-encoding")
            .match_header("accept-encoding", "identity")
            .with_status(200)
            .with_body("user header")
            .create_async()
            .await;
        let range = server
            .mock("GET", "/range")
            .match_header("range", "bytes=0-3")
            .match_header("accept-encoding", Matcher::Missing)
            .with_status(206)
            .with_body("part")
            .create_async()
            .await;
        let client = HttpClient::new();

        let mut explicit = Request::new(HttpMethod::GET, format!("{}/user-encoding", server.url()));
        explicit.add_header("Accept-Encoding", "identity");
        let explicit = client
            .execute(explicit)
            .await
            .expect("the user Accept-Encoding request should succeed");
        assert_eq!(explicit.body(), "user header");

        let mut ranged = Request::new(HttpMethod::GET, format!("{}/range", server.url()));
        ranged.add_header("Range", "bytes=0-3");
        let ranged = client
            .execute(ranged)
            .await
            .expect("the ranged request should succeed");
        assert_eq!(ranged.status(), 206);
        assert_eq!(ranged.body(), "part");

        user_header.assert_async().await;
        range.assert_async().await;
    }

    #[tokio::test]
    async fn corrupt_compressed_body_returns_a_readable_decode_error() {
        let mut server = Server::new_async().await;
        let corrupt = server
            .mock("GET", "/corrupt-gzip")
            .match_header("accept-encoding", DEFAULT_ACCEPT_ENCODING)
            .with_status(200)
            .with_header("content-encoding", "gzip")
            .with_body("not a gzip stream")
            .create_async()
            .await;

        let result = HttpClient::new()
            .execute(Request::new(
                HttpMethod::GET,
                format!("{}/corrupt-gzip", server.url()),
            ))
            .await;
        let error = match result {
            Ok(_) => panic!("corrupt gzip bytes must not become a successful text response"),
            Err(error) => error,
        };

        assert!(
            error.to_string().to_ascii_lowercase().contains("decod"),
            "the decoder failure should be readable: {error}"
        );
        corrupt.assert_async().await;
    }

    #[tokio::test]
    async fn default_client_follows_redirect_to_final_response() {
        let mut server = Server::new_async().await;
        let redirected_url = format!("{}/anything/redirected", server.url());
        let final_body = format!(r#"{{"method":"GET","url":"{redirected_url}"}}"#);
        let redirect = server
            .mock("GET", "/redirect-to")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("url".into(), "/anything/redirected".into()),
                Matcher::UrlEncoded("status_code".into(), "302".into()),
            ]))
            .with_status(302)
            .with_header("location", "/anything/redirected")
            .create_async()
            .await;
        let target = server
            .mock("GET", "/anything/redirected")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(final_body.clone())
            .create_async()
            .await;

        let response = HttpClient::new()
            .execute(Request::new(
                HttpMethod::GET,
                format!(
                    "{}/redirect-to?url=%2Fanything%2Fredirected&status_code=302",
                    server.url()
                ),
            ))
            .await
            .expect("the default client should follow the redirect");

        assert_eq!(response.status(), 200);
        assert_eq!(response.body(), final_body);
        redirect.assert_async().await;
        target.assert_async().await;
    }

    #[tokio::test]
    async fn session_cookie_is_retained_across_redirects_sent_later_and_clearable() {
        let mut server = Server::new_async().await;
        let set_cookie = server
            .mock("GET", "/cookies/set")
            .match_query(Matcher::UrlEncoded(
                "session".into(),
                "cookie-e2e-demo".into(),
            ))
            .with_status(302)
            .with_header("location", "/cookies")
            .with_header("set-cookie", "session=cookie-e2e-demo; Path=/")
            .create_async()
            .await;
        let echo_with_cookie = server
            .mock("GET", "/cookies")
            .match_header("cookie", "session=cookie-e2e-demo")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"cookies":{"session":"cookie-e2e-demo"}}"#)
            .expect(2)
            .create_async()
            .await;
        let echo_without_cookie = server
            .mock("GET", "/cookies")
            .match_header("cookie", Matcher::Missing)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"cookies":{}}"#)
            .create_async()
            .await;
        let client = HttpClient::new();

        let set_response = client
            .execute(Request::new(
                HttpMethod::GET,
                format!("{}/cookies/set?session=cookie-e2e-demo", server.url()),
            ))
            .await
            .expect("the cookie-setting redirect should complete");
        assert_eq!(
            set_response.body(),
            r#"{"cookies":{"session":"cookie-e2e-demo"}}"#
        );
        assert_eq!(
            set_response.stored_cookies(),
            &[(server.url(), "session".to_string())],
            "the response keeps non-sensitive evidence from the intermediate Set-Cookie"
        );
        assert_eq!(
            client.cookie_snapshot(),
            vec![(server.url(), "session".to_string())],
            "application state exposes only origin and cookie name"
        );

        let echoed = client
            .execute(Request::new(
                HttpMethod::GET,
                format!("{}/cookies", server.url()),
            ))
            .await
            .expect("a later request should use the same cookie session");
        assert_eq!(
            echoed.body(),
            r#"{"cookies":{"session":"cookie-e2e-demo"}}"#
        );
        assert!(echoed.stored_cookies().is_empty());

        assert_eq!(client.clear_cookies(), 1);
        assert!(client.cookie_snapshot().is_empty());
        let cleared = client
            .execute(Request::new(
                HttpMethod::GET,
                format!("{}/cookies", server.url()),
            ))
            .await
            .expect("the cleared client should remain usable");
        assert_eq!(cleared.body(), r#"{"cookies":{}}"#);
        assert!(cleared.stored_cookies().is_empty());

        set_cookie.assert_async().await;
        echo_with_cookie.assert_async().await;
        echo_without_cookie.assert_async().await;
    }

    #[tokio::test]
    async fn concurrent_responses_keep_cookie_capture_request_scoped() {
        let mut server = Server::new_async().await;
        let first = server
            .mock("GET", "/cookies/first")
            .with_status(200)
            .with_header("set-cookie", "first=one; Path=/")
            .with_body("first")
            .create_async()
            .await;
        let second = server
            .mock("GET", "/cookies/second")
            .with_status(200)
            .with_header("set-cookie", "second=two; Path=/")
            .with_body("second")
            .create_async()
            .await;
        let client = HttpClient::new();

        let (first_response, second_response) = tokio::join!(
            client.execute(Request::new(
                HttpMethod::GET,
                format!("{}/cookies/first", server.url()),
            )),
            client.execute(Request::new(
                HttpMethod::GET,
                format!("{}/cookies/second", server.url()),
            )),
        );
        let first_response = first_response.expect("the first response should complete");
        let second_response = second_response.expect("the second response should complete");

        assert_eq!(
            first_response.stored_cookies(),
            &[(server.url(), "first".to_string())]
        );
        assert_eq!(
            second_response.stored_cookies(),
            &[(server.url(), "second".to_string())]
        );
        first.assert_async().await;
        second.assert_async().await;
    }

    #[tokio::test]
    async fn raw_body_sends_exact_bytes_without_content_type() {
        let body = "plain text body";
        let mut server = Server::new_async().await;
        let mock = server
            .mock("PUT", "/anything/raw")
            .match_header("content-type", Matcher::Missing)
            .match_body(Matcher::Exact(body.to_string()))
            .with_status(200)
            .with_body("ok")
            .create_async()
            .await;

        let mut request = Request::new(HttpMethod::PUT, format!("{}/anything/raw", server.url()));
        request.body = RequestBody::Raw(body.to_string());
        let response = HttpClient::new()
            .execute(request)
            .await
            .expect("raw request should succeed");

        assert_eq!(response.status(), 200);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn multipart_body_sends_ordered_text_and_file_parts() {
        let mut server = Server::new_async().await;
        let fixture_path = std::env::temp_dir().join(format!(
            "postman-gpui-multipart-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should follow the Unix epoch")
                .as_nanos()
        ));
        std::fs::write(&fixture_path, "file payload").expect("fixture should be writable");

        let mock = server
            .mock("POST", "/upload")
            .match_header(
                "content-type",
                Matcher::Regex("^multipart/form-data; boundary=".to_string()),
            )
            .match_body(Matcher::AllOf(vec![
                Matcher::Regex(
                    "(?s)name=\"note\".*hello multipart.*name=\"category\".*gpui.*name=\"attachment\""
                        .to_string(),
                ),
                Matcher::Regex("filename=\"fixture.txt\"".to_string()),
                Matcher::Regex("(?i)content-type: text/plain".to_string()),
                Matcher::Regex("file payload".to_string()),
            ]))
            .with_status(200)
            .with_body("ok")
            .create_async()
            .await;

        let mut request = Request::new(HttpMethod::POST, format!("{}/upload", server.url()));
        request.body = RequestBody::Multipart(vec![
            MultipartPart::text("note", "hello multipart"),
            MultipartPart::text("category", "gpui"),
            MultipartPart {
                name: "attachment".to_string(),
                value: MultipartValue::File {
                    path: fixture_path.clone(),
                    file_name: Some("fixture.txt".to_string()),
                    content_type: Some("text/plain".to_string()),
                },
            },
        ]);
        let response = HttpClient::new()
            .execute(request)
            .await
            .expect("multipart request should succeed");

        let _ = std::fs::remove_file(&fixture_path);
        assert_eq!(response.status(), 200);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn missing_multipart_file_error_identifies_the_field_and_path_before_transport() {
        let missing_path = std::env::temp_dir().join(format!(
            "postman-gpui-missing-transport-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should follow the Unix epoch")
                .as_nanos()
        ));
        let mut request = Request::new(HttpMethod::POST, "http://127.0.0.1:9/upload");
        request.body = RequestBody::Multipart(vec![MultipartPart {
            name: "upload".to_string(),
            value: MultipartValue::File {
                path: missing_path.clone(),
                file_name: Some("missing.txt".to_string()),
                content_type: Some("text/plain".to_string()),
            },
        }]);

        let error = match HttpClient::new().execute(request).await {
            Ok(_) => panic!("a missing selected file must fail before transport"),
            Err(error) => error,
        };
        let message = error.to_string();
        assert!(message.contains("field `upload`"));
        assert!(message.contains(missing_path.to_string_lossy().as_ref()));
        assert!(!message.contains("file payload"));
    }
}

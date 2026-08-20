use crate::errors::AppError;
use crate::http::response::HttpResponse;
use crate::models::{HttpMethod, MultipartValue, Request, RequestBody};
use reqwest::{multipart, Client, RequestBuilder};

const DEFAULT_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

#[derive(Clone)]
pub(super) struct HttpClient {
    client: Client,
}

impl HttpClient {
    pub(super) fn new() -> Self {
        HttpClient {
            client: Client::builder()
                .user_agent(DEFAULT_USER_AGENT)
                .build()
                .expect("the built-in HTTP client configuration should be valid"),
        }
    }

    /// Executes the complete request command without rebuilding or coercing its semantics.
    pub(super) async fn execute(&self, request: Request) -> Result<HttpResponse, AppError> {
        let Request {
            method,
            url,
            headers,
            body,
        } = request;
        let mut request = match method {
            HttpMethod::GET => self.client.get(&url),
            HttpMethod::POST => self.client.post(&url),
            HttpMethod::PUT => self.client.put(&url),
            HttpMethod::DELETE => self.client.delete(&url),
            HttpMethod::PATCH => self.client.patch(&url),
            HttpMethod::HEAD => self.client.head(&url),
            HttpMethod::OPTIONS => self.client.request(reqwest::Method::OPTIONS, &url),
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
                    form = match part.value {
                        MultipartValue::Text(value) => form.text(part.name, value),
                        MultipartValue::File {
                            path,
                            file_name,
                            content_type,
                        } => {
                            let bytes = tokio::fs::read(&path).await.map_err(|error| {
                                AppError::ValidationError(format!(
                                    "failed to read multipart file {}: {error}",
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
                            form.part(part.name, file_part)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MultipartPart, MultipartValue};
    use mockito::{Matcher, Server};

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
}

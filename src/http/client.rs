use crate::errors::AppError;
use crate::http::response::HttpResponse;
use crate::models::HttpMethod;
use reqwest::{Client, RequestBuilder};
use std::collections::HashMap;

#[derive(Clone)]
pub struct HttpClient {
    client: Client,
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient {
    pub fn new() -> Self {
        HttpClient {
            client: Client::new(),
        }
    }

    pub async fn get(&self, url: &str) -> Result<HttpResponse, AppError> {
        self.request(HttpMethod::GET, url, None, None).await
    }

    pub async fn get_with_headers(
        &self,
        url: &str,
        headers: Option<HashMap<String, String>>,
    ) -> Result<HttpResponse, AppError> {
        self.request(HttpMethod::GET, url, headers, None).await
    }

    pub async fn post(
        &self,
        url: &str,
        body: &str,
        headers: Option<HashMap<String, String>>,
    ) -> Result<HttpResponse, AppError> {
        self.request(HttpMethod::POST, url, headers, Some(body.to_string()))
            .await
    }

    pub async fn request(
        &self,
        method: HttpMethod,
        url: &str,
        headers: Option<HashMap<String, String>>,
        body: Option<String>,
    ) -> Result<HttpResponse, AppError> {
        let mut request = match method {
            HttpMethod::GET => self.client.get(url),
            HttpMethod::POST => self.client.post(url),
            HttpMethod::PUT => self.client.put(url),
            HttpMethod::DELETE => self.client.delete(url),
            HttpMethod::PATCH => self.client.patch(url),
            HttpMethod::HEAD => self.client.head(url),
            HttpMethod::OPTIONS => self.client.request(reqwest::Method::OPTIONS, url),
        };

        if let Some(h) = headers {
            for (key, value) in h {
                request = request.header(key, value);
            }
        }

        if let Some(body) = body {
            request = request.body(body);
        }

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

    #[test]
    fn test_http_client_creation() {
        let client = HttpClient::new();
        // Verify that the client can be created
        assert!(std::mem::size_of_val(&client) > 0);
    }

    #[test]
    fn test_default_client() {
        let client = HttpClient::default();
        // Verify that default implementation works
        assert!(std::mem::size_of_val(&client) > 0);
    }
}

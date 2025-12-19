// filepath: /postman-gpui/postman-gpui/src/http/client.rs
use reqwest::{Client, Error};
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

    pub async fn get(
        &self,
        url: &str,
        headers: Option<HashMap<String, String>>,
    ) -> Result<String, Error> {
        let mut request = self.client.get(url);

        if let Some(h) = headers {
            for (key, value) in h {
                request = request.header(key, value);
            }
        }

        let response = request.send().await?;
        let body = response.text().await?;
        Ok(body)
    }

    pub async fn post(
        &self,
        url: &str,
        body: &str,
        headers: Option<HashMap<String, String>>,
    ) -> Result<String, Error> {
        let mut request = self.client.post(url).body(body.to_string());

        if let Some(h) = headers {
            for (key, value) in h {
                request = request.header(key, value);
            }
        }

        let response = request.send().await?;
        let response_body = response.text().await?;
        Ok(response_body)
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

// This file defines the structure and methods for creating HTTP requests.

use std::collections::HashMap;

#[derive(Debug)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
}

impl HttpRequest {
    pub fn new(method: &str, url: &str) -> Self {
        HttpRequest {
            method: method.to_string(),
            url: url.to_string(),
            headers: HashMap::new(),
            body: None,
        }
    }

    pub fn add_header(&mut self, key: &str, value: &str) {
        self.headers.insert(key.to_string(), value.to_string());
    }

    pub fn set_body(&mut self, body: &str) {
        self.body = Some(body.to_string());
    }
}

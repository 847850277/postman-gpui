use std::collections::HashMap;

/// 统一的 HTTP 请求模型
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl Request {
    /// 创建新的请求
    pub fn new(method: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            url: url.into(),
            headers: Vec::new(),
            body: None,
        }
    }

    /// 添加 header
    pub fn add_header(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.headers.push((key.into(), value.into()));
    }

    /// 设置请求体
    pub fn set_body(&mut self, body: impl Into<String>) {
        self.body = Some(body.into());
    }

    /// 转换 headers 为 HashMap 格式（用于 HTTP 客户端）
    pub fn headers_as_map(&self) -> HashMap<String, String> {
        self.headers.iter().cloned().collect()
    }

    /// 验证请求是否有效
    pub fn is_valid(&self) -> bool {
        !self.url.trim().is_empty() && !self.method.trim().is_empty()
    }
}

impl Default for Request {
    fn default() -> Self {
        Self {
            method: "GET".to_string(),
            url: String::new(),
            headers: Vec::new(),
            body: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_request() {
        let request = Request::new("GET", "https://api.example.com");
        assert_eq!(request.method, "GET");
        assert_eq!(request.url, "https://api.example.com");
        assert!(request.headers.is_empty());
        assert!(request.body.is_none());
    }

    #[test]
    fn test_add_header() {
        let mut request = Request::new("GET", "https://api.example.com");
        request.add_header("Authorization", "Bearer token123");
        assert_eq!(request.headers.len(), 1);
        assert_eq!(
            request.headers[0],
            ("Authorization".to_string(), "Bearer token123".to_string())
        );
    }

    #[test]
    fn test_set_body() {
        let mut request = Request::new("POST", "https://api.example.com");
        request.set_body("{\"key\": \"value\"}");
        assert_eq!(request.body, Some("{\"key\": \"value\"}".to_string()));
    }

    #[test]
    fn test_set_form_data_body() {
        let mut request = Request::new("POST", "https://api.example.com/submit");
        let form_data = "username=john_doe&email=john@example.com&age=30";
        request.set_body(form_data);
        request.add_header("Content-Type", "application/x-www-form-urlencoded");
        
        assert_eq!(request.body, Some(form_data.to_string()));
        assert_eq!(request.headers.len(), 1);
        assert_eq!(
            request.headers[0],
            ("Content-Type".to_string(), "application/x-www-form-urlencoded".to_string())
        );
    }

    #[test]
    fn test_headers_as_map() {
        let mut request = Request::new("GET", "https://api.example.com");
        request.add_header("Content-Type", "application/json");
        request.add_header("Authorization", "Bearer token");

        let map = request.headers_as_map();
        assert_eq!(map.len(), 2);
        assert_eq!(
            map.get("Content-Type"),
            Some(&"application/json".to_string())
        );
    }

    #[test]
    fn test_is_valid() {
        let valid_request = Request::new("GET", "https://api.example.com");
        assert!(valid_request.is_valid());

        let invalid_request = Request::new("GET", "");
        assert!(!invalid_request.is_valid());
    }
}

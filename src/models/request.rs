use std::collections::HashMap;
use std::fmt;

/// HTTP 请求方法枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
    HEAD,
    OPTIONS,
}

impl HttpMethod {
    /// 从字符串解析 HTTP 方法（不区分大小写）
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.to_uppercase().as_str() {
            "GET" => Ok(HttpMethod::GET),
            "POST" => Ok(HttpMethod::POST),
            "PUT" => Ok(HttpMethod::PUT),
            "DELETE" => Ok(HttpMethod::DELETE),
            "PATCH" => Ok(HttpMethod::PATCH),
            "HEAD" => Ok(HttpMethod::HEAD),
            "OPTIONS" => Ok(HttpMethod::OPTIONS),
            _ => Err(format!("Unsupported HTTP method: {}", s)),
        }
    }

    /// 获取所有支持的 HTTP 方法
    pub fn all() -> Vec<HttpMethod> {
        vec![
            HttpMethod::GET,
            HttpMethod::POST,
            HttpMethod::PUT,
            HttpMethod::DELETE,
            HttpMethod::PATCH,
            HttpMethod::HEAD,
            HttpMethod::OPTIONS,
        ]
    }
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpMethod::GET => write!(f, "GET"),
            HttpMethod::POST => write!(f, "POST"),
            HttpMethod::PUT => write!(f, "PUT"),
            HttpMethod::DELETE => write!(f, "DELETE"),
            HttpMethod::PATCH => write!(f, "PATCH"),
            HttpMethod::HEAD => write!(f, "HEAD"),
            HttpMethod::OPTIONS => write!(f, "OPTIONS"),
        }
    }
}

impl From<&str> for HttpMethod {
    fn from(s: &str) -> Self {
        HttpMethod::from_str(s).unwrap_or(HttpMethod::GET)
    }
}

impl From<String> for HttpMethod {
    fn from(s: String) -> Self {
        HttpMethod::from_str(&s).unwrap_or(HttpMethod::GET)
    }
}

impl From<HttpMethod> for String {
    fn from(method: HttpMethod) -> Self {
        method.to_string()
    }
}

/// 统一的 HTTP 请求模型
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl Request {
    /// 创建新的请求
    pub fn new(method: impl Into<HttpMethod>, url: impl Into<String>) -> Self {
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
        !self.url.trim().is_empty()
    }
}

impl Default for Request {
    fn default() -> Self {
        Self {
            method: HttpMethod::GET,
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
        assert_eq!(request.method, HttpMethod::GET);
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
            (
                "Content-Type".to_string(),
                "application/x-www-form-urlencoded".to_string()
            )
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

    #[test]
    fn test_http_method_display() {
        assert_eq!(HttpMethod::GET.to_string(), "GET");
        assert_eq!(HttpMethod::POST.to_string(), "POST");
        assert_eq!(HttpMethod::PUT.to_string(), "PUT");
        assert_eq!(HttpMethod::DELETE.to_string(), "DELETE");
        assert_eq!(HttpMethod::PATCH.to_string(), "PATCH");
        assert_eq!(HttpMethod::HEAD.to_string(), "HEAD");
        assert_eq!(HttpMethod::OPTIONS.to_string(), "OPTIONS");
    }

    #[test]
    fn test_http_method_from_str() {
        assert_eq!(HttpMethod::from_str("GET").unwrap(), HttpMethod::GET);
        assert_eq!(HttpMethod::from_str("get").unwrap(), HttpMethod::GET);
        assert_eq!(HttpMethod::from_str("post").unwrap(), HttpMethod::POST);
        assert_eq!(HttpMethod::from_str("PUT").unwrap(), HttpMethod::PUT);
        assert!(HttpMethod::from_str("INVALID").is_err());
    }

    #[test]
    fn test_http_method_from_string() {
        let method: HttpMethod = "GET".into();
        assert_eq!(method, HttpMethod::GET);
        
        let method: HttpMethod = "post".to_string().into();
        assert_eq!(method, HttpMethod::POST);
    }

    #[test]
    fn test_http_method_all() {
        let all = HttpMethod::all();
        assert_eq!(all.len(), 7);
        assert!(all.contains(&HttpMethod::GET));
        assert!(all.contains(&HttpMethod::POST));
        assert!(all.contains(&HttpMethod::PUT));
        assert!(all.contains(&HttpMethod::DELETE));
        assert!(all.contains(&HttpMethod::PATCH));
        assert!(all.contains(&HttpMethod::HEAD));
        assert!(all.contains(&HttpMethod::OPTIONS));
    }

    #[test]
    fn test_request_with_http_method_enum() {
        let request = Request::new(HttpMethod::POST, "https://api.example.com");
        assert_eq!(request.method, HttpMethod::POST);
    }
}

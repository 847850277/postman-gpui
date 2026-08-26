use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

/// Product default used by the transport and persisted replay snapshots.
pub const DEFAULT_MAX_REDIRECT_HOPS: u32 = 10;
pub const MAX_REDIRECT_HOPS: u32 = 100;

/// Redirect behavior is part of replay intent and is configured independently for every request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectPolicy {
    Follow,
    DoNotFollow,
}

/// Request options that affect wire behavior and therefore must survive History replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestOptions {
    pub timeout_ms: Option<u64>,
    pub redirect_policy: RedirectPolicy,
    pub max_redirect_hops: u32,
}

impl Default for RequestOptions {
    fn default() -> Self {
        Self {
            timeout_ms: None,
            redirect_policy: RedirectPolicy::Follow,
            max_redirect_hops: DEFAULT_MAX_REDIRECT_HOPS,
        }
    }
}

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

    /// Methods that carry a request body in this client.
    pub fn allows_body(self) -> bool {
        matches!(self, Self::POST | Self::PUT | Self::PATCH)
    }
}

impl FromStr for HttpMethod {
    type Err = String;

    /// Parse an HTTP method case-insensitively.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
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
        s.parse().unwrap_or(HttpMethod::GET)
    }
}

impl From<String> for HttpMethod {
    fn from(s: String) -> Self {
        s.parse().unwrap_or(HttpMethod::GET)
    }
}

impl From<HttpMethod> for String {
    fn from(method: HttpMethod) -> Self {
        method.to_string()
    }
}

/// One multipart field. File contents are loaded only when the transport executes the request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultipartPart {
    pub name: String,
    pub value: MultipartValue,
}

impl MultipartPart {
    pub fn text(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: MultipartValue::Text(value.into()),
        }
    }

    pub fn file(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            value: MultipartValue::File {
                path: path.into(),
                file_name: None,
                content_type: None,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultipartValue {
    Text(String),
    File {
        path: PathBuf,
        file_name: Option<String>,
        content_type: Option<String>,
    },
}

/// Strongly typed request body. The encoding choice is part of the request itself instead of
/// living in a second `body_kind` flag that can drift out of sync with its payload.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RequestBody {
    #[default]
    None,
    Json(String),
    Raw(String),
    UrlEncoded(String),
    Multipart(Vec<MultipartPart>),
}

impl RequestBody {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Json(value) | Self::Raw(value) | Self::UrlEncoded(value) => Some(value),
            Self::None | Self::Multipart(_) => None,
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::None => true,
            Self::Json(value) | Self::Raw(value) | Self::UrlEncoded(value) => value.is_empty(),
            Self::Multipart(parts) => parts.is_empty(),
        }
    }

    pub fn payload_len(&self) -> usize {
        match self {
            Self::None => 0,
            Self::Json(value) | Self::Raw(value) | Self::UrlEncoded(value) => value.len(),
            Self::Multipart(parts) => parts
                .iter()
                .map(|part| match &part.value {
                    MultipartValue::Text(value) => value.len(),
                    MultipartValue::File { .. } => 0,
                })
                .sum(),
        }
    }

    pub fn searchable_text(&self) -> String {
        match self {
            Self::None => String::new(),
            Self::Json(value) | Self::Raw(value) | Self::UrlEncoded(value) => value.clone(),
            Self::Multipart(parts) => parts
                .iter()
                .map(|part| match &part.value {
                    MultipartValue::Text(value) => format!("{}={value}", part.name),
                    MultipartValue::File { path, .. } => {
                        format!("{}=@{}", part.name, path.display())
                    }
                })
                .collect::<Vec<_>>()
                .join("&"),
        }
    }
}

/// 统一的 HTTP 请求模型
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: RequestBody,
}

impl Request {
    /// 创建新的请求
    pub fn new(method: impl Into<HttpMethod>, url: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            url: url.into(),
            headers: Vec::new(),
            body: RequestBody::None,
        }
    }

    /// 添加 header
    pub fn add_header(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.headers.push((key.into(), value.into()));
    }

    /// 设置请求体
    pub fn set_body(&mut self, body: impl Into<String>) {
        self.body = RequestBody::Raw(body.into());
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
            body: RequestBody::None,
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
        assert_eq!(request.body, RequestBody::None);
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
        assert_eq!(
            request.body,
            RequestBody::Raw("{\"key\": \"value\"}".to_string())
        );
    }

    #[test]
    fn test_set_form_data_body() {
        let mut request = Request::new("POST", "https://api.example.com/submit");
        let form_data = "username=john_doe&email=john@example.com&age=30";
        request.set_body(form_data);
        request.add_header("Content-Type", "application/x-www-form-urlencoded");

        assert_eq!(request.body, RequestBody::Raw(form_data.to_string()));
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
    fn typed_body_keeps_encoding_with_payload() {
        let body = RequestBody::UrlEncoded("name=Ada+Lovelace".to_string());
        assert_eq!(body.as_text(), Some("name=Ada+Lovelace"));
        assert!(!body.is_none());
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
        assert_eq!("GET".parse::<HttpMethod>().unwrap(), HttpMethod::GET);
        assert_eq!("get".parse::<HttpMethod>().unwrap(), HttpMethod::GET);
        assert_eq!("post".parse::<HttpMethod>().unwrap(), HttpMethod::POST);
        assert_eq!("PUT".parse::<HttpMethod>().unwrap(), HttpMethod::PUT);
        assert!("INVALID".parse::<HttpMethod>().is_err());
    }

    #[test]
    fn test_http_method_from_string() {
        let method: HttpMethod = "GET".into();
        assert_eq!(method, HttpMethod::GET);

        let method: HttpMethod = "post".to_string().into();
        assert_eq!(method, HttpMethod::POST);
    }

    #[test]
    fn test_http_method_allows_body() {
        assert!(!HttpMethod::GET.allows_body());
        assert!(!HttpMethod::DELETE.allows_body());
        assert!(!HttpMethod::HEAD.allows_body());
        assert!(!HttpMethod::OPTIONS.allows_body());
        assert!(HttpMethod::POST.allows_body());
        assert!(HttpMethod::PUT.allows_body());
        assert!(HttpMethod::PATCH.allows_body());
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

    #[test]
    fn request_options_match_current_transport_defaults() {
        assert_eq!(
            RequestOptions::default(),
            RequestOptions {
                timeout_ms: None,
                redirect_policy: RedirectPolicy::Follow,
                max_redirect_hops: 10,
            }
        );
    }
}

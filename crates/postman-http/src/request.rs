use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

/// Product default used by the transport and persisted replay snapshots.
pub const DEFAULT_MAX_REDIRECT_HOPS: u32 = 10;
pub const MAX_REDIRECT_HOPS: u32 = 100;

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

#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: RequestBody,
}

impl Request {
    pub fn new(method: impl Into<HttpMethod>, url: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            url: url.into(),
            headers: Vec::new(),
            body: RequestBody::None,
        }
    }

    pub fn add_header(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.headers.push((key.into(), value.into()));
    }

    pub fn set_body(&mut self, body: impl Into<String>) {
        self.body = RequestBody::Raw(body.into());
    }

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

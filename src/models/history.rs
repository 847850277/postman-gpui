use super::request::{MultipartValue, Request, RequestOptions};
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Maximum number of SQLite-backed History rows rendered by the application.
const DEFAULT_MAX_HISTORY_ENTRIES: usize = 50;

/// Editor-only state captured with a completed request. The effective `Request` remains the
/// transport truth; this snapshot preserves disabled and incomplete multipart rows for replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestEditorIntent {
    Multipart(Vec<MultipartEditorPart>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultipartEditorPart {
    pub enabled: bool,
    pub name: String,
    pub value: MultipartValue,
}

/// Persisted response body evidence attached to one immutable History row.
///
/// History never restores transport cookie state. `Unsupported` represents binary, download, or
/// otherwise non-textual payloads whose bytes are deliberately excluded from SQLite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoricalResponseBody {
    Empty,
    Text(String),
    TruncatedText(String),
    Unsupported,
}

impl HistoricalResponseBody {
    pub fn preview(&self) -> Option<&str> {
        match self {
            Self::Text(preview) | Self::TruncatedText(preview) => Some(preview),
            Self::Empty | Self::Unsupported => None,
        }
    }

    pub fn is_truncated(&self) -> bool {
        matches!(self, Self::TruncatedText(_))
    }

    pub fn is_available(&self) -> bool {
        !matches!(self, Self::Unsupported)
    }
}

/// Sanitized response evidence that may be replayed without performing a network request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: HistoricalResponseBody,
    pub media_type: Option<String>,
    pub elapsed_ms: u128,
    pub original_size: usize,
    pub persisted_size: usize,
}

impl HistoricalResponse {
    /// Build the unsanitized runtime candidate produced by a completed send. The persistence
    /// snapshot boundary owns sanitization, body classification, and truncation.
    pub fn completed(
        status: u16,
        headers: Vec<(String, String)>,
        body: String,
        elapsed_ms: u128,
    ) -> Self {
        let original_size = body.len();
        let media_type = headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
            .map(|(_, value)| value.clone());
        let body = if body.is_empty() {
            HistoricalResponseBody::Empty
        } else {
            HistoricalResponseBody::Text(body)
        };
        Self {
            status,
            headers,
            body,
            media_type,
            elapsed_ms,
            original_size,
            persisted_size: original_size,
        }
    }
}

/// Request history entry
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    /// Stable identity retained across persistence and replay.
    pub id: String,
    pub request: Request,
    pub editor_intent: Option<RequestEditorIntent>,
    pub request_options: RequestOptions,
    pub timestamp: DateTime<Utc>,
    pub name: String,
    pub status: Option<u16>,
    pub elapsed_ms: Option<u128>,
    pub response_size: Option<usize>,
    /// `None` identifies a V1 row whose response was never stored.
    pub historical_response: Option<HistoricalResponse>,
}

impl HistoryEntry {
    pub fn new(request: Request, name: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            request,
            editor_intent: None,
            request_options: RequestOptions::default(),
            timestamp: Utc::now(),
            name,
            status: None,
            elapsed_ms: None,
            response_size: None,
            historical_response: None,
        }
    }

    pub fn completed(
        request: Request,
        name: String,
        status: u16,
        elapsed_ms: u128,
        response_size: usize,
    ) -> Self {
        Self::completed_with_intent(request, name, status, elapsed_ms, response_size, None)
    }

    pub fn completed_with_intent(
        request: Request,
        name: String,
        status: u16,
        elapsed_ms: u128,
        response_size: usize,
        editor_intent: Option<RequestEditorIntent>,
    ) -> Self {
        Self::completed_with_intent_and_options(
            request,
            name,
            status,
            elapsed_ms,
            response_size,
            editor_intent,
            RequestOptions::default(),
        )
    }

    pub fn completed_with_intent_and_options(
        request: Request,
        name: String,
        status: u16,
        elapsed_ms: u128,
        response_size: usize,
        editor_intent: Option<RequestEditorIntent>,
        request_options: RequestOptions,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            request,
            editor_intent,
            request_options,
            timestamp: Utc::now(),
            name,
            status: Some(status),
            elapsed_ms: Some(elapsed_ms),
            response_size: Some(response_size),
            historical_response: None,
        }
    }

    pub fn with_historical_response(mut self, response: HistoricalResponse) -> Self {
        self.historical_response = Some(response);
        self
    }

    /// Get a display name for the history entry
    pub fn display_name(&self) -> String {
        format!("{} {}", self.request.method, self.name)
    }

    /// Get formatted timestamp
    pub fn formatted_time(&self) -> String {
        self.timestamp.format("%H:%M:%S").to_string()
    }
}

/// Latest successful SQLite query result used by the UI.
///
/// This type is not a repository and has no independent append/clear behavior. Mutations replace
/// the complete query result so SQLite remains the single authoritative History data source.
#[derive(Debug, Clone)]
pub struct RequestHistory {
    entries: Vec<HistoryEntry>,
}

impl RequestHistory {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Replace the render projection with one newest-first database query result.
    pub(crate) fn replace(&mut self, mut entries: Vec<HistoryEntry>) {
        entries.truncate(DEFAULT_MAX_HISTORY_ENTRIES);
        self.entries = entries;
    }

    /// Get all history entries
    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    /// Get a specific entry by index
    pub fn get(&self, index: usize) -> Option<&HistoryEntry> {
        self.entries.get(index)
    }

    /// Get the number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if history is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for RequestHistory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_query_result_replaces_the_complete_projection() {
        let mut history = RequestHistory::new();
        let first = HistoryEntry::new(
            Request::new("GET", "https://api.example.com/users"),
            "first query".to_string(),
        );
        let second = HistoryEntry::new(
            Request::new("POST", "https://api.example.com/orders"),
            "second query".to_string(),
        );

        history.replace(vec![first]);
        history.replace(vec![second.clone()]);

        assert_eq!(history.len(), 1);
        assert!(!history.is_empty());
        assert_eq!(history.get(0).unwrap().id, second.id);
    }

    #[test]
    fn database_order_is_preserved_without_independent_reordering() {
        let mut history = RequestHistory::new();
        let first = HistoryEntry::new(Request::default(), "newest".to_string());
        let second = HistoryEntry::new(Request::default(), "older".to_string());
        history.replace(vec![first, second]);

        assert_eq!(history.len(), 2);
        assert_eq!(history.get(0).unwrap().name, "newest");
        assert_eq!(history.get(1).unwrap().name, "older");
    }

    #[test]
    fn projection_defensively_caps_untrusted_query_results_at_fifty() {
        let mut history = RequestHistory::new();
        let entries = (0..60)
            .map(|index| HistoryEntry::new(Request::default(), format!("Request {index}")))
            .collect();

        history.replace(entries);

        assert_eq!(history.len(), DEFAULT_MAX_HISTORY_ENTRIES);
        assert_eq!(history.get(0).unwrap().name, "Request 0");
        assert_eq!(history.get(49).unwrap().name, "Request 49");
    }

    #[test]
    fn test_history_entry_display_name() {
        let request = Request::new("GET", "https://api.example.com/users");
        let entry = HistoryEntry::new(request, "Users API".to_string());

        assert_eq!(entry.display_name(), "GET Users API");
    }

    #[test]
    fn history_entries_receive_distinct_stable_ids() {
        let first = HistoryEntry::new(Request::default(), "first".to_string());
        let second = HistoryEntry::new(Request::default(), "second".to_string());

        assert_ne!(first.id, second.id);
        assert!(Uuid::parse_str(&first.id).is_ok());
        assert!(Uuid::parse_str(&second.id).is_ok());
    }
}

use super::request::Request;
use chrono::{DateTime, Utc};

/// Maximum number of history entries to keep
const DEFAULT_MAX_HISTORY_ENTRIES: usize = 50;

/// Request history entry
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub request: Request,
    pub timestamp: DateTime<Utc>,
    pub name: String,
}

impl HistoryEntry {
    pub fn new(request: Request, name: String) -> Self {
        Self {
            request,
            timestamp: Utc::now(),
            name,
        }
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

/// Request history manager
#[derive(Debug, Clone)]
pub struct RequestHistory {
    entries: Vec<HistoryEntry>,
    max_entries: usize,
}

impl RequestHistory {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            max_entries: DEFAULT_MAX_HISTORY_ENTRIES,
        }
    }

    /// Add a request to history
    pub fn add(&mut self, request: Request, name: String) {
        let entry = HistoryEntry::new(request, name);
        self.entries.insert(0, entry); // Add to front (newest first)

        // Trim to max entries
        if self.entries.len() > self.max_entries {
            self.entries.truncate(self.max_entries);
        }
    }

    /// Get all history entries
    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    /// Get a specific entry by index
    pub fn get(&self, index: usize) -> Option<&HistoryEntry> {
        self.entries.get(index)
    }

    /// Clear all history
    pub fn clear(&mut self) {
        self.entries.clear();
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
    fn test_add_history_entry() {
        let mut history = RequestHistory::new();
        let mut request = Request::new("GET", "https://api.example.com/users");
        request.add_header("Authorization", "Bearer token");
        
        history.add(request.clone(), "https://api.example.com/users".to_string());
        
        assert_eq!(history.len(), 1);
        assert!(!history.is_empty());
        
        let entry = history.get(0).unwrap();
        assert_eq!(entry.request.method, "GET");
        assert_eq!(entry.request.url, "https://api.example.com/users");
        assert_eq!(entry.request.headers.len(), 1);
    }

    #[test]
    fn test_history_with_query_parameters() {
        let mut history = RequestHistory::new();
        let url_with_params = "https://api.example.com/search?q=test&limit=10";
        let request = Request::new("GET", url_with_params);
        
        history.add(request, url_with_params.to_string());
        
        let entry = history.get(0).unwrap();
        assert_eq!(entry.request.url, url_with_params);
        assert!(entry.request.url.contains("?"));
        assert!(entry.request.url.contains("q=test"));
        assert!(entry.request.url.contains("limit=10"));
    }

    #[test]
    fn test_history_with_body() {
        let mut history = RequestHistory::new();
        let mut request = Request::new("POST", "https://api.example.com/users");
        request.set_body(r#"{"name": "John", "email": "john@example.com"}"#);
        
        history.add(request, "https://api.example.com/users".to_string());
        
        let entry = history.get(0).unwrap();
        assert_eq!(entry.request.method, "POST");
        assert!(entry.request.body.is_some());
        let body = entry.request.body.as_ref().unwrap();
        assert!(body.contains("John"));
    }

    #[test]
    fn test_history_order() {
        let mut history = RequestHistory::new();
        
        // Add first request
        let request1 = Request::new("GET", "https://api.example.com/first");
        history.add(request1, "First request".to_string());
        
        // Add second request
        let request2 = Request::new("POST", "https://api.example.com/second");
        history.add(request2, "Second request".to_string());
        
        // Verify newest is first (index 0)
        assert_eq!(history.len(), 2);
        assert_eq!(history.get(0).unwrap().name, "Second request");
        assert_eq!(history.get(1).unwrap().name, "First request");
    }

    #[test]
    fn test_history_max_entries() {
        let mut history = RequestHistory::new();
        
        // Add more than max entries
        for i in 0..60 {
            let request = Request::new("GET", &format!("https://api.example.com/{}", i));
            history.add(request, format!("Request {}", i));
        }
        
        // Should be limited to max
        assert_eq!(history.len(), 50); // DEFAULT_MAX_HISTORY_ENTRIES
    }

    #[test]
    fn test_history_clear() {
        let mut history = RequestHistory::new();
        let request = Request::new("GET", "https://api.example.com");
        history.add(request, "Test".to_string());
        
        assert_eq!(history.len(), 1);
        
        history.clear();
        
        assert_eq!(history.len(), 0);
        assert!(history.is_empty());
    }

    #[test]
    fn test_history_entry_display_name() {
        let request = Request::new("GET", "https://api.example.com/users");
        let entry = HistoryEntry::new(request, "Users API".to_string());
        
        assert_eq!(entry.display_name(), "GET Users API");
    }
}

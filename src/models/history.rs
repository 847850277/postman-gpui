use super::request::Request;
use chrono::{DateTime, Utc};

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
            max_entries: 50, // Keep last 50 requests
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

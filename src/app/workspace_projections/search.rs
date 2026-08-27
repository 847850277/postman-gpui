use super::RequestTagProjection;
use crate::{
    app::request_lifecycle::RequestTabId,
    models::{HistoryEntry, HttpMethod},
};

/// One open-request match in the application-wide search projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlobalSearchRequestResult {
    pub tab_id: RequestTabId,
    pub display_name: String,
    pub method: HttpMethod,
    pub url: String,
}

/// One persisted History match in the application-wide search projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlobalSearchHistoryResult {
    pub entry_id: String,
    pub display_name: String,
    pub method: HttpMethod,
    pub url: String,
    pub status: Option<u16>,
    pub response_size: Option<usize>,
}

/// Deterministic, grouped search results derived from immutable projection inputs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GlobalSearchResults {
    requests: Vec<GlobalSearchRequestResult>,
    history: Vec<GlobalSearchHistoryResult>,
}

impl GlobalSearchResults {
    pub fn requests(&self) -> &[GlobalSearchRequestResult] {
        &self.requests
    }

    pub fn history(&self) -> &[GlobalSearchHistoryResult] {
        &self.history
    }

    pub fn len(&self) -> usize {
        self.requests.len() + self.history.len()
    }

    pub fn is_empty(&self) -> bool {
        self.requests.is_empty() && self.history.is_empty()
    }
}

/// Immutable request-tag and History inputs for one search evaluation.
pub(crate) struct GlobalSearchInput<'a> {
    pub(crate) query: &'a str,
    pub(crate) request_tags: &'a [RequestTagProjection],
    pub(crate) history: &'a [HistoryEntry],
}

/// Stateless index/evaluator. Source order is the index order, so UI grouping and keyboard
/// selection remain stable without a second mutable copy of request or History state.
pub(crate) struct GlobalSearchProjection;

impl GlobalSearchProjection {
    pub(crate) fn results(&self, input: GlobalSearchInput<'_>) -> GlobalSearchResults {
        let query = input.query.trim().to_lowercase();
        if query.is_empty() {
            return GlobalSearchResults::default();
        }

        let matches = |display_name: &str, method: HttpMethod, url: &str| {
            display_name.to_lowercase().contains(&query)
                || method.to_string().to_lowercase().contains(&query)
                || url.to_lowercase().contains(&query)
        };

        let requests = input
            .request_tags
            .iter()
            .filter(|tag| matches(&tag.display_name, tag.method, &tag.url))
            .map(|tag| GlobalSearchRequestResult {
                tab_id: tag.tab_id,
                display_name: tag.display_name.clone(),
                method: tag.method,
                url: tag.url.clone(),
            })
            .collect();
        let history = input
            .history
            .iter()
            .filter(|entry| matches(&entry.name, entry.request.method, &entry.request.url))
            .map(|entry| GlobalSearchHistoryResult {
                entry_id: entry.id.clone(),
                display_name: entry.name.clone(),
                method: entry.request.method,
                url: entry.request.url.clone(),
                status: entry.status,
                response_size: entry.response_size,
            })
            .collect();

        GlobalSearchResults { requests, history }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Request;

    #[test]
    fn grouped_results_preserve_request_and_history_source_order() {
        let request_tags = vec![
            RequestTagProjection {
                tab_id: RequestTabId(1),
                method: HttpMethod::GET,
                display_name: "first match".into(),
                url: "https://first.example".into(),
                dirty: false,
            },
            RequestTagProjection {
                tab_id: RequestTabId(2),
                method: HttpMethod::POST,
                display_name: "second match".into(),
                url: "https://second.example".into(),
                dirty: true,
            },
        ];
        let history = vec![
            HistoryEntry::new(Request::default(), "newest match".into()),
            HistoryEntry::new(Request::default(), "older match".into()),
        ];

        let results = GlobalSearchProjection.results(GlobalSearchInput {
            query: " MATCH ",
            request_tags: &request_tags,
            history: &history,
        });

        assert_eq!(
            results
                .requests()
                .iter()
                .map(|result| result.tab_id)
                .collect::<Vec<_>>(),
            vec![RequestTabId(1), RequestTabId(2)]
        );
        assert_eq!(
            results
                .history()
                .iter()
                .map(|result| result.display_name.as_str())
                .collect::<Vec<_>>(),
            vec!["newest match", "older match"]
        );
    }

    #[test]
    fn blank_query_resets_the_complete_result_projection() {
        let request_tags = vec![RequestTagProjection {
            tab_id: RequestTabId(1),
            method: HttpMethod::GET,
            display_name: "match".into(),
            url: String::new(),
            dirty: false,
        }];

        let results = GlobalSearchProjection.results(GlobalSearchInput {
            query: "   ",
            request_tags: &request_tags,
            history: &[],
        });

        assert!(results.is_empty());
    }
}

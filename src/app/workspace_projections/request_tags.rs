use crate::{app::request_lifecycle::RequestTabId, models::HttpMethod};

/// Immutable draft input consumed by the request tag/tab-label projection.
pub(crate) struct RequestTagInput<'a> {
    pub(crate) tab_id: RequestTabId,
    pub(crate) method: HttpMethod,
    pub(crate) url: &'a str,
    pub(crate) dirty: bool,
}

/// Read model for one request tag in the existing request-tab chrome.
///
/// This is presentation metadata derived from a specific stable tab identity; it does not add a
/// new user-editable tagging feature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestTagProjection {
    pub tab_id: RequestTabId,
    pub method: HttpMethod,
    pub display_name: String,
    pub url: String,
    pub dirty: bool,
}

pub(crate) struct RequestTagProjector;

impl RequestTagProjector {
    pub(crate) fn project(&self, input: RequestTagInput<'_>) -> RequestTagProjection {
        RequestTagProjection {
            tab_id: input.tab_id,
            method: input.method,
            display_name: request_tag_title(input.url),
            url: input.url.to_string(),
            dirty: input.dirty,
        }
    }
}

pub(crate) fn request_tag_title(url: &str) -> String {
    if url.trim().is_empty() {
        return "Untitled request".to_string();
    }
    let without_scheme = url.split_once("://").map(|(_, value)| value).unwrap_or(url);
    let title = without_scheme.chars().take(28).collect::<String>();
    if without_scheme.chars().count() > 28 {
        format!("{title}…")
    } else {
        title
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_inputs_project_to_their_own_stable_request_tags() {
        let projector = RequestTagProjector;
        let first = projector.project(RequestTagInput {
            tab_id: RequestTabId(7),
            method: HttpMethod::GET,
            url: "https://first.example/one",
            dirty: false,
        });
        let second = projector.project(RequestTagInput {
            tab_id: RequestTabId(8),
            method: HttpMethod::POST,
            url: "https://second.example/two",
            dirty: true,
        });

        assert_eq!(first.tab_id, RequestTabId(7));
        assert_eq!(first.display_name, "first.example/one");
        assert!(!first.dirty);
        assert_eq!(second.tab_id, RequestTabId(8));
        assert_eq!(second.display_name, "second.example/two");
        assert!(second.dirty);
    }

    #[test]
    fn title_contract_preserves_empty_truncation_and_scheme_behavior() {
        assert_eq!(request_tag_title(""), "Untitled request");
        assert_eq!(
            request_tag_title("https://example.com/request"),
            "example.com/request"
        );
        assert_eq!(
            request_tag_title("https://example.com/abcdefghijklmnopqrstuvwxyz"),
            "example.com/abcdefghijklmnop…"
        );
    }
}

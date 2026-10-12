//! UI endpoint environments, deliberately separate from Flow compiler/session environments.
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Environment {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) base_url: String,
}

impl Environment {
    pub(super) fn blank() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: String::new(),
            base_url: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EnvironmentSet {
    version: u32,
    pub(super) environments: Vec<Environment>,
    pub(super) active_id: Option<String>,
}

impl Default for EnvironmentSet {
    fn default() -> Self {
        Self {
            version: 1,
            environments: Vec::new(),
            active_id: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FieldError {
    pub(super) id: String,
    pub(super) field: &'static str,
    pub(super) message: String,
}

impl EnvironmentSet {
    pub(super) fn draft(environments: Vec<Environment>, active_id: Option<String>) -> Self {
        Self {
            version: 1,
            environments,
            active_id,
        }
    }

    pub(super) fn active_base(&self) -> Option<&str> {
        self.environments
            .iter()
            .find(|e| Some(&e.id) == self.active_id.as_ref())
            .map(|e| e.base_url.as_str())
    }

    pub(super) fn validate(mut self) -> Result<Self, FieldError> {
        let mut names = HashSet::new();
        let mut ids = HashSet::new();
        for env in &mut self.environments {
            let error = |field, message: &str| FieldError {
                id: env.id.clone(),
                field,
                message: message.into(),
            };
            if env.id.is_empty() || !ids.insert(env.id.clone()) {
                return Err(error("name", "Environment identifiers must be unique."));
            }
            env.name = env.name.trim().to_string();
            if env.name.is_empty() || env.name.chars().count() > 40 {
                return Err(error("name", "Use a name between 1 and 40 characters."));
            }
            if !names.insert(env.name.to_lowercase()) {
                return Err(error("name", "Choose a unique environment name."));
            }
            env.base_url =
                normalize_base(&env.base_url).map_err(|message| error("base-url", message))?;
        }
        if !self
            .environments
            .iter()
            .any(|e| Some(&e.id) == self.active_id.as_ref())
        {
            self.active_id = self.environments.first().map(|e| e.id.clone());
        }
        Ok(self)
    }

    pub(super) fn decode(bytes: &[u8]) -> Result<(Self, Option<String>), String> {
        let saved: Self = serde_json::from_slice(bytes)
            .map_err(|_| "Environment configuration is not valid JSON.".to_string())?;
        if saved.version != 1 {
            return Err("Unsupported environment configuration version.".into());
        }
        let previous = saved.active_id.clone();
        let validated = saved.validate().map_err(|e| e.message)?;
        let warning = (previous != validated.active_id).then(|| {
            "The saved environment is unavailable. Selected the first remaining environment.".into()
        });
        Ok((validated, warning))
    }
}

fn normalize_base(raw: &str) -> Result<String, &'static str> {
    let raw = raw.trim();
    let invalid = "Use an absolute HTTP or HTTPS URL without credentials, query, or fragment.";
    let url = absolute_url(raw).ok_or(invalid)?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid);
    }
    Ok(format!(
        "{}{}",
        url.origin().ascii_serialization(),
        url.path().trim_end_matches('/')
    ))
}

fn absolute_url(raw: &str) -> Option<Url> {
    if raw.chars().any(|c| c.is_whitespace() || c.is_control()) || raw.contains('\\') {
        return None;
    }
    let (scheme, _) = raw.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let url = Url::parse(raw).ok()?;
    url.host_str()?;
    Some(url)
}

/// Parsed-origin and path-segment matching prevents host/prefix lookalikes from being rewritten.
/// Concatenation (not URL join or query reconstruction) retains escaped paths and repeated pairs.
pub(crate) fn rebase_url(raw: &str, previous: &str, next: &str) -> Option<String> {
    if previous == next {
        return None;
    }
    let url = absolute_url(raw)?;
    let old = absolute_url(previous)?;
    let next = normalize_base(next).ok()?;
    let prefix = old.path().trim_end_matches('/');
    if url.origin() != old.origin() || !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    let suffix = url.path().strip_prefix(prefix)?;
    if !suffix.is_empty() && !suffix.starts_with('/') {
        return None;
    }
    let mut result = format!("{next}{suffix}");
    if let Some(index) = raw.find(['?', '#']) {
        result.push_str(&raw[index..]);
    }
    (result != raw).then_some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn env(id: &str, name: &str, base: &str) -> Environment {
        Environment {
            id: id.into(),
            name: name.into(),
            base_url: base.into(),
        }
    }
    #[test]
    fn validation_normalization_identity_and_active_deletion() {
        let set = EnvironmentSet::draft(
            vec![
                env("a", " Development ", "https://api.example.com/api///"),
                env("b", "Local", "http://localhost:3000"),
            ],
            Some("a".into()),
        )
        .validate()
        .unwrap();
        assert_eq!(set.environments[0].name, "Development");
        assert_eq!(set.active_base(), Some("https://api.example.com/api"));
        let renamed = EnvironmentSet::draft(
            vec![env("a", "Renamed", "https://api.example.com/api")],
            set.active_id.clone(),
        )
        .validate()
        .unwrap();
        assert_eq!(renamed.active_id, set.active_id);
        let deleted = EnvironmentSet::draft(vec![set.environments[1].clone()], set.active_id)
            .validate()
            .unwrap();
        assert_eq!(deleted.active_id.as_deref(), Some("b"));
        for name in ["", "  ", &"a".repeat(41)] {
            assert!(
                EnvironmentSet::draft(vec![env("a", name, "https://example.com")], None)
                    .validate()
                    .is_err()
            );
        }
        assert!(EnvironmentSet::draft(
            vec![
                env("a", "Dev", "https://example.com"),
                env("b", " dev ", "https://example.com")
            ],
            None
        )
        .validate()
        .is_err());
        for base in [
            "example.com",
            "https:example.com",
            "ftp://example.com",
            "https://u:p@example.com",
            "https://example.com?",
            "https://example.com#",
            "https://",
            "https://example.com/a b",
            "https://example.com\\x",
        ] {
            assert!(normalize_base(base).is_err(), "{base}");
        }
    }
    #[test]
    fn rebase_preserves_request_suffix_and_rejects_unrelated_or_incomplete_urls() {
        let old = "https://api.example.com/api";
        let next = "http://localhost:3000/v2";
        assert_eq!(
            rebase_url(
                "https://api.example.com/api/users%2F1?tag=a&tag=b&v=%2f+%20#part",
                old,
                next
            )
            .unwrap(),
            "http://localhost:3000/v2/users%2F1?tag=a&tag=b&v=%2f+%20#part"
        );
        assert_eq!(
            rebase_url("https://api.example.com/api?x=", old, next).unwrap(),
            "http://localhost:3000/v2?x="
        );
        assert_eq!(
            rebase_url("https://api.example.com/api/", old, next).unwrap(),
            "http://localhost:3000/v2/"
        );
        assert_eq!(
            rebase_url(
                "https://api.example.com/api/users?q=中文&q=%E4%B8%AD#片段",
                old,
                next
            )
            .unwrap(),
            "http://localhost:3000/v2/users?q=中文&q=%E4%B8%AD#片段"
        );
        assert_eq!(
            rebase_url(
                "https://api.example.com/users",
                "https://api.example.com/",
                next
            )
            .unwrap(),
            "http://localhost:3000/v2/users"
        );
        for raw in [
            "",
            "https://",
            "https://api.example.com/api x",
            "https://api.example.com.evil/api/users",
            "https://api.example.com:444/api/users",
            "https://api.example.com/apiary/users",
            "http://api.example.com/api/users",
            "https://u:p@api.example.com/api/users",
        ] {
            assert_eq!(rebase_url(raw, old, next), None, "{raw}");
        }
        assert_eq!(
            rebase_url("https://api.example.com/api/users", old, old),
            None
        );
    }
}

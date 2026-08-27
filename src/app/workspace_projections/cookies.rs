/// Non-sensitive projection of one application-session cookie. Cookie values stay exclusively in
/// the transport jar; this read model exposes only origin and name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CookieJarEntry {
    pub origin: String,
    pub name: String,
}

/// Explicit transport/session inputs consumed by the Cookie read model.
pub(crate) enum CookieProjectionEvent {
    Snapshot(Vec<(String, String)>),
    Cleared { count: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CookieProjectionTransition {
    SnapshotApplied { cookie_count: usize },
    Reset { cleared_count: usize },
}

/// Application-session Cookie metadata. Values and transport mutation remain owned by the HTTP
/// client; this projection cannot leak or recreate them.
#[derive(Default)]
pub(crate) struct CookieProjection {
    entries: Vec<CookieJarEntry>,
    last_clear_count: Option<usize>,
}

impl CookieProjection {
    pub(crate) fn apply(&mut self, event: CookieProjectionEvent) -> CookieProjectionTransition {
        match event {
            CookieProjectionEvent::Snapshot(snapshot) => {
                let mut entries = snapshot
                    .into_iter()
                    .map(|(origin, name)| CookieJarEntry { origin, name })
                    .collect::<Vec<_>>();
                entries.sort_by(|left, right| {
                    left.origin
                        .cmp(&right.origin)
                        .then_with(|| left.name.cmp(&right.name))
                });
                entries.dedup();
                if !entries.is_empty() {
                    self.last_clear_count = None;
                }
                self.entries = entries;
                CookieProjectionTransition::SnapshotApplied {
                    cookie_count: self.entries.len(),
                }
            }
            CookieProjectionEvent::Cleared { count } => {
                self.entries.clear();
                self.last_clear_count = Some(count);
                CookieProjectionTransition::Reset {
                    cleared_count: count,
                }
            }
        }
    }

    pub(crate) fn entries(&self) -> &[CookieJarEntry] {
        &self.entries
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn last_clear_count(&self) -> Option<usize> {
        self.last_clear_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_is_sorted_deduplicated_and_contains_no_values() {
        let mut projection = CookieProjection::default();

        let transition = projection.apply(CookieProjectionEvent::Snapshot(vec![
            ("https://second.example".into(), "session".into()),
            ("https://first.example".into(), "token".into()),
            ("https://first.example".into(), "token".into()),
        ]));

        assert_eq!(
            transition,
            CookieProjectionTransition::SnapshotApplied { cookie_count: 2 }
        );
        assert_eq!(
            projection.entries(),
            &[
                CookieJarEntry {
                    origin: "https://first.example".into(),
                    name: "token".into(),
                },
                CookieJarEntry {
                    origin: "https://second.example".into(),
                    name: "session".into(),
                },
            ]
        );
    }

    #[test]
    fn reset_and_followup_snapshot_are_scoped_to_one_session_projection() {
        let mut first_session = CookieProjection::default();
        let mut second_session = CookieProjection::default();
        first_session.apply(CookieProjectionEvent::Snapshot(vec![(
            "https://first.example".into(),
            "session".into(),
        )]));
        second_session.apply(CookieProjectionEvent::Snapshot(vec![(
            "https://second.example".into(),
            "session".into(),
        )]));

        assert_eq!(
            first_session.apply(CookieProjectionEvent::Cleared { count: 1 }),
            CookieProjectionTransition::Reset { cleared_count: 1 }
        );
        assert!(first_session.entries().is_empty());
        assert_eq!(first_session.last_clear_count(), Some(1));
        assert_eq!(second_session.len(), 1);
        assert_eq!(second_session.last_clear_count(), None);

        first_session.apply(CookieProjectionEvent::Snapshot(vec![(
            "https://first.example".into(),
            "new-session".into(),
        )]));
        assert_eq!(first_session.last_clear_count(), None);
    }
}

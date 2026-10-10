//! Non-sensitive HTTP layout preferences. Drafts and responses never enter this store.
use gpui::{App, Global, Task};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub(crate) const DEFAULT_SHARE: f32 = 0.46;
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct HttpLayout {
    pub(crate) stacked: bool,
    pub(crate) horizontal: f32,
    pub(crate) vertical: f32,
}
impl Default for HttpLayout {
    fn default() -> Self {
        Self {
            stacked: false,
            horizontal: DEFAULT_SHARE,
            vertical: DEFAULT_SHARE,
        }
    }
}
impl HttpLayout {
    pub(crate) fn share(self, stacked: bool) -> f32 {
        if stacked {
            self.vertical
        } else {
            self.horizontal
        }
    }
    pub(crate) fn set_share(&mut self, stacked: bool, share: f32) {
        if stacked {
            self.vertical = share;
        } else {
            self.horizontal = share;
        }
    }
}

pub struct HttpLayoutPreferences {
    path: PathBuf,
    value: HttpLayout,
    pending: Option<Task<()>>,
    revision: u64,
    error: Option<String>,
}
impl Global for HttpLayoutPreferences {}
impl HttpLayoutPreferences {
    pub fn init(path: PathBuf, cx: &mut App) {
        let (value, error) = match fs::read_to_string(&path) {
            Ok(text) => match decode(&text) {
                Some(value) => (value, None),
                None => (
                    HttpLayout::default(),
                    Some("Invalid saved panel sizes; using defaults".into()),
                ),
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => (HttpLayout::default(), None),
            Err(e) => (
                HttpLayout::default(),
                Some(format!("Cannot read panel sizes: {e}")),
            ),
        };
        cx.set_global(Self {
            path,
            value,
            pending: None,
            revision: 0,
            error,
        });
    }
    pub(crate) fn current(cx: &App) -> HttpLayout {
        cx.try_global::<Self>()
            .map_or_else(HttpLayout::default, |state| state.value)
    }
    pub fn error(cx: &App) -> Option<&str> {
        cx.try_global::<Self>()
            .and_then(|state| state.error.as_deref())
    }
    pub(crate) fn save(value: HttpLayout, cx: &mut App) {
        let Some(state) = cx.try_global::<Self>() else {
            return;
        };
        let path = state.path.clone();
        let state = cx.global_mut::<Self>();
        state.value = value;
        state.revision += 1;
        let revision = state.revision;
        let previous = state.pending.take();
        let task = cx.spawn(async move |cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            let result = cx
                .background_executor()
                .spawn(async move { save(&path, value) })
                .await;
            cx.update(|cx| {
                let state = cx.global_mut::<Self>();
                if revision == state.revision {
                    state.error = result.err().map(|e| {
                        format!("Panel sizes apply this session but could not be saved: {e}")
                    });
                    cx.refresh_windows();
                }
            });
        });
        cx.global_mut::<Self>().pending = Some(task);
    }
}
fn decode(text: &str) -> Option<HttpLayout> {
    let value: HttpLayout = serde_json::from_str(text).ok()?;
    [value.horizontal, value.vertical]
        .iter()
        .all(|n| n.is_finite() && *n > 0. && *n < 1.)
        .then_some(value)
}
fn save(path: &Path, value: HttpLayout) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Missing preference directory"))?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".http-layout-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        fs::write(&temporary, serde_json::to_vec(&value)?)?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[gpui_kit::test]
    fn preferences_validate_restore_and_fail_without_losing_session_changes(
        cx: &mut gpui::TestAppContext,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("http-layout.json");
        cx.update(|cx| HttpLayoutPreferences::init(path.clone(), cx));
        let wanted = HttpLayout {
            stacked: true,
            horizontal: 0.7,
            vertical: 0.35,
        };
        cx.update(|cx| {
            for value in [HttpLayout::default(), wanted] {
                HttpLayoutPreferences::save(value, cx);
            }
        });
        cx.run_until_parked();
        assert_eq!(decode(&fs::read_to_string(&path).unwrap()), Some(wanted));
        cx.update(|cx| {
            HttpLayoutPreferences::init(path.clone(), cx);
            assert_eq!(HttpLayoutPreferences::current(cx), wanted);
            assert!(HttpLayoutPreferences::error(cx).is_none());
        });
        for bad in [
            "invalid",
            r#"{"horizontal":0}"#,
            r#"{"vertical":1.5}"#,
            r#"{"horizontal":"secret"}"#,
            r#"{"stacked":null}"#,
        ] {
            fs::write(&path, bad).unwrap();
            cx.update(|cx| {
                HttpLayoutPreferences::init(path.clone(), cx);
                assert_eq!(HttpLayoutPreferences::current(cx), HttpLayout::default());
                assert!(HttpLayoutPreferences::error(cx).is_some());
            });
        }
        cx.update(|cx| {
            HttpLayoutPreferences::init(path.join("unwritable.json"), cx);
            HttpLayoutPreferences::save(wanted, cx);
        });
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(HttpLayoutPreferences::current(cx), wanted);
            assert!(HttpLayoutPreferences::error(cx)
                .unwrap()
                .contains("could not be saved"));
        });
    }
}

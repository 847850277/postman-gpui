//! The only persisted P1 preference. Kit remains the source of the active mode.

use gpui::{App, Global, Task};
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{assets::IconName, component::ActiveTheme};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub struct Appearance {
    path: PathBuf,
    pending: Option<Task<()>>,
    revision: u64,
    error: Option<String>,
}
impl Global for Appearance {}

impl Appearance {
    pub fn init(path: PathBuf, cx: &mut App) {
        let (mode, error) = match fs::read_to_string(&path) {
            Ok(value) => (decode(&value), None),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (ThemeMode::Light, None),
            Err(error) => (
                ThemeMode::Light,
                Some(format!("Cannot read appearance: {error}")),
            ),
        };
        crate::ui::theme::apply(mode, cx);
        cx.set_global(Self {
            path,
            pending: None,
            revision: 0,
            error,
        });
    }

    pub fn error(cx: &App) -> Option<&str> {
        cx.try_global::<Self>().and_then(|s| s.error.as_deref())
    }

    pub fn toggle(cx: &mut App) {
        let mode = if Theme::global(cx).mode.is_dark() {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        };
        crate::ui::theme::apply(mode, cx);
        // Headless callers can opt out of persistence by not installing Appearance.
        let Some(state) = cx.try_global::<Self>() else {
            return;
        };
        let path = state.path.clone();
        let state = cx.global_mut::<Self>();
        state.revision += 1;
        let revision = state.revision;
        let previous = state.pending.take();
        state.error = None;
        let task = cx.spawn(async move |cx| {
            // Serialize writes: rapid toggles cannot let an older preference win.
            if let Some(previous) = previous {
                previous.await;
            }
            let result = cx
                .background_executor()
                .spawn(async move { save(&path, mode) })
                .await;
            cx.update(|cx| {
                let state = cx.global_mut::<Self>();
                if state.revision == revision {
                    state.error = result
                        .err()
                        .map(|e| format!("Appearance changed, but could not be saved: {e}"));
                    cx.refresh_windows();
                }
            });
        });
        cx.global_mut::<Self>().pending = Some(task);
    }
}

fn decode(value: &str) -> ThemeMode {
    match serde_json::from_str::<String>(value).as_deref() {
        Ok("dark") => ThemeMode::Dark,
        _ => ThemeMode::Light,
    }
}

fn save(path: &Path, mode: ThemeMode) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("missing preference directory"))?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".appearance-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        fs::write(
            &temporary,
            if mode.is_dark() {
                "\"dark\"\n"
            } else {
                "\"light\"\n"
            },
        )?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn button(id: &'static str, cx: &gpui::App) -> gpui_kit::component::button::Button {
    let dark = cx.theme().mode.is_dark();
    crate::ui::components::kit_controls::icon_button(
        id,
        if dark { IconName::Sun } else { IconName::Moon },
        if dark {
            "Switch to light theme"
        } else {
            "Switch to dark theme"
        },
    )
    .on_click(|_, _, cx| Appearance::toggle(cx))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[gpui_kit::test]
    fn preference_survives_restart_and_invalid_values_fall_back_to_light(
        cx: &mut gpui::TestAppContext,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("appearance.json");
        cx.update(|cx| {
            crate::ui::kit::init(cx);
            Appearance::init(path.clone(), cx);
            assert_eq!(Theme::global(cx).mode, ThemeMode::Light);
        });
        for mode in [ThemeMode::Dark, ThemeMode::Light] {
            save(&path, mode).unwrap();
            cx.update(|cx| {
                Appearance::init(path.clone(), cx);
                assert_eq!(Theme::global(cx).mode, mode);
            });
        }
        for value in ["", "null", "\"system\"", "broken json", "{}"] {
            fs::write(&path, value).unwrap();
            cx.update(|cx| {
                Appearance::init(path.clone(), cx);
                assert_eq!(Theme::global(cx).mode, ThemeMode::Light);
            });
        }
    }

    #[gpui_kit::test]
    fn rapid_toggles_save_the_last_choice_and_failures_remain_visible(
        cx: &mut gpui::TestAppContext,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("appearance.json");
        cx.update(|cx| {
            crate::ui::kit::init(cx);
            Appearance::init(path.clone(), cx);
            for _ in 0..5 {
                Appearance::toggle(cx);
            }
        });
        cx.run_until_parked();
        assert_eq!(decode(&fs::read_to_string(&path).unwrap()), ThemeMode::Dark);
        cx.update(|cx| {
            assert!(Appearance::error(cx).is_none());
            Appearance::init(path.join("cannot-be-created.json"), cx);
            Appearance::toggle(cx);
        });
        cx.run_until_parked();
        cx.update(|cx| {
            assert!(Appearance::error(cx)
                .unwrap()
                .contains("could not be saved"))
        });
    }
}

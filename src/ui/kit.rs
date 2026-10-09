//! Shared GPUI Kit initialization for application windows and native UI tests.

use gpui::App;
use gpui_kit::component::{Theme, ThemeMode};

use super::theme::{FONT_MONO, FONT_UI};

/// Call once after registering the application's embedded fonts and before opening windows.
pub fn init(cx: &mut App) {
    gpui_kit::init(cx);
    Theme::change(ThemeMode::Light, None, cx);
    Theme::update(cx, |theme| {
        theme.font_family = FONT_UI.into();
        theme.mono_font_family = FONT_MONO.into();
    });
}

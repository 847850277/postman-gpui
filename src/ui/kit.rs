//! Shared GPUI Kit initialization for application windows and native UI tests.

use gpui::App;
use gpui_kit::component::ThemeMode;

/// Call once after registering the application's embedded fonts and before opening windows.
pub fn init(cx: &mut App) {
    gpui_kit::init(cx);
    super::theme::apply(ThemeMode::Light, cx);
}

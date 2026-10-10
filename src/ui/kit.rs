//! Shared GPUI Kit initialization for application windows and native UI tests.

use gpui::App;
use gpui_kit::component::ThemeMode;

/// Call once after registering the application's embedded fonts and before opening windows.
pub fn init(cx: &mut App) {
    gpui_kit::init(cx);
    // Retain the editor's established Control aliases alongside Kit's platform-native
    // Command shortcuts. Scope them to the URL editor, not every Kit input.
    use gpui_kit::component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
    cx.bind_keys([
        gpui::KeyBinding::new("ctrl-a", SelectAll, Some("RequestUrl > Input")),
        gpui::KeyBinding::new("ctrl-c", Copy, Some("RequestUrl > Input")),
        gpui::KeyBinding::new("ctrl-x", Cut, Some("RequestUrl > Input")),
        gpui::KeyBinding::new("ctrl-v", Paste, Some("RequestUrl > Input")),
        gpui::KeyBinding::new("ctrl-z", Undo, Some("RequestUrl > Input")),
        gpui::KeyBinding::new("ctrl-y", Redo, Some("RequestUrl > Input")),
    ]);
    super::theme::apply(ThemeMode::Light, cx);
}

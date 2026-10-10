//! Shared GPUI Kit initialization for application windows and native UI tests.

use gpui::App;
use gpui_kit::component::ThemeMode;

/// Call once after registering the application's embedded fonts and before opening windows.
pub fn init(cx: &mut App) {
    gpui_kit::init(cx);
    // Preserve the application's Control aliases alongside Kit's platform-native shortcuts.
    // Limit these bindings to request editors; Select's search fields keep Kit's defaults.
    use gpui_kit::component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
    for context in [
        "RequestUrl > Input",
        "HeaderInput > Input",
        "TableCellInput > Input",
    ] {
        cx.bind_keys([
            gpui::KeyBinding::new("ctrl-a", SelectAll, Some(context)),
            gpui::KeyBinding::new("ctrl-c", Copy, Some(context)),
            gpui::KeyBinding::new("ctrl-x", Cut, Some(context)),
            gpui::KeyBinding::new("ctrl-v", Paste, Some(context)),
            gpui::KeyBinding::new("ctrl-z", Undo, Some(context)),
            gpui::KeyBinding::new("ctrl-y", Redo, Some(context)),
            gpui::KeyBinding::new("ctrl-shift-z", Redo, Some(context)),
        ]);
    }
    super::theme::apply(ThemeMode::Light, cx);
}

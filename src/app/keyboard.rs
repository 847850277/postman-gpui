use gpui::{actions, KeyBinding};

pub(crate) use crate::ui::components::common::keyboard::ActivateControl;

actions!(
    postman_keyboard,
    [
        FocusNextControl,
        FocusPreviousControl,
        SendOrCancel,
        NewRequest,
        CloseRequest,
        FocusUrl,
        FocusHistorySearch,
        FocusGlobalSearch,
        ActivateNextRequest,
        ActivatePreviousRequest,
        ToggleShortcutHelp,
        DismissOverlay,
    ]
);

/// Application commands are context-scoped so ordinary text editing keeps ownership of
/// selection, clipboard, Undo/Redo, and unmodified navigation keys.
pub(crate) fn setup_application_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("enter", ActivateControl, Some("KeyboardButton")),
        KeyBinding::new("space", ActivateControl, Some("KeyboardButton")),
        KeyBinding::new("tab", FocusNextControl, Some("PostmanApp")),
        KeyBinding::new("shift-tab", FocusPreviousControl, Some("PostmanApp")),
        KeyBinding::new("cmd-enter", SendOrCancel, None),
        KeyBinding::new("ctrl-enter", SendOrCancel, None),
        KeyBinding::new("cmd-enter", SendOrCancel, Some("HeaderInput > Input")),
        KeyBinding::new("ctrl-enter", SendOrCancel, Some("HeaderInput > Input")),
        KeyBinding::new("cmd-t", NewRequest, None),
        KeyBinding::new("ctrl-t", NewRequest, None),
        KeyBinding::new("cmd-w", CloseRequest, None),
        KeyBinding::new("ctrl-w", CloseRequest, None),
        KeyBinding::new("cmd-l", FocusUrl, None),
        KeyBinding::new("ctrl-l", FocusUrl, None),
        KeyBinding::new("cmd-shift-f", FocusHistorySearch, None),
        KeyBinding::new("ctrl-shift-f", FocusHistorySearch, None),
        KeyBinding::new("cmd-k", FocusGlobalSearch, None),
        KeyBinding::new("ctrl-k", FocusGlobalSearch, None),
        KeyBinding::new("ctrl-tab", ActivateNextRequest, None),
        KeyBinding::new("ctrl-shift-tab", ActivatePreviousRequest, None),
        KeyBinding::new("cmd-/", ToggleShortcutHelp, None),
        KeyBinding::new("ctrl-/", ToggleShortcutHelp, None),
        KeyBinding::new("escape", DismissOverlay, Some("OverlayTrigger")),
    ]
}

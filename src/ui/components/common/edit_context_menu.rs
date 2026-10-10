use gpui_kit::component::menu::PopupMenu;

/// Populate a Kit menu with the same actions handled by the editor's keyboard path.
/// The trigger must focus the editor during right-button capture so Kit can restore
/// that focus on Escape; action_context also routes chosen commands to that editor.
pub(crate) fn edit_popup_menu(
    mut menu: PopupMenu,
    focus: gpui::FocusHandle,
    commands: Vec<(&'static str, Box<dyn gpui::Action>)>,
) -> PopupMenu {
    menu = menu.action_context(focus);
    for (label, action) in commands {
        menu = menu.menu(label, action);
    }
    menu
}

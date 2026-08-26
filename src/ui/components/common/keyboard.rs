use gpui::actions;

// Shared presentation-level activation command. Application features bind platform keys and
// attach business handlers, while reusable UI controls can participate without depending on app.
actions!(shared_keyboard, [ActivateControl]);

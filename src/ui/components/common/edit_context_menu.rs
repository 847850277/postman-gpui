use gpui::{
    anchored, deferred, div, point, px, rgb, Anchor, AnyElement, Context, FontWeight,
    InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, Point, Styled, Window,
};

use crate::ui::theme::{FONT_UI, LINE, MUTED, PANEL, PANEL_ALT, SUBTEXT, TEXT};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditContextAction {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Dismiss,
}

impl EditContextAction {
    fn label(self) -> &'static str {
        match self {
            Self::Undo => "Undo",
            Self::Redo => "Redo",
            Self::Cut => "Cut",
            Self::Copy => "Copy",
            Self::Paste => "Paste",
            Self::SelectAll => "Select All",
            Self::Dismiss => "",
        }
    }

    fn shortcut(self) -> &'static str {
        match self {
            Self::Undo => "⌘Z / Ctrl+Z",
            Self::Redo => "⌘⇧Z / Ctrl+Y",
            Self::Cut => "⌘X / Ctrl+X",
            Self::Copy => "⌘C / Ctrl+C",
            Self::Paste => "⌘V / Ctrl+V",
            Self::SelectAll => "⌘A / Ctrl+A",
            Self::Dismiss => "",
        }
    }

    fn selector(self) -> &'static str {
        match self {
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::Cut => "cut",
            Self::Copy => "copy",
            Self::Paste => "paste",
            Self::SelectAll => "select-all",
            Self::Dismiss => "dismiss",
        }
    }
}

pub const EDITABLE_ACTIONS: &[EditContextAction] = &[
    EditContextAction::Undo,
    EditContextAction::Redo,
    EditContextAction::Cut,
    EditContextAction::Copy,
    EditContextAction::Paste,
    EditContextAction::SelectAll,
];

/// Password-like fields can be changed and selected, but their plaintext is never copied or cut
/// into the system clipboard.
pub const MASKED_EDITABLE_ACTIONS: &[EditContextAction] = &[
    EditContextAction::Undo,
    EditContextAction::Redo,
    EditContextAction::Paste,
    EditContextAction::SelectAll,
];

pub const READ_ONLY_ACTIONS: &[EditContextAction] =
    &[EditContextAction::Copy, EditContextAction::SelectAll];

/// Builds a lightweight, window-level edit menu without pulling in Zed's full `ui` crate.
///
/// The transparent deferred layer dismisses the menu on an outside click. The visible menu is
/// painted one priority above it so its items continue to receive mouse events.
pub fn edit_context_menu<V: 'static>(
    position: Point<Pixels>,
    id: &'static str,
    actions: &'static [EditContextAction],
    on_action: fn(&mut V, EditContextAction, &mut Window, &mut Context<V>),
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let viewport = window.viewport_size();
    let overlay_selector = format!("{id}-overlay");
    let overlay = deferred(
        anchored()
            .anchor(Anchor::TopLeft)
            .position(point(px(0.0), px(0.0)))
            .child(
                div()
                    .debug_selector(move || overlay_selector.clone())
                    .w(viewport.width)
                    .h(viewport.height)
                    .occlude()
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            on_action(this, EditContextAction::Dismiss, window, cx);
                        }),
                    ),
            ),
    )
    .with_priority(100);

    let menu_selector = id.to_string();
    let menu = deferred(
        anchored()
            .anchor(Anchor::TopLeft)
            .position(position)
            .snap_to_window_with_margin(px(8.0))
            .child(
                div()
                    .id(id)
                    .debug_selector(move || menu_selector.clone())
                    .occlude()
                    .w(px(190.0))
                    .p_1()
                    .bg(rgb(PANEL))
                    .border_1()
                    .border_color(rgb(LINE))
                    .rounded_lg()
                    .shadow_lg()
                    .font_family(FONT_UI)
                    .children(actions.iter().copied().enumerate().map(|(index, action)| {
                        let item_selector = format!("{id}-{}", action.selector());
                        div()
                            .id((id, index))
                            .debug_selector(move || item_selector.clone())
                            .h(px(30.0))
                            .px_2()
                            .flex()
                            .items_center()
                            .justify_between()
                            .rounded_md()
                            .cursor_pointer()
                            .text_size(px(12.0))
                            .text_color(rgb(TEXT))
                            .hover(|style| style.bg(rgb(PANEL_ALT)))
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    on_action(this, action, window, cx);
                                }),
                            )
                            .child(div().font_weight(FontWeight::MEDIUM).child(action.label()))
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(rgb(if action == EditContextAction::Paste {
                                        SUBTEXT
                                    } else {
                                        MUTED
                                    }))
                                    .child(action.shortcut()),
                            )
                    })),
            ),
    )
    .with_priority(101);

    div().child(overlay).child(menu).into_any_element()
}

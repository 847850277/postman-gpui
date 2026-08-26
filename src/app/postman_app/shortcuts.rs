use super::PostmanApp;
use crate::{
    app::{
        ActivateControl, ActivateNextRequest, ActivatePreviousRequest, CloseRequest,
        DismissOverlay, FocusHistorySearch, FocusNextControl, FocusPreviousControl, FocusUrl,
        NewRequest, SendOrCancel, ToggleShortcutHelp,
    },
    ui::theme::{
        ACCENT, ACCENT_SOFT, FONT_HEADING, FONT_MONO, FONT_UI, MUTED, PANEL, PANEL_ALT, SUBTEXT,
        TEXT,
    },
};
use gpui::{
    div, px, rgb, Context, FontWeight, InteractiveElement, IntoElement, ParentElement, Role,
    StatefulInteractiveElement, Styled, Window,
};

const SHORTCUTS: [(&str, &str); 9] = [
    ("Send / cancel active request", "⌘/Ctrl + Enter"),
    ("New request tab", "⌘/Ctrl + T"),
    ("Close active request tab", "⌘/Ctrl + W"),
    ("Focus request URL", "⌘/Ctrl + L"),
    ("Focus History search", "⌘/Ctrl + Shift + F"),
    ("Search requests and history", "⌘/Ctrl + K"),
    ("Next request tab", "Ctrl + Tab"),
    ("Previous request tab", "Ctrl + Shift + Tab"),
    ("Open / close this help", "⌘/Ctrl + /"),
];

impl PostmanApp {
    pub(super) fn focus_next_control(
        &mut self,
        _: &FocusNextControl,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus_next(cx);
    }

    pub(super) fn focus_previous_control(
        &mut self,
        _: &FocusPreviousControl,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus_prev(cx);
    }

    pub(super) fn send_or_cancel(
        &mut self,
        _: &SendOrCancel,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_workspace
            .update(cx, |workspace, cx| workspace.send_or_cancel(cx));
    }

    pub(super) fn new_request_command(
        &mut self,
        _: &NewRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.new_request(cx);
        self.request_workspace.update(cx, |workspace, cx| {
            workspace.focus_active_request_tab(window, cx)
        });
    }

    pub(super) fn close_request_command(
        &mut self,
        _: &CloseRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_workspace.update(cx, |workspace, cx| {
            workspace.close_active_request(window, cx)
        });
    }

    pub(super) fn focus_url(&mut self, _: &FocusUrl, window: &mut Window, cx: &mut Context<Self>) {
        self.request_workspace
            .update(cx, |workspace, cx| workspace.focus_url(window, cx));
    }

    pub(super) fn focus_history_search(
        &mut self,
        _: &FocusHistorySearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.history_list
            .update(cx, |history, cx| history.focus_search(window, cx));
    }

    pub(super) fn activate_next_request(
        &mut self,
        _: &ActivateNextRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_workspace.update(cx, |workspace, cx| {
            workspace.activate_relative_request(1, window, cx)
        });
    }

    pub(super) fn activate_previous_request(
        &mut self,
        _: &ActivatePreviousRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_workspace.update(cx, |workspace, cx| {
            workspace.activate_relative_request(-1, window, cx)
        });
    }

    pub(super) fn toggle_shortcut_help(
        &mut self,
        _: &ToggleShortcutHelp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.shortcut_help_open {
            self.close_shortcut_help(window, cx);
        } else {
            self.shortcut_help_return_focus = window.focused(cx).map(|focus| focus.downgrade());
            self.shortcut_help_open = true;
            self.shortcut_help_focus.focus(window, cx);
            cx.notify();
        }
    }

    fn close_shortcut_help(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.shortcut_help_open = false;
        self.shortcut_help_return_focus
            .take()
            .and_then(|focus| focus.upgrade())
            .unwrap_or_else(|| self.app_focus_handle.clone())
            .focus(window, cx);
        cx.notify();
    }

    pub(super) fn dismiss_overlay(
        &mut self,
        _: &DismissOverlay,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.shortcut_help_open {
            self.close_shortcut_help(window, cx);
        } else if self.cookie_jar_open {
            self.cookie_jar_open = false;
            self.cookie_trigger_focus.focus(window, cx);
            cx.notify();
        }
    }

    pub(super) fn render_shortcut_help(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.shortcut_help_focus.clone();
        let mouse_focus = focus.clone();
        div()
            .debug_selector(|| "shortcut-help-backdrop".into())
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x0000_0000_0099))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| this.close_shortcut_help(window, cx)),
            )
            .child(
                div()
                    .id("shortcut-help-dialog")
                    .debug_selector(|| "shortcut-help-dialog".into())
                    .track_focus(&focus)
                    .key_context("KeyboardButton ShortcutHelp")
                    .role(Role::Dialog)
                    .aria_label("Keyboard shortcuts")
                    .w(px(520.0))
                    .p_5()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .rounded(px(14.0))
                    .border_1()
                    .border_color(rgb(ACCENT))
                    .bg(rgb(PANEL))
                    .on_mouse_up(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_action(cx.listener(Self::dismiss_overlay))
                    .on_action(cx.listener(
                        |this, _: &ActivateControl, window, cx| {
                            this.close_shortcut_help(window, cx)
                        },
                    ))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .child(
                                div()
                                    .flex_1()
                                    .font_family(FONT_HEADING)
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(20.0))
                                    .text_color(rgb(TEXT))
                                    .child("Keyboard shortcuts"),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "shortcut-help-close".into())
                                    .size(px(30.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_lg()
                                    .bg(rgb(ACCENT_SOFT))
                                    .text_color(rgb(ACCENT))
                                    .font_family(FONT_UI)
                                    .font_weight(FontWeight::BOLD)
                                    .cursor_pointer()
                                    .child("×")
                                    .on_mouse_up(
                                        gpui::MouseButton::Left,
                                        cx.listener(|this, _, window, cx| {
                                            this.close_shortcut_help(window, cx)
                                        }),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .font_family(FONT_UI)
                            .text_size(px(12.0))
                            .text_color(rgb(SUBTEXT))
                            .child(
                                "Text fields keep standard selection, clipboard, Undo/Redo, and word-navigation shortcuts.",
                            ),
                    )
                    .children(SHORTCUTS.into_iter().map(|(command, shortcut)| {
                        div()
                            .h(px(34.0))
                            .flex()
                            .items_center()
                            .px_3()
                            .rounded_lg()
                            .bg(rgb(PANEL_ALT))
                            .child(
                                div()
                                    .flex_1()
                                    .font_family(FONT_UI)
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT))
                                    .child(command),
                            )
                            .child(
                                div()
                                    .font_family(FONT_MONO)
                                    .text_size(px(11.0))
                                    .text_color(rgb(MUTED))
                                    .child(shortcut),
                            )
                    }))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        move |_, window, cx| mouse_focus.focus(window, cx),
                    ),
            )
    }
}

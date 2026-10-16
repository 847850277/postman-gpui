use super::{navigation::AppRoute, PostmanApp};
use crate::app::{
    ActivateNextRequest, ActivatePreviousRequest, CloseRequest, DismissOverlay, FocusHistorySearch,
    FocusNextControl, FocusPreviousControl, FocusUrl, NewRequest, SendOrCancel, ToggleShortcutHelp,
};
use crate::ui::theme::{metrics as m, FONT_MONO, LINE, MUTED};
use gpui::{div, Context, InteractiveElement, ParentElement, Styled, Window};
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    IconName, Sizable, WindowExt,
};

impl PostmanApp {
    pub(super) fn save_request_command(
        &mut self,
        _: &crate::app::SaveRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route == AppRoute::Flows {
            self.flows.update(cx, |flows, cx| {
                flows.save_flow(super::flows::SaveKind::Save, window, cx)
            });
        } else if self.route == AppRoute::Http {
            self.saved_requests
                .update(cx, |saved, cx| saved.save_active(false, window, cx));
        }
    }
    pub(super) fn rename_request_command(
        &mut self,
        _: &crate::app::RenameRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route == AppRoute::Http {
            self.saved_requests
                .update(cx, |saved, cx| saved.save_active(true, window, cx));
        }
    }
    pub(super) fn quit_command(
        &mut self,
        _: &crate::app::Quit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_quit(window, cx);
    }

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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.route {
            AppRoute::Http => self
                .request_workspace
                .update(cx, |workspace, cx| workspace.send_or_cancel(window, cx)),
            AppRoute::Flows => self
                .flows
                .update(cx, |flows, cx| flows.execution_shortcut(window, cx)),
            AppRoute::Home => {}
        }
    }

    pub(super) fn new_request_command(
        &mut self,
        _: &NewRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate(AppRoute::Http, window, cx);
        self.new_request(window, cx);
        self.request_workspace.update(cx, |workspace, cx| {
            workspace.focus_active_request_tab(window, cx)
        });
    }

    pub(super) fn open_flow_command(
        &mut self,
        _: &crate::app::keyboard::OpenFlow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route == AppRoute::Flows {
            self.flows
                .update(cx, |flows, cx| flows.open_flow(window, cx));
        }
    }
    pub(super) fn save_flow_as_command(
        &mut self,
        _: &crate::app::keyboard::SaveFlowAs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route == AppRoute::Flows {
            self.flows.update(cx, |flows, cx| {
                flows.save_flow(super::flows::SaveKind::SaveAs, window, cx)
            });
        }
    }
    pub(super) fn close_request_command(
        &mut self,
        _: &CloseRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route == AppRoute::Flows {
            self.flows
                .update(cx, |flows, cx| flows.close_flow(window, cx));
            return;
        }
        if self.route != AppRoute::Http {
            return;
        }
        self.request_workspace.update(cx, |workspace, cx| {
            workspace.close_active_request(window, cx)
        });
    }

    pub(super) fn focus_url(&mut self, _: &FocusUrl, window: &mut Window, cx: &mut Context<Self>) {
        if self.route != AppRoute::Http {
            return;
        }
        self.request_workspace
            .update(cx, |workspace, cx| workspace.focus_url(window, cx));
    }

    pub(super) fn focus_history_search(
        &mut self,
        _: &FocusHistorySearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_finder(super::FinderKind::History, window, cx);
    }

    pub(super) fn activate_next_request(
        &mut self,
        _: &ActivateNextRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route != AppRoute::Http {
            return;
        }
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
        if self.route != AppRoute::Http {
            return;
        }
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
        if window.has_active_dialog(cx) {
            return;
        }
        self.active_tool = Some(super::ToolDialog::Help);
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            let owner = owner.clone();
            let modifier = if cfg!(target_os = "macos") { "⌘" } else { "Ctrl" };
            let shortcuts = [
                ("Send / cancel HTTP; Run / Cancel Flow canvas", format!("{modifier} + Enter")),
                ("Save active HTTP request or Flow", format!("{modifier} + S")),
                ("Open Flow file (Flows)", format!("{modifier} + O")),
                ("Save Flow as (Flows)", format!("{modifier} + Shift + S")),
                ("New request tab", format!("{modifier} + T")),
                ("Close active HTTP request or Flow", format!("{modifier} + W")),
                ("Focus request URL (HTTP)", format!("{modifier} + L")),
                ("History", format!("{modifier} + Shift + F")),
                ("Search requests and history", format!("{modifier} + K")),
                ("Next request tab (HTTP)", "Ctrl + Tab".into()),
                ("Previous request tab (HTTP)", "Ctrl + Shift + Tab".into()),
                ("Keyboard shortcuts", format!("{modifier} + /")),
                ("Close topmost dialog or menu", "Esc".into()),
            ];
            let rows = shortcuts.into_iter().map(|(label, key)| {
                div().flex().items_center().gap_4().py_3()
                    .border_b_1().border_color(LINE.resolve(cx)).text_size(m::LABEL)
                    .child(div().flex_1().child(label))
                    .child(div().font_family(FONT_MONO).text_color(MUTED.resolve(cx)).child(key))
            });
            let help = "In text fields: use standard selection, copy/paste, Undo/Redo, and word navigation. \
                        In search results: ↑ / ↓ selects and Enter opens. \
                        Background editor commands are unavailable while a dialog is open.";
            dialog
                .title(div().text_size(m::BODY).font_weight(m::BOLD).child("Keyboard shortcuts"))
                .w(window.rem_size() * 35.)
                .close_button(true)
                .child(div().debug_selector(|| "shortcut-help-dialog".into()).flex().flex_col().children(rows))
                .child(div().py_3().text_size(m::LABEL).text_color(MUTED.resolve(cx)).child(help))
                .footer(
                    Button::new("shortcut-help-close")
                        .debug_selector(|| "shortcut-help-close".into())
                        .small().ghost().icon(IconName::Check).label("Done")
                        .on_click(|_, w, cx| w.dispatch_action(Box::new(gpui_kit::base::actions::Cancel), cx))
                )
                .on_ok(|_, _, _| false)
                .on_close(move |_, _, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        this.active_tool = None;
                        cx.notify();
                    });
                })
        });
        cx.notify();
    }
    pub(super) fn dismiss_overlay(
        &mut self,
        _: &DismissOverlay,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.cookie_jar_open {
            self.cookie_jar_open = false;
            self.cookie_trigger_focus.focus(window, cx);
            cx.notify();
        }
    }
}

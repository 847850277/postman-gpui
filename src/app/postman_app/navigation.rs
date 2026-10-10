use super::PostmanApp;
use crate::app::RequestTabId;
use gpui::{Context, Window};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AppRoute {
    Home,
    Http,
    Flows,
}

impl AppRoute {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Http => "HTTP requests",
            Self::Flows => "Flows",
        }
    }
}

impl PostmanApp {
    /// Change only the visible surface. The editor, runner and Flows entities remain owned
    /// by the window, so navigation cannot cancel a request or discard its controls/draft.
    pub(super) fn navigate(
        &mut self,
        route: AppRoute,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.route = route;
        self.cookie_jar_open = false;
        self.shortcut_help_open = false;
        self.shortcut_help_return_focus = None;
        self.global_search_return_focus = None;
        self.reset_global_search(cx);
        match route {
            AppRoute::Home => self.app_focus_handle.focus(window, cx),
            AppRoute::Http => self.request_workspace.update(cx, |workspace, cx| {
                workspace.focus_active_request_tab(window, cx)
            }),
            AppRoute::Flows => self.flows.read(cx).focus_handle().focus(window, cx),
        }
        cx.notify();
    }

    pub(super) fn resume_request(
        &mut self,
        id: RequestTabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.view_model.read(cx).request_for_tab(id).is_none() {
            return;
        }
        self.request_workspace
            .update(cx, |workspace, cx| workspace.activate_request_tab(id, cx));
        self.navigate(AppRoute::Http, window, cx);
    }
}

use crate::{
    app::{request_runner::RequestRunner, WorkspaceViewModel},
    ui::theme::{BG, FONT_UI, LINE, PANEL, TEXT},
};
use gpui::{
    div, prelude::FluentBuilder, px, rgb, AppContext, Context, Entity, InteractiveElement,
    IntoElement, ParentElement, Render, Styled, Subscription, Window,
};

mod chrome;
mod history_panel;
mod request_workspace;

use history_panel::{HistoryList, HistoryListEvent};
use request_workspace::{CookiePane, CookiePaneEvent, RequestWorkspace, RequestWorkspaceEvent};

/// Application composition root. Feature-specific controls and task lifetimes live in child
/// entities; this type only wires the shell together.
pub struct PostmanApp {
    view_model: Entity<WorkspaceViewModel>,
    request_workspace: Entity<RequestWorkspace>,
    request_runner: Entity<RequestRunner>,
    history_list: Entity<HistoryList>,
    cookie_pane: Entity<CookiePane>,
    cookie_jar_open: bool,
    _subscriptions: Vec<Subscription>,
}

impl PostmanApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let view_model = cx.new(|_| WorkspaceViewModel::new());
        Self::with_view_model(view_model, cx)
    }

    /// Dependency-injected constructor used by app hosts and black-box UI tests that need to
    /// observe the ViewModel without mutating the View through a second command surface.
    pub fn with_view_model(view_model: Entity<WorkspaceViewModel>, cx: &mut Context<Self>) -> Self {
        let request_workspace = cx.new(|cx| RequestWorkspace::new(view_model.clone(), cx));
        let request_runner = cx.new(|_| RequestRunner::new());
        let history_list = cx.new(|cx| HistoryList::new(view_model.clone(), cx));
        let cookie_pane = cx.new(|cx| CookiePane::new(view_model.clone(), cx));
        let subscriptions = vec![
            cx.subscribe(&request_workspace, Self::on_request_workspace_event),
            cx.subscribe(&history_list, Self::on_history_selected),
            cx.subscribe(&cookie_pane, Self::on_cookie_pane_event),
        ];

        Self {
            view_model,
            request_workspace,
            request_runner,
            history_list,
            cookie_pane,
            cookie_jar_open: false,
            _subscriptions: subscriptions,
        }
    }

    fn on_request_workspace_event(
        &mut self,
        _workspace: Entity<RequestWorkspace>,
        event: &RequestWorkspaceEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            RequestWorkspaceEvent::Execute(pending) => {
                let pending = pending.clone();
                let view_model = self.view_model.clone();
                self.request_runner
                    .update(cx, |runner, cx| runner.execute(pending, view_model, cx));
            }
            RequestWorkspaceEvent::Abort(send_id) => {
                self.request_runner
                    .update(cx, |runner, _| runner.abort(*send_id));
            }
            RequestWorkspaceEvent::OpenCookieJar => self.open_cookie_jar(cx),
        }
    }

    fn on_cookie_pane_event(
        &mut self,
        _pane: Entity<CookiePane>,
        event: &CookiePaneEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            CookiePaneEvent::ClearAllRequested => {
                let cleared = self
                    .request_runner
                    .update(cx, |runner, _| runner.clear_cookies());
                self.view_model.update(cx, |view_model, cx| {
                    view_model.record_cookies_cleared(cleared);
                    cx.notify();
                });
            }
            CookiePaneEvent::CloseRequested => {
                self.cookie_jar_open = false;
                cx.notify();
            }
        }
    }

    pub(super) fn toggle_cookie_jar(&mut self, cx: &mut Context<Self>) {
        self.cookie_jar_open = !self.cookie_jar_open;
        cx.notify();
    }

    fn open_cookie_jar(&mut self, cx: &mut Context<Self>) {
        self.cookie_jar_open = true;
        cx.notify();
    }

    fn on_history_selected(
        &mut self,
        _list: Entity<HistoryList>,
        event: &HistoryListEvent,
        cx: &mut Context<Self>,
    ) {
        let HistoryListEvent::RequestSelected(entry) = event;
        self.request_workspace
            .update(cx, |workspace, cx| workspace.load_history_entry(entry, cx));
    }

    fn new_request(&mut self, cx: &mut Context<Self>) {
        self.request_workspace
            .update(cx, RequestWorkspace::new_request);
    }
}

impl Render for PostmanApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("main-container")
            .relative()
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .font_family(FONT_UI)
            .child(self.render_top_header(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.render_left_rail(cx))
                    .child(self.history_list.clone())
                    .child(self.request_workspace.clone()),
            )
            .when(self.cookie_jar_open, |root| {
                root.child(
                    div()
                        .debug_selector(|| "cookie-jar-workspace-overlay".into())
                        .absolute()
                        .top(px(80.0))
                        .right(px(16.0))
                        .w(px(640.0))
                        .h(px(360.0))
                        .rounded(px(14.0))
                        .border_1()
                        .border_color(rgb(LINE))
                        .bg(rgb(PANEL))
                        .overflow_hidden()
                        .child(self.cookie_pane.clone()),
                )
            })
    }
}

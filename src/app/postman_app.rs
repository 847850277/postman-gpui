use crate::{
    app::{
        request_runner::RequestRunner, setup_application_key_bindings,
        spawn_history_operation_and_reload, HistoryStorageStage, WorkspaceViewModel,
    },
    persistence::{
        HistoryRepositoryWorker, SqliteHistoryRepository, DEFAULT_HISTORY_RETENTION_LIMIT,
    },
    ui::theme::{BG, FONT_UI, LINE, PANEL, TEXT},
};
use gpui::{
    div, prelude::FluentBuilder, px, AppContext, Context, Entity, FocusHandle, InteractiveElement,
    IntoElement, ParentElement, Render, Styled, Subscription, Window,
};
use std::{fs, path::PathBuf, sync::Arc};
use uuid::Uuid;

mod chrome;
mod flows;
mod home;
mod navigation;
mod request_finder;
mod request_workspace;
mod shortcuts;

use flows::FlowsView;
use gpui_kit::component::WindowExt;
use navigation::AppRoute;
use request_finder::{FinderKind, RequestFinder, RequestFinderEvent, RequestTarget};
use request_workspace::{CookiePane, CookiePaneEvent, RequestWorkspace, RequestWorkspaceEvent};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToolDialog {
    History,
    Search,
    Help,
}

/// Application composition root. Feature-specific controls and task lifetimes live in child
/// entities; this type only wires the shell together.
pub struct PostmanApp {
    view_model: Entity<WorkspaceViewModel>,
    route: AppRoute,
    flows: Entity<FlowsView>,
    home_scroll: gpui::ScrollHandle,
    active_tool: Option<ToolDialog>,
    request_workspace: Entity<RequestWorkspace>,
    request_runner: Entity<RequestRunner>,
    history_list: Entity<RequestFinder>,
    history_worker: Option<Arc<HistoryRepositoryWorker>>,
    _temporary_history_database: Option<TemporaryHistoryDatabase>,
    cookie_pane: Entity<CookiePane>,
    global_search: Entity<RequestFinder>,
    cookie_jar_open: bool,
    app_focus_handle: FocusHandle,
    cookie_trigger_focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl PostmanApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let view_model = cx.new(|_| WorkspaceViewModel::new());
        Self::compose(
            view_model,
            SqliteHistoryRepository::production(),
            None,
            window,
            cx,
        )
    }

    /// Dependency-injected constructor used by app hosts and black-box UI tests that need to
    /// observe the ViewModel without mutating the View through a second command surface.
    pub fn with_view_model(
        view_model: Entity<WorkspaceViewModel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let temporary_database = TemporaryHistoryDatabase::new();
        let repository = SqliteHistoryRepository::new(temporary_database.path());
        Self::compose(view_model, repository, Some(temporary_database), window, cx)
    }

    /// Construct the application around an explicit file-backed SQLite database. This is useful
    /// for restart/lifecycle tests and alternate app hosts; production uses the platform path.
    pub fn with_view_model_and_history_path(
        view_model: Entity<WorkspaceViewModel>,
        path: impl Into<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::compose(
            view_model,
            SqliteHistoryRepository::new(path),
            None,
            window,
            cx,
        )
    }

    fn compose(
        view_model: Entity<WorkspaceViewModel>,
        repository: Result<SqliteHistoryRepository, crate::persistence::HistoryRepositoryError>,
        temporary_history_database: Option<TemporaryHistoryDatabase>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.bind_keys(setup_application_key_bindings());
        let history_worker = repository
            .and_then(HistoryRepositoryWorker::start)
            .map(Arc::new);
        let history_worker = match history_worker {
            Ok(worker) => Some(worker),
            Err(error) => {
                view_model.update(cx, |view_model, cx| {
                    view_model.set_history_storage_error(
                        HistoryStorageStage::Initialize,
                        error.to_string(),
                    );
                    cx.notify();
                });
                None
            }
        };
        let request_workspace = cx.new(|cx| RequestWorkspace::new(view_model.clone(), window, cx));
        let runner_history_worker = history_worker.clone();
        let request_runner = cx.new(move |_| RequestRunner::new(runner_history_worker));
        let history_list =
            cx.new(|cx| RequestFinder::new(FinderKind::History, view_model.clone(), window, cx));
        let cookie_pane = cx.new(|cx| CookiePane::new(view_model.clone(), cx));
        let global_search =
            cx.new(|cx| RequestFinder::new(FinderKind::Search, view_model.clone(), window, cx));
        let subscriptions = vec![
            cx.subscribe(&request_workspace, Self::on_request_workspace_event),
            cx.subscribe_in(&history_list, window, Self::on_finder_event),
            cx.subscribe(&cookie_pane, Self::on_cookie_pane_event),
            cx.subscribe_in(&global_search, window, Self::on_finder_event),
            cx.observe(&view_model, |_, _, cx| cx.notify()),
        ];

        let flows = cx.new(FlowsView::new);
        let app = Self {
            route: AppRoute::Home,
            flows,
            home_scroll: gpui::ScrollHandle::new(),
            active_tool: None,
            view_model,
            request_workspace,
            request_runner,
            history_list,
            history_worker,
            _temporary_history_database: temporary_history_database,
            cookie_pane,
            global_search,
            cookie_jar_open: false,
            app_focus_handle: cx.focus_handle(),
            cookie_trigger_focus: cx.focus_handle().tab_index(0).tab_stop(true),
            _subscriptions: subscriptions,
        };
        if let Some(worker) = app.history_worker.clone() {
            Self::initialize_and_load_history(app.view_model.clone(), worker, cx);
        }
        app
    }

    fn initialize_and_load_history(
        view_model: Entity<WorkspaceViewModel>,
        worker: Arc<HistoryRepositoryWorker>,
        cx: &mut Context<Self>,
    ) {
        let initialize = worker.initialize();
        // Both commands are queued immediately, so load can never overtake schema initialization.
        let load = worker.load_recent(DEFAULT_HISTORY_RETENTION_LIMIT);
        spawn_history_operation_and_reload(
            view_model,
            HistoryStorageStage::Initialize,
            initialize,
            load,
            None,
            cx,
        );
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

    pub(super) fn toggle_cookie_jar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cookie_jar_open = !self.cookie_jar_open;
        if self.cookie_jar_open {
            self.cookie_pane
                .update(cx, |pane, cx| pane.focus_first(window, cx));
        } else {
            self.cookie_trigger_focus.focus(window, cx);
        }
        cx.notify();
    }

    fn open_cookie_jar(&mut self, cx: &mut Context<Self>) {
        self.cookie_jar_open = true;
        cx.notify();
    }

    fn on_finder_event(
        &mut self,
        _: &Entity<RequestFinder>,
        event: &RequestFinderEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            RequestFinderEvent::Activate(target) => {
                // Resolve against the authoritative model, never a stale row index.
                match target {
                    RequestTarget::Request(id) => {
                        if self.view_model.read(cx).request_for_tab(*id).is_none() {
                            return;
                        }
                        window.close_dialog(cx);
                        self.active_tool = None;
                        self.resume_request(*id, window, cx);
                    }
                    RequestTarget::History(id) => {
                        let entry = self
                            .view_model
                            .read(cx)
                            .history()
                            .iter()
                            .find(|e| e.id == *id)
                            .cloned();
                        if let Some(entry) = entry {
                            window.close_dialog(cx);
                            self.active_tool = None;
                            self.navigate(AppRoute::Http, window, cx);
                            self.request_workspace.update(cx, |workspace, cx| {
                                workspace.load_history_entry(&entry, window, cx);
                                workspace.focus_url(window, cx);
                            });
                        }
                    }
                }
            }
            RequestFinderEvent::Closed => {
                self.active_tool = None;
                cx.notify();
            }
            RequestFinderEvent::Refresh => self.refresh_history(cx),
            RequestFinderEvent::Clear => self.clear_history(cx),
        }
    }

    fn open_finder(&mut self, kind: FinderKind, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        self.active_tool = Some(match kind {
            FinderKind::History => ToolDialog::History,
            FinderKind::Search => ToolDialog::Search,
        });
        let finder = match kind {
            FinderKind::History => &self.history_list,
            FinderKind::Search => &self.global_search,
        };
        finder.update(cx, |finder, cx| finder.open(window, cx));
        cx.notify();
    }

    fn refresh_history(&mut self, cx: &mut Context<Self>) {
        let Some(worker) = self.history_worker.clone() else {
            self.view_model.update(cx, |view_model, cx| {
                view_model.set_history_storage_error(
                    HistoryStorageStage::Load,
                    "SQLite History is unavailable",
                );
                cx.notify();
            });
            return;
        };
        Self::initialize_and_load_history(self.view_model.clone(), worker, cx);
    }

    fn clear_history(&mut self, cx: &mut Context<Self>) {
        let Some(worker) = self.history_worker.clone() else {
            self.view_model.update(cx, |view_model, cx| {
                view_model.set_history_storage_error(
                    HistoryStorageStage::Clear,
                    "SQLite History is unavailable",
                );
                cx.notify();
            });
            return;
        };
        let clear = worker.clear();
        let load = worker.load_recent(DEFAULT_HISTORY_RETENTION_LIMIT);
        spawn_history_operation_and_reload(
            self.view_model.clone(),
            HistoryStorageStage::Clear,
            clear,
            load,
            None,
            cx,
        );
    }

    fn new_request(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.request_workspace
            .update(cx, |workspace, cx| workspace.new_request(window, cx));
    }
}

struct TemporaryHistoryDatabase {
    directory: PathBuf,
}

impl TemporaryHistoryDatabase {
    fn new() -> Self {
        Self {
            directory: std::env::temp_dir()
                .join(format!("postman-gpui-history-test-{}", Uuid::new_v4())),
        }
    }

    fn path(&self) -> PathBuf {
        self.directory.join("request-history.sqlite3")
    }
}

impl Drop for TemporaryHistoryDatabase {
    fn drop(&mut self) {
        let is_owned_test_directory = self
            .directory
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("postman-gpui-history-test-"))
            && self.directory.parent() == Some(std::env::temp_dir().as_path());
        if is_owned_test_directory {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }
}

impl Render for PostmanApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if window.focused(cx).is_none() {
            self.app_focus_handle.focus(window, cx);
        }
        div()
            .id("main-container")
            .debug_selector(|| "main-container".into())
            .relative()
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .bg(BG.resolve(cx))
            .text_color(TEXT.resolve(cx))
            .font_family(FONT_UI)
            .track_focus(&self.app_focus_handle)
            .key_context("PostmanApp")
            .on_action(cx.listener(Self::send_or_cancel))
            .on_action(cx.listener(Self::focus_next_control))
            .on_action(cx.listener(Self::focus_previous_control))
            .on_action(cx.listener(Self::new_request_command))
            .on_action(cx.listener(Self::close_request_command))
            .on_action(cx.listener(Self::focus_url))
            .on_action(cx.listener(Self::focus_history_search))
            .on_action(cx.listener(Self::focus_global_search))
            .on_action(cx.listener(Self::activate_next_request))
            .on_action(cx.listener(Self::activate_previous_request))
            .on_action(cx.listener(Self::toggle_shortcut_help))
            .on_action(cx.listener(Self::dismiss_overlay))
            .child(self.render_top_header(window, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .flex()
                    .child(self.render_left_rail(window, cx))
                    .child(match self.route {
                        AppRoute::Home => self.render_home(window, cx).into_any_element(),
                        AppRoute::Http => self.render_http().into_any_element(),
                        AppRoute::Flows => self.flows.clone().into_any_element(),
                    }),
            )
            .child(self.render_status_bar(cx))
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
                        .border_color(LINE.resolve(cx))
                        .bg(PANEL.resolve(cx))
                        .overflow_hidden()
                        .child(self.cookie_pane.clone()),
                )
            })
    }
}

impl PostmanApp {
    fn render_http(&self) -> impl IntoElement {
        div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .child(self.request_workspace.clone())
    }
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext, TestAppContext};

    #[gpui_kit::test]
    fn auth_and_options_cannot_undo_into_another_equal_valued_tab(cx: &mut TestAppContext) {
        use crate::app::{AuthorizationKind, PostmanApp, RequestPane, WorkspaceViewModel};
        use gpui::{Modifiers, VisualTestContext};

        fn click(cx: &mut VisualTestContext, selector: &'static str) {
            let bounds = cx.debug_bounds(selector).expect(selector);
            cx.simulate_click(bounds.center(), Modifiers::none());
            let _ = cx.debug_bounds(selector);
        }

        fn field_value(model: &WorkspaceViewModel, selector: &str) -> String {
            let request = model.active_request().unwrap();
            match selector {
                "authorization-input" => request.bearer_token().to_owned(),
                "basic-auth-username-input" => request.basic_username().to_owned(),
                "basic-auth-password-input" => request.basic_password().to_owned(),
                "request-timeout-input" => request.timeout_ms().to_string(),
                "redirect-max-hops-input" => request.max_redirect_hops().to_string(),
                _ => unreachable!(),
            }
        }

        cx.update(crate::ui::kit::init);
        type InputScenario = (
            RequestPane,
            AuthorizationKind,
            &'static [(&'static str, &'static str)],
        );
        let scenarios: &[InputScenario] = &[
            (
                RequestPane::Authorization,
                AuthorizationKind::Bearer,
                &[("authorization-input", "shared-token")],
            ),
            (
                RequestPane::Authorization,
                AuthorizationKind::Basic,
                &[
                    ("basic-auth-username-input", "shared-user"),
                    ("basic-auth-password-input", "shared-password"),
                ],
            ),
            (
                RequestPane::Options,
                AuthorizationKind::Bearer,
                &[
                    ("request-timeout-input", "5000"),
                    ("redirect-max-hops-input", "7"),
                ],
            ),
        ];
        for &(pane, kind, fields) in scenarios {
            let model = cx.new(|_| {
                let mut model = WorkspaceViewModel::new();
                for index in 0..2 {
                    if index > 0 {
                        model.new_request();
                    }
                    let request = model.active_request_mut().unwrap();
                    request.set_request_pane(pane);
                    request.set_authorization_kind(kind);
                    request.set_bearer_token(if index == 0 {
                        "old-token"
                    } else {
                        "shared-token"
                    });
                    request.set_basic_username(if index == 0 {
                        "old-user"
                    } else {
                        "shared-user"
                    });
                    request.set_basic_password(if index == 0 {
                        "old-password"
                    } else {
                        "shared-password"
                    });
                    request.set_timeout_ms(if index == 0 { 2500 } else { 5000 });
                    request.set_max_redirect_hops(if index == 0 { 3 } else { 7 });
                }
                model.select_tab(0);
                model
            });
            let observed = model.clone();
            let (_, visual) = cx.add_window_view(move |window, cx| {
                let app = cx.new(|cx| PostmanApp::with_view_model(observed, window, cx));
                gpui_kit::component::Root::new(app, window, cx)
            });
            click(visual, "nav-http");
            for &(selector, value) in fields {
                click(visual, selector);
                visual.simulate_keystrokes("ctrl-a");
                visual.simulate_input(value);
                assert_eq!(
                    model.read_with(visual, |model, _| field_value(model, selector)),
                    value
                );
            }
            // The next tab starts with precisely the values just typed in the first tab.
            // Both directions must reset the reused input histories despite equal text.
            for tab in ["request-tab-1", "request-tab-0"] {
                click(visual, tab);
                for &(selector, value) in fields {
                    click(visual, selector);
                    visual.simulate_keystrokes("ctrl-z");
                    assert_eq!(
                        model.read_with(visual, |model, _| field_value(model, selector)),
                        value,
                        "{selector} inherited another tab's editing history"
                    );
                }
            }
        }
    }
}

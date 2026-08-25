use crate::{
    app::{
        request_runner::RequestRunner, spawn_history_operation_and_reload, HistoryStorageStage,
        WorkspaceViewModel,
    },
    persistence::{
        HistoryRepositoryWorker, SqliteHistoryRepository, DEFAULT_HISTORY_RETENTION_LIMIT,
    },
    ui::theme::{BG, FONT_UI, LINE, PANEL, TEXT},
};
use gpui::{
    div, prelude::FluentBuilder, px, rgb, AppContext, Context, Entity, InteractiveElement,
    IntoElement, ParentElement, Render, Styled, Subscription, Window,
};
use std::{fs, path::PathBuf, sync::Arc};
use uuid::Uuid;

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
    history_worker: Option<Arc<HistoryRepositoryWorker>>,
    _temporary_history_database: Option<TemporaryHistoryDatabase>,
    cookie_pane: Entity<CookiePane>,
    cookie_jar_open: bool,
    _subscriptions: Vec<Subscription>,
}

impl PostmanApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let view_model = cx.new(|_| WorkspaceViewModel::new());
        Self::compose(view_model, SqliteHistoryRepository::production(), None, cx)
    }

    /// Dependency-injected constructor used by app hosts and black-box UI tests that need to
    /// observe the ViewModel without mutating the View through a second command surface.
    pub fn with_view_model(view_model: Entity<WorkspaceViewModel>, cx: &mut Context<Self>) -> Self {
        let temporary_database = TemporaryHistoryDatabase::new();
        let repository = SqliteHistoryRepository::new(temporary_database.path());
        Self::compose(view_model, repository, Some(temporary_database), cx)
    }

    /// Construct the application around an explicit file-backed SQLite database. This is useful
    /// for restart/lifecycle tests and alternate app hosts; production uses the platform path.
    pub fn with_view_model_and_history_path(
        view_model: Entity<WorkspaceViewModel>,
        path: impl Into<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::compose(view_model, SqliteHistoryRepository::new(path), None, cx)
    }

    fn compose(
        view_model: Entity<WorkspaceViewModel>,
        repository: Result<SqliteHistoryRepository, crate::persistence::HistoryRepositoryError>,
        temporary_history_database: Option<TemporaryHistoryDatabase>,
        cx: &mut Context<Self>,
    ) -> Self {
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
        let request_workspace = cx.new(|cx| RequestWorkspace::new(view_model.clone(), cx));
        let runner_history_worker = history_worker.clone();
        let request_runner = cx.new(move |_| RequestRunner::new(runner_history_worker));
        let history_list = cx.new(|cx| HistoryList::new(view_model.clone(), cx));
        let cookie_pane = cx.new(|cx| CookiePane::new(view_model.clone(), cx));
        let subscriptions = vec![
            cx.subscribe(&request_workspace, Self::on_request_workspace_event),
            cx.subscribe(&history_list, Self::on_history_selected),
            cx.subscribe(&cookie_pane, Self::on_cookie_pane_event),
        ];

        let app = Self {
            view_model,
            request_workspace,
            request_runner,
            history_list,
            history_worker,
            _temporary_history_database: temporary_history_database,
            cookie_pane,
            cookie_jar_open: false,
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
        match event {
            HistoryListEvent::RequestSelected(entry) => {
                self.request_workspace
                    .update(cx, |workspace, cx| workspace.load_history_entry(entry, cx));
            }
            HistoryListEvent::RefreshRequested => self.refresh_history(cx),
            HistoryListEvent::ClearRequested => self.clear_history(cx),
        }
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
            cx,
        );
    }

    fn new_request(&mut self, cx: &mut Context<Self>) {
        self.request_workspace
            .update(cx, RequestWorkspace::new_request);
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

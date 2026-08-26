use crate::{
    app::{
        spawn_history_operation_and_reload, HistoryStorageStage, PendingRequest, SendId,
        WorkspaceViewModel,
    },
    http::executor::RequestExecutor,
    models::HistoryEntry,
    persistence::{
        HistoryRepositoryWorker, VersionedHistorySnapshot, DEFAULT_HISTORY_RETENTION_LIMIT,
    },
};
use gpui::{Context, Entity};
use std::{collections::HashMap, sync::Arc};

/// Application service that owns HTTP task lifetimes.
///
/// Views emit immutable request commands; this coordinator executes them and applies the result
/// back to the workspace ViewModel. Keeping abort handles here prevents the composition root and
/// request editor from becoming transport owners.
pub(super) struct RequestRunner {
    executor: RequestExecutor,
    history_worker: Option<Arc<HistoryRepositoryWorker>>,
    in_flight: HashMap<SendId, tokio::task::AbortHandle>,
}

impl RequestRunner {
    pub(super) fn new(history_worker: Option<Arc<HistoryRepositoryWorker>>) -> Self {
        Self {
            executor: RequestExecutor::new(),
            history_worker,
            in_flight: HashMap::new(),
        }
    }

    pub(super) fn execute(
        &mut self,
        pending: PendingRequest,
        view_model: Entity<WorkspaceViewModel>,
        cx: &mut Context<Self>,
    ) {
        let send_id = pending.send_id();
        let request_task = self
            .executor
            .spawn_with_options(pending.request().clone(), pending.request_options());
        self.in_flight.insert(send_id, request_task.abort_handle());

        // Joining the Tokio task on GPUI's background executor keeps transport wake-ups away
        // from the foreground executor and remains deterministic for long-running GPUI tests.
        let result_task = cx
            .background_executor()
            .spawn(async move { request_task.join_on_background_thread() });
        cx.spawn(async move |this, cx| {
            let result = result_task.await;
            let _ = this.update(cx, |this, cx| {
                this.in_flight.remove(&send_id);
                let cookie_snapshot = this.executor.cookie_snapshot();
                let stored_cookies = result
                    .as_ref()
                    .map(|response| response.stored_cookies.clone())
                    .unwrap_or_default();
                let completion = view_model.update(cx, |view_model, cx| {
                    view_model.sync_cookie_jar(cookie_snapshot);
                    let completion = view_model.complete_send_with_stored_cookies(
                        pending,
                        result,
                        stored_cookies,
                    );
                    cx.notify();
                    completion
                });
                if let Some(entry) = completion.history_entry().cloned() {
                    this.persist_history_entry(entry, view_model, cx);
                }
            });
        })
        .detach();
    }

    fn persist_history_entry(
        &self,
        entry: HistoryEntry,
        view_model: Entity<WorkspaceViewModel>,
        cx: &mut Context<Self>,
    ) {
        let Some(worker) = self.history_worker.clone() else {
            view_model.update(cx, |view_model, cx| {
                view_model.set_history_storage_error(
                    HistoryStorageStage::Append,
                    "SQLite History is unavailable",
                );
                cx.notify();
            });
            return;
        };
        let runtime_replay_request = (entry.id.clone(), entry.request.clone());
        let snapshot = match VersionedHistorySnapshot::try_from(&entry) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                view_model.update(cx, |view_model, cx| {
                    view_model
                        .set_history_storage_error(HistoryStorageStage::Append, error.to_string());
                    cx.notify();
                });
                return;
            }
        };

        let append = worker.append_and_trim(snapshot, DEFAULT_HISTORY_RETENTION_LIMIT);
        // Queue the authoritative reload directly behind the append. Awaiting both through GPUI's
        // background executor also keeps worker-thread wakeups away from the foreground executor.
        let load = worker.load_recent(DEFAULT_HISTORY_RETENTION_LIMIT);
        spawn_history_operation_and_reload(
            view_model,
            HistoryStorageStage::Append,
            append,
            load,
            Some(runtime_replay_request),
            cx,
        );
    }

    pub(super) fn abort(&mut self, send_id: SendId) {
        if let Some(handle) = self.in_flight.remove(&send_id) {
            handle.abort();
        }
    }

    pub(super) fn clear_cookies(&self) -> usize {
        self.executor.clear_cookies()
    }

    #[cfg(test)]
    fn in_flight_count(&self) -> usize {
        self.in_flight.len()
    }
}

impl Drop for RequestRunner {
    fn drop(&mut self) {
        for (_, handle) in self.in_flight.drain() {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::AppContext;

    #[gpui::test]
    fn starts_empty(cx: &mut gpui::TestAppContext) {
        let runner = cx.new(|_| RequestRunner::new(None));
        assert_eq!(
            runner.read_with(cx, |runner, _| runner.in_flight_count()),
            0
        );
    }
}

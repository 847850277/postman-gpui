use crate::{
    app::{PendingRequest, SendId, WorkspaceViewModel},
    http::executor::RequestExecutor,
};
use gpui::{Context, Entity};
use std::collections::HashMap;

/// Application service that owns HTTP task lifetimes.
///
/// Views emit immutable request commands; this coordinator executes them and applies the result
/// back to the workspace ViewModel. Keeping abort handles here prevents the composition root and
/// request editor from becoming transport owners.
pub(super) struct RequestRunner {
    executor: RequestExecutor,
    in_flight: HashMap<SendId, tokio::task::AbortHandle>,
}

impl RequestRunner {
    pub(super) fn new() -> Self {
        Self {
            executor: RequestExecutor::new(),
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
        let request_task = self.executor.spawn(pending.request().clone());
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
                view_model.update(cx, |view_model, cx| {
                    view_model.sync_cookie_jar(cookie_snapshot);
                    view_model.complete_send(pending, result);
                    cx.notify();
                });
            });
        })
        .detach();
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
        let runner = cx.new(|_| RequestRunner::new());
        assert_eq!(
            runner.read_with(cx, |runner, _| runner.in_flight_count()),
            0
        );
    }
}

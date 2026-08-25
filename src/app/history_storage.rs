use super::{HistoryStorageStage, WorkspaceViewModel};
use crate::{
    models::{HistoryEntry, Request},
    persistence::{HistoryLoadResult, HistoryLoadWarningKind, HistoryRepositoryTask},
};
use gpui::{Context, Entity};

/// Convert one successful repository query into the transient render projection. Row-level
/// failures are isolated so one malformed durable row cannot hide other valid History entries.
fn history_entries_from_load(result: HistoryLoadResult) -> (Vec<HistoryEntry>, usize) {
    let (snapshots, warnings) = result.into_parts();
    for warning in &warnings {
        let warning_kind = match warning.kind() {
            HistoryLoadWarningKind::SnapshotDecode(_) => "snapshot_decode",
            HistoryLoadWarningKind::MetadataMismatch { .. } => "metadata_mismatch",
        };
        tracing::warn!(
            entry_id = warning.entry_id(),
            warning_kind,
            "skipping an invalid SQLite History row"
        );
    }

    let mut skipped_rows = warnings.len();
    let entries = snapshots
        .into_iter()
        .filter_map(|snapshot| match HistoryEntry::try_from(snapshot) {
            Ok(entry) => Some(entry),
            Err(error) => {
                skipped_rows += 1;
                tracing::warn!(%error, "skipping a History snapshot that cannot be rendered");
                None
            }
        })
        .collect();
    (entries, skipped_rows)
}

/// Run one already-queued SQLite mutation, then replace visible History only with the already-
/// queued authoritative reload. This helper owns no data or repository state.
pub(crate) fn spawn_history_operation_and_reload<A: 'static>(
    view_model: Entity<WorkspaceViewModel>,
    operation_stage: HistoryStorageStage,
    operation: HistoryRepositoryTask<()>,
    reload: HistoryRepositoryTask<HistoryLoadResult>,
    runtime_replay_request: Option<(String, Request)>,
    cx: &mut Context<A>,
) {
    view_model.update(cx, |view_model, cx| {
        view_model.set_history_loading(operation_stage);
        cx.notify();
    });
    // Blocking waits live on GPUI's background executor. The SQLite worker therefore never wakes
    // or blocks the foreground scheduler directly.
    let operation = cx
        .background_executor()
        .spawn(async move { operation.join_on_background_thread() });
    let reload = cx
        .background_executor()
        .spawn(async move { reload.join_on_background_thread() });
    cx.spawn(async move |_this, cx| {
        if let Err(error) = operation.await {
            view_model.update(cx, |view_model, cx| {
                view_model.set_history_storage_error(operation_stage, error.to_string());
                cx.notify();
            });
            return;
        }

        view_model.update(cx, |view_model, cx| {
            view_model.set_history_loading(HistoryStorageStage::Load);
            cx.notify();
        });
        match reload.await {
            Ok(result) => {
                let (entries, skipped_rows) = history_entries_from_load(result);
                view_model.update(cx, move |view_model, cx| {
                    view_model.replace_history_query_result(entries, skipped_rows);
                    if let Some((entry_id, request)) = runtime_replay_request {
                        view_model.confirm_runtime_replay_request(entry_id, request);
                    }
                    cx.notify();
                });
            }
            Err(error) => {
                view_model.update(cx, |view_model, cx| {
                    view_model
                        .set_history_storage_error(HistoryStorageStage::Load, error.to_string());
                    cx.notify();
                });
            }
        }
    })
    .detach();
}

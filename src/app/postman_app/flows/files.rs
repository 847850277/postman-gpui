//! File commands retain document identity across native pickers and background I/O.
mod close;
mod storage;
#[cfg(test)]
mod tests;
mod view;

use super::{FlowId, FlowsView};
use gpui::{App, Context, Window};
use gpui_kit::component::WindowExt;
use postman_flow::FlowDocument;
use std::{collections::HashMap, path::PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::app::postman_app) enum SaveKind {
    Save,
    SaveAs,
    Export,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum CloseTarget {
    Flow(FlowId),
    Quit,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum DialogKind {
    Close(FlowId),
    Stop(FlowId),
    Conflict,
    Recent,
}
struct SaveSnapshot {
    flow: FlowId,
    kind: SaveKind,
    document: FlowDocument,
}
#[derive(Clone)]
struct Write {
    flow: FlowId,
    kind: SaveKind,
    document: FlowDocument,
    path: PathBuf,
    bytes: Vec<u8>,
    expected: Option<Vec<u8>>,
}
#[derive(Default)]
pub(super) struct Files {
    index: Option<PathBuf>,
    recent: Vec<storage::Recent>,
    recent_error: Option<String>,
    error: Option<String>,
    busy: bool,
    busy_flow: Option<FlowId>,
    conflict: Option<Write>,
    dialog: Option<DialogKind>,
    return_focus: Option<gpui::FocusHandle>,
    close: Option<CloseTarget>,
    discarded: HashMap<FlowId, u64>,
    stop_approved: HashMap<FlowId, u64>,
}
impl FlowsView {
    pub(in crate::app::postman_app) fn configure_files(
        &mut self,
        index: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.files.index = index.clone();
        self.files.busy = true;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    index
                        .ok_or_else(|| "Recent-flow storage is unavailable.".to_string())
                        .and_then(|p| storage::load_recent(&p))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(recent) => {
                        this.files.recent = recent;
                        this.files.recent_error = None;
                    }
                    Err(e) => this.files.recent_error = Some(e),
                }
                this.files.busy = false;
                cx.notify();
            });
        })
        .detach();
    }
    fn file_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.files.error = Some(error);
        if self.files.dialog.is_none() {
            self.files.close = None;
            self.files.return_focus = None;
        }
        self.files.busy = false;
        self.files.busy_flow = None;
        cx.notify();
    }
    fn files_available(&self, window: &mut Window, cx: &mut App) -> bool {
        !self.files.busy
            && self.files.close.is_none()
            && self.files.conflict.is_none()
            && !window.has_active_dialog(cx)
    }
    pub(super) fn is_dirty(&self, flow: FlowId, cx: &App) -> bool {
        self.library.get(flow).is_some_and(|d| d.is_dirty())
            || !self.draft_issues(flow, cx).is_empty()
    }
    pub(in crate::app::postman_app) fn save_flow(
        &mut self,
        kind: SaveKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.files_available(window, cx) {
            return;
        }
        if let Some(draft) = self.library.active() {
            self.save_document(draft.id(), kind, window, cx);
        }
    }
    fn save_document(
        &mut self,
        flow: FlowId,
        kind: SaveKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.files.busy {
            return;
        }
        if !self.draft_issues(flow, cx).is_empty() {
            self.file_error("Fix the pending document edits before saving or exporting. Check locates invalid fields, including hidden steps and Inputs.".into(), cx);
            if self.files.dialog.is_none() {
                self.select_flow(flow, window, cx);
                self.check_flow(window, cx);
            }
            return;
        }
        let Some(draft) = self.library.get(flow) else {
            return;
        };
        let document = draft.document().clone();
        let source = draft.saved().map(|s| (s.path.clone(), s.bytes.clone()));
        self.files.busy = true;
        self.files.busy_flow = Some(flow);
        self.files.error = None;
        if kind == SaveKind::Save {
            if let Some((path, expected)) = source {
                self.prepare_write(
                    SaveSnapshot {
                        flow,
                        kind,
                        document,
                    },
                    path,
                    Some(expected),
                    window,
                    cx,
                );
                return;
            }
        }
        let directory = source
            .as_ref()
            .and_then(|(p, _)| p.parent().map(|p| p.to_path_buf()))
            .or_else(|| {
                directories::UserDirs::new()
                    .map(|d| d.document_dir().unwrap_or(d.home_dir()).to_path_buf())
            })
            .unwrap_or_default();
        let filename = source
            .as_ref()
            .and_then(|(p, _)| p.file_name())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| suggested_name(&document.flow.name));
        let selection = cx.prompt_for_new_path(&directory, Some(&filename));
        cx.spawn_in(window, async move |this, cx| {
            let result = selection.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(Ok(Some(path))) => this.prepare_write(
                    SaveSnapshot {
                        flow,
                        kind,
                        document,
                    },
                    path,
                    None,
                    window,
                    cx,
                ),
                Ok(Err(e)) => this.file_error(format!("Could not open the file picker: {e}"), cx),
                _ => {
                    this.files.busy = false;
                    this.files.busy_flow = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
    fn prepare_write(
        &mut self,
        snapshot: SaveSnapshot,
        path: PathBuf,
        expected: Option<Vec<u8>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let SaveSnapshot {
            flow,
            kind,
            document,
        } = snapshot;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_executor().spawn(async move {
                let path = storage::identity(&path)?;
                let bytes = storage::encode(&document)?;
                Ok::<_, String>(Write { flow, kind, document, path, bytes, expected })
            }).await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Err(e) => this.file_error(e, cx),
                Ok(mut write) => {
                    if this.library.get(flow).is_none() { this.file_error("This Flow is no longer open.".into(), cx); return; }
                    let collision = this.library.drafts().iter().find(|d| d.saved().is_some_and(|s| s.path == write.path));
                    if let Some(other) = collision {
                        if other.id() != flow || kind == SaveKind::Export {
                            this.file_error("This destination belongs to an open Flow. Use Save on that Flow, or choose another filename.".into(), cx);
                            return;
                        }
                        // Save as to the same file has the same external-change protection as Save.
                        write.expected = other.saved().map(|s| s.bytes.clone());
                    }
                    this.write_document(write, window, cx);
                }
            });
        }).detach();
    }
    fn write_document(&mut self, write: Write, window: &mut Window, cx: &mut Context<Self>) {
        self.files.busy = true;
        self.files.busy_flow = Some(write.flow);
        let task = write.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result =
                cx.background_executor()
                    .spawn(async move {
                        storage::write(&task.path, &task.bytes, task.expected.as_deref())
                    })
                    .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(()) => {
                    if write.kind != SaveKind::Export {
                        this.library.associate(
                            write.flow,
                            write.path.clone(),
                            write.bytes,
                            write.document.clone(),
                        );
                        storage::remember(
                            &mut this.files.recent,
                            write.path,
                            write.document.flow.name,
                        );
                    }
                    this.files.error = None;
                    this.persist_recent(window, cx);
                }
                Err(storage::WriteError::Failed(e)) => this.file_error(
                    format!(
                        "Could not write {}: {e}. Your edits are still open.",
                        write.path.display()
                    ),
                    cx,
                ),
                Err(storage::WriteError::Conflict(actual)) => {
                    this.files.busy = false;
                    this.files.busy_flow = None;
                    let mut write = write;
                    write.expected = actual;
                    this.files.conflict = Some(write);
                    this.dismiss_file_dialog(window, cx);
                    cx.defer_in(window, |this, window, cx| this.open_conflict(window, cx));
                }
            });
        })
        .detach();
        cx.notify();
    }
    /// Serialize recent-index writes with file commands. An unavailable index never invalidates
    /// a successful document save, and a damaged index is never silently overwritten.
    fn persist_recent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.files.index.clone();
        let recent = self.files.recent.clone();
        let writable = self.files.recent_error.is_none();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    if writable {
                        if let Some(index) = index {
                            return storage::save_recent(&index, &recent);
                        }
                    }
                    Ok(())
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if let Err(e) = result {
                    this.files.recent_error = Some(e);
                }
                this.files.busy = false;
                this.files.busy_flow = None;
                if matches!(this.files.dialog, Some(DialogKind::Close(_))) {
                    this.dismiss_file_dialog(window, cx);
                }
                if this.files.close.is_none() && this.files.dialog.is_none() {
                    this.files.return_focus = None;
                }
                cx.defer_in(window, |this, window, cx| {
                    this.advance_file_close(window, cx)
                });
                cx.notify();
            });
        })
        .detach();
    }
    pub(in crate::app::postman_app) fn open_flow(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.files_available(window, cx) {
            return;
        }
        self.files.busy = true;
        self.files.error = None;
        let selection = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open Flow (.http.yml)".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = selection.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(Ok(Some(paths))) if !paths.is_empty() => {
                    this.open_path(paths[0].clone(), window, cx)
                }
                Ok(Err(e)) => this.file_error(format!("Could not open the file picker: {e}"), cx),
                _ => {
                    this.files.busy = false;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
    fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.files.busy = true;
        self.files.error = None;
        cx.spawn_in(window, async move |this, cx| {
            let resolved = cx
                .background_executor()
                .spawn(async move { storage::identity(&path) })
                .await;
            let Ok(path) = resolved else {
                let _ = this.update(cx, |this, cx| this.file_error(resolved.unwrap_err(), cx));
                return;
            };
            let already_open = this
                .update_in(cx, |this, window, cx| {
                    if let Some(id) = this
                        .library
                        .drafts()
                        .iter()
                        .find(|d| d.saved().is_some_and(|s| s.path == path))
                        .map(|d| d.id())
                    {
                        this.select_flow(id, window, cx);
                        this.files.busy = false;
                        this.files.return_focus = None;
                        cx.notify();
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(true);
            if already_open {
                return;
            }
            let result = cx
                .background_executor()
                .spawn(async move { storage::open(&path) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Err(e) => this.file_error(e, cx),
                Ok(loaded) => {
                    let document = loaded.document;
                    let flow = this.library.insert(document.clone());
                    this.library.associate(
                        flow,
                        loaded.path.clone(),
                        loaded.bytes,
                        document.clone(),
                    );
                    storage::remember(&mut this.files.recent, loaded.path, document.flow.name);
                    this.sync_fields(window, cx);
                    this.sync_picker(window, cx);
                    this.persist_recent(window, cx);
                }
            });
        })
        .detach();
        cx.notify();
    }
}
fn suggested_name(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(70)
        .collect();
    format!(
        "{}.http.yml",
        if name.trim_matches('-').is_empty() {
            "flow"
        } else {
            name.trim_matches('-')
        }
    )
}

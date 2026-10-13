use super::*;

#[derive(Default)]
pub(super) struct DocumentLibrary {
    pub(super) documents: Vec<RequestDocument>,
    pub(super) loading: bool,
    pub(super) ready: bool,
    pub(super) error: Option<String>,
    pub(super) saving: Option<(RequestTabId, String, u64)>,
}
impl RequestViewModel {
    pub fn saved_document_id(&self) -> Option<&str> {
        self.saved_document.as_ref().map(|d| d.id())
    }
    pub(crate) fn document_snapshot(&self, name: &str) -> Result<RequestDocument, String> {
        RequestDocument::validate_name(name)?;
        let (id, revision) = self
            .saved_document
            .as_ref()
            .map(|d| (d.id().to_owned(), d.revision() + 1))
            .unwrap_or_else(|| (uuid::Uuid::new_v4().to_string(), 1));
        Ok(RequestDocument::snapshot(
            id,
            name.trim().into(),
            revision,
            &self.draft,
            &self.pre_request_script,
            &self.tests_script,
        ))
    }
}
impl WorkspaceViewModel {
    pub fn saved_requests(&self) -> &[RequestDocument] {
        &self.documents.documents
    }
    pub fn documents_loading(&self) -> bool {
        self.documents.loading
    }
    pub fn documents_ready(&self) -> bool {
        self.documents.ready
    }
    pub fn documents_error(&self) -> Option<&str> {
        self.documents.error.as_deref()
    }
    pub fn saving_request(&self) -> Option<RequestTabId> {
        self.documents.saving.as_ref().map(|(tab, _, _)| *tab)
    }
    pub(crate) fn set_document_loading(&mut self) {
        self.documents.loading = true;
        self.documents.error = None;
    }
    pub(crate) fn complete_document_load(
        &mut self,
        result: Result<(Vec<RequestDocument>, usize), String>,
    ) {
        self.documents.loading = false;
        match result {
            Ok((documents, skipped)) => {
                self.documents.documents = documents;
                self.documents.ready = true;
                self.documents.error = (skipped > 0).then(|| format!("{skipped} saved request(s) could not be read. Their stored data has been preserved."));
            }
            Err(error) => {
                self.documents.ready = false;
                self.documents.error = Some(error);
            }
        }
    }
    pub(crate) fn begin_document_save(
        &mut self,
        tab: RequestTabId,
        snapshot: &RequestDocument,
    ) -> bool {
        if self.documents.saving.is_some() || !self.documents.ready || self.documents.loading {
            return false;
        }
        self.documents.saving = Some((tab, snapshot.id().to_owned(), snapshot.revision()));
        self.documents.error = None;
        true
    }
    pub(crate) fn complete_document_save(
        &mut self,
        tab: RequestTabId,
        snapshot: RequestDocument,
        result: Result<(), String>,
    ) {
        if self.documents.saving.as_ref()
            != Some(&(tab, snapshot.id().to_owned(), snapshot.revision()))
        {
            return;
        }
        self.documents.saving = None;
        match result {
            Ok(()) => {
                if let Some(request) = self.request_for_tab_mut(tab) {
                    request.saved_document = Some(snapshot.clone());
                }
                self.documents.documents.retain(|d| d.id() != snapshot.id());
                self.documents.documents.insert(0, snapshot);
                self.documents.error = None;
            }
            Err(error) => self.documents.error = Some(error),
        }
    }
    pub(crate) fn open_saved_request(&mut self, id: &str) -> Option<RequestTabId> {
        if let Some(tab) = self
            .tabs()
            .iter()
            .find(|r| r.saved_document_id() == Some(id))
            .map(|r| r.tab_id())
        {
            self.select_tab_by_id(tab);
            return Some(tab);
        }
        let document = self.saved_requests().iter().find(|d| d.id() == id)?.clone();
        let mut request = RequestViewModel::new();
        request.draft = document.draft().clone();
        request.pre_request_script = document.pre_request_script().into();
        request.tests_script = document.tests_script().into();
        request.saved_document = Some(document);
        self.tabs.push(request);
        let tab = self.active_tab_id()?;
        self.touch_request(tab);
        Some(tab)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_save_and_late_completions_do_not_clear_newer_edits_or_another_tab() {
        let mut model = WorkspaceViewModel::new();
        model.complete_document_load(Ok((vec![], 0)));
        let tab = model.active_tab_id().unwrap();
        model
            .active_request_mut()
            .unwrap()
            .set_url("https://httpbingo.org/get?first=1");
        let first = model
            .active_request()
            .unwrap()
            .document_snapshot("Echo")
            .unwrap();
        assert!(model.begin_document_save(tab, &first));
        assert!(!model.begin_document_save(tab, &first));
        model
            .active_request_mut()
            .unwrap()
            .set_url("https://httpbingo.org/get?latest=2");
        model.new_request();
        let other = model.active_tab_id().unwrap();
        model.complete_document_save(tab, first.clone(), Ok(()));
        assert!(model.request_for_tab(tab).unwrap().is_dirty());
        assert!(model
            .request_for_tab(other)
            .unwrap()
            .saved_document_id()
            .is_none());
        let next = model
            .request_for_tab(tab)
            .unwrap()
            .document_snapshot("Renamed")
            .unwrap();
        assert!(model.begin_document_save(tab, &next));
        model.complete_document_save(tab, first, Ok(()));
        assert_eq!(model.saving_request(), Some(tab));
        model.complete_document_save(tab, next, Err("disk full".into()));
        assert!(model.request_for_tab(tab).unwrap().is_dirty());
        assert_eq!(model.saved_requests()[0].name(), "Echo");
        let next = model
            .request_for_tab(tab)
            .unwrap()
            .document_snapshot("Renamed")
            .unwrap();
        assert!(model.begin_document_save(tab, &next));
        model.complete_document_save(tab, next.clone(), Ok(()));
        assert!(!model.request_for_tab(tab).unwrap().is_dirty());
        assert_eq!(model.open_saved_request(next.id()), Some(tab));
        assert_eq!(model.tab_count(), 2);
        model.close_tab_by_id(tab);
        let restored = model.open_saved_request(next.id()).unwrap();
        assert_eq!(
            model.request_for_tab(restored).unwrap().url(),
            "https://httpbingo.org/get?latest=2"
        );
        assert!(!model.request_for_tab(restored).unwrap().is_dirty());
    }
    #[test]
    fn script_edits_and_reverts_track_the_committed_baseline() {
        let mut model = WorkspaceViewModel::new();
        model.complete_document_load(Ok((vec![], 0)));
        let tab = model.active_tab_id().unwrap();
        let snapshot = model
            .active_request()
            .unwrap()
            .document_snapshot("Empty draft")
            .unwrap();
        assert!(model.begin_document_save(tab, &snapshot));
        model.complete_document_save(tab, snapshot, Ok(()));
        model
            .active_request_mut()
            .unwrap()
            .set_tests_script("assert true");
        assert!(model.active_request().unwrap().is_dirty());
        model.active_request_mut().unwrap().set_tests_script("");
        assert!(!model.active_request().unwrap().is_dirty());
    }
}

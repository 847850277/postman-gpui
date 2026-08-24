use super::*;
use crate::models::{HistoryEntry, HttpMethod, Request};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct WorkerEvidence {
    thread_ids: Vec<thread::ThreadId>,
    thread_names: Vec<Option<String>>,
}

struct RecordingRepository {
    evidence: Arc<Mutex<WorkerEvidence>>,
}

impl RecordingRepository {
    fn record(&self) {
        let current = thread::current();
        let mut evidence = self.evidence.lock().unwrap();
        evidence.thread_ids.push(current.id());
        evidence
            .thread_names
            .push(current.name().map(ToString::to_string));
    }
}

impl HistoryRepository for RecordingRepository {
    fn initialize(&mut self) -> Result<(), HistoryRepositoryError> {
        self.record();
        Ok(())
    }

    fn load_recent(&mut self, _limit: usize) -> Result<HistoryLoadResult, HistoryRepositoryError> {
        self.record();
        Ok(HistoryLoadResult::default())
    }

    fn append_and_trim(
        &mut self,
        _snapshot: &VersionedHistorySnapshot,
        _limit: usize,
    ) -> Result<(), HistoryRepositoryError> {
        self.record();
        Ok(())
    }

    fn clear(&mut self) -> Result<(), HistoryRepositoryError> {
        self.record();
        Ok(())
    }
}

#[tokio::test]
async fn every_repository_operation_runs_on_the_dedicated_storage_thread() {
    let caller_thread = thread::current().id();
    let evidence = Arc::new(Mutex::new(WorkerEvidence::default()));
    let worker = HistoryRepositoryWorker::start(RecordingRepository {
        evidence: evidence.clone(),
    })
    .unwrap();

    worker.initialize().await.unwrap();
    worker.load_recent(50).await.unwrap();
    let entry = HistoryEntry::completed(
        Request::new(HttpMethod::GET, "https://example.com/"),
        "example".to_string(),
        200,
        1,
        0,
    );
    let snapshot = VersionedHistorySnapshot::try_from(&entry).unwrap();
    worker.append_and_trim(snapshot, 50).await.unwrap();
    worker.clear().await.unwrap();

    let evidence = evidence.lock().unwrap();
    assert_eq!(evidence.thread_ids.len(), 4);
    assert!(evidence
        .thread_ids
        .iter()
        .all(|thread_id| *thread_id != caller_thread));
    assert!(evidence
        .thread_ids
        .windows(2)
        .all(|pair| pair[0] == pair[1]));
    assert!(evidence
        .thread_names
        .iter()
        .all(|name| name.as_deref() == Some(HISTORY_STORAGE_THREAD_NAME)));
}

struct FailingRepository;

impl HistoryRepository for FailingRepository {
    fn initialize(&mut self) -> Result<(), HistoryRepositoryError> {
        Err(HistoryRepositoryError::Busy {
            operation: super::super::HistoryRepositoryOperation::Initialize,
        })
    }

    fn load_recent(&mut self, _limit: usize) -> Result<HistoryLoadResult, HistoryRepositoryError> {
        unreachable!()
    }

    fn append_and_trim(
        &mut self,
        _snapshot: &VersionedHistorySnapshot,
        _limit: usize,
    ) -> Result<(), HistoryRepositoryError> {
        unreachable!()
    }

    fn clear(&mut self) -> Result<(), HistoryRepositoryError> {
        unreachable!()
    }
}

#[tokio::test]
async fn typed_repository_errors_cross_the_worker_boundary_unchanged() {
    let worker = HistoryRepositoryWorker::start(FailingRepository).unwrap();
    assert_eq!(
        worker.initialize().await.unwrap_err(),
        HistoryRepositoryError::Busy {
            operation: super::super::HistoryRepositoryOperation::Initialize,
        }
    );
}

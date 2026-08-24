use super::{
    HistoryLoadResult, HistoryRepository, HistoryRepositoryError, VersionedHistorySnapshot,
};
use std::{sync::mpsc, thread};
use tokio::sync::oneshot;

const HISTORY_STORAGE_THREAD_NAME: &str = "postman-history-storage";

/// Async application boundary backed by one dedicated blocking thread. GPUI code can await these
/// methods without ever opening or using a SQLite connection itself.
pub struct HistoryRepositoryWorker {
    sender: mpsc::Sender<HistoryRepositoryCommand>,
}

impl HistoryRepositoryWorker {
    pub fn start(
        repository: impl HistoryRepository + 'static,
    ) -> Result<Self, HistoryRepositoryError> {
        let (sender, receiver) = mpsc::channel();
        thread::Builder::new()
            .name(HISTORY_STORAGE_THREAD_NAME.to_string())
            .spawn(move || run_worker(repository, receiver))
            .map_err(|error| HistoryRepositoryError::WorkerStart {
                message: error.to_string(),
            })?;
        Ok(Self { sender })
    }

    pub async fn initialize(&self) -> Result<(), HistoryRepositoryError> {
        let (reply, response) = oneshot::channel();
        self.sender
            .send(HistoryRepositoryCommand::Initialize { reply })
            .map_err(|_| HistoryRepositoryError::WorkerUnavailable)?;
        response
            .await
            .map_err(|_| HistoryRepositoryError::WorkerUnavailable)?
    }

    pub async fn load_recent(
        &self,
        limit: usize,
    ) -> Result<HistoryLoadResult, HistoryRepositoryError> {
        let (reply, response) = oneshot::channel();
        self.sender
            .send(HistoryRepositoryCommand::LoadRecent { limit, reply })
            .map_err(|_| HistoryRepositoryError::WorkerUnavailable)?;
        response
            .await
            .map_err(|_| HistoryRepositoryError::WorkerUnavailable)?
    }

    pub async fn append_and_trim(
        &self,
        snapshot: VersionedHistorySnapshot,
        limit: usize,
    ) -> Result<(), HistoryRepositoryError> {
        let (reply, response) = oneshot::channel();
        self.sender
            .send(HistoryRepositoryCommand::AppendAndTrim {
                snapshot: Box::new(snapshot),
                limit,
                reply,
            })
            .map_err(|_| HistoryRepositoryError::WorkerUnavailable)?;
        response
            .await
            .map_err(|_| HistoryRepositoryError::WorkerUnavailable)?
    }

    pub async fn clear(&self) -> Result<(), HistoryRepositoryError> {
        let (reply, response) = oneshot::channel();
        self.sender
            .send(HistoryRepositoryCommand::Clear { reply })
            .map_err(|_| HistoryRepositoryError::WorkerUnavailable)?;
        response
            .await
            .map_err(|_| HistoryRepositoryError::WorkerUnavailable)?
    }
}

enum HistoryRepositoryCommand {
    Initialize {
        reply: oneshot::Sender<Result<(), HistoryRepositoryError>>,
    },
    LoadRecent {
        limit: usize,
        reply: oneshot::Sender<Result<HistoryLoadResult, HistoryRepositoryError>>,
    },
    AppendAndTrim {
        snapshot: Box<VersionedHistorySnapshot>,
        limit: usize,
        reply: oneshot::Sender<Result<(), HistoryRepositoryError>>,
    },
    Clear {
        reply: oneshot::Sender<Result<(), HistoryRepositoryError>>,
    },
}

fn run_worker(
    mut repository: impl HistoryRepository,
    receiver: mpsc::Receiver<HistoryRepositoryCommand>,
) {
    while let Ok(command) = receiver.recv() {
        match command {
            HistoryRepositoryCommand::Initialize { reply } => {
                let _ = reply.send(repository.initialize());
            }
            HistoryRepositoryCommand::LoadRecent { limit, reply } => {
                let _ = reply.send(repository.load_recent(limit));
            }
            HistoryRepositoryCommand::AppendAndTrim {
                snapshot,
                limit,
                reply,
            } => {
                let _ = reply.send(repository.append_and_trim(&snapshot, limit));
            }
            HistoryRepositoryCommand::Clear { reply } => {
                let _ = reply.send(repository.clear());
            }
        }
    }
}

#[cfg(test)]
mod tests;

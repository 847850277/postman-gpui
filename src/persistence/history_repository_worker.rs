use super::{
    HistoryLoadResult, HistoryRepository, HistoryRepositoryError, VersionedHistorySnapshot,
};
use std::{
    future::Future,
    pin::Pin,
    sync::mpsc,
    task::{Context, Poll},
    thread,
};
use tokio::sync::oneshot;

const HISTORY_STORAGE_THREAD_NAME: &str = "postman-history-storage";

/// Async application boundary backed by one dedicated blocking thread. GPUI code can await these
/// methods without ever opening or using a SQLite connection itself.
pub struct HistoryRepositoryWorker {
    sender: mpsc::Sender<HistoryRepositoryCommand>,
}

/// Result handle for one command already queued on the SQLite worker.
///
/// Ordinary async callers may await it. GPUI hosts use `join_on_background_thread` inside their
/// background executor so the dedicated storage thread never directly wakes the UI scheduler.
pub struct HistoryRepositoryTask<T> {
    queued: Result<(), HistoryRepositoryError>,
    response: oneshot::Receiver<Result<T, HistoryRepositoryError>>,
}

impl<T> HistoryRepositoryTask<T> {
    pub fn join_on_background_thread(self) -> Result<T, HistoryRepositoryError> {
        self.queued?;
        self.response
            .blocking_recv()
            .map_err(|_| HistoryRepositoryError::WorkerUnavailable)?
    }
}

impl<T> Future for HistoryRepositoryTask<T> {
    type Output = Result<T, HistoryRepositoryError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Err(error) = &self.queued {
            return Poll::Ready(Err(error.clone()));
        }
        match Pin::new(&mut self.response).poll(cx) {
            Poll::Ready(Ok(result)) => Poll::Ready(result),
            Poll::Ready(Err(_)) => Poll::Ready(Err(HistoryRepositoryError::WorkerUnavailable)),
            Poll::Pending => Poll::Pending,
        }
    }
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

    /// Queue initialization immediately and return a future for its result. Enqueuing before the
    /// future is first polled preserves command ordering when GPUI starts storage tasks together.
    pub fn initialize(&self) -> HistoryRepositoryTask<()> {
        let (reply, response) = oneshot::channel();
        let queued = self
            .sender
            .send(HistoryRepositoryCommand::Initialize { reply })
            .map_err(|_| HistoryRepositoryError::WorkerUnavailable);
        HistoryRepositoryTask { queued, response }
    }

    pub fn load_recent(&self, limit: usize) -> HistoryRepositoryTask<HistoryLoadResult> {
        let (reply, response) = oneshot::channel();
        let queued = self
            .sender
            .send(HistoryRepositoryCommand::LoadRecent { limit, reply })
            .map_err(|_| HistoryRepositoryError::WorkerUnavailable);
        HistoryRepositoryTask { queued, response }
    }

    pub fn append_and_trim(
        &self,
        snapshot: VersionedHistorySnapshot,
        limit: usize,
    ) -> HistoryRepositoryTask<()> {
        let (reply, response) = oneshot::channel();
        let queued = self
            .sender
            .send(HistoryRepositoryCommand::AppendAndTrim {
                snapshot: Box::new(snapshot),
                limit,
                reply,
            })
            .map_err(|_| HistoryRepositoryError::WorkerUnavailable);
        HistoryRepositoryTask { queued, response }
    }

    pub fn clear(&self) -> HistoryRepositoryTask<()> {
        let (reply, response) = oneshot::channel();
        let queued = self
            .sender
            .send(HistoryRepositoryCommand::Clear { reply })
            .map_err(|_| HistoryRepositoryError::WorkerUnavailable);
        HistoryRepositoryTask { queued, response }
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

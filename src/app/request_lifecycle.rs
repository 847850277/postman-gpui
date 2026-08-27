use crate::models::{Request, RequestEditorIntent, RequestOptions};
use std::{
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

/// Stable identity for a request tab.
///
/// Async work must carry this identity and must never resolve its destination from the currently
/// selected tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RequestTabId(pub(crate) u64);

impl fmt::Display for RequestTabId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Monotonic identity for one send attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SendId(pub(crate) u64);

impl fmt::Display for SendId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl SendId {
    /// Human-readable identity rendered while one send attempt owns the active lifecycle.
    pub fn request_id(self) -> String {
        format!("req-{:02}", self.0)
    }
}

/// Immutable command emitted by the ViewModel for the application service to execute.
#[derive(Clone, Debug)]
pub struct PendingRequest {
    tab_id: RequestTabId,
    send_id: SendId,
    start: SendStart,
    request: Request,
    editor_intent: Option<RequestEditorIntent>,
    request_options: RequestOptions,
    cancelled: Arc<AtomicBool>,
}

impl PendingRequest {
    pub(crate) fn new(
        tab_id: RequestTabId,
        send_id: SendId,
        start: SendStart,
        request: Request,
        editor_intent: Option<RequestEditorIntent>,
        request_options: RequestOptions,
        cancelled: Arc<AtomicBool>,
    ) -> Self {
        Self {
            tab_id,
            send_id,
            start,
            request,
            editor_intent,
            request_options,
            cancelled,
        }
    }

    pub fn tab_id(&self) -> RequestTabId {
        self.tab_id
    }

    pub fn send_id(&self) -> SendId {
        self.send_id
    }

    pub fn start(&self) -> SendStart {
        self.start
    }

    /// Previous terminal attempt on the same tab when this command is an explicit retry.
    pub fn retry_of(&self) -> Option<SendId> {
        self.start.retry_of()
    }

    pub fn request(&self) -> &Request {
        &self.request
    }

    pub fn editor_intent(&self) -> Option<&RequestEditorIntent> {
        self.editor_intent.as_ref()
    }

    /// Per-request deadline captured at Send. `None` means the deadline is disabled.
    pub fn timeout_ms(&self) -> Option<u64> {
        self.request_options.timeout_ms
    }

    /// Complete wire policy captured when Send was pressed.
    pub fn request_options(&self) -> RequestOptions {
        self.request_options
    }

    pub(crate) fn was_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// How a send entered the lifecycle. A retry is deliberate metadata, not an automatic transport
/// policy or an inference from whichever request happened to run previously.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendStart {
    Begin,
    Retry { previous_send_id: SendId },
}

impl SendStart {
    pub fn retry_of(self) -> Option<SendId> {
        match self {
            Self::Begin => None,
            Self::Retry { previous_send_id } => Some(previous_send_id),
        }
    }
}

/// Observable progress owned by one send identity. Transport integrations can add events without
/// coupling progress rejection to rendered response state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendProgress {
    Started,
    Uploading { bytes_sent: u64 },
    WaitingForResponse,
    Downloading { bytes_received: u64 },
}

/// A terminal result retained by the state machine so cancellation and timeout remain distinct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendTerminalOutcome {
    Completed,
    Failed,
    TimedOut,
    Cancelled,
    Superseded,
    Abandoned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SendTerminal {
    send_id: SendId,
    outcome: SendTerminalOutcome,
}

impl SendTerminal {
    pub fn send_id(self) -> SendId {
        self.send_id
    }

    pub fn outcome(self) -> SendTerminalOutcome {
        self.outcome
    }
}

/// Why a lifecycle event was rejected without changing request or response state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendRejection {
    TabNotFound {
        tab_id: RequestTabId,
        send_id: SendId,
    },
    NoActiveSend {
        send_id: SendId,
    },
    StaleSend {
        send_id: SendId,
        active_send_id: SendId,
    },
    DuplicateTerminal {
        send_id: SendId,
        outcome: SendTerminalOutcome,
    },
    RetryUnavailable {
        send_id: SendId,
        active_send_id: Option<SendId>,
        last_terminal_send_id: Option<SendId>,
    },
}

/// Result of progress, completion, or cancellation routing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendTransition {
    Applied,
    Rejected(SendRejection),
}

impl SendTransition {
    pub fn is_applied(self) -> bool {
        matches!(self, Self::Applied)
    }

    pub fn rejection(self) -> Option<SendRejection> {
        match self {
            Self::Applied => None,
            Self::Rejected(rejection) => Some(rejection),
        }
    }
}

#[derive(Clone, Debug)]
struct ActiveSend {
    send_id: SendId,
    cancellation: Arc<AtomicBool>,
    progress: SendProgress,
}

/// Output of the begin transition. Task ownership stays at the application boundary; this output
/// only hands it the cancellation signal captured by the matching immutable command.
#[derive(Clone, Debug)]
pub(crate) struct BeginSendTransition {
    pub(crate) cancellation: Arc<AtomicBool>,
    pub(crate) start: SendStart,
    pub(crate) superseded: Option<SendId>,
}

/// Pure per-tab send coordinator. It owns identities and transition validity, but no GPUI entity,
/// transport task, History store, or rendering concern.
#[derive(Clone, Debug, Default)]
pub(crate) struct RequestSendLifecycle {
    active: Option<ActiveSend>,
    last_terminal: Option<SendTerminal>,
}

impl RequestSendLifecycle {
    pub(crate) fn begin(&mut self, send_id: SendId) -> BeginSendTransition {
        self.start(send_id, SendStart::Begin)
    }

    pub(crate) fn retry(&mut self, send_id: SendId) -> Result<BeginSendTransition, SendRejection> {
        let active_send_id = self.active_send_id();
        let last_terminal_send_id = self.last_terminal.map(SendTerminal::send_id);
        if active_send_id.is_some() || last_terminal_send_id.is_none() {
            return Err(SendRejection::RetryUnavailable {
                send_id,
                active_send_id,
                last_terminal_send_id,
            });
        }
        Ok(self.start(
            send_id,
            SendStart::Retry {
                previous_send_id: last_terminal_send_id.expect("terminal identity was validated"),
            },
        ))
    }

    fn start(&mut self, send_id: SendId, start: SendStart) -> BeginSendTransition {
        let superseded = self.active.take().map(|active| {
            active.cancellation.store(true, Ordering::Release);
            self.last_terminal = Some(SendTerminal {
                send_id: active.send_id,
                outcome: SendTerminalOutcome::Superseded,
            });
            active.send_id
        });
        let cancellation = Arc::new(AtomicBool::new(false));
        self.active = Some(ActiveSend {
            send_id,
            cancellation: cancellation.clone(),
            progress: SendProgress::Started,
        });
        BeginSendTransition {
            cancellation,
            start,
            superseded,
        }
    }

    pub(crate) fn progress(&mut self, send_id: SendId, progress: SendProgress) -> SendTransition {
        match self.active.as_mut() {
            Some(active) if active.send_id == send_id => {
                active.progress = progress;
                SendTransition::Applied
            }
            _ => SendTransition::Rejected(self.rejection_for(send_id)),
        }
    }

    pub(crate) fn complete(
        &mut self,
        send_id: SendId,
        outcome: SendTerminalOutcome,
    ) -> SendTransition {
        match self.active.as_ref() {
            Some(active) if active.send_id == send_id => {
                self.active = None;
                self.last_terminal = Some(SendTerminal { send_id, outcome });
                SendTransition::Applied
            }
            _ => SendTransition::Rejected(self.rejection_for(send_id)),
        }
    }

    pub(crate) fn cancel(&mut self, send_id: SendId) -> SendTransition {
        match self.active.take() {
            Some(active) if active.send_id == send_id => {
                active.cancellation.store(true, Ordering::Release);
                self.last_terminal = Some(SendTerminal {
                    send_id,
                    outcome: SendTerminalOutcome::Cancelled,
                });
                SendTransition::Applied
            }
            Some(active) => {
                self.active = Some(active);
                SendTransition::Rejected(self.rejection_for(send_id))
            }
            None => SendTransition::Rejected(self.rejection_for(send_id)),
        }
    }

    pub(crate) fn abandon(&mut self) -> Option<SendId> {
        let active = self.active.take()?;
        active.cancellation.store(true, Ordering::Release);
        self.last_terminal = Some(SendTerminal {
            send_id: active.send_id,
            outcome: SendTerminalOutcome::Abandoned,
        });
        Some(active.send_id)
    }

    /// Ends ownership for a request that is being replaced and forgets its attempt lineage.
    pub(crate) fn reset(&mut self) -> Option<SendId> {
        let abandoned_send_id = self.abandon();
        self.last_terminal = None;
        abandoned_send_id
    }

    pub(crate) fn active_send_id(&self) -> Option<SendId> {
        self.active.as_ref().map(|active| active.send_id)
    }

    pub(crate) fn progress_state(&self) -> Option<SendProgress> {
        self.active.as_ref().map(|active| active.progress)
    }

    pub(crate) fn last_terminal(&self) -> Option<SendTerminal> {
        self.last_terminal
    }

    fn rejection_for(&self, send_id: SendId) -> SendRejection {
        if let Some(active) = &self.active {
            return SendRejection::StaleSend {
                send_id,
                active_send_id: active.send_id,
            };
        }
        if let Some(terminal) = self.last_terminal.filter(|item| item.send_id == send_id) {
            return SendRejection::DuplicateTerminal {
                send_id,
                outcome: terminal.outcome,
            };
        }
        SendRejection::NoActiveSend { send_id }
    }
}

/// Adapter required by the stable tab collection. The collection owns selection and identity;
/// the value owns its request editor and send state.
pub(crate) trait RequestTabValue {
    fn tab_id(&self) -> RequestTabId;
    fn assign_tab_id(&mut self, tab_id: RequestTabId);
    fn reset_for_replacement(&mut self) -> Option<SendId>;
    fn prepare_for_close(&mut self) -> Option<SendId>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CloseTabTransition {
    Rejected,
    Reset {
        tab_id: RequestTabId,
        abandoned_send_id: Option<SendId>,
    },
    Closed {
        tab_id: RequestTabId,
        active_tab_id: Option<RequestTabId>,
        abandoned_send_id: Option<SendId>,
    },
}

impl CloseTabTransition {
    pub(crate) fn changed(self) -> bool {
        !matches!(self, Self::Rejected)
    }
}

/// Stable-ID tab collection with explicit active selection and close/replace transitions.
pub(crate) struct RequestTabs<T: RequestTabValue> {
    values: Vec<T>,
    active_tab_id: Option<RequestTabId>,
    next_tab_id: u64,
}

impl<T: RequestTabValue> RequestTabs<T> {
    pub(crate) fn with_initial(mut initial: T) -> Self {
        let tab_id = RequestTabId(1);
        initial.assign_tab_id(tab_id);
        Self {
            values: vec![initial],
            active_tab_id: Some(tab_id),
            next_tab_id: 2,
        }
    }

    pub(crate) fn values(&self) -> &[T] {
        &self.values
    }

    pub(crate) fn values_mut(&mut self) -> &mut [T] {
        &mut self.values
    }

    pub(crate) fn len(&self) -> usize {
        self.values.len()
    }

    pub(crate) fn active_tab_id(&self) -> Option<RequestTabId> {
        self.active_tab_id
            .filter(|tab_id| self.get(*tab_id).is_some())
    }

    pub(crate) fn active(&self) -> Option<&T> {
        self.active_tab_id.and_then(|tab_id| self.get(tab_id))
    }

    pub(crate) fn active_mut(&mut self) -> Option<&mut T> {
        let tab_id = self.active_tab_id?;
        self.get_mut(tab_id)
    }

    pub(crate) fn get(&self, tab_id: RequestTabId) -> Option<&T> {
        self.values.iter().find(|value| value.tab_id() == tab_id)
    }

    pub(crate) fn get_mut(&mut self, tab_id: RequestTabId) -> Option<&mut T> {
        self.values
            .iter_mut()
            .find(|value| value.tab_id() == tab_id)
    }

    pub(crate) fn get_at(&self, index: usize) -> Option<&T> {
        self.values.get(index)
    }

    pub(crate) fn index_of(&self, tab_id: RequestTabId) -> Option<usize> {
        self.values
            .iter()
            .position(|value| value.tab_id() == tab_id)
    }

    pub(crate) fn select_index(&mut self, index: usize) -> bool {
        let Some(tab_id) = self.values.get(index).map(RequestTabValue::tab_id) else {
            return false;
        };
        if self.active_tab_id == Some(tab_id) {
            return false;
        }
        self.active_tab_id = Some(tab_id);
        true
    }

    pub(crate) fn select_id(&mut self, tab_id: RequestTabId) -> bool {
        self.index_of(tab_id)
            .is_some_and(|index| self.select_index(index))
    }

    pub(crate) fn push(&mut self, mut value: T) -> RequestTabId {
        let tab_id = RequestTabId(self.next_tab_id);
        self.next_tab_id += 1;
        value.assign_tab_id(tab_id);
        self.values.push(value);
        self.active_tab_id = Some(tab_id);
        tab_id
    }

    pub(crate) fn close(&mut self, index: usize) -> CloseTabTransition {
        if index >= self.values.len() {
            return CloseTabTransition::Rejected;
        }

        if self.values.len() == 1 {
            let tab_id = self.values[0].tab_id();
            let abandoned_send_id = self.values[0].reset_for_replacement();
            self.active_tab_id = Some(tab_id);
            return CloseTabTransition::Reset {
                tab_id,
                abandoned_send_id,
            };
        }

        let tab_id = self.values[index].tab_id();
        let abandoned_send_id = self.values[index].prepare_for_close();
        self.values.remove(index);
        if self.active_tab_id == Some(tab_id) {
            let next_index = index.min(self.values.len() - 1);
            self.active_tab_id = Some(self.values[next_index].tab_id());
        } else if self
            .active_tab_id
            .is_some_and(|active_tab_id| self.index_of(active_tab_id).is_none())
        {
            self.active_tab_id = None;
        }
        CloseTabTransition::Closed {
            tab_id,
            active_tab_id: self.active_tab_id(),
            abandoned_send_id,
        }
    }

    #[cfg(test)]
    pub(crate) fn clear_active_selection(&mut self) {
        self.active_tab_id = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestTab {
        id: RequestTabId,
        active_send: Option<SendId>,
        reset_count: usize,
    }

    impl TestTab {
        fn new(active_send: Option<SendId>) -> Self {
            Self {
                id: RequestTabId(0),
                active_send,
                reset_count: 0,
            }
        }
    }

    impl RequestTabValue for TestTab {
        fn tab_id(&self) -> RequestTabId {
            self.id
        }

        fn assign_tab_id(&mut self, tab_id: RequestTabId) {
            self.id = tab_id;
        }

        fn reset_for_replacement(&mut self) -> Option<SendId> {
            self.reset_count += 1;
            self.active_send.take()
        }

        fn prepare_for_close(&mut self) -> Option<SendId> {
            self.active_send.take()
        }
    }

    #[test]
    fn stable_tab_ids_route_selection_and_close_without_retargeting() {
        let mut tabs = RequestTabs::with_initial(TestTab::new(Some(SendId(7))));
        let first_id = tabs.active_tab_id().unwrap();
        let second_id = tabs.push(TestTab::new(None));
        assert_ne!(first_id, second_id);
        assert!(tabs.select_id(first_id));

        assert_eq!(
            tabs.close(0),
            CloseTabTransition::Closed {
                tab_id: first_id,
                active_tab_id: Some(second_id),
                abandoned_send_id: Some(SendId(7)),
            }
        );
        assert_eq!(tabs.active_tab_id(), Some(second_id));
        assert!(tabs.get(first_id).is_none());
    }

    #[test]
    fn closing_the_last_tab_resets_it_without_reusing_its_identity() {
        let mut tabs = RequestTabs::with_initial(TestTab::new(Some(SendId(4))));
        let tab_id = tabs.active_tab_id().unwrap();

        assert_eq!(
            tabs.close(0),
            CloseTabTransition::Reset {
                tab_id,
                abandoned_send_id: Some(SendId(4)),
            }
        );
        assert_eq!(tabs.active_tab_id(), Some(tab_id));
        assert_eq!(tabs.values()[0].reset_count, 1);
    }

    #[test]
    fn duplicate_completion_is_rejected_after_the_first_terminal_transition() {
        let mut lifecycle = RequestSendLifecycle::default();
        lifecycle.begin(SendId(1));

        assert_eq!(
            lifecycle.complete(SendId(1), SendTerminalOutcome::Completed),
            SendTransition::Applied
        );
        assert_eq!(
            lifecycle.complete(SendId(1), SendTerminalOutcome::Completed),
            SendTransition::Rejected(SendRejection::DuplicateTerminal {
                send_id: SendId(1),
                outcome: SendTerminalOutcome::Completed,
            })
        );
    }

    #[test]
    fn cancellation_and_timeout_are_distinct_terminal_outcomes() {
        let mut lifecycle = RequestSendLifecycle::default();
        let cancelled = lifecycle.begin(SendId(1));
        assert_eq!(lifecycle.cancel(SendId(1)), SendTransition::Applied);
        assert!(cancelled.cancellation.load(Ordering::Acquire));
        assert_eq!(
            lifecycle.last_terminal().unwrap().outcome(),
            SendTerminalOutcome::Cancelled
        );

        let retry = lifecycle.retry(SendId(2)).unwrap();
        assert_eq!(
            retry.start,
            SendStart::Retry {
                previous_send_id: SendId(1),
            }
        );
        assert_eq!(
            lifecycle.complete(SendId(2), SendTerminalOutcome::TimedOut),
            SendTransition::Applied
        );
        assert_eq!(
            lifecycle.last_terminal().unwrap().outcome(),
            SendTerminalOutcome::TimedOut
        );
    }

    #[test]
    fn superseding_an_attempt_rejects_its_progress_and_completion() {
        let mut lifecycle = RequestSendLifecycle::default();
        let first = lifecycle.begin(SendId(1));
        let second = lifecycle.begin(SendId(2));

        assert_eq!(second.superseded, Some(SendId(1)));
        assert!(first.cancellation.load(Ordering::Acquire));
        assert_eq!(
            lifecycle.progress(SendId(1), SendProgress::Downloading { bytes_received: 5 }),
            SendTransition::Rejected(SendRejection::StaleSend {
                send_id: SendId(1),
                active_send_id: SendId(2),
            })
        );
        assert_eq!(lifecycle.progress_state(), Some(SendProgress::Started));
        assert_eq!(
            lifecycle.complete(SendId(1), SendTerminalOutcome::Completed),
            SendTransition::Rejected(SendRejection::StaleSend {
                send_id: SendId(1),
                active_send_id: SendId(2),
            })
        );
    }

    #[test]
    fn retry_is_rejected_without_a_terminal_predecessor() {
        let mut lifecycle = RequestSendLifecycle::default();
        assert!(matches!(
            lifecycle.retry(SendId(1)),
            Err(SendRejection::RetryUnavailable {
                send_id: SendId(1),
                active_send_id: None,
                last_terminal_send_id: None,
            })
        ));
    }
}

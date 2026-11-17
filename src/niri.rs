//! niri IPC effects on worker threads.
//!
//! One thread owns the blocking event stream and reconnects with bounded
//! backoff. A second thread sends focus actions on separate short-lived
//! sockets. Both share a generation gate: after a disconnect is observed,
//! intents that have not been authorized for dispatch are rejected. An
//! already-authorized request is in flight and cannot reliably be recalled.
//! The UI only ever sees ordered [`IpcMessage`]s.

use crate::workspaces::{
    DisplaySelection, DisplaySnapshot, FocusIntent, IpcMessage, select_display,
};
use niri_ipc::socket::Socket;
use niri_ipc::{Event, Request, Response};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const INITIAL_BACKOFF: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(8);
const CHECK_INTERVAL: Duration = Duration::from_millis(100);
const SLEEP_STEP: Duration = Duration::from_millis(50);

/// Pure action-gate decision: an intent may only be sent while its session is
/// still the live one. Kept separate so the stale/queued behaviour is testable
/// without a compositor.
pub fn intent_allowed(intent: &FocusIntent, live_generation: u64) -> bool {
    intent.generation == live_generation
        && matches!(&intent.action, niri_ipc::Action::FocusWorkspace { .. })
}

/// Serializes the decision to dispatch against generation invalidation.
/// No socket operations may run while this lock is held.
#[derive(Debug, Default)]
pub struct FocusGate {
    generation: Mutex<u64>,
}

impl FocusGate {
    pub fn new() -> Self {
        Self {
            generation: Mutex::new(0),
        }
    }

    pub fn generation(&self) -> u64 {
        *self.generation.lock().expect("focus gate poisoned")
    }

    pub fn set(&self, generation: u64) {
        *self.generation.lock().expect("focus gate poisoned") = generation;
    }

    pub fn advance(&self) -> u64 {
        let mut guard = self.generation.lock().expect("focus gate poisoned");
        *guard += 1;
        *guard
    }

    pub fn allows(&self, intent: &FocusIntent) -> bool {
        intent_allowed(intent, self.generation())
    }
}

/// Connect first, then authorize immediately before dispatch. Once authorized,
/// an action is considered in flight: disconnect can invalidate the session
/// without waiting for the send or reply, but cannot recall this action.
fn dispatch_focus<T>(
    gate: &FocusGate,
    intent: &FocusIntent,
    connect: impl FnOnce() -> Option<T>,
    send: impl FnOnce(T, &FocusIntent),
) {
    // Avoid connecting for an already stale queued intent; recheck after
    // connect because it may block while the event stream disconnects.
    if !gate.allows(intent) {
        return;
    }
    let Some(socket) = connect() else {
        return;
    };
    if gate.allows(intent) {
        send(socket, intent);
    }
}

/// Handle given to the UI. Messages are applied in arrival order; focus
/// intents are sent off the UI thread and re-validated at send time.
pub struct NiriClient {
    messages: Receiver<IpcMessage>,
    focus_tx: Sender<FocusIntent>,
    shutdown: Arc<AtomicBool>,
}

impl NiriClient {
    /// Start the workers. `wake` is Rune's repaint waker, called whenever a
    /// message is enqueued so the UI drains it on its own thread.
    pub fn start(displays: Vec<DisplaySnapshot>, wake: rune::Waker) -> Self {
        let (message_tx, messages) = mpsc::channel();
        let (focus_tx, focus_rx) = mpsc::channel();
        let gate = Arc::new(FocusGate::new());
        let shutdown = Arc::new(AtomicBool::new(false));

        let action_gate = Arc::clone(&gate);
        let action_shutdown = Arc::clone(&shutdown);
        thread::Builder::new()
            .name("snowbar-niri-actions".into())
            .spawn(move || {
                while !action_shutdown.load(Ordering::Relaxed) {
                    match focus_rx.recv_timeout(CHECK_INTERVAL) {
                        Ok(intent) => dispatch_focus(
                            &action_gate,
                            &intent,
                            || Socket::connect().ok(),
                            |mut socket, intent| {
                                let _ = socket.send(Request::Action(intent.action.clone()));
                            },
                        ),
                        Err(RecvTimeoutError::Timeout) => continue,
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .expect("spawn niri action worker");

        let event_gate = Arc::clone(&gate);
        let event_shutdown = Arc::clone(&shutdown);
        thread::Builder::new()
            .name("snowbar-niri-events".into())
            .spawn(move || {
                run_sessions(displays, event_gate, event_shutdown, message_tx, wake);
            })
            .expect("spawn niri event worker");

        Self {
            messages,
            focus_tx,
            shutdown,
        }
    }

    /// Drain all pending messages without blocking.
    pub fn drain(&self) -> Vec<IpcMessage> {
        let mut drained = Vec::new();
        while let Ok(message) = self.messages.try_recv() {
            drained.push(message);
        }
        drained
    }

    /// Queue a focus intent. A disconnect between the click and this call is
    /// caught by the action gate unless the intent is already authorized for
    /// dispatch, in which case it is in flight.
    pub fn focus(&self, intent: FocusIntent) {
        let _ = self.focus_tx.send(intent);
    }

    /// Clone of the intent sender, kept by the view so clicks can enqueue
    /// without reaching into the worker-owned client.
    pub fn focus_sender(&self) -> Sender<FocusIntent> {
        self.focus_tx.clone()
    }
}

impl Drop for NiriClient {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}

/// Capped exponential backoff, paused briefly between attempts.
fn next_backoff(current: Duration) -> Duration {
    (current * 2).min(MAX_BACKOFF)
}

fn sleep_bounded(duration: Duration, shutdown: &AtomicBool) {
    let mut left = duration;
    while left > Duration::ZERO && !shutdown.load(Ordering::Relaxed) {
        let nap = left.min(SLEEP_STEP);
        thread::sleep(nap);
        left -= nap;
    }
}

fn publish(ui: &Sender<IpcMessage>, wake: &rune::Waker, message: IpcMessage) -> bool {
    let alive = ui.send(message).is_ok();
    wake();
    alive
}

/// Connect, validate the single-output mapping, stream events until loss.
/// Called repeatedly with a fresh generation on every attempt. Returns whether
/// a validated snapshot was ever published, so backoff can reset after a real
/// session.
fn session_once(
    displays: &[DisplaySnapshot],
    generation: u64,
    ui: &Sender<IpcMessage>,
    wake: &rune::Waker,
) -> bool {
    let outputs = {
        let Ok(mut socket) = Socket::connect() else {
            return false;
        };
        match socket.send(Request::Outputs) {
            Ok(Ok(Response::Outputs(outputs))) => outputs,
            _ => return false,
        }
    };
    let output_names: Vec<String> = outputs.into_keys().collect();
    if output_names.len() != 1 {
        return false;
    }
    let output = &output_names[0];
    if !matches!(
        select_display(displays, &output_names, generation),
        DisplaySelection::Bound(_)
    ) {
        return false;
    }

    // niri stops reading requests on a socket after EventStream, so the action
    // worker always opens its own socket.
    let Ok(mut socket) = Socket::connect() else {
        return false;
    };
    if !matches!(socket.send(Request::EventStream), Ok(Ok(Response::Handled))) {
        return false;
    }
    let mut read_event = socket.read_events();

    let mut ready = false;
    loop {
        let Ok(event) = read_event() else {
            return ready;
        };
        if let Event::WorkspacesChanged { workspaces } = &event {
            if workspaces.iter().any(|workspace| {
                workspace
                    .output
                    .as_deref()
                    .is_some_and(|name| name != output)
            }) {
                // Output inventory drifted: fail closed, reconnect, revalidate.
                return ready;
            }
            if !ready {
                if !publish(
                    ui,
                    wake,
                    IpcMessage::Ready {
                        generation,
                        output: output.clone(),
                        workspaces: workspaces.clone(),
                    },
                ) {
                    return ready;
                }
                ready = true;
                continue;
            }
        }
        if !publish(ui, wake, IpcMessage::Event { generation, event }) {
            return ready;
        }
    }
}

fn run_sessions(
    displays: Vec<DisplaySnapshot>,
    gate: Arc<FocusGate>,
    shutdown: Arc<AtomicBool>,
    ui: Sender<IpcMessage>,
    wake: rune::Waker,
) {
    let mut backoff = INITIAL_BACKOFF;
    while !shutdown.load(Ordering::Relaxed) {
        let generation = gate.advance();
        if !publish(&ui, &wake, IpcMessage::Session { generation }) {
            break;
        }

        let served = session_once(&displays, generation, &ui, &wake);

        // Invalidate before notifying the UI. Socket operations in the action
        // worker do not hold this lock and cannot delay invalidation.
        let invalidated = gate.advance();
        if !publish(
            &ui,
            &wake,
            IpcMessage::Session {
                generation: invalidated,
            },
        ) {
            break;
        }

        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        if served {
            backoff = INITIAL_BACKOFF;
        }
        sleep_bounded(backoff, &shutdown);
        backoff = next_backoff(backoff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use niri_ipc::{Action, WorkspaceReferenceArg};

    fn intent(generation: u64) -> FocusIntent {
        FocusIntent {
            generation,
            workspace_id: 42,
            action: Action::FocusWorkspace {
                reference: WorkspaceReferenceArg::Id(42),
            },
        }
    }

    #[test]
    fn stale_and_queued_intents_are_rejected_by_generation() {
        let gate = FocusGate::new();
        gate.set(3);
        assert!(gate.allows(&intent(3)));
        assert!(!gate.allows(&intent(2)), "queued from an old session");
        assert!(!gate.allows(&intent(4)), "from a future session");

        assert_eq!(gate.advance(), 4);
        assert!(
            !gate.allows(&intent(3)),
            "disconnect invalidated the old intent"
        );
        assert!(gate.allows(&intent(4)));
    }

    #[test]
    fn disconnect_during_connect_rejects_the_intent() {
        let gate = Arc::new(FocusGate::new());
        gate.set(3);
        let worker_gate = Arc::clone(&gate);
        let (started_tx, started_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let (sent_tx, sent_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            dispatch_focus(
                &worker_gate,
                &intent(3),
                || {
                    started_tx.send(()).unwrap();
                    resume_rx.recv().unwrap();
                    Some(())
                },
                |(), _| sent_tx.send(()).unwrap(),
            );
        });
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(gate.advance(), 4);
        resume_tx.send(()).unwrap();
        worker.join().unwrap();
        assert!(sent_rx.try_recv().is_err());
    }

    #[test]
    fn stalled_in_flight_action_does_not_block_invalidation_or_revive_queue() {
        let gate = Arc::new(FocusGate::new());
        gate.set(3);
        let worker_gate = Arc::clone(&gate);
        let (started_tx, started_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let (sent_tx, sent_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            dispatch_focus(
                &worker_gate,
                &intent(3),
                || Some(()),
                |(), _| {
                    started_tx.send(()).unwrap();
                    resume_rx.recv().unwrap();
                },
            );
            // This was queued before the disconnect, but has not been authorized.
            dispatch_focus(
                &worker_gate,
                &intent(3),
                || Some(()),
                |(), _| sent_tx.send(()).unwrap(),
            );
        });
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let invalidate_gate = Arc::clone(&gate);
        let (invalidated_tx, invalidated_rx) = mpsc::channel();
        let invalidator = thread::spawn(move || {
            invalidated_tx.send(invalidate_gate.advance()).unwrap();
        });
        let result = invalidated_rx.recv_timeout(Duration::from_secs(2));
        // Always release the worker, even if the assertion fails, so the test
        // does not leave a stalled thread holding shared resources.
        resume_tx.send(()).unwrap();
        worker.join().unwrap();
        invalidator.join().unwrap();
        assert_eq!(result.unwrap(), 4, "invalidation must not wait for IPC");
        assert!(sent_rx.try_recv().is_err());

        gate.set(5); // A new session cannot revive the old queued intent.
        dispatch_focus(
            &gate,
            &intent(3),
            || Some(()),
            |(), _| panic!("old session intent must not be dispatched"),
        );
    }

    #[test]
    fn connection_failure_does_not_dispatch() {
        let gate = FocusGate::new();
        gate.set(3);
        dispatch_focus(
            &gate,
            &intent(3),
            || None::<()>,
            |(), _| panic!("failed connection must not dispatch"),
        );
    }

    #[test]
    fn action_gate_only_accepts_focus_workspace_actions() {
        assert!(intent_allowed(&intent(1), 1));
        let mut other = intent(1);
        other.action = Action::PowerOffMonitors {};
        assert!(!intent_allowed(&other, 1));
    }

    #[test]
    fn backoff_is_bounded() {
        let mut backoff = INITIAL_BACKOFF;
        let mut seen = Vec::new();
        for _ in 0..12 {
            seen.push(backoff);
            backoff = next_backoff(backoff);
        }
        assert_eq!(seen[0], INITIAL_BACKOFF);
        assert_eq!(seen[1], Duration::from_millis(500));
        assert!(seen.iter().all(|d| *d <= MAX_BACKOFF));
        assert!(seen.last().is_some_and(|d| *d == MAX_BACKOFF));
    }
}

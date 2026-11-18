//! Pure power menu state and typed effect descriptions.
//!
//! This module never opens the niri socket, spawns a process or reads the
//! environment. It owns the selection/confirmation/pending state transitions
//! and the exact typed mapping from one of three actions to one effect. The
//! production runner lives in the binary behind `cfg(not(test))`; tests drive
//! these transitions with a fake channel and never execute a command.

use niri_ipc::{Action, Reply, Request, Response};
use std::fmt;

/// The only actions the menu offers. Adding one requires a mapping and a
/// confirmation/copy review here and in the view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerAction {
    LogOut,
    Restart,
    ShutDown,
}

impl PowerAction {
    pub const ALL: [Self; 3] = [Self::LogOut, Self::Restart, Self::ShutDown];

    pub fn label(self) -> &'static str {
        match self {
            Self::LogOut => "Log out",
            Self::Restart => "Restart",
            Self::ShutDown => "Shut down",
        }
    }

    /// Action-specific confirmation prompt shown before the explicit confirm
    /// control is usable.
    pub fn prompt(self) -> &'static str {
        match self {
            Self::LogOut => "Log out of this session?",
            Self::Restart => "Restart the computer?",
            Self::ShutDown => "Shut down the computer?",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            Self::LogOut => "niri will quit the current session.",
            Self::Restart => "logind will restart the computer.",
            Self::ShutDown => "logind will power off the computer.",
        }
    }

    pub fn pending_line(self) -> &'static str {
        match self {
            Self::LogOut => "Log out request sent; waiting for niri.",
            Self::Restart => "Restart request sent; waiting for logind.",
            Self::ShutDown => "Shut down request sent; waiting for logind.",
        }
    }

    /// Deliberately says accepted, not completed: niri/logind acknowledge the
    /// request before the session or machine necessarily changes.
    pub fn accepted_line(self) -> &'static str {
        match self {
            Self::LogOut => "Log out accepted. niri should end this session.",
            Self::Restart => "Restart accepted by logind. The computer may restart.",
            Self::ShutDown => "Shut down accepted by logind. The computer may power off.",
        }
    }

    /// The one exact typed effect for this action.
    pub fn effect(self) -> Effect {
        match self {
            Self::LogOut => Effect::NiriQuit,
            Self::Restart => Effect::Logind(LogindMethod::Reboot),
            Self::ShutDown => Effect::Logind(LogindMethod::PowerOff),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogindMethod {
    Reboot,
    PowerOff,
}

impl LogindMethod {
    pub fn name(self) -> &'static str {
        match self {
            Self::Reboot => "Reboot",
            Self::PowerOff => "PowerOff",
        }
    }
}

/// The complete set of side effects the menu can request. The production
/// executor maps each variant to exactly one transport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// `Request::Action(Action::Quit { skip_confirmation: true })` on the
    /// session's current `NIRI_SOCKET`.
    NiriQuit,
    /// systemd-logind `Reboot`/`PowerOff` with the normal interactive boolean.
    Logind(LogindMethod),
}

/// The exact niri request sent after Snowbar's own confirmation. niri's own
/// confirmation prompt is skipped only because the menu already obtained an
/// explicit confirmation.
pub fn niri_request() -> Request {
    Request::Action(Action::Quit {
        skip_confirmation: true,
    })
}

/// Fixed `busctl` invocation for normal logind authorization through polkit.
/// No shell, sudo, force flag, inhibitor bypass or user-supplied argument is
/// involved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogindCommand {
    pub program: &'static str,
    pub args: [&'static str; 9],
}

pub fn logind_command(action: PowerAction) -> Option<LogindCommand> {
    let method = match action.effect() {
        Effect::Logind(method) => method,
        Effect::NiriQuit => return None,
    };
    Some(LogindCommand {
        program: "busctl",
        args: [
            "--system",
            // Long enough for a normal polkit agent prompt, still bounded.
            "--timeout=60",
            "call",
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
            method.name(),
            "b",
            "true",
        ],
    })
}

/// A short, safe failure reason. Transport code must construct `Rejected`
/// through [`PowerError::rejected`] so UI text is bounded and single-line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PowerError {
    MissingBackend(&'static str),
    MissingTool(&'static str),
    PermissionDenied,
    Connection,
    NoReply,
    Protocol,
    Rejected(String),
    HelperFailed(String),
    Timeout,
    ProcessFailed,
    SpawnFailed,
    WorkerDisconnected,
    Internal,
    Unsupported,
}

impl PowerError {
    /// Bound service text before it reaches the view.
    pub fn rejected(detail: impl AsRef<str>) -> Self {
        let clean = bounded_detail(detail);
        if clean.is_empty() {
            Self::Internal
        } else {
            Self::Rejected(clean)
        }
    }

    /// Ambiguous helper output, worded without claiming a definite rejection.
    pub fn helper_failed(detail: impl AsRef<str>) -> Self {
        let clean = bounded_detail(detail);
        if clean.is_empty() {
            Self::ProcessFailed
        } else {
            Self::HelperFailed(clean)
        }
    }
}

fn bounded_detail(detail: impl AsRef<str>) -> String {
    let mut clean = String::new();
    for character in detail.as_ref().chars() {
        let character = if character.is_control() {
            ' '
        } else {
            character
        };
        if clean.chars().count() >= 80 {
            break;
        }
        clean.push(character);
    }
    clean.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl fmt::Display for PowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingBackend(what) => write!(f, "{what} is unavailable"),
            Self::MissingTool(tool) => write!(f, "{tool} is not installed"),
            Self::PermissionDenied => f.write_str("permission was denied"),
            Self::Connection => f.write_str("could not reach the service"),
            Self::NoReply => f.write_str("the service closed the connection without a reply"),
            Self::Protocol => f.write_str("the service sent an unreadable reply"),
            Self::Rejected(detail) => write!(f, "the request was rejected: {detail}"),
            Self::HelperFailed(detail) => write!(f, "the helper reported: {detail}"),
            Self::Timeout => f.write_str("no response arrived before the timeout"),
            Self::ProcessFailed => f.write_str("the helper did not return a usable result"),
            Self::SpawnFailed => f.write_str("the background helper could not start"),
            Self::WorkerDisconnected => f.write_str("the background helper is not running"),
            Self::Internal => f.write_str("an internal error occurred"),
            Self::Unsupported => f.write_str("power actions are unavailable on this platform"),
        }
    }
}

/// Result of one dispatch attempt. `Uncertain` is separate from failure
/// because a timed-out request may already have been executed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PowerOutcome {
    Accepted,
    Failed(PowerError),
    Uncertain(PowerError),
}

impl PowerOutcome {
    pub fn is_accepted(&self) -> bool {
        matches!(self, Self::Accepted)
    }

    pub fn summary(&self, action: PowerAction) -> String {
        match self {
            Self::Accepted => action.accepted_line().to_owned(),
            Self::Failed(error) => format!("{} failed: {error}.", action.label()),
            Self::Uncertain(error) => format!(
                "{} status is uncertain: {error}; it may already be in progress.",
                action.label()
            ),
        }
    }
}

/// A connection failure happens before any request bytes are written, so it is
/// a definite failure: the action cannot have happened.
pub fn niri_connect_outcome(kind: std::io::ErrorKind) -> PowerOutcome {
    match kind {
        std::io::ErrorKind::NotFound => {
            PowerOutcome::Failed(PowerError::MissingBackend("niri socket"))
        }
        std::io::ErrorKind::PermissionDenied => PowerOutcome::Failed(PowerError::PermissionDenied),
        _ => PowerOutcome::Failed(PowerError::Connection),
    }
}

/// A write or read error after the request may have been sent is ambiguous:
/// default to `Uncertain` rather than claiming the action failed or succeeded.
pub fn niri_post_send_outcome(kind: std::io::ErrorKind) -> PowerOutcome {
    match kind {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
            PowerOutcome::Uncertain(PowerError::Timeout)
        }
        _ => PowerOutcome::Uncertain(PowerError::Connection),
    }
}

/// EOF after a quit request can mean niri closed the command socket because it
/// processed the request, so it is uncertain rather than a rejection.
pub fn niri_eof_outcome() -> PowerOutcome {
    PowerOutcome::Uncertain(PowerError::NoReply)
}

/// Classify one newline-framed niri reply. A valid `Handled` is accepted; a
/// valid `Err` is niri's definite rejection; malformed or unexpected replies
/// are ambiguous after dispatch.
pub fn niri_reply_outcome(bytes: &[u8]) -> PowerOutcome {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return PowerOutcome::Uncertain(PowerError::Protocol);
    };
    let Ok(reply) = serde_json::from_str::<Reply>(text) else {
        return PowerOutcome::Uncertain(PowerError::Protocol);
    };
    match reply {
        Ok(Response::Handled) => PowerOutcome::Accepted,
        Ok(_) => PowerOutcome::Uncertain(PowerError::Protocol),
        Err(detail) => PowerOutcome::Failed(niri_rejection(detail)),
    }
}

fn niri_rejection(detail: String) -> PowerError {
    let lower = detail.to_lowercase();
    if lower.contains("permission")
        || lower.contains("denied")
        || lower.contains("not authorized")
        || lower.contains("authentication")
    {
        PowerError::PermissionDenied
    } else {
        PowerError::rejected(detail)
    }
}

/// What `busctl` did, reduced to data so the classification is testable
/// without starting a process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogindStatus {
    Success,
    Failure(i32),
    Signaled,
}

/// Classify logind/`busctl` after the process has ended. Definite denial,
/// connection failure and inhibitor rejection stay `Failed`; a timeout,
/// signal or otherwise ambiguous failure is `Uncertain` because the call may
/// already have reached logind.
pub fn logind_outcome(status: LogindStatus, stderr: &str) -> PowerOutcome {
    match status {
        LogindStatus::Success => PowerOutcome::Accepted,
        LogindStatus::Signaled => PowerOutcome::Uncertain(PowerError::ProcessFailed),
        LogindStatus::Failure(code) => {
            let lower = stderr.to_lowercase();
            if lower.contains("timed out") || lower.contains("timeout") {
                return PowerOutcome::Uncertain(PowerError::Timeout);
            }
            if lower.contains("access denied")
                || lower.contains("not authorized")
                || lower.contains("authentication")
            {
                return PowerOutcome::Failed(PowerError::PermissionDenied);
            }
            if lower.contains("failed to connect")
                || lower.contains("connection refused")
                || lower.contains("no such file")
            {
                return PowerOutcome::Failed(PowerError::Connection);
            }
            if lower.contains("inhibited") {
                return PowerOutcome::Failed(PowerError::rejected(stderr));
            }
            if stderr.trim().is_empty() {
                return PowerOutcome::Uncertain(PowerError::ProcessFailed);
            }
            let detail = format!("busctl exited with status {code}: {stderr}");
            PowerOutcome::Uncertain(PowerError::helper_failed(detail))
        }
    }
}

/// A process that never started cannot have performed the action.
pub fn logind_spawn_outcome(kind: std::io::ErrorKind) -> PowerOutcome {
    match kind {
        std::io::ErrorKind::NotFound => PowerOutcome::Failed(PowerError::MissingTool("busctl")),
        std::io::ErrorKind::PermissionDenied => PowerOutcome::Failed(PowerError::PermissionDenied),
        _ => PowerOutcome::Failed(PowerError::SpawnFailed),
    }
}

/// One authorized dispatch. The attempt number makes a late completion from a
/// previous attempt harmless after an explicit retry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dispatch {
    pub attempt: u64,
    pub action: PowerAction,
    pub effect: Effect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PowerEvent {
    pub attempt: u64,
    pub outcome: PowerOutcome,
}

/// What the menu currently shows. Confirming and pending are deliberately
/// distinct: only `confirm` moves out of `Confirming`, and nothing moves back
/// from `Pending` until a matching result arrives.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum MenuState {
    #[default]
    List,
    Confirming(PowerAction),
    Pending(PowerAction),
    Result {
        action: PowerAction,
        outcome: PowerOutcome,
    },
}

/// Pure menu state machine owned by the power overlay.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PowerMenu {
    state: MenuState,
    last_attempt: u64,
}

impl PowerMenu {
    pub fn state(&self) -> &MenuState {
        &self.state
    }

    pub fn is_list(&self) -> bool {
        self.state == MenuState::List
    }

    /// Selecting a row only opens its confirmation. It never dispatches.
    pub fn select(&mut self, action: PowerAction) -> bool {
        if self.state == MenuState::List {
            self.state = MenuState::Confirming(action);
            true
        } else {
            false
        }
    }

    /// Opening the overlay is fresh for an unconfirmed selection. Outstanding
    /// work and results are deliberately preserved.
    pub fn on_open(&mut self) -> bool {
        self.dismiss()
    }

    /// Escape or the bar's power toggle dismisses only an unconfirmed action.
    /// A pending request stays pending and is never presented as cancelled.
    pub fn dismiss(&mut self) -> bool {
        if matches!(self.state, MenuState::Confirming(_)) {
            self.state = MenuState::List;
            true
        } else {
            false
        }
    }

    /// Explicit Cancel/Back returns to the action list. It cannot cancel a
    /// request that is already pending or a completed result.
    pub fn cancel(&mut self) -> bool {
        match &self.state {
            MenuState::Confirming(_) => {
                self.state = MenuState::List;
                true
            }
            MenuState::Result { outcome, .. } if !outcome.is_accepted() => {
                self.state = MenuState::List;
                true
            }
            _ => false,
        }
    }

    /// Atomically leave confirmation and enter single-flight pending before
    /// the caller hands the dispatch to the worker. Repeated calls while
    /// pending return `None`.
    pub fn confirm(&mut self) -> Option<Dispatch> {
        let MenuState::Confirming(action) = self.state else {
            return None;
        };
        self.last_attempt = self.last_attempt.wrapping_add(1);
        let attempt = self.last_attempt;
        self.state = MenuState::Pending(action);
        Some(Dispatch {
            attempt,
            action,
            effect: action.effect(),
        })
    }

    /// Apply a worker completion only when it belongs to the pending attempt.
    /// Stale, duplicate and out-of-order results are ignored.
    pub fn apply_result(&mut self, event: PowerEvent) -> bool {
        let MenuState::Pending(action) = self.state else {
            return false;
        };
        if event.attempt != self.last_attempt {
            return false;
        }
        self.state = MenuState::Result {
            action,
            outcome: event.outcome,
        };
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_three_actions_have_exact_typed_mappings() {
        assert_eq!(PowerAction::LogOut.effect(), Effect::NiriQuit);
        assert_eq!(
            PowerAction::Restart.effect(),
            Effect::Logind(LogindMethod::Reboot)
        );
        assert_eq!(
            PowerAction::ShutDown.effect(),
            Effect::Logind(LogindMethod::PowerOff)
        );

        assert!(matches!(
            niri_request(),
            Request::Action(Action::Quit {
                skip_confirmation: true
            })
        ));
        assert_eq!(
            serde_json::to_string(&niri_request()).expect("typed niri request serializes"),
            r#"{"Action":{"Quit":{"skip_confirmation":true}}}"#
        );
        assert!(logind_command(PowerAction::LogOut).is_none());

        let reboot = logind_command(PowerAction::Restart).expect("reboot command");
        assert_eq!(reboot.program, "busctl");
        assert_eq!(
            reboot.args,
            [
                "--system",
                "--timeout=60",
                "call",
                "org.freedesktop.login1",
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
                "Reboot",
                "b",
                "true",
            ]
        );

        let power_off = logind_command(PowerAction::ShutDown).expect("power off command");
        assert_eq!(power_off.program, "busctl");
        assert_eq!(
            power_off.args,
            [
                "--system",
                "--timeout=60",
                "call",
                "org.freedesktop.login1",
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
                "PowerOff",
                "b",
                "true",
            ]
        );
    }

    fn synthetic_reply(reply: Reply) -> Vec<u8> {
        serde_json::to_vec(&reply).expect("synthetic niri reply serializes")
    }

    #[test]
    fn niri_reply_classification_separates_acceptance_rejection_and_ambiguity() {
        assert_eq!(
            niri_reply_outcome(&synthetic_reply(Ok(Response::Handled))),
            PowerOutcome::Accepted
        );
        assert_eq!(
            niri_reply_outcome(&synthetic_reply(Err("permission denied".to_owned()))),
            PowerOutcome::Failed(PowerError::PermissionDenied)
        );
        assert_eq!(
            niri_reply_outcome(&synthetic_reply(Err("not authorized".to_owned()))),
            PowerOutcome::Failed(PowerError::PermissionDenied)
        );
        assert_eq!(
            niri_reply_outcome(&synthetic_reply(Err("not a valid action".to_owned()))),
            PowerOutcome::Failed(PowerError::Rejected("not a valid action".to_owned()))
        );
        assert_eq!(
            niri_reply_outcome(&synthetic_reply(Ok(Response::Version("niri".to_owned())))),
            PowerOutcome::Uncertain(PowerError::Protocol)
        );
        assert_eq!(
            niri_reply_outcome(b"not a json reply"),
            PowerOutcome::Uncertain(PowerError::Protocol)
        );
        assert_eq!(
            niri_eof_outcome(),
            PowerOutcome::Uncertain(PowerError::NoReply)
        );
    }

    #[test]
    fn niri_transport_failures_default_to_uncertain_after_send() {
        assert_eq!(
            niri_connect_outcome(std::io::ErrorKind::NotFound),
            PowerOutcome::Failed(PowerError::MissingBackend("niri socket"))
        );
        assert_eq!(
            niri_connect_outcome(std::io::ErrorKind::PermissionDenied),
            PowerOutcome::Failed(PowerError::PermissionDenied)
        );
        assert_eq!(
            niri_connect_outcome(std::io::ErrorKind::ConnectionRefused),
            PowerOutcome::Failed(PowerError::Connection)
        );
        assert_eq!(
            niri_connect_outcome(std::io::ErrorKind::TimedOut),
            PowerOutcome::Failed(PowerError::Connection),
            "a connect timeout happens before any request is sent"
        );
        assert_eq!(
            niri_post_send_outcome(std::io::ErrorKind::TimedOut),
            PowerOutcome::Uncertain(PowerError::Timeout)
        );
        assert_eq!(
            niri_post_send_outcome(std::io::ErrorKind::ConnectionReset),
            PowerOutcome::Uncertain(PowerError::Connection)
        );
    }

    #[test]
    fn logind_classification_keeps_definite_failures_and_defaults_ambiguity_to_uncertain() {
        assert_eq!(
            logind_outcome(LogindStatus::Success, ""),
            PowerOutcome::Accepted
        );
        assert_eq!(
            logind_outcome(LogindStatus::Failure(1), "Access denied"),
            PowerOutcome::Failed(PowerError::PermissionDenied)
        );
        assert_eq!(
            logind_outcome(LogindStatus::Failure(1), "Failed to connect to system bus"),
            PowerOutcome::Failed(PowerError::Connection)
        );
        assert_eq!(
            logind_outcome(LogindStatus::Failure(1), "Operation inhibited by user"),
            PowerOutcome::Failed(PowerError::Rejected(
                "Operation inhibited by user".to_owned()
            ))
        );
        assert_eq!(
            logind_outcome(LogindStatus::Failure(1), "Call timed out"),
            PowerOutcome::Uncertain(PowerError::Timeout)
        );
        assert_eq!(
            logind_outcome(LogindStatus::Failure(1), ""),
            PowerOutcome::Uncertain(PowerError::ProcessFailed)
        );
        assert_eq!(
            logind_outcome(LogindStatus::Failure(1), "odd failure"),
            PowerOutcome::Uncertain(PowerError::HelperFailed(
                "busctl exited with status 1: odd failure".to_owned()
            ))
        );
        assert_eq!(
            logind_outcome(LogindStatus::Signaled, ""),
            PowerOutcome::Uncertain(PowerError::ProcessFailed)
        );
    }

    #[test]
    fn logind_spawn_failures_are_definite() {
        assert_eq!(
            logind_spawn_outcome(std::io::ErrorKind::NotFound),
            PowerOutcome::Failed(PowerError::MissingTool("busctl"))
        );
        assert_eq!(
            logind_spawn_outcome(std::io::ErrorKind::PermissionDenied),
            PowerOutcome::Failed(PowerError::PermissionDenied)
        );
        assert_eq!(
            logind_spawn_outcome(std::io::ErrorKind::Other),
            PowerOutcome::Failed(PowerError::SpawnFailed)
        );
    }

    #[test]
    fn selection_only_opens_confirmation() {
        let mut menu = PowerMenu::default();
        assert!(menu.is_list());
        assert!(menu.select(PowerAction::Restart));
        assert_eq!(menu.state(), &MenuState::Confirming(PowerAction::Restart));
        assert!(menu.confirm().is_some());
    }

    #[test]
    fn confirm_is_single_flight_and_survives_dismiss_and_reopen() {
        let mut menu = PowerMenu::default();
        assert!(menu.select(PowerAction::LogOut));
        let dispatch = menu.confirm().expect("first confirmation dispatches");
        assert_eq!(dispatch.action, PowerAction::LogOut);
        assert_eq!(dispatch.effect, Effect::NiriQuit);
        assert_eq!(menu.state(), &MenuState::Pending(PowerAction::LogOut));

        assert!(menu.confirm().is_none(), "a repeated click cannot rearm");
        assert!(!menu.dismiss(), "dismiss cannot cancel pending work");
        assert!(!menu.on_open(), "reopen cannot rearm pending work");
        assert_eq!(menu.state(), &MenuState::Pending(PowerAction::LogOut));
        assert!(!menu.cancel(), "Cancel cannot cancel pending work");
    }

    #[test]
    fn dismiss_and_cancel_clear_only_an_unconfirmed_action() {
        let mut menu = PowerMenu::default();
        assert!(menu.select(PowerAction::Restart));
        assert!(menu.dismiss());
        assert!(menu.is_list());
        assert!(menu.confirm().is_none());

        assert!(menu.select(PowerAction::ShutDown));
        assert!(menu.on_open());
        assert!(menu.is_list());
        assert!(menu.confirm().is_none());

        assert!(menu.select(PowerAction::LogOut));
        assert!(menu.cancel());
        assert!(menu.is_list());
        assert!(menu.confirm().is_none());
    }

    #[test]
    fn failure_is_persistent_and_retry_needs_a_fresh_confirmation() {
        let mut menu = PowerMenu::default();
        assert!(menu.select(PowerAction::Restart));
        let first = menu.confirm().expect("first dispatch");
        assert!(menu.apply_result(PowerEvent {
            attempt: first.attempt,
            outcome: PowerOutcome::Failed(PowerError::PermissionDenied),
        }));

        let MenuState::Result { action, outcome } = menu.state() else {
            panic!("failure must be shown as a result");
        };
        assert_eq!(*action, PowerAction::Restart);
        assert_eq!(*outcome, PowerOutcome::Failed(PowerError::PermissionDenied));

        // Escape/toggle preserve the failure for the next open.
        assert!(!menu.dismiss());
        assert!(!menu.on_open());
        assert!(matches!(menu.state(), MenuState::Result { .. }));
        assert!(menu.confirm().is_none(), "failure is not a latent confirm");

        // Back to the list, then an explicit select + confirm is required.
        assert!(menu.cancel());
        assert!(menu.is_list());
        assert!(menu.select(PowerAction::Restart));
        let second = menu
            .confirm()
            .expect("retry dispatches only after confirming");
        assert_eq!(second.attempt, first.attempt + 1);

        // A late result from the first attempt cannot complete the retry.
        assert!(!menu.apply_result(PowerEvent {
            attempt: first.attempt,
            outcome: PowerOutcome::Accepted,
        }));
        assert_eq!(menu.state(), &MenuState::Pending(PowerAction::Restart));

        assert!(menu.apply_result(PowerEvent {
            attempt: second.attempt,
            outcome: PowerOutcome::Accepted,
        }));
        assert!(matches!(
            menu.state(),
            MenuState::Result {
                action: PowerAction::Restart,
                outcome: PowerOutcome::Accepted
            }
        ));
    }

    #[test]
    fn timeout_is_reported_as_uncertain_not_as_plain_failure() {
        let mut menu = PowerMenu::default();
        assert!(menu.select(PowerAction::ShutDown));
        let dispatch = menu.confirm().expect("dispatch");
        assert!(menu.apply_result(PowerEvent {
            attempt: dispatch.attempt,
            outcome: PowerOutcome::Uncertain(PowerError::Timeout),
        }));
        let MenuState::Result { outcome, .. } = menu.state() else {
            panic!("result expected");
        };
        assert_eq!(*outcome, PowerOutcome::Uncertain(PowerError::Timeout));
        let text = outcome.summary(PowerAction::ShutDown);
        assert!(text.contains("uncertain"));
        assert!(text.contains("may already be in progress"));
    }

    #[test]
    fn failure_summary_names_the_requested_action() {
        for action in PowerAction::ALL {
            let text = PowerOutcome::Failed(PowerError::PermissionDenied).summary(action);
            assert!(
                text.starts_with(action.label()),
                "{text:?} must identify {action:?}"
            );
            assert!(text.contains("permission was denied"));
        }
    }

    #[test]
    fn acceptance_is_not_described_as_completion() {
        for action in PowerAction::ALL {
            let line = action.accepted_line();
            assert!(line.contains("accepted"));
            assert!(!line.contains("completed"));
            assert!(!line.contains("complete"));
        }

        let mut menu = PowerMenu::default();
        assert!(menu.select(PowerAction::LogOut));
        let dispatch = menu.confirm().expect("dispatch");
        assert!(menu.apply_result(PowerEvent {
            attempt: dispatch.attempt,
            outcome: PowerOutcome::Accepted,
        }));
        assert!(matches!(
            menu.state(),
            MenuState::Result {
                action: PowerAction::LogOut,
                outcome: PowerOutcome::Accepted
            }
        ));
        assert!(!menu.apply_result(PowerEvent {
            attempt: dispatch.attempt,
            outcome: PowerOutcome::Accepted,
        }));
    }

    #[test]
    fn unmatched_results_are_ignored() {
        let mut menu = PowerMenu::default();
        assert!(!menu.apply_result(PowerEvent {
            attempt: 1,
            outcome: PowerOutcome::Accepted,
        }));
        assert!(menu.select(PowerAction::LogOut));
        let dispatch = menu.confirm().expect("dispatch");
        assert!(!menu.apply_result(PowerEvent {
            attempt: dispatch.attempt.saturating_sub(1),
            outcome: PowerOutcome::Failed(PowerError::Connection),
        }));
        assert_eq!(menu.state(), &MenuState::Pending(PowerAction::LogOut));
    }

    #[test]
    fn rejected_detail_is_bounded_and_single_line() {
        let error = PowerError::rejected("  denied\nby\tthe\tbus  ".repeat(40));
        let PowerError::Rejected(detail) = error else {
            panic!("non-empty detail is preserved");
        };
        assert!(detail.chars().count() <= 80);
        assert!(!detail.contains('\n'));
        assert!(!detail.contains('\t'));
        assert_eq!(PowerError::rejected(" \n "), PowerError::Internal);
        assert!(matches!(
            PowerError::helper_failed("odd failure"),
            PowerError::HelperFailed(_)
        ));
        assert_eq!(PowerError::helper_failed(" \n "), PowerError::ProcessFailed);
    }
}

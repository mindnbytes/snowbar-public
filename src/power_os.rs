//! Production-only power effect runner.
//!
//! The binary declares this module behind `cfg(not(test))`; tests use fake
//! channels and never reach a socket or process here. The worker is created
//! with the power overlay, runs one dispatch at a time, and reports exactly
//! one completion. It never retries on its own and catches panics so a helper
//! bug becomes a visible `Internal` result instead of a silent pending state.

use snowbar::power::{Dispatch, PowerError, PowerEvent, PowerOutcome};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

#[cfg(target_os = "linux")]
use snowbar::power::{self, Effect, LogindMethod, LogindStatus};
#[cfg(target_os = "linux")]
use socket2::{Domain, SockAddr, Socket, Type};
#[cfg(target_os = "linux")]
use std::io::{Read, Write};
#[cfg(target_os = "linux")]
use std::os::unix::net::UnixStream;
#[cfg(target_os = "linux")]
use std::process::{Command, Stdio};
#[cfg(target_os = "linux")]
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
const NIRI_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
#[cfg(target_os = "linux")]
const NIRI_SEND_TIMEOUT: Duration = Duration::from_secs(3);
#[cfg(target_os = "linux")]
const NIRI_REPLY_TIMEOUT: Duration = Duration::from_secs(3);
/// niri replies are one short JSON line. A larger or dribbling reply is
/// classified as ambiguous instead of growing without bound.
#[cfg(target_os = "linux")]
const NIRI_MAX_REPLY: usize = 8 * 1024;
/// Generous enough for a normal polkit agent interaction; `busctl` itself
/// keeps its own sixty-second method-call timeout. Killing busctl cannot prove
/// that a call already accepted by logind did not happen, so timeout and
/// signal outcomes are reported as uncertain.
#[cfg(target_os = "linux")]
const LOGIND_TIMEOUT: Duration = Duration::from_secs(70);
#[cfg(target_os = "linux")]
const LOGIND_POLL: Duration = Duration::from_millis(25);
#[cfg(target_os = "linux")]
const STDERR_TIMEOUT: Duration = Duration::from_secs(1);
/// Keep some service detail without letting a noisy helper grow memory. Bytes
/// beyond this are still drained so the pipe cannot fill and stall the child.
#[cfg(target_os = "linux")]
const STDERR_MAX: usize = 4 * 1024;

/// Start the single worker thread and return the dispatch/result ends. A
/// failed thread spawn is returned so the UI can show it on confirmation
/// instead of leaving a pending state.
pub fn start(wake: rune::Waker) -> Result<(Sender<Dispatch>, Receiver<PowerEvent>), PowerError> {
    if !cfg!(target_os = "linux") {
        return Err(PowerError::Unsupported);
    }

    let (dispatch_tx, dispatch_rx) = mpsc::channel::<Dispatch>();
    let (event_tx, event_rx) = mpsc::channel::<PowerEvent>();
    thread::Builder::new()
        .name("snowbar-power".into())
        .spawn(move || {
            while let Ok(dispatch) = dispatch_rx.recv() {
                let outcome =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| execute(dispatch)))
                        .unwrap_or(PowerOutcome::Failed(PowerError::Internal));
                let event = PowerEvent {
                    attempt: dispatch.attempt,
                    outcome,
                };
                if event_tx.send(event).is_err() {
                    break;
                }
                wake();
            }
        })
        .map_err(|_| PowerError::SpawnFailed)?;

    Ok((dispatch_tx, event_rx))
}

#[cfg(target_os = "linux")]
fn execute(dispatch: Dispatch) -> PowerOutcome {
    match dispatch.effect {
        Effect::NiriQuit => run_niri_quit(),
        Effect::Logind(method) => run_logind(method),
    }
}

#[cfg(not(target_os = "linux"))]
fn execute(_dispatch: Dispatch) -> PowerOutcome {
    PowerOutcome::Failed(PowerError::Unsupported)
}

#[cfg(target_os = "linux")]
fn run_niri_quit() -> PowerOutcome {
    let Some(socket_path) = std::env::var_os("NIRI_SOCKET").filter(|path| !path.is_empty()) else {
        return PowerOutcome::Failed(PowerError::MissingBackend("niri socket"));
    };

    let socket = match Socket::new(Domain::UNIX, Type::STREAM, None) {
        Ok(socket) => socket,
        Err(error) => return power::niri_connect_outcome(error.kind()),
    };
    // Apply the send timeout before connect, as the Unix socket is reused for
    // the request. Connect itself uses an absolute timeout.
    if let Err(error) = socket.set_write_timeout(Some(NIRI_SEND_TIMEOUT)) {
        return power::niri_connect_outcome(error.kind());
    }
    let address = match SockAddr::unix(socket_path) {
        Ok(address) => address,
        Err(error) => return power::niri_connect_outcome(error.kind()),
    };
    if let Err(error) = socket.connect_timeout(&address, NIRI_CONNECT_TIMEOUT) {
        return power::niri_connect_outcome(error.kind());
    }
    let mut stream = UnixStream::from(socket);

    let mut payload = match serde_json::to_vec(&power::niri_request()) {
        Ok(payload) => payload,
        Err(_) => return PowerOutcome::Failed(PowerError::Internal),
    };
    payload.push(b'\n');

    let write_deadline = Instant::now() + NIRI_SEND_TIMEOUT;
    let mut written = 0;
    while written < payload.len() {
        let remaining = write_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return if written == 0 {
                PowerOutcome::Failed(PowerError::Connection)
            } else {
                PowerOutcome::Uncertain(PowerError::Timeout)
            };
        }
        if stream.set_write_timeout(Some(remaining)).is_err() {
            return if written == 0 {
                PowerOutcome::Failed(PowerError::Connection)
            } else {
                PowerOutcome::Uncertain(PowerError::Connection)
            };
        }
        match stream.write(&payload[written..]) {
            Ok(0) => {
                return if written == 0 {
                    PowerOutcome::Failed(PowerError::Connection)
                } else {
                    PowerOutcome::Uncertain(PowerError::Connection)
                };
            }
            Ok(count) => written += count,
            Err(error) => {
                return if written == 0 {
                    power::niri_connect_outcome(error.kind())
                } else {
                    power::niri_post_send_outcome(error.kind())
                };
            }
        }
    }

    let read_deadline = Instant::now() + NIRI_REPLY_TIMEOUT;
    let mut reply = Vec::with_capacity(256);
    let mut chunk = [0u8; 512];
    loop {
        let remaining = read_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return PowerOutcome::Uncertain(PowerError::Timeout);
        }
        if stream.set_read_timeout(Some(remaining)).is_err() {
            return PowerOutcome::Uncertain(PowerError::Connection);
        }
        match stream.read(&mut chunk) {
            // A successful quit can close the command socket before the reply
            // is written, so EOF is not proof that the action failed.
            Ok(0) => return power::niri_eof_outcome(),
            Ok(count) => {
                reply.extend_from_slice(&chunk[..count]);
                if reply.len() > NIRI_MAX_REPLY {
                    return PowerOutcome::Uncertain(PowerError::Protocol);
                }
                if let Some(newline) = reply.iter().position(|&byte| byte == b'\n') {
                    reply.truncate(newline);
                    return power::niri_reply_outcome(&reply);
                }
            }
            Err(error) if is_timeout(&error) => {
                return PowerOutcome::Uncertain(PowerError::Timeout);
            }
            Err(error) => return power::niri_post_send_outcome(error.kind()),
        }
    }
}

#[cfg(target_os = "linux")]
fn is_timeout(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    )
}

/// One fixed `busctl` call. `busctl` gets the normal system-bus authorization
/// path (polkit); there is no shell, sudo, force flag or inhibitor bypass.
#[cfg(target_os = "linux")]
fn run_logind(method: LogindMethod) -> PowerOutcome {
    let action = match method {
        LogindMethod::Reboot => power::PowerAction::Restart,
        LogindMethod::PowerOff => power::PowerAction::ShutDown,
    };
    let Some(command) = power::logind_command(action) else {
        return PowerOutcome::Failed(PowerError::Internal);
    };

    let mut child = match Command::new(command.program)
        .args(command.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => return power::logind_spawn_outcome(error.kind()),
    };

    // Drain stderr while the child runs. Reading it only after exit could
    // block the child on a full pipe; waiting for the reader is bounded too.
    let Some(mut stderr) = child.stderr.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return PowerOutcome::Uncertain(PowerError::ProcessFailed);
    };
    let (stderr_tx, stderr_rx) = mpsc::channel();
    if thread::Builder::new()
        .name("snowbar-power-stderr".into())
        .spawn(move || {
            let mut buffer = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                match stderr.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(count) => {
                        let room = STDERR_MAX.saturating_sub(buffer.len());
                        if room > 0 {
                            buffer.extend_from_slice(&chunk[..count.min(room)]);
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            let _ = stderr_tx.send(buffer);
        })
        .is_err()
    {
        let _ = child.kill();
        let _ = child.wait();
        return PowerOutcome::Uncertain(PowerError::ProcessFailed);
    }

    let deadline = Instant::now() + LOGIND_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stderr = stderr_rx.recv_timeout(STDERR_TIMEOUT).unwrap_or_default();
                let text = String::from_utf8_lossy(&stderr);
                let status = match status.code() {
                    Some(0) => LogindStatus::Success,
                    Some(code) => LogindStatus::Failure(code),
                    None => LogindStatus::Signaled,
                };
                return power::logind_outcome(status, &text);
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(LOGIND_POLL),
            Ok(None) => {
                // The call may already have reached logind; killing busctl
                // cannot prove otherwise.
                let _ = child.kill();
                let _ = child.wait();
                return PowerOutcome::Uncertain(PowerError::Timeout);
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return PowerOutcome::Uncertain(PowerError::ProcessFailed);
            }
        }
    }
}

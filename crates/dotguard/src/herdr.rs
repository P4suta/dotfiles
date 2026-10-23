//! Observations shared by the SSH selector and the session health adapter.

use crate::locate;
use rules::herdr_rules::Agent;
use std::io::{self, Read};
use std::os::unix::fs::FileTypeExt;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

pub const SOCKET: &str = ".local/state/herdr/agent.sock";
pub const KEYS: &str = ".config/herdr-agent/keys";
pub const KEY_LIFETIME: &str = "8h";
pub const SERVICE_LABEL: &str = "dev.dotfiles.herdr-agent";
pub const SERVICE_UNIT: &str = "herdr-agent.service";
pub const PROBE_LIMIT: Duration = Duration::from_millis(500);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const KEY_FIELDS: usize = 2;

/// Capture a diagnostic child with a bound, terminating only that child on timeout.
///
/// # Errors
/// Returns process, output, and timeout errors.
pub fn capture(mut command: Command, limit: Duration) -> io::Result<(ExitStatus, Vec<u8>)> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("diagnostic output is missing"))?;
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout.read_to_end(&mut bytes).map(|_| bytes);
        platform::cleanup::report(sender.send(result), "return the SSH diagnostic output");
    });
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            let bytes = receiver
                .recv_timeout(limit.saturating_sub(started.elapsed()))
                .map_err(|_| {
                    io::Error::new(io::ErrorKind::TimedOut, "SSH diagnostic output timed out")
                })??;
            return Ok((status, bytes));
        }
        if started.elapsed() >= limit {
            platform::cleanup::report(child.kill(), "stop the timed out SSH diagnostic child");
            platform::cleanup::report(child.wait(), "reap the SSH diagnostic child");
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "SSH diagnostic timed out",
            ));
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// The configured key material, without comments.
pub fn public_keys(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            line.split_whitespace()
                .take(KEY_FIELDS)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

/// Probe a daemon's socket without loading a key or waiting for approval.
pub fn socket_responding(socket: &Path, limit: Duration) -> bool {
    let mut command = locate::child("ssh-add");
    command.arg("-l").env("SSH_AUTH_SOCK", socket);
    !limit.is_zero()
        && capture(command, limit.min(PROBE_LIMIT))
            .is_ok_and(|(status, _)| matches!(status.code(), Some(0 | 1)))
}

/// Inspect the dedicated agent without contacting 1Password.
pub fn agent(home: &Path) -> Agent {
    let socket = home.join(SOCKET);
    match std::fs::symlink_metadata(&socket) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Agent::Missing,
        Ok(metadata) if metadata.file_type().is_socket() => {}
        Ok(_) | Err(_) => return Agent::Stale,
    }
    let mut command = locate::child("ssh-add");
    command.arg("-L").env("SSH_AUTH_SOCK", socket);
    let Ok((status, bytes)) = capture(command, PROBE_LIMIT) else {
        return Agent::Stale;
    };
    match status.code() {
        Some(0) => {
            let wanted = std::fs::read_to_string(home.join(KEYS))
                .map_or_else(|_| Vec::new(), |text| public_keys(&text));
            let actual = public_keys(&String::from_utf8_lossy(&bytes));
            if !wanted.is_empty() && wanted.iter().all(|key| actual.contains(key)) {
                Agent::Loaded
            } else {
                Agent::Empty
            }
        }
        Some(1) => Agent::Empty,
        _ => Agent::Stale,
    }
}

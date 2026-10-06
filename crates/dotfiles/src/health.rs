//! A bounded probe of process creation, which stalls on Windows when the Defender scan engine stops responding.
//! The probe only observes: it never changes Defender settings.

use crate::refusal::Refusal;
use crate::start_health_rules::{
    Events, NEXT_ACTION, STALL_MILLIS, Start, Verdict, needs_events, verdict,
};
use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Names a probe command line that replaces the default trivial process, so a fixture can stand in for a stalled host.
pub const PROBE_VARIABLE: &str = "DOTFILES_START_PROBE";

const LIMIT: Duration = Duration::from_millis(STALL_MILLIS);

/// The probe this host runs: the configured fixture, a trivial Windows process, or none elsewhere.
#[must_use]
pub fn configured_probe() -> Option<Command> {
    if let Some(line) = std::env::var_os(PROBE_VARIABLE) {
        let line = line.to_string_lossy().into_owned();
        let mut words = line.split_whitespace();
        let mut command = crate::tool::external(words.next()?);
        command.args(words);
        return Some(command);
    }
    if cfg!(windows) {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        let mut command = crate::tool::external(PathBuf::from(root).join("System32/cmd.exe"));
        command.args(["/d", "/c", "exit 0"]);
        return Some(command);
    }
    None
}

fn millis(elapsed: Duration) -> u64 {
    u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
}

/// Starts `command` on another thread and waits at most `limit` for it to exit.
/// Process creation itself stalls, so the caller never blocks on the start.
/// The probe kills a process still running at the limit.
pub(crate) fn timed(mut command: Command, limit: Duration) -> Result<(Start, Vec<u8>)> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let started = Instant::now();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(command.spawn());
    });
    let mut child = match receiver.recv_timeout(limit) {
        Ok(spawned) => spawned.context("start the process-start probe")?,
        Err(_) => return Ok((Start::TimedOut, Vec::new())),
    };
    let mut stdout = child.stdout.take().context("probe output is unavailable")?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = std::io::Read::read_to_end(&mut stdout, &mut bytes);
        bytes
    });
    loop {
        if child.try_wait()?.is_some() {
            let elapsed = millis(started.elapsed());
            return Ok((
                Start::Completed { millis: elapsed },
                reader.join().unwrap_or_default(),
            ));
        }
        if started.elapsed() >= limit {
            let _ = child.kill();
            let _ = child.wait();
            return Ok((Start::TimedOut, Vec::new()));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Counts the stall events in `wevtutil` XML output and lists each as evidence.
#[must_use]
pub fn parse_events(xml: &str) -> (Events, Vec<String>) {
    let mut events = Events::NONE;
    let mut evidence = Vec::new();
    for record in xml.split("<Event ").skip(1) {
        let identifier = record
            .split_once("<EventID")
            .and_then(|(_, rest)| rest.split_once('>'))
            .and_then(|(_, rest)| rest.split_once('<'))
            .map(|(digits, _)| digits.trim());
        let time = record
            .split_once("SystemTime='")
            .or_else(|| record.split_once("SystemTime=\""))
            .and_then(|(_, rest)| rest.split_once(['\'', '"']))
            .map_or("an unknown time", |(time, _)| time);
        let counter = match identifier {
            Some("5008") => &mut events.engine_terminated,
            Some("3002") => &mut events.protection_failed,
            Some("7011") => &mut events.service_timeout,
            _ => continue,
        };
        *counter = counter.saturating_add(1);
        evidence.push(format!(
            "event {} at {time}",
            identifier.unwrap_or_default()
        ));
    }
    (events, evidence)
}

const QUERIES: [(&str, &str); 2] = [
    (
        "Microsoft-Windows-Windows Defender/Operational",
        "*[System[(EventID=5008 or EventID=3002) and TimeCreated[timediff(@SystemTime) <= 3600000]]]",
    ),
    (
        "System",
        "*[System[(EventID=7011) and TimeCreated[timediff(@SystemTime) <= 3600000]]]",
    ),
];

/// The last hour of stall events, or `None` when a query outlasts the bound.
fn recent_events() -> Result<(Option<Events>, Vec<String>)> {
    if !cfg!(windows) {
        return Ok((Some(Events::NONE), Vec::new()));
    }
    let mut total = Events::NONE;
    let mut evidence = Vec::new();
    for (log, query) in QUERIES {
        let mut command = crate::tool::external("wevtutil");
        command
            .args(["qe", log])
            .arg(format!("/q:{query}"))
            .args(["/f:xml", "/c:20", "/rd:true"]);
        let (start, output) = timed(command, LIMIT)?;
        if start == Start::TimedOut {
            evidence.push(format!("the {log} event query did not finish"));
            return Ok((None, evidence));
        }
        let (events, found) = parse_events(&String::from_utf8_lossy(&output));
        total.engine_terminated = total
            .engine_terminated
            .saturating_add(events.engine_terminated);
        total.protection_failed = total
            .protection_failed
            .saturating_add(events.protection_failed);
        total.service_timeout = total.service_timeout.saturating_add(events.service_timeout);
        evidence.extend(found);
    }
    Ok((Some(total), evidence))
}

/// The append-only incident log, which lets the owner compare stalls across incidents.
#[must_use]
pub fn incidents(home: &Path) -> PathBuf {
    home.join(".local/state/start-health/incidents.jsonl")
}

fn record(home: &Path, context: &str, start: Start, evidence: &[String]) -> Result<PathBuf> {
    let path = incidents(home);
    fs::create_dir_all(path.parent().context("incident log has no directory")?)?;
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, millis);
    let start = match start {
        Start::Completed { millis } => serde_json::json!(millis),
        Start::TimedOut => serde_json::Value::Null,
    };
    let line = serde_json::json!({
        "at_ms": at,
        "context": context,
        "start_ms": start,
        "limit_ms": STALL_MILLIS,
        "events": evidence,
    });
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    writeln!(file, "{line}")?;
    Ok(path)
}

/// Refuses work on a host whose process creation stalls, after recording the occurrence.
///
/// # Errors
///
/// Returns a `host.start-stalled` refusal when the probe shows a stall.
/// Returns an error when the probe or an event query can't start, or when it can't record the incident.
pub fn gate_with(home: &Path, probe: Option<Command>, context: &str) -> Result<()> {
    let Some(probe) = probe else {
        return Ok(());
    };
    let (start, _) = timed(probe, LIMIT)?;
    let (events, evidence) = if needs_events(start) {
        recent_events()?
    } else {
        (Some(Events::NONE), Vec::new())
    };
    if verdict(start, events) == Verdict::Healthy {
        return Ok(());
    }
    let path = record(home, context, start, &evidence)?;
    let mut refusal = Refusal::new(
        "host.start-stalled",
        "starting a process on this host stalls, as it does when the Defender scan engine hangs, so work started now would hang; Defender settings stay unchanged",
        NEXT_ACTION,
    )
    .evidence(match start {
        Start::Completed { millis } => format!("a trivial process took {millis} ms"),
        Start::TimedOut => format!("a trivial process did not finish within {STALL_MILLIS} ms"),
    });
    for item in evidence {
        refusal = refusal.evidence(item);
    }
    Err(refusal
        .evidence(format!("recorded in {}", path.display()))
        .into())
}

/// The gate with this host's configured probe.
///
/// # Errors
///
/// Returns a `host.start-stalled` refusal when the probe shows a stall.
/// Returns an error when the probe or an event query can't start, or when it can't record the incident.
pub fn gate(home: &Path, context: &str) -> Result<()> {
    gate_with(home, configured_probe(), context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stall_events_are_counted_with_their_times() {
        let xml = "<Events><Event xmlns='x'><System><EventID>5008</EventID><TimeCreated SystemTime='2026-10-06T01:00:00Z'/></System></Event><Event xmlns='x'><System><EventID Qualifiers='49152'>7011</EventID><TimeCreated SystemTime='2026-10-06T01:00:05Z'/></System></Event><Event xmlns='x'><System><EventID>1116</EventID></System></Event></Events>";
        let (events, evidence) = parse_events(xml);
        assert_eq!(
            events,
            Events {
                engine_terminated: 1,
                protection_failed: 0,
                service_timeout: 1
            }
        );
        assert_eq!(
            evidence,
            [
                "event 5008 at 2026-10-06T01:00:00Z",
                "event 7011 at 2026-10-06T01:00:05Z"
            ]
        );
        assert_eq!(parse_events("").0, Events::NONE);
    }
}

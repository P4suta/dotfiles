//! Agent hook runs that failed rather than refused, kept in owner-local state until the next session start reports them.

use crate::hook_failure_rules::{Summary, summarize};
use anyhow::{Context, Result};
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write as _};
use std::path::Path;

const LOG: &str = ".local/state/dotfiles/hook-failures.log";
/// The log stops growing at this size, so a hook that fails on every call can't fill the disk between sessions.
const MAX_LOG_BYTES: u64 = 256 * 1024;
/// The longest cause a record keeps.
const MAX_CAUSE_CHARS: usize = 200;
const NO_CAUSE: &str = "no output";

/// Records a failure of `hook`, and reports a recording failure on standard error without failing the hook again.
pub fn record(hook: &str, cause: &str) {
    let Some(home) = platform::home::home() else {
        return;
    };
    record_in(&home, hook, cause);
}

pub(crate) fn record_in(home: &Path, hook: &str, cause: &str) {
    if let Err(error) = append(&home.join(LOG), hook, cause) {
        eprintln!("dotfiles: the hook failure record is unavailable: {error:#}");
    }
}

/// Records a panic of `hook` before the default panic report runs.
pub fn record_panics(hook: &'static str) {
    let report = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        record(hook, &panic.to_string());
        report(panic);
    }));
}

/// Prints one line per hook that failed since the last session start, and empties the log.
/// Prints nothing when every hook worked.
pub fn session_start() {
    let Some(home) = platform::home::home() else {
        return;
    };
    match take(&home.join(LOG)) {
        Ok(records) => {
            if let Some(report) = report(&records) {
                print!("{report}");
            }
        }
        Err(error) => eprintln!("dotfiles: the hook failure record is unreadable: {error:#}"),
    }
}

/// Appends one `hook<TAB>cause` line under the file lock, and drops it once the log reaches its size limit.
///
/// # Errors
/// Returns an error when creating, locking, or writing the log fails.
pub fn append(path: &Path, hook: &str, cause: &str) -> Result<()> {
    fs::create_dir_all(path.parent().context("hook failure log directory")?)?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.lock()?;
    if file.metadata()?.len() < MAX_LOG_BYTES {
        file.write_all(line(hook, cause).as_bytes())?;
    }
    Ok(())
}

/// Reads every record and empties the log under the file lock, so each failure reaches one session start.
///
/// # Errors
/// Returns an error when locking, reading, or emptying an existing log fails.
pub fn take(path: &Path) -> Result<String> {
    let mut file = match OpenOptions::new().read(true).write(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(error) => return Err(error.into()),
    };
    file.lock()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    file.set_len(0)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// The session-start lines for the recorded failures, or `None` when there are none.
#[must_use]
pub fn report(records: &str) -> Option<String> {
    let records: Vec<(&str, &str)> = records
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .collect();
    let hooks: Vec<&str> = records.iter().map(|(hook, _)| *hook).collect();
    let summaries = summarize(&hooks);
    if summaries.is_empty() {
        return None;
    }
    let mut report = String::new();
    for Summary {
        first,
        latest,
        count,
    } in summaries
    {
        let hook = records.get(first).map_or("", |(hook, _)| *hook);
        let cause = records.get(latest).map_or("", |(_, cause)| *cause);
        let times = if count == 1 {
            "once".to_owned()
        } else {
            format!("{count} times")
        };
        writeln!(
            report,
            "The dotfiles `{hook}` hook failed {times} since the last session start. Most recent cause: {cause}"
        )
        .ok()?;
    }
    report.push_str("Next action: report these hook failures to the owner as dotfiles defects, or propose a fix.\n");
    Some(report)
}

fn line(hook: &str, cause: &str) -> String {
    let cause: String = cause
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(NO_CAUSE)
        .replace('\t', " ")
        .chars()
        .take(MAX_CAUSE_CHARS)
        .collect();
    format!("{}\t{cause}\n", hook.replace(['\t', '\n'], " "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_start_reports_each_failed_hook_once_and_then_nothing() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let log = directory.path().join("state/hook-failures.log");
        assert_eq!(take(&log)?, "");
        append(
            &log,
            "claude-bash",
            "error: unexpected argument '--rewrite' found\n\nUsage",
        )?;
        append(&log, "issue-guard", "")?;
        append(&log, "claude-bash", "\tpanicked at main.rs")?;
        let report = report(&take(&log)?).unwrap();
        assert_eq!(
            report,
            "The dotfiles `claude-bash` hook failed 2 times since the last session start. Most recent cause: panicked at main.rs\n\
             The dotfiles `issue-guard` hook failed once since the last session start. Most recent cause: no output\n\
             Next action: report these hook failures to the owner as dotfiles defects, or propose a fix.\n"
        );
        assert_eq!(super::report(&take(&log)?), None);
        Ok(())
    }

    #[test]
    fn a_long_cause_keeps_its_first_line_within_the_limit() {
        let cause = "x".repeat(MAX_CAUSE_CHARS * 2);
        let line = line("assure", &format!("{cause}\nsecond"));
        assert_eq!(line.len(), "assure\t".len() + MAX_CAUSE_CHARS + 1);
        assert!(!line.contains("second"));
    }

    #[test]
    fn the_log_stops_growing_at_its_limit() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let log = directory.path().join("hook-failures.log");
        fs::write(&log, "x".repeat(usize::try_from(MAX_LOG_BYTES)?))?;
        append(&log, "assure", "failed")?;
        assert_eq!(fs::metadata(&log)?.len(), MAX_LOG_BYTES);
        Ok(())
    }
}

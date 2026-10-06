//! Helpers shared by both reapers: timestamps, the single-instance lock, desktop notifications, and ERE matching without a regular expression engine.

use platform::process::command;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};

use dotguard::clock::{
    LAST_DAY_OF_MONTH, LAST_HOUR, LAST_MINUTE, LAST_SECOND, MONTHS_PER_YEAR, days_from_civil,
};
pub use dotguard::clock::{SECONDS_PER_DAY, SECONDS_PER_HOUR, SECONDS_PER_MINUTE};

/// The system directories, from Homebrew on Apple silicon to the base system, where launchd's minimal `PATH` misses tools.
const SYSTEM_BIN: [&str; 4] = ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"];
/// The system grep, which every Mac and Linux host has.
const GREP: &str = "/usr/bin/grep";

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

/// Converts an RFC 3339 timestamp to Unix seconds, returning 0 for the zero value or a parse error.
/// Docker reports UTC.
pub fn epoch_of(ts: &str) -> i64 {
    if ts.is_empty() || ts == "0001-01-01T00:00:00Z" {
        return 0;
    }
    let t = ts.split('.').next().unwrap_or(ts);
    let t = t.trim_end_matches('Z');
    let t = t.split('+').next().unwrap_or(t);

    let Some((date, time)) = t.split_once('T') else {
        return 0;
    };
    let mut d = date.split('-');
    let (Some(y), Some(m), Some(day), None) = (d.next(), d.next(), d.next(), d.next()) else {
        return 0;
    };
    let (Ok(y), Ok(m), Ok(day)) = (y.parse::<i64>(), m.parse::<i64>(), day.parse::<i64>()) else {
        return 0;
    };
    if !(1..=MONTHS_PER_YEAR).contains(&m) || !(1..=LAST_DAY_OF_MONTH).contains(&day) {
        return 0;
    }
    let mut parts = time.split(':');
    let (Some(hh), Some(mm), Some(ss), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return 0;
    };
    let (Ok(hh), Ok(mm), Ok(ss)) = (hh.parse::<i64>(), mm.parse::<i64>(), ss.parse::<i64>()) else {
        return 0;
    };
    if !(0..=LAST_HOUR).contains(&hh)
        || !(0..=LAST_MINUTE).contains(&mm)
        || !(0..=LAST_SECOND).contains(&ss)
    {
        return 0;
    }

    let days = days_from_civil(y, m, day);
    days * SECONDS_PER_DAY + hh * SECONDS_PER_HOUR + mm * SECONDS_PER_MINUTE + ss
}

/// `[[DD-]HH:]MM:SS`, as `ps -o etime=` prints it, to seconds.
pub fn etime_seconds(raw: &str) -> i64 {
    let (days, rest) = match raw.split_once('-') {
        Some((day_part, time_part)) => (day_part.parse::<i64>().unwrap_or(0), time_part),
        None => (0, raw),
    };
    let fields: Vec<&str> = rest.split(':').collect();
    let secs = match fields.as_slice() {
        [only] => only.parse::<i64>().unwrap_or(0),
        [minutes, seconds] => {
            minutes.parse::<i64>().unwrap_or(0) * SECONDS_PER_MINUTE
                + seconds.parse::<i64>().unwrap_or(0)
        }
        [hours, minutes, seconds] => {
            hours.parse::<i64>().unwrap_or(0) * SECONDS_PER_HOUR
                + minutes.parse::<i64>().unwrap_or(0) * SECONDS_PER_MINUTE
                + seconds.parse::<i64>().unwrap_or(0)
        }
        _ => 0,
    };
    days * SECONDS_PER_DAY + secs
}

/// Single-instance guard: an atomic `mkdir`, because macOS ships no `flock(1)`.
/// The run steals a lock older than an hour, which a killed run left behind.
pub struct Lock {
    dir: PathBuf,
}

impl Lock {
    pub fn take(state_dir: &Path, _name: &str, now: i64, log: &dyn Fn(&str)) -> Option<Lock> {
        let dir = state_dir.join(".lock");
        if std::fs::create_dir(&dir).is_ok() {
            return Some(Lock { dir });
        }
        let stale = std::fs::metadata(&dir)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(now, |d| now - i64::try_from(d.as_secs()).unwrap_or(0));
        if stale > SECONDS_PER_HOUR {
            log("stealing a stale lock (older than 1h)");
            platform::cleanup::remove_dir_all(&dir);
            if std::fs::create_dir(&dir).is_ok() {
                return Some(Lock { dir });
            }
        }
        log("another instance is running, skipping");
        None
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        platform::cleanup::remove_dir_all(&self.dir);
    }
}

/// Sends a desktop notification through terminal-notifier when present, with osascript as the fallback.
pub fn notify(title: &str, body: &str, enabled: bool) {
    if !enabled {
        return;
    }
    let ok = if cfg!(target_os = "linux") {
        command("notify-send")
            .args(["-a", "dotfiles", title, body])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    } else if let Some(tn) = which("terminal-notifier") {
        command(tn)
            .args(["-title", title, "-message", body, "-group", title])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    } else {
        command("osascript")
            .args([
                "-e",
                &format!("display notification \"{body}\" with title \"{title}\""),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    if !ok {
        eprintln!("warning: could not show the notification {title:?}");
    }
}

/// The reapers run from launchd with a minimal `PATH`, so tools resolve from the directories the dotfiles add to `PATH`.
fn which(program: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let mut dirs: Vec<PathBuf> = vec![home.join(".local/bin")];
    dirs.extend(SYSTEM_BIN.iter().map(PathBuf::from));
    if let Ok(p) = std::env::var("PATH") {
        dirs.extend(std::env::split_paths(&p));
    }
    dirs.iter().map(|d| d.join(program)).find(|p| p.is_file())
}

/// Returns the `lines` that match the ERE, through `grep -E`, because both patterns come from user configuration.
/// `anchor_line` makes the whole line the subject of the match.
/// `None` means grep failed to answer, from a missing binary, an invalid pattern, or an error, and callers must never treat it as an empty match.
pub fn grep_matching(pattern: &str, lines: &[String]) -> Option<Vec<usize>> {
    let mut cmd = command(GREP);
    cmd.args(["-n", "-E", "-e", pattern])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().ok()?;
    // grep answers about what it read, so a short write leaves no answer rather than one that misses lines.
    if let Some(mut si) = child.stdin.take() {
        si.write_all(lines.join("\n").as_bytes()).ok()?;
    }
    let output = child.wait_with_output().ok()?;
    match output.status.code() {
        Some(0) => Some(
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|l| l.split_once(':').and_then(|(n, _)| n.parse::<usize>().ok()))
                .map(|n| n.saturating_sub(1))
                .collect(),
        ),
        Some(1) => Some(Vec::new()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{epoch_of, etime_seconds};

    #[test]
    fn rfc3339_round_trips_through_known_instants() {
        assert_eq!(epoch_of("2024-01-15T10:30:45.123456789Z"), 1_705_314_645);
        assert_eq!(epoch_of("2024-01-15T10:30:45Z"), 1_705_314_645);
        assert_eq!(epoch_of("1970-01-01T00:00:00Z"), 0);
        assert_eq!(epoch_of("0001-01-01T00:00:00Z"), 0);
        assert_eq!(epoch_of(""), 0);
        assert_eq!(epoch_of("not a date"), 0);
        assert_eq!(epoch_of("2024-13-01T00:00:00Z"), 0);
    }

    #[test]
    fn etime_parses_every_shape_ps_prints() {
        assert_eq!(etime_seconds("05:23"), 323);
        assert_eq!(etime_seconds("1:05:23"), 3923);
        assert_eq!(etime_seconds("2-04:17:30"), 188_250);
        assert_eq!(etime_seconds("garbage"), 0);
    }

    /// The adapter runs the host's `/usr/bin/grep`, which exists only on Unix.
    #[cfg(unix)]
    #[test]
    fn grep_matching_reports_indices() {
        use super::grep_matching;
        let lines: Vec<String> = vec!["java".into(), "sleep".into(), "gradlew".into()];
        let hits = grep_matching("java|gradle|kotlin", &lines);
        assert_eq!(hits, Some(vec![0, 2]));
        assert_eq!(grep_matching("nothing", &lines), Some(Vec::new()));
        let dashed: Vec<String> = vec!["-x".into()];
        assert_eq!(grep_matching("-x", &dashed), Some(vec![0]));
        assert_eq!(grep_matching("(", &lines), None);
    }
}

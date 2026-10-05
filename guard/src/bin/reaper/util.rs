//! Shared pieces of both reapers: timestamps, the single-instance lock, best-effort desktop notifications, and ERE matching without a regex engine.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

/// RFC 3339 to unix seconds, 0 on the zero value or a parse error — the same contract the bash versions got from `date -j -f` after trimming the fraction and the zone.
/// Docker always reports UTC.
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
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) {
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
    if !(0..=23).contains(&hh) || !(0..=59).contains(&mm) || !(0..=60).contains(&ss) {
        return 0;
    }

    let days = days_from_civil(y, m, day);
    days * 86_400 + hh * 3_600 + mm * 60 + ss
}

/// Days since the Unix epoch — Howard Hinnant's `days_from_civil`, the same algorithm `guard/src/bypass.rs` uses for the inverse direction.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
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
            minutes.parse::<i64>().unwrap_or(0) * 60 + seconds.parse::<i64>().unwrap_or(0)
        }
        [hours, minutes, seconds] => {
            hours.parse::<i64>().unwrap_or(0) * 3_600
                + minutes.parse::<i64>().unwrap_or(0) * 60
                + seconds.parse::<i64>().unwrap_or(0)
        }
        _ => 0,
    };
    days * 86_400 + secs
}

/// Single-instance guard: an atomic `mkdir`, because macOS ships no `flock(1)`.
/// A lock older than an hour is assumed to belong to a run that was killed mid-flight and is stolen rather than deadlocking the agent forever.
/// Returns the lock path to remove when the run ends.
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
        if stale > 3600 {
            log("stealing a stale lock (older than 1h)");
            let _ = std::fs::remove_dir_all(&dir);
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
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Best-effort desktop notification: terminal-notifier (Brewfile) when present, osascript as the no-dependency fallback.
/// The stdout log line is the real audit record; this is garnish on top.
pub fn notify(title: &str, body: &str, enabled: bool) {
    if !enabled {
        return;
    }
    let ok = if cfg!(target_os = "linux") {
        Command::new("notify-send")
            .args(["-a", "dotfiles", title, body])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    } else if let Some(tn) = which("terminal-notifier") {
        Command::new(tn)
            .args(["-title", title, "-message", body, "-group", title])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    } else {
        Command::new("osascript")
            .args([
                "-e",
                &format!("display notification \"{body}\" with title \"{title}\""),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    let _ = ok;
}

/// The reapers run from launchd, whose PATH is minimal; resolve tools from the same directories the dotfiles machinery puts on PATH.
fn which(program: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let mut dirs: Vec<PathBuf> = vec![
        home.join(".local/bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
    ];
    if let Ok(p) = std::env::var("PATH") {
        dirs.extend(std::env::split_paths(&p));
    }
    dirs.iter().map(|d| d.join(program)).find(|p| p.is_file())
}

/// Which of `lines` match the extended regular expression, via `grep -E`: the two patterns this serves are user configuration, and reimplementing ERE without a regex engine would change what a config edit means.
/// `anchor_line` makes the whole line the subject, like `[[ =~ ]]` did.
/// `None` means grep could not answer (missing, invalid pattern, or failed), which callers must not read as "no match".
pub fn grep_matching(pattern: &str, lines: &[String]) -> Option<Vec<usize>> {
    let mut cmd = Command::new("/usr/bin/grep");
    cmd.args(["-n", "-E", "-e", pattern])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().ok()?;
    if let Some(mut si) = child.stdin.take() {
        let _ = si.write_all(lines.join("\n").as_bytes());
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

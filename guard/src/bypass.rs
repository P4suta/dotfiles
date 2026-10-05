//! Escape hatches and their audit trail.
//!
//! Destructive-operation and language gates have per-invocation overrides.
//! Signing and hook enforcement have none.
//! Each override appends a line to `~/.local/state/git-bypass.log`.

use std::fmt;
use std::fs::OpenOptions;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// Operations that destroy history or working-tree state.
    Force,
    /// Operations that would skip signing or hook enforcement.
    NoVerify,
    /// Text outside the languages of this repository.
    Foreign,
    /// A commit whose tree equals its parent's.
    Empty,
}

impl Category {
    /// The environment variable that waives this category for one invocation, if any.
    pub const fn env(self) -> Option<&'static str> {
        match self {
            Self::Force => Some("ALLOW_FORCE"),
            Self::NoVerify => None,
            Self::Foreign => Some("ALLOW_FOREIGN"),
            Self::Empty => Some("ALLOW_EMPTY"),
        }
    }

    const fn tag(self) -> &'static str {
        match self {
            Self::Force => "force",
            Self::NoVerify => "no-verify",
            Self::Foreign => "foreign",
            Self::Empty => "empty",
        }
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.tag())
    }
}

/// Reports whether this category has a waiver for the current invocation.
///
/// Any non-empty value counts.
pub fn waived(category: Category) -> bool {
    category
        .env()
        .is_some_and(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}

/// Append one line to the bypass log.
/// A failure to write the log never stops the git command.
pub fn record(outcome: &str, category: Category, reason: &str, argv: &[String]) {
    let mut path = crate::locate::home();
    if path.as_os_str().is_empty() {
        return;
    }
    path.push(".local/state");
    let _ = std::fs::create_dir_all(&path);
    path.push("git-bypass.log");

    let cwd = std::env::current_dir().map_or_else(|_| "?".into(), |p| p.display().to_string());
    let line = format!(
        "{}\t{}\t{}\tcwd={}\treason={}\targs={}\n",
        timestamp(),
        outcome,
        category,
        cwd,
        reason,
        argv.join(" ")
    );

    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// The current Coordinated Universal Time as `YYYY-MM-DDTHH:MM:SSZ`.
fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (y, m, d) = civil_from_days(i64::try_from(days).unwrap_or(0));
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// The `civil_from_days` algorithm of Howard Hinnant: days since the Unix epoch to a proleptic-Gregorian date.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        if m <= 2 { y + 1 } else { y },
        u32::try_from(m).unwrap_or(1),
        u32::try_from(d).unwrap_or(1),
    )
}

#[cfg(test)]
mod tests {
    use super::{Category, civil_from_days, waived};

    #[test]
    fn hook_and_signing_checks_have_no_waiver() {
        assert_eq!(Category::NoVerify.env(), None);
        assert!(!waived(Category::NoVerify));
    }

    #[test]
    fn epoch_and_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1)); // a leap year
        assert_eq!(civil_from_days(19_784), (2024, 3, 2)); // the day after Feb 29
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }
}

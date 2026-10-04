//! Escape hatches and the audit trail behind them.
//!
//! Destructive-operation and language gates have per-invocation overrides.
//! Signing and hook enforcement cannot be waived.
//! Overrides are recorded in ~/.local/state/git-bypass.log for review.

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
    /// Text that does not belong in this repository's languages.
    Foreign,
}

impl Category {
    /// The environment variable that waives this category for one invocation, if any.
    pub const fn env(self) -> Option<&'static str> {
        match self {
            Self::Force => Some("ALLOW_FORCE"),
            Self::NoVerify => None,
            Self::Foreign => Some("ALLOW_FOREIGN"),
        }
    }

    const fn tag(self) -> &'static str {
        match self {
            Self::Force => "force",
            Self::NoVerify => "no-verify",
            Self::Foreign => "foreign",
        }
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.tag())
    }
}

/// Is this category waived for the current invocation?
///
/// Any non-empty value counts.
/// Insisting on `=1` would only mean someone who typed `ALLOW_FORCE=true` gets a refusal they will read as a bug in us.
pub fn waived(category: Category) -> bool {
    category
        .env()
        .is_some_and(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}

/// Append one line to the bypass log.
/// Failures are swallowed: not being able to write the audit trail must never be the reason a git command dies.
pub fn record(outcome: &str, category: Category, reason: &str, argv: &[String]) {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let mut path = std::path::PathBuf::from(home);
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

/// RFC 3339 in UTC.
///
/// UTC rather than local time because std has no timezone database and reaching for `chrono` to print one field is not a trade this crate makes.
/// A `Z` is unambiguous; a bare local timestamp with no offset would not be.
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

/// Howard Hinnant's `civil_from_days`: days since the Unix epoch to a proleptic-Gregorian date, with no lookup tables and no leap-second lies.
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

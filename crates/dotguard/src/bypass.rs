//! Escape hatches and their audit trail.
//!
//! Destructive-operation and language gates have per-invocation overrides.
//! Signing and hook enforcement have none.
//! Each override appends a line to `~/.local/state/git-bypass.log`.

use std::fmt;
use std::fs::OpenOptions;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    /// Operations that destroy history or working-tree state.
    Force,
    /// Operations that would skip signing or hook enforcement.
    NoVerify,
    /// Text outside the languages of this repository.
    Foreign,
    /// A commit whose tree equals its parent's.
    Empty,
    /// A post-commit run under `GIT_HOOK_FORCE_SIGN_LOCK`, which skips re-signing and the empty-commit refusal.
    SignLock,
    /// An agent invocation of a legacy command with its recorded reason.
    Legacy,
}

impl Rule {
    /// The environment variable that bypasses this rule for one invocation, if any.
    pub const fn env(self) -> Option<&'static str> {
        match self {
            Self::Force => Some("ALLOW_FORCE"),
            Self::NoVerify => None,
            Self::Foreign => Some("ALLOW_FOREIGN"),
            Self::Empty => Some("ALLOW_EMPTY"),
            Self::SignLock => Some("GIT_HOOK_FORCE_SIGN_LOCK"),
            Self::Legacy => Some("DOTFILES_LEGACY_BYPASS"),
        }
    }

    const fn tag(self) -> &'static str {
        match self {
            Self::Force => "force",
            Self::NoVerify => "no-verify",
            Self::Foreign => "foreign",
            Self::Empty => "empty",
            Self::SignLock => "sign-lock",
            Self::Legacy => "legacy",
        }
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.tag())
    }
}

/// Reports whether the current invocation bypasses this rule.
///
/// Any non-empty value counts.
pub fn bypassed(rule: Rule) -> bool {
    rule.env()
        .is_some_and(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}

/// Append one line to the bypass log.
/// A failure to write the log never stops the git command.
pub fn record(outcome: &str, rule: Rule, reason: &str, argv: &[String]) {
    let mut path = crate::locate::home();
    if path.as_os_str().is_empty() {
        return;
    }
    path.push(".local/state");
    platform::cleanup::report(
        std::fs::create_dir_all(&path),
        format_args!("create {} for the bypass log", path.display()),
    );
    path.push("git-bypass.log");

    let cwd = std::env::current_dir().map_or_else(|_| "?".into(), |p| p.display().to_string());
    let line = format!(
        "{}\t{}\t{}\tcwd={}\treason={}\targs={}\n",
        timestamp(),
        outcome,
        rule,
        cwd,
        reason,
        argv.join(" ")
    );

    platform::cleanup::report(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut log| log.write_all(line.as_bytes())),
        format_args!("append to {}", path.display()),
    );
}

/// The current time in UTC as `YYYY-MM-DDTHH:MM:SSZ`.
fn timestamp() -> String {
    use crate::clock::{SECONDS_PER_DAY, SECONDS_PER_HOUR, SECONDS_PER_MINUTE, civil_from_days};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
    let (days, rem) = (
        secs.div_euclid(SECONDS_PER_DAY),
        secs.rem_euclid(SECONDS_PER_DAY),
    );
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / SECONDS_PER_HOUR,
        (rem % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE,
        rem % SECONDS_PER_MINUTE
    )
}

#[cfg(test)]
mod tests {
    use super::{Rule, bypassed};
    use crate::clock::civil_from_days;

    #[test]
    fn hook_and_signing_checks_have_no_bypass() {
        assert_eq!(Rule::NoVerify.env(), None);
        assert!(!bypassed(Rule::NoVerify));
    }

    #[test]
    fn epoch_and_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1)); // 2024 is a leap year.
        assert_eq!(civil_from_days(19_784), (2024, 3, 2)); // the day after Feb 29
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }
}

//! Automatic failure observations in the skill-ops incident store.

use crate::refusal::{Refusal, string};
use rules::incident_rules::{
    EXPERIMENTAL_OPT_IN, Expected, ExperimentalOptIn, OwnerStop, SkillMode, Source, excluded,
    skill_mode,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

const STATE: &str = ".local/state/skill-ops";
const CONFIG: &str = ".config";
const DIRECTORY: &str = ".config/skill-ops";
const OPT_IN_READ_LIMIT: u64 = 9;
/// The machine-local owner marker that stops skill operations.
pub const DISABLED: &str = ".config/skill-ops/disabled";
pub const EXPERIMENTAL: &str = ".config/skill-ops/experimental";
const EXPECTED: &str = "DOTFILES_EXPECTED_FAILURE";
const HEX: &[u8] = b"0123456789abcdef";
const NIBBLE_BITS: u32 = 4;
const NIBBLE_MASK: u8 = 15;

/// Whether this machine explicitly opted into experimental skill operations.
#[must_use]
pub fn enabled() -> bool {
    crate::home::home().is_some_and(|home| enabled_at(&home))
}

/// Validates machine-local consent without changing it or the skill history.
#[must_use]
pub fn enabled_at(home: &Path) -> bool {
    for parent in [home.join(CONFIG), home.join(DIRECTORY)] {
        match fs::symlink_metadata(parent) {
            Ok(entry) if entry.is_dir() => {}
            _ => return false,
        }
    }
    let stop = match fs::symlink_metadata(home.join(DISABLED)) {
        Ok(_) => OwnerStop::Present,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => OwnerStop::Absent,
        Err(_) => OwnerStop::Unavailable,
    };
    let path = home.join(EXPERIMENTAL);
    let opt_in = match fs::symlink_metadata(&path) {
        Ok(entry) if entry.is_file() => match fs::File::open(path).and_then(|file| {
            let mut content = Vec::new();
            file.take(OPT_IN_READ_LIMIT).read_to_end(&mut content)?;
            Ok(content)
        }) {
            Ok(content) if content == EXPERIMENTAL_OPT_IN => ExperimentalOptIn::Valid,
            Ok(_) => ExperimentalOptIn::Invalid,
            Err(_) => ExperimentalOptIn::Unavailable,
        },
        Ok(_) => ExperimentalOptIn::Invalid,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ExperimentalOptIn::Missing,
        Err(_) => ExperimentalOptIn::Unavailable,
    };
    skill_mode(stop, opt_in) == SkillMode::Experimental
}

/// The expected-failure labels of the current verification process.
#[must_use]
pub fn expected() -> Expected {
    let label = std::env::var(EXPECTED).unwrap_or_default();
    Expected {
        red_test: label == "red-test",
        counterexample: label == "counterexample",
        dry_run: label == "dry-run",
    }
}

/// Records a structured refusal without changing the operation's result.
pub fn record(refusal: &Refusal, source: Source) {
    if !enabled() || excluded(expected()) {
        return;
    }
    let state = std::env::var_os("SKILL_OPS_STATE")
        .map(std::path::PathBuf::from)
        .or_else(|| crate::home::home().map(|home| home.join(STATE)));
    let Some(state) = state else {
        return;
    };
    let session = [
        "CODEX_THREAD_ID",
        "CLAUDE_SESSION_ID",
        "OPENCODE_SESSION_ID",
    ]
    .into_iter()
    .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
    .unwrap_or_else(|| format!("process-{}", std::process::id()));
    crate::cleanup::report(
        write(&state, &session, refusal, source),
        "record the failure observation",
    );
}

fn write(state: &Path, session: &str, refusal: &Refusal, source: Source) -> std::io::Result<()> {
    let mut evidence = String::from("[");
    for (index, item) in std::iter::once(&refusal.cause)
        .chain(&refusal.evidence)
        .enumerate()
    {
        if index > 0 {
            evidence.push(',');
        }
        string(&mut evidence, item);
    }
    evidence.push(']');
    let mut identity = String::from("[");
    for value in [&*refusal.rule, session, source.key()] {
        string(&mut identity, value);
        identity.push(',');
    }
    identity.push_str(&evidence);
    identity.push(']');
    let id: String = Sha256::digest(identity.as_bytes())
        .iter()
        .flat_map(|&byte| {
            [
                char::from(HEX[usize::from(byte >> NIBBLE_BITS)]),
                char::from(HEX[usize::from(byte & NIBBLE_MASK)]),
            ]
        })
        .collect();
    let mut record = format!("{{\"schema\":1,\"id\":\"{id}\",\"rule\":");
    string(&mut record, &refusal.rule);
    record.push_str(",\"session\":");
    string(&mut record, session);
    record.push_str(",\"source\":");
    string(&mut record, source.key());
    record.push_str(",\"evidence\":");
    record.push_str(&evidence);
    record.push_str("}\n");
    let directory = state.join("incidents");
    fs::create_dir_all(&directory)?;
    let destination = directory.join(format!("{id}.json"));
    match fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&destination)
    {
        Ok(mut file) => file.write_all(record.as_bytes()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if fs::read_to_string(destination)? == record {
                Ok(())
            } else {
                Err(std::io::Error::other("conflicting incident identity"))
            }
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_regular_consent_is_required_and_owner_stops_and_lookup_errors_win() {
        let home = tempfile::tempdir().unwrap();
        assert!(!enabled_at(home.path()));
        let marker = home.path().join(DISABLED);
        fs::create_dir_all(marker.parent().unwrap()).unwrap();
        let opt_in = home.path().join(EXPERIMENTAL);
        for invalid in [
            "",
            "enabled",
            "enabled\r\n",
            "true\n",
            " enabled\n",
            "enabled\nextra",
        ] {
            fs::write(&opt_in, invalid).unwrap();
            assert!(!enabled_at(home.path()), "{invalid:?}");
        }
        fs::write(&opt_in, EXPERIMENTAL_OPT_IN).unwrap();
        assert!(enabled_at(home.path()));
        fs::write(&marker, "").unwrap();
        assert!(!enabled_at(home.path()));
        fs::remove_file(&marker).unwrap();
        assert!(enabled_at(home.path()));
        fs::create_dir(&marker).unwrap();
        assert!(!enabled_at(home.path()));
        fs::remove_dir(&marker).unwrap();
        fs::remove_file(&opt_in).unwrap();
        fs::create_dir(&opt_in).unwrap();
        assert!(!enabled_at(home.path()));
        fs::remove_dir_all(home.path().join(".config")).unwrap();
        fs::write(home.path().join(".config"), "not a directory").unwrap();
        assert!(!enabled_at(home.path()));
    }

    #[cfg(unix)]
    #[test]
    fn consent_symlinks_do_not_enable_experimental_collection() {
        let home = tempfile::tempdir().unwrap();
        let opt_in = home.path().join(EXPERIMENTAL);
        fs::create_dir_all(opt_in.parent().unwrap()).unwrap();
        let external = home.path().join("external");
        fs::write(&external, EXPERIMENTAL_OPT_IN).unwrap();
        std::os::unix::fs::symlink(&external, &opt_in).unwrap();
        assert!(!enabled_at(home.path()));
    }

    #[test]
    fn repeated_failure_replays_are_identical_and_distinct_sources_stay_separate() {
        let directory = tempfile::tempdir().unwrap();
        let refusal = Refusal::new("checks.required", "a required check failed", "just check")
            .evidence("lint");
        for _ in 0..2 {
            write(
                directory.path(),
                "session",
                &refusal,
                Source::FailedDelivery,
            )
            .unwrap();
        }
        assert_eq!(
            fs::read_dir(directory.path().join("incidents"))
                .unwrap()
                .count(),
            1
        );
        write(
            directory.path(),
            "session",
            &refusal,
            Source::VerificationDefect,
        )
        .unwrap();
        assert_eq!(
            fs::read_dir(directory.path().join("incidents"))
                .unwrap()
                .count(),
            2
        );
    }
}

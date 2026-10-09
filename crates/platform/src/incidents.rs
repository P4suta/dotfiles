//! Automatic failure observations in the skill-ops incident store.

use crate::refusal::{Refusal, string};
use rules::incident_rules::{Expected, Source, excluded};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::Path;

const STATE: &str = ".local/state/skill-ops";
const EXPECTED: &str = "DOTFILES_EXPECTED_FAILURE";
const HEX: &[u8] = b"0123456789abcdef";
const NIBBLE_BITS: u32 = 4;
const NIBBLE_MASK: u8 = 15;

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
    if excluded(expected()) {
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

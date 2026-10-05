//! Claude Code shell timeouts set from measured durations.
//! PreToolUse records when a Bash or PowerShell call starts and sets its timeout from the signature's history.
//! PostToolUse and PostToolUseFailure record how long it took.

use crate::shell_write_rules::{program, segments};
use crate::timeout_rules::{CLIENT_LIMIT_MS, Timeout, WINDOW, overran, percentile, timeout};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Once the history file grows past this size, the hook rewrites it to the newest runs of each signature.
const COMPACT_BYTES: u64 = 1 << 20;

/// Words that only prepare the shell.
/// A command made of them alone has no signature of its own.
const PREPARATION: [&str; 12] = [
    "cd",
    "export",
    "set",
    "source",
    ".",
    "pushd",
    "popd",
    "echo",
    "printf",
    "true",
    "unset",
    "set-location",
];

/// A word that names a subcommand or recipe rather than a path, an identifier, or a value.
fn plain(word: &str) -> bool {
    let hex = word.len() >= 7 && word.chars().all(|c| c.is_ascii_hexdigit());
    !word.is_empty()
        && word.len() <= 32
        && !word.starts_with('-')
        && !hex
        && !word.chars().all(|c| c.is_ascii_digit())
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-'))
}

/// The signature of one simple command consists of its program and up to two subcommand words.
/// It drops wrappers and arguments.
fn simple(words: &[String]) -> Option<String> {
    let mut start = 0;
    while let Some(word) = words.get(start) {
        let name = program(word);
        let assignment = word.split_once('=').is_some_and(|(name, _)| {
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        });
        if assignment
            || matches!(
                name.as_str(),
                "command" | "exec" | "time" | "nohup" | "sudo"
            )
        {
            start += 1;
        } else if name == "env" {
            start += 1;
            while let Some(option) = words.get(start).filter(|word| word.starts_with('-')) {
                start += if matches!(option.as_str(), "-u" | "-C" | "--unset" | "--chdir") {
                    2
                } else {
                    1
                };
            }
        } else if name == "timeout" {
            start += 2;
        } else if name == "mise"
            && matches!(words.get(start + 1).map(String::as_str), Some("x" | "exec"))
        {
            start += 2;
            if words.get(start).is_some_and(|word| word == "--") {
                start += 1;
            }
        } else {
            break;
        }
    }
    let name = program(words.get(start)?);
    if PREPARATION.contains(&name.as_str()) {
        return None;
    }
    let mut signature = vec![name.clone()];
    let mut index = start + 1;
    while signature.len() < 3 {
        let Some(word) = words.get(index) else { break };
        if name == "git" && matches!(word.as_str(), "-C" | "-c") {
            index += 2;
            continue;
        }
        if !plain(word) {
            break;
        }
        signature.push(word.clone());
        index += 1;
    }
    Some(signature.join(" "))
}

/// The normalized signature of a shell command: the signatures of its first three simple commands that do more than prepare the shell.
pub fn signature(command: &str) -> String {
    let parts: Vec<String> = segments(command)
        .iter()
        .filter_map(|words| simple(words))
        .take(3)
        .collect();
    if parts.is_empty() {
        "shell".to_owned()
    } else {
        parts.join(" && ")
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Run {
    signature: String,
    millis: u64,
    succeeded: bool,
}

/// Per-signature defaults for commands with no history, from the profile.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Defaults {
    default_ms: u64,
    #[serde(default)]
    signatures: BTreeMap<String, u64>,
}

impl Defaults {
    fn read(home: &Path) -> Self {
        fs::read_to_string(home.join(".config/dotfiles/command-timeouts.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(Self {
                default_ms: 120_000,
                signatures: BTreeMap::new(),
            })
    }

    fn of(&self, signature: &str) -> u64 {
        self.signatures
            .get(signature)
            .copied()
            .unwrap_or(self.default_ms)
    }
}

fn state(home: &Path) -> PathBuf {
    home.join(".local/state/dotfiles/command-timing")
}

fn history(home: &Path) -> Vec<Run> {
    runs(&state(home).join("history.jsonl"))
}

/// The runs recorded in `path`.
/// A missing file or an unreadable line holds none.
fn runs(path: &Path) -> Vec<Run> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// The newest durations of `signature`, ascending.
fn window(runs: &[Run], signature: &str) -> Vec<u64> {
    let mut latest: Vec<u64> = runs
        .iter()
        .rev()
        .filter(|run| run.signature == signature)
        .take(WINDOW)
        .map(|run| run.millis)
        .collect();
    latest.sort_unstable();
    latest
}

fn seconds(millis: u64) -> String {
    format!("{:.0} s", millis.div_ceil(1000))
}

/// The start marker of one tool call.
/// Its name matches the identifier of the call, kept to safe characters.
fn marker(home: &Path, id: &str) -> Option<PathBuf> {
    let safe = !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'));
    safe.then(|| state(home).join("starts").join(id))
}

/// Appends one run as a single line write, so concurrent hooks never interleave inside a record.
fn record(home: &Path, run: &Run) -> Result<()> {
    let directory = state(home);
    fs::create_dir_all(&directory)?;
    let path = directory.join("history.jsonl");
    let mut line = serde_json::to_string(run)?;
    line.push('\n');
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?
        .write_all(line.as_bytes())?;
    if fs::metadata(&path)?.len() > COMPACT_BYTES {
        compact(&directory, &path)?;
    }
    Ok(())
}

/// Keeps the newest window of each signature.
/// The rename loses a run that another hook appends meanwhile, which only thins the statistics.
fn compact(directory: &Path, path: &Path) -> Result<()> {
    let runs = runs(path);
    let mut kept: Vec<&Run> = Vec::new();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for run in runs.iter().rev() {
        let count = counts.entry(run.signature.as_str()).or_default();
        if *count < WINDOW {
            *count += 1;
            kept.push(run);
        }
    }
    let mut text = String::new();
    for run in kept.iter().rev() {
        text.push_str(&serde_json::to_string(run)?);
        text.push('\n');
    }
    let temporary = directory.join(format!("history.{}.tmp", std::process::id()));
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path)?;
    Ok(())
}

fn context(event: &str, text: String) -> Value {
    json!({"hookSpecificOutput": {"hookEventName": event, "additionalContext": text}})
}

/// The hook output for one event at `now`, given in milliseconds since the epoch.
/// Returns `None` when the call needs nothing.
pub fn handle(event: &Value, home: &Path, now: u64) -> Result<Option<Value>> {
    if !matches!(event["tool_name"].as_str(), Some("Bash" | "PowerShell")) {
        return Ok(None);
    }
    let input = &event["tool_input"];
    if input["run_in_background"].as_bool() == Some(true) {
        return Ok(None);
    }
    let command = input["command"]
        .as_str()
        .context("shell event has no command")?;
    let signature = signature(command);
    let runs = history(home);
    let latest = window(&runs, &signature);
    let observed = percentile(&latest);
    let id = event["tool_use_id"].as_str().unwrap_or_default();
    let name = event["hook_event_name"].as_str().unwrap_or_default();
    match name {
        "PreToolUse" => {
            if let Some(path) = marker(home, id) {
                fs::create_dir_all(path.parent().context("marker directory")?)?;
                fs::write(path, now.to_string())?;
            }
            let requested = input["timeout"].as_u64();
            let decided = timeout(
                observed,
                Defaults::read(home).of(&signature),
                requested,
                CLIENT_LIMIT_MS,
            );
            let mut text = match observed {
                Some(observed) => format!(
                    "`{signature}` usually finishes within {} (95th percentile of {} runs); timeout {}.",
                    seconds(observed),
                    latest.len(),
                    seconds(decided.millis())
                ),
                None => format!(
                    "`{signature}` has no measured runs; the profile default timeout is {}.",
                    seconds(decided.millis())
                ),
            };
            if observed.is_some_and(|observed| observed >= CLIENT_LIMIT_MS) {
                text.push_str(
                    " It can outlast the client limit; run it with run_in_background instead.",
                );
            }
            let mut output = context(name, text);
            if let Timeout::Set(millis) = decided
                && requested != Some(millis)
            {
                let mut updated = input.clone();
                updated["timeout"] = json!(millis);
                output["hookSpecificOutput"]["updatedInput"] = updated;
            }
            Ok(Some(output))
        }
        "PostToolUse" | "PostToolUseFailure" => {
            let Some(path) = marker(home, id) else {
                return Ok(None);
            };
            let Some(started) = fs::read_to_string(&path)
                .ok()
                .and_then(|text| text.trim().parse::<u64>().ok())
            else {
                return Ok(None);
            };
            let _ = fs::remove_file(&path);
            let millis = now.saturating_sub(started);
            record(
                home,
                &Run {
                    signature: signature.clone(),
                    millis,
                    succeeded: name == "PostToolUse",
                },
            )?;
            Ok(overran(observed, millis).then(|| {
                context(
                    name,
                    format!(
                        "`{signature}` took {}, beyond its expected {}; run it with run_in_background next time.",
                        seconds(millis),
                        seconds(observed.unwrap_or_default())
                    ),
                )
            }))
        }
        _ => Ok(None),
    }
}

/// Reads one hook event from standard input and writes the hook output, if any, to standard output.
pub fn run() -> Result<()> {
    let mut text = String::new();
    std::io::stdin().read_to_string(&mut text)?;
    let event: Value = serde_json::from_str(&text).context("hook event is not JSON")?;
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .context("native home is unavailable")?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis();
    if let Some(output) = handle(&event, Path::new(&home), u64::try_from(now)?)? {
        println!("{output}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_keep_the_program_and_subcommand_and_drop_paths_and_identifiers() {
        for (command, expected) in [
            ("just check", "just check"),
            (
                "mise x -- cargo test --locked --manifest-path xtask/Cargo.toml",
                "cargo test",
            ),
            ("env -u SSH_AUTH_SOCK git commit -F msg.txt", "git commit"),
            ("git -C /c/repo push -u origin feat/x", "git push"),
            ("cd /c/repo && just proofs", "just proofs"),
            ("gh pr view 138 --json state", "gh pr view"),
            ("gh run watch 1234567890", "gh run watch"),
            ("git show 3ef69a8c", "git show"),
            ("C:/tools/cargo.exe build", "cargo build"),
            (
                "timeout 900 domyjob run host --wait -- just check",
                "domyjob run host",
            ),
            ("cd x", "shell"),
            ("just check | tail -5", "just check && tail"),
        ] {
            assert_eq!(signature(command), expected, "{command}");
        }
    }

    fn event(name: &str, command: &str, timeout: Option<u64>) -> Value {
        let mut input = json!({"command": command, "description": "d"});
        if let Some(timeout) = timeout {
            input["timeout"] = json!(timeout);
        }
        json!({"hook_event_name": name, "tool_name": "Bash", "tool_use_id": "toolu_1", "tool_input": input})
    }

    #[test]
    fn a_measured_run_raises_a_short_timeout_on_the_next_call() {
        let home = tempfile::tempdir().unwrap();
        let home = home.path();
        let pre = handle(&event("PreToolUse", "just check", None), home, 1_000)
            .unwrap()
            .unwrap();
        assert_eq!(
            pre["hookSpecificOutput"]["updatedInput"]["timeout"],
            120_000
        );
        assert!(
            pre["hookSpecificOutput"]["additionalContext"]
                .as_str()
                .unwrap()
                .contains("no measured runs")
        );
        assert_eq!(
            handle(&event("PostToolUse", "just check", None), home, 201_000).unwrap(),
            None
        );
        let pre = handle(
            &event("PreToolUse", "just check", Some(60_000)),
            home,
            300_000,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            pre["hookSpecificOutput"]["updatedInput"]["timeout"],
            310_000
        );
        assert_eq!(
            pre["hookSpecificOutput"]["updatedInput"]["description"],
            "d"
        );
        let post = handle(
            &event("PostToolUseFailure", "just check", None),
            home,
            800_000,
        )
        .unwrap()
        .unwrap();
        assert!(
            post["hookSpecificOutput"]["additionalContext"]
                .as_str()
                .unwrap()
                .contains("run_in_background")
        );
        let lines = fs::read_to_string(state(home).join("history.jsonl")).unwrap();
        assert_eq!(lines.lines().count(), 2);
        assert!(lines.contains("\"succeeded\":false"));
    }

    #[test]
    fn a_sufficient_timeout_is_kept_and_other_tools_are_ignored() {
        let home = tempfile::tempdir().unwrap();
        let pre = handle(
            &event("PreToolUse", "just check", Some(500_000)),
            home.path(),
            0,
        )
        .unwrap()
        .unwrap();
        assert!(pre["hookSpecificOutput"].get("updatedInput").is_none());
        let write =
            json!({"hook_event_name": "PreToolUse", "tool_name": "Write", "tool_input": {}});
        assert_eq!(handle(&write, home.path(), 0).unwrap(), None);
        let background = json!({"hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": {"command": "just check", "run_in_background": true}});
        assert_eq!(handle(&background, home.path(), 0).unwrap(), None);
    }

    #[test]
    fn the_profile_default_decides_a_signature_without_history() {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join(".config/dotfiles")).unwrap();
        fs::write(
            home.path().join(".config/dotfiles/command-timeouts.json"),
            r#"{"default_ms": 120000, "signatures": {"just proofs": 600000}}"#,
        )
        .unwrap();
        let pre = handle(&event("PreToolUse", "just proofs", None), home.path(), 0)
            .unwrap()
            .unwrap();
        assert_eq!(
            pre["hookSpecificOutput"]["updatedInput"]["timeout"],
            600_000
        );
    }

    #[test]
    fn the_managed_defaults_parse_and_cover_the_long_project_commands() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../dot_config/dotfiles/command-timeouts.json");
        let defaults: Defaults = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert!(defaults.default_ms <= CLIENT_LIMIT_MS);
        for signature in [
            "just check",
            "just proofs",
            "cargo test",
            "git commit",
            "git push",
        ] {
            assert_eq!(defaults.of(signature), CLIENT_LIMIT_MS, "{signature}");
        }
        assert_eq!(defaults.of("git status"), defaults.default_ms);
    }

    #[test]
    fn compaction_keeps_the_latest_window_of_each_signature() {
        let home = tempfile::tempdir().unwrap();
        let directory = state(home.path());
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("history.jsonl");
        let mut text = String::new();
        for millis in 0..30 {
            for signature in ["a", "b"] {
                text.push_str(
                    &serde_json::to_string(&Run {
                        signature: signature.into(),
                        millis,
                        succeeded: true,
                    })
                    .unwrap(),
                );
                text.push('\n');
            }
        }
        fs::write(&path, text).unwrap();
        compact(&directory, &path).unwrap();
        let runs = history(home.path());
        assert_eq!(runs.len(), 2 * WINDOW);
        assert_eq!(window(&runs, "a").first(), Some(&10));
    }
}

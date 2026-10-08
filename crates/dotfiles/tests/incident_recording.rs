//! The claude-bash hook records every refusal as an incident, run as installed.

use anyhow::{Result, ensure};
use serde_json::json;
use std::fs;
use std::path::Path;

/// Runs one `PreToolUse` event through the installed hook with an isolated incident store.
fn hook(state: &Path, event: &serde_json::Value) -> anyhow::Result<std::process::Output> {
    use std::io::Write as _;
    let mut child = platform::process::command(env!("CARGO_BIN_EXE_dotfiles"))
        .env("SKILL_OPS_STATE", state)
        .arg("claude-bash")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    child
        .stdin
        .as_mut()
        .unwrap_or_else(|| panic!("piped standard input"))
        .write_all(event.to_string().as_bytes())?;
    Ok(child.wait_with_output()?)
}

#[test]
fn every_refusal_records_an_incident_of_its_rule() -> Result<()> {
    let state = tempfile::tempdir()?;
    let event = json!({
        "tool_name": "Bash",
        "session_id": "session-1",
        "tool_input": {"command": "sed -i 's/a/b/' file.txt"}
    });
    let output = hook(state.path(), &event)?;
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("in-place editor"), "{stderr}");
    let incidents: Vec<_> = fs::read_dir(state.path().join("incidents"))?
        .filter_map(std::result::Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|value| value == "json")
        })
        .collect();
    assert_eq!(incidents.len(), 1);
    let recorded: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(incidents[0].path())?)?;
    assert_eq!(recorded["rule"], "hook.shell-write");
    assert_eq!(recorded["session"], "session-1");
    assert_eq!(recorded["source"], "gate-refusal");
    let replay = hook(state.path(), &event)?;
    assert_eq!(replay.status.code(), Some(2));
    let count = fs::read_dir(state.path().join("incidents"))?
        .filter_map(std::result::Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|value| value == "json")
        })
        .count();
    assert_eq!(count, 1, "a retry of the same failure deduplicates");
    Ok(())
}

#[test]
fn a_session_without_an_identity_still_records() -> Result<()> {
    let state = tempfile::tempdir()?;
    let event = json!({
        "tool_name": "Bash",
        "tool_input": {"command": "echo hi > out.txt"}
    });
    let output = hook(state.path(), &event)?;
    assert_eq!(output.status.code(), Some(2));
    let incident = fs::read_dir(state.path().join("incidents"))?
        .filter_map(std::result::Result::ok)
        .find(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|value| value == "json")
        })
        .unwrap_or_else(|| panic!("the refusal recorded an incident"));
    let recorded: serde_json::Value = serde_json::from_str(&fs::read_to_string(incident.path())?)?;
    assert_eq!(recorded["session"], "no-session");
    Ok(())
}

#[test]
fn an_admitted_command_records_nothing() -> Result<()> {
    let state = tempfile::tempdir()?;
    let event = json!({
        "tool_name": "Bash",
        "session_id": "session-2",
        "tool_input": {"command": "just check"}
    });
    let output = hook(state.path(), &event)?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!state.path().join("incidents").exists());
    Ok(())
}

//! The native guard blocks refusals and records their identities in experimental mode.

use anyhow::{Result, ensure};
use serde_json::json;
use std::fs;
use std::path::Path;

#[path = "../../platform/tests/support/fixture_process.rs"]
mod fixture_process;

/// Runs one `PreToolUse` event through the installed hook with an isolated incident store.
fn hook(state: &Path, event: &serde_json::Value) -> anyhow::Result<std::process::Output> {
    use std::io::Write as _;
    let mut child = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), state)
        .env("HOME", state)
        .env("USERPROFILE", state)
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

fn experimental(home: &Path) -> Result<()> {
    let opt_in = home.join(platform::incidents::EXPERIMENTAL);
    fs::create_dir_all(opt_in.parent().unwrap())?;
    fs::write(opt_in, skill_ops::incident_rules::EXPERIMENTAL_OPT_IN)?;
    Ok(())
}

#[test]
fn compound_refusals_preserve_rule_identities_and_never_execute_the_command() -> Result<()> {
    for enabled in [false, true] {
        let state = tempfile::tempdir()?;
        if enabled {
            experimental(state.path())?;
        }
        let event = json!({
            "tool_name": "Bash", "session_id": "aggregate", "cwd": state.path(),
            "tool_input": {"command": "du -sh .; ls -la; python -c 'open(\"out\", \"w\").write(\"x\")'"}
        });
        let output = hook(state.path(), &event)?;
        assert_eq!(output.status.code(), Some(2));
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        for finding in [
            "walks a directory tree",
            "inline interpreter",
            "`ls` is a legacy command",
        ] {
            assert!(diagnostics.contains(finding), "{diagnostics}");
        }
        assert!(!state.path().join("out").exists());
        let incidents = state.path().join("incidents");
        if enabled {
            let rules = fs::read_dir(incidents)?
                .map(|entry| -> Result<String> {
                    let record: serde_json::Value =
                        serde_json::from_slice(&fs::read(entry?.path())?)?;
                    Ok(record["rule"].as_str().unwrap().to_owned())
                })
                .collect::<Result<std::collections::BTreeSet<_>>>()?;
            assert_eq!(
                rules,
                ["hook.disk-scan", "hook.legacy", "hook.shell-write"]
                    .map(str::to_owned)
                    .into_iter()
                    .collect()
            );
        } else {
            assert!(!incidents.exists());
        }
    }
    Ok(())
}

#[test]
fn unavailable_inspection_keeps_later_refusals_blocking_in_both_modes() -> Result<()> {
    for enabled in [false, true] {
        let state = tempfile::tempdir()?;
        if enabled {
            experimental(state.path())?;
        }
        fs::write(state.path().join("broken.sh"), [0xff])?;
        let event = json!({
            "tool_name": "Bash", "session_id": "unavailable", "cwd": state.path(),
            "tool_input": {"command": "sh broken.sh; ls -la; wc -l Cargo.toml; touch executed"}
        });
        let output = hook(state.path(), &event)?;
        assert_eq!(output.status.code(), Some(2));
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        for finding in [
            "could not finish",
            "broken.sh",
            "`ls` is a legacy command",
            "`wc` is a legacy command",
        ] {
            assert!(diagnostics.contains(finding), "{diagnostics}");
        }
        assert!(!state.path().join("executed").exists());
        let incidents = state.path().join("incidents");
        if enabled {
            let rules = fs::read_dir(incidents)?
                .map(|entry| -> Result<String> {
                    let record: serde_json::Value =
                        serde_json::from_slice(&fs::read(entry?.path())?)?;
                    Ok(record["rule"].as_str().unwrap().to_owned())
                })
                .collect::<Result<std::collections::BTreeSet<_>>>()?;
            assert_eq!(
                rules,
                ["hook.inspection", "hook.legacy"]
                    .map(str::to_owned)
                    .into_iter()
                    .collect()
            );
        } else {
            assert!(!incidents.exists());
        }
    }
    Ok(())
}

#[test]
fn the_owner_marker_stops_both_recorders_without_admitting_refused_commands() -> Result<()> {
    let home = tempfile::tempdir()?;
    let marker = home.path().join(platform::incidents::DISABLED);
    fs::create_dir_all(marker.parent().unwrap())?;
    fs::write(marker, "owner stop")?;
    experimental(home.path())?;
    let history = home.path().join("history.json");
    fs::write(&history, "preserved")?;
    let refused = hook(
        home.path(),
        &json!({
            "tool_name":"Bash", "session_id":"disabled",
            "tool_input":{"command":"sed -i 's/a/b/' file.txt"}
        }),
    )?;
    assert_eq!(refused.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("in-place editor"));
    let malformed = hook(
        home.path(),
        &json!({
            "tool_name":"Bash", "tool_input":{"command":7}
        }),
    )?;
    assert_eq!(malformed.status.code(), Some(2));
    assert_ne!(malformed.stderr, Vec::<u8>::new());
    assert!(!home.path().join("incidents").exists());
    assert!(!home.path().join("state").exists());
    assert_eq!(fs::read_to_string(history)?, "preserved");
    Ok(())
}

#[test]
fn every_refusal_records_an_incident_of_its_rule() -> Result<()> {
    let state = tempfile::tempdir()?;
    experimental(state.path())?;
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
    let mut rules = std::collections::BTreeSet::new();
    for incident in &incidents {
        let recorded: serde_json::Value = serde_json::from_slice(&fs::read(incident.path())?)?;
        assert_eq!(recorded["session"], "session-1");
        assert_eq!(recorded["source"], "gate-refusal");
        rules.insert(recorded["rule"].as_str().unwrap().to_owned());
    }
    assert_eq!(
        rules,
        ["hook.legacy", "hook.shell-write"]
            .map(str::to_owned)
            .into_iter()
            .collect()
    );
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
    assert_eq!(count, incidents.len(), "a retry deduplicates each refusal");
    Ok(())
}

#[test]
fn a_session_without_an_identity_still_records() -> Result<()> {
    let state = tempfile::tempdir()?;
    experimental(state.path())?;
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

#[test]
fn every_client_and_subagent_uses_the_write_and_syntax_guards() -> Result<()> {
    let state = tempfile::tempdir()?;
    for event in [
        json!({"tool_name":"Bash", "agent_id":"worker", "tool_input":{"command":"sed -i 's/a/b/' file"}}),
        json!({"tool_name":"bash", "tool_input":{"command":"awk '{print > \"out\"}' file"}}),
        json!({"tool_name":"shell", "tool_input":{"command":["bash", "-lc", "cat > out <<EOF\ntext\nEOF"]}}),
        json!({"tool_name":"Bash", "tool_input":{"command":"if then"}}),
        json!({"tool_name":"bash", "tool_input":{"command":"if then"}}),
        json!({"tool_name":"shell", "tool_input":{"command":["bash", "-lc", "if then"]}}),
    ] {
        let output = hook(state.path(), &event)?;
        assert_eq!(
            output.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

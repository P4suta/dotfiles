//! Native hook execution evidence preserves child decisions and streams.

use anyhow::Result;
use serde_json::Value;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

#[path = "../../platform/tests/support/fixture_process.rs"]
mod fixture_process;

const LOG: &str = ".local/state/dotfiles/codex-hooks.jsonl";

fn runtime(home: &Path) -> Command {
    let mut command = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), home);
    command
        .current_dir(home)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("CODEX_THREAD_ID", "hook-trace-fixture");
    command
}

fn input(mut command: Command, bytes: &[u8]) -> Result<Output> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child.stdin.take().unwrap().write_all(bytes)?;
    Ok(child.wait_with_output()?)
}

fn traced(home: &Path, name: &str, arguments: &[&str]) -> Command {
    let mut command = runtime(home);
    command
        .args(["codex-hooks", "run", "--name", name, "--"])
        .arg(env!("CARGO_BIN_EXE_dotfiles"))
        .args(arguments);
    command
}

fn records(home: &Path) -> Result<Vec<Value>> {
    std::fs::read_to_string(home.join(LOG))?
        .lines()
        .map(|line| Ok(serde_json::from_str(line)?))
        .collect()
}

#[test]
fn hook_trace_preserves_success_and_exposes_actual_execution() -> Result<()> {
    let home = tempfile::tempdir()?;
    let output = input(
        traced(home.path(), "assure-stop", &["assure", "stop"]),
        br#"{"session_id":"payload-session"}"#,
    )?;
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout)?,
        serde_json::json!({})
    );
    let events = records(home.path())?;
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["phase"], "started");
    assert_eq!(events[1]["phase"], "exited");
    assert_eq!(events[0]["id"], events[1]["id"]);
    assert_eq!(events[1]["name"], "assure-stop");
    assert_eq!(events[1]["code"], 0);
    assert_eq!(events[1]["thread_id"], "payload-session");
    assert_eq!(
        events[0]["hook_metadata"]["observed_event_cwd"]["state"],
        "missing"
    );
    assert_eq!(
        events[0]["hook_metadata"]["tool_input_state"]["state"],
        "missing"
    );
    assert_eq!(
        events[0]["hook_metadata"]["observed_tool_input_workdir"]["state"],
        "missing"
    );
    assert_eq!(
        events[0]["hook_metadata"]["derived_execution_directory"]["state"],
        "unavailable"
    );
    let logs = runtime(home.path())
        .args(["codex-hooks", "logs", "--limit", "1"])
        .output()?;
    assert!(logs.status.success(), "{logs:?}");
    assert_eq!(serde_json::from_slice::<Value>(&logs.stdout)?, events[1]);
    Ok(())
}

#[test]
fn hook_trace_preserves_rejection_and_records_its_diagnostic() -> Result<()> {
    let home = tempfile::tempdir()?;
    let mut direct = runtime(home.path());
    direct.arg("claude-bash");
    let bytes = b"not-json private-hook-input-marker";
    let expected = input(direct, bytes)?;
    assert_eq!(expected.status.code(), Some(2));
    let observed = input(traced(home.path(), "shell-guard", &["claude-bash"]), bytes)?;
    assert_eq!(observed.status.code(), expected.status.code());
    assert_eq!(observed.stdout, expected.stdout);
    assert_eq!(observed.stderr, expected.stderr);
    let events = records(home.path())?;
    assert_eq!(events[1]["phase"], "exited");
    assert_eq!(events[1]["code"], 2);
    assert_eq!(events[1]["stderr"], String::from_utf8(expected.stderr)?);
    assert!(events[1].get("hook_metadata").is_none());
    assert!(!std::fs::read_to_string(home.path().join(LOG))?.contains("private-hook-input-marker"));
    Ok(())
}

#[test]
fn hook_trace_records_allowlisted_directory_metadata_without_raw_input() -> Result<()> {
    let home = tempfile::tempdir()?;
    let session = home.path().join("session");
    let execution = home.path().join("execution");
    std::fs::create_dir(&session)?;
    std::fs::create_dir(&execution)?;
    let event = serde_json::json!({
        "session_id":"payload-session",
        "tool_name":"Bash",
        "cwd":session,
        "tool_input":{
            "command":"if (",
            "workdir":execution,
            "cwd":7
        },
        "api_key":"PRIVATE_TOKEN_SENTINEL"
    });
    let output = input(
        traced(home.path(), "shell-guard", &["claude-bash"]),
        &serde_json::to_vec(&event)?,
    )?;
    assert_eq!(output.status.code(), Some(2));

    let events = records(home.path())?;
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["phase"], "started");
    assert_eq!(events[1]["phase"], "exited");
    assert_eq!(events[1]["code"], 2);
    assert_eq!(events[0]["hook_metadata"], events[1]["hook_metadata"]);

    let metadata = &events[1]["hook_metadata"];
    assert_eq!(metadata["observed_tool_name"]["state"], "string");
    assert_eq!(metadata["observed_tool_name"]["value"], "Bash");
    assert_eq!(
        metadata["observed_event_cwd"]["value"],
        session.to_string_lossy().as_ref()
    );
    assert_eq!(metadata["tool_input_state"]["state"], "object");
    assert_eq!(
        metadata["observed_tool_input_workdir"]["value"],
        execution.to_string_lossy().as_ref()
    );
    assert_eq!(metadata["observed_tool_input_cwd"]["state"], "invalid_type");
    assert_eq!(metadata["observed_tool_input_cwd"]["value_type"], "number");
    assert_eq!(metadata["derived_execution_directory"]["state"], "resolved");
    assert_eq!(
        metadata["derived_execution_directory"]["value"],
        execution.to_string_lossy().as_ref()
    );
    assert!(metadata.get("command").is_none());
    assert!(!metadata.to_string().contains("if ("));

    let log = std::fs::read_to_string(home.path().join(LOG))?;
    assert!(!log.contains("PRIVATE_TOKEN_SENTINEL"));
    Ok(())
}

#[test]
fn a_failed_launch_has_failure_evidence_and_unobserved_logs_refuse_success() -> Result<()> {
    let home = tempfile::tempdir()?;
    let missing = runtime(home.path())
        .args(["codex-hooks", "logs"])
        .output()?;
    assert!(!missing.status.success());
    assert_eq!(missing.stdout, Vec::<u8>::new());
    let output = runtime(home.path())
        .args(["codex-hooks", "run", "--name", "missing", "--"])
        .arg(home.path().join("missing-runtime.exe"))
        .stdin(Stdio::null())
        .output()?;
    assert!(!output.status.success());
    let events = records(home.path())?;
    assert_eq!(events.len(), 2);
    assert_eq!(events[1]["phase"], "spawn_failed");
    assert!(events[1]["error"].is_string());
    assert!(events[1].get("code").is_none());
    Ok(())
}

#[test]
fn concurrent_hooks_keep_separate_complete_records() -> Result<()> {
    let home = tempfile::tempdir()?;
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let directory = home.path().to_path_buf();
            std::thread::spawn(move || {
                input(
                    traced(&directory, "assure-stop", &["assure", "stop"]),
                    b"{}",
                )
                .unwrap()
            })
        })
        .collect();
    for worker in workers {
        assert!(worker.join().unwrap().status.success());
    }
    let events = records(home.path())?;
    assert_eq!(events.len(), 8);
    let mut starts = std::collections::BTreeSet::new();
    let mut ends = std::collections::BTreeSet::new();
    for event in events {
        let id = event["id"].as_str().unwrap().to_owned();
        match event["phase"].as_str().unwrap() {
            "started" => assert!(starts.insert(id)),
            "exited" => {
                assert_eq!(event["code"], 0);
                assert!(ends.insert(id));
            }
            other => panic!("unexpected phase: {other}"),
        }
    }
    assert_eq!(starts, ends);
    Ok(())
}

#[test]
fn unavailable_logging_preserves_the_hook_decision() -> Result<()> {
    let home = tempfile::tempdir()?;
    std::fs::write(
        home.path().join(".local"),
        "a file blocks the log directory",
    )?;
    let output = input(
        traced(home.path(), "shell-guard", &["claude-bash"]),
        b"invalid JSON",
    )?;
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("execution evidence unavailable"));
    assert!(stderr.contains("Refused:"));
    Ok(())
}

#[test]
fn hook_log_compaction_keeps_recent_records_and_the_new_run() -> Result<()> {
    let home = tempfile::tempdir()?;
    let log = home.path().join(LOG);
    std::fs::create_dir_all(log.parent().unwrap())?;
    let mut previous = String::new();
    for index in 0..2048 {
        previous.push_str(&serde_json::json!({"old":index, "padding":"p".repeat(512)}).to_string());
        previous.push('\n');
    }
    std::fs::write(&log, &previous)?;
    let output = input(
        traced(home.path(), "assure-stop", &["assure", "stop"]),
        b"{}",
    )?;
    assert!(output.status.success(), "{output:?}");
    let events = records(home.path())?;
    assert_eq!(events.len(), 130);
    assert_eq!(events[0]["old"], 1920);
    assert_eq!(events[129]["phase"], "exited");
    assert_eq!(events[129]["code"], 0);
    assert!(std::fs::metadata(log)?.len() < u64::try_from(previous.len())?);
    Ok(())
}

#[cfg(windows)]
#[test]
fn canonical_codex_events_use_the_native_dialect_and_keep_policy_checks() -> Result<()> {
    let home = tempfile::tempdir()?;
    for (command, shell, allowed) in [
        ("& 'C:/tools/example.exe' --help", None, true),
        ("if ($true) { Get-Date }", None, true),
        ("git status 2>$null", None, true),
        ("if (", None, false),
        ("Write-Output hi > output.txt", None, false),
        ("if true; then printf ok; fi", Some("bash"), true),
        ("if then", Some("bash"), false),
        ("printf hi > output.txt", Some("bash"), false),
    ] {
        let mut event = serde_json::json!({"tool_name":"Bash", "tool_input":{"command":command}});
        if let Some(shell) = shell {
            event["tool_input"]["shell"] = shell.into();
        }
        let output = input(
            traced(home.path(), "claude-bash", &["claude-bash"]),
            &serde_json::to_vec(&event)?,
        )?;
        assert_eq!(output.status.success(), allowed, "{event}: {output:?}");
        assert!(!home.path().join("output.txt").exists());
    }
    let mut direct = runtime(home.path());
    direct.arg("claude-bash");
    let output = input(
        direct,
        br#"{"tool_name":"Bash","tool_input":{"command":"if ($true) { Get-Date }"}}"#,
    )?;
    assert_eq!(output.status.code(), Some(2));
    for malformed in [
        Value::Bool(true),
        Value::String("invalid".into()),
        Value::Null,
    ] {
        let event = serde_json::json!({"tool_name":"Bash", "tool_input":malformed});
        let output = input(
            traced(home.path(), "claude-bash", &["claude-bash"]),
            &serde_json::to_vec(&event)?,
        )?;
        assert_eq!(output.status.code(), Some(2), "{event}: {output:?}");
    }
    Ok(())
}

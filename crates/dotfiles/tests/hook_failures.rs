//! A failed agent hook reaches the next session start once, and a refusal or a clean run never does.

use anyhow::Result;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

#[path = "../../platform/tests/support/fixture_process.rs"]
mod fixture_process;

fn runtime(home: &Path, arguments: &[&str]) -> Command {
    let mut command = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), home);
    command
        .current_dir(home)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .args(arguments);
    command
}

fn run(home: &Path, arguments: &[&str], input: &[u8]) -> Result<Output> {
    let mut child = runtime(home, arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child.stdin.take().unwrap().write_all(input)?;
    Ok(child.wait_with_output()?)
}

fn session_start(home: &Path) -> Result<String> {
    let output = run(home, &["assure", "start"], b"")?;
    assert!(output.status.success(), "{output:?}");
    Ok(String::from_utf8(output.stdout)?)
}

#[test]
fn the_next_session_start_reports_failed_hooks_once() -> Result<()> {
    let home = tempfile::tempdir()?;
    let home = home.path();
    assert_eq!(session_start(home)?, "");

    let stale = run(home, &["claude-bash", "--retired-flag"], b"")?;
    assert_eq!(stale.status.code(), Some(2));
    let malformed = run(home, &["claude-bash"], b"not json")?;
    assert_eq!(malformed.status.code(), Some(2));
    let refused = run(
        home,
        &["claude-bash"],
        br#"{"tool_name":"Bash","session_id":"fixture","tool_input":{"command":"ls"}}"#,
    )?;
    assert_eq!(refused.status.code(), Some(2));
    let passed = run(
        home,
        &["claude-bash"],
        br#"{"tool_name":"Bash","session_id":"fixture","tool_input":{"command":"eza"}}"#,
    )?;
    assert!(passed.status.success(), "{passed:?}");

    let report = session_start(home)?;
    let lines: Vec<&str> = report.lines().collect();
    assert_eq!(lines.len(), 2, "{report}");
    assert!(
        lines[0].starts_with(
            "The dotfiles `claude-bash` hook failed 2 times since the last session start. Most recent cause: Refused: command inspection could not finish"
        ),
        "{report}"
    );
    assert!(lines[1].starts_with("Next action: "), "{report}");
    assert_eq!(session_start(home)?, "");
    Ok(())
}

#[test]
fn syntax_rejections_preserve_diagnostics_without_reporting_a_failed_hook() -> Result<()> {
    let home = tempfile::tempdir()?;
    let home = home.path();
    std::fs::write(home.join("broken.sh"), "if then; touch executed")?;
    for source in [
        "if then; touch executed",
        "if then; ls -la",
        "if then; bash missing.sh",
        "bash -c 'if then; touch executed'",
        "sh <<'END'\nif then; touch executed\nEND\n",
        "bash broken.sh",
        "DOTFILES_LEGACY_BYPASS='broken.sh: fixture' bash broken.sh",
    ] {
        let event = serde_json::json!({
            "tool_name": "Bash",
            "session_id": "syntax-rejection",
            "cwd": home,
            "tool_input": {"command": source},
        });
        let output = run(home, &["claude-bash"], event.to_string().as_bytes())?;
        assert_eq!(output.status.code(), Some(2), "{source}: {output:?}");
        let diagnostics = String::from_utf8(output.stderr)?;
        assert!(
            diagnostics.contains("1:1: `if` must be followed by a statement list"),
            "{source}: {diagnostics}"
        );
        assert!(
            !diagnostics.contains("the pinned shfmt parser could not finish"),
            "{source}: {diagnostics}"
        );
        assert!(!diagnostics.contains("To run this one script anyway"));
        assert!(!home.join("executed").exists());
        assert_eq!(session_start(home)?, "", "{source}");
    }
    Ok(())
}

#[test]
fn overflowed_parser_columns_remain_syntax_rejections() -> Result<()> {
    const COLUMN_OVERFLOW: usize = 1 << 14;
    let home = tempfile::tempdir()?;
    let source = format!("{}if then; touch executed", " ".repeat(COLUMN_OVERFLOW));
    let event = serde_json::json!({
        "tool_name": "Bash",
        "session_id": "syntax-column-overflow",
        "cwd": home.path(),
        "tool_input": {"command": source},
    });
    let output = run(home.path(), &["claude-bash"], event.to_string().as_bytes())?;
    assert_eq!(output.status.code(), Some(2));
    let diagnostics = String::from_utf8(output.stderr)?;
    assert!(diagnostics.contains("1:?: `if` must be followed by a statement list"));
    assert!(!diagnostics.contains("the pinned shfmt parser could not finish"));
    assert!(!home.path().join("executed").exists());
    assert_eq!(session_start(home.path())?, "");
    Ok(())
}

#[test]
fn an_unreadable_script_remains_an_inspection_failure() -> Result<()> {
    let home = tempfile::tempdir()?;
    let event = serde_json::json!({
        "tool_name": "Bash",
        "session_id": "unreadable-script",
        "cwd": home.path(),
        "tool_input": {"command": "bash missing.sh"},
    });
    let output = run(home.path(), &["claude-bash"], event.to_string().as_bytes())?;
    assert_eq!(output.status.code(), Some(2));
    let diagnostics = String::from_utf8(output.stderr)?;
    assert!(diagnostics.contains("inspect the script missing.sh"));
    assert!(
        session_start(home.path())?.starts_with(
            "The dotfiles `claude-bash` hook failed once since the last session start."
        )
    );
    Ok(())
}

#[test]
fn a_codex_hook_that_never_starts_reaches_the_next_session_start() -> Result<()> {
    let home = tempfile::tempdir()?;
    let home = home.path();
    let missing = home.join("missing-hook-program");
    let missing = missing.to_str().unwrap();
    let output = run(
        home,
        &["codex-hooks", "run", "--name", "edit", "--", missing],
        b"{}",
    )?;
    assert!(!output.status.success());
    let report = session_start(home)?;
    assert!(
        report.starts_with("The dotfiles `edit` hook failed once since the last session start."),
        "{report}"
    );
    Ok(())
}

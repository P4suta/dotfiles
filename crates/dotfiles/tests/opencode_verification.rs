//! The `OpenCode` verification plugin gives a session the same start report and stop decision as the other clients' hooks.

use anyhow::{Result, ensure};
use std::fs;
use std::path::Path;

#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

const PARSER: &str = "pub fn parse(text: &str) -> Option<u8> {\n    text.parse().ok()\n}\n";

#[test]
fn answer_evidence_reaches_the_native_checker_and_bounds_corrections() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let home = scope.path().join("home");
    let root = scope.path().join("outside-a-repository");
    fs::create_dir_all(&home)?;
    fs::create_dir(&root)?;
    let output = platform::process::command("bun")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/answer-adapter.ts"))
        .arg(&root)
        .arg(env!("CARGO_BIN_EXE_dotfiles"))
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .output()?;
    ensure!(
        output.status.success(),
        "OpenCode answer adapter failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert!(
        summary["context"]
            .to_string()
            .contains("--session answer-fixture"),
        "{summary}"
    );
    assert_eq!(summary["accepted"], 0, "{summary}");
    let prompts = summary["prompts"].as_array().cloned().unwrap_or_default();
    assert_eq!(prompts.len(), 1, "{summary}");
    assert!(
        prompts[0]
            .as_str()
            .is_some_and(|prompt| prompt.contains("stale or incomplete")),
        "{summary}"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("Stop requirements remain unmet"));
    Ok(())
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[test]
fn a_session_carries_the_start_report_and_a_stop_refusal_returns_once() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let home = scope.path().join("home");
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../home/dot_config/assure"),
        &home.join(".config/assure"),
    )?;
    let config = scope.path().join("gitconfig");
    fs::write(
        &config,
        "[user]\n\tname = Fixture\n\temail = fixture@example.com\n[commit]\n\tgpgsign = false\n[init]\n\tdefaultBranch = main\n",
    )?;
    let root = scope.path().join("repository");
    fs::create_dir(&root)?;
    let git = |arguments: &[&str]| -> Result<()> {
        let output = fixture_git::command("git", &config)
            .current_dir(&root)
            .args(arguments)
            .output()?;
        ensure!(
            output.status.success(),
            "git {arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    };
    git(&["init", "--quiet"])?;
    fs::write(root.join("README.md"), "fixture\n")?;
    git(&["add", "--all"])?;
    git(&["commit", "--quiet", "-m", "start"])?;
    fs::create_dir(root.join("src"))?;
    fs::write(root.join("src/lib.rs"), PARSER)?;

    let runtime = platform::process::command("bun")
        .args(["-e", "console.log(process.execPath)"])
        .output()?;
    ensure!(runtime.status.success(), "bun is unavailable");
    let runtime = String::from_utf8(runtime.stdout)?;
    let output = platform::process::command(runtime.trim())
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/verification-adapter.ts"))
        .arg(&root)
        .arg(env!("CARGO_BIN_EXE_dotfiles"))
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .output()?;
    ensure!(
        output.status.success(),
        "OpenCode adapter failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let system = summary["system"].to_string();
    assert!(
        system.contains("src/lib.rs:1: parser has no owner"),
        "{summary}"
    );
    assert_eq!(summary["repeated"], summary["system"], "{summary}");
    assert_eq!(summary["afterFirstStop"], 1, "{summary}");
    let prompts = summary["prompts"].as_array().cloned().unwrap_or_default();
    assert_eq!(
        prompts.len(),
        1,
        "a second refusal ends the turn: {summary}"
    );
    assert!(
        prompts[0].as_str().is_some_and(
            |prompt| prompt.starts_with("fixture-session: ") && prompt.contains("src/lib.rs")
        ),
        "{summary}"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Stop requirements remain unmet"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

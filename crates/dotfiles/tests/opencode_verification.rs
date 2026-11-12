//! The `OpenCode` verification plugin gives a session the same start report and stop decision as the other clients' hooks.

use anyhow::{Result, ensure};
use std::fs;
use std::path::Path;

#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

#[path = "support/configuration.rs"]
mod configuration;

use configuration::copy as copy_tree;

const PARSER: &str = "pub fn parse(text: &str) -> Option<u8> {\n    text.parse().ok()\n}\n";

#[test]
fn installed_opencode_api_registers_the_native_guards_and_preserves_session_boundaries()
-> Result<()> {
    let scope = tempfile::tempdir()?;
    let root = scope.path().join("repository");
    let home = scope.path().join("home");
    fs::create_dir_all(&root)?;
    fs::create_dir(&home)?;
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../home/dot_config/assure"),
        &home.join(".config/assure"),
    )?;
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../home/dot_config/prose"),
        &home.join(".config/prose"),
    )?;
    let config = scope.path().join("gitconfig");
    fs::write(
        &config,
        "[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n[commit]\n\tgpgsign = false\n",
    )?;
    let initialized = fixture_git::command("git", &config)
        .current_dir(&root)
        .args(["init", "--quiet"])
        .output()?;
    ensure!(initialized.status.success(), "{initialized:?}");
    let output = fixture_git::command("bun", &config)
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/opencode-native-adapter.ts"),
        )
        .arg(&root)
        .arg(env!("CARGO_BIN_EXE_dotfiles"))
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .output()?;
    ensure!(
        output.status.success(),
        "native OpenCode adapter failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn ordinary_answers_complete_without_evidence_or_correction_requests() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let home = scope.path().join("home");
    let root = scope.path().join("outside-a-repository");
    fs::create_dir_all(&home)?;
    fs::create_dir(&root)?;
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../home/dot_config/prose"),
        &home.join(".config/prose"),
    )?;
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
        summary["context"].to_string().contains("short reason"),
        "{summary}"
    );
    assert_eq!(summary["accepted"], 0, "{summary}");
    let prompts = summary["prompts"].as_array().cloned().unwrap_or_default();
    assert!(prompts.is_empty(), "{summary}");
    assert!(!home.join(".local/state/dotfiles/answers").exists());
    assert!(!root.join("draft.txt").exists());
    assert!(!root.join("evidence.json").exists());
    Ok(())
}

#[test]
fn a_session_reminds_the_agent_without_requiring_owner_metadata() -> Result<()> {
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
        system.contains("before every push") && system.contains("short reason"),
        "{summary}"
    );
    assert_eq!(summary["repeated"], summary["system"], "{summary}");
    assert_eq!(summary["afterFirstStop"], 0, "{summary}");
    let prompts = summary["prompts"].as_array().cloned().unwrap_or_default();
    assert!(prompts.is_empty(), "{summary}");
    Ok(())
}

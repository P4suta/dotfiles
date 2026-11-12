//! Routine confirmation requires instructions and a short reason, without evidence submissions.

use anyhow::Result;
use serde_json::json;
use std::io::Write;
use std::path::Path;
use std::process::Stdio;

#[path = "../../platform/tests/support/fixture_process.rs"]
mod fixture_process;

#[path = "support/configuration.rs"]
mod configuration;

#[test]
fn reminders_require_real_checks_and_a_short_reason_without_creating_records() -> Result<()> {
    let home = tempfile::tempdir()?;
    let output = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), home.path())
        .current_dir(home.path())
        .args(["assure", "remind"])
        .output()?;
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout)?;
    assert!(text.contains("before every push"), "{text}");
    assert!(text.contains("short reason"), "{text}");
    assert!(text.contains("unverified"), "{text}");
    assert!(text.contains("ask the user"), "{text}");
    assert!(!home.path().join(".local/state/dotfiles/answers").exists());
    Ok(())
}

#[test]
fn reply_checks_need_no_catalog_request_or_evidence_but_still_check_language() -> Result<()> {
    let home = tempfile::tempdir()?;
    configuration::copy(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../home/dot_config/prose"),
        &home.path().join(".config/prose"),
    )?;
    for (reply, expected) in [
        ("確認済みです。", json!({})),
        ("一回確認しました。", json!("block")),
        (
            "The implementation is complete and all checks passed.",
            json!("block"),
        ),
    ] {
        let mut child = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), home.path())
            .current_dir(home.path())
            .args(["assure", "reply", "--reply-language", "Japanese"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        child.stdin.take().unwrap().write_all(&serde_json::to_vec(
            &json!({"last_assistant_message":reply}),
        )?)?;
        let output = child.wait_with_output()?;
        assert!(output.status.success(), "{output:?}");
        let result: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        if expected.is_object() {
            assert_eq!(result, expected);
        } else {
            assert_eq!(result["decision"], expected);
        }
    }
    assert!(!home.path().join(".local/state/dotfiles/answers").exists());
    Ok(())
}

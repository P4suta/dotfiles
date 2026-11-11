use anyhow::Result;
use serde_json::json;
use std::io::Write;
use std::path::Path;
use std::process::{Output, Stdio};

#[path = "../../platform/tests/support/fixture_process.rs"]
mod fixture_process;

fn hook(root: &Path, command: &str, configured: bool) -> Result<Output> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config = if configured {
        source.join("home/dot_config/prose")
    } else {
        root.join("missing-config")
    };
    let mut child = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), root)
        .arg("claude-bash")
        .current_dir(root)
        .env("PROSE_CONFIG", config)
        .env("PROSE_RUNTIME", root.join("prose-runtime"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let event = json!({"tool_name":"Bash", "cwd":root, "session_id":"github-body-fixture", "tool_input":{"command":command}});
    child
        .stdin
        .take()
        .unwrap()
        .write_all(event.to_string().as_bytes())?;
    Ok(child.wait_with_output()?)
}

fn check_publications(root: &Path, cases: &[(&str, bool, &str)]) -> Result<()> {
    for &(command, accepted, diagnostic) in cases {
        let output = hook(root, command, true)?;
        assert_eq!(
            output.status.code(),
            Some(if accepted { 0 } else { 2 }),
            "{command}: {output:?}"
        );
        if !accepted {
            assert!(
                String::from_utf8_lossy(&output.stderr).contains(diagnostic),
                "{command}: {output:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn the_native_hook_checks_publication_inputs_through_the_installed_bundle_adapter() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    std::fs::write(
        root.join("bad.md"),
        "The check passed. The issue is complete.\n",
    )?;
    std::fs::write(
        root.join("good.md"),
        "The check passed.\nThe issue is complete.\n",
    )?;
    std::fs::write(
        root.join("nested.json"),
        r#"{"input":{"comments":[{"body":"The check passed. The issue is complete."}]}}"#,
    )?;
    check_publications(
        root,
        &[
            (
                "gh issue close 3 --comment 'The check passed. The issue is complete.'",
                false,
                "SentencePerLine",
            ),
            (
                "gh pr close 3 -c 'The check passed. The issue is complete.'",
                false,
                "SentencePerLine",
            ),
            ("gh issue comment 3 -F bad.md", false, "SentencePerLine"),
            ("gh api x --input nested.json", false, "SentencePerLine"),
            ("gh pr comment 3 -F good.md", true, ""),
            ("gh issue close 3 --comment 'The check passed.'", true, ""),
            (
                "gh pr comment 3 --body 'Co-authored-by: A. Human <human@example.com>'",
                true,
                "",
            ),
            (
                "gh pr comment 3 --body 'Co-authored-by: Claude <noreply@anthropic.com>'",
                false,
                "AI attribution",
            ),
            (
                "gh pr comment 3 --body-file missing.md",
                false,
                "inspect GitHub body file",
            ),
        ],
    )
}

#[test]
fn attachments_preserve_explicit_body_inspection() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    std::fs::write(
        root.join("bad.md"),
        "The check passed. The issue is complete.\n",
    )?;
    std::fs::write(
        root.join("good.md"),
        "The check passed.\nThe issue is complete.\n",
    )?;
    check_publications(
        root,
        &[
            (
                "gh pr create --attach before.png --body-file good.md --attach=after.png",
                true,
                "",
            ),
            (
                "gh pr edit 3 --body-file good.md --attach image.png",
                true,
                "",
            ),
            (
                "gh pr comment 3 --attach 'image file.png' --body-file good.md",
                true,
                "",
            ),
            (
                "gh issue create --body-file good.md --attach image.png",
                true,
                "",
            ),
            (
                "gh issue edit 3 --attach image.png --body-file good.md",
                true,
                "",
            ),
            (
                "gh issue comment 3 --attach image.png --body-file good.md",
                true,
                "",
            ),
            (
                "gh pr create --attach image.png --body-file bad.md",
                false,
                "SentencePerLine",
            ),
            (
                "gh pr edit 3 --title 'A title' --attach image.png",
                false,
                "interactive or implicit input",
            ),
            (
                "gh pr create --body-file good.md --attach",
                false,
                "option has no value",
            ),
            (
                "gh pr create --body-file good.md --attach=",
                false,
                "attachment option has no file",
            ),
            (
                "gh pr create --attach image.png --body-file missing.md",
                false,
                "inspect GitHub body file",
            ),
            (
                "gh pr create --body-file good.md --attach image.png --unrecognized-option",
                false,
                "option has no supported body contract",
            ),
        ],
    )
}

#[test]
fn attachment_captions_use_publication_rules() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    std::fs::write(root.join("good.md"), "![A facility.](image.png)\n")?;
    check_publications(
        root,
        &[
            (
                "gh pr create --body-file good.md --attach 'extra.png#First sentence. Second sentence.'",
                false,
                "SentencePerLine",
            ),
            (
                "gh pr create --body-file good.md --attach 'First! Second!.png'",
                false,
                "SentencePerLine",
            ),
            (
                "gh pr create --body-file good.md --attach 'extra.png#First sentence.\nSecond sentence.'",
                false,
                "SentencePerLine",
            ),
            (
                "gh pr create --body-file good.md --attach 'image.png#A facility.'",
                true,
                "",
            ),
            (
                "gh pr create --body-file good.md --attach The.check.passed.png",
                true,
                "",
            ),
            (
                "gh pr create --body-file good.md --attach clip.mp4",
                true,
                "",
            ),
            (
                "gh pr create --body-file good.md --attach .png",
                false,
                "attachment has no filename caption",
            ),
            (
                "gh pr create --body-file good.md --attach 'clip.mp4#A caption'",
                false,
                "video attachment cannot have alt text",
            ),
        ],
    )
}

#[test]
fn a_missing_prose_bundle_blocks_publication_without_blocking_read_only_commands() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let output = hook(
        directory.path(),
        "gh pr comment 3 --body 'The check passed.'",
        false,
    )?;
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("the prose check did not finish"),
        "{output:?}"
    );
    let output = hook(directory.path(), "gh pr view 3 --json body", false)?;
    assert!(output.status.success(), "{output:?}");
    Ok(())
}

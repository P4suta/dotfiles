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

fn attachment_files(root: &Path) -> Result<()> {
    for file in [
        "before.png",
        "after.png",
        "image.png",
        "image file.png",
        "extra.png",
        "First! Second!.png",
        "The.check.passed.png",
        "clip.mp4",
        "First! Second!.mp4",
        ".png",
    ] {
        std::fs::write(root.join(file), b"fixture")?;
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
    attachment_files(root)?;
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
    attachment_files(root)?;
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
fn referenced_images_keep_the_body_alt_text() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    std::fs::write(root.join("good.md"), "![A facility.](image.png)\n")?;
    std::fs::write(root.join("image.png"), b"fixture")?;
    check_publications(
        root,
        &[(
            "gh pr create --body-file good.md --attach 'image.png#First sentence. Second sentence.'",
            true,
            "",
        )],
    )
}

#[test]
fn standalone_video_images_discard_their_alt_text() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    attachment_files(root)?;
    std::fs::write(
        root.join("body.md"),
        "The check passed.\n\n![First sentence. Second sentence.](clip.mp4)\n",
    )?;
    check_publications(
        root,
        &[(
            "gh pr create --body-file body.md --attach clip.mp4",
            true,
            "",
        )],
    )
}

#[test]
fn a_video_replacement_cannot_hide_an_appended_image_caption() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    attachment_files(root)?;
    std::fs::write(
        root.join("body.md"),
        "The check passed.\n\n![See [the screenshot](./image.png)](./clip.mp4)\n",
    )?;
    check_publications(
        root,
        &[(
            "gh pr create --body-file body.md --attach clip.mp4 --attach 'image.png#First sentence. Second sentence.'",
            false,
            "SentencePerLine",
        )],
    )
}

#[test]
fn video_replacements_preserve_writes_to_reference_definitions() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    attachment_files(root)?;
    for (body, accepted, diagnostic) in [
        (
            "![See [the screenshot][screen]](clip.mp4)\n\n[screen]: image.png\n",
            true,
            "",
        ),
        (
            "> [screen]:\n>   image.png\n\n[spare]: image.png\n\n![See [the screenshot][screen]](clip.mp4)\n",
            true,
            "",
        ),
        (
            "![See ![the clip][video]](clip.mp4)\n\n[video]: clip.mp4\n",
            false,
            "reference-style image",
        ),
    ] {
        std::fs::write(root.join("body.md"), format!("The check passed.\n\n{body}"))?;
        check_publications(
            root,
            &[(
                "gh pr create --body-file body.md --attach clip.mp4 --attach 'image.png#First sentence. Second sentence.'",
                accepted,
                diagnostic,
            )],
        )?;
    }
    Ok(())
}

#[test]
fn video_projection_preserves_inline_labels_and_filename_fallbacks() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    attachment_files(root)?;
    for (body, attachments, accepted, diagnostic) in [
        ("Watch ![the clip](clip.mp4) here.\n", "clip.mp4", true, ""),
        ("Watch ![](clip.mp4) here.\n", "clip.mp4", true, ""),
        (
            "Watch ![First sentence. Second sentence.](clip.mp4) here.\n",
            "clip.mp4",
            false,
            "SentencePerLine",
        ),
        (
            "Watch ![](<First! Second!.mp4>) here.\n",
            "'First! Second!.mp4'",
            false,
            "SentencePerLine",
        ),
        (
            "Watch ![See [the screenshot](image.png)](clip.mp4) here.\n",
            "clip.mp4 --attach 'image.png#First sentence. Second sentence.'",
            true,
            "",
        ),
        (
            "> ![First sentence. Second sentence.](clip.mp4)\n",
            "clip.mp4",
            true,
            "",
        ),
        (
            "![The clip][video]\n\n[video]: clip.mp4\n",
            "clip.mp4",
            false,
            "reference-style image",
        ),
    ] {
        std::fs::write(root.join("body.md"), format!("The check passed.\n\n{body}"))?;
        let command = format!("gh pr create --body-file body.md --attach {attachments}");
        check_publications(root, &[(&command, accepted, diagnostic)])?;
    }
    Ok(())
}

#[test]
fn existing_attachment_names_take_precedence_over_hash_delimiters() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    std::fs::write(root.join("good.md"), "The check passed.\n")?;
    std::fs::write(root.join("First! Second!#caption.png"), b"fixture")?;
    check_publications(
        root,
        &[(
            "gh pr create --body-file good.md --attach 'First! Second!#caption.png'",
            false,
            "SentencePerLine",
        )],
    )
}

#[test]
fn attachment_references_follow_markdown_instead_of_opaque_text() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    attachment_files(root)?;
    for (body, file, accepted) in [
        ("![A facility.](./image.png)\n", "image.png", true),
        ("![A facility.](<image file.png>)\n", "image file.png", true),
        ("![A facility.](image%20file.png)\n", "image file.png", true),
        (
            "![A facility.][facility]\n\n[facility]: image.png\n",
            "image.png",
            true,
        ),
        (
            "The check passed.\n\n`![A facility.](image.png)`\n",
            "image.png",
            false,
        ),
        (
            "The check passed.\n\n```\n![A facility.](image.png)\n```\n",
            "image.png",
            false,
        ),
        (
            "![A facility.](https://example.invalid/image.png)\n",
            "image.png",
            false,
        ),
    ] {
        std::fs::write(root.join("body.md"), body)?;
        let command = format!(
            "gh pr create --body-file body.md --attach '{file}#First sentence. Second sentence.'"
        );
        check_publications(
            root,
            &[(
                &command,
                accepted,
                if accepted { "" } else { "SentencePerLine" },
            )],
        )?;
    }
    Ok(())
}

#[test]
fn attachment_references_prefer_the_markdown_directory_before_the_working_directory() -> Result<()>
{
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    attachment_files(root)?;
    std::fs::create_dir(root.join("docs"))?;
    std::fs::write(root.join("docs/image.png"), b"fixture")?;
    std::fs::write(root.join("docs/body.md"), "![A facility.](image.png)\n")?;
    std::fs::write(
        root.join("docs/parent.md"),
        "![A facility.](../image.png)\n",
    )?;
    check_publications(
        root,
        &[
            (
                "gh pr create --body-file docs/body.md --attach 'docs/image.png#First sentence. Second sentence.'",
                true,
                "",
            ),
            (
                "gh pr create --body-file docs/body.md --attach 'image.png#First sentence. Second sentence.' --attach docs/image.png",
                false,
                "SentencePerLine",
            ),
            (
                "gh pr create --body-file docs/parent.md --attach 'image.png#First sentence. Second sentence.'",
                true,
                "",
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

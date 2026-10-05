use crate::body_rules::{self, Verdict as BodyVerdict};
use crate::shell_write_rules::{Source, Verdict, body_sources, inspect, next_action};
use anyhow::{Context, Result};
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};

/// A Git Bash path such as `/c/Users/x` names `C:/Users/x` to Windows programs.
fn native_path(path: &str) -> PathBuf {
    if cfg!(windows) {
        let mut parts = path.splitn(3, '/');
        if let (Some(""), Some(drive), rest) = (parts.next(), parts.next(), parts.next())
            && drive.len() == 1
            && drive.chars().all(|c| c.is_ascii_alphabetic())
        {
            return PathBuf::from(format!(
                "{}:/{}",
                drive.to_ascii_uppercase(),
                rest.unwrap_or_default()
            ));
        }
    }
    PathBuf::from(path)
}

fn read_body(path: &str, directory: Option<&Path>) -> Option<String> {
    let path = native_path(path);
    let path = match directory {
        Some(directory) if path.is_relative() => directory.join(path),
        _ => path,
    };
    std::fs::read_to_string(path).ok()
}

/// The text a source sends; a file that cannot be read sends nothing here, and `gh` reports the failure itself.
fn body_text(source: &Source, directory: Option<&Path>) -> Option<String> {
    match source {
        Source::Inline(text) | Source::Stdin(text) => Some(text.clone()),
        Source::File(path) => read_body(path, directory),
        Source::Json(path) => {
            let text = read_body(path, directory)?;
            let json: Value = serde_json::from_str(&text).ok()?;
            Some(json["body"].as_str().unwrap_or_default().to_owned())
        }
    }
}

fn attribution_refusal(command: &str, directory: Option<&Path>) -> Option<String> {
    for source in body_sources(command) {
        let Some(text) = body_text(&source, directory) else {
            continue;
        };
        let found = dotguard::attribution::find(&text);
        if let BodyVerdict::Refuse(reason) = body_rules::verdict(found.is_some()) {
            let (number, line) = found?;
            return Some(format!(
                "Refused: this gh command sends a body with an AI attribution line (body line {number}: {line}).\nNext action: {}",
                body_rules::next_action(reason)
            ));
        }
    }
    None
}

/// The refusal for one PreToolUse event, or `None` when the tool call is admitted.
pub fn refusal(event: &str) -> Result<Option<String>> {
    let event: Value = serde_json::from_str(event).context("PreToolUse event is not JSON")?;
    if event["tool_name"] != "Bash" {
        return Ok(None);
    }
    let command = event["tool_input"]["command"]
        .as_str()
        .context("Bash event has no command")?;
    if let Verdict::Refuse(reason) = inspect(command) {
        return Ok(Some(format!(
            "Refused: this command writes a file through an inline interpreter, heredoc, or redirect.\nNext action: {}",
            next_action(reason)
        )));
    }
    Ok(attribution_refusal(
        command,
        event["cwd"].as_str().map(Path::new),
    ))
}

/// Reads one event from standard input and exits 2, the blocking status, with the refusal on standard error.
pub fn run() -> Result<()> {
    let mut event = String::new();
    std::io::stdin().read_to_string(&mut event)?;
    if let Some(message) = refusal(&event)? {
        eprintln!("{message}");
        std::process::exit(2);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bash(command: &str) -> String {
        json!({"tool_name": "Bash", "tool_input": {"command": command}}).to_string()
    }

    const FORMS: [&str; 5] = [
        "Claude-Session: https://claude.ai/code/session_01Qesu",
        "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>",
        "co-authored-by: Claude <noreply@anthropic.com>",
        "🤖 Generated with [Claude Code](https://claude.com/claude-code)",
        "https://claude.ai/code/session_01Qesu",
    ];

    const ADMITTED: [&str; 4] = [
        "Co-authored-by: Example User <example@example.com>",
        "Co-authored-by: dependabot[bot] <support@github.com>",
        "Document the CLAUDE.md contract for new contributors.",
        "See https://claude.ai/code/session_01Qesu for the transcript.",
    ];

    fn refused(command: &str) -> String {
        refusal(&bash(command))
            .unwrap()
            .unwrap_or_else(|| panic!("admitted: {command}"))
    }

    #[test]
    fn an_inline_interpreter_file_write_is_refused_with_the_next_action() {
        let message = refusal(&bash("python -c \"open('a','w').write('x')\""))
            .unwrap()
            .unwrap();
        assert!(message.contains("Next action") && message.contains("Edit or Write tool"));
    }

    #[test]
    fn project_commands_and_other_tools_are_admitted() {
        for command in ["just check", "pr-workflow start", "git status", "cat a.md"] {
            assert_eq!(refusal(&bash(command)).unwrap(), None, "{command}");
        }
        let write = json!({"tool_name": "Write", "tool_input": {"file_path": "a"}}).to_string();
        assert_eq!(refusal(&write).unwrap(), None);
    }

    #[test]
    fn a_malformed_event_is_an_error_rather_than_an_admission() {
        assert!(refusal("not json").is_err());
        assert!(refusal(r#"{"tool_name":"Bash","tool_input":{}}"#).is_err());
    }

    #[test]
    fn every_attribution_form_is_refused_on_every_inline_path() {
        for form in FORMS {
            for template in [
                "gh pr create --title t --body \"Body.\n\n{}\"",
                "gh pr edit 3 -b 'Body.\n{}'",
                "gh issue create --body=\"{}\"",
                "gh issue edit 3 --body \"{}\"",
                "gh pr comment 3 --body \"{}\"",
                "gh issue comment 3 -b \"{}\"",
                "gh pr review 3 --approve --body \"{}\"",
                "gh api repos/a/b/issues/3/comments -f body=\"{}\"",
                "gh api repos/a/b/issues/3 -X PATCH -F body=\"{}\"",
                "gh pr create --body \"$(cat <<'EOF'\n{}\nEOF\n)\"",
                "gh pr create --body-file - <<'EOF'\nBody.\n{}\nEOF",
                "cat <<'EOF' | gh issue comment 3 -F -\n{}\nEOF",
                "env -u SSH_AUTH_SOCK gh pr create -R a/b --body \"{}\"",
            ] {
                let command = template.replace("{}", form);
                let message = refused(&command);
                assert!(
                    message.contains("AI attribution") && message.contains("Delete that line"),
                    "{command}: {message}"
                );
            }
        }
    }

    #[test]
    fn the_authors_own_trailers_and_prose_about_claude_are_admitted_on_every_inline_path() {
        for text in ADMITTED {
            for template in [
                "gh pr create --title t --body \"Body.\n\n{}\"",
                "gh pr comment 3 --body \"{}\"",
                "gh api repos/a/b/issues/3/comments -f body=\"{}\"",
                "gh pr create --body-file - <<'EOF'\n{}\nEOF",
                "cat <<'EOF' | gh issue comment 3 -F -\n{}\nEOF",
            ] {
                let command = template.replace("{}", text);
                assert_eq!(refusal(&bash(&command)).unwrap(), None, "{command}");
            }
        }
    }

    #[test]
    fn bodies_read_from_files_are_checked_by_relative_and_absolute_path() {
        let directory = tempfile::tempdir().unwrap();
        let cwd = directory.path().to_str().unwrap();
        let event = |command: &str| {
            json!({"tool_name": "Bash", "cwd": cwd, "tool_input": {"command": command}}).to_string()
        };
        for form in FORMS {
            std::fs::write(
                directory.path().join("body.md"),
                format!("Body.\n\n{form}\n"),
            )
            .unwrap();
            std::fs::write(
                directory.path().join("payload.json"),
                json!({ "body": format!("Body.\n\n{form}") }).to_string(),
            )
            .unwrap();
            let absolute = directory.path().join("body.md");
            let absolute = absolute.to_str().unwrap().replace('\\', "/");
            for command in [
                "gh pr create --body-file body.md".to_owned(),
                "gh pr edit 3 -F body.md".to_owned(),
                "gh issue comment 3 --body-file=body.md".to_owned(),
                format!("gh issue create --body-file \"{absolute}\""),
                "gh api repos/a/b/issues/3/comments -F body=@body.md".to_owned(),
                "gh api repos/a/b/issues/3 -X PATCH --input payload.json".to_owned(),
            ] {
                let message = refusal(&event(&command)).unwrap();
                assert!(
                    message.is_some_and(|m| m.contains("AI attribution")),
                    "{command}: {form}"
                );
            }
        }
        for text in ADMITTED {
            std::fs::write(
                directory.path().join("body.md"),
                format!("Body.\n\n{text}\n"),
            )
            .unwrap();
            std::fs::write(
                directory.path().join("payload.json"),
                json!({ "body": text }).to_string(),
            )
            .unwrap();
            for command in [
                "gh pr create --body-file body.md",
                "gh api repos/a/b/issues/3/comments -F body=@body.md",
                "gh api repos/a/b/issues/3 -X PATCH --input payload.json",
                "gh pr create --body-file missing.md",
            ] {
                assert_eq!(refusal(&event(command)).unwrap(), None, "{command}: {text}");
            }
        }
    }

    #[test]
    fn git_bash_drive_paths_name_windows_paths() {
        if cfg!(windows) {
            assert_eq!(
                native_path("/c/Users/x/b.md"),
                PathBuf::from("C:/Users/x/b.md")
            );
            assert_eq!(native_path("/d"), PathBuf::from("D:/"));
        }
        assert_eq!(native_path("relative/b.md"), PathBuf::from("relative/b.md"));
    }
}

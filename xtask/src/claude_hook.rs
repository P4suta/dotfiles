use crate::body_rules::{self, Verdict as BodyVerdict};
use crate::disk_scan_rules;
use crate::shell_write_rules::{
    Source, Verdict, body_sources, inspect, next_action, program, segments,
};
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

/// Whether `command` walks a tree and whether it reports sizes: `du` and its kin, `find` with a size test or size output, or a recursive `Get-ChildItem` measured by length.
fn disk_scan(command: &str) -> (bool, bool) {
    let mut recursive = false;
    let mut sizes = false;
    for words in segments(command) {
        let start = words
            .iter()
            .position(|word| {
                !word.contains('=')
                    && !matches!(
                        program(word).as_str(),
                        "env" | "sudo" | "command" | "exec" | "time" | "nohup" | "nice"
                    )
            })
            .unwrap_or(words.len());
        let Some(first) = words.get(start) else {
            continue;
        };
        let rest = &words[start + 1..];
        match program(first).as_str() {
            "du" | "dust" | "ncdu" | "gdu" | "diskus" => {
                if !rest
                    .iter()
                    .any(|word| matches!(word.as_str(), "--help" | "--version"))
                {
                    recursive = true;
                    sizes = true;
                }
            }
            "find" => {
                recursive = true;
                let printf_size = rest
                    .windows(2)
                    .any(|pair| pair[0].starts_with("-printf") || pair[0] == "-fprintf")
                    && rest
                        .iter()
                        .any(|word| word.contains("%s") || word.contains("%k"));
                sizes |= rest.iter().any(|word| word == "-size") || printf_size;
            }
            "get-childitem" | "gci" | "dir" | "ls" | "childitem" => {
                recursive |= rest.iter().any(|word| {
                    let word = word.to_ascii_lowercase();
                    word.len() >= 2 && "-recurse".starts_with(&word)
                });
            }
            _ => {}
        }
    }
    // PowerShell reports a file's size as its `Length`, whether measured, sorted, or selected.
    sizes |= command
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
        .any(|word| {
            word.eq_ignore_ascii_case("length") || word.eq_ignore_ascii_case("measure-object")
        });
    (recursive, sizes)
}

fn disk_scan_refusal(command: &str) -> Option<String> {
    let (recursive, sizes) = disk_scan(command);
    (disk_scan_rules::verdict(recursive, sizes) == disk_scan_rules::Verdict::Refuse).then(|| {
        format!(
            "Refused: this command walks a directory tree to measure disk usage.\nNext action: {}",
            disk_scan_rules::NEXT_ACTION
        )
    })
}

/// The refusal for one PreToolUse event, or `None` when the tool call is admitted.
/// PowerShell commands are checked only for disk scans, because the file-write rules read POSIX shell syntax.
pub fn refusal(event: &str) -> Result<Option<String>> {
    let event: Value = serde_json::from_str(event).context("PreToolUse event is not JSON")?;
    let tool = event["tool_name"].as_str().unwrap_or_default();
    if !matches!(tool, "Bash" | "PowerShell") {
        return Ok(None);
    }
    let command = event["tool_input"]["command"]
        .as_str()
        .context("shell event has no command")?;
    if let Some(message) = disk_scan_refusal(command) {
        return Ok(Some(message));
    }
    if tool == "PowerShell" {
        return Ok(None);
    }
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
    fn recursive_disk_scans_are_refused_with_storage_scout() {
        let powershell = |command: &str| {
            json!({"tool_name": "PowerShell", "tool_input": {"command": command}}).to_string()
        };
        for event in [
            bash("du -sh ~/.cargo"),
            bash("sudo du -h --max-depth=1 / | sort -h"),
            bash("find /c/Users -type f -size +100M"),
            bash("find . -printf '%s %p\\n' | sort -n"),
            powershell(
                "Get-ChildItem C:\\Users -Recurse -File | Measure-Object -Property Length -Sum",
            ),
            powershell("gci -r D:\\ | Sort-Object Length -Descending"),
        ] {
            let message = refusal(&event)
                .unwrap()
                .unwrap_or_else(|| panic!("{event}"));
            assert!(message.contains("storage-scout scan"), "{message}");
        }
        for event in [
            bash("du --version"),
            bash("find . -name '*.rs'"),
            bash("ls -la"),
            powershell("Get-ChildItem C:\\Users"),
            powershell("git status 2>$null"),
            powershell("Get-ChildItem -Recurse -Filter *.md"),
        ] {
            assert_eq!(refusal(&event).unwrap(), None, "{event}");
        }
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

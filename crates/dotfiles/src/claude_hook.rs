use crate::body_rules::{self, Verdict as BodyVerdict};
use crate::disk_scan_rules;
use crate::path_conversion_rules;
use crate::shell_write_rules::{
    Source, Verdict, body_sources, inspect, next_action, program, segments,
};
use anyhow::{Context, Result};
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};

/// A Git Bash drive path's parts: the empty root, the drive letter, and the rest.
const DRIVE_PATH_PARTS: usize = 3;

/// The shortest unambiguous abbreviation of a PowerShell switch: its dash and one letter.
const MIN_SWITCH_ABBREVIATION: usize = 2;

/// Claude Code blocks the tool call and shows standard error to the model on this exit status.
const BLOCK: i32 = 2;

/// A Git Bash path such as `/c/Users/x` names `C:/Users/x` to Windows programs.
fn native_path(path: &str) -> PathBuf {
    if cfg!(windows) {
        let mut parts = path.splitn(DRIVE_PATH_PARTS, '/');
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

/// The text a source sends.
/// A file that fails to open sends nothing here, and `gh` reports the failure itself.
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
                // A format option counts only when a format follows it.
                let printf_size = rest.split_last().is_some_and(|(_, options)| {
                    options
                        .iter()
                        .any(|word| word.starts_with("-printf") || word == "-fprintf")
                }) && rest
                    .iter()
                    .any(|word| word.contains("%s") || word.contains("%k"));
                sizes |= rest.iter().any(|word| word == "-size") || printf_size;
            }
            "get-childitem" | "gci" | "dir" | "ls" | "childitem" => {
                recursive |= rest.iter().any(|word| {
                    let word = word.to_ascii_lowercase();
                    word.len() >= MIN_SWITCH_ABBREVIATION && "-recurse".starts_with(&word)
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

/// Whether a domyjob command passes the remote host an argument that looks like an absolute POSIX path, and whether the command turns Git Bash's path conversion off.
fn remote_paths(command: &str) -> (bool, bool) {
    let mut remote_path = false;
    let mut conversion_off = false;
    let off = |word: &str| matches!(word, "MSYS_NO_PATHCONV=1" | "MSYS2_ARG_CONV_EXCL=*");
    for words in segments(command) {
        if words.first().is_some_and(|word| word == "export") {
            conversion_off |= words[1..].iter().any(|word| off(word));
            continue;
        }
        let start = words
            .iter()
            .position(|word| !word.contains('=') && program(word) != "env")
            .unwrap_or(words.len());
        if words
            .get(start)
            .is_none_or(|word| program(word) != "domyjob")
        {
            continue;
        }
        let assigned = words[..start].iter().any(|word| off(word));
        let remote = words[start..]
            .iter()
            .skip_while(|word| *word != "--")
            .skip(1)
            .any(|word| (word.starts_with('/') && !word.starts_with("//")) || word.contains("=/"));
        remote_path |= remote && !assigned;
    }
    (remote_path, conversion_off)
}

/// Git Bash on Windows rewrites remote POSIX paths before domyjob starts, so the remote host receives a Windows path it lacks.
fn path_conversion_refusal(command: &str, converting_shell: bool) -> Option<String> {
    let (remote_path, conversion_off) = remote_paths(command);
    (path_conversion_rules::verdict(converting_shell, remote_path, conversion_off)
        == path_conversion_rules::Verdict::Refuse)
        .then(|| {
            format!(
                "Refused: Git Bash would rewrite the absolute paths this domyjob command sends to the remote host into Windows paths.\nNext action: {}",
                path_conversion_rules::NEXT_ACTION
            )
        })
}

/// The refusal for one `PreToolUse` event, or `None` when the hook admits the tool call.
/// The hook checks PowerShell commands only for disk scans, because the file-write rules read POSIX shell syntax.
pub(crate) fn refusal(event: &str) -> Result<Option<String>> {
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
    if let Some(message) = path_conversion_refusal(command, cfg!(windows)) {
        return Ok(Some(message));
    }
    if let crate::shell_syntax_rules::Verdict::Refuse(reason) =
        crate::shell_syntax_rules::inspect(command)
    {
        return Ok(Some(format!(
            "Refused: this shell command is malformed: {}.\nNext action: {}",
            crate::shell_syntax_rules::cause(reason),
            crate::shell_syntax_rules::next_action(reason)
        )));
    }
    if let Verdict::Refuse(reason) = inspect(command) {
        return Ok(Some(format!(
            "Refused: this command writes a file through an inline interpreter, in-place editor, heredoc, or redirect.\nNext action: {}",
            next_action(reason)
        )));
    }
    Ok(attribution_refusal(
        command,
        event["cwd"].as_str().map(Path::new),
    ))
}

/// Reads one event from standard input and exits 2, the blocking status, with the refusal on standard error.
///
/// # Errors
///
/// Returns an error when it can't read standard input, the event doesn't parse as JSON, or a shell event has no command.
pub fn run() -> Result<()> {
    let mut event = String::new();
    std::io::stdin().read_to_string(&mut event)?;
    if let Some(message) = refusal(&event)? {
        eprintln!("{message}");
        std::process::exit(BLOCK);
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
    fn remote_absolute_paths_that_git_bash_would_rewrite_are_found() {
        for command in [
            "domyjob on mac --wait -- /bin/pwd",
            "cd x && domyjob run mac --wait -- sh -c 'echo' /tmp/a",
            "domyjob.exe on mac -- tool --config=/etc/tool.toml",
        ] {
            assert_eq!(remote_paths(command), (true, false), "{command}");
            assert!(
                path_conversion_refusal(command, true).is_some(),
                "{command}"
            );
            assert_eq!(path_conversion_refusal(command, false), None, "{command}");
        }
        for command in [
            "MSYS_NO_PATHCONV=1 domyjob on mac --wait -- /bin/pwd",
            "domyjob on mac --wait -- just check",
            "domyjob ls mac",
            "ls /bin/pwd",
            "domyjob on mac -- //server/share",
        ] {
            assert_eq!(path_conversion_refusal(command, true), None, "{command}");
        }
        let exported = "export MSYS_NO_PATHCONV=1; domyjob on mac -- /bin/pwd";
        assert_eq!(remote_paths(exported), (true, true));
        assert_eq!(path_conversion_refusal(exported, true), None);
    }

    #[test]
    fn an_inline_interpreter_file_write_is_refused_with_the_next_action() {
        let message = refusal(&bash("python -c \"open('a','w').write('x')\""))
            .unwrap()
            .unwrap();
        assert!(message.contains("Next action") && message.contains("Edit or Write tool"));
    }

    #[test]
    fn an_in_place_editor_write_is_refused_with_the_next_action() {
        for command in [
            "sed -i 's/a/b/' file.txt",
            "awk -i inplace '{print $1}' data.tsv",
            "awk '{print $1 > \"out.txt\"}' data.tsv",
        ] {
            let message = refusal(&bash(command))
                .unwrap()
                .unwrap_or_else(|| panic!("{command}"));
            assert!(
                message.contains("in-place editor") && message.contains("Edit tool"),
                "{command}: {message}"
            );
        }
        for command in ["sed -n 's/a/b/p' file.txt", "awk '{print $1}' data.tsv"] {
            assert_eq!(refusal(&bash(command)).unwrap(), None, "{command}");
        }
    }

    #[test]
    fn the_incident_command_is_refused_with_its_corrected_form() {
        let message = refusal(&bash(r#"export CARGO_TARGET_DIR="D:\cargo\$k""#))
            .unwrap()
            .unwrap();
        assert!(
            message.contains("malformed") && message.contains("forward slashes"),
            "{message}"
        );
        for command in [
            "echo \"unterminated",
            "echo \"${name\"",
            "export DIR='D:\\cargo$k'",
        ] {
            let message = refusal(&bash(command))
                .unwrap()
                .unwrap_or_else(|| panic!("{command}"));
            assert!(message.contains("malformed"), "{command}: {message}");
        }
        for command in [
            "export CARGO_TARGET_DIR=\"D:/cargo/$name\"",
            "cargo build 2>&1",
            "echo 'D:\\cargo'",
        ] {
            assert_eq!(refusal(&bash(command)).unwrap(), None, "{command}");
        }
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

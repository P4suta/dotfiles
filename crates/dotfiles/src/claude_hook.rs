use crate::body_rules::{self, Verdict as BodyVerdict};
use crate::disk_scan_rules;
use crate::path_conversion_rules;
use crate::shell_syntax_rules;
use crate::shell_write_rules::{
    Source, Verdict, body_sources, inspect, next_action, program, segments,
};
use anyhow::{Context, Result};
use serde_json::Value;
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

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

/// One refusal the hook blocks a tool call with, and the incident it records.
pub(crate) struct Blocked {
    /// The dotted rule that refused, which the incident store keys its discharge by.
    pub rule: &'static str,
    /// Where the incident came from.
    pub source: skill_ops::incident_rules::Source,
    /// The session the event names, or an anonymous stand-in.
    pub session: String,
    /// The refusal text, which names the next action.
    pub message: String,
}

/// The session identity an event without one records, so the incident still deduplicates.
const NO_SESSION: &str = "no-session";

fn shell_command(event: &Value) -> Result<String> {
    let input = &event["tool_input"];
    let command = input.get("command").or_else(|| input.get("cmd"));
    match command {
        Some(Value::String(text)) => Ok(text.clone()),
        Some(Value::Array(words)) => {
            let words: Vec<&str> = words
                .iter()
                .map(|word| word.as_str().context("shell command word is not a string"))
                .collect::<Result<_>>()?;
            if words
                .first()
                .is_some_and(|word| matches!(program(word).as_str(), "pwsh" | "powershell"))
                && let Some(position) = words.iter().position(|word| {
                    word.eq_ignore_ascii_case("-Command") || word.eq_ignore_ascii_case("-c")
                })
                && let Some(script) = words.get(position + 1)
            {
                return Ok((*script).to_owned());
            }
            match words.as_slice() {
                [shell, flag, script]
                    if matches!(program(shell).as_str(), "bash" | "sh" | "zsh" | "dash")
                        && flag.starts_with('-')
                        && flag.ends_with('c') =>
                {
                    Ok((*script).to_owned())
                }
                _ => Ok(words
                    .iter()
                    .map(|word| crate::refusal::quote(word))
                    .collect::<Vec<_>>()
                    .join(" ")),
            }
        }
        _ => anyhow::bail!("shell event has no command"),
    }
}

fn powershell(event: &Value) -> bool {
    let input = &event["tool_input"];
    let explicit = input["shell"]
        .as_str()
        .or_else(|| input["cmd"].as_array()?.first()?.as_str())
        .or_else(|| input["command"].as_array()?.first()?.as_str());
    event["tool_name"] == "PowerShell"
        || explicit.is_some_and(|shell| matches!(program(shell).as_str(), "pwsh" | "powershell"))
        || cfg!(windows) && event["tool_name"] == "exec_command" && explicit.is_none()
}

const POWERSHELL_PARSE: &str = "$tokens = $null; $errors = $null; $ast = [System.Management.Automation.Language.Parser]::ParseInput([Console]::In.ReadToEnd(), [ref]$tokens, [ref]$errors); $writes = @($ast.FindAll({param($node) $node -is [System.Management.Automation.Language.FileRedirectionAst] -and -not ($node.Location -is [System.Management.Automation.Language.VariableExpressionAst] -and $node.Location.VariablePath.UserPath -ieq 'null')}, $true)).Count -gt 0; @{errors = @($errors); file_write = $writes} | ConvertTo-Json -Depth 4 -Compress";

/// The independent policy refusals for one event.
/// Client and subagent events share the same command policy.
fn policy_refusals(event: &str) -> Result<Vec<Blocked>> {
    let event: Value = serde_json::from_str(event).context("PreToolUse event is not JSON")?;
    let tool = event["tool_name"].as_str().unwrap_or_default();
    if !matches!(
        tool,
        "Bash" | "PowerShell" | "bash" | "exec_command" | "shell"
    ) {
        return Ok(Vec::new());
    }
    let command = shell_command(&event)?;
    let command = command.as_str();
    let session = event["session_id"]
        .as_str()
        .filter(|value| !value.is_empty())
        .unwrap_or(NO_SESSION)
        .to_owned();
    let blocked = |rule: &'static str, source, message: String| Blocked {
        rule,
        source,
        session: session.clone(),
        message,
    };
    let mut findings = Vec::new();
    if let Some(message) = disk_scan_refusal(command) {
        findings.push(blocked(
            "hook.disk-scan",
            skill_ops::incident_rules::Source::GateRefusal,
            message,
        ));
    }
    if let Some(message) = path_conversion_refusal(command, cfg!(windows) && !powershell(&event)) {
        findings.push(blocked(
            "hook.path-conversion",
            skill_ops::incident_rules::Source::GateRefusal,
            message,
        ));
    }
    if !powershell(&event)
        && let shell_syntax_rules::Verdict::Refuse(reason) = shell_syntax_rules::inspect(command)
    {
        findings.push(blocked(
            "hook.shell-syntax",
            skill_ops::incident_rules::Source::MalformedInput,
            format!(
                "Refused: this shell command is malformed: {}.\nNext action: {}",
                shell_syntax_rules::cause(reason),
                shell_syntax_rules::next_action(reason)
            ),
        ));
    }
    if let Verdict::Refuse(reason) = inspect(command)
        && (!powershell(&event) || reason != crate::shell_write_rules::Reason::FileRedirect)
    {
        findings.push(blocked(
            "hook.shell-write",
            skill_ops::incident_rules::Source::GateRefusal,
            format!(
                "Refused: this command writes a file through an inline interpreter, in-place editor, heredoc, or redirect.\nNext action: {}",
                next_action(reason)
            ),
        ));
    }
    if let Some(message) = attribution_refusal(command, event["cwd"].as_str().map(Path::new)) {
        findings.push(blocked(
            "hook.attribution",
            skill_ops::incident_rules::Source::GateRefusal,
            message,
        ));
    }
    let legacy = crate::legacy::findings(command, event["cwd"].as_str().map(Path::new))
        .unwrap_or_else(|error| crate::legacy::Findings {
            refusals: Vec::new(),
            unavailable: vec![format!("{error:#}")],
        });
    for message in legacy.refusals {
        findings.push(blocked(
            "hook.legacy",
            skill_ops::incident_rules::Source::GateRefusal,
            message,
        ));
    }
    for message in legacy.unavailable {
        findings.push(blocked(
            "hook.inspection",
            skill_ops::incident_rules::Source::MalformedInput,
            format!("Refused: command policy inspection could not finish: {message}.\nNext action: correct the script's path, permissions, encoding, or inspection limit before resubmitting."),
        ));
    }
    Ok(findings)
}

#[cfg(test)]
fn combined(findings: Vec<Blocked>) -> Option<Blocked> {
    let mut findings = findings.into_iter();
    let mut first = findings.next()?;
    for finding in findings {
        first.message.push_str("\n\n");
        first.message.push_str(&finding.message);
    }
    Some(first)
}

#[cfg(test)]
pub(crate) fn refusal(event: &str) -> Result<Option<Blocked>> {
    Ok(combined(policy_refusals(event)?))
}

fn validate_command(
    mut validator: std::process::Command,
    command: &str,
) -> Result<std::process::Output> {
    let mut child = validator
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let written = child
        .stdin
        .take()
        .context("syntax validator has no stdin")
        .and_then(|mut input| {
            input
                .write_all(command.as_bytes())
                .context("write the command to its syntax validator")
        });
    if let Err(error) = written {
        platform::cleanup::report(child.kill(), "stop the syntax validator");
        platform::cleanup::report(child.wait(), "wait for the syntax validator");
        return Err(error);
    }
    Ok(child.wait_with_output()?)
}

fn checked_events(event: &str) -> Result<Vec<Blocked>> {
    let mut findings = policy_refusals(event)?;
    let value: Value = serde_json::from_str(event)?;
    if !matches!(
        value["tool_name"].as_str(),
        Some("Bash" | "PowerShell" | "bash" | "shell" | "exec_command")
    ) {
        return Ok(findings);
    }
    let command = shell_command(&value)?;
    let validation = (|| -> Result<(bool, bool)> {
        let cache = powershell(&value)
            .then(platform::process::PowerShellCache::new)
            .transpose()?;
        let mut validator = if powershell(&value) {
            let mut validator = crate::tool::Tool::Pwsh.command();
            validator.args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                POWERSHELL_PARSE,
            ]);
            validator
        } else {
            let mut validator = crate::tool::Tool::Shellcheck.command();
            validator.args(["--shell=bash", "--severity=error", "--format=json", "-"]);
            validator
        };
        if let Some(cache) = &cache {
            cache.apply(&mut validator);
        }
        let output = validate_command(validator, &command)?;
        let result: Value = serde_json::from_slice(&output.stdout)?;
        let (findings, file_write) = if powershell(&value) {
            (
                result["errors"]
                    .as_array()
                    .context("PowerShell returned no errors array")?,
                result["file_write"]
                    .as_bool()
                    .context("PowerShell returned no write verdict")?,
            )
        } else {
            (
                result
                    .as_array()
                    .context("ShellCheck returned no findings array")?,
                false,
            )
        };
        anyhow::ensure!(
            output.status.success() || !findings.is_empty(),
            "ShellCheck did not produce a validation result"
        );
        Ok((!findings.is_empty(), file_write))
    })();
    let available = validation.is_ok();
    let (errors, file_write) = validation.unwrap_or((false, false));
    let verdict =
        shell_syntax_rules::checked(shell_syntax_rules::Verdict::Admit, available, errors);
    if available
        && let Verdict::Refuse(reason) = crate::shell_write_rules::verdict(false, false, file_write)
    {
        findings.push(Blocked {
            rule: "hook.shell-write",
            source: skill_ops::incident_rules::Source::GateRefusal,
            session: value["session_id"]
                .as_str()
                .unwrap_or(NO_SESSION)
                .to_owned(),
            message: format!(
                "Refused: this PowerShell command writes a file.\nNext action: {}",
                next_action(reason)
            ),
        });
    }
    if let shell_syntax_rules::Verdict::Refuse(reason) = verdict {
        findings.push(Blocked {
            rule: "hook.shell-syntax",
            source: skill_ops::incident_rules::Source::MalformedInput,
            session: value["session_id"]
                .as_str()
                .filter(|value| !value.is_empty())
                .unwrap_or(NO_SESSION)
                .to_owned(),
            message: if powershell(&value) {
                "Refused: the mandatory PowerShell parser did not validate the command.\nNext action: install the pinned powershell tool and correct its syntax errors.".to_owned()
            } else {
                format!(
                    "Refused: {}.\nNext action: {}",
                    shell_syntax_rules::cause(reason),
                    shell_syntax_rules::next_action(reason)
                )
            },
        });
    }
    Ok(findings)
}

#[cfg(all(test, windows))]
fn checked_event(event: &str) -> Result<Option<Blocked>> {
    Ok(combined(checked_events(event)?))
}

/// The skill-ops state directory the incident store lives under, which the test environment can move.
fn incident_state() -> Option<PathBuf> {
    if let Some(state) = std::env::var_os("SKILL_OPS_STATE") {
        return Some(PathBuf::from(state));
    }
    platform::home::home().map(|home| home.join(".local/state/skill-ops"))
}

/// Records the refusal as an incident, so no session ends without a prevention decision for its rule.
/// A recording failure reports itself and never admits the command the hook refused.
fn record(blocked: &Blocked) {
    if !platform::incidents::enabled() {
        return;
    }
    let Some(state) = incident_state() else {
        return;
    };
    let incident = skill_ops::Incident::new(
        blocked.rule,
        &blocked.session,
        blocked.source,
        vec![
            blocked
                .message
                .lines()
                .next()
                .unwrap_or_default()
                .to_owned(),
        ],
    )
    .and_then(|incident| {
        skill_ops::record_incident(&state, &incident, platform::incidents::expected())
            .map(|recorded| (incident, recorded))
    });
    match incident {
        Ok((_, recorded)) if recorded => {}
        Ok(_) => {}
        Err(error) => eprintln!("warning: could not record the refusal as an incident: {error:#}"),
    }
}

/// Reads one event from standard input and exits 2, the blocking status, with the refusal on standard error.
///
/// # Errors
///
/// Returns an error when it can't read standard input, the event doesn't parse as JSON, or a shell event has no command.
pub fn run() -> Result<()> {
    let mut event = String::new();
    let shell_context = std::env::var(crate::codex_hooks::SHELL_CONTEXT).ok();
    let findings = std::io::stdin()
        .read_to_string(&mut event)
        .context("read the tool event")
        .and_then(|_| {
            if cfg!(windows) && shell_context.as_deref() == Some(crate::codex_hooks::POWERSHELL) {
                let mut value: Value = serde_json::from_str(&event).context("PreToolUse event is not JSON")?;
                if value["tool_name"] == "Bash"
                    && value["tool_input"]["command"].is_string()
                    && value["tool_input"].get("shell").is_none()
                {
                    value["tool_input"]["shell"] = crate::codex_hooks::POWERSHELL.into();
                }
                checked_events(&value.to_string())
            } else {
                checked_events(&event)
            }
        })
        .unwrap_or_else(|error| {
        vec![Blocked {
            rule: "hook.inspection",
            source: skill_ops::incident_rules::Source::MalformedInput,
            session: NO_SESSION.to_owned(),
            message: format!("Refused: command inspection could not finish: {error:#}.\nNext action: provide a valid tool event before resubmitting."),
        }]
        });
    if !findings.is_empty() {
        for blocked in &findings {
            record(blocked);
            eprintln!("{}\n", blocked.message);
        }
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
            .message
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
            .unwrap()
            .message;
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
                .unwrap_or_else(|| panic!("{command}"))
                .message;
            assert!(
                message.contains("in-place editor") && message.contains("Edit tool"),
                "{command}: {message}"
            );
        }
        for command in ["sed -n 's/a/b/p' file.txt", "awk '{print $1}' data.tsv"] {
            assert_eq!(
                refusal(&bash(command)).unwrap().map(|blocked| blocked.rule),
                Some("hook.legacy"),
                "{command}"
            );
        }
    }

    #[test]
    fn the_incident_command_is_refused_with_its_corrected_form() {
        let message = refusal(&bash(r#"export CARGO_TARGET_DIR="D:\cargo\$k""#))
            .unwrap()
            .unwrap()
            .message;
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
                .unwrap_or_else(|| panic!("{command}"))
                .message;
            assert!(message.contains("malformed"), "{command}: {message}");
        }
        for command in [
            "export CARGO_TARGET_DIR=\"D:/cargo/$name\"",
            "cargo build 2>&1",
            "echo 'D:\\cargo'",
        ] {
            assert_eq!(
                refusal(&bash(command))
                    .unwrap()
                    .map(|blocked| blocked.message),
                None,
                "{command}"
            );
        }
    }

    #[test]
    fn project_commands_and_other_tools_are_admitted() {
        for command in ["just check", "pr-workflow start", "git status", "bat a.md"] {
            assert_eq!(
                refusal(&bash(command))
                    .unwrap()
                    .map(|blocked| blocked.message),
                None,
                "{command}"
            );
        }
        assert!(refused("cat a.md").contains("legacy command"));
        let write = json!({"tool_name": "Write", "tool_input": {"file_path": "a"}}).to_string();
        assert_eq!(
            refusal(&write).unwrap().map(|blocked| blocked.message),
            None
        );
    }

    #[test]
    fn compound_preflight_reports_independent_refusal_families_together() {
        let command = "du -sh .; ls -la; python -c 'open(\"out\", \"w\").write(\"x\")'";
        let message = refusal(&bash(command)).unwrap().unwrap().message;
        assert!(message.contains("walks a directory tree"), "{message}");
        assert!(message.contains("inline interpreter"), "{message}");
        assert!(message.contains("`ls` is a legacy command"), "{message}");
        assert!(message.contains("storage-scout"), "{message}");
        assert!(message.contains("Edit or Write tool"), "{message}");
    }

    #[test]
    fn a_policy_refusal_does_not_hide_external_syntax_findings() {
        let findings = checked_events(&bash("ls -la; if then")).unwrap();
        assert!(findings.iter().any(|finding| finding.rule == "hook.legacy"));
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "hook.shell-syntax")
        );
    }

    #[test]
    fn a_powershell_syntax_error_does_not_hide_an_observed_file_write() {
        let directory = tempfile::tempdir().unwrap();
        let event = json!({
            "tool_name": "PowerShell",
            "cwd": directory.path(),
            "tool_input": {"command": "Write-Output value > output.txt; if ("}
        })
        .to_string();
        let findings = checked_events(&event).unwrap();
        for rule in ["hook.shell-syntax", "hook.shell-write"] {
            assert!(
                findings.iter().any(|finding| finding.rule == rule),
                "missing {rule}"
            );
        }
        assert!(!directory.path().join("output.txt").exists());
    }

    #[test]
    fn unavailable_script_inspection_is_an_explicit_blocking_finding() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("broken.sh"), [0xff]).unwrap();
        let event = json!({"tool_name": "Bash", "cwd": directory.path(), "tool_input": {"command": "du -sh .; sh broken.sh; ls -la; wc -l Cargo.toml"}}).to_string();
        let findings = policy_refusals(&event).unwrap();
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "hook.disk-scan")
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "hook.inspection"
                    && finding.message.contains("could not finish"))
        );
        for name in ["ls", "wc"] {
            assert!(
                findings.iter().any(|finding| finding.rule == "hook.legacy"
                    && finding
                        .message
                        .contains(&format!("`{name}` is a legacy command"))),
                "{}",
                findings
                    .iter()
                    .map(|finding| finding.message.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
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
                .unwrap_or_else(|| panic!("{event}"))
                .message;
            assert!(message.contains("storage-scout scan"), "{message}");
        }
        for command in ["du --version", "find . -name '*.rs'", "ls -la"] {
            assert!(refused(command).contains("legacy command"), "{command}");
        }
        for event in [
            bash("dust --version"),
            bash("fd -g '*.rs'"),
            bash("eza -la"),
            powershell("Get-ChildItem C:\\Users"),
            powershell("git status 2>$null"),
            powershell("Get-ChildItem -Recurse -Filter *.md"),
        ] {
            assert_eq!(
                refusal(&event).unwrap().map(|blocked| blocked.message),
                None,
                "{event}"
            );
        }
    }

    #[test]
    fn a_malformed_event_is_an_error_rather_than_an_admission() {
        assert!(refusal("not json").is_err());
        assert!(refusal(r#"{"tool_name":"Bash","tool_input":{}}"#).is_err());
    }

    #[test]
    #[cfg(windows)]
    fn powershell_null_sinks_and_file_redirects_use_the_native_ast() {
        for (command, blocked) in [
            ("git status 2>$null", false),
            ("git status 2>&1", false),
            ("Write-Output hi > result.txt", true),
            ("Write-Output hi > '$null'", true),
            ("git status 2>$null; Write-Output hi > result.txt", true),
        ] {
            let event =
                json!({"tool_name": "PowerShell", "tool_input": {"command": command}}).to_string();
            assert_eq!(
                checked_event(&event).unwrap().is_some(),
                blocked,
                "{command}"
            );
        }
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
                "bat --plain <<'EOF' | gh issue comment 3 -F -\n{}\nEOF",
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
                "bat --plain <<'EOF' | gh issue comment 3 -F -\n{}\nEOF",
            ] {
                let command = template.replace("{}", text);
                assert_eq!(
                    refusal(&bash(&command))
                        .unwrap()
                        .map(|blocked| blocked.message),
                    None,
                    "{command}"
                );
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
                let blocked = refusal(&event(&command)).unwrap();
                assert!(
                    blocked.is_some_and(|blocked| blocked.message.contains("AI attribution")),
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
                assert_eq!(
                    refusal(&event(command))
                        .unwrap()
                        .map(|blocked| blocked.message),
                    None,
                    "{command}: {text}"
                );
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

use crate::disk_scan_rules;
use crate::path_conversion_rules;
use crate::shell_syntax_rules;
use crate::shell_write_rules::{Verdict, inspect, next_action, program, segments};
use crate::tailscale_rules;
use anyhow::{Context, Result};
use serde_json::Value;
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

pub mod github;

/// A Git Bash drive path's parts: the empty root, the drive letter, and the rest.
const DRIVE_PATH_PARTS: usize = 3;

/// The shortest unambiguous abbreviation of a PowerShell switch: its dash and one letter.
const MIN_SWITCH_ABBREVIATION: usize = 2;

/// Claude Code blocks the tool call and shows standard error to the model on this exit status.
const BLOCK: i32 = 2;

/// The rule of a command whose inspection didn't finish.
const INSPECTION: &str = "hook.inspection";

/// The rule of a legacy command the modern tool table maps.
const LEGACY: &str = "hook.legacy";

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

fn tailscale_refusals(command: &str, depth: usize) -> Vec<String> {
    let mut messages = Vec::new();
    for words in segments(command) {
        let (words, _) = crate::legacy::command_words(&words);
        let Some((first, arguments)) = words.split_first() else {
            continue;
        };
        if program(first) != "tailscale" {
            continue;
        }
        let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
        if tailscale_rules::verdict(&arguments) == tailscale_rules::Verdict::Refuse {
            let subcommand = match arguments.as_slice() {
                [group @ ("dns" | "debug" | "exit-node" | "lock"), child, ..] => {
                    format!("{group} {child}")
                }
                [subcommand, ..] => (*subcommand).to_owned(),
                [] => "(bare invocation)".to_owned(),
            };
            messages.push(format!(
                "Refused: tailscale subcommand `{subcommand}` is not read-only.\nNext action: {}",
                tailscale_rules::NEXT_ACTION
            ));
        }
    }
    if depth < crate::legacy::DEPTH {
        for script in crate::legacy::nested_scripts(command) {
            messages.extend(tailscale_refusals(&script, depth + 1));
        }
    }
    messages
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

pub(crate) fn shell_command(event: &Value) -> Result<String> {
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

pub(crate) fn execution_directory(event: &Value) -> Result<Option<PathBuf>> {
    let base = event
        .get("cwd")
        .filter(|value| !value.is_null())
        .map(|value| {
            value
                .as_str()
                .map(native_path)
                .context("the session directory is not a string")
        })
        .transpose()?;
    let requested = event["tool_input"]
        .get("workdir")
        .or_else(|| event["tool_input"].get("cwd"))
        .filter(|value| !value.is_null())
        .map(|value| {
            value
                .as_str()
                .map(native_path)
                .context("the execution directory is not a string")
        })
        .transpose()?;
    match requested {
        Some(directory) if directory.is_absolute() => Ok(Some(directory)),
        Some(directory) => Ok(base
            .filter(|base| base.is_absolute())
            .map(|base| base.join(directory))
            .filter(|path| path.is_absolute())),
        None => Ok(base.filter(|base| base.is_absolute())),
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

/// The redirect and `tee` targets inside the session's checkout that its ignore rules exclude, so no commit carries what they write.
/// A target with an expansion, a glob, or a parent step never counts, because the hook can't tell where it lands.
fn ignored_targets(command: &str, directory: Option<&Path>) -> Vec<String> {
    let Some(directory) = directory.and_then(|directory| crate::canonical(directory).ok()) else {
        return Vec::new();
    };
    let Some(root) = crate::line_endings::work_tree(&directory)
        .ok()
        .flatten()
        .and_then(|root| crate::canonical(&root).ok())
    else {
        return Vec::new();
    };
    crate::shell_write_rules::write_targets(command)
        .iter()
        .map(crate::shell_write_rules::Target::as_str)
        .filter(|target| {
            !target.is_empty()
                && !target.contains(['$', '~', '*', '?', '[', '`'])
                && Path::new(target)
                    .components()
                    .all(|part| !matches!(part, std::path::Component::ParentDir))
                && directory
                    .join(target)
                    .strip_prefix(&root)
                    .is_ok_and(|relative| ignored(&root, relative))
        })
        .map(str::to_owned)
        .collect()
}

/// Whether the checkout at `root` ignores `relative`.
fn ignored(root: &Path, relative: &Path) -> bool {
    !relative.as_os_str().is_empty()
        && crate::tool::Tool::Git
            .command()
            .arg("-C")
            .arg(root)
            .args(["check-ignore", "-q", "--"])
            .arg(relative)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
}

/// The command that replaces an event's legacy shell command when the preferred tool prints the same bytes and the replacement passes every rule.
pub(crate) fn exact_rewrite(event: &Value) -> Option<String> {
    if event["tool_name"] != "Bash"
        || !event["tool_input"]["command"].is_string()
        || powershell(event)
    {
        return None;
    }
    let replaced = crate::legacy::replacement(&shell_command(event).ok()?)?;
    let mut rewritten = event.clone();
    rewritten["tool_input"]["command"] = Value::String(replaced.clone());
    checked_events(&rewritten.to_string())
        .ok()?
        .is_empty()
        .then_some(replaced)
}

/// Whether the hook refuses the event's command for a policy reason it can decide without the syntax validators.
pub(crate) fn refused(event: &Value) -> bool {
    policy_refusals(&event.to_string()).is_ok_and(|findings| !findings.is_empty())
        && exact_rewrite(event).is_none()
}

/// The client output that runs `replaced` in place of the event's command and names it for the agent.
fn rewrite_output(event: &Value, replaced: &str) -> Value {
    let mut input = event["tool_input"].clone();
    let original = input["command"].as_str().unwrap_or_default().to_owned();
    input["command"] = Value::String(replaced.to_owned());
    serde_json::json!({"hookSpecificOutput": {
        "hookEventName": "PreToolUse",
        "updatedInput": input,
        "additionalContext": format!("dotfiles ran `{replaced}` in place of the legacy `{original}`; the preferred tool prints the same bytes.")
    }})
}

/// The independent policy refusals for one event.
/// Client and subagent events share the same command policy.
fn policy_refusals(event: &str) -> Result<Vec<Blocked>> {
    policy_refusals_with(event, prose::Bundle::installed)
}

fn policy_refusals_with(
    event: &str,
    bundle: impl FnOnce() -> Result<prose::Bundle>,
) -> Result<Vec<Blocked>> {
    use skill_ops::incident_rules::Source::GateRefusal;

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
    let directory = execution_directory(&event)?;
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
    for message in tailscale_refusals(command, 0) {
        findings.push(blocked("hook.tailscale", GateRefusal, message));
    }
    if let Some(message) = disk_scan_refusal(command) {
        findings.push(blocked("hook.disk-scan", GateRefusal, message));
    }
    if let Some(message) = path_conversion_refusal(command, cfg!(windows) && !powershell(&event)) {
        findings.push(blocked("hook.path-conversion", GateRefusal, message));
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
    let written = match inspect(command) {
        Verdict::Refuse(crate::shell_write_rules::Reason::FileRedirect) => {
            let ignored = ignored_targets(command, directory.as_deref());
            crate::shell_write_rules::inspect_except(
                command,
                &ignored.iter().map(String::as_str).collect::<Vec<_>>(),
            )
        }
        verdict => verdict,
    };
    if let Verdict::Refuse(reason) = written
        && (!powershell(&event) || reason != crate::shell_write_rules::Reason::FileRedirect)
    {
        findings.push(blocked(
            "hook.shell-write",
            GateRefusal,
            format!(
                "Refused: this command writes a file through an inline interpreter, in-place editor, heredoc, or redirect.\nNext action: {}",
                next_action(reason)
            ),
        ));
    }
    let shell = if powershell(&event) {
        crate::legacy::Shell::PowerShell
    } else {
        crate::legacy::Shell::Bash
    };
    let legacy = crate::legacy::findings_in_shell(command, directory.as_deref(), shell)
        .unwrap_or_else(|error| crate::legacy::Findings {
            refusals: Vec::new(),
            unavailable: vec![format!("{error:#}")],
            ..crate::legacy::Findings::default()
        });
    for message in github::refusals(&legacy.bodies, bundle) {
        findings.push(blocked("hook.prose", GateRefusal, message));
    }
    for message in legacy.refusals {
        findings.push(blocked(LEGACY, GateRefusal, message));
    }
    for message in legacy.wrapper_refusals {
        findings.push(blocked("hook.wrapper", GateRefusal, message));
    }
    for message in crate::lint_hook::scripts(&legacy.scripts) {
        findings.push(blocked("hook.lint", GateRefusal, message));
    }
    for message in legacy.unavailable {
        findings.push(blocked(
            INSPECTION,
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
    Ok(combined(policy_refusals_with(event, test_bundle)?))
}

#[cfg(test)]
fn test_bundle() -> Result<prose::Bundle> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .context("dotfiles source root is unavailable")?;
    prose::Bundle::source(root)
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
/// With `rewrite`, a client that applies a hook's input rewrite runs an exact replacement instead of a refused legacy command.
///
/// # Errors
///
/// Returns an error when it can't read standard input, the event doesn't parse as JSON, or a shell event has no command.
pub fn run(rewrite: bool) -> Result<()> {
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
            rule: INSPECTION,
            source: skill_ops::incident_rules::Source::MalformedInput,
            session: NO_SESSION.to_owned(),
            message: format!("Refused: command inspection could not finish: {error:#}.\nNext action: provide a valid tool event before resubmitting."),
        }]
        });
    if !findings.is_empty() {
        if rewrite
            && findings.iter().all(|blocked| blocked.rule == LEGACY)
            && let Ok(value) = serde_json::from_str::<Value>(&event)
            && let Some(replaced) = exact_rewrite(&value)
        {
            println!("{}", rewrite_output(&value, &replaced));
            return Ok(());
        }
        for blocked in &findings {
            record(blocked);
            eprintln!("{}\n", blocked.message);
        }
        if let Some(blocked) = findings.iter().find(|blocked| blocked.rule == INSPECTION) {
            eprintln!("{}", platform::refusal::DEFECT);
            crate::hook_failures::record("claude-bash", &blocked.message);
        }
        eprintln!("{}", platform::refusal::IMPROVE);
        std::process::exit(BLOCK);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dotguard::provenance_rules;
    use serde_json::json;

    fn policy_refusals(event: &str) -> Result<Vec<Blocked>> {
        super::policy_refusals_with(event, super::test_bundle)
    }

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
    fn github_close_comments_refuse_two_sentences_on_one_line() -> Result<()> {
        for command in [
            "gh issue close 3 --comment 'The check passed. The issue is complete.'",
            "gh issue close 3 -c 'The check passed. The issue is complete.'",
            "gh pr close 3 --comment='The check passed. The issue is complete.'",
        ] {
            let findings = policy_refusals(&bash(command))?;
            assert!(
                findings.iter().any(|finding| finding.rule == "hook.prose"),
                "admitted prose without a sentence break: {command}"
            );
        }
        Ok(())
    }

    #[test]
    fn github_close_comments_admit_one_sentence_per_line() -> Result<()> {
        for command in [
            "gh issue close 3 --comment 'The check passed.\nThe issue is complete.'",
            "gh issue close 3 -c 'The check passed.\nThe issue is complete.'",
            "gh pr close 3 --comment='The check passed.\nThe issue is complete.'",
        ] {
            let findings = policy_refusals(&bash(command))?;
            assert!(
                findings.is_empty(),
                "{command}: {}",
                findings
                    .iter()
                    .map(|finding| finding.message.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
        Ok(())
    }

    #[test]
    fn github_body_inputs_refuse_unreadable_files_and_unresolved_values() -> Result<()> {
        let directory = tempfile::tempdir()?;
        for command in [
            "gh issue comment 3 --body-file missing.md",
            "gh api repos/a/b/issues/3 --input missing.json",
            "gh issue comment 3 --body \"$BODY\"",
            "gh issue comment 3 --body-file -",
        ] {
            let event = json!({
                "tool_name": "Bash", "cwd": directory.path(),
                "tool_input": {"command": command}
            })
            .to_string();
            let findings = policy_refusals(&event)?;
            assert!(
                findings.iter().any(|finding| finding.rule == "hook.prose"
                    && finding.message.contains("pr-workflow")),
                "admitted an uninspected body: {command}"
            );
        }
        Ok(())
    }

    #[test]
    fn github_prose_checks_every_nested_json_body() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("review.json");
        let payload = json!({
            "body": "The review is complete.",
            "comments": [{
                "path": "src/lib.rs", "line": 1,
                "body": "The check passed. The issue is complete."
            }]
        });
        std::fs::write(&path, serde_json::to_vec(&payload)?)?;
        let event = json!({
            "tool_name": "Bash", "cwd": directory.path(),
            "tool_input": {"command": "gh api repos/a/b/pulls/3/reviews --input review.json"}
        })
        .to_string();
        let findings = policy_refusals(&event)?;
        assert!(
            findings.iter().any(|finding| finding.rule == "hook.prose"),
            "admitted a nested body without a sentence break"
        );
        Ok(())
    }

    #[test]
    fn github_unknown_aliases_have_no_checked_body_contract() -> Result<()> {
        let findings = policy_refusals(&bash("gh publish-saved-body"))?;
        assert!(
            findings.iter().any(|finding| finding.rule == "hook.prose"),
            "admitted an alias with an uninspected body"
        );
        Ok(())
    }

    #[test]
    fn github_file_bodies_follow_the_tool_workdir() -> Result<()> {
        let session = tempfile::tempdir()?;
        let execution = tempfile::tempdir()?;
        std::fs::write(session.path().join("body.md"), "The check passed.\n")?;
        std::fs::write(
            execution.path().join("body.md"),
            "The check passed. The issue is complete.\n",
        )?;
        let event = json!({
            "tool_name": "exec_command", "cwd": session.path(),
            "tool_input": {
                "cmd": "gh issue comment 3 --body-file body.md",
                "workdir": execution.path()
            }
        })
        .to_string();
        let findings = policy_refusals(&event)?;
        assert!(
            findings.iter().any(|finding| finding.rule == "hook.prose"),
            "checked the session directory instead of the execution directory"
        );
        Ok(())
    }

    #[test]
    fn directory_flow_native_wrapper_quotes_never_read_the_event_directory() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let execution = directory.path().join("project's files");
        std::fs::create_dir(&execution)?;
        std::fs::write(directory.path().join("check.sh"), "cat wrong\n")?;
        std::fs::write(execution.join("check.sh"), "grep selected input\n")?;
        let target = execution
            .to_string_lossy()
            .replace('\\', "/")
            .replace('\'', "''");
        let event = json!({
            "tool_name":"Bash", "cwd":directory.path(),
            "tool_input":{"command":["pwsh","-NoProfile","-Command",format!("Set-Location -LiteralPath '{target}'; bash check.sh")]}
        });
        let findings = policy_refusals_with(&event.to_string(), test_bundle)?;
        assert!(
            findings.iter().any(|finding| finding.rule == INSPECTION
                && finding.message.contains("execution directory"))
        );
        assert!(findings.iter().any(|finding| finding.rule == "hook.lint"
            && finding.message.contains("execution directory")));
        assert!(
            !findings
                .iter()
                .any(|finding| finding.message.contains("`cat` is a legacy command"))
        );
        Ok(())
    }

    #[test]
    fn unsupported_wrappers_name_the_command_as_a_policy_refusal() -> Result<()> {
        let directory = tempfile::tempdir()?;
        std::fs::write(directory.path().join("check.sh"), "cat wrong\n")?;
        for command in [
            "env BASH_ENV=reset.sh bash check.sh",
            "bash --login check.sh",
            "mise x -C /unresolved -- bash check.sh",
        ] {
            let event = json!({
                "tool_name":"Bash", "cwd":directory.path(),
                "tool_input":{"command":command}
            });
            let findings = policy_refusals_with(&event.to_string(), test_bundle)?;
            let messages: Vec<_> = findings.iter().map(|finding| &finding.message).collect();
            assert!(
                findings.iter().any(|finding| finding.rule == "hook.wrapper"
                    && finding.source == skill_ops::incident_rules::Source::GateRefusal
                    && finding.message.contains(command)),
                "{command}: {messages:?}"
            );
            assert!(
                !findings
                    .iter()
                    .any(|finding| finding.rule == INSPECTION || finding.message.contains("`cat`")),
                "{command}: {messages:?}"
            );
        }
        let event = json!({
            "tool_name":"Bash", "cwd":directory.path(),
            "tool_input":{"command":"mise x -- cargo check"}
        });
        assert!(policy_refusals_with(&event.to_string(), test_bundle)?.is_empty());
        Ok(())
    }

    #[test]
    fn regex_end_anchors_pass_shell_policy_and_open_parameters_still_refuse() -> Result<()> {
        for command in [r#"rg -v "^\s*$""#, r#"rg "foo$" file"#] {
            assert!(
                policy_refusals_with(&bash(command), test_bundle)?.is_empty(),
                "{command}"
            );
        }
        let findings = policy_refusals_with(&bash(r#"rg "${name" file"#), test_bundle)?;
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "hook.shell-syntax")
        );
        Ok(())
    }

    #[test]
    fn powershell_literal_scripts_use_the_tool_workdir_and_keep_body_checks() -> Result<()> {
        let session = tempfile::tempdir()?;
        let execution = tempfile::tempdir()?;
        std::fs::write(
            execution.path().join("check.sh"),
            "#!/bin/sh\nprintf 'ready\\n'\n",
        )?;
        std::fs::write(
            execution.path().join("publish.sh"),
            "gh issue close 3 --comment 'The check passed. The issue is complete.'\n",
        )?;
        let event = |script: &str| {
            json!({
            "tool_name":"exec_command","cwd":session.path(),
            "tool_input":{"cmd":format!("sh {script}"),"shell":"pwsh","workdir":execution.path()}
        }).to_string()
        };
        let valid = policy_refusals_with(&event("check.sh"), test_bundle)?;
        assert!(
            valid.is_empty(),
            "{}",
            valid
                .iter()
                .map(|finding| finding.message.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        );
        let invalid = policy_refusals_with(&event("publish.sh"), test_bundle)?;
        assert!(invalid.iter().any(|finding| finding.rule == "hook.prose"));
        Ok(())
    }

    #[test]
    fn relative_scripts_without_an_absolute_event_directory_are_refused() -> Result<()> {
        let events = [
            json!({
                "tool_name":"Bash",
                "tool_input":{"command":"bash test/vulnerability_scan.sh"}
            }),
            json!({
                "tool_name":"Bash", "cwd":".",
                "tool_input":{"command":"bash test/vulnerability_scan.sh"}
            }),
            json!({
                "tool_name":"Bash",
                "tool_input":{"command":"bash test/vulnerability_scan.sh","workdir":"relative"}
            }),
        ];
        for event in events {
            let findings = policy_refusals_with(&event.to_string(), test_bundle)?;
            assert!(
                findings.iter().any(|finding| {
                    finding.rule == "hook.lint"
                        && finding
                            .message
                            .contains("known absolute execution directory")
                }),
                "{event}"
            );
        }
        Ok(())
    }

    #[test]
    fn relative_workdir_resolves_from_the_absolute_event_cwd() -> Result<()> {
        let session = tempfile::tempdir()?;
        let execution = session.path().join("execution");
        std::fs::create_dir(&execution)?;
        std::fs::write(execution.join("check.sh"), "#!/bin/sh\nprintf 'ready\\n'\n")?;
        let event = json!({
            "tool_name":"exec_command", "cwd":session.path(),
            "tool_input":{"cmd":"sh check.sh","shell":"pwsh","workdir":"execution"}
        });
        let findings = policy_refusals_with(&event.to_string(), test_bundle)?;
        let diagnostic = findings
            .iter()
            .map(|finding| format!("{}: {}", finding.rule, finding.message))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(findings.is_empty(), "{diagnostic}");
        Ok(())
    }

    #[test]
    fn github_opaque_inputs_never_claim_to_have_checked_body_content() -> Result<()> {
        for command in [
            "gh pr create --body \"$(printf 'The checks passed.')\"",
            "printf 'The checks passed.' | gh issue comment 3 -F -",
            "gh pr create --body-file missing.md",
            "bash <<EOF\n$PUBLISH\nEOF",
        ] {
            let findings = policy_refusals(&bash(command))?;
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.message.contains("unavailable")
                        || finding.message.contains("unresolved")
                        || finding.message.contains("could not be inspected")
                        || finding.message.contains("no known execution directory")),
                "{command}"
            );
            assert!(
                !findings
                    .iter()
                    .any(|finding| finding.message.contains("AI attribution")),
                "{command}"
            );
        }
        Ok(())
    }

    #[test]
    fn github_prose_inspection_survives_legacy_script_exceptions() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let excepted = "home/.chezmoitemplates/partials/install-mise-sh";
        std::fs::create_dir_all(directory.path().join("home/.chezmoitemplates/partials"))?;
        for path in ["publish.sh", excepted] {
            std::fs::write(
                directory.path().join(path),
                "gh issue close 3 --comment 'The check passed. The issue is complete.'\n",
            )?;
        }
        for command in [
            "./publish.sh".to_owned(),
            "bash publish.sh".to_owned(),
            "DOTFILES_LEGACY_BYPASS='publish.sh: upstream script' bash publish.sh".to_owned(),
            format!("sh {excepted}"),
        ] {
            let event = json!({"tool_name":"Bash", "cwd":directory.path(), "tool_input":{"command":command}}).to_string();
            assert!(
                policy_refusals(&event)?
                    .iter()
                    .any(|finding| finding.rule == "hook.prose"
                        && finding.message.contains("SentencePerLine")),
                "{event}"
            );
        }
        Ok(())
    }

    #[test]
    fn github_file_reads_require_a_stable_execution_context() -> Result<()> {
        let directory = tempfile::tempdir()?;
        std::fs::write(directory.path().join("body.md"), "The check passed.\n")?;
        for command in [
            "cd elsewhere && gh issue comment 3 -F body.md",
            "gh issue comment 3 -F body.md > body.md",
        ] {
            let event = json!({"tool_name":"Bash", "cwd":directory.path(), "tool_input":{"command":command}}).to_string();
            assert!(
                policy_refusals(&event)?
                    .iter()
                    .any(|finding| finding.rule == "hook.prose"
                        && finding.message.contains("uncertain execution context")),
                "{event}"
            );
        }
        Ok(())
    }

    #[test]
    fn an_exact_legacy_command_runs_its_replacement_and_tells_the_agent() {
        let event: Value = serde_json::from_str(&bash("cat Cargo.toml")).unwrap();
        let replaced = exact_rewrite(&event).unwrap();
        assert_eq!(
            replaced,
            "bat --plain --paging=never --binary=as-text Cargo.toml"
        );
        let output = rewrite_output(&event, &replaced);
        assert_eq!(
            output["hookSpecificOutput"]["updatedInput"]["command"],
            replaced.as_str()
        );
        assert!(
            output["hookSpecificOutput"]["additionalContext"]
                .as_str()
                .unwrap()
                .contains("`cat Cargo.toml`")
        );
        assert!(
            output["hookSpecificOutput"]
                .get("permissionDecision")
                .is_none()
        );
        assert!(!super::refused(&event));
        for command in [
            "cat Cargo.toml > copy.toml",
            "cat Cargo.toml | grep x",
            "cat -n Cargo.toml",
        ] {
            let event: Value = serde_json::from_str(&bash(command)).unwrap();
            assert_eq!(exact_rewrite(&event), None, "{command}");
            assert!(super::refused(&event), "{command}");
        }
    }

    #[test]
    fn a_redirect_into_an_ignored_path_of_the_checkout_passes() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let initialized = platform::process::command("git")
            .args(["init", "--quiet"])
            .current_dir(root)
            .output()
            .unwrap();
        assert!(initialized.status.success(), "{initialized:?}");
        std::fs::write(root.join(".gitignore"), "target/\n").unwrap();
        let event = |command: &str| {
            json!({"tool_name": "Bash", "cwd": root, "tool_input": {"command": command}})
                .to_string()
        };
        let writes = |command: &str| {
            policy_refusals(&event(command))
                .unwrap()
                .iter()
                .any(|finding| finding.rule == "hook.shell-write")
        };
        for command in [
            "printf x > target/out.txt",
            "cargo test 2> target/test.log | tee target/copy.log",
        ] {
            assert!(!writes(command), "{command}");
        }
        for command in [
            "printf x > out.txt",
            "printf x > target/../out.txt",
            "printf x > \"$HOME/target/out.txt\"",
            "printf x > target/out.txt; printf y > notes.txt",
        ] {
            assert!(writes(command), "{command}");
        }
    }

    #[test]
    fn no_legacy_entry_suggests_a_command_that_another_rule_refuses() {
        for tool in &crate::legacy::table().unwrap().tools {
            let commands: Vec<&str> = if tool.next.contains('`') {
                tool.next.split('`').skip(1).step_by(2).collect()
            } else if tool.next.contains("{args}") {
                vec![tool.next.as_str()]
            } else {
                Vec::new()
            };
            for command in commands {
                let command = command.replace("{args}", ".").replace(['<', '>'], "");
                assert!(
                    refusal(&bash(&command)).unwrap().is_none(),
                    "{:?} suggests `{command}`, which the hook refuses",
                    tool.legacy
                );
            }
        }
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
    fn read_only_tailscale_subcommands_are_admitted() {
        for arguments in [
            "status",
            "status --json",
            "ping linux",
            "netcheck",
            "ip -4",
            "whois 100.64.0.1",
            "version",
            "dns status",
            "exit-node list",
            "lock status",
            "metrics",
            "bugreport",
            "debug derp-map",
            "debug daemon-goroutines",
            "debug daemon-logs",
            "debug daemon-bus-events",
            "debug daemon-bus-graph",
            "debug daemon-bus-queues",
            "debug metrics",
            "debug env",
            "debug stat /dev/null",
            "debug hostinfo",
            "debug control-knobs",
            "debug prefs",
            "debug watch-ipn",
            "debug netmap",
            "debug via 1 192.0.2.0/24",
            "debug peer-endpoint-changes linux",
            "debug resolve linux",
            "debug go-buildinfo",
            "debug peer-relay-servers",
            "debug statedir",
        ] {
            let command = format!("tailscale {arguments}");
            assert_eq!(
                refusal(&bash(&command))
                    .unwrap()
                    .map(|blocked| blocked.message),
                None,
                "{command}"
            );
        }
    }

    #[test]
    fn mutating_tailscale_subcommands_are_refused_with_the_next_action() {
        for arguments in [
            "up",
            "down",
            "login",
            "logout",
            "set",
            "switch",
            "ssh",
            "serve",
            "funnel",
            "cert",
            "file",
            "drive",
            "web",
            "configure",
            "update",
            "syspolicy",
            "dns",
            "dns set",
            "exit-node",
            "exit-node suggest",
            "lock",
            "lock init",
            "lock add",
            "lock remove",
            "lock sign",
            "lock disable",
            "lock log",
            "debug",
            "debug rebind",
            "debug restun",
            "debug localapi",
            "debug local-creds",
            "debug component-logs",
            "debug set-expire",
            "debug reload-config",
            "debug --file=delete:received",
            "unknown",
            "--help",
            "",
        ] {
            let command = format!("tailscale {arguments}");
            let findings = policy_refusals(&bash(&command)).unwrap();
            let blocked = findings
                .iter()
                .find(|finding| finding.rule == "hook.tailscale")
                .unwrap_or_else(|| panic!("admitted: {command}"));
            let evidence = if arguments.is_empty() {
                "(bare invocation)"
            } else {
                arguments
            };
            assert!(blocked.message.contains(evidence), "{}", blocked.message);
            assert!(blocked.message.contains("Next action: ask the owner to run it; a tailscale command can open a sign-in window on the owner's desktop"), "{}", blocked.message);
        }
    }

    #[test]
    fn tailscale_mutations_through_wrappers_and_paths_are_refused() {
        for command in [
            "sudo tailscale up",
            "env tailscale up",
            "env TS_DEBUG=1 tailscale up",
            "TS_DEBUG=1 tailscale up",
            "mise x -- tailscale up",
            "mise exec -- tailscale up",
            "sudo env TS_DEBUG=1 mise x -- tailscale up",
            "command tailscale up",
            "exec tailscale up",
            "nohup tailscale up",
            "nice tailscale up",
            "tailscale status; tailscale up",
            "tailscale status && tailscale up",
            "tailscale status || tailscale up",
            "tailscale up | bat",
            "(tailscale up)",
            "sh -c 'tailscale up'",
            "bash -lc 'env tailscale up'",
            "env sh -c 'mise x -- tailscale up'",
            "sh -c \"sh -c 'tailscale up'\"",
            "/Applications/Tailscale.app/Contents/MacOS/Tailscale up",
            "/usr/local/bin/tailscale up",
            "./tailscale up",
            "tailscale.exe up",
            r"'C:\Program Files\Tailscale\tailscale.exe' up",
            "sudo /Applications/Tailscale.app/Contents/MacOS/Tailscale up",
        ] {
            let message = refused(command);
            assert!(message.contains("`up`"), "{command}: {message}");
            assert!(
                message.contains("ask the owner to run it"),
                "{command}: {message}"
            );
        }
    }

    #[test]
    fn tailscale_guard_preserves_read_only_wrappers_and_openssh() {
        let directory = tempfile::tempdir().unwrap();
        for command in [
            "ssh linux",
            "sudo ssh linux",
            "mise x -- tailscale status",
            "env sh -c 'tailscale status'",
            "/Applications/Tailscale.app/Contents/MacOS/Tailscale status",
            "tailscale.exe status",
            "echo 'tailscale up'",
            "rg tailscale Cargo.toml",
        ] {
            let event = json!({
                "tool_name": "Bash",
                "cwd": directory.path(),
                "tool_input": {"command": command}
            });
            assert_eq!(
                refusal(&event.to_string())
                    .unwrap()
                    .map(|blocked| blocked.message),
                None,
                "{command}"
            );
        }
    }

    #[test]
    fn tailscale_guard_checks_every_agent_shell_client() {
        for tool in ["Bash", "PowerShell", "bash", "exec_command", "shell"] {
            let event = json!({"tool_name": tool, "tool_input": {"command": "tailscale up"}});
            assert!(super::refused(&event), "{tool}");
        }
        let event =
            json!({"tool_name": "shell", "tool_input": {"command": ["sh", "-c", "tailscale up"]}});
        assert!(super::refused(&event));
    }

    #[test]
    fn compound_preflight_reports_independent_refusal_families_together() {
        let command =
            "du -sh .; ls -la; python -c 'open(\"out\", \"w\").write(\"x\")'; tailscale up";
        let message = refusal(&bash(command)).unwrap().unwrap().message;
        assert!(message.contains("walks a directory tree"), "{message}");
        assert!(message.contains("inline interpreter"), "{message}");
        assert!(message.contains("`ls` is a legacy command"), "{message}");
        assert!(message.contains("storage-scout"), "{message}");
        assert!(message.contains("Edit or Write tool"), "{message}");
        assert!(message.contains("ask the owner to run it"), "{message}");
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
    fn powershell_source_does_not_enter_the_bash_argument_parser() -> Result<()> {
        let event = json!({
            "tool_name": "PowerShell",
            "tool_input": {"command": "$items = @(1, 2); $items | ForEach-Object { $_.ToString() }"}
        })
        .to_string();
        let findings = policy_refusals(&event)?;
        assert!(
            findings.is_empty(),
            "{:?}",
            findings
                .iter()
                .map(|finding| &finding.message)
                .collect::<Vec<_>>()
        );
        Ok(())
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
                "gh pr create --body-file - <<'EOF'\nBody.\n{}\nEOF",
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
    fn every_provenance_phrase_is_refused_and_a_quoted_one_admitted() {
        for phrase in provenance_rules::PHRASES {
            for template in [
                "gh pr create --title t --body \"Body {}.\"",
                "gh issue comment 3 -b \"Notes {} here.\"",
                "gh pr review 3 --approve --body \"Checked {}.\"",
                "gh api repos/a/b/issues/3/comments -f body=\"{}\"",
                "gh pr create --body-file - <<'EOF'\nBody.\n{}\nEOF",
            ] {
                let command = template.replace("{}", phrase);
                let message = refused(&command);
                assert!(
                    message.contains("says how its text came about")
                        && message.contains("Rewrite that sentence"),
                    "{command}: {message}"
                );
            }
        }
        let quoted = "gh pr comment 3 --body 'Search old notes for `ai-assisted`.'";
        assert_eq!(
            refusal(&bash(quoted))
                .unwrap()
                .map(|blocked| blocked.message),
            None
        );
    }

    #[test]
    fn the_authors_own_trailers_and_prose_about_claude_are_admitted_on_every_inline_path() {
        for text in ADMITTED {
            for template in [
                "gh pr create --title t --body \"Body.\n\n{}\"",
                "gh pr comment 3 --body \"{}\"",
                "gh api repos/a/b/issues/3/comments -f body=\"{}\"",
                "gh pr create --body-file - <<'EOF'\n{}\nEOF",
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

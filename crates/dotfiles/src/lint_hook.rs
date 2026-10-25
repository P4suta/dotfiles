use crate::lint_hook_rules::{self as rules, Admission, Configuration, FileType, Linter, Outcome};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const LINTER_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(20);
const MAX_LOCATIONS: usize = 3;
const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_OUTPUT_BYTES: u64 = 4 * 1024 * 1024;
const BASH4_MAJOR: u32 = 4;
const SC2016_HINT: &str = "For an intentional single-quoted jaq or rg program, place # shellcheck disable=SC2016 on the line immediately before that command.";
const PATCH_PREFIXES: &[&str] = &["*** Add File: ", "*** Update File: ", "*** Move to: "];
const BASH_VERSION: &str = "--version";
const FIRST_LINE_BYTES: u64 = 4096;
const SHARED_VERSIONS: &str = include_str!("../../../home/.chezmoidata/versions.toml");
const STRICT_BIOME: &str = r#"{"root":true,"linter":{"rules":{"recommended":true}}}"#;
const STRICT_HADOLINT: &str = "ignored: []\n";

#[derive(Debug, Default)]
pub struct Report {
    pub findings: Vec<String>,
}

#[derive(Debug)]
struct Completed {
    outcome: Outcome,
    stdout: String,
    stderr: String,
}

fn bounded(command: &mut Command, input: Option<&str>) -> Result<Completed> {
    bounded_for(command, input, LINTER_TIMEOUT)
}

fn bounded_for(command: &mut Command, input: Option<&str>, timeout: Duration) -> Result<Completed> {
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    let mut stdin = tempfile::tempfile()?;
    if let Some(input) = input {
        stdin.write_all(input.as_bytes())?;
    }
    stdin.seek(SeekFrom::Start(0))?;
    let child = command
        .stdin(stdin)
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?)
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Completed {
                outcome: Outcome::Missing,
                stdout: String::new(),
                stderr: error.to_string(),
            });
        }
        Err(error) => return Err(error.into()),
    };
    let began = Instant::now();
    let outcome = loop {
        if let Some(status) = child.try_wait()? {
            break if status.success() {
                Outcome::Clean
            } else {
                Outcome::Findings
            };
        }
        if began.elapsed() >= timeout {
            platform::cleanup::report(child.kill(), "stop a timed-out linter");
            platform::cleanup::report(child.wait(), "reap a timed-out linter");
            break Outcome::Timeout;
        }
        std::thread::sleep(POLL_INTERVAL);
    };
    stdout.seek(SeekFrom::Start(0))?;
    stderr.seek(SeekFrom::Start(0))?;
    let mut output = String::new();
    let mut diagnostics = String::new();
    stdout.take(MAX_OUTPUT_BYTES).read_to_string(&mut output)?;
    stderr
        .take(MAX_OUTPUT_BYTES)
        .read_to_string(&mut diagnostics)?;
    Ok(Completed {
        outcome,
        stdout: output,
        stderr: diagnostics,
    })
}

pub(crate) fn check_shell_syntax_source(input: &str) -> Result<()> {
    ensure!(
        u64::try_from(input.len())? <= MAX_FILE_BYTES,
        "shell syntax input exceeds the inspection limit"
    );
    ensure!(
        !input.contains('\0'),
        "shell syntax input contains a NUL byte"
    );
    Ok(())
}

pub(crate) fn check_shell_syntax_output(input: &str) -> Result<()> {
    ensure!(
        u64::try_from(input.len())? < MAX_OUTPUT_BYTES,
        "shell syntax output reached the inspection limit"
    );
    Ok(())
}

pub(crate) fn shell_syntax_tree(input: &str) -> Result<String> {
    check_shell_syntax_source(input)?;
    let completed = bounded(
        Tool::Shfmt
            .command()
            .args(["--to-json", "--language-dialect", "bash"]),
        Some(input),
    )?;
    ensure!(
        completed.outcome == Outcome::Clean,
        "the pinned shfmt parser could not finish: {:?}",
        completed.outcome
    );
    check_shell_syntax_output(&completed.stdout)?;
    Ok(completed.stdout)
}

pub(crate) fn powershell_syntax_tree(input: &str, projection: &str) -> Result<String> {
    check_shell_syntax_source(input)?;
    let completed = bounded(
        Tool::Pwsh.command().args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            projection,
        ]),
        Some(input),
    )?;
    ensure!(
        completed.outcome == Outcome::Clean,
        "the pinned PowerShell parser could not finish: {:?}: {}",
        completed.outcome,
        completed.stderr
    );
    check_shell_syntax_output(&completed.stdout)?;
    Ok(completed.stdout)
}

fn tool(linter: Linter) -> Tool {
    match linter {
        Linter::Shellcheck => Tool::Shellcheck,
        Linter::Shfmt => Tool::Shfmt,
        Linter::Zsh => Tool::Zsh,
        Linter::RuffCheck | Linter::RuffFormat => Tool::Ruff,
        Linter::Biome => Tool::Biome,
        Linter::Actionlint => Tool::Actionlint,
        Linter::Zizmor => Tool::Zizmor,
        Linter::TaploFormat | Linter::TaploCheck => Tool::Taplo,
        Linter::Hadolint => Tool::Hadolint,
        Linter::Just => Tool::Just,
    }
}

fn repository(path: &Path) -> Option<PathBuf> {
    path.parent()?
        .ancestors()
        .find(|directory| directory.join(".git").exists())
        .map(Path::to_path_buf)
}

fn config_names(linter: Linter) -> &'static [&'static str] {
    match linter {
        Linter::Shellcheck => &[".shellcheckrc", "shellcheckrc"],
        Linter::Shfmt => &[".editorconfig"],
        Linter::RuffCheck | Linter::RuffFormat => &["ruff.toml", ".ruff.toml", "pyproject.toml"],
        Linter::Biome => &["biome.json", "biome.jsonc"],
        Linter::Actionlint => &[".github/actionlint.yaml", ".github/actionlint.yml"],
        Linter::Zizmor => &["zizmor.yml", ".github/zizmor.yml"],
        Linter::TaploFormat | Linter::TaploCheck => &["taplo.toml", ".taplo.toml"],
        Linter::Hadolint => &[".hadolint.yaml", ".hadolint.yml", "hadolint.yaml"],
        Linter::Zsh | Linter::Just => &[],
    }
}

fn configuration(path: &Path, root: Option<&Path>, linter: Linter) -> Option<PathBuf> {
    let root = root?;
    for directory in path
        .parent()?
        .ancestors()
        .take_while(|directory| directory.starts_with(root))
    {
        for name in config_names(linter) {
            let candidate = directory.join(name);
            if !candidate.is_file() {
                continue;
            }
            if *name == "pyproject.toml" {
                let text = fs::read_to_string(&candidate).ok()?;
                let parsed: toml::Value = toml::from_str(&text).ok()?;
                if parsed
                    .get("tool")
                    .and_then(|tool| tool.get("ruff"))
                    .is_none()
                {
                    continue;
                }
            }
            return Some(candidate);
        }
    }
    None
}

fn arguments(linter: Linter, config: Configuration) -> Vec<&'static str> {
    let strict = config == Configuration::Strict;
    match linter {
        Linter::Shellcheck if strict => vec![
            "--format=json",
            "--norc",
            "--enable=all",
            "--severity=style",
            "--external-sources",
            "--check-sourced",
        ],
        Linter::Shellcheck => vec!["--format=json"],
        Linter::Shfmt => vec!["--diff", "--simplify", "--apply-ignore"],
        Linter::Zsh => vec!["-n"],
        Linter::RuffCheck if strict => vec![
            "check",
            "--isolated",
            "--select",
            "ALL",
            "--output-format",
            "json",
            "--force-exclude",
        ],
        Linter::RuffCheck => vec!["check", "--output-format", "json", "--force-exclude"],
        Linter::RuffFormat if strict => vec!["format", "--check", "--isolated", "--force-exclude"],
        Linter::RuffFormat => vec!["format", "--check", "--force-exclude"],
        Linter::Biome => vec![
            "check",
            "--error-on-warnings",
            "--no-errors-on-unmatched",
            "--reporter=json",
            "--max-diagnostics=none",
        ],
        Linter::Actionlint => vec!["-oneline"],
        Linter::Zizmor if strict => vec![
            "--offline",
            "--format",
            "sarif",
            "--persona",
            "pedantic",
            "--strict-collection",
            "--no-config",
        ],
        Linter::Zizmor => vec!["--offline", "--format", "sarif", "--strict-collection"],
        Linter::TaploFormat if strict => vec!["fmt", "--check", "--no-auto-config"],
        Linter::TaploFormat => vec!["fmt", "--check"],
        Linter::TaploCheck if strict => vec!["check", "--no-auto-config"],
        Linter::TaploCheck => vec!["check"],
        Linter::Hadolint if strict => vec!["--format", "json", "--failure-threshold", "style"],
        Linter::Hadolint => vec!["--format", "json"],
        Linter::Just => vec!["--fmt", "--check", "--unstable", "--justfile"],
    }
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn native_quote(value: &str) -> String {
    if cfg!(windows) {
        format!("'{}'", value.replace('\'', "''"))
    } else {
        quote(value)
    }
}

fn reproduce(command: &Command, input: Option<&str>) -> String {
    let mut words = Vec::new();
    let mut environment = Vec::new();
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            if cfg!(windows) {
                environment.push(format!(
                    "$env:{} = {}; ",
                    key.to_string_lossy(),
                    native_quote(&value.to_string_lossy())
                ));
            } else {
                words.push(quote(&format!(
                    "{}={}",
                    key.to_string_lossy(),
                    value.to_string_lossy()
                )));
            }
        }
    }
    if !words.is_empty() {
        words.insert(0, "env".to_owned());
    }
    if cfg!(windows) {
        words.push("&".to_owned());
    }
    words.push(native_quote(&command.get_program().to_string_lossy()));
    words.extend(
        command
            .get_args()
            .map(|argument| native_quote(&argument.to_string_lossy())),
    );
    let invocation = words.join(" ");
    let invocation = input.map_or_else(
        || invocation.clone(),
        |input| {
            if cfg!(windows) {
                format!("{} | {invocation}", native_quote(input))
            } else {
                format!("printf '%s' {} | {invocation}", quote(input))
            }
        },
    );
    let invocation = format!("{}{invocation}", environment.concat());
    match command.get_current_dir() {
        Some(directory) if cfg!(windows) => format!(
            "Set-Location -LiteralPath {}; {invocation}",
            native_quote(&directory.to_string_lossy())
        ),
        Some(directory) => format!("cd {} && {invocation}", quote(&directory.to_string_lossy())),
        None => invocation,
    }
}

fn reproduce_hook(event: &Value, hook: &str) -> Result<String> {
    let mut command = crate::tool::external(std::env::current_exe()?);
    command.arg(hook);
    Ok(reproduce(&command, Some(&event.to_string())))
}

#[derive(Debug, Deserialize)]
struct Location {
    row: u64,
    column: u64,
}

#[derive(Debug, Default)]
struct Group {
    count: usize,
    locations: Vec<String>,
}

fn grouped(linter: Linter, path: &Path, output: &str) -> Option<String> {
    let value: Value = if linter == Linter::Actionlint {
        Value::Null
    } else {
        serde_json::from_str(output).ok()?
    };
    let mut groups: BTreeMap<String, Group> = BTreeMap::new();
    let mut record = |rule: String, location: String| {
        let group = groups.entry(rule).or_default();
        group.count += 1;
        if group.locations.len() < MAX_LOCATIONS {
            group.locations.push(location);
        }
    };
    match linter {
        Linter::Actionlint => {
            for line in output.lines().filter(|line| !line.trim().is_empty()) {
                let rule = line
                    .rsplit_once(" [")
                    .and_then(|(_, suffix)| suffix.strip_suffix(']'))
                    .unwrap_or("actionlint");
                record(rule.to_owned(), line.to_owned());
            }
        }
        Linter::Biome => {
            for finding in value["diagnostics"].as_array()? {
                let location = &finding["location"]["start"];
                record(
                    finding["category"].as_str().unwrap_or("biome").to_owned(),
                    format!(
                        "{}:{}:{}: {}",
                        path.display(),
                        location["line"],
                        location["column"],
                        finding["message"].as_str().unwrap_or_default()
                    ),
                );
            }
        }
        Linter::Shellcheck | Linter::Hadolint | Linter::RuffCheck => {
            for finding in value.as_array()? {
                let code = match linter {
                    Linter::Shellcheck => format!("SC{}", finding["code"]),
                    _ => finding["code"].as_str().unwrap_or("diagnostic").to_owned(),
                };
                let (line, column) = if linter == Linter::RuffCheck {
                    let location: Location =
                        serde_json::from_value(finding["location"].clone()).ok()?;
                    (location.row, location.column)
                } else {
                    (
                        finding["line"].as_u64().unwrap_or_default(),
                        finding["column"].as_u64().unwrap_or_default(),
                    )
                };
                record(
                    code,
                    format!(
                        "{}:{line}:{column}: {}",
                        path.display(),
                        finding["message"].as_str().unwrap_or_default()
                    ),
                );
            }
        }
        Linter::Zizmor => {
            for run in value["runs"].as_array()? {
                for finding in run["results"].as_array()? {
                    if finding["suppressions"]
                        .as_array()
                        .is_some_and(|suppressed| !suppressed.is_empty())
                    {
                        continue;
                    }
                    let location = &finding["locations"][0]["physicalLocation"];
                    record(
                        finding["ruleId"].as_str().unwrap_or("zizmor").to_owned(),
                        format!(
                            "{}:{}:{}: {}",
                            path.display(),
                            location["region"]["startLine"],
                            location["region"]["startColumn"],
                            finding["message"]["text"].as_str().unwrap_or_default()
                        ),
                    );
                }
            }
        }
        _ => return None,
    }
    render_groups(groups)
}

fn render_groups(groups: BTreeMap<String, Group>) -> Option<String> {
    if groups.is_empty() {
        return None;
    }
    let mut messages = Vec::new();
    for (rule, group) in groups {
        messages.push(format!(
            "{rule}: {} finding(s)\n  {}",
            group.count,
            group.locations.join("\n  ")
        ));
        if rule == "SC2016" {
            messages.push(SC2016_HINT.to_owned());
        }
    }
    Some(messages.join("\n"))
}

fn installation(tool: Tool) -> Result<String> {
    let data = model::data::parse("versions.toml", SHARED_VERSIONS).map_err(anyhow::Error::msg)?;
    let versions = data
        .shared_tools
        .and_then(|tools| tools.versions)
        .context("shared tool versions are missing")?;
    if let Some(version) = versions.get(tool.program()) {
        return Ok(format!("mise install {}@{version}", tool.program()));
    }
    Ok(match tool.provision() {
        platform::tool::Provision::Pinned(key) => format!("mise install {key}"),
        _ => format!(
            "Install {} with your operating system's package manager",
            tool.program()
        ),
    })
}

fn apply_configuration(command: &mut Command, linter: Linter, config: &Path) {
    match linter {
        Linter::Shellcheck => {
            command.arg(format!("--rcfile={}", config.display()));
        }
        Linter::RuffCheck
        | Linter::RuffFormat
        | Linter::TaploCheck
        | Linter::TaploFormat
        | Linter::Hadolint
        | Linter::Zizmor => {
            command.arg("--config").arg(config);
        }
        Linter::Biome => {
            command.arg("--config-path").arg(config);
        }
        Linter::Actionlint => {
            command.arg("-config-file").arg(config);
        }
        Linter::Shfmt | Linter::Zsh | Linter::Just => {}
    }
}

fn lint(path: &Path, linter: Linter, root: Option<&Path>) -> Result<(Outcome, Option<String>)> {
    let mut config = configuration(path, root, linter);
    let source = rules::configuration(config.is_some());
    let defaults =
        if source == Configuration::Strict && matches!(linter, Linter::Biome | Linter::Hadolint) {
            let directory = tempfile::tempdir()?;
            let (name, contents) = if linter == Linter::Biome {
                ("biome.json", STRICT_BIOME)
            } else {
                ("hadolint.yaml", STRICT_HADOLINT)
            };
            let path = directory.path().join(name);
            fs::write(&path, contents)?;
            config = Some(path);
            Some(directory)
        } else {
            None
        };
    let mut command = tool(linter).command();
    command.current_dir(
        root.or_else(|| path.parent())
            .context("file has no parent")?,
    );
    command.args(arguments(linter, source));
    if linter == Linter::Shfmt && source == Configuration::Strict {
        command.env("SHFMT_NO_EDITORCONFIG", "1");
    }
    if let Some(config) = config {
        apply_configuration(&mut command, linter, &config);
    }
    let action = rules::classify(&path.to_string_lossy(), "") == FileType::Action
        && linter == Linter::Actionlint;
    let synthetic = if action {
        let relative = path
            .parent()
            .and_then(|parent| parent.strip_prefix(root.unwrap_or(parent)).ok())
            .map_or_else(String::new, |relative| {
                relative.to_string_lossy().replace('\\', "/")
            });
        let uses = serde_json::to_string(&format!("./{relative}"))?;
        Some(format!(
            "on: push\njobs:\n  metadata:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: {uses}\n"
        ))
    } else {
        None
    };
    if action {
        command.arg("-");
    } else {
        let argument = if linter == Linter::Just {
            path
        } else {
            command
                .get_current_dir()
                .and_then(|directory| path.strip_prefix(directory).ok())
                .unwrap_or(path)
        };
        if linter != Linter::Just {
            command.arg("--");
        }
        command.arg(argument);
    }
    let reproduce = if defaults.is_some() {
        let event = json!({"cwd":path.parent(), "tool_name":"Write", "tool_input":{"file_path":path}, "hook_event_name":"PostToolUse"});
        reproduce_hook(&event, "lint-hook")?
    } else {
        reproduce(&command, synthetic.as_deref())
    };
    let mut completed = bounded(&mut command, synthetic.as_deref())?;
    let grouped = grouped(linter, path, &completed.stdout);
    if grouped.is_some() && completed.outcome == Outcome::Clean {
        completed.outcome = Outcome::Findings;
    }
    let message = match completed.outcome {
        Outcome::Clean | Outcome::Excluded => None,
        Outcome::Missing => Some(format!(
            "{}: missing {}.\nNext action: {}\nReproduce: {reproduce}",
            path.display(),
            tool(linter).program(),
            installation(tool(linter))?
        )),
        Outcome::Timeout => Some(format!(
            "{}: {} timed out after {} seconds.\nReproduce: {reproduce}",
            path.display(),
            tool(linter).program(),
            LINTER_TIMEOUT.as_secs()
        )),
        Outcome::Findings | Outcome::Failed => {
            let output = format!("{}{}", completed.stdout, completed.stderr);
            let findings = grouped.unwrap_or(output);
            Some(format!(
                "{} ({})\n{findings}\nReproduce: {reproduce}",
                path.display(),
                tool(linter).program()
            ))
        }
    };
    Ok((completed.outcome, message))
}

fn bash_interpreter(first_line: &str, interpreter: Option<&str>) -> Option<String> {
    if let Some(interpreter) = interpreter {
        return (crate::shell_write_rules::program(interpreter) == "bash")
            .then(|| interpreter.to_owned());
    }
    let line = first_line.strip_prefix("#!")?;
    let mut words = line.split_whitespace();
    let executable = words.next()?;
    if executable.ends_with("/env") {
        return words.find(|word| *word == "bash").map(str::to_owned);
    }
    executable.ends_with("/bash").then(|| executable.to_owned())
}

fn portability(path: &Path, text: &str, interpreter: Option<&str>) -> Result<Vec<String>> {
    let Some(bash) = bash_interpreter(text.lines().next().unwrap_or_default(), interpreter) else {
        return Ok(Vec::new());
    };
    let mut command = crate::tool::external(&bash);
    command.arg(BASH_VERSION);
    let completed = bounded(&mut command, None)?;
    if completed.outcome != Outcome::Clean {
        return Ok(vec![format!(
            "Cannot determine the Bash interpreter version for {}: {bash} {BASH_VERSION}; {:?}.\nNext action: install the interpreter and run `{bash} {BASH_VERSION}`.",
            path.display(),
            completed.outcome
        )]);
    }
    let major = completed
        .stdout
        .split("version ")
        .nth(1)
        .and_then(|version| version.split('.').next())
        .and_then(|major| major.parse::<u32>().ok())
        .context("Bash returned no version")?;
    if major >= BASH4_MAJOR {
        return Ok(Vec::new());
    }
    let mut groups: BTreeMap<String, Group> = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        for feature in rules::bash4_features(line) {
            let group = groups.entry(format!("bash4.{feature:?}")).or_default();
            group.count += 1;
            if group.locations.len() < MAX_LOCATIONS {
                group.locations.push(format!("{}:{}: {bash} resolves to Bash {major}; use Bash 3 syntax or explicitly select Bash 4 or newer.", path.display(), index + 1));
            }
        }
    }
    let event = json!({"tool_name":"Bash", "cwd":path.parent(), "tool_input":{"command":format!("{} {}", quote(&bash), quote(&path.to_string_lossy()))}});
    let reproduce = reproduce_hook(&event, "claude-bash")?;
    Ok(render_groups(groups)
        .map(|message| format!("{message}\nReproduce: {reproduce}"))
        .into_iter()
        .collect())
}

/// Lint one file using its repository configuration and the interpreter a command explicitly selects.
///
/// # Errors
/// Returns an error when reading the file or a linter's output fails.
pub fn file(path: &Path, interpreter: Option<&str>) -> Result<Report> {
    let path = crate::canonical(path)?;
    let path = path.as_path();
    let mut prefix = Vec::new();
    fs::File::open(path)?
        .take(FIRST_LINE_BYTES)
        .read_to_end(&mut prefix)?;
    let prefix = String::from_utf8_lossy(&prefix);
    let first_line = prefix.lines().next().unwrap_or_default();
    let mut kind = rules::classify(&path.to_string_lossy(), first_line);
    if let Some(shell) = interpreter {
        kind = if crate::shell_write_rules::program(shell) == "zsh" {
            FileType::Zsh
        } else {
            FileType::Shell
        };
    }
    if kind == FileType::Other {
        return Ok(Report::default());
    }
    let metadata = fs::metadata(path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_FILE_BYTES,
        "{} must be a regular file no larger than {MAX_FILE_BYTES} bytes",
        path.display()
    );
    let text = fs::read_to_string(path)?;
    let root = repository(path);
    let mut report = Report::default();
    let mut outcomes = Vec::new();
    for &linter in rules::linters(kind) {
        match lint(path, linter, root.as_deref()) {
            Ok((outcome, finding)) => {
                outcomes.push(outcome);
                report.findings.extend(finding);
            }
            Err(error) => {
                outcomes.push(Outcome::Failed);
                report.findings.push(format!(
                    "{}: {} could not run: {error:#}",
                    path.display(),
                    tool(linter).program()
                ));
            }
        }
    }
    if kind == FileType::Shell {
        let found = portability(path, &text, interpreter)?;
        if !found.is_empty() {
            outcomes.push(Outcome::Findings);
            report.findings.extend(found);
        }
    }
    if rules::admission(&outcomes) == Admission::Admit {
        report.findings.clear();
    }
    Ok(report)
}

fn paths(event: &Value) -> Result<Vec<PathBuf>> {
    let tool = event["tool_name"]
        .as_str()
        .context("lint hook needs a string tool_name")?
        .rsplit(['.', ':'])
        .next()
        .unwrap_or_default();
    let input = &event["tool_input"];
    if tool == "apply_patch" {
        let patch = input
            .as_str()
            .or_else(|| {
                ["input", "patch", "patchText", "command"]
                    .iter()
                    .find_map(|key| input[key].as_str())
            })
            .context("apply_patch has no patch text")?;
        return Ok(patch
            .lines()
            .filter_map(|line| {
                PATCH_PREFIXES
                    .iter()
                    .find_map(|prefix| line.strip_prefix(prefix))
            })
            .map(PathBuf::from)
            .collect());
    }
    if !matches!(tool, "Edit" | "Write" | "MultiEdit" | "edit" | "write") {
        return Ok(Vec::new());
    }
    let path = input["file_path"]
        .as_str()
        .or_else(|| input["filePath"].as_str())
        .context("write tool has no file path")?;
    Ok(vec![PathBuf::from(path)])
}

/// Inspect only paths written by the hook event.
///
/// # Errors
/// Returns an error for malformed payloads or unreadable written files.
pub fn check(event: &Value) -> Result<Report> {
    let directory = match event.get("cwd") {
        Some(cwd) => PathBuf::from(cwd.as_str().context("lint hook cwd must be a string")?),
        None => std::env::current_dir()?,
    };
    let mut paths = paths(event)?;
    paths.sort();
    paths.dedup();
    let mut report = Report::default();
    for path in paths {
        let path = directory.join(path);
        if !path.try_exists()? {
            continue;
        }
        match file(&path, None) {
            Ok(found) => report.findings.extend(found.findings),
            Err(error) => report
                .findings
                .push(format!("{}: {error:#}", path.display())),
        }
    }
    Ok(report)
}

pub(crate) fn scripts(scripts: &[crate::legacy::InspectedScript]) -> Vec<String> {
    let mut findings = Vec::new();
    for script in scripts {
        match script {
            crate::legacy::InspectedScript::Unavailable(reason) => findings.push(reason.clone()),
            crate::legacy::InspectedScript::File { path, interpreter } => match file(path, interpreter.as_deref()) {
                Ok(report) => findings.extend(report.findings),
                Err(error) => findings.push(format!("Refused: cannot lint script {}: {error:#}\nNext action: use a literal path to a readable script.", path.display())),
            },
        }
    }
    findings
}

/// Answer a Claude Code or Codex `PostToolUse` event, or an `OpenCode` bridge event.
///
/// # Errors
/// Returns an error when reading the hook payload or serializing its response fails.
pub fn run() -> Result<i32> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let result = serde_json::from_str(&input)
        .context("lint hook input is not JSON")
        .and_then(|event| check(&event));
    let findings = match result {
        Ok(report) => report.findings,
        Err(error) => vec![format!("Lint hook refused invalid input: {error:#}")],
    };
    if findings.is_empty() {
        return Ok(0);
    }
    let reason = format!(
        "Fix the written files before continuing.\n{}",
        findings.join("\n\n")
    );
    println!("{}", json!({"decision":"block", "reason":reason}));
    eprintln!("{reason}");
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANY_FINDINGS: usize = 221;
    #[cfg(unix)]
    const TEST_TIMEOUT: Duration = Duration::from_millis(30);

    #[test]
    fn shell_syntax_limits_reject_oversized_sources_and_truncated_output() {
        let source = "x".repeat(usize::try_from(MAX_FILE_BYTES).unwrap());
        assert!(check_shell_syntax_source(&source).is_ok());
        assert!(check_shell_syntax_source(&format!("{source}x")).is_err());
        assert!(check_shell_syntax_source("jq \0").is_err());
        let output = "x".repeat(usize::try_from(MAX_OUTPUT_BYTES).unwrap());
        assert!(check_shell_syntax_output(&output[..output.len() - 1]).is_ok());
        assert!(check_shell_syntax_output(&output).is_err());
    }

    #[test]
    fn missing_executables_supply_the_pinned_installation_command() {
        let directory = tempfile::tempdir().unwrap();
        let mut command = crate::tool::external(directory.path().join("missing-linter"));
        assert_eq!(
            bounded(&mut command, None).unwrap().outcome,
            Outcome::Missing
        );
        assert_eq!(
            installation(Tool::Ruff).unwrap(),
            "mise install ruff@0.17.1"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_linter_deadline_terminates_and_reaps_the_child() {
        let mut command = Tool::Bash.command();
        command.args(["-c", "exec sleep 60"]);
        assert_eq!(
            bounded_for(&mut command, None, TEST_TIMEOUT)
                .unwrap()
                .outcome,
            Outcome::Timeout
        );
    }

    #[test]
    fn diagnostics_group_every_finding_and_limit_example_locations() {
        let diagnostics: Vec<Value> = (1..=MANY_FINDINGS)
            .map(|line| json!({"code":2250,"line":line,"column":1,"message":"Prefer braces"}))
            .chain([json!({"code":2016,"line":1,"column":1,"message":"Single quotes"})])
            .collect();
        let message = grouped(
            Linter::Shellcheck,
            Path::new("probe.sh"),
            &serde_json::to_string(&diagnostics).unwrap(),
        )
        .unwrap();
        assert!(message.contains("SC2250: 221 finding(s)"), "{message}");
        assert_eq!(message.matches("Prefer braces").count(), MAX_LOCATIONS);
        assert!(message.contains(SC2016_HINT), "{message}");
    }

    #[test]
    fn relative_scripts_require_a_known_absolute_execution_directory() {
        for directory in [None, Some(Path::new("."))] {
            let report =
                crate::legacy::findings("bash test/vulnerability_scan.sh", directory).unwrap();
            let findings = scripts(&report.scripts);
            assert_eq!(findings.len(), 1, "{directory:?}: {report:?}");
            assert!(findings[0].contains("known absolute execution directory"));
            assert_eq!(report.unavailable.len(), 1, "{directory:?}: {report:?}");
            assert!(
                report.unavailable[0].contains("execution directory"),
                "{directory:?}: {}",
                report.unavailable[0]
            );
        }
    }
}

//! The agent shell hooks' refusal of the legacy commands that the modern tool table maps to a preferred tool.

use crate::legacy_rules::{self, Verdict};
use crate::shell_write_rules::{program, segments};
use anyhow::{Context, Result, ensure};
use model::data::{Modern, ModernTool};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The table every profile pins through mise, which the hooks read as built.
const TABLE: &str = include_str!("../../../home/.chezmoidata/modern.toml");

/// The variable whose value `LEGACY: REASON` admits one refused command and records why.
/// Its value `SCRIPT_PATH: REASON` skips that script's mapped legacy findings and records why.
pub const BYPASS: &str = "DOTFILES_LEGACY_BYPASS";

/// The largest script the hook reads to inspect the commands it runs, 1 MiB.
const SCRIPT_LIMIT: u64 = 1024 * 1024;
const SCRIPT_PREFIX_BYTES: u64 = 4096;

/// How deep the hook follows scripts that a command runs.
pub(crate) const DEPTH: usize = 4;

/// The shells whose script argument the hook reads.
const SHELLS: [&str; 5] = ["sh", "bash", "zsh", "dash", "ksh"];

mod directory;
mod syntax;
mod translate;

pub(crate) fn nested_scripts(command: &str) -> Vec<String> {
    segments(command)
        .into_iter()
        .filter_map(|words| {
            let (words, _) = command_words(&words);
            let (first, rest) = words.split_first()?;
            if !SHELLS.contains(&program(first).as_str()) {
                return None;
            }
            let position = rest.iter().position(|word| {
                word.starts_with('-') && !word.starts_with("--") && word.contains('c')
            })?;
            rest.get(position + 1).cloned()
        })
        .collect()
}

/// The modern tool table.
///
/// # Errors
///
/// Returns an error when the table that the build embedded doesn't parse, which its tests rule out.
pub fn table() -> Result<&'static Modern> {
    static PARSED: OnceLock<Result<Modern, String>> = OnceLock::new();
    PARSED
        .get_or_init(|| {
            model::data::parse("modern.toml", TABLE)?
                .modern
                .ok_or_else(|| "modern.toml holds no [modern] table".to_owned())
        })
        .as_ref()
        .map_err(|error| anyhow::anyhow!("the modern tool table is invalid: {error}"))
}

fn is_assignment(word: &str) -> bool {
    legacy_rules::environment_word(word, legacy_rules::EnvironmentRole::Assignment)
        != legacy_rules::EnvironmentWord::Other
}

/// The command words after the assignments and wrappers that only start it, and the bypass value an assignment names.
pub(crate) fn command_words(words: &[String]) -> (&[String], Option<&str>) {
    match command_view(words) {
        CommandView::Known { command, bypass } => (command, bypass),
        CommandView::Unknown => (&[], None),
    }
}

enum CommandView<'a> {
    Known {
        command: &'a [String],
        bypass: Option<&'a str>,
    },
    Unknown,
}

fn command_view(words: &[String]) -> CommandView<'_> {
    let mut bypass = None;
    let mut rest = words;
    loop {
        rest = match rest {
            [word, tail @ ..] if is_assignment(word) => {
                if let Some(value) = word
                    .strip_prefix(BYPASS)
                    .and_then(|rest| rest.strip_prefix('='))
                {
                    bypass = Some(value);
                }
                tail
            }
            [word, tail @ ..] if matches!(program(word).as_str(), "env" | "sudo") => {
                let kind = if program(word) == "env" {
                    legacy_rules::WrapperKind::Env
                } else {
                    legacy_rules::WrapperKind::Sudo
                };
                let legacy_rules::CommandPrefix::Child { start } =
                    legacy_rules::wrapper_prefix(kind, tail)
                else {
                    return CommandView::Unknown;
                };
                let Some(child) = tail.get(start..) else {
                    return CommandView::Unknown;
                };
                child
            }
            [word, tail @ ..]
                if matches!(
                    program(word).as_str(),
                    "command" | "exec" | "nohup" | "nice"
                ) =>
            {
                match tail {
                    [flag, ..]
                        if program(word) == "command" && matches!(flag.as_str(), "-v" | "-V") =>
                    {
                        return CommandView::Known {
                            command: rest,
                            bypass,
                        };
                    }
                    [separator, child @ ..] if separator == "--" => child,
                    [flag, ..] if flag.starts_with('-') => return CommandView::Unknown,
                    _ => tail,
                }
            }
            [word, tail @ ..] if program(word) == "xargs" => {
                let mut arguments = tail;
                loop {
                    arguments = match arguments {
                        [flag, _, after @ ..]
                            if matches!(
                                flag.as_str(),
                                "-n" | "-P"
                                    | "-I"
                                    | "-L"
                                    | "-s"
                                    | "-d"
                                    | "--max-args"
                                    | "--max-procs"
                                    | "--replace"
                                    | "--delimiter"
                            ) =>
                        {
                            after
                        }
                        [flag, after @ ..] if flag.starts_with('-') => after,
                        _ => break,
                    };
                }
                arguments
            }
            [word, arguments @ ..] if program(word) == "mise" => {
                match legacy_rules::mise_prefix(arguments) {
                    legacy_rules::CommandPrefix::Terminal => {
                        return CommandView::Known {
                            command: rest,
                            bypass,
                        };
                    }
                    legacy_rules::CommandPrefix::Child { start } => {
                        let Some(child) = arguments.get(start..) else {
                            return CommandView::Unknown;
                        };
                        child
                    }
                    legacy_rules::CommandPrefix::Unknown => return CommandView::Unknown,
                }
            }
            _ => {
                return CommandView::Known {
                    command: rest,
                    bypass,
                };
            }
        };
    }
}

/// The script file a command runs, such as `bash build.sh`, `./build.sh`, or `. env.sh`.
pub(crate) fn script_path<'a>(
    name: &str,
    first: &'a str,
    arguments: &'a [String],
    directory: Option<&Path>,
    first_literal: bool,
) -> Option<&'a str> {
    if SHELLS.contains(&name) {
        let inline = arguments.iter().any(|argument| {
            argument.starts_with('-') && !argument.starts_with("--") && argument.contains('c')
        });
        return (!inline)
            .then(|| arguments.iter().find(|argument| !argument.starts_with('-')))
            .flatten()
            .map(String::as_str);
    }
    if name == "source" || first == "." {
        return arguments
            .iter()
            .find(|argument| !argument.starts_with('-'))
            .map(String::as_str);
    }
    let shell_file = Path::new(first).extension().is_some_and(|extension| {
        ["sh", "bash", "zsh"]
            .iter()
            .any(|name| extension.eq_ignore_ascii_case(name))
    });
    let shebang = if first.contains('/') && Path::new(first).extension().is_none() {
        if !first_literal
            || (Path::new(first).is_relative() && !directory.is_some_and(Path::is_absolute))
        {
            return Some(first);
        }
        let path =
            directory.map_or_else(|| Path::new(first).to_path_buf(), |root| root.join(first));
        let mut prefix = Vec::new();
        if let Ok(file) = std::fs::File::open(path)
            && file
                .take(SCRIPT_PREFIX_BYTES)
                .read_to_end(&mut prefix)
                .is_err()
        {
            return Some(first);
        }
        let text = String::from_utf8_lossy(&prefix);
        matches!(
            crate::lint_hook_rules::classify(first, text.lines().next().unwrap_or_default()),
            crate::lint_hook_rules::FileType::Shell | crate::lint_hook_rules::FileType::Zsh
        )
    } else {
        false
    };
    (first.contains('/') && (shell_file || shebang)).then_some(first)
}

pub(crate) fn assigned(
    words: &[String],
    variables: &mut std::collections::BTreeMap<String, String>,
) -> bool {
    let assignments = if words.first().is_some_and(|word| word == "export") {
        &words[1..]
    } else {
        words
    };
    if assignments.is_empty() || !assignments.iter().all(|word| is_assignment(word)) {
        return false;
    }
    for word in assignments {
        if let Some((name, value)) = word.split_once('=') {
            let facts: Vec<_> = variables
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str()))
                .collect();
            match crate::lint_hook_rules::resolve_script(value, &facts) {
                crate::lint_hook_rules::ScriptPath::Literal(value) => {
                    variables.insert(name.to_owned(), value);
                }
                crate::lint_hook_rules::ScriptPath::Unresolved => {
                    variables.remove(name);
                }
            }
        }
    }
    true
}

fn excepted(table: &Modern, path: &Path, directory: Option<&Path>) -> bool {
    let relative = directory.and_then(|directory| path.strip_prefix(directory).ok());
    let path = relative
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    table
        .exceptions
        .iter()
        .any(|exception| path == exception.path)
}

fn refusal_text(
    tool: &ModernTool,
    legacy: &str,
    arguments: Option<&[syntax::Argument<'_>]>,
) -> String {
    format!(
        "Refused: `{legacy}` is a legacy command, and the modern tool table prefers `{preferred}`: {reason}\nNext action: {next}\nThe legacy tool stays correct for: {keep}\nTo run it once anyway, put the table entry and your reason before the command: {BYPASS}='{legacy}: <reason>' {legacy} ...",
        preferred = tool.preferred,
        reason = tool.reason,
        next = arguments.map_or_else(
            || format!(
                "Use `{}` with the original shell arguments after checking their syntax.",
                tool.preferred
            ),
            |arguments| translate::rewrite_spelled(&tool.next, arguments).to_string(),
        ),
        keep = tool.keep,
    )
}

fn script_bypass_hint(path: &str) -> String {
    format!(
        "To run this one script anyway, put its path and your reason before the command: {BYPASS}='{path}: <reason>' bash {path} ..."
    )
}

/// Keep refusals and unavailable inspections independent across every script a command runs.
#[derive(Debug, Default)]
pub struct Findings {
    pub refusals: Vec<String>,
    pub wrapper_refusals: Vec<String>,
    pub unavailable: Vec<String>,
    pub(crate) bodies: Vec<crate::claude_hook::github::Body>,
    pub(crate) scripts: Vec<InspectedScript>,
}

#[derive(Debug)]
pub(crate) enum InspectedScript {
    File {
        path: PathBuf,
        interpreter: Option<String>,
    },
    Unavailable(String),
}

struct ScriptContext<'a> {
    directory: Option<&'a Path>,
    table: &'a Modern,
    depth: usize,
    variables: &'a std::collections::BTreeMap<String, String>,
    interpreter: Option<&'a str>,
}

impl Findings {
    fn inspect_inline(
        &mut self,
        argument: Option<&syntax::Argument<'_>>,
        directory: Option<&Path>,
        table: &Modern,
        depth: usize,
    ) {
        if let Some(script) = self.inline_script(argument) {
            if depth < DEPTH {
                self.append(inspect(&script, directory, table, depth + 1));
            } else {
                self.unavailable
                    .push("script nesting exceeds the inspection limit".to_owned());
            }
        }
    }

    fn inspect_wrapper(&mut self, invocation: &syntax::Invocation<'_>) -> bool {
        if invocation.unresolved_wrapper {
            let command = invocation.arguments(0).map_or_else(
                |_| invocation.values().join(" "),
                |arguments| {
                    arguments
                        .iter()
                        .map(|argument| argument.spelling)
                        .collect::<Vec<_>>()
                        .join(" ")
                },
            );
            self.wrapper_refusals.push(format!(
                "Refused: command `{command}` uses a wrapper whose startup files or options leave its execution directory unresolved.\nNext action: use supported wrapper options without startup overrides or directory changes."
            ));
        }
        invocation.unresolved_wrapper
    }

    fn inspect_legacy(
        &mut self,
        invocation: &syntax::Invocation<'_>,
        words: &[String],
        table: &Modern,
    ) {
        let (command, bypass) = command_words(words);
        let Some((first, rest)) = command.split_first() else {
            return;
        };
        let name = program(first);
        let arguments: Vec<_> = rest.iter().map(String::as_str).collect();
        let tool = table.tools.iter().find(|tool| {
            tool.legacy.iter().any(|legacy| legacy == name.as_str())
                && legacy_rules::invokes(
                    &arguments,
                    &tool.when.iter().map(String::as_str).collect::<Vec<_>>(),
                )
                && !(tool.stream && legacy_rules::reads_stream(&arguments))
        });
        let bypassed = bypass.is_some_and(|value| legacy_rules::bypass_names(value, name.as_str()));
        match (legacy_rules::verdict(tool.is_some(), false, bypassed), tool) {
            (Verdict::Refuse, Some(tool)) => {
                let spelling = invocation.arguments(words.len() - rest.len());
                self.refusals
                    .push(refusal_text(tool, name.as_str(), spelling.as_deref().ok()));
            }
            (Verdict::Bypassed, _) => {
                let reason = bypass
                    .and_then(|value| value.split_once(':'))
                    .map_or("", |(_, reason)| reason.trim());
                record_bypass(reason, words);
            }
            _ => {}
        }
    }

    fn inspect_publications(
        &mut self,
        invocation: &syntax::Invocation<'_>,
        directory: Option<&Path>,
        files_safe: bool,
    ) -> Vec<String> {
        let mut words = invocation.values();
        let spelled = invocation.arguments(0).ok();
        if let Some(arguments) = &spelled {
            for (word, argument) in words.iter_mut().zip(arguments) {
                if let Some(value) = crate::claude_hook::github::literal_word(argument.spelling) {
                    *word = value;
                }
            }
        }
        let spellings = spelled.as_ref().map(|arguments| {
            arguments
                .iter()
                .map(|argument| argument.spelling)
                .collect::<Vec<_>>()
        });
        self.bodies.extend(crate::claude_hook::github::collect(
            &words,
            spellings.as_deref(),
            invocation.literal_stdin(),
            directory,
            files_safe && invocation.file_inputs_safe(),
        ));
        words
    }

    fn inline_script(&mut self, argument: Option<&syntax::Argument<'_>>) -> Option<String> {
        let script = argument
            .and_then(|argument| crate::claude_hook::github::literal_word(argument.spelling));
        if script.is_none() {
            self.unavailable
                .push("the inline shell script is not a known literal".to_owned());
            self.bodies
                .push(crate::claude_hook::github::Body::Unreadable(
                    "the inline shell script can supply an uninspected GitHub body".to_owned(),
                ));
        }
        script
    }

    fn inspect_script(
        &mut self,
        path: &str,
        argument: Option<&syntax::Argument<'_>>,
        words: &[String],
        files_safe: bool,
        context: &ScriptContext<'_>,
    ) {
        if !files_safe {
            self.bodies
                .push(crate::claude_hook::github::Body::Unreadable(
                "the shell script has an uncertain execution context for GitHub body inspection"
                    .to_owned(),
            ));
        }
        if argument
            .and_then(|argument| crate::claude_hook::github::literal_word(argument.spelling))
            .is_none()
        {
            self.bodies
                .push(crate::claude_hook::github::Body::Unreadable(
                    "the shell script path can supply an uninspected GitHub body".to_owned(),
                ));
        }
        let facts: Vec<_> = context
            .variables
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        let crate::lint_hook_rules::ScriptPath::Literal(resolved) =
            crate::lint_hook_rules::resolve_script(path, &facts)
        else {
            self.refusals.push(format!("Refused: cannot resolve script path {path}.\nNext action: use a literal path, or assign its variables to literal paths in this command."));
            return;
        };
        let path = resolved.as_str();
        let (_, bypass) = command_words(words);
        if let Some(value) = bypass.filter(|value| legacy_rules::script_bypass_names(value, path)) {
            let reason = value
                .strip_prefix(path)
                .and_then(|suffix| suffix.strip_prefix(':'))
                .map_or("", str::trim);
            record_bypass(reason, words);
            match script_refusal(
                path,
                context.directory,
                context.table,
                context.depth,
                context.interpreter,
            ) {
                Ok(report) => {
                    self.bodies.extend(report.bodies);
                    self.wrapper_refusals.extend(report.wrapper_refusals);
                    self.unavailable.extend(report.unavailable);
                    self.scripts.extend(report.scripts);
                }
                Err(error) => {
                    self.scripts.push(InspectedScript::Unavailable(format!(
                        "Refused: cannot lint script {path}: {error:#}"
                    )));
                    self.bodies.push(crate::claude_hook::github::Body::Unreadable(format!(
                        "the bypassed script could not be inspected for GitHub bodies: {error:#}"
                    )));
                }
            }
        } else {
            match script_refusal(
                path,
                context.directory,
                context.table,
                context.depth,
                context.interpreter,
            )
            .with_context(|| format!("inspect the script {path}"))
            {
                Ok(report) => self.append(report),
                Err(error) => {
                    self.scripts.push(InspectedScript::Unavailable(format!(
                        "Refused: cannot lint script {path}: {error:#}"
                    )));
                    self.unavailable
                        .push(format!("{error:#}\n{}", script_bypass_hint(path)));
                }
            }
        }
    }

    fn append(&mut self, other: Self) {
        self.refusals.extend(other.refusals);
        self.wrapper_refusals.extend(other.wrapper_refusals);
        self.unavailable.extend(other.unavailable);
        self.bodies.extend(other.bodies);
        self.scripts.extend(other.scripts);
    }

    fn inspect_stdin(
        &mut self,
        invocation: &syntax::Invocation<'_>,
        directory: Option<&Path>,
        table: &Modern,
        depth: usize,
    ) {
        if !invocation.inputs().is_empty() && invocation.literal_stdin().is_none() {
            self.unavailable.push("Refused: the shell's standard-input script could not be inspected.\nNext action: use a literal script path or a literal heredoc for inspection.".to_owned());
            self.bodies
                .push(crate::claude_hook::github::Body::Unreadable(
                    "the shell's standard-input script is unavailable for GitHub body inspection"
                        .to_owned(),
                ));
            return;
        }
        for input in invocation.inputs() {
            match input {
                syntax::Input::Script(script) if depth < DEPTH => self.append(inspect(script, directory, table, depth + 1)),
                syntax::Input::Script(_) => self.unavailable.push("script nesting exceeds the inspection limit".to_owned()),
                syntax::Input::Unavailable => self.unavailable.push("Refused: the shell's standard-input script could not be inspected.\nNext action: use a literal script path or a literal heredoc for inspection.".to_owned()),
            }
        }
    }
}

/// Every mapped invocation and unavailable inspection in the command's bounded scripts.
/// A bypass that names the entry with a reason admits the command and appends a line to the Git bypass log.
/// A script bypass skips mapped legacy findings while mandatory GitHub body inspection remains active.
///
/// # Errors
///
/// Returns an error when the embedded table doesn't parse.
pub fn findings(command: &str, directory: Option<&Path>) -> Result<Findings> {
    Ok(inspect(command, directory, table()?, 0))
}

#[derive(Clone, Copy)]
pub(crate) enum Shell {
    Bash,
    PowerShell,
}

impl Shell {
    fn invocations<'a>(
        self,
        command: &'a str,
        directory: Option<&Path>,
        findings: &mut Findings,
    ) -> Vec<syntax::Invocation<'a>> {
        let unparsed = || {
            segments(command)
                .into_iter()
                .map(syntax::Invocation::unparsed)
                .collect()
        };
        match self {
            Self::PowerShell => match syntax::parse_powershell_in(command, directory) {
                Ok(invocations) => invocations,
                Err(error) => {
                    findings.unavailable.push(format!("Refused: PowerShell argument spelling could not be verified: {error:#}.\nNext action: correct the PowerShell syntax or restore the pinned parser before using a suggested command."));
                    unparsed()
                }
            },
            Self::Bash => match syntax::parse_in(command, directory) {
                Ok(invocations) => invocations,
                Err(error) => {
                    findings.unavailable.push(format!("Refused: shell argument spelling could not be verified: {error:#}.\nNext action: correct the shell syntax or restore the pinned shfmt parser before using a suggested command."));
                    unparsed()
                }
            },
        }
    }
}

pub(crate) fn findings_in_shell(
    command: &str,
    directory: Option<&Path>,
    shell: Shell,
) -> Result<Findings> {
    Ok(inspect_shell(command, directory, table()?, 0, shell))
}

/// Checks the byte ranges in argument metadata without executing the shell source.
///
/// # Errors
///
/// Returns an error for unreadable syntax metadata, unsupported source, or an invalid source range.
pub fn check_shell_spelling_tree(source: &str, tree: &str) -> Result<()> {
    syntax::check(source, tree)
}

/// Checks native PowerShell argument metadata for the submitted source.
///
/// # Errors
/// Returns an error for malformed metadata, another source, or invalid argument ranges.
pub fn check_powershell_spelling_tree(source: &str, tree: &str) -> Result<()> {
    syntax::check_powershell(source, tree)
}

/// The joined diagnostics, or `None` when every supported inspection admits the command.
///
/// # Errors
///
/// Returns an error when the embedded table doesn't parse.
pub fn refusal(command: &str, directory: Option<&Path>) -> Result<Option<String>> {
    let findings = findings(command, directory)?;
    let messages: Vec<_> = findings
        .refusals
        .into_iter()
        .chain(findings.wrapper_refusals)
        .chain(findings.unavailable)
        .collect();
    Ok((!messages.is_empty()).then(|| messages.join("\n\n")))
}

fn script_refusal(
    path: &str,
    directory: Option<&Path>,
    table: &Modern,
    depth: usize,
    interpreter: Option<&str>,
) -> Result<Findings> {
    ensure!(
        Path::new(path).is_absolute() || directory.is_some_and(Path::is_absolute),
        "the relative script has no known absolute execution directory"
    );
    let resolved = match directory {
        Some(directory) if Path::new(path).is_relative() => directory.join(path),
        _ => Path::new(path).to_path_buf(),
    };
    let skip_legacy = excepted(table, &resolved, directory);
    let metadata = resolved.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= SCRIPT_LIMIT,
        "the script {path} must be a regular file no larger than {SCRIPT_LIMIT} bytes"
    );
    ensure!(depth < DEPTH, "script nesting exceeds the inspection limit");
    let mut bytes = Vec::new();
    std::fs::File::open(&resolved)?
        .take(SCRIPT_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        u64::try_from(bytes.len())? <= SCRIPT_LIMIT,
        "the script grew beyond its inspection limit"
    );
    let text = String::from_utf8(bytes)
        .with_context(|| format!("read the script {}", resolved.display()))?;
    let mut findings = inspect(&text, directory, table, depth + 1);
    findings.scripts.push(InspectedScript::File {
        path: resolved,
        interpreter: interpreter.map(str::to_owned),
    });
    if skip_legacy {
        findings.refusals.clear();
    }
    for message in findings
        .refusals
        .iter_mut()
        .chain(&mut findings.unavailable)
    {
        message.push_str("\nThe script ");
        message.push_str(path);
        message.push_str(" runs this command.");
        message.push('\n');
        message.push_str(&script_bypass_hint(path));
    }
    Ok(findings)
}

/// Appends an admitted bypass to the owner's Git bypass log, or under unit tests to the thread's own record.
fn record_bypass(reason: &str, words: &[String]) {
    #[cfg(not(test))]
    dotguard::bypass::record("bypassed", dotguard::bypass::Rule::Legacy, reason, words);
    #[cfg(test)]
    tests::BYPASSES.with(|log| {
        log.borrow_mut()
            .push(format!("{reason}\t{}", words.join(" ")));
    });
}

/// The preferred command that prints exactly what `command` prints, when it starts with `cat FILE...` or `sed -n 'A,Bp' FILE`.
/// Only the leading legacy words change, so the rest of the command line keeps its quoting and expansions.
#[must_use]
pub fn replacement(command: &str) -> Option<String> {
    const BAT: &str = "bat --plain --paging=never --binary=as-text";
    let words = segments(command).into_iter().next()?;
    let (first, rest) = words.split_first()?;
    let arguments: Vec<&str> = rest.iter().map(String::as_str).collect();
    let remainder = command.trim_start().strip_prefix(first.as_str())?;
    let blank = |text: &str| text.starts_with([' ', '\t']);
    if !blank(remainder) {
        return None;
    }
    match legacy_rules::replacement(first, &arguments)? {
        legacy_rules::Replacement::Files => Some(format!("{BAT}{remainder}")),
        legacy_rules::Replacement::Lines { first, last } => {
            let after = remainder.trim_start().strip_prefix("-n")?;
            if !blank(after) {
                return None;
            }
            let (script, tail) = shell_word(after.trim_start())?;
            (Some(script) == arguments.get(1).copied())
                .then(|| format!("{BAT} --line-range {first}:{last}{tail}"))
        }
    }
}

/// The text of one leading shell word without a backslash, variable, or substitution, and the text after it.
fn shell_word(text: &str) -> Option<(&str, &str)> {
    let first = text.chars().next()?;
    let (word, tail) = if let quote @ ('\'' | '"') = first {
        let inner = &text[1..];
        let end = inner.find(quote)?;
        (&inner[..end], &inner[end + 1..])
    } else {
        let end = text
            .find(|c: char| c.is_whitespace() || ";|&<>()".contains(c))
            .unwrap_or(text.len());
        text.split_at(end)
    };
    (!word.contains(['\\', '$', '`', '\'', '"'])).then_some((word, tail))
}

fn inspect(command: &str, directory: Option<&Path>, table: &Modern, depth: usize) -> Findings {
    inspect_shell(command, directory, table, depth, Shell::Bash)
}

fn inspect_shell(
    command: &str,
    directory: Option<&Path>,
    table: &Modern,
    depth: usize,
    shell: Shell,
) -> Findings {
    let mut findings = Findings::default();
    let mut scopes = std::collections::BTreeMap::<
        directory::VariableScope,
        std::collections::BTreeMap<String, String>,
    >::new();
    let invocations = shell.invocations(command, directory, &mut findings);
    let files_safe = depth == 0 && invocations.len() == 1;
    for invocation in invocations {
        let directory = invocation.directory.directory().map(PathBuf::as_path);
        let mut variables = invocation
            .variable_scope
            .and_then(|scope| scopes.remove(&scope))
            .unwrap_or_default();
        if directory.is_none() {
            variables.clear();
        }
        if findings.inspect_wrapper(&invocation) {
            continue;
        }
        let words = findings.inspect_publications(&invocation, directory, files_safe);
        let spelled = invocation.arguments(0).ok();
        let assignment = assigned(&words, &mut variables);
        let (command_words, _) = command_words(&words);
        let Some((first, rest)) = command_words.split_first() else {
            if assignment && let Some(scope) = invocation.variable_scope {
                scopes.insert(scope, variables);
            }
            continue;
        };
        let name = program(first);
        let name = name.as_str();
        let start = words.len() - rest.len() - 1;
        let path = script_path(
            name,
            first,
            rest,
            directory,
            invocation.literal(start).is_some(),
        );
        let inline = rest.iter().position(|word| {
            word.starts_with('-') && !word.starts_with("--") && word.contains('c')
        });
        if SHELLS.contains(&name) && inline.is_some_and(|position| rest.get(position + 1).is_some())
        {
            let argument = words.len() - rest.len() + inline.unwrap_or_default() + 1;
            findings.inspect_inline(
                spelled
                    .as_ref()
                    .and_then(|arguments| arguments.get(argument)),
                directory,
                table,
                depth,
            );
        }
        if SHELLS.contains(&name) && path.is_none() && inline.is_none() {
            findings.inspect_stdin(&invocation, directory, table, depth);
        }
        if (SHELLS.contains(&name) || path.is_some())
            && dynamic_xargs_script(&words, command_words, path)
        {
            findings.refusals.push("Refused: xargs supplies an unresolved script path.\nNext action: use a literal path in the shell's script argument before invoking xargs.".to_owned());
            continue;
        }
        if let Some(path) = path {
            let position = if first == path {
                Some(words.len() - rest.len() - 1)
            } else {
                rest.iter()
                    .position(|argument| argument == path)
                    .map(|position| words.len() - rest.len() + position)
            };
            let argument = position.and_then(|position| {
                spelled
                    .as_ref()
                    .and_then(|arguments| arguments.get(position))
            });
            findings.inspect_script(
                path,
                argument,
                &words,
                files_safe && invocation.file_inputs_safe(),
                &ScriptContext {
                    directory,
                    table,
                    depth,
                    variables: &variables,
                    interpreter: SHELLS.contains(&name).then_some(first.as_str()),
                },
            );
        }
        findings.inspect_legacy(&invocation, &words, table);
        if assignment && let Some(scope) = invocation.variable_scope {
            scopes.insert(scope, variables);
        }
    }
    findings
}

fn dynamic_xargs_script(words: &[String], command: &[String], path: Option<&str>) -> bool {
    use crate::lint_hook_rules::{ScriptArgument, ScriptInput, script_argument};
    let wrappers: Vec<_> = words
        .iter()
        .take(words.len().saturating_sub(command.len()))
        .collect();
    let xargs = wrappers.iter().any(|word| program(word) == "xargs");
    let replacement = wrappers.iter().enumerate().find_map(|(index, word)| {
        if *word == "-I" {
            return wrappers.get(index + 1).map(|word| word.as_str());
        }
        word.strip_prefix("-I").filter(|value| !value.is_empty())
    });
    let replaced = replacement.is_some_and(|marker| path.is_some_and(|path| path.contains(marker)));
    let inline = command
        .iter()
        .skip(1)
        .any(|word| word.starts_with('-') && !word.starts_with("--") && word.contains('c'));
    let input = if path.is_none() {
        ScriptInput::Missing
    } else if replaced {
        ScriptInput::Replaced
    } else {
        ScriptInput::Literal
    };
    script_argument(xargs, input, inline) == ScriptArgument::NeedsLiteral
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapper_directory_flags_in_child_arguments_are_not_wrapper_options() -> Result<()> {
        let directory = tempfile::tempdir()?;
        for shell in [Shell::Bash, Shell::PowerShell] {
            for command in [
                "mise x -- cargo clippy --locked --all-targets --all-features --manifest-path .ci/ci-policy/Cargo.toml -- -D warnings",
                "mise x rust@1.99.0 -- cargo clippy -- -D warnings",
                "mise exec rust@1.99.0 just@1.58.0 -- cargo clippy -- -D warnings",
                "env -- cargo check -C child --chdir other --directory next --cd final",
                "sudo -- cargo clippy -- -D warnings",
                "mise x -- env -- sudo -- cargo clippy -- -D warnings",
                "env -- cargo -- mise x -C /child",
                "mise ls",
                "mise --quiet ls",
                "mise -v ls",
                "mise --quiet run bash check.sh",
            ] {
                let report = findings_in_shell(command, Some(directory.path()), shell)?;
                assert!(report.unavailable.is_empty(), "{command}: {report:?}");
                assert!(report.refusals.is_empty(), "{command}: {report:?}");
            }
        }
        Ok(())
    }

    #[test]
    fn wrapper_directory_nested_changes_and_unknown_options_refuse_stale_reads() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let wrong = directory.path().join("check.sh");
        std::fs::write(&wrong, "cat wrong\n")?;
        for shell in [Shell::Bash, Shell::PowerShell] {
            for command in [
                "env -C/unresolved bash check.sh",
                "env -S 'bash check.sh'",
                "sudo -D /unresolved bash check.sh",
                "mise x -C /unresolved -- bash check.sh",
                "mise -C /unresolved x -- bash check.sh",
                "mise x -- mise x -- env -C/unresolved bash check.sh",
                "mise x -- env --chdir=/unresolved bash check.sh",
                "MODE=check mise x -- env -C/unresolved bash check.sh",
                "mise x -- command -- env -C/unresolved bash check.sh",
                "mise x rust@1.99.0 -- env BASH_ENV=reset.sh bash check.sh",
                "env -- mise x -- sudo -D /unresolved bash check.sh",
                "sudo -- mise x -- env --chdir=/unresolved bash check.sh",
                "mise x -- env --unknown bash check.sh",
                "sudo --unknown bash check.sh",
                "mise x --unknown -- bash check.sh",
                "mise x -c 'bash check.sh'",
                "mise x rust@1.99.0 --",
            ] {
                let report = findings_in_shell(command, Some(directory.path()), shell)?;
                assert!(
                    report
                        .wrapper_refusals
                        .iter()
                        .any(|message| message.contains("wrapper")),
                    "{command}: {report:?}"
                );
                assert!(!report.scripts.iter().any(|script| matches!(script, InspectedScript::File { path, .. } if path == &wrong)), "{command}: {report:?}");
            }
        }
        Ok(())
    }

    #[test]
    fn wrapper_directory_stable_nested_commands_keep_script_inspection() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let script = directory.path().join("check.sh");
        std::fs::write(&script, "grep selected input\n")?;
        for shell in [Shell::Bash, Shell::PowerShell] {
            for command in [
                "mise x -- env MODE=check sudo -- bash check.sh",
                "env -- mise x rust@1.99.0 -- bash check.sh",
                "sudo -n -- mise exec rust@1.99.0 -- bash check.sh",
                "env -u BASH_ENV bash check.sh",
            ] {
                let report = findings_in_shell(command, Some(directory.path()), shell)?;
                assert!(report.unavailable.is_empty(), "{command}: {report:?}");
                assert!(
                    report
                        .refusals
                        .iter()
                        .any(|message| message.contains("`grep`")),
                    "{command}: {report:?}"
                );
                assert!(report.scripts.iter().any(|entry| matches!(entry, InspectedScript::File { path, .. } if path == &script)), "{command}: {report:?}");
            }
        }
        Ok(())
    }

    #[test]
    fn wrapper_directory_command_queries_remain_terminal() -> Result<()> {
        let directory = tempfile::tempdir()?;
        for shell in [Shell::Bash, Shell::PowerShell] {
            for command in [
                "command -v nu",
                "command -V bash",
                "mise x -- command -v env",
                "mise --version",
                "mise -V",
                "mise --help",
                "mise -h",
                "mise --quiet --version",
                "env -- mise --help",
            ] {
                let report = findings_in_shell(command, Some(directory.path()), shell)?;
                assert!(report.unavailable.is_empty(), "{command}: {report:?}");
                assert!(report.refusals.is_empty(), "{command}: {report:?}");
            }
        }
        Ok(())
    }

    #[test]
    fn wrapper_directory_startup_assignments_remain_owned_after_separators() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let wrong = directory.path().join("check.sh");
        std::fs::write(&wrong, "cat wrong\n")?;
        for shell in [Shell::Bash, Shell::PowerShell] {
            for command in [
                "mise x -- env -- BASH_ENV=reset.sh bash check.sh",
                "mise x -- sudo -- BASH_ENV=reset.sh bash check.sh",
                "sudo BASH_ENV=reset.sh bash check.sh",
            ] {
                let report = findings_in_shell(command, Some(directory.path()), shell)?;
                assert!(
                    report
                        .wrapper_refusals
                        .iter()
                        .any(|message| message.contains("wrapper")),
                    "{report:?}"
                );
                assert!(
                    !report.scripts.iter().any(|script| matches!(script, InspectedScript::File { path, .. } if path == &wrong)),
                    "{report:?}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn wrapper_directory_mise_global_options_are_not_task_arguments() -> Result<()> {
        let directory = tempfile::tempdir()?;
        for shell in [Shell::Bash, Shell::PowerShell] {
            for command in [
                "mise run --cd /unresolved task",
                "mise trust --cd=/unresolved file",
            ] {
                let report = findings_in_shell(command, Some(directory.path()), shell)?;
                assert!(
                    report
                        .wrapper_refusals
                        .iter()
                        .any(|message| message.contains("wrapper")),
                    "{command}: {report:?}"
                );
            }
            let report =
                findings_in_shell("mise run task -- --cd child", Some(directory.path()), shell)?;
            assert!(report.unavailable.is_empty(), "{report:?}");
        }
        Ok(())
    }

    #[test]
    fn directory_flow_child_and_prefix_assignments_never_select_the_parent_script() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let wrong = directory.path().join("wrong.sh");
        std::fs::write(&wrong, "cat wrong\n")?;
        std::fs::write(
            directory.path().join("selected.sh"),
            "grep selected input\n",
        )?;
        for command in [
            "script=selected.sh; (script=wrong.sh); bash \"$script\"",
            "script=selected.sh; script=wrong.sh bash \"$script\"",
            "script=selected.sh; printf '%s' \"$(script=wrong.sh)\"; bash \"$script\"",
            "script=selected.sh; false && script=wrong.sh; bash \"$script\"",
        ] {
            let report = findings(command, Some(directory.path()))?;
            assert!(
                !report.scripts.iter().any(|script| matches!(script,
                InspectedScript::File { path, .. } if path == &wrong)),
                "{command}: {report:?}"
            );
            assert!(
                report
                    .refusals
                    .iter()
                    .any(|message| message.contains("`grep`"))
                    || report
                        .refusals
                        .iter()
                        .any(|message| message.contains("cannot resolve script path"))
                    || !report.unavailable.is_empty(),
                "{command}: {report:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn directory_flow_startup_and_environment_wrappers_refuse_stale_script_reads() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let wrong = directory.path().join("check.sh");
        std::fs::write(&wrong, "cat wrong\n")?;
        std::fs::write(directory.path().join("reset.sh"), "cd /unresolved\n")?;
        for command in [
            "BASH_ENV=reset.sh bash check.sh",
            "env -C/unresolved bash check.sh",
            "env -S 'bash check.sh'",
            "bash --rcfile reset.sh -i check.sh",
            "bash --login check.sh",
            "export BASH_ENV=reset.sh; bash check.sh",
            "declare -x BASH_ENV=reset.sh; bash check.sh",
            "read -r BASH_ENV; bash check.sh",
            "printf -v BASH_ENV '%s' reset.sh; bash check.sh",
            "bash -i check.sh",
        ] {
            let report = findings(command, Some(directory.path()))?;
            assert!(
                !report.scripts.iter().any(|script| matches!(script,
                InspectedScript::File { path, .. } if path == &wrong)),
                "{command}: {report:?}"
            );
            assert!(
                !report.unavailable.is_empty() || !report.wrapper_refusals.is_empty(),
                "{command}: {report:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn directory_flow_successful_bash_change_selects_the_child_directory() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let child = directory.path().join("selected");
        std::fs::create_dir(&child)?;
        std::fs::write(directory.path().join("check.sh"), "cat wrong\n")?;
        std::fs::write(child.join("check.sh"), "grep selected input\n")?;
        let target = child.to_string_lossy().replace('\\', "/");
        let report = findings(
            &format!("cd -- '{target}' && bash check.sh"),
            Some(directory.path()),
        )?;
        assert!(
            report
                .refusals
                .iter()
                .any(|message| message.contains("`grep`")),
            "{report:?}"
        );
        assert!(
            !report
                .refusals
                .iter()
                .any(|message| message.contains("`cat`")),
            "{report:?}"
        );
        Ok(())
    }

    #[test]
    fn directory_flow_unknown_changes_refuse_before_relative_script_reads() -> Result<()> {
        let directory = tempfile::tempdir()?;
        std::fs::write(directory.path().join("check.sh"), "cat wrong\n")?;
        std::fs::write(directory.path().join("check"), "#!/bin/sh\ncat wrong\n")?;
        std::fs::write(directory.path().join("reset.sh"), "cd -- /unresolved\n")?;
        for command in [
            "cd -- /unresolved; bash check.sh",
            "cd -- \"$target\" && bash check.sh",
            "if ready; then cd -- /unresolved; fi; bash check.sh",
            "cd() { :; }; cd -- /unresolved && bash check.sh",
            "cd -- /unresolved; ./check",
            "source reset.sh; bash check.sh",
            "pushd /unresolved; bash check.sh",
            "command cd /unresolved; bash check.sh",
            "env --chdir=/unresolved bash check.sh",
        ] {
            let report = findings(command, Some(directory.path()))?;
            assert!(
                report
                    .unavailable
                    .iter()
                    .chain(&report.wrapper_refusals)
                    .any(|message| message.contains("execution directory")),
                "{command}: {report:?}"
            );
            assert!(
                !report
                    .refusals
                    .iter()
                    .any(|message| message.contains("`cat`")),
                "{command}: {report:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn directory_flow_powershell_continue_and_bindings_refuse_relative_reads() -> Result<()> {
        let directory = tempfile::tempdir()?;
        std::fs::write(directory.path().join("check.sh"), "cat wrong\n")?;
        for prefix in [
            "Set-Location -LiteralPath 'unresolved'",
            "try { Set-Location -LiteralPath 'unresolved' -ErrorAction Stop } catch { }",
            "function Set-Location { }; Set-Location -LiteralPath 'unresolved' -ErrorAction Stop",
            "Set-Location '-LiteralPath' 'unresolved' '-ErrorAction' 'Stop'",
            "Push-Location 'unresolved'",
        ] {
            let report = findings_in_shell(
                &format!("{prefix}; bash check.sh"),
                Some(directory.path()),
                Shell::PowerShell,
            )?;
            assert!(
                report
                    .unavailable
                    .iter()
                    .any(|message| message.contains("execution directory")),
                "{prefix}: {report:?}"
            );
            assert!(
                !report
                    .refusals
                    .iter()
                    .any(|message| message.contains("`cat`")),
                "{prefix}: {report:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn directory_flow_absolute_entries_keep_the_callers_directory() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let entry = directory.path().join("entries");
        std::fs::create_dir(&entry)?;
        std::fs::write(entry.join("outer.sh"), "bash child.sh\n")?;
        std::fs::write(entry.join("child.sh"), "cat wrong\n")?;
        std::fs::write(directory.path().join("child.sh"), "grep selected input\n")?;
        let path = entry.join("outer.sh").to_string_lossy().replace('\\', "/");
        let report = findings(&format!("bash '{path}'"), Some(directory.path()))?;
        assert!(
            report
                .refusals
                .iter()
                .any(|message| message.contains("`grep`")),
            "{report:?}"
        );
        assert!(
            !report
                .refusals
                .iter()
                .any(|message| message.contains("`cat`")),
            "{report:?}"
        );
        assert!(report.scripts.iter().any(|script| matches!(script, InspectedScript::File { path, .. } if path == &directory.path().join("child.sh"))), "{report:?}");
        Ok(())
    }

    #[test]
    fn directory_flow_children_do_not_change_later_parent_invocations() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let child = directory.path().join("selected");
        std::fs::create_dir(&child)?;
        std::fs::write(directory.path().join("parent.sh"), "cat parent\n")?;
        std::fs::write(child.join("parent.sh"), "wc -l wrong\n")?;
        std::fs::write(child.join("check.sh"), "grep selected input\n")?;
        let target = child.to_string_lossy().replace('\\', "/");
        for command in [
            format!("(cd -- '{target}' && bash check.sh); bash parent.sh"),
            format!("bash -c \"cd -- '{target}' && bash check.sh\"; bash parent.sh"),
        ] {
            let report = findings(&command, Some(directory.path()))?;
            for expected in ["`grep`", "`cat`"] {
                assert!(
                    report
                        .refusals
                        .iter()
                        .any(|message| message.contains(expected)),
                    "{command}: {report:?}"
                );
            }
            assert!(
                !report
                    .refusals
                    .iter()
                    .any(|message| message.contains("`wc`")),
                "{command}: {report:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn directory_flow_an_external_cd_file_does_not_change_the_parent_directory() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let child = directory.path().join("selected");
        std::fs::create_dir(&child)?;
        std::fs::write(
            directory.path().join("cd"),
            "#!/bin/sh\nprintf 'ready\\n'\n",
        )?;
        std::fs::write(directory.path().join("check.sh"), "cat parent\n")?;
        std::fs::write(child.join("check.sh"), "grep wrong input\n")?;
        let target = child.to_string_lossy().replace('\\', "/");
        for executable in ["./cd", "CD", "cd.exe"] {
            let report = findings(
                &format!("{executable} -- '{target}' && bash check.sh"),
                Some(directory.path()),
            )?;
            assert!(
                report
                    .refusals
                    .iter()
                    .any(|message| message.contains("`cat`")),
                "{executable}: {report:?}"
            );
            assert!(
                !report
                    .refusals
                    .iter()
                    .any(|message| message.contains("`grep`")),
                "{executable}: {report:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn directory_flow_powershell_argument_effects_invalidate_the_invocation_directory() -> Result<()>
    {
        let directory = tempfile::tempdir()?;
        std::fs::write(directory.path().join("check.sh"), "cat wrong\n")?;
        for command in [
            "Write-Output $(Set-Location -LiteralPath 'unresolved'); bash check.sh",
            "ForEach-Object -Begin { Set-Location -LiteralPath 'unresolved' }; bash check.sh",
            "bash check.sh $(Set-Location -LiteralPath 'unresolved')",
        ] {
            let report = findings_in_shell(command, Some(directory.path()), Shell::PowerShell)?;
            assert!(
                report
                    .unavailable
                    .iter()
                    .any(|message| message.contains("execution directory")),
                "{command}: {report:?}"
            );
            assert!(
                !report
                    .refusals
                    .iter()
                    .any(|message| message.contains("`cat`")),
                "{command}: {report:?}"
            );
        }
        Ok(())
    }

    fn assert_powershell_builtin_location_refuses_stale_read(command: &str) -> Result<()> {
        let directory = tempfile::tempdir()?;
        let initial = directory.path().join("initial");
        std::fs::create_dir(&initial)?;
        let stale = initial.join("check.sh");
        std::fs::write(&stale, "cat wrong\n")?;
        std::fs::write(directory.path().join("check.sh"), "grep selected input\n")?;
        let report = findings_in_shell(command, Some(&initial), Shell::PowerShell)?;
        assert!(
            !report.scripts.iter().any(
                |script| matches!(script, InspectedScript::File { path, .. } if path == &stale)
            ),
            "{command}: {report:?}"
        );
        assert!(
            !report
                .refusals
                .iter()
                .any(|message| message.contains("`cat`")),
            "{command}: {report:?}"
        );
        assert!(
            report
                .unavailable
                .iter()
                .any(|message| message.contains("execution directory")),
            "{command}: {report:?}"
        );
        Ok(())
    }

    #[test]
    fn directory_flow_powershell_builtin_parent_refuses_stale_reads() -> Result<()> {
        assert_powershell_builtin_location_refuses_stale_read("cd..; bash check.sh")
    }

    #[test]
    fn directory_flow_powershell_builtin_home_refuses_stale_reads() -> Result<()> {
        assert_powershell_builtin_location_refuses_stale_read("& 'cd~'; bash check.sh")
    }

    #[test]
    fn directory_flow_powershell_builtin_root_refuses_stale_reads() -> Result<()> {
        for command in [r"cd\; bash check.sh", r"& 'cd\'; bash check.sh"] {
            assert_powershell_builtin_location_refuses_stale_read(command)?;
        }
        Ok(())
    }

    #[test]
    fn directory_flow_powershell_builtin_drives_refuse_stale_reads() -> Result<()> {
        for drive in b'A'..=b'Z' {
            assert_powershell_builtin_location_refuses_stale_read(&format!(
                "{}:; bash check.sh",
                char::from(drive)
            ))?;
        }
        assert_powershell_builtin_location_refuses_stale_read("c:; bash check.sh")
    }

    #[test]
    fn directory_flow_qualified_powershell_location_never_reads_the_old_directory() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let child = directory.path().join("selected files");
        std::fs::create_dir(&child)?;
        std::fs::write(directory.path().join("check.sh"), "cat wrong\n")?;
        std::fs::write(child.join("check.sh"), "grep selected input\n")?;
        let target = child.to_string_lossy().replace('\\', "/");
        let qualified = r"Microsoft.PowerShell.Management\Set-Location";
        let command =
            format!("& '{qualified}' -LiteralPath '{target}' -ErrorAction Stop; bash check.sh");
        let calls = syntax::parse_powershell_in(&command, Some(directory.path()))?;
        assert_eq!(
            calls.first().and_then(|call| call.literal(0)).as_deref(),
            Some(qualified)
        );
        let report = findings_in_shell(&command, Some(directory.path()), Shell::PowerShell)?;
        assert!(
            !report
                .refusals
                .iter()
                .any(|message| message.contains("`cat`")),
            "{report:?}"
        );
        assert!(
            report
                .refusals
                .iter()
                .any(|message| message.contains("`grep`"))
                || report
                    .unavailable
                    .iter()
                    .any(|message| message.contains("execution directory")),
            "{report:?}"
        );
        assert!(
            report.scripts.iter().any(|script| matches!(script, InspectedScript::File { path, .. } if path == &child.join("check.sh"))),
            "{report:?}"
        );
        let unknown = command.replace(qualified, r"Other.Module\Set-Location");
        let report = findings_in_shell(&unknown, Some(directory.path()), Shell::PowerShell)?;
        assert!(
            report
                .unavailable
                .iter()
                .any(|message| message.contains("execution directory")),
            "{report:?}"
        );
        assert!(
            !report
                .refusals
                .iter()
                .any(|message| message.contains("`cat`")),
            "{report:?}"
        );
        Ok(())
    }

    #[test]
    fn directory_flow_powershell_script_scope_never_preserves_a_stale_location() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let child = directory.path().join("selected files");
        std::fs::create_dir(&child)?;
        let target = child.to_string_lossy().replace('\\', "/");
        std::fs::write(
            directory.path().join("move.ps1"),
            format!("Set-Location -LiteralPath '{target}' -ErrorAction Stop\n"),
        )?;
        std::fs::write(directory.path().join("check.sh"), "cat wrong\n")?;
        std::fs::write(child.join("check.sh"), "grep selected input\n")?;
        let command = "& './move.ps1'; bash check.sh";
        let report = findings_in_shell(command, Some(directory.path()), Shell::PowerShell)?;
        assert!(
            !report
                .refusals
                .iter()
                .any(|message| message.contains("`cat`")),
            "{report:?}"
        );
        assert!(
            report
                .refusals
                .iter()
                .any(|message| message.contains("`grep`"))
                || report
                    .unavailable
                    .iter()
                    .any(|message| message.contains("execution directory")),
            "{report:?}"
        );
        Ok(())
    }

    #[test]
    fn directory_flow_powershell_stop_and_stable_calls_share_resolved_lint_entries() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let child = directory.path().join("selected files");
        std::fs::create_dir(&child)?;
        std::fs::write(directory.path().join("check.sh"), "cat wrong\n")?;
        std::fs::write(child.join("check.sh"), "grep selected input\n")?;
        let target = child.to_string_lossy().replace('\\', "/");
        for (command, root) in [
            (
                format!("Set-Location -LiteralPath '{target}' -ErrorAction Stop; bash check.sh"),
                directory.path(),
            ),
            ("bash check.sh".to_owned(), child.as_path()),
        ] {
            let report = findings_in_shell(&command, Some(root), Shell::PowerShell)?;
            assert!(
                report
                    .refusals
                    .iter()
                    .any(|message| message.contains("`grep`")),
                "{report:?}"
            );
            assert!(
                !report
                    .refusals
                    .iter()
                    .any(|message| message.contains("`cat`")),
                "{report:?}"
            );
            assert!(report.scripts.iter().any(|script| matches!(script, InspectedScript::File { path, .. } if path == &child.join("check.sh"))), "{report:?}");
        }
        Ok(())
    }

    #[test]
    fn powershell_literal_script_keeps_inspectable_file_inputs() -> Result<()> {
        let directory = tempfile::tempdir()?;
        std::fs::write(directory.path().join("check.sh"), "printf 'ready\\n'\n")?;
        let report = findings_in_shell("bash check.sh", Some(directory.path()), Shell::PowerShell)?;
        assert!(report.bodies.is_empty(), "{report:?}");
        assert!(report.unavailable.is_empty(), "{report:?}");
        assert!(report.refusals.is_empty(), "{report:?}");
        Ok(())
    }

    thread_local! {
        /// The bypasses a test admitted, which stay out of the owner's Git bypass log.
        pub(super) static BYPASSES: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    #[test]
    fn exact_replacements_change_only_the_leading_legacy_words() {
        for (command, expected) in [
            (
                "cat Cargo.toml",
                "bat --plain --paging=never --binary=as-text Cargo.toml",
            ),
            (
                "cat 'a b.txt' \"$HOME/c\" | jaq .x",
                "bat --plain --paging=never --binary=as-text 'a b.txt' \"$HOME/c\" | jaq .x",
            ),
            (
                "sed -n '2,4p' src/main.rs",
                "bat --plain --paging=never --binary=as-text --line-range 2:4 src/main.rs",
            ),
            (
                "  sed -n 7p notes.md | rg x",
                "bat --plain --paging=never --binary=as-text --line-range 7:7 notes.md | rg x",
            ),
        ] {
            assert_eq!(replacement(command).as_deref(), Some(expected), "{command}");
        }
        for command in [
            "cat -n Cargo.toml",
            "cat",
            "cat <<'EOF'\nx\nEOF",
            "/bin/cat Cargo.toml",
            "env cat Cargo.toml",
            "cd x && cat Cargo.toml",
            "sed -n '2,4p'",
            "sed -n \"$range\" file",
            "sed -i '2,4p' file",
            "sed -n '$p' file",
            "ls -la",
        ] {
            assert_eq!(replacement(command), None, "{command}");
        }
    }

    #[test]
    fn a_line_count_of_a_stream_passes_and_of_a_file_names_rg() {
        assert_eq!(refusal("rg x src | wc -l", None).unwrap(), None);
        assert_eq!(refusal("git log --oneline | wc -l -", None).unwrap(), None);
        let message = refused("wc -l src/main.rs");
        assert!(message.contains("`rg -c ''`"), "{message}");
    }

    fn refused(command: &str) -> String {
        refusal(command, None)
            .unwrap()
            .unwrap_or_else(|| panic!("admitted: {command}"))
    }

    #[test]
    fn suggestions_preserve_shell_argument_spelling() {
        for arguments in [
            "-c '.foo' ~/.codex/hooks.json",
            "'.foo' '~/path with spaces'",
            "'.foo' ~/path\\ with\\ spaces",
            "'.foo' \"$HOME/file name\" ''",
            "'.foo' 'literal'\"suffix\"",
        ] {
            let message = refused(&format!("jq {arguments} 2>&1"));
            assert!(
                message
                    .lines()
                    .any(|line| line == format!("Next action: jaq {arguments}")),
                "{message}"
            );
        }
    }

    #[test]
    fn an_inline_script_inside_a_heredoc_is_inspected_once() -> Result<()> {
        let findings = findings("sh <<'EOF'\nbash -c 'grep x f'\nEOF", None)?;
        assert_eq!(findings.refusals.len(), 1, "{:?}", findings.refusals);
        assert!(
            findings.unavailable.is_empty(),
            "{:?}",
            findings.unavailable
        );
        Ok(())
    }

    #[test]
    fn the_table_parses_and_names_one_entry_per_legacy_command() {
        let table = table().unwrap();
        let mut seen = std::collections::BTreeSet::new();
        for tool in &table.tools {
            assert!(
                !tool.version.is_empty() && !tool.reason.is_empty(),
                "{}",
                tool.preferred
            );
            for legacy in &tool.legacy {
                assert!(seen.insert(legacy.clone()), "{legacy} has two entries");
            }
        }
        assert!(seen.contains("grep") && seen.contains("find"));
    }

    #[test]
    fn a_recursive_grep_is_refused_with_the_rg_equivalent() {
        let message = refused("grep -r pattern .");
        assert!(message.contains("Next action: rg pattern ."), "{message}");
        assert!(message.contains(BYPASS), "{message}");
        for command in [
            "cd src && grep -n x a.rs",
            "env LC_ALL=C grep x f",
            "mise x -- find . -name '*.rs'",
            "rg x | jq .x",
            "bash -c 'sed s/a/b/'",
            "sh <<'EOF'\ngrep x f\nEOF",
            "wc -l src/main.rs",
            "ls -la",
        ] {
            assert!(refused(command).contains("Next action"), "{command}");
        }
    }

    #[test]
    fn preferred_tools_conditional_entries_and_bypasses_are_admitted() {
        for command in [
            "rg -n x src",
            "fd -e rs",
            "wc -c file",
            "git grep x",
            "just check",
            "DOTFILES_LEGACY_BYPASS='grep: a caller parses GNU grep output' grep -c x f",
        ] {
            assert_eq!(refusal(command, None).unwrap(), None, "{command}");
        }
        BYPASSES.with(|log| {
            assert!(
                log.borrow()
                    .iter()
                    .any(|line| line.starts_with("a caller parses GNU grep output\t")),
                "{log:?}"
            );
        });
        for command in [
            "DOTFILES_LEGACY_BYPASS='find: reason' grep x f",
            "DOTFILES_LEGACY_BYPASS='grep:' grep x f",
        ] {
            assert!(refusal(command, None).unwrap().is_some(), "{command}");
        }
    }

    #[test]
    fn scripts_a_command_runs_are_inspected_unless_the_table_lists_them() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::write(root.join("scan.sh"), "#!/bin/sh\ngrep -r TODO .\n").unwrap();
        std::fs::create_dir_all(root.join("home/.chezmoitemplates/partials")).unwrap();
        std::fs::write(
            root.join("home/.chezmoitemplates/partials/install-mise-sh"),
            "#!/bin/sh\ncurl -fsSL https://mise.run | tar -x\n",
        )
        .unwrap();
        let message = refusal("sh scan.sh", Some(root)).unwrap().unwrap();
        assert!(
            message.contains("rg TODO .") && message.contains("scan.sh"),
            "{message}"
        );
        assert!(refusal("./scan.sh", Some(root)).unwrap().is_some());
        assert_eq!(
            refusal(
                "sh home/.chezmoitemplates/partials/install-mise-sh",
                Some(root)
            )
            .unwrap(),
            None
        );
        assert_eq!(
            findings("bash missing.sh", Some(root))
                .unwrap()
                .unavailable
                .len(),
            1
        );
    }

    #[test]
    fn a_script_bypass_admits_and_records_each_script_invocation_once() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("scan.sh"), "grep -r TODO .\n").unwrap();
        for (invocation, path) in [
            ("sh scan.sh argument", "scan.sh"),
            ("bash scan.sh argument", "scan.sh"),
            ("zsh scan.sh argument", "scan.sh"),
            ("dash scan.sh argument", "scan.sh"),
            ("ksh scan.sh argument", "scan.sh"),
            ("source scan.sh argument", "scan.sh"),
            (". scan.sh argument", "scan.sh"),
            ("./scan.sh argument", "./scan.sh"),
        ] {
            let refused = refusal(invocation, Some(directory.path()))
                .unwrap()
                .unwrap_or_else(|| panic!("admitted: {invocation}"));
            assert!(refused.contains("`grep` is a legacy command"), "{refused}");
            assert!(
                refused.contains(&format!("{BYPASS}='{path}: <reason>' bash {path}")),
                "{refused}"
            );
            let command = format!("{BYPASS}='{path}: pinned upstream tag' {invocation}");
            BYPASSES.with(|log| log.borrow_mut().clear());
            assert_eq!(
                refusal(&command, Some(directory.path())).unwrap(),
                None,
                "{command}"
            );
            BYPASSES.with(|log| {
                assert_eq!(
                    *log.borrow(),
                    [format!(
                        "pinned upstream tag\t{BYPASS}={path}: pinned upstream tag {invocation}"
                    )],
                    "{command}"
                );
            });
        }
    }

    #[test]
    fn a_script_bypass_preserves_spaces_and_colons_in_paths_and_reasons() {
        for path in ["C:\\tmp\\scan.sh", "/tmp/upstream:tag/install.sh", "a b.sh"] {
            let command = format!("{BYPASS}='{path}: pinned: upstream' bash '{path}' argument");
            BYPASSES.with(|log| log.borrow_mut().clear());
            assert_eq!(refusal(&command, None).unwrap(), None, "{command}");
            BYPASSES.with(|log| {
                assert_eq!(
                    *log.borrow(),
                    [format!(
                        "pinned: upstream\t{BYPASS}={path}: pinned: upstream bash {path} argument"
                    )],
                    "{command}"
                );
            });
        }
    }

    #[test]
    fn a_script_bypass_requires_its_exact_path_and_a_reason() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("scan.sh"), "grep proof Cargo.toml\n").unwrap();
        for value in [
            "other.sh: upstream",
            "./scan.sh: upstream",
            " scan.sh: upstream",
            "scan.sh : upstream",
            "scan.shx: upstream",
            "scan.sh",
            "scan.sh:",
            "scan.sh: \u{2003}",
            "grep: upstream",
        ] {
            let command = format!("{BYPASS}='{value}' bash scan.sh");
            BYPASSES.with(|log| log.borrow_mut().clear());
            let message = refusal(&command, Some(directory.path())).unwrap().unwrap();
            assert!(message.contains("`grep` is a legacy command"), "{command}");
            BYPASSES.with(|log| assert!(log.borrow().is_empty(), "{command}"));
        }
    }

    #[test]
    fn a_script_bypass_keeps_sibling_commands_and_inline_scripts_inspected() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("scan.sh"), "grep proof Cargo.toml\n").unwrap();
        let command = format!(
            "{BYPASS}='scan.sh: upstream' bash scan.sh; grep proof Cargo.toml; bash scan.sh; bash -c 'awk 1 Cargo.toml'"
        );
        BYPASSES.with(|log| log.borrow_mut().clear());
        let report = findings(&command, Some(directory.path())).unwrap();
        assert!(report.unavailable.is_empty(), "{report:?}");
        assert_eq!(report.refusals.len(), 3, "{report:?}");
        let messages = report.refusals.join("\n");
        assert_eq!(messages.matches("`grep` is a legacy command").count(), 2);
        assert_eq!(messages.matches("`awk` is a legacy command").count(), 1);
        BYPASSES.with(|log| assert_eq!(log.borrow().len(), 1));
    }

    #[test]
    fn a_script_bypass_keeps_the_invocations_own_command_inspected() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("scan.sh"), "grep proof Cargo.toml\n").unwrap();
        let mut table = table().unwrap().clone();
        table.tools.push(ModernTool {
            legacy: vec!["bash".to_owned()],
            ..ModernTool::default()
        });
        let report = inspect(
            &format!("{BYPASS}='scan.sh: upstream' bash scan.sh"),
            Some(directory.path()),
            &table,
            0,
        );
        assert!(report.unavailable.is_empty(), "{report:?}");
        assert_eq!(report.refusals.len(), 1, "{report:?}");
        assert!(report.refusals[0].contains("`bash` is a legacy command"));
    }

    #[test]
    fn a_script_bypass_preserves_mandatory_unavailable_body_inspections() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::write(root.join("outer.sh"), "sh nested.sh; sh missing.sh\n").unwrap();
        std::fs::write(root.join("nested.sh"), "grep proof Cargo.toml\n").unwrap();
        std::fs::write(root.join("broken.sh"), [0xff]).unwrap();
        std::fs::create_dir(root.join("directory.sh")).unwrap();
        std::fs::File::create(root.join("oversize.sh"))
            .unwrap()
            .set_len(SCRIPT_LIMIT + 1)
            .unwrap();
        for path in [
            "outer.sh",
            "missing.sh",
            "broken.sh",
            "directory.sh",
            "oversize.sh",
        ] {
            let message = refusal(&format!("sh {path}"), Some(root)).unwrap().unwrap();
            assert!(
                message.contains(&format!("{BYPASS}='{path}: <reason>'")),
                "{message}"
            );
            let command = format!("{BYPASS}='{path}:  pinned upstream tag  ' sh {path}");
            for depth in [0, DEPTH] {
                BYPASSES.with(|log| log.borrow_mut().clear());
                let report = inspect(&command, Some(root), table().unwrap(), depth);
                assert!(report.refusals.is_empty(), "{path}: {report:?}");
                assert!(
                    !report.unavailable.is_empty()
                        || report.bodies.iter().any(|body| matches!(
                            body,
                            crate::claude_hook::github::Body::Unreadable(_)
                        )),
                    "{path}: {report:?}"
                );
                BYPASSES.with(|log| {
                    let log = log.borrow();
                    assert_eq!(log.len(), 1, "{path}");
                    assert!(log[0].starts_with("pinned upstream tag\t"), "{log:?}");
                });
            }
        }
    }

    #[test]
    fn compound_commands_report_every_mapped_invocation_before_resubmission() {
        let command = "rg -n -i proof src | head -80; ls -la; fd -H -t f assure; wc -l src/*.rs";
        let message = refused(command);
        for tool in ["ls", "wc"] {
            assert!(
                message.contains(&format!("`{tool}` is a legacy command")),
                "{message}"
            );
        }
        assert!(message.contains("eza -la"), "{message}");
        assert!(message.contains("tokei"), "{message}");
        assert_eq!(
            refusal(
                "rg -n -i proof src | head -80; eza -la; fd -H -t f assure; tokei src",
                None
            )
            .unwrap(),
            None
        );
    }

    #[test]
    fn compound_diagnostics_keep_bypasses_scoped_and_include_nested_scripts() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("scan.sh"),
            "grep -n proof src/main.rs\nwc -l src/main.rs\n",
        )
        .unwrap();
        let message = refusal("DOTFILES_LEGACY_BYPASS='ls: structured output compatibility' ls -la; bash scan.sh; bash -c 'cat src/main.rs'; ls -l", Some(directory.path())).unwrap().unwrap();
        for tool in ["grep", "wc", "cat", "ls"] {
            assert!(
                message.contains(&format!("`{tool}` is a legacy command")),
                "{message}"
            );
        }
        assert_eq!(
            message.matches("`ls` is a legacy command").count(),
            1,
            "{message}"
        );
        assert!(message.contains("scan.sh"), "{message}");
    }

    #[test]
    fn unavailable_nested_inspections_preserve_known_findings_from_other_segments() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("broken.sh"), [0xff]).unwrap();
        std::fs::write(directory.path().join("valid.sh"), "grep proof Cargo.toml\n").unwrap();
        let report = findings(
            "sh broken.sh; ls -la; sh missing.sh; sh valid.sh; wc -l Cargo.toml",
            Some(directory.path()),
        )
        .unwrap();
        assert_eq!(report.unavailable.len(), 2, "{report:?}");
        assert_eq!(report.refusals.len(), 3, "{report:?}");
        for name in ["ls", "grep", "wc"] {
            assert!(
                report
                    .refusals
                    .iter()
                    .any(|message| message.contains(&format!("`{name}` is a legacy command"))),
                "{report:?}"
            );
        }
    }

    #[test]
    fn an_exceeded_script_depth_preserves_sibling_findings() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("loop.sh"), "sh loop.sh; ls -la\n").unwrap();
        let report = findings("sh loop.sh; wc -l Cargo.toml", Some(directory.path())).unwrap();
        assert_eq!(report.unavailable.len(), 1, "{report:?}");
        assert!(
            report
                .unavailable
                .first()
                .unwrap()
                .contains("nesting exceeds")
        );
        assert_eq!(report.refusals.len(), DEPTH + 1, "{report:?}");
    }
}

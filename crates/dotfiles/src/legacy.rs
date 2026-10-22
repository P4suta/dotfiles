//! The agent shell hooks' refusal of the legacy commands that the modern tool table maps to a preferred tool.

use crate::legacy_rules::{self, Verdict};
use crate::shell_write_rules::{program, segments};
use anyhow::{Context, Result, ensure};
use model::data::{Modern, ModernTool};
use std::path::Path;
use std::sync::OnceLock;

/// The table every profile pins through mise, which the hooks read as built.
const TABLE: &str = include_str!("../../../home/.chezmoidata/modern.toml");

/// The variable whose value `LEGACY: REASON` admits one refused command and records why.
/// Its value `SCRIPT_PATH: REASON` skips inspecting that one script and records why.
pub const BYPASS: &str = "DOTFILES_LEGACY_BYPASS";

/// The largest script the hook reads to inspect the commands it runs, 1 MiB.
const SCRIPT_LIMIT: u64 = 1024 * 1024;

/// How deep the hook follows scripts that a command runs.
pub(crate) const DEPTH: usize = 4;

/// The shells whose script argument the hook reads.
const SHELLS: [&str; 5] = ["sh", "bash", "zsh", "dash", "ksh"];

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
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// The command words after the assignments and wrappers that only start it, and the bypass value an assignment names.
pub(crate) fn command_words(words: &[String]) -> (&[String], Option<&str>) {
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
            [word, tail @ ..]
                if matches!(
                    program(word).as_str(),
                    "env" | "command" | "exec" | "nohup" | "sudo" | "nice"
                ) =>
            {
                tail
            }
            [word, verb, tail @ ..]
                if program(word) == "mise" && matches!(verb.as_str(), "x" | "exec") =>
            {
                match tail {
                    [separator, after @ ..] if separator == "--" => after,
                    _ => tail,
                }
            }
            _ => return (rest, bypass),
        };
    }
}

/// The script file a command runs, such as `bash build.sh`, `./build.sh`, or `. env.sh`.
fn script_path<'a>(name: &str, first: &'a str, arguments: &'a [String]) -> Option<&'a str> {
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
    let shell_file = Path::new(first)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("sh"));
    (first.contains('/') && shell_file).then_some(first)
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

fn refusal_text(tool: &ModernTool, legacy: &str, arguments: &[&str]) -> String {
    format!(
        "Refused: `{legacy}` is a legacy command, and the modern tool table prefers `{preferred}`: {reason}\nNext action: {next}\nThe legacy tool stays correct for: {keep}\nTo run it once anyway, put the table entry and your reason before the command: {BYPASS}='{legacy}: <reason>' {legacy} ...",
        preferred = tool.preferred,
        reason = tool.reason,
        next = translate::rewrite(&tool.next, arguments),
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
    pub unavailable: Vec<String>,
}

impl Findings {
    fn append(&mut self, other: Self) {
        self.refusals.extend(other.refusals);
        self.unavailable.extend(other.unavailable);
    }
}

/// Every mapped invocation and unavailable inspection in the command's bounded scripts.
/// A bypass that names the entry with a reason admits the command and appends a line to the Git bypass log.
/// A bypass that names the exact script path with a reason skips inspecting that script and records it once.
///
/// # Errors
///
/// Returns an error when the embedded table doesn't parse.
pub fn findings(command: &str, directory: Option<&Path>) -> Result<Findings> {
    Ok(inspect(command, directory, table()?, 0))
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
        .chain(findings.unavailable)
        .collect();
    Ok((!messages.is_empty()).then(|| messages.join("\n\n")))
}

fn script_refusal(
    path: &str,
    directory: Option<&Path>,
    table: &Modern,
    depth: usize,
) -> Result<Findings> {
    let resolved = match directory {
        Some(directory) if Path::new(path).is_relative() => directory.join(path),
        _ => Path::new(path).to_path_buf(),
    };
    if excepted(table, &resolved, directory) {
        return Ok(Findings::default());
    }
    let metadata = resolved.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= SCRIPT_LIMIT,
        "the script {path} must be a regular file no larger than {SCRIPT_LIMIT} bytes"
    );
    ensure!(depth < DEPTH, "script nesting exceeds the inspection limit");
    let text = std::fs::read_to_string(&resolved)
        .with_context(|| format!("read the script {}", resolved.display()))?;
    let mut findings = inspect(&text, directory, table, depth + 1);
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
    let nested = depth < DEPTH;
    let mut findings = Findings::default();
    for words in segments(command) {
        let (command_words, bypass) = command_words(&words);
        let Some((first, rest)) = command_words.split_first() else {
            continue;
        };
        let name = program(first);
        let name = name.as_str();
        if let Some(path) = script_path(name, first, rest) {
            if let Some(value) =
                bypass.filter(|value| legacy_rules::script_bypass_names(value, path))
            {
                let reason = value
                    .strip_prefix(path)
                    .and_then(|suffix| suffix.strip_prefix(':'))
                    .map_or("", str::trim);
                record_bypass(reason, &words);
            } else {
                match script_refusal(path, directory, table, depth)
                    .with_context(|| format!("inspect the script {path}"))
                {
                    Ok(report) => findings.append(report),
                    Err(error) => findings
                        .unavailable
                        .push(format!("{error:#}\n{}", script_bypass_hint(path))),
                }
            }
        }
        let arguments: Vec<&str> = rest.iter().map(String::as_str).collect();
        let tool = table.tools.iter().find(|tool| {
            tool.legacy.iter().any(|legacy| legacy == name)
                && legacy_rules::invokes(
                    &arguments,
                    &tool.when.iter().map(String::as_str).collect::<Vec<_>>(),
                )
                && !(tool.stream && legacy_rules::reads_stream(&arguments))
        });
        let bypassed = bypass.is_some_and(|value| legacy_rules::bypass_names(value, name));
        match (legacy_rules::verdict(tool.is_some(), false, bypassed), tool) {
            (Verdict::Refuse, Some(tool)) => {
                findings.refusals.push(refusal_text(tool, name, &arguments));
            }
            (Verdict::Bypassed, _) => {
                let reason = bypass
                    .and_then(|value| value.split_once(':'))
                    .map_or("", |(_, reason)| reason.trim());
                record_bypass(reason, &words);
            }
            _ => {}
        }
    }
    for script in nested_scripts(command) {
        if nested {
            findings.append(inspect(script.as_str(), directory, table, depth + 1));
        } else {
            findings
                .unavailable
                .push("script nesting exceeds the inspection limit".to_owned());
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_script_bypass_skips_nested_and_unavailable_inspections_before_reading() {
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
                assert!(report.unavailable.is_empty(), "{path}: {report:?}");
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

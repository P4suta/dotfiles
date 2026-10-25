/// What the agent shell hooks do with a command that a legacy tool runs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    /// The modern tool table maps nothing here, or the command runs a bootstrap script the table lists as an exception.
    Admit,
    /// A recorded bypass names the table entry with a reason, so the hook admits the command and records the bypass.
    Bypassed,
    /// The table maps the command to its preferred tool.
    Refuse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnvironmentRole {
    Assignment,
    ExportOperand,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnvironmentWord {
    Startup,
    Assignment,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WrapperKind {
    Env,
    Sudo,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandPrefix {
    Terminal,
    Child { start: usize },
    Unknown,
}

#[must_use]
pub fn environment_word(word: &str, role: EnvironmentRole) -> EnvironmentWord {
    let assignment = word.split_once('=');
    let name = match assignment {
        Some((name, _))
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') =>
        {
            name
        }
        None if role == EnvironmentRole::ExportOperand => word,
        Some(_) | None => return EnvironmentWord::Other,
    };
    if matches!(name, "BASH_ENV" | "ENV" | "ZDOTDIR" | "PWD" | "OLDPWD") {
        EnvironmentWord::Startup
    } else if assignment.is_some() {
        EnvironmentWord::Assignment
    } else {
        EnvironmentWord::Other
    }
}

#[must_use]
pub fn wrapper_prefix<T: AsRef<str>>(kind: WrapperKind, arguments: &[T]) -> CommandPrefix {
    let original = arguments;
    let mut arguments = arguments;
    let child = loop {
        arguments = match arguments {
            [separator, child @ ..] if separator.as_ref() == "--" => break child,
            [flag, child @ ..]
                if (kind == WrapperKind::Env
                    && matches!(flag.as_ref(), "-i" | "--ignore-environment"))
                    || (kind == WrapperKind::Sudo
                        && matches!(flag.as_ref(), "-n" | "--non-interactive")) =>
            {
                child
            }
            [flag, value, child @ ..]
                if ((kind == WrapperKind::Env && matches!(flag.as_ref(), "-u" | "--unset"))
                    || (kind == WrapperKind::Sudo
                        && matches!(flag.as_ref(), "-u" | "--user" | "-g" | "--group")))
                    && !value.as_ref().is_empty()
                    && !value.as_ref().starts_with('-') =>
            {
                child
            }
            [first, ..] if first.as_ref().starts_with('-') => return CommandPrefix::Unknown,
            _ => break arguments,
        };
    };
    let startup = child
        .iter()
        .map(AsRef::as_ref)
        .take_while(|word| {
            environment_word(word, EnvironmentRole::Assignment) != EnvironmentWord::Other
        })
        .any(|word| {
            environment_word(word, EnvironmentRole::Assignment) == EnvironmentWord::Startup
        });
    if startup {
        CommandPrefix::Unknown
    } else {
        CommandPrefix::Child {
            start: original.len() - child.len(),
        }
    }
}

#[must_use]
pub fn mise_prefix<T: AsRef<str>>(arguments: &[T]) -> CommandPrefix {
    let original = arguments;
    let mut arguments = arguments;
    while let [flag, child @ ..] = arguments
        && matches!(
            flag.as_ref(),
            "-q" | "--quiet" | "-v" | "--verbose" | "-y" | "--yes" | "--locked" | "--silent"
        )
    {
        arguments = child;
    }
    match arguments {
        [verb, arguments @ ..] if matches!(verb.as_ref(), "x" | "exec") => {
            let Some(separator) = arguments
                .iter()
                .position(|argument| argument.as_ref() == "--")
            else {
                return CommandPrefix::Unknown;
            };
            let (tools, child) = arguments.split_at(separator);
            if tools
                .iter()
                .any(|tool| tool.as_ref().is_empty() || tool.as_ref().starts_with('-'))
            {
                return CommandPrefix::Unknown;
            }
            match child {
                [_, command @ ..] if !command.is_empty() => CommandPrefix::Child {
                    start: original.len() - command.len(),
                },
                _ => CommandPrefix::Unknown,
            }
        }
        [query] if matches!(query.as_ref(), "-V" | "--version" | "-h" | "--help") => {
            CommandPrefix::Terminal
        }
        [flag, ..] if flag.as_ref().starts_with('-') => CommandPrefix::Unknown,
        _ => {
            if arguments
                .iter()
                .map(AsRef::as_ref)
                .take_while(|argument| *argument != "--")
                .any(|argument| {
                    matches!(argument, "-C" | "--cd")
                        || argument.starts_with("--cd=")
                        || argument.starts_with("-C")
                })
            {
                CommandPrefix::Unknown
            } else {
                CommandPrefix::Terminal
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryContext<T> {
    directory: Option<T>,
    native_binding: bool,
}

impl<T> DirectoryContext<T> {
    #[must_use]
    pub const fn new(directory: Option<T>) -> Self {
        Self {
            directory,
            native_binding: true,
        }
    }

    #[must_use]
    pub const fn directory(&self) -> Option<&T> {
        self.directory.as_ref()
    }

    #[must_use]
    pub fn invalidate(self) -> Self {
        Self {
            directory: None,
            native_binding: false,
        }
    }

    #[must_use]
    pub fn changed(self, directory: Option<T>) -> Self {
        Self {
            directory: if self.native_binding { directory } else { None },
            ..self
        }
    }
}

impl<T: Eq> DirectoryContext<T> {
    #[must_use]
    pub fn joined(self, other: &Self) -> Self {
        if !self.native_binding || !other.native_binding {
            return self.invalidate();
        }
        if self.directory == other.directory {
            self
        } else {
            self.changed(None)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceRange {
    start: usize,
    end: usize,
}

impl SourceRange {
    #[must_use]
    pub const fn bounds(self) -> (usize, usize) {
        (self.start, self.end)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpellingSpan {
    InSource(SourceRange),
    Unpositioned,
    Empty,
    Outside,
    NonBoundary,
}

#[must_use]
pub const fn spelling_span(
    start: usize,
    end: usize,
    length: usize,
    positioned: bool,
    boundaries: [bool; 2],
) -> SpellingSpan {
    if !positioned {
        SpellingSpan::Unpositioned
    } else if start >= end {
        SpellingSpan::Empty
    } else if end > length {
        SpellingSpan::Outside
    } else if !boundaries[0] || !boundaries[1] {
        SpellingSpan::NonBoundary
    } else {
        SpellingSpan::InSource(SourceRange { start, end })
    }
}

#[must_use]
pub fn verdict(mapped: bool, excepted: bool, bypassed: bool) -> Verdict {
    if !mapped || excepted {
        Verdict::Admit
    } else if bypassed {
        Verdict::Bypassed
    } else {
        Verdict::Refuse
    }
}

#[must_use]
pub const fn spelling_follows(previous: Option<usize>, current: usize) -> bool {
    match previous {
        None => true,
        Some(previous) => previous < current,
    }
}

/// Whether a bypass value of the form `LEGACY: REASON` names `legacy` and gives a reason.
///
///
#[must_use]
pub fn bypass_names(value: &str, legacy: &str) -> bool {
    value
        .split_once(':')
        .is_some_and(|(name, reason)| name.trim() == legacy && !reason.trim().is_empty())
}

/// Whether a bypass names the exact script path followed by `:` and a nonempty trimmed reason.
#[must_use]
pub fn script_bypass_names(value: &str, path: &str) -> bool {
    value
        .strip_prefix(path)
        .and_then(|suffix| suffix.strip_prefix(':'))
        .is_some_and(|reason| !reason.trim().is_empty())
}

/// Whether `arguments` make an invocation legacy, when the table maps only invocations with one of the `when` flags.
/// A short flag such as `-l` also matches inside a cluster such as `-lc`, and a long flag also matches with `=VALUE`.
///
#[must_use]
pub fn invokes(arguments: &[&str], when: &[&str]) -> bool {
    when.is_empty()
        || arguments.iter().any(|argument| {
            when.iter().any(|flag| match flag.strip_prefix("--") {
                Some(long) => argument.strip_prefix("--").is_some_and(|given| {
                    given == long
                        || given
                            .strip_prefix(long)
                            .is_some_and(|suffix| suffix.starts_with('='))
                }),
                None => flag.strip_prefix('-').is_some_and(|short| {
                    argument
                        .strip_prefix('-')
                        .filter(|cluster| !cluster.starts_with('-'))
                        .is_some_and(|cluster| cluster.contains(short))
                }),
            })
        })
}

/// Whether a mapped command reads only standard input: every argument names an option or `-`.
#[must_use]
pub fn reads_stream(arguments: &[&str]) -> bool {
    arguments.iter().all(|argument| argument.starts_with('-'))
}

/// A legacy invocation whose output the preferred tool repeats byte for byte.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Replacement {
    /// `cat FILE...` without options prints each file.
    Files,
    /// `sed -n 'Np' FILE` or `sed -n 'A,Bp' FILE` prints lines `first` through `last`.
    Lines { first: u64, last: u64 },
}

/// The exact replacement for the legacy command `name` with `arguments`, or `None` when the preferred tool could print anything else.
#[must_use]
pub fn replacement(name: &str, arguments: &[&str]) -> Option<Replacement> {
    match (name, arguments) {
        ("cat", [_, ..]) if arguments.iter().all(|argument| operand(argument)) => {
            Some(Replacement::Files)
        }
        ("sed", ["-n", script, file]) if operand(file) => {
            let range = script.strip_suffix('p')?;
            let (first, last) = range.split_once(',').unwrap_or((range, range));
            let (first, last) = (line(first)?, line(last)?);
            (first <= last).then_some(Replacement::Lines { first, last })
        }
        _ => None,
    }
}

/// A file operand, which no option or standard input stands in for.
fn operand(argument: &str) -> bool {
    !argument.is_empty() && !argument.starts_with('-')
}

/// A line number counted from one.
fn line(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok().filter(|number| *number > 0)
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    fn spec_child_suffix(selector: bool, verbose: bool, directory_flag: bool) {
        let flag = if directory_flag { "--cd" } else { "-D" };
        let child = ["cargo", "clippy", "--", flag];
        let (arguments, start): (&[&str], usize) = match (selector, verbose) {
            (false, false) => (&["x", "--", "cargo", "clippy", "--", flag], 2),
            (true, false) => (
                &["exec", "rust@1.99.0", "--", "cargo", "clippy", "--", flag],
                3,
            ),
            (false, true) => (&["--quiet", "x", "--", "cargo", "clippy", "--", flag], 3),
            (true, true) => (
                &[
                    "--verbose",
                    "exec",
                    "rust@1.99.0",
                    "--",
                    "cargo",
                    "clippy",
                    "--",
                    flag,
                ],
                4,
            ),
        };
        let decision = mise_prefix(arguments);
        assert_eq!(decision, CommandPrefix::Child { start });
        if let CommandPrefix::Child { start } = decision {
            assert!(start <= arguments.len());
            assert_eq!(arguments.get(start..), Some(child.as_slice()));
        }
        for kind in [WrapperKind::Env, WrapperKind::Sudo] {
            assert_eq!(
                wrapper_prefix(kind, &child),
                CommandPrefix::Child { start: 0 }
            );
        }
    }

    fn spec_mise_terminal(choice: u8) {
        let arguments: &[&str] = match choice % 8 {
            0 => &["ls"],
            1 => &["--quiet", "ls"],
            2 => &["-v", "install"],
            3 => &["--version"],
            4 => &["--help"],
            5 => &["--quiet", "run", "task", "--", "--cd", "child"],
            6 => &["--locked", "trust", "file"],
            _ => &[],
        };
        assert_eq!(mise_prefix(arguments), CommandPrefix::Terminal);
        for arguments in [
            &["x"][..],
            &["exec", "--"],
            &["x", "--cd", "dir", "--", "cargo"],
            &["run", "--cd", "dir", "task"],
            &["trust", "-Cdir", "file"],
            &["--unknown", "ls"],
            &["x", "", "--", "cargo"],
        ] {
            assert_eq!(mise_prefix(arguments), CommandPrefix::Unknown);
        }
    }

    fn spec_wrapper_ownership(choice: u8, sudo: bool) {
        let kind = if sudo {
            WrapperKind::Sudo
        } else {
            WrapperKind::Env
        };
        let arguments: &[&str] = match choice % 4 {
            0 => &["-C", "dir", "bash", "check.sh"],
            1 => &["--unknown", "bash", "check.sh"],
            2 => &["-u"],
            _ => &["-u", "-invalid", "bash", "check.sh"],
        };
        assert_eq!(wrapper_prefix(kind, arguments), CommandPrefix::Unknown);
        let outer = [
            "x",
            "--",
            if sudo { "sudo" } else { "env" },
            "-C",
            "dir",
            "bash",
            "check.sh",
        ];
        let decision = mise_prefix(&outer);
        assert_eq!(decision, CommandPrefix::Child { start: 2 });
        if let CommandPrefix::Child { start } = decision {
            let child = outer.get(start..).and_then(|child| child.split_first());
            assert!(child.is_some());
            if let Some((_, arguments)) = child {
                assert_eq!(wrapper_prefix(kind, arguments), CommandPrefix::Unknown);
            }
        }
        assert_eq!(
            wrapper_prefix::<&str>(kind, &[]),
            CommandPrefix::Child { start: 0 }
        );
        assert_eq!(
            wrapper_prefix(kind, &["--"]),
            CommandPrefix::Child { start: 1 }
        );
    }

    fn spec_startup_ownership(choice: u8, sudo: bool, separator: bool) {
        let assignment = match choice % 6 {
            0 => "BASH_ENV=reset.sh",
            1 => "ENV=reset.sh",
            2 => "ZDOTDIR=dir",
            3 => "PWD=dir",
            4 => "OLDPWD=dir",
            _ => "MODE=check",
        };
        let kind = if sudo {
            WrapperKind::Sudo
        } else {
            WrapperKind::Env
        };
        let arguments: &[&str] = if separator {
            &["--", assignment, "bash", "check.sh"]
        } else {
            &[assignment, "bash", "check.sh"]
        };
        assert_eq!(
            wrapper_prefix(kind, arguments),
            if choice % 6 < 5 {
                CommandPrefix::Unknown
            } else {
                CommandPrefix::Child {
                    start: usize::from(separator),
                }
            }
        );
        assert_eq!(
            wrapper_prefix(kind, &["-u", "BASH_ENV", "bash", "check.sh"]),
            CommandPrefix::Child { start: 2 }
        );
        assert_eq!(
            wrapper_prefix(kind, &["cargo", assignment]),
            CommandPrefix::Child { start: 0 }
        );
    }

    fn spec_environment_roles(choice: u8) {
        let (name, assignment, expected) = match choice % 8 {
            0 => ("BASH_ENV", "BASH_ENV=x", EnvironmentWord::Startup),
            1 => ("ENV", "ENV=x", EnvironmentWord::Startup),
            2 => ("ZDOTDIR", "ZDOTDIR=x", EnvironmentWord::Startup),
            3 => ("PWD", "PWD=x", EnvironmentWord::Startup),
            4 => ("OLDPWD", "OLDPWD=x", EnvironmentWord::Startup),
            5 => ("MODE", "MODE=x", EnvironmentWord::Assignment),
            6 => ("BAD-NAME", "BAD-NAME=x", EnvironmentWord::Other),
            _ => ("", "=x", EnvironmentWord::Other),
        };
        assert_eq!(
            environment_word(name, EnvironmentRole::Assignment),
            EnvironmentWord::Other
        );
        assert_eq!(
            environment_word(assignment, EnvironmentRole::Assignment),
            expected
        );
        assert_eq!(
            environment_word(name, EnvironmentRole::ExportOperand),
            if expected == EnvironmentWord::Startup {
                EnvironmentWord::Startup
            } else {
                EnvironmentWord::Other
            }
        );
        assert_eq!(
            environment_word("1=x", EnvironmentRole::Assignment),
            EnvironmentWord::Assignment
        );
    }

    #[test]
    fn wrapper_option_ownership_matches_the_finite_specifications() {
        for choice in 0..8 {
            spec_mise_terminal(choice);
            spec_environment_roles(choice);
            for [first, second, third] in crate::domain::flags() {
                spec_child_suffix(first, second, third);
                spec_wrapper_ownership(choice, first);
                spec_startup_ownership(choice, second, third);
            }
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(64)]
    fn child_arguments_remain_an_exact_bounded_suffix() {
        let [selector, verbose, directory_flag]: [bool; 3] = kani::any();
        spec_child_suffix(selector, verbose, directory_flag);
        kani::cover!(selector && verbose);
        kani::cover!(!selector && !verbose && !directory_flag);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(64)]
    fn mise_queries_and_non_exec_commands_remain_terminal() {
        let choice: u8 = kani::any();
        spec_mise_terminal(choice);
        kani::cover!(choice % 8 == 1);
        kani::cover!(choice % 8 == 3);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(64)]
    fn wrapper_owned_options_and_nested_effects_fail_closed() {
        let choice: u8 = kani::any();
        let sudo: bool = kani::any();
        spec_wrapper_ownership(choice, sudo);
        kani::cover!(sudo);
        kani::cover!(!sudo && choice % 4 == 2);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(64)]
    fn startup_assignments_keep_their_wrapper_ownership_after_separators() {
        let choice: u8 = kani::any();
        let [sudo, separator]: [bool; 2] = kani::any();
        spec_startup_ownership(choice, sudo, separator);
        kani::cover!(sudo && separator && choice % 6 == 0);
        kani::cover!(!sudo && !separator && choice % 6 == 5);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(64)]
    fn environment_roles_distinguish_assignments_from_export_operands() {
        let choice: u8 = kani::any();
        spec_environment_roles(choice);
        kani::cover!(choice % 8 == 0);
        kani::cover!(choice % 8 == 6);
    }

    fn spec_directory_context(left: Option<u8>, right: Option<u8>) {
        let joined = DirectoryContext::new(left).joined(&DirectoryContext::new(right));
        assert_eq!(
            joined.directory().copied(),
            if left == right { left } else { None }
        );
        assert!(
            DirectoryContext::new(left)
                .invalidate()
                .changed(right)
                .directory()
                .is_none()
        );
        assert!(
            DirectoryContext::new(left)
                .joined(&DirectoryContext::new(right).invalidate())
                .changed(left)
                .directory()
                .is_none()
        );
    }

    #[test]
    fn directory_flow_joins_and_binding_invalidation_preserve_uncertainty() {
        for left in [None, Some(0), Some(1)] {
            for right in [None, Some(0), Some(1)] {
                spec_directory_context(left, right);
            }
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(2)]
    fn directory_flow_never_invents_a_known_location_or_restores_an_unknown_binding() {
        let left: Option<u8> = kani::any();
        let right: Option<u8> = kani::any();
        spec_directory_context(left, right);
        kani::cover!(left.is_some() && left == right);
        kani::cover!(left != right);
        kani::cover!(left.is_none());
    }

    pub fn spec_spelling_order(previous: Option<usize>, current: usize) {
        assert_eq!(
            spelling_follows(previous, current),
            previous.is_none_or(|previous| previous < current)
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(2)]
    fn copied_dynamic_spellings_keep_strict_source_order() {
        let previous: Option<usize> = kani::any();
        let current: usize = kani::any();
        spec_spelling_order(previous, current);
        kani::cover!(previous.is_none());
        kani::cover!(previous.is_some() && spelling_follows(previous, current));
        kani::cover!(!spelling_follows(previous, current));
    }

    pub fn spec_spelling_span(
        start: usize,
        end: usize,
        length: usize,
        positioned: bool,
        boundaries: [bool; 2],
    ) {
        let result = spelling_span(start, end, length, positioned, boundaries);
        assert_eq!(
            matches!(result, SpellingSpan::InSource(_)),
            positioned && boundaries[0] && boundaries[1] && start < end && end <= length
        );
        if let SpellingSpan::InSource(range) = result {
            assert_eq!(range.bounds(), (start, end));
            assert!(range.start < range.end && range.end <= length);
        }
        assert_eq!(result == SpellingSpan::Unpositioned, !positioned);
        assert_eq!(result == SpellingSpan::Empty, positioned && start >= end);
        assert_eq!(
            result == SpellingSpan::Outside,
            positioned && start < end && end > length
        );
        assert_eq!(
            result == SpellingSpan::NonBoundary,
            positioned && start < end && end <= length && (!boundaries[0] || !boundaries[1])
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(2)]
    fn spelling_ranges_are_nonempty_and_inside_the_observed_utf8_source() {
        let (start, end, length, positioned, boundaries): (usize, usize, usize, bool, [bool; 2]) = (
            kani::any(),
            kani::any(),
            kani::any(),
            kani::any(),
            kani::any(),
        );
        spec_spelling_span(start, end, length, positioned, boundaries);
        let result = spelling_span(start, end, length, positioned, boundaries);
        kani::cover!(matches!(result, SpellingSpan::InSource(_)));
        kani::cover!(result == SpellingSpan::Unpositioned);
        kani::cover!(result == SpellingSpan::Empty);
        kani::cover!(result == SpellingSpan::Outside);
        kani::cover!(result == SpellingSpan::NonBoundary);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(8)]
    fn only_an_optionless_cat_or_a_line_print_has_a_replacement() {
        let kind: u8 = kani::any();
        kani::assume(kind < 8);
        let found = match kind {
            0 => replacement("cat", &["a"]),
            1 => replacement("sed", &["-n", "2,4p", "a"]),
            2 => replacement("sed", &["-n", "3p", "a"]),
            3 => replacement("cat", &["-n", "a"]),
            4 => replacement("cat", &[]),
            5 => replacement("sed", &["-n", "4,2p", "a"]),
            6 => replacement("sed", &["-n", "0p", "a"]),
            _ => replacement("sed", &["2,4p", "a"]),
        };
        assert_eq!(found.is_some(), kind < 3);
        kani::cover!(found == Some(Replacement::Files));
        kani::cover!(found == Some(Replacement::Lines { first: 2, last: 4 }));
        kani::cover!(found.is_none());
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(4)]
    fn a_stream_reader_names_no_file() {
        let kind: u8 = kani::any();
        kani::assume(kind < 4);
        let stream = match kind {
            0 => reads_stream(&[]),
            1 => reads_stream(&["-l"]),
            2 => reads_stream(&["-l", "-"]),
            _ => reads_stream(&["-l", "a"]),
        };
        assert_eq!(stream, kind < 3);
        kani::cover!(stream);
        kani::cover!(!stream);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(16)]
    fn a_bypass_needs_the_named_entry_and_a_nonempty_reason() {
        let same: bool = kani::any();
        let reason: bool = kani::any();
        let matched = match (same, reason) {
            (true, true) => bypass_names("grep: output", "grep"),
            (true, false) => bypass_names("grep: ", "grep"),
            (false, true) => bypass_names("find: output", "grep"),
            (false, false) => bypass_names("find: ", "grep"),
        };
        assert_eq!(matched, same && reason);
        kani::cover!(matched);
        kani::cover!(!matched);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(32)]
    fn a_script_bypass_needs_the_exact_path_and_a_nonempty_reason() {
        let kind: u8 = kani::any();
        kani::assume(kind < 8);
        let (value, path) = match kind {
            0 => ("./x.sh: upstream", "./x.sh"),
            1 => ("C:\\x.sh: upstream", "C:\\x.sh"),
            2 => ("./x.sh: \u{2003}upstream\u{2003}", "./x.sh"),
            3 => ("./y.sh: upstream", "./x.sh"),
            4 => ("./x.sh: \u{2003}", "./x.sh"),
            5 => ("grep: upstream", "./x.sh"),
            6 => ("./x.shx: upstream", "./x.sh"),
            _ => ("./x.sh upstream", "./x.sh"),
        };
        let matched = script_bypass_names(value, path);
        assert_eq!(matched, kind < 3);
        kani::cover!(matched);
        kani::cover!(!matched);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(16)]
    fn conditional_entries_match_the_exact_flag_or_its_value() {
        let kind: u8 = kani::any();
        kani::assume(kind < 4);
        let matches = match kind {
            0 => invokes(&["--lines"], &["--lines", "-l"]),
            1 => invokes(&["--lines=1"], &["--lines", "-l"]),
            2 => invokes(&["-lc"], &["--lines", "-l"]),
            _ => invokes(&["--linesx"], &["--lines", "-l"]),
        };
        assert_eq!(matches, kind < 3);
        kani::cover!(matches);
        kani::cover!(!matches);
    }

    /// The hook refuses a command the table maps unless a listed exception runs it or a recorded bypass names it, and it refuses nothing the table leaves out.
    pub fn spec_verdict(mapped: bool, excepted: bool, bypassed: bool) {
        let result = verdict(mapped, excepted, bypassed);
        assert_eq!(result == Verdict::Refuse, mapped && !excepted && !bypassed);
        assert_eq!(result == Verdict::Bypassed, mapped && !excepted && bypassed);
        assert_eq!(result == Verdict::Admit, !mapped || excepted);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_mapped_legacy_command_runs_only_through_an_exception_or_a_recorded_bypass() {
        let (mapped, excepted, bypassed): (bool, bool, bool) =
            (kani::any(), kani::any(), kani::any());
        spec_verdict(mapped, excepted, bypassed);
        let result = verdict(mapped, excepted, bypassed);
        kani::cover!(result == Verdict::Refuse);
        kani::cover!(result == Verdict::Bypassed);
        kani::cover!((result == Verdict::Admit) & mapped);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spelling_order_covers_first_duplicate_reversed_and_integer_boundaries() {
        for previous in [None, Some(0), Some(1), Some(usize::MAX)] {
            for current in [0, 1, usize::MAX] {
                proofs::spec_spelling_order(previous, current);
            }
        }
    }

    #[test]
    fn source_ranges_cover_invalid_positions_and_integer_boundaries() {
        for start in [0, 1, 2, usize::MAX] {
            for end in [0, 1, 2, usize::MAX] {
                for length in [0, 1, 2, usize::MAX] {
                    for [positioned, first, last] in crate::domain::flags() {
                        proofs::spec_spelling_span(start, end, length, positioned, [first, last]);
                    }
                }
            }
        }
    }

    #[test]
    fn every_input_meets_the_verdict_specification() {
        for [mapped, excepted, bypassed] in crate::domain::flags() {
            proofs::spec_verdict(mapped, excepted, bypassed);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "text matching is exercised by the native rules tests")]
    fn a_bypass_names_exactly_one_entry_and_a_reason() {
        for value in [
            "grep: output",
            " grep : GNU output",
            "grep: a:b",
            "\u{2003}grep\u{2003}: reason",
        ] {
            assert!(bypass_names(value, "grep"), "{value}");
        }
        for value in [
            "grep",
            "grep:",
            "grep:  ",
            "rg: output",
            "grepx: output",
            "grep: \u{2003}",
        ] {
            assert!(!bypass_names(value, "grep"), "{value}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "text matching is exercised by the native rules tests")]
    fn a_script_bypass_names_the_exact_path_including_drive_colons() {
        for (value, path) in [
            ("./x.sh: upstream", "./x.sh"),
            ("/tmp/x.sh: pinned: upstream", "/tmp/x.sh"),
            ("a b.sh:\u{2003}upstream\u{2003}", "a b.sh"),
            ("C:\\tmp\\x.sh: upstream", "C:\\tmp\\x.sh"),
        ] {
            assert!(script_bypass_names(value, path), "{value}");
        }
        for value in [
            "./y.sh: upstream",
            "x.sh: upstream",
            " ./x.sh: upstream",
            "./x.sh : upstream",
            "./x.shx: upstream",
            "./x.sh",
            "./x.sh:",
            "./x.sh: \t\n\u{2003}",
            "grep: upstream",
        ] {
            assert!(!script_bypass_names(value, "./x.sh"), "{value}");
        }
        assert!(!script_bypass_names("C: upstream", "C:\\tmp\\x.sh"));
        assert!(!script_bypass_names(
            "C:\\tmp\\y.sh: upstream",
            "C:\\tmp\\x.sh"
        ));
    }

    #[test]
    #[cfg_attr(miri, ignore = "text matching is exercised by the native rules tests")]
    fn conditional_flags_match_clusters_and_values_without_prefix_collisions() {
        assert!(invokes(&["file"], &[]));
        for argument in ["--lines", "--lines=1", "-l", "-lc"] {
            assert!(invokes(&[argument], &["--lines", "-l"]), "{argument}");
        }
        for argument in ["file", "--linesx", "--l", "-c", "--lc"] {
            assert!(!invokes(&[argument], &["--lines", "-l"]), "{argument}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "text matching is exercised by the native rules tests")]
    fn only_exact_invocations_have_a_replacement() {
        assert_eq!(replacement("cat", &["a", "b c"]), Some(Replacement::Files));
        assert_eq!(
            replacement("sed", &["-n", "12,40p", "a"]),
            Some(Replacement::Lines {
                first: 12,
                last: 40
            })
        );
        assert_eq!(
            replacement("sed", &["-n", "7p", "a"]),
            Some(Replacement::Lines { first: 7, last: 7 })
        );
        for (name, arguments) in [
            ("cat", &[][..]),
            ("cat", &["-n", "a"]),
            ("cat", &["a", "-"]),
            ("sed", &["-n", "2,4p"]),
            ("sed", &["-n", "2,4p", "a", "b"]),
            ("sed", &["2,4p", "a"]),
            ("sed", &["-n", "2,4d", "a"]),
            ("sed", &["-n", "4,2p", "a"]),
            ("sed", &["-n", "0,2p", "a"]),
            ("sed", &["-n", "$p", "a"]),
            ("sed", &["-n", "1,+3p", "a"]),
            ("sed", &["-n", "2,4p", "-"]),
            ("ls", &["a"]),
        ] {
            assert_eq!(replacement(name, arguments), None, "{name} {arguments:?}");
        }
    }

    #[test]
    fn a_stream_reader_names_only_options() {
        assert!(reads_stream(&[]));
        assert!(reads_stream(&["-l"]));
        assert!(reads_stream(&["-l", "-"]));
        assert!(!reads_stream(&["-l", "file"]));
    }
}

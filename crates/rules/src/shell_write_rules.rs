extern crate alloc;
use self::alloc::borrow::ToOwned;
use self::alloc::string::String;
use self::alloc::vec::Vec;

/// How a shell command writes a file when it should have used a file tool or a project command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reason {
    /// An inline interpreter runs code from `-c`, `-e`, or standard input.
    InlineInterpreter,
    /// An in-place editor flag or an awk program statement writes a named file.
    InPlaceEdit,
    /// A redirect, `tee`, or heredoc target names a file.
    FileRedirect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Admit,
    Refuse(Reason),
}

#[must_use]
pub fn verdict(inline_interpreter: bool, in_place: bool, file_write: bool) -> Verdict {
    if inline_interpreter {
        Verdict::Refuse(Reason::InlineInterpreter)
    } else if in_place {
        Verdict::Refuse(Reason::InPlaceEdit)
    } else if file_write {
        Verdict::Refuse(Reason::FileRedirect)
    } else {
        Verdict::Admit
    }
}

#[must_use]
pub const fn next_action(reason: Reason) -> &'static str {
    match reason {
        Reason::InlineInterpreter => {
            "Use the Edit or Write tool to change files, or add the logic as a project command (`just`, `xtask`, `pr-workflow`) and run that."
        }
        Reason::InPlaceEdit => {
            "Use the Edit tool to change the file, or run a project command (`just`, `xtask`, `pr-workflow`) that applies the change."
        }
        Reason::FileRedirect => {
            "Use the Write tool for new files and the Edit tool for changes, or run a project command (`just`, `xtask`, `pr-workflow`) that produces the file."
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
enum Token {
    Word { text: String, plain: bool },
    Separator,
    Redirect,
}

#[derive(Default)]
struct Scan {
    tokens: Vec<Token>,
    heredoc: bool,
    /// The text of every heredoc body, in order.
    heredocs: Vec<String>,
}

fn flush(word: &mut String, plain: &mut bool, started: &mut bool, tokens: &mut Vec<Token>) {
    if *started {
        tokens.push(Token::Word {
            text: core::mem::take(word),
            plain: *plain,
        });
    }
    *plain = true;
    *started = false;
}

fn skip_heredoc_body(
    chars: &[char],
    mut index: usize,
    delimiters: &mut Vec<String>,
    bodies: &mut Vec<String>,
) -> usize {
    for delimiter in delimiters.drain(..) {
        let mut body = String::new();
        loop {
            let end = chars
                .get(index..)
                .unwrap_or_default()
                .iter()
                .position(|c| *c == '\n')
                .map_or(chars.len(), |offset| index + offset);
            let line: String = chars.get(index..end).unwrap_or_default().iter().collect();
            index = (end + 1).min(chars.len());
            if line.trim_start_matches('\t') == delimiter {
                break;
            }
            body.push_str(&line);
            body.push('\n');
            if end == chars.len() {
                break;
            }
        }
        bodies.push(body);
    }
    index
}

/// The unquoted delimiter of a here-document whose `<<` ends just before `index`, and the index after it.
fn heredoc_delimiter(chars: &[char], mut index: usize) -> (String, usize) {
    if chars.get(index) == Some(&'-') {
        index += 1;
    }
    while matches!(chars.get(index), Some(' ' | '\t')) {
        index += 1;
    }
    let mut delimiter = String::new();
    while let Some(d) = chars.get(index) {
        if d.is_whitespace() || matches!(d, ';' | '|' | '&' | '(' | ')' | '<' | '>') {
            break;
        }
        if !matches!(d, '\'' | '"' | '\\') {
            delimiter.push(*d);
        }
        index += 1;
    }
    (delimiter, index)
}

fn scan(command: &str) -> Scan {
    let chars: Vec<char> = command.chars().collect();
    let mut scan = Scan::default();
    let mut word = String::new();
    let mut plain = true;
    let mut started = false;
    let mut delimiters: Vec<String> = Vec::new();
    let mut index = 0;
    while let Some(&c) = chars.get(index) {
        index += 1;
        match c {
            '\'' => {
                started = true;
                plain = false;
                while let Some(&quoted) = chars.get(index).filter(|&&quoted| quoted != '\'') {
                    word.push(quoted);
                    index += 1;
                }
                index += 1;
            }
            '"' => {
                started = true;
                plain = false;
                while let Some(&quoted) = chars.get(index).filter(|&&quoted| quoted != '"') {
                    let escaped = if quoted == '\\' {
                        chars.get(index + 1).copied()
                    } else {
                        None
                    };
                    if let Some(next) = escaped {
                        index += 1;
                        word.push(next);
                    } else {
                        word.push(quoted);
                    }
                    index += 1;
                }
                index += 1;
            }
            '\\' => {
                started = true;
                if let Some(&next) = chars.get(index) {
                    word.push(next);
                    index += 1;
                }
            }
            ' ' | '\t' | '\r' => flush(&mut word, &mut plain, &mut started, &mut scan.tokens),
            '\n' => {
                flush(&mut word, &mut plain, &mut started, &mut scan.tokens);
                scan.tokens.push(Token::Separator);
                index = skip_heredoc_body(&chars, index, &mut delimiters, &mut scan.heredocs);
            }
            '&' if chars.get(index) == Some(&'>') => {
                flush(&mut word, &mut plain, &mut started, &mut scan.tokens);
                index += 1;
                if chars.get(index) == Some(&'>') {
                    index += 1;
                }
                scan.tokens.push(Token::Redirect);
            }
            ';' | '|' | '(' | ')' | '{' | '}' | '&' if separates(c, &chars, index, started) => {
                flush(&mut word, &mut plain, &mut started, &mut scan.tokens);
                scan.tokens.push(Token::Separator);
            }
            '>' => {
                if started && word.chars().all(|d| d.is_ascii_digit()) {
                    word.clear();
                    started = false;
                    plain = true;
                }
                flush(&mut word, &mut plain, &mut started, &mut scan.tokens);
                if chars.get(index) == Some(&'&') {
                    index += 1;
                    continue;
                }
                if matches!(chars.get(index), Some('>' | '|')) {
                    index += 1;
                }
                scan.tokens.push(Token::Redirect);
            }
            '<' if chars.get(index) == Some(&'<') && chars.get(index + 1) != Some(&'<') => {
                flush(&mut word, &mut plain, &mut started, &mut scan.tokens);
                let (delimiter, next) = heredoc_delimiter(&chars, index + 1);
                index = next;
                scan.heredoc = true;
                delimiters.push(delimiter);
            }
            other => {
                started = true;
                word.push(other);
            }
        }
    }
    flush(&mut word, &mut plain, &mut started, &mut scan.tokens);
    scan
}

/// A brace separates commands only as a whole word, as in `{ cmd; }`, never inside one, as in `${name}` or `HEAD^{tree}`.
fn separates(c: char, chars: &[char], index: usize, started: bool) -> bool {
    !matches!(c, '{' | '}')
        || !started
            && chars
                .get(index)
                .is_none_or(|next| next.is_whitespace() || matches!(next, ';' | '|' | '&' | ')'))
}

/// The words of each simple command in `command`, in order, each up to its first redirect.
///
/// Exempt as disproportionate: a shell parser over unbounded text needs fuzzing, which no verifier here owns yet.
/// Evidence: `crates/rules/src/shell_write_rules.rs::tests::shell_segments_preserve_literal_words_and_nested_scripts`.
/// Reconsider when a cargo-fuzz target owns this function.
#[must_use]
pub fn segments(command: &str) -> Vec<Vec<String>> {
    script_segments(command, 0)
}

fn script_segments(command: &str, depth: usize) -> Vec<Vec<String>> {
    let scan = scan(command);
    let mut result: Vec<Vec<String>> = scan
        .tokens
        .split(|token| *token == Token::Separator)
        .map(|segment| {
            segment_words(segment)
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|words| !words.is_empty())
        .collect();
    let shell = result.iter().any(|words| {
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        command_words(&words)
            .first()
            .is_some_and(|first| SCRIPT_SHELLS.contains(&program(first).as_str()))
    });
    if shell && depth < SCRIPT_DEPTH {
        for body in scan.heredocs {
            result.extend(script_segments(&body, depth + 1));
        }
    }
    result
}

/// The program a command word names: its filename, folded to small letters, without `.exe` or a version suffix.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Program(String);

impl Program {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PartialEq<&str> for Program {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

/// The program that the command word `text` names.
///
/// Exempt as disproportionate: even a two-byte normalization domain composes reverse delimiter search, UTF-8 suffix decoding, and allocated case folding.
/// The exhaustive enumeration covers four-byte words over a four-byte alphabet.
/// Evidence: `crates/rules/src/shell_write_rules.rs::tests::program_names_enumerate_a_small_alphabet_through_four_bytes`.
/// Reconsider when normalization exposes allocation-free byte boundaries before constructing its output.
#[must_use]
pub fn program(text: &str) -> Program {
    let name = text.rsplit(['/', '\\']).next().unwrap_or(text);
    let lower = name.to_ascii_lowercase();
    let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
    Program(
        stem.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.')
            .to_owned(),
    )
}

fn is_assignment(text: &str) -> bool {
    text.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

fn runs_inline(name: &str, arguments: &[&str], heredoc: bool) -> bool {
    if !matches!(name, "python" | "py" | "node" | "nodejs" | "perl" | "ruby") {
        return false;
    }
    let mut script = false;
    for argument in arguments {
        let flags = argument.strip_prefix('-');
        let Some(flags) = flags.filter(|f| !f.is_empty()) else {
            script = *argument != "-";
            if *argument == "-" {
                return true;
            }
            break;
        };
        let long = flags.starts_with('-');
        let eval = match name {
            "python" | "py" => !long && flags.contains('c'),
            "node" | "nodejs" => matches!(*argument, "-e" | "-p" | "--eval" | "--print"),
            _ => !long && (flags.contains('e') || flags.contains('E')),
        };
        if eval {
            return true;
        }
        if matches!(*argument, "-V" | "--version" | "-h" | "--help" | "-v") {
            script = true;
        }
    }
    heredoc || !script
}

/// The device files a write may target without writing a file.
const DISCARD: &str = "/dev/null";
const STDOUT: &str = "/dev/stdout";
const STDERR: &str = "/dev/stderr";

/// Whether the flags of `sed` or `perl` request in-place edits, or `awk` loads the `inplace` extension.
///
/// Exempt as disproportionate: sixteen total bytes across a name and two arguments compose UTF-8 flag searches, long options, and cross-argument lookahead.
/// The exhaustive argument matrix covers five names, seven first arguments, and four second arguments.
/// Evidence: `crates/rules/src/shell_write_rules.rs::tests::in_place_flags_enumerate_every_short_argument_pair`.
/// Reconsider when a byte classifier with its own contract handles individual arguments before lookahead.
#[must_use]
pub fn in_place_flag(name: &str, arguments: &[&str]) -> bool {
    let editor = matches!(name, "sed" | "perl");
    let awk = matches!(name, "awk" | "gawk");
    if !editor && !awk {
        return false;
    }
    for (index, argument) in arguments.iter().enumerate() {
        if argument == &"--" {
            return false;
        }
        let Some(flags) = argument.strip_prefix('-').filter(|f| !f.is_empty()) else {
            continue;
        };
        let long = flags.starts_with('-');
        let found = if editor {
            long && (flags == "-in-place" || flags.starts_with("-in-place="))
                || !long && flags.contains('i')
        } else if long {
            false
        } else {
            (flags == "i"
                && arguments
                    .get(index + 1)
                    .is_some_and(|word| *word == "inplace"))
                || flags == "iinplace"
        };
        if found {
            return true;
        }
    }
    false
}

/// Whether an awk program text redirects a `print` or `printf` statement into a file.
///
/// A `>` in a print statement always names a redirect target under the awk grammar, never a comparison, and `>>` only appends.
///
/// Exempt as disproportionate: eight input bytes compose up to nine statement partitions with UTF-8 trimming, quoting, and escape state.
/// The exhaustive enumeration covers three-byte print suffixes over a four-byte alphabet.
/// Evidence: `crates/rules/src/shell_write_rules.rs::tests::awk_redirects_enumerate_every_short_print_suffix`.
/// Reconsider when quote and escape state has a byte-transition contract separate from statement partitioning.
#[must_use]
pub fn awk_program_writes(program: &str) -> bool {
    program
        .split([';', '{', '}', '\n'])
        .filter(|statement| {
            let body = statement.trim_start();
            body.starts_with("print ")
                || body.starts_with("print(")
                || body.starts_with("printf ")
                || body.starts_with("printf(")
        })
        .any(statement_redirects)
}

/// Whether a print statement holds a `>` or `>>` outside its string literals.
fn statement_redirects(statement: &str) -> bool {
    let mut quoted = false;
    let mut escaped = false;
    let mut chars = statement.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                quoted = false;
            }
            continue;
        }
        match c {
            '"' => quoted = true,
            '>' => {
                if chars.peek() != Some(&'=') {
                    return true;
                }
                chars.next();
            }
            _ => {}
        }
    }
    false
}

fn writes_to(target: Option<&Token>) -> bool {
    match target {
        Some(Token::Word { text, .. }) => !matches!(text.as_str(), DISCARD | STDOUT | STDERR),
        _ => false,
    }
}

/// The words of one simple command, up to its first redirect.
fn segment_words(segment: &[Token]) -> Vec<&str> {
    segment
        .iter()
        .take_while(|token| **token != Token::Redirect)
        .filter_map(|token| match token {
            Token::Word { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// The two detections behind the verdict.
/// They cover an inline interpreter and a file write.
///
/// Exempt as disproportionate: a shell parser over unbounded text needs fuzzing, which no verifier here owns yet.
/// Evidence: `crates/rules/src/shell_write_rules.rs::tests::inline_interpreters_are_refused`.
/// Reconsider when a cargo-fuzz target owns this function.
#[must_use]
pub fn inspect(command: &str) -> Verdict {
    inspect_at(command, 0, &[])
}

/// Like [`inspect`], except that a redirect or `tee` target that `ignored` names writes nothing a commit can carry.
///
/// Exempt as disproportionate: a shell parser over unbounded text needs fuzzing, which no verifier here owns yet.
/// Evidence: `crates/rules/src/shell_write_rules.rs::tests::writes_into_ignored_targets_are_admitted`.
/// Reconsider when a cargo-fuzz target owns this function.
#[must_use]
pub fn inspect_except(command: &str, ignored: &[&str]) -> Verdict {
    inspect_at(command, 0, ignored)
}

/// Every redirect and `tee` target at each depth that [`inspect`] reads, so the caller can ask the checkout which ones it ignores.
///
/// Exempt as disproportionate: a shell parser over unbounded text needs fuzzing, which no verifier here owns yet.
/// Evidence: `crates/rules/src/shell_write_rules.rs::tests::writes_into_ignored_targets_are_admitted`.
/// Reconsider when a cargo-fuzz target owns this function.
#[must_use]
pub fn write_targets(command: &str) -> Vec<Target> {
    let mut targets = Vec::new();
    targets_at(command, 0, &mut targets);
    targets
}

/// A file that a redirect or `tee` names, spelled as the command line spells it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Target(String);

impl Target {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PartialEq<&str> for Target {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

fn targets_at(command: &str, depth: usize, targets: &mut Vec<Target>) {
    let scan = scan(command);
    for (index, token) in scan.tokens.iter().enumerate() {
        if *token == Token::Redirect
            && writes_to(scan.tokens.get(index + 1))
            && let Some(Token::Word { text, .. }) = scan.tokens.get(index + 1)
        {
            targets.push(Target(text.clone()));
        }
    }
    for segment in scan.tokens.split(|token| *token == Token::Separator) {
        let words = segment_words(segment);
        let command = command_words(&words);
        let Some((first, rest)) = command.split_first() else {
            continue;
        };
        let name = program(first);
        if name == "tee" {
            targets.extend(
                rest.iter()
                    .filter(|a| !a.starts_with('-') && !matches!(**a, DISCARD | STDOUT))
                    .map(|a| Target((*a).to_owned())),
            );
        }
        if SCRIPT_SHELLS.contains(&name.as_str()) && depth < SCRIPT_DEPTH {
            let flag = rest.iter().position(|word| {
                word.starts_with('-') && !word.starts_with("--") && word.contains('c')
            });
            match flag {
                Some(flag) => {
                    if let Some(script) = rest.get(flag + 1) {
                        targets_at(script, depth + 1, targets);
                    }
                }
                None => {
                    for body in &scan.heredocs {
                        targets_at(body, depth + 1, targets);
                    }
                }
            }
        }
    }
}

/// Inspect recursive shell scripts through this many nested wrappers.
const SCRIPT_DEPTH: usize = 4;
const SCRIPT_SHELLS: [&str; 5] = ["sh", "bash", "zsh", "dash", "ksh"];

fn command_words<'a>(mut words: &'a [&'a str]) -> &'a [&'a str] {
    loop {
        words = match words {
            [word, rest @ ..]
                if is_assignment(word)
                    || matches!(
                        program(word).as_str(),
                        "env" | "command" | "exec" | "time" | "nohup" | "sudo"
                    ) =>
            {
                rest
            }
            [word, "x" | "exec", "--", rest @ ..] | [word, "x" | "exec", rest @ ..]
                if program(word) == "mise" =>
            {
                rest
            }
            _ => return words,
        };
    }
}

fn inspect_at(command: &str, depth: usize, ignored: &[&str]) -> Verdict {
    let scan = scan(command);
    let mut interpreter = false;
    let mut in_place = false;
    let mut file_write = false;
    for (index, token) in scan.tokens.iter().enumerate() {
        let target = scan.tokens.get(index + 1);
        if *token == Token::Redirect
            && writes_to(target)
            && !matches!(target, Some(Token::Word { text, .. }) if ignored.contains(&text.as_str()))
        {
            file_write = true;
        }
    }
    for segment in scan.tokens.split(|token| *token == Token::Separator) {
        let words = segment_words(segment);
        let command = command_words(&words);
        let Some((first, rest)) = command.split_first() else {
            continue;
        };
        let name = program(first);
        if SCRIPT_SHELLS.contains(&name.as_str()) {
            let flag = rest.iter().position(|word| {
                word.starts_with('-') && !word.starts_with("--") && word.contains('c')
            });
            let scripts: Vec<&str> = match flag {
                Some(flag) => rest.get(flag + 1).copied().into_iter().collect(),
                None => scan.heredocs.iter().map(String::as_str).collect(),
            };
            if !scripts.is_empty() && depth >= SCRIPT_DEPTH {
                interpreter = true;
            } else {
                for script in scripts {
                    let found = inspect_at(script, depth + 1, ignored);
                    if matches!(found, Verdict::Refuse(_)) {
                        return found;
                    }
                }
            }
        }
        if runs_inline(name.as_str(), rest, scan.heredoc) {
            interpreter = true;
        }
        if in_place_flag(name.as_str(), rest) {
            in_place = true;
        }
        if matches!(name.as_str(), "awk" | "gawk" | "mawk" | "nawk")
            && rest.iter().any(|argument| awk_program_writes(argument))
        {
            in_place = true;
        }
        if name == "xargs"
            && let Some(position) = rest.iter().position(|word| !word.starts_with('-'))
            && rest
                .get(position..)
                .and_then(|words| words.split_first())
                .is_some_and(|(word, arguments)| in_place_flag(program(word).as_str(), arguments))
        {
            in_place = true;
        }
        if name == "find"
            && let Some(marker) = rest
                .iter()
                .position(|word| matches!(*word, "-exec" | "-execdir" | "-ok" | "-okdir"))
            && rest.get(marker..).is_some_and(|words| {
                let [_, editor, arguments @ ..] = words else {
                    return false;
                };
                in_place_flag(program(editor).as_str(), arguments)
            })
        {
            in_place = true;
        }
        if name == "tee"
            && rest.iter().any(|a| {
                !a.starts_with('-') && !matches!(*a, DISCARD | STDOUT) && !ignored.contains(a)
            })
        {
            file_write = true;
        }
    }
    verdict(interpreter, in_place, file_write)
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    #[cfg(test)]
    pub fn spec_program(text: &str) -> bool {
        let bytes = text.as_bytes();
        let start = bytes
            .iter()
            .rposition(|byte| matches!(byte, b'/' | b'\\'))
            .map_or(0, |position| position + 1);
        let mut end = bytes.len();
        if end - start >= 4
            && bytes
                .get(end - 4..)
                .is_some_and(|suffix| suffix.eq_ignore_ascii_case(b".exe"))
        {
            end -= 4;
        }
        while end > start
            && bytes
                .get(end - 1)
                .is_some_and(|byte| byte.is_ascii_digit() || *byte == b'.')
        {
            end -= 1;
        }
        let actual = program(text);
        assert_eq!(actual.as_str().len(), end - start);
        for (index, byte) in actual.as_str().bytes().enumerate() {
            assert_eq!(
                Some(byte),
                bytes.get(start + index).map(u8::to_ascii_lowercase)
            );
        }
        actual.as_str() != text
    }

    #[cfg(test)]
    pub fn spec_in_place_flag(name: &str, arguments: &[&str]) -> bool {
        let editor = matches!(name.as_bytes(), b"sed" | b"perl");
        let awk = matches!(name.as_bytes(), b"awk" | b"gawk");
        let end = arguments
            .iter()
            .position(|word| *word == "--")
            .unwrap_or(arguments.len());
        let expected =
            arguments
                .iter()
                .take(end)
                .enumerate()
                .any(|(index, word)| match word.as_bytes() {
                    [b'-', b'-', flags @ ..] => {
                        editor && (flags == b"in-place" || flags.starts_with(b"in-place="))
                    }
                    [b'-', flags @ ..] if !flags.is_empty() => {
                        editor && flags.contains(&b'i')
                            || awk
                                && (flags == b"iinplace"
                                    || flags == b"i"
                                        && arguments.get(index + 1) == Some(&"inplace"))
                    }
                    _ => false,
                });
        let actual = in_place_flag(name, arguments);
        assert_eq!(actual, expected);
        actual
    }

    #[cfg(test)]
    fn reference_awk_redirect(statement: &str) -> bool {
        let bytes = statement.trim_start().as_bytes();
        if !(bytes.starts_with(b"print ")
            || bytes.starts_with(b"print(")
            || bytes.starts_with(b"printf ")
            || bytes.starts_with(b"printf("))
        {
            return false;
        }
        let mut quoted = false;
        let mut escaped = false;
        for (index, &byte) in bytes.iter().enumerate() {
            if escaped {
                escaped = false;
            } else if quoted && byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = !quoted;
            } else if !quoted && byte == b'>' && bytes.get(index + 1) != Some(&b'=') {
                return true;
            }
        }
        false
    }

    #[cfg(test)]
    pub fn spec_awk_program(program: &str) -> bool {
        let expected = program
            .split([';', '{', '}', '\n'])
            .any(reference_awk_redirect);
        let actual = awk_program_writes(program);
        assert_eq!(actual, expected);
        actual
    }

    /// The verdict never admits a file-writing command, the refusal reasons keep their precedence, and each refusal names a file tool.
    pub fn spec_verdict(interpreter: bool, in_place: bool, file: bool) {
        let result = verdict(interpreter, in_place, file);
        let expected = if interpreter {
            Verdict::Refuse(Reason::InlineInterpreter)
        } else if in_place {
            Verdict::Refuse(Reason::InPlaceEdit)
        } else if file {
            Verdict::Refuse(Reason::FileRedirect)
        } else {
            Verdict::Admit
        };
        assert_eq!(result, expected);
        if let Verdict::Refuse(reason) = result {
            assert!(next_action(reason).len() > 1);
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    fn file_writing_commands_are_never_admitted_and_every_refusal_names_a_next_action() {
        let interpreter: bool = kani::any();
        let in_place: bool = kani::any();
        let file: bool = kani::any();
        spec_verdict(interpreter, in_place, file);
        let result = verdict(interpreter, in_place, file);
        kani::cover!(result == Verdict::Admit);
        kani::cover!(result == Verdict::Refuse(Reason::InlineInterpreter));
        kani::cover!(result == Verdict::Refuse(Reason::InPlaceEdit));
        kani::cover!(result == Verdict::Refuse(Reason::FileRedirect));
    }
}

#[cfg(test)]
mod tests {

    #[cfg(miri)]
    #[test]
    fn examples_share_one_miri_test() {
        every_input_meets_the_verdict_specification();
        redirects_into_files_are_refused();
        standard_input_bodies_include_heredocs_and_piped_words();
        commands_that_send_no_body_have_no_sources();
        a_program_name_drops_its_directory_case_and_suffixes();
        refusals_name_a_next_action();
        the_scanner_reads_quotes_escapes_and_separators_into_tokens();
        the_scanner_tells_each_redirect_from_a_descriptor_copy();
        a_heredoc_body_ends_at_its_delimiter_and_stays_out_of_the_words();
        segments_drop_empty_commands_and_stop_at_a_redirect();
        writes_into_ignored_targets_are_admitted();
        an_assignment_names_a_variable_before_its_equals_sign();
        each_interpreter_runs_inline_only_through_its_own_flags();
        a_command_prefix_is_skipped_only_for_mise_and_the_wrappers();
        api_and_command_bodies_come_from_every_flag_form();
        standard_input_takes_the_words_of_every_other_command();
    }
    #[cfg_attr(not(miri), test)]
    fn every_input_meets_the_verdict_specification() {
        for [interpreter, in_place, file] in crate::domain::flags() {
            super::proofs::spec_verdict(interpreter, in_place, file);
        }
        assert!(super::next_action(super::Reason::InlineInterpreter).starts_with("Use the Edit"));
        assert!(super::next_action(super::Reason::InPlaceEdit).starts_with("Use the Edit"));
        assert!(super::next_action(super::Reason::FileRedirect).starts_with("Use the Write"));
    }

    use super::*;

    #[test]
    fn program_names_enumerate_a_small_alphabet_through_four_bytes() {
        for bytes in crate::domain::words(b"A./1", 4) {
            proofs::spec_program(core::str::from_utf8(&bytes).unwrap());
        }
    }

    #[test]
    fn awk_redirects_enumerate_every_short_print_suffix() {
        for bytes in crate::domain::words(b" >\"=", 3) {
            let suffix = core::str::from_utf8(&bytes).unwrap();
            proofs::spec_awk_program(&alloc::format!("print{suffix}"));
        }
    }

    #[test]
    fn in_place_flags_enumerate_every_short_argument_pair() {
        for name in ["sed", "perl", "awk", "gawk", "other"] {
            for first in ["", "--", "-i", "-pi", "--in-place", "-iinplace", "inplace"] {
                for second in ["", "--", "-i", "inplace"] {
                    proofs::spec_in_place_flag(name, &[first, second]);
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "nested shell regressions allocate parsed command text")]
    fn nested_shells_and_heredocs_cannot_hide_file_writes() {
        assert_eq!(
            segments("sh <<'EOF'\nrg x .\nEOF"),
            [vec!["sh"], vec!["rg", "x", "."]]
        );
        assert_eq!(segments("cat <<'EOF'\nrg x .\nEOF"), [vec!["cat"]]);
        let mut body = String::from("rg x .");
        for depth in 0..=super::SCRIPT_DEPTH {
            body = alloc::format!("sh <<'EOF{depth}'\n{body}\nEOF{depth}");
        }
        let parsed = segments(&body);
        assert_eq!(parsed.len(), super::SCRIPT_DEPTH + 1);
        assert!(
            !parsed
                .iter()
                .any(|words| words.first().is_some_and(|word| word == "rg"))
        );
        assert_eq!(inspect(&body), REFUSED_INTERPRETER);
        let mut body = String::from("sh script.sh");
        for depth in 0..super::SCRIPT_DEPTH {
            body = alloc::format!("sh <<'EOF{depth}'\n{body}\nEOF{depth}");
        }
        assert_eq!(inspect(&body), Verdict::Admit);
        for command in [
            "bash -c \"cat > notes.md <<'EOF'\nbody\nEOF\"",
            "sh <<'EOF'\nsed -i s/a/b/ f\nEOF",
            "bash -c 'awk \"{print > f}\" input'",
            "sh -c 'sh -c \"sed -i s/a/b/ f\"'",
        ] {
            assert!(matches!(inspect(command), Verdict::Refuse(_)), "{command}");
        }
        assert_eq!(inspect("sh <<'EOF'\nrg x .\nEOF"), Verdict::Admit);
    }
    use alloc::vec;

    const REFUSED_INTERPRETER: Verdict = Verdict::Refuse(Reason::InlineInterpreter);
    const REFUSED_IN_PLACE: Verdict = Verdict::Refuse(Reason::InPlaceEdit);
    const REFUSED_FILE: Verdict = Verdict::Refuse(Reason::FileRedirect);

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every interpreter")]
    fn inline_interpreters_are_refused() {
        for command in [
            "python -c \"open('a','w').write('x')\"",
            "python3 -c 'print(1)'",
            "/usr/bin/python3.12 -c 'x'",
            "node -e \"require('fs').writeFileSync('a','b')\"",
            "perl -pi -e 's/a/b/' file",
            "ruby -e 'File.write(1,2)'",
            "python - <<'EOF'\nopen('a','w').write('x')\nEOF",
            "python <<EOF\nprint(1)\nEOF",
            "cat script.py | python",
            "echo hi && python -c 'x'",
            "FOO=1 python -c 'x'",
            "mise x -- python -c 'x'",
            "env python3 -c 'x'",
        ] {
            assert_eq!(inspect(command), REFUSED_INTERPRETER, "{command}");
        }
    }

    #[cfg_attr(not(miri), test)]
    fn redirects_into_files_are_refused() {
        for command in [
            "cat > notes.txt <<'EOF'\nbody\nEOF",
            "cat <<EOF >> notes.txt\nbody\nEOF",
            "echo hi | tee -a notes.txt <<'EOF'\nbody\nEOF",
            "echo hi > out.txt",
            "echo hi >> out.txt",
            "echo hi 2> err.txt",
            "echo hi &> all.txt",
            "echo hi | tee out.txt",
            "echo hi |tee -a out.txt",
            "printf x >|out",
        ] {
            assert_eq!(inspect(command), REFUSED_FILE, "{command}");
        }
    }

    #[cfg_attr(not(miri), test)]
    fn writes_into_ignored_targets_are_admitted() {
        for command in [
            "cargo test > target/test.log",
            "echo hi | tee target/a.txt",
            "bash -c 'echo hi >> target/a.txt'",
        ] {
            let targets = write_targets(command);
            assert!(!targets.is_empty(), "{command}");
            let ignored: Vec<&str> = targets.iter().map(Target::as_str).collect();
            assert_eq!(
                inspect_except(command, &ignored),
                Verdict::Admit,
                "{command}"
            );
            assert_eq!(inspect(command), REFUSED_FILE, "{command}");
        }
        assert_eq!(
            write_targets("echo a > one.txt 2> /dev/null; echo b | tee -a two.txt"),
            ["one.txt", "two.txt"]
        );
        assert_eq!(
            inspect_except(
                "echo a > target/a.txt; echo b > notes.txt",
                &["target/a.txt"]
            ),
            REFUSED_FILE
        );
        assert_eq!(
            inspect_except("python -c 'x' > target/a.txt", &["target/a.txt"]),
            Verdict::Refuse(Reason::InlineInterpreter)
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every command")]
    fn in_place_editor_flags_are_refused() {
        for command in [
            "sed -i 's/a/b/' file.txt",
            "sed --in-place 's/a/b/' file.txt",
            "sed --in-place=.bak 's/a/b/' file.txt",
            "sed -i.bak 's/a/b/' file.txt",
            "sed -ni 's/a/b/p' file.txt",
            "awk -i inplace '{print $1}' data.tsv",
            "gawk -iinplace '{print}' data.tsv",
            "sudo sed -i 's/a/b/' /etc/hosts",
            "env sed -i 's/a/b/' file.txt",
            "mise x -- sed -i 's/a/b/' file.txt",
            "find src -name '*.rs' -exec sed -i 's/a/b/' {} +",
            "find -- src -exec sed -i 's/a/b/' {} +",
            "git ls-files | xargs sed -i 's/a/b/'",
            "git ls-files -z | xargs -0 sed -i 's/a/b/'",
        ] {
            assert_eq!(inspect(command), REFUSED_IN_PLACE, "{command}");
        }
        assert!(in_place_flag("sed", &["-i", "s/a/b/"]));
        assert!(in_place_flag("sed", &["--in-place"]));
        assert!(in_place_flag("sed", &["--in-place=.bak"]));
        assert!(in_place_flag("perl", &["-pi"]));
        assert!(in_place_flag("awk", &["-i", "inplace"]));
        assert!(!in_place_flag("sed", &["-n", "s/a/b/p"]));
        assert!(!in_place_flag("sed", &["--", "-i"]));
        assert!(!in_place_flag("awk", &["-i"]));
        assert!(!in_place_flag("awk", &["-v", "i=x"]));
        assert!(!in_place_flag("grep", &["-i", "pattern"]));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every command")]
    fn awk_programs_that_write_files_are_refused() {
        for command in [
            "awk '{print $1 > \"out.txt\"}' data.tsv",
            "awk '{printf \"%s\\n\", $0 >> \"log.txt\"}' data",
            "awk '{ print > \"out\" }' data",
            "awk 'NR>1 {print; print $2 > \"out.txt\"}' data",
            "awk -F, '{print $1 > (\"part\" $2)}' data",
            "gawk '{print $0 > \"out.txt\"}' data",
        ] {
            assert_eq!(inspect(command), REFUSED_IN_PLACE, "{command}");
        }
        assert!(awk_program_writes("{print > \"out\"}"));
        assert!(awk_program_writes("printf(\"x\") > \"f\""));
        assert!(awk_program_writes("print $1 >> f"));
        assert!(!awk_program_writes("{print $1}"));
        assert!(!awk_program_writes("{print \"a>b\"}"));
        assert!(!awk_program_writes("{print $1 >= $2}"));
        assert!(!awk_program_writes("if (a > \"b\") print a"));
        assert!(!awk_program_writes("{ x = 1; print x }"));
        assert!(!awk_program_writes("data.tsv"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every command")]
    fn reading_and_project_commands_are_admitted() {
        for command in [
            "sed --help",
            "sh script.c 'sed -i x f'",
            "sh -x 'sed -i x f'",
            "bash --rcfile 'sed -i x f'",
            "cat README.md",
            "grep -rn foo src",
            "git status --short",
            "git commit -F msg.txt",
            "gh issue view 123 --repo P4suta/dotfiles",
            "gh pr create --body-file - <<'EOF'\nbody > not a redirect\nEOF",
            "jq .name package.json",
            "just check",
            "cargo run --manifest-path xtask/Cargo.toml -- proofs",
            "pr-workflow create --repo a/b",
            "echo hi > /dev/null",
            "cargo build 2>&1",
            "cargo build 2>/dev/null",
            "echo hi >&2",
            "git commit -m \"explain python -c and > redirects\"",
            "echo 'python -c x'",
            "python script.py",
            "python -m json.tool data.json",
            "python --version",
            "node script.js",
            "node --version",
            "ls | grep foo",
            "sed -n 's/a/b/p' file.txt",
            "sed 's/a/b/' file.txt",
            "awk '{print $1}' data.tsv",
            "awk '$1 > 1 {print}' data.tsv",
            "if [ 1 -lt 2 ]; then echo yes; fi",
            "tee --help",
            "",
        ] {
            assert_eq!(inspect(command), Verdict::Admit, "{command}");
        }
    }

    #[cfg_attr(not(miri), test)]
    fn a_program_name_drops_its_directory_case_and_suffixes() {
        assert_eq!(program("/usr/bin/Python3.12"), "python");
        assert_eq!(program(r"C:\Tools\GIT.EXE"), "git");
        assert_eq!(program("node"), "node");
        assert_eq!(program("dir/"), "");
        assert_eq!(program("2to3"), "2to");
    }

    #[cfg_attr(not(miri), test)]
    fn refusals_name_a_next_action() {
        for (reason, start) in [
            (Reason::InlineInterpreter, "Use the Edit"),
            (Reason::InPlaceEdit, "Use the Edit"),
            (Reason::FileRedirect, "Use the Write"),
        ] {
            assert!(next_action(reason).starts_with(start), "{reason:?}");
        }
    }

    fn word(text: &str, plain: bool) -> Token {
        Token::Word {
            text: text.to_owned(),
            plain,
        }
    }

    fn plain(text: &str) -> Token {
        word(text, true)
    }

    #[cfg_attr(not(miri), test)]
    fn the_scanner_reads_quotes_escapes_and_separators_into_tokens() {
        assert_eq!(scan("'a b'c").tokens, [word("a bc", false)]);
        assert_eq!(scan("'a' 'b'").tokens, [word("a", false), word("b", false)]);
        assert_eq!(
            scan("\"a\\\"b\" c").tokens,
            [word("a\"b", false), plain("c")]
        );
        assert_eq!(scan("\"a\\\\\" b").tokens, [word("a\\", false), plain("b")]);
        assert_eq!(scan("a\\ b c").tokens, [plain("a b"), plain("c")]);
        assert_eq!(scan("a\\").tokens, [plain("a")]);
        assert_eq!(
            scan("a;b|c(d)e{f}g&h").tokens,
            [
                plain("a"),
                Token::Separator,
                plain("b"),
                Token::Separator,
                plain("c"),
                Token::Separator,
                plain("d"),
                Token::Separator,
                plain("e{f}g"),
                Token::Separator,
                plain("h")
            ]
        );
        assert_eq!(
            scan("{ f; }").tokens,
            [
                Token::Separator,
                plain("f"),
                Token::Separator,
                Token::Separator
            ]
        );
        assert_eq!(scan("a\tb\rc").tokens, [plain("a"), plain("b"), plain("c")]);
    }

    #[cfg_attr(not(miri), test)]
    fn the_scanner_tells_each_redirect_from_a_descriptor_copy() {
        for command in ["a &> b", "a &>> b", "a > b", "a >> b", "a >| b", "a 2> b"] {
            assert_eq!(
                scan(command).tokens,
                [plain("a"), Token::Redirect, plain("b")],
                "{command}"
            );
        }
        assert_eq!(
            scan("ab>f").tokens,
            [plain("ab"), Token::Redirect, plain("f")]
        );
        assert_eq!(
            scan("a2x>f").tokens,
            [plain("a2x"), Token::Redirect, plain("f")]
        );
        assert_eq!(
            scan("a 2>&1 b").tokens,
            [plain("a"), plain("1"), plain("b")]
        );
        assert_eq!(
            scan("a & b").tokens,
            [plain("a"), Token::Separator, plain("b")]
        );
        assert_eq!(scan("cat <x").tokens, [plain("cat"), plain("<x")]);
        // A here-string feeds standard input as a heredoc does, so its text never counts as an argument.
        let here_string = scan("cat <<< x");
        assert_eq!(here_string.tokens, [plain("cat"), plain("<")]);
        assert!(here_string.heredoc);
    }

    #[cfg_attr(not(miri), test)]
    fn a_heredoc_body_ends_at_its_delimiter_and_stays_out_of_the_words() {
        let found = scan("cat <<'EOF' > f\nline one\n\tEOF\nnext");
        assert!(found.heredoc);
        assert_eq!(found.heredocs, ["line one\n"]);
        assert_eq!(
            found.tokens,
            [
                plain("cat"),
                Token::Redirect,
                plain("f"),
                Token::Separator,
                plain("next")
            ]
        );
        let stripped = scan("cat <<-  \"END\"\nbody\nEND\n");
        assert_eq!(stripped.heredocs, ["body\n"]);
        let unterminated = scan("cat <<X\nopen");
        assert_eq!(unterminated.heredocs, ["open\n"]);
        let chars: Vec<char> = "-  EOF; rest".chars().collect();
        assert_eq!(heredoc_delimiter(&chars, 0), ("EOF".to_owned(), 6));
        let chars: Vec<char> = "x  E\\O\"F\" rest".chars().collect();
        assert_eq!(heredoc_delimiter(&chars, 1), ("EOF".to_owned(), 9));
        let chars: Vec<char> = "a\nEND\nb".chars().collect();
        let mut delimiters = alloc::vec!["END".to_owned()];
        let mut bodies = Vec::new();
        assert_eq!(
            skip_heredoc_body(&chars, 0, &mut delimiters, &mut bodies),
            6
        );
        assert_eq!(bodies, ["a\n"]);
    }

    #[cfg_attr(not(miri), test)]
    fn segments_drop_empty_commands_and_stop_at_a_redirect() {
        assert_eq!(segments("a b; ;c > f d"), [vec!["a", "b"], vec!["c"]]);
        assert_eq!(segments(" ; "), Vec::<Vec<String>>::new());
    }

    #[cfg_attr(not(miri), test)]
    fn only_a_whole_word_brace_separates_commands() {
        assert_eq!(
            segments("git rev-parse HEAD^{tree}"),
            [vec!["git", "rev-parse", "HEAD^{tree}"]]
        );
        assert_eq!(
            segments("echo ${tree} {a,b}"),
            [vec!["echo", "${tree}", "{a,b}"]]
        );
        assert_eq!(
            segments("find . -exec rm {} \\;"),
            [vec!["find", ".", "-exec", "rm", "{}", ";"]]
        );
        assert_eq!(segments("{ ls; }"), [vec!["ls"]]);
        assert_eq!(segments("{ ls;}|x"), [vec!["ls"], vec!["x"]]);
        assert_eq!(segments("f() { ls; }"), [vec!["f"], vec!["ls"]]);
    }

    #[cfg_attr(not(miri), test)]
    fn an_assignment_names_a_variable_before_its_equals_sign() {
        assert!(is_assignment("A_1=x"));
        assert!(is_assignment("a="));
        assert!(!is_assignment("=x"));
        assert!(!is_assignment("a-b=x"));
        assert!(!is_assignment("plain"));
    }

    #[cfg_attr(not(miri), test)]
    fn each_interpreter_runs_inline_only_through_its_own_flags() {
        assert!(runs_inline("node", &["--eval", "x"], false));
        assert!(runs_inline("node", &["-p", "x"], false));
        assert!(!runs_inline("node", &["--check", "s.js"], false));
        assert!(!runs_inline(
            "python",
            &["--check-hash-based-pycs", "always", "s.py"],
            false
        ));
        assert!(runs_inline("python", &["-Bc", "x"], false));
        assert!(runs_inline("ruby", &["-E", "x"], false));
        assert!(!runs_inline("ruby", &["--encoding", "s.rb"], false));
        assert!(runs_inline("perl", &["-", "x"], false));
        assert!(!runs_inline("perl", &["s.pl"], false));
        assert!(runs_inline("perl", &["s.pl"], true));
        assert!(!runs_inline("python", &["-V"], false));
        assert!(runs_inline("python", &[], false));
        assert!(!runs_inline("bash", &["-c", "x"], false));
    }

    #[cfg_attr(not(miri), test)]
    fn a_command_prefix_is_skipped_only_for_mise_and_the_wrappers() {
        assert_eq!(inspect("foo x python -c 1"), Verdict::Admit);
        assert_eq!(
            inspect("mise x -- python -c 1"),
            Verdict::Refuse(Reason::InlineInterpreter)
        );
        assert_eq!(
            inspect("mise exec python -c 1"),
            Verdict::Refuse(Reason::InlineInterpreter)
        );
        assert_eq!(
            inspect("A=1 env sudo time python -c 1"),
            Verdict::Refuse(Reason::InlineInterpreter)
        );
        assert_eq!(inspect("mise run python"), Verdict::Admit);
        assert_eq!(inspect("tee -a /dev/null"), Verdict::Admit);
        assert_eq!(inspect("tee log"), Verdict::Refuse(Reason::FileRedirect));
    }

    #[test]
    fn shell_segments_preserve_literal_words_and_nested_scripts() {
        assert_eq!(
            segments("gh pr create --body 'a\nb'"),
            [vec!["gh", "pr", "create", "--body", "a\nb"]]
        );
        assert_eq!(
            segments("sh <<'EOF'\ngrep x f\nEOF"),
            [vec!["sh"], vec!["grep", "x", "f"]]
        );
    }
}

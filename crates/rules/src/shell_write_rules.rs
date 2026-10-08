extern crate alloc;
use self::alloc::borrow::ToOwned;
use self::alloc::string::String;
use self::alloc::vec::Vec;

/// How a shell command writes a file when it should have used a file tool or a project command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reason {
    /// An inline interpreter runs code from `-c`, `-e`, or standard input.
    InlineInterpreter,
    /// A redirect, `tee`, or heredoc target names a file.
    FileRedirect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Admit,
    Refuse(Reason),
}

#[must_use]
pub fn verdict(inline_interpreter: bool, file_write: bool) -> Verdict {
    if inline_interpreter {
        Verdict::Refuse(Reason::InlineInterpreter)
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
            ';' | '|' | '(' | ')' | '{' | '}' | '&' => {
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

/// The words of each simple command in `command`, in order, each up to its first redirect.
///
/// Exempt as disproportionate: a shell parser over unbounded text needs fuzzing, which no verifier here owns yet.
/// Evidence: `crates/rules/src/shell_write_rules.rs::tests::standard_input_bodies_include_heredocs_and_piped_words`.
/// Reconsider when a cargo-fuzz target owns this function.
#[must_use]
pub fn segments(command: &str) -> Vec<Vec<String>> {
    scan(command)
        .tokens
        .split(|token| *token == Token::Separator)
        .map(|segment| {
            segment_words(segment)
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|words| !words.is_empty())
        .collect()
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
/// Exempt as disproportionate: Kani spends past its budget on the string search of even a three-byte word.
/// Evidence: `crates/rules/src/shell_write_rules.rs::tests::a_program_name_drops_its_directory_case_and_suffixes`.
/// Reconsider when a cargo-fuzz target owns this function.
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

/// The words a flag and its separate value take.
const FLAG_WITH_VALUE: usize = 2;

/// The words of a `gh` subcommand: its group and its verb.
const SUBCOMMAND_WORDS: usize = 2;

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

/// Where the text of a body sent to GitHub comes from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Source {
    /// The command line carries the body.
    Inline(String),
    /// The command reads the body from a file that the command line names.
    File(String),
    /// The request payload comes from a JSON file, and the command sends its `body` string.
    Json(String),
    /// The body arrives on standard input: heredocs and the words of the rest of the command.
    Stdin(String),
}

/// The `gh` subcommands that send a body, as `(group, verb)`.
const BODY_COMMANDS: [(&str, &str); 7] = [
    ("pr", "create"),
    ("pr", "edit"),
    ("pr", "comment"),
    ("pr", "review"),
    ("issue", "create"),
    ("issue", "edit"),
    ("issue", "comment"),
];

fn flag_value<'a>(arguments: &[&'a str], index: usize, names: &[&str]) -> Option<(&'a str, usize)> {
    let argument = *arguments.get(index)?;
    for name in names {
        if argument == *name {
            return arguments
                .get(index + 1)
                .map(|value| (*value, FLAG_WITH_VALUE));
        }
        let joined = argument
            .strip_prefix(name)
            .and_then(|rest| rest.strip_prefix('='))
            .filter(|_| name.starts_with("--"));
        if let Some(value) = joined {
            return Some((value, 1));
        }
    }
    None
}

fn file_source(path: &str) -> Source {
    if path == "-" {
        Source::Stdin(String::new())
    } else {
        Source::File(path.to_owned())
    }
}

/// The value of a `key=value` field when the key names a body.
fn body_value(field: &str) -> Option<&str> {
    field
        .split_once('=')
        .filter(|(key, _)| *key == "body" || key.ends_with("[body]"))
        .map(|(_, value)| value)
}

/// The words after the one at `index`.
fn following<'a, 'b>(words: &'b [&'a str], index: usize) -> &'b [&'a str] {
    words
        .get(index..)
        .and_then(<[&str]>::split_first)
        .map_or(&[], |(_, rest)| rest)
}

/// The arguments after the `used` words a flag took, always at least the flag itself.
fn after_flag<'a, 'b>(arguments: &'b [&'a str], used: usize) -> &'b [&'a str] {
    arguments.get(used.max(1)..).unwrap_or_default()
}

fn api_sources(mut arguments: &[&str], sources: &mut Vec<Source>) {
    while !arguments.is_empty() {
        let mut used = 1;
        if let Some((value, taken)) = flag_value(arguments, 0, &["-f", "--raw-field"]) {
            if let Some(text) = body_value(value) {
                sources.push(Source::Inline(text.to_owned()));
            }
            used = taken;
        } else if let Some((value, taken)) = flag_value(arguments, 0, &["-F", "--field"]) {
            if let Some(text) = body_value(value) {
                sources.push(match text.strip_prefix('@') {
                    Some(path) => file_source(path),
                    None => Source::Inline(text.to_owned()),
                });
            }
            used = taken;
        } else if let Some((value, taken)) = flag_value(arguments, 0, &["--input"]) {
            sources.push(match value {
                "-" => Source::Stdin(String::new()),
                path => Source::Json(path.to_owned()),
            });
            used = taken;
        }
        arguments = after_flag(arguments, used);
    }
}

fn command_sources(mut arguments: &[&str], sources: &mut Vec<Source>) {
    while !arguments.is_empty() {
        let mut used = 1;
        if let Some((value, taken)) = flag_value(arguments, 0, &["-b", "--body"]) {
            sources.push(Source::Inline(value.to_owned()));
            used = taken;
        } else if let Some((value, taken)) = flag_value(arguments, 0, &["-F", "--body-file"]) {
            sources.push(file_source(value));
            used = taken;
        }
        arguments = after_flag(arguments, used);
    }
}

/// Every place the bodies of the `gh` writes in a command come from.
///
/// This function resolves standard input because it knows the command text.
/// Standard input covers the heredoc bodies and the words of every other command in the line.
/// It also covers `echo "$text" | gh pr comment --body-file -`.
///
/// Exempt as disproportionate: a shell parser over unbounded text needs fuzzing, which no verifier here owns yet.
/// Evidence: `crates/rules/src/shell_write_rules.rs::tests::gh_body_arguments_are_collected_from_every_spelling`.
/// Reconsider when a cargo-fuzz target owns this function.
pub fn body_sources(command: &str) -> Vec<Source> {
    let scan = scan(command);
    let segments: Vec<Vec<&str>> = scan
        .tokens
        .split(|token| *token == Token::Separator)
        .map(segment_words)
        .collect();
    let mut sources = Vec::new();
    for (position, words) in segments.iter().enumerate() {
        let Some(gh) = words.iter().position(|word| program(word) == "gh") else {
            continue;
        };
        let rest = following(words, gh);
        let before = sources.len();
        if let Some(api) = rest.iter().position(|word| *word == "api") {
            api_sources(following(rest, api), &mut sources);
        } else if BODY_COMMANDS.iter().any(|(group, verb)| {
            rest.windows(SUBCOMMAND_WORDS)
                .any(|pair| pair == [*group, *verb])
        }) {
            command_sources(rest, &mut sources);
        }
        let mut stdin = scan.heredocs.join("");
        for (other, words) in segments.iter().enumerate() {
            if other != position {
                for word in words {
                    stdin.push_str(word);
                    stdin.push('\n');
                }
            }
        }
        for source in sources.iter_mut().skip(before) {
            if matches!(source, Source::Stdin(_)) {
                *source = Source::Stdin(stdin.clone());
            }
        }
    }
    sources
}

/// The two detections behind the verdict.
/// They cover an inline interpreter and a file write.
///
/// Exempt as disproportionate: a shell parser over unbounded text needs fuzzing, which no verifier here owns yet.
/// Evidence: `crates/rules/src/shell_write_rules.rs::tests::inline_interpreters_are_refused`.
/// Reconsider when a cargo-fuzz target owns this function.
#[must_use]
pub fn inspect(command: &str) -> Verdict {
    let scan = scan(command);
    let mut interpreter = false;
    let mut file_write = false;
    for (index, token) in scan.tokens.iter().enumerate() {
        if *token == Token::Redirect && writes_to(scan.tokens.get(index + 1)) {
            file_write = true;
        }
    }
    for segment in scan.tokens.split(|token| *token == Token::Separator) {
        let words = segment_words(segment);
        let mut command = words.as_slice();
        loop {
            command = match command {
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
                _ => break,
            };
        }
        let Some((first, rest)) = command.split_first() else {
            continue;
        };
        let name = program(first);
        if runs_inline(name.as_str(), rest, scan.heredoc) {
            interpreter = true;
        }
        if name == "tee"
            && rest
                .iter()
                .any(|a| !a.starts_with('-') && !matches!(*a, DISCARD | STDOUT))
        {
            file_write = true;
        }
    }
    verdict(interpreter, file_write)
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// The verdict never admits a file-writing command, an inline interpreter names its own refusal first, and each refusal names a file tool.
    pub fn spec_verdict(interpreter: bool, file: bool) {
        let result = verdict(interpreter, file);
        let expected = if interpreter {
            Verdict::Refuse(Reason::InlineInterpreter)
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
        let file: bool = kani::any();
        spec_verdict(interpreter, file);
        let result = verdict(interpreter, file);
        kani::cover!(result == Verdict::Admit);
        kani::cover!(result == Verdict::Refuse(Reason::InlineInterpreter));
        kani::cover!(result == Verdict::Refuse(Reason::FileRedirect));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_input_meets_the_verdict_specification() {
        for [interpreter, file] in crate::domain::flags() {
            super::proofs::spec_verdict(interpreter, file);
        }
        assert!(super::next_action(super::Reason::InlineInterpreter).starts_with("Use the Edit"));
        assert!(super::next_action(super::Reason::FileRedirect).starts_with("Use the Write"));
    }

    use super::*;
    use alloc::vec;

    const REFUSED_INTERPRETER: Verdict = Verdict::Refuse(Reason::InlineInterpreter);
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

    #[test]
    fn redirects_into_files_are_refused() {
        for command in [
            "cat > notes.txt <<'EOF'\nbody\nEOF",
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

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every command")]
    fn reading_and_project_commands_are_admitted() {
        for command in [
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
            "if [ 1 -lt 2 ]; then echo yes; fi",
            "tee --help",
            "",
        ] {
            assert_eq!(inspect(command), Verdict::Admit, "{command}");
        }
    }

    fn inline(text: &str) -> Source {
        Source::Inline(text.to_owned())
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every spelling")]
    fn gh_body_arguments_are_collected_from_every_spelling() {
        for (command, expected) in [
            ("gh pr create --title t --body 'a\nb'", vec![inline("a\nb")]),
            ("gh pr create -b text", vec![inline("text")]),
            ("gh issue create --body=text", vec![inline("text")]),
            (
                "gh pr edit 3 --body-file notes.md",
                vec![Source::File("notes.md".into())],
            ),
            (
                "env -u SSH_AUTH_SOCK gh -R a/b pr comment 3 -F notes.md",
                vec![Source::File("notes.md".into())],
            ),
            (
                "gh issue comment 3 --body-file=notes.md",
                vec![Source::File("notes.md".into())],
            ),
            (
                "gh api repos/a/b/issues/3/comments -f body=text",
                vec![inline("text")],
            ),
            (
                "gh api repos/a/b/issues/3 -X PATCH -F body=@notes.md",
                vec![Source::File("notes.md".into())],
            ),
            (
                "gh api repos/a/b/issues -F body=literal@text",
                vec![inline("literal@text")],
            ),
            (
                "gh api x --raw-field body=@notes.md",
                vec![inline("@notes.md")],
            ),
            (
                "gh api x --input payload.json",
                vec![Source::Json("payload.json".into())],
            ),
            ("gh api x -f comment[body]=text", vec![inline("text")]),
        ] {
            assert_eq!(body_sources(command), expected, "{command}");
        }
    }

    #[test]
    fn standard_input_bodies_include_heredocs_and_piped_words() {
        assert_eq!(
            body_sources("gh pr create --body-file - <<'EOF'\nline one\nline two\nEOF"),
            vec![Source::Stdin("line one\nline two\n".into())]
        );
        assert_eq!(
            body_sources("echo \"hello\" | gh pr comment 3 -F -"),
            vec![Source::Stdin("echo\nhello\n".into())]
        );
        assert_eq!(
            body_sources("gh api x --input - <<EOF\n{\"body\":\"b\"}\nEOF"),
            vec![Source::Stdin("{\"body\":\"b\"}\n".into())]
        );
    }

    #[test]
    fn commands_that_send_no_body_have_no_sources() {
        for command in [
            "gh pr view 3 --json body",
            "gh pr create --fill",
            "gh pr list --search body",
            "gh issue view 3 --comments",
            "gh api repos/a/b/issues/3",
            "gh api x -f title=text",
            "git commit -F msg.txt",
            "echo --body text",
            "",
        ] {
            assert!(body_sources(command).is_empty(), "{command}");
        }
    }

    #[test]
    fn a_program_name_drops_its_directory_case_and_suffixes() {
        assert_eq!(program("/usr/bin/Python3.12"), "python");
        assert_eq!(program(r"C:\Tools\GIT.EXE"), "git");
        assert_eq!(program("node"), "node");
        assert_eq!(program("dir/"), "");
        assert_eq!(program("2to3"), "2to");
    }

    #[test]
    fn refusals_name_a_next_action() {
        for reason in [Reason::InlineInterpreter, Reason::FileRedirect] {
            assert!(next_action(reason).contains("Write"));
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

    #[test]
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
                plain("e"),
                Token::Separator,
                plain("f"),
                Token::Separator,
                plain("g"),
                Token::Separator,
                plain("h")
            ]
        );
        assert_eq!(scan("a\tb\rc").tokens, [plain("a"), plain("b"), plain("c")]);
    }

    #[test]
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

    #[test]
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

    #[test]
    fn segments_drop_empty_commands_and_stop_at_a_redirect() {
        assert_eq!(segments("a b; ;c > f d"), [vec!["a", "b"], vec!["c"]]);
        assert_eq!(segments(" ; "), Vec::<Vec<String>>::new());
    }

    #[test]
    fn an_assignment_names_a_variable_before_its_equals_sign() {
        assert!(is_assignment("A_1=x"));
        assert!(is_assignment("a="));
        assert!(!is_assignment("=x"));
        assert!(!is_assignment("a-b=x"));
        assert!(!is_assignment("plain"));
    }

    #[test]
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

    #[test]
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
    fn api_and_command_bodies_come_from_every_flag_form() {
        let mut sources = Vec::new();
        api_sources(
            &[
                "repos/x",
                "-f",
                "body=a",
                "-f",
                "title=t",
                "--raw-field=body=b",
                "-F",
                "body=@f.md",
                "-F",
                "body=c",
                "--field",
                "body=@-",
                "--input",
                "p.json",
                "--input=-",
                "-f",
            ],
            &mut sources,
        );
        assert_eq!(
            sources,
            [
                Source::Inline("a".to_owned()),
                Source::Inline("b".to_owned()),
                Source::File("f.md".to_owned()),
                Source::Inline("c".to_owned()),
                Source::Stdin(String::new()),
                Source::Json("p.json".to_owned()),
                Source::Stdin(String::new()),
            ]
        );
        let mut sources = Vec::new();
        command_sources(
            &[
                "pr",
                "create",
                "-b",
                "x",
                "--body=y",
                "--body-file",
                "-",
                "-F",
                "f.md",
                "--body",
            ],
            &mut sources,
        );
        assert_eq!(
            sources,
            [
                Source::Inline("x".to_owned()),
                Source::Inline("y".to_owned()),
                Source::Stdin(String::new()),
                Source::File("f.md".to_owned()),
            ]
        );
        assert_eq!(flag_value(&["-b"], 0, &["-b"]), None);
        assert_eq!(flag_value(&["-bx"], 0, &["-b"]), None);
        assert_eq!(flag_value(&["--body=v"], 0, &["--body"]), Some(("v", 1)));
        assert_eq!(flag_value(&["x", "-b", "v"], 1, &["-b"]), Some(("v", 2)));
    }

    #[test]
    fn standard_input_takes_the_words_of_every_other_command() {
        assert_eq!(
            body_sources("echo hello | env gh pr comment --body-file -"),
            [Source::Stdin("echo\nhello\n".to_owned())]
        );
        assert_eq!(
            body_sources("env gh api x -f body=a"),
            [Source::Inline("a".to_owned())]
        );
        assert_eq!(
            body_sources("gh repo api x -f body=a"),
            [Source::Inline("a".to_owned())]
        );
        assert_eq!(body_sources("gh pr list --body x"), []);
    }
}

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

pub fn verdict(inline_interpreter: bool, file_write: bool) -> Verdict {
    if inline_interpreter {
        Verdict::Refuse(Reason::InlineInterpreter)
    } else if file_write {
        Verdict::Refuse(Reason::FileRedirect)
    } else {
        Verdict::Admit
    }
}

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
            text: std::mem::take(word),
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
            let end = chars[index..]
                .iter()
                .position(|c| *c == '\n')
                .map_or(chars.len(), |offset| index + offset);
            let line: String = chars[index..end].iter().collect();
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

fn scan(command: &str) -> Scan {
    let chars: Vec<char> = command.chars().collect();
    let mut scan = Scan::default();
    let mut word = String::new();
    let mut plain = true;
    let mut started = false;
    let mut delimiters: Vec<String> = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        index += 1;
        match c {
            '\'' => {
                started = true;
                plain = false;
                while index < chars.len() && chars[index] != '\'' {
                    word.push(chars[index]);
                    index += 1;
                }
                index += 1;
            }
            '"' => {
                started = true;
                plain = false;
                while index < chars.len() && chars[index] != '"' {
                    if chars[index] == '\\' && index + 1 < chars.len() {
                        index += 1;
                    }
                    word.push(chars[index]);
                    index += 1;
                }
                index += 1;
            }
            '\\' => {
                started = true;
                if index < chars.len() {
                    word.push(chars[index]);
                    index += 1;
                }
            }
            ' ' | '\t' | '\r' => flush(&mut word, &mut plain, &mut started, &mut scan.tokens),
            '\n' => {
                flush(&mut word, &mut plain, &mut started, &mut scan.tokens);
                scan.tokens.push(Token::Separator);
                index = skip_heredoc_body(&chars, index, &mut delimiters, &mut scan.heredocs);
            }
            ';' | '|' | '(' | ')' | '{' | '}' => {
                flush(&mut word, &mut plain, &mut started, &mut scan.tokens);
                scan.tokens.push(Token::Separator);
            }
            '&' if chars.get(index) == Some(&'>') => {
                flush(&mut word, &mut plain, &mut started, &mut scan.tokens);
                index += 1;
                if chars.get(index) == Some(&'>') {
                    index += 1;
                }
                scan.tokens.push(Token::Redirect);
            }
            '&' => {
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
                index += 1;
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

/// The program a command word names: its file name, lowercased, without `.exe` or a version suffix.
pub fn program(text: &str) -> String {
    let name = text.rsplit(['/', '\\']).next().unwrap_or(text);
    let lower = name.to_ascii_lowercase();
    let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
    stem.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.')
        .to_owned()
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

fn writes_to(target: Option<&Token>) -> bool {
    match target {
        Some(Token::Word { text, .. }) => {
            !matches!(text.as_str(), "/dev/null" | "/dev/stdout" | "/dev/stderr")
        }
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
    /// The body is written on the command line.
    Inline(String),
    /// The body is read from a file named on the command line.
    File(String),
    /// The request payload is a JSON file whose `body` string is sent.
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
    let argument = arguments[index];
    for name in names {
        if argument == *name {
            return arguments.get(index + 1).map(|value| (*value, 2));
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

/// The value of a `key=value` field when the key is a body.
fn body_value(field: &str) -> Option<&str> {
    field
        .split_once('=')
        .filter(|(key, _)| *key == "body" || key.ends_with("[body]"))
        .map(|(_, value)| value)
}

fn api_sources(arguments: &[&str], sources: &mut Vec<Source>) {
    let mut index = 0;
    while index < arguments.len() {
        if let Some((value, used)) = flag_value(arguments, index, &["-f", "--raw-field"]) {
            if let Some(text) = body_value(value) {
                sources.push(Source::Inline(text.to_owned()));
            }
            index += used;
        } else if let Some((value, used)) = flag_value(arguments, index, &["-F", "--field"]) {
            if let Some(text) = body_value(value) {
                sources.push(match text.strip_prefix('@') {
                    Some(path) => file_source(path),
                    None => Source::Inline(text.to_owned()),
                });
            }
            index += used;
        } else if let Some((value, used)) = flag_value(arguments, index, &["--input"]) {
            sources.push(match value {
                "-" => Source::Stdin(String::new()),
                path => Source::Json(path.to_owned()),
            });
            index += used;
        } else {
            index += 1;
        }
    }
}

fn command_sources(arguments: &[&str], sources: &mut Vec<Source>) {
    let mut index = 0;
    while index < arguments.len() {
        if let Some((value, used)) = flag_value(arguments, index, &["-b", "--body"]) {
            sources.push(Source::Inline(value.to_owned()));
            index += used;
        } else if let Some((value, used)) = flag_value(arguments, index, &["-F", "--body-file"]) {
            sources.push(file_source(value));
            index += used;
        } else {
            index += 1;
        }
    }
}

/// Every place the bodies of the `gh` writes in a command come from.
///
/// Standard input is resolved here, where the command text is known: it is the heredoc bodies and the words of every other command in the line, so `echo "$text" | gh pr comment --body-file -` is read as well.
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
        let rest = &words[gh + 1..];
        let before = sources.len();
        if let Some(api) = rest.iter().position(|word| *word == "api") {
            api_sources(&rest[api + 1..], &mut sources);
        } else if BODY_COMMANDS.iter().any(|(group, verb)| {
            rest.windows(2)
                .any(|pair| pair[0] == *group && pair[1] == *verb)
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
        for source in &mut sources[before..] {
            if matches!(source, Source::Stdin(_)) {
                *source = Source::Stdin(stdin.clone());
            }
        }
    }
    sources
}

/// The two detections the verdict is made from: an inline interpreter and a file write.
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
        let mut start = 0;
        while start < words.len() {
            let name = program(words[start]);
            if is_assignment(words[start])
                || matches!(
                    name.as_str(),
                    "env" | "command" | "exec" | "time" | "nohup" | "sudo"
                )
            {
                start += 1;
            } else if name == "mise" && matches!(words.get(start + 1), Some(&"x" | &"exec")) {
                start += 2;
                if words.get(start) == Some(&"--") {
                    start += 1;
                }
            } else {
                break;
            }
        }
        let Some(first) = words.get(start) else {
            continue;
        };
        let name = program(first);
        let rest = &words[start + 1..];
        if runs_inline(&name, rest, scan.heredoc) {
            interpreter = true;
        }
        if name == "tee"
            && rest
                .iter()
                .any(|a| !a.starts_with('-') && !matches!(*a, "/dev/null" | "/dev/stdout"))
        {
            file_write = true;
        }
    }
    verdict(interpreter, file_write)
}

#[cfg(kani)]
#[kani::proof]
fn file_writing_commands_are_never_admitted() {
    let interpreter: bool = kani::any();
    let file: bool = kani::any();
    let result = verdict(interpreter, file);
    assert_eq!(result == Verdict::Admit, !interpreter && !file);
    kani::cover!(result == Verdict::Admit);
    kani::cover!(result != Verdict::Admit);
}

#[cfg(kani)]
#[kani::proof]
fn every_refusal_names_a_next_action() {
    let interpreter: bool = kani::any();
    let file: bool = kani::any();
    let result = verdict(interpreter, file);
    if let Verdict::Refuse(reason) = result {
        assert!(!next_action(reason).is_empty());
    }
    kani::cover!(result == Verdict::Refuse(Reason::InlineInterpreter));
    kani::cover!(result == Verdict::Refuse(Reason::FileRedirect));
}

#[cfg(test)]
mod tests {
    use super::*;

    const REFUSED_INTERPRETER: Verdict = Verdict::Refuse(Reason::InlineInterpreter);
    const REFUSED_FILE: Verdict = Verdict::Refuse(Reason::FileRedirect);

    #[test]
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
    fn refusals_name_a_next_action() {
        for reason in [Reason::InlineInterpreter, Reason::FileRedirect] {
            assert!(next_action(reason).contains("Write"));
        }
    }
}

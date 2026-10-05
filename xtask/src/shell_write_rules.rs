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

fn skip_heredoc_body(chars: &[char], mut index: usize, delimiters: &mut Vec<String>) -> usize {
    for delimiter in delimiters.drain(..) {
        loop {
            let end = chars[index..]
                .iter()
                .position(|c| *c == '\n')
                .map_or(chars.len(), |offset| index + offset);
            let line: String = chars[index..end].iter().collect();
            index = (end + 1).min(chars.len());
            if line.trim_start_matches('\t') == delimiter || end == chars.len() {
                break;
            }
        }
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
                index = skip_heredoc_body(&chars, index, &mut delimiters);
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

fn program(text: &str) -> String {
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
        let words: Vec<&str> = segment
            .iter()
            .take_while(|token| **token != Token::Redirect)
            .filter_map(|token| match token {
                Token::Word { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
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

    #[test]
    fn refusals_name_a_next_action() {
        for reason in [Reason::InlineInterpreter, Reason::FileRedirect] {
            assert!(next_action(reason).contains("Write"));
        }
    }
}

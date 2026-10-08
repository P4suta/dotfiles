#[cfg(test)]
use crate::legacy_rules::{bypass_names, invokes};
use std::borrow::ToOwned;

/// The bytes a shell word may hold without quoting.
fn plain(word: &str) -> bool {
    !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_./:=@%+,-".contains(c))
}

/// One shell word that reads back as `word`.
fn quote(word: &str) -> String {
    if plain(word) {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

fn join(words: &[String]) -> String {
    words.join(" ")
}

/// The next action a refusal names: the preferred command, or the tool to use instead.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rewrite(String);

impl core::fmt::Display for Rewrite {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl PartialEq<&str> for Rewrite {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

/// The preferred command that does what `arguments` asked of the legacy command, from an entry's `next` text.
/// `grep` and `find` translate their flags, and any other text replaces `{args}` with the quoted arguments.
///
#[must_use]
pub fn rewrite(next: &str, arguments: &[&str]) -> Rewrite {
    Rewrite(match next {
        "grep" => grep_to_rg(arguments),
        "find" => find_to_fd(arguments),
        text => text.replace(
            "{args}",
            &join(&arguments.iter().map(|word| quote(word)).collect::<Vec<_>>()),
        ),
    })
}

/// The grep flags whose value follows them in the same word or the next.
const GREP_VALUES: &str = "ABCefmd";

fn grep_to_rg(arguments: &[&str]) -> String {
    let mut out = vec!["rg".to_owned()];
    let mut operands = Vec::new();
    let mut options = true;
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        if !options || !argument.starts_with('-') || *argument == "-" {
            operands.push(quote(argument));
            continue;
        }
        if *argument == "--" {
            options = false;
            operands.push("--".to_owned());
            continue;
        }
        if let Some(long) = argument.strip_prefix("--") {
            let (name, value) = long.split_once('=').unwrap_or((long, ""));
            match name {
                "recursive" | "dereference-recursive" | "extended-regexp" => {}
                "include" => out.extend(["-g".to_owned(), quote(value)]),
                "exclude" => out.extend(["-g".to_owned(), quote(&format!("!{value}"))]),
                "exclude-dir" => out.extend(["-g".to_owned(), quote(&format!("!{value}/"))]),
                "no-filename" => out.push("-I".to_owned()),
                _ => out.push(quote(argument)),
            }
            continue;
        }
        let mut cluster = argument.strip_prefix('-').unwrap_or_default().chars();
        while let Some(flag) = cluster.next() {
            match flag {
                'r' | 'R' | 'E' => {}
                'h' => out.push("-I".to_owned()),
                's' => out.push("--no-messages".to_owned()),
                'L' => out.push("--files-without-match".to_owned()),
                value if GREP_VALUES.contains(value) => {
                    out.push(format!("-{value}"));
                    let rest = cluster.as_str();
                    if rest.is_empty() {
                        out.extend(arguments.next().map(|next| quote(next)));
                    } else {
                        out.push(quote(rest));
                    }
                    break;
                }
                other => out.push(format!("-{other}")),
            }
        }
    }
    out.extend(operands);
    join(&out)
}

fn find_to_fd(arguments: &[&str]) -> String {
    let paths: Vec<String> = arguments
        .iter()
        .take_while(|argument| !argument.starts_with('-') && !matches!(**argument, "(" | "!"))
        .map(|path| quote(path))
        .collect();
    let mut flags = Vec::new();
    let mut pattern = None;
    let mut exec = Vec::new();
    let mut untranslated = Vec::new();
    let mut arguments = arguments.iter().skip(paths.len()).peekable();
    while let Some(argument) = arguments.next() {
        let value = arguments.peek().map(|word| quote(word));
        let flag = match *argument {
            "-name" => {
                pattern = value;
                None
            }
            "-iname" => {
                pattern = value;
                Some("--ignore-case")
            }
            "-path" | "-wholename" => {
                pattern = value;
                Some("--full-path")
            }
            "-type" | "-maxdepth" | "-mindepth" => {
                flags.push(
                    match *argument {
                        "-type" => "--type",
                        "-maxdepth" => "--max-depth",
                        _ => "--min-depth",
                    }
                    .to_owned(),
                );
                flags.extend(value);
                None
            }
            "-exec" | "-execdir" => {
                let mut command = Vec::new();
                for word in arguments.by_ref() {
                    if matches!(*word, ";" | "+") {
                        exec.push(
                            if *word == "+" {
                                "--exec-batch"
                            } else {
                                "--exec"
                            }
                            .to_owned(),
                        );
                        break;
                    }
                    command.push(quote(word));
                }
                exec.extend(command);
                continue;
            }
            "-print" => continue,
            other => {
                untranslated.push(other.to_owned());
                continue;
            }
        };
        arguments.next();
        flags.extend(flag.map(ToOwned::to_owned));
    }
    let mut out = vec!["fd".to_owned()];
    out.extend(flags);
    match pattern {
        Some(glob) => out.extend(["--glob".to_owned(), glob]),
        None if !paths.is_empty() => out.push(".".to_owned()),
        None => {}
    }
    out.extend(paths);
    out.extend(exec);
    let command = join(&out);
    if untranslated.is_empty() {
        command
    } else {
        format!(
            "{command}, and translate {} by hand from `fd --help`",
            untranslated.join(" ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every translation")]
    fn grep_and_find_translate_into_rg_and_fd() {
        for (grep, rg) in [
            ("-r pattern .", "rg pattern ."),
            ("-rn TODO src", "rg -n TODO src"),
            ("-Rni --include=*.rs foo", "rg -n -i -g '*.rs' foo"),
            ("-E 'a|b' file", "rg 'a|b' file"),
            ("-e x -e y f", "rg -e x -e y f"),
            ("-A3 x f", "rg -A 3 x f"),
            ("-C 2 x", "rg -C 2 x"),
            (
                "-rlh --exclude-dir=target x .",
                "rg -l -I -g '!target/' x .",
            ),
            ("-sL x", "rg --no-messages --files-without-match x"),
            ("--exclude=*.md --count x", "rg -g '!*.md' --count x"),
            ("-- -x f", "rg -- -x f"),
            ("--recursive --no-filename x", "rg -I x"),
        ] {
            let words: Vec<&str> = grep.split(' ').map(|w| w.trim_matches('\'')).collect();
            assert_eq!(rewrite("grep", &words), rg, "{grep}");
        }
        for (find, fd) in [
            (". -name *.rs", "fd --glob '*.rs' ."),
            (
                "src -type f -iname *.md",
                "fd --type f --ignore-case --glob '*.md' src",
            ),
            (
                ". -maxdepth 2 -mindepth 1 -type d",
                "fd --max-depth 2 --min-depth 1 --type d . .",
            ),
            ("-name x", "fd --glob x"),
            (". -path */a/* -print", "fd --full-path --glob '*/a/*' ."),
            (
                ". -name *.o -exec rm {} ;",
                "fd --glob '*.o' . --exec rm '{}'",
            ),
            (
                ". -name *.o -exec rm {} +",
                "fd --glob '*.o' . --exec-batch rm '{}'",
            ),
            (
                ". -newer f",
                "fd . ., and translate -newer f by hand from `fd --help`",
            ),
        ] {
            let words: Vec<&str> = find.split(' ').collect();
            assert_eq!(rewrite("find", &words), fd, "{find}");
        }
        assert_eq!(rewrite("eza {args}", &["-la", "a b"]), "eza -la 'a b'");
        assert_eq!(rewrite("procs", &["aux"]), "procs");
        assert_eq!(quote("it's"), "'it'\\''s'");
        assert_eq!(quote(""), "''");
    }

    #[test]
    fn a_bypass_names_its_entry_and_a_reason() {
        assert!(bypass_names("grep: byte-exact output", "grep"));
        assert!(bypass_names(" grep :x", "grep"));
        assert!(!bypass_names("grep:", "grep"));
        assert!(!bypass_names("grep:  ", "grep"));
        assert!(!bypass_names("find: reason", "grep"));
        assert!(!bypass_names("grep reason", "grep"));
    }

    #[test]
    fn conditional_entries_match_only_their_flags() {
        assert!(invokes(&["-l", "f"], &["-l", "--lines"]));
        assert!(invokes(&["-cl", "f"], &["-l"]));
        assert!(invokes(&["--lines", "f"], &["--lines"]));
        assert!(invokes(&["--lines=1"], &["--lines"]));
        assert!(!invokes(&["-c", "f"], &["-l", "--lines"]));
        assert!(!invokes(&["--long", "f"], &["-l"]));
        assert!(!invokes(&["--linesx"], &["--lines"]));
        assert!(!invokes(&["file-l"], &["-l"]));
        assert!(invokes(&[], &[]));
    }
}

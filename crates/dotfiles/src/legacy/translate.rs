use super::syntax::Argument;
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

struct Literal<'a>(&'a str);

impl<'a> Literal<'a> {
    fn read(argument: &Argument<'a>) -> Option<Self> {
        let source = argument.spelling;
        let value = if plain(source) {
            source
        } else if let Some(inner) = source
            .strip_prefix('\'')
            .and_then(|text| text.strip_suffix('\''))
        {
            (!inner.contains('\'')).then_some(inner)?
        } else {
            let inner = source
                .strip_prefix('"')
                .and_then(|text| text.strip_suffix('"'))?;
            (!inner.contains(['$', '`', '\\', '"', '!'])).then_some(inner)?
        };
        (value == argument.value).then_some(Self(value))
    }

    fn suffix(self, start: usize) -> Option<Self> {
        self.0.get(start..).map(Self)
    }

    fn suffix_of(argument: &Argument<'a>, start: usize) -> Option<Self> {
        let value = argument.value.get(start..)?;
        let prefix = argument.value.get(..start)?;
        if !plain(prefix) {
            return None;
        }
        if let Some(spelling) = argument.spelling.strip_prefix(prefix) {
            if spelling.is_empty() && value.is_empty() {
                return Some(Self(""));
            }
            Self::read(&Argument {
                value,
                spelling,
                order: argument.order,
            })
        } else {
            Self::read(argument)?.suffix(start)
        }
    }

    fn rendered(&self, prefix: &str, suffix: &str) -> String {
        quote(&format!("{prefix}{}{suffix}", self.0))
    }
}

enum Piece<'a> {
    Generated(String),
    Source {
        spelling: &'a str,
        dynamic_order: Option<usize>,
    },
}

impl From<String> for Piece<'_> {
    fn from(text: String) -> Self {
        Self::Generated(text)
    }
}

impl<'a> From<&Argument<'a>> for Piece<'a> {
    fn from(argument: &Argument<'a>) -> Self {
        Self::Source {
            spelling: argument.spelling,
            dynamic_order: Literal::read(argument).is_none().then_some(argument.order),
        }
    }
}

impl Piece<'_> {
    fn text(&self) -> &str {
        match self {
            Self::Generated(text) => text,
            Self::Source { spelling, .. } => spelling,
        }
    }
}

fn join(words: &[Piece<'_>]) -> Option<String> {
    let mut previous = None;
    for word in words {
        if let Piece::Source {
            dynamic_order: Some(order),
            ..
        } = word
        {
            if !crate::legacy_rules::spelling_follows(previous, *order) {
                return None;
            }
            previous = Some(*order);
        }
    }
    Some(words.iter().map(Piece::text).collect::<Vec<_>>().join(" "))
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
#[cfg(test)]
fn rewrite(next: &str, arguments: &[&str]) -> Rewrite {
    let spellings = arguments.iter().map(|word| quote(word)).collect::<Vec<_>>();
    let arguments = arguments
        .iter()
        .zip(&spellings)
        .enumerate()
        .map(|(order, (value, spelling))| Argument {
            value,
            spelling,
            order,
        })
        .collect::<Vec<_>>();
    rewrite_spelled(next, &arguments)
}

pub(super) fn rewrite_spelled(next: &str, arguments: &[Argument<'_>]) -> Rewrite {
    match next {
        "grep" => Rewrite(grep_to_rg(arguments).unwrap_or_else(|| {
            "Use `rg` with separate option values, the original expansion order, and shell quoting.".to_owned()
        })),
        "find" => Rewrite(find_to_fd(arguments).unwrap_or_else(|| {
            "Use `fd` with complete option values, the original expansion order, and shell quoting.".to_owned()
        })),
        text => Rewrite(
            text.replace(
                "{args}",
                &arguments
                    .iter()
                    .map(|argument| argument.spelling)
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
        ),
    }
}

/// The grep flags whose value follows them in the same word or the next.
const GREP_VALUES: &str = "ABCefmd";

fn grep_to_rg(arguments: &[Argument<'_>]) -> Option<String> {
    let mut out = vec![Piece::Generated("rg".to_owned())];
    let mut operands = Vec::new();
    let mut options = true;
    let mut arguments = arguments.iter();
    while let Some(word) = arguments.next() {
        let argument = word.value;
        if !options || !argument.starts_with('-') || argument == "-" {
            operands.push(Piece::from(word));
            continue;
        }
        if argument == "--" {
            options = false;
            operands.push(Piece::Generated("--".to_owned()));
            continue;
        }
        if let Some(long) = argument.strip_prefix("--") {
            let (name, value) = long.split_once('=').unwrap_or((long, ""));
            match name {
                "recursive" | "dereference-recursive" | "extended-regexp" => {
                    if long.contains('=') {
                        return None;
                    }
                }
                "include" | "exclude" | "exclude-dir" => {
                    let (prefix, suffix) = match name {
                        "include" => ("", ""),
                        "exclude" => ("!", ""),
                        _ => ("!", "/"),
                    };
                    let pattern = if long.contains('=') {
                        Literal::suffix_of(word, argument.len() - value.len())?
                            .rendered(prefix, suffix)
                            .into()
                    } else {
                        let value = arguments.next()?;
                        if name == "include" {
                            Piece::from(value)
                        } else {
                            Literal::read(value)?.rendered(prefix, suffix).into()
                        }
                    };
                    out.extend([Piece::Generated("-g".to_owned()), pattern]);
                }
                "no-filename" => {
                    if long.contains('=') {
                        return None;
                    }
                    out.push(Piece::Generated("-I".to_owned()));
                }
                _ => out.push(Piece::from(word)),
            }
            continue;
        }
        let mut cluster = argument.strip_prefix('-').unwrap_or_default().chars();
        while let Some(flag) = cluster.next() {
            match flag {
                'r' | 'R' | 'E' => {}
                'h' => out.push(Piece::Generated("-I".to_owned())),
                's' => out.push(Piece::Generated("--no-messages".to_owned())),
                'L' => out.push(Piece::Generated("--files-without-match".to_owned())),
                value if GREP_VALUES.contains(value) => {
                    out.push(Piece::Generated(format!("-{value}")));
                    let rest = cluster.as_str();
                    if rest.is_empty() {
                        Literal::read(word)?;
                        out.push(Piece::from(arguments.next()?));
                    } else {
                        out.push(
                            Literal::suffix_of(word, argument.len() - rest.len())?
                                .rendered("", "")
                                .into(),
                        );
                    }
                    break;
                }
                other => {
                    Literal::read(word)?;
                    out.push(Piece::Generated(format!("-{other}")));
                }
            }
        }
    }
    out.extend(operands);
    join(&out)
}

fn find_to_fd(arguments: &[Argument<'_>]) -> Option<String> {
    let paths: Vec<Piece<'_>> = arguments
        .iter()
        .take_while(|argument| {
            !argument.value.starts_with('-') && !matches!(argument.value, "(" | "!")
        })
        .map(Piece::from)
        .collect();
    let mut flags = Vec::new();
    let mut pattern = None;
    let mut exec = Vec::new();
    let mut untranslated = Vec::new();
    let mut arguments = arguments.iter().skip(paths.len()).peekable();
    while let Some(argument) = arguments.next() {
        let value = arguments.peek().map(|word| Piece::from(*word));
        if pattern.is_some()
            && matches!(argument.value, "-name" | "-iname" | "-path" | "-wholename")
        {
            return None;
        }
        let flag = match argument.value {
            "-name" => {
                pattern = Some(value?);
                None
            }
            "-iname" => {
                pattern = Some(value?);
                Some("--ignore-case")
            }
            "-path" | "-wholename" => {
                pattern = Some(value?);
                Some("--full-path")
            }
            "-type" | "-maxdepth" | "-mindepth" => {
                flags.push(
                    match argument.value {
                        "-type" => "--type",
                        "-maxdepth" => "--max-depth",
                        _ => "--min-depth",
                    }
                    .to_owned()
                    .into(),
                );
                flags.push(value?);
                None
            }
            "-exec" | "-execdir" => {
                let mut command = Vec::new();
                let mut terminated = false;
                for word in arguments.by_ref() {
                    if matches!(word.value, ";" | "+") {
                        exec.push(
                            if word.value == "+" {
                                "--exec-batch"
                            } else {
                                "--exec"
                            }
                            .to_owned()
                            .into(),
                        );
                        terminated = true;
                        break;
                    }
                    command.push(Piece::from(word));
                }
                if !terminated || command.is_empty() {
                    return None;
                }
                exec.extend(command);
                continue;
            }
            "-print" => continue,
            _ => {
                untranslated.push(argument.spelling.to_owned());
                continue;
            }
        };
        arguments.next();
        flags.extend(flag.map(|flag| Piece::Generated(flag.to_owned())));
    }
    let mut out = vec![Piece::Generated("fd".to_owned())];
    out.extend(flags);
    match pattern {
        Some(glob) => out.extend([Piece::Generated("--glob".to_owned()), glob]),
        None if !paths.is_empty() => out.push(Piece::Generated(".".to_owned())),
        None => {}
    }
    out.extend(paths);
    out.extend(exec);
    let command = join(&out)?;
    if untranslated.is_empty() {
        Some(command)
    } else {
        Some(format!(
            "{command}, and translate {} by hand from `fd --help`",
            untranslated.join(" ")
        ))
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

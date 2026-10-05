//! The one refusal format every gate in this repository emits.
//!
//! A refusal is printed as readable text followed by exactly one machine-readable line:
//!
//! ```text
//! dotfiles-refusal/1 {"rule":"...","cause":"...","evidence":["..."],"next":"...","waiver":null}
//! ```
//!
//! The line is the prefix and one compact JSON object with these five keys in this order.
//! `next` is a command to run; `<name>` marks a value only the author can supply.
//! `waiver` is the command that overrides the gate once and is recorded, or `null` when the gate cannot be waived.
//!
//! This file is std-only so `dotguard` and the xtask binaries compile the same source.

use std::fmt::{self, Write as _};

/// Starts the machine-readable line; the version changes only with an incompatible format.
pub const PREFIX: &str = "dotfiles-refusal/1 ";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// Stable dotted identifier of the gate rule, such as `git.force`.
    pub rule: String,
    /// Why the operation was refused.
    pub cause: String,
    /// The observed facts the refusal rests on.
    pub evidence: Vec<String>,
    /// The command to run next.
    pub next: String,
    /// The recorded one-time override, when the rule has one.
    pub waiver: Option<String>,
}

impl Refusal {
    pub fn new(rule: impl Into<String>, cause: impl Into<String>, next: impl Into<String>) -> Self {
        Self {
            rule: rule.into(),
            cause: cause.into(),
            evidence: Vec::new(),
            next: next.into(),
            waiver: None,
        }
    }

    #[must_use]
    pub fn evidence(mut self, item: impl Into<String>) -> Self {
        self.evidence.push(item.into());
        self
    }

    #[must_use]
    pub fn waiver(mut self, command: impl Into<String>) -> Self {
        self.waiver = Some(command.into());
        self
    }

    /// Whether every field an agent needs is present: a dotted rule, a cause, evidence, a one-line next command, and a one-line waiver when there is one.
    pub fn is_complete(&self) -> bool {
        let one_line = |text: &str| !text.trim().is_empty() && !text.contains(['\n', '\r']);
        !self.rule.is_empty()
            && self.rule.split(['.', '-']).all(|part| {
                !part.is_empty()
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
            })
            && !self.cause.trim().is_empty()
            && !self.evidence.is_empty()
            && self.evidence.iter().all(|item| !item.trim().is_empty())
            && one_line(&self.next)
            && self.waiver.as_deref().is_none_or(one_line)
    }

    /// The machine-readable line, without a trailing newline.
    pub fn line(&self) -> String {
        let mut out = String::from(PREFIX);
        out.push_str("{\"rule\":");
        string(&mut out, &self.rule);
        out.push_str(",\"cause\":");
        string(&mut out, &self.cause);
        out.push_str(",\"evidence\":[");
        for (index, item) in self.evidence.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            string(&mut out, item);
        }
        out.push_str("],\"next\":");
        string(&mut out, &self.next);
        out.push_str(",\"waiver\":");
        match &self.waiver {
            Some(waiver) => string(&mut out, waiver),
            None => out.push_str("null"),
        }
        out.push('}');
        out
    }

    /// The readable text, ending with a newline.
    pub fn text(&self) -> String {
        let mut out = format!(
            "::error:: {} refused: {}\n",
            self.rule,
            self.cause.trim_end()
        );
        out.push_str("\nEvidence:\n");
        for item in &self.evidence {
            for line in item.lines() {
                out.push_str("  ");
                out.push_str(line);
                out.push('\n');
            }
        }
        out.push_str("\nNext:\n  ");
        out.push_str(&self.next);
        out.push('\n');
        if let Some(waiver) = &self.waiver {
            out.push_str("\nWaiver:\n  ");
            out.push_str(waiver);
            out.push('\n');
        }
        out
    }

    /// The readable text followed by the machine-readable line.
    pub fn render(&self) -> String {
        format!("{}\n{}\n", self.text(), self.line())
    }

    /// Writes the rendered refusal to standard error.
    pub fn emit(&self) {
        eprint!("{}", self.render());
    }

    /// Reads a machine-readable line back, or `None` when it is not exactly this format.
    pub fn parse(line: &str) -> Option<Self> {
        let mut reader = Reader {
            rest: line.strip_prefix(PREFIX)?,
        };
        reader.literal("{\"rule\":")?;
        let rule = reader.string()?;
        reader.literal(",\"cause\":")?;
        let cause = reader.string()?;
        reader.literal(",\"evidence\":[")?;
        let mut evidence = Vec::new();
        if reader.literal("]").is_none() {
            loop {
                evidence.push(reader.string()?);
                if reader.literal("]").is_some() {
                    break;
                }
                reader.literal(",")?;
            }
        }
        reader.literal(",\"next\":")?;
        let next = reader.string()?;
        reader.literal(",\"waiver\":")?;
        let waiver = if reader.literal("null").is_some() {
            None
        } else {
            Some(reader.string()?)
        };
        reader.literal("}")?;
        reader.rest.is_empty().then_some(Self {
            rule,
            cause,
            evidence,
            next,
            waiver,
        })
    }

    /// The last machine-readable line in a gate's output.
    pub fn find(output: &str) -> Option<Self> {
        output.lines().rev().find_map(Self::parse)
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.cause)
    }
}

impl std::error::Error for Refusal {}

/// Quotes one argument for a POSIX shell, leaving plain words and `<name>` placeholders unquoted.
pub fn quote(argument: &str) -> String {
    let placeholder = argument
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
        .is_some_and(|name| {
            !name.is_empty()
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
        });
    if placeholder
        || !argument.is_empty()
            && argument
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"@%_+=:,./-".contains(&byte))
    {
        argument.to_owned()
    } else {
        format!("'{}'", argument.replace('\'', r"'\''"))
    }
}

/// Joins arguments into one shell command line.
pub fn command<S: AsRef<str>>(arguments: &[S]) -> String {
    arguments
        .iter()
        .map(|argument| quote(argument.as_ref()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn string(out: &mut String, value: &str) {
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if u32::from(control) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(control));
            }
            other => out.push(other),
        }
    }
    out.push('"');
}

struct Reader<'a> {
    rest: &'a str,
}

impl Reader<'_> {
    fn literal(&mut self, expected: &str) -> Option<()> {
        self.rest = self.rest.strip_prefix(expected)?;
        Some(())
    }

    fn string(&mut self) -> Option<String> {
        self.literal("\"")?;
        let mut out = String::new();
        let mut characters = self.rest.char_indices();
        while let Some((index, character)) = characters.next() {
            match character {
                '"' => {
                    self.rest = &self.rest[index + 1..];
                    return Some(out);
                }
                '\\' => {
                    let (_, escape) = characters.next()?;
                    out.push(match escape {
                        '"' => '"',
                        '\\' => '\\',
                        '/' => '/',
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        'b' => '\u{8}',
                        'f' => '\u{c}',
                        'u' => {
                            let mut code = 0;
                            for _ in 0..4 {
                                code = code * 16 + characters.next()?.1.to_digit(16)?;
                            }
                            char::from_u32(code)?
                        }
                        _ => return None,
                    });
                }
                control if u32::from(control) < 0x20 => return None,
                other => out.push(other),
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{PREFIX, Refusal, command, quote};

    fn sample() -> Refusal {
        Refusal::new(
            "git.force",
            "refusing `git reset --hard`.\nIt discards work.",
            "git stash push --include-untracked",
        )
        .evidence("argv: git reset --hard")
        .waiver("ALLOW_FORCE=1 git reset --hard")
    }

    #[test]
    fn the_line_is_one_line_with_the_five_fields_in_order() {
        let line = sample().line();
        assert!(!line.contains('\n'));
        assert_eq!(
            line,
            format!(
                "{PREFIX}{{\"rule\":\"git.force\",\"cause\":\"refusing `git reset --hard`.\\nIt discards work.\",\"evidence\":[\"argv: git reset --hard\"],\"next\":\"git stash push --include-untracked\",\"waiver\":\"ALLOW_FORCE=1 git reset --hard\"}}"
            )
        );
    }

    #[test]
    fn the_rendered_refusal_ends_with_its_machine_line() {
        let refusal = sample();
        let rendered = refusal.render();
        assert!(rendered.starts_with("::error:: git.force refused: refusing"));
        assert!(rendered.contains("\nNext:\n  git stash push --include-untracked\n"));
        assert!(rendered.contains("\nWaiver:\n  ALLOW_FORCE=1"));
        assert_eq!(Refusal::find(&rendered), Some(refusal));
    }

    #[test]
    fn every_character_survives_a_round_trip_on_one_line() {
        let all: String = (0..=0x10_FFFF).filter_map(char::from_u32).collect();
        let refusal = Refusal::new("a.b", all.clone(), "x").evidence(all);
        let line = refusal.line();
        assert_eq!(line.lines().count(), 1);
        assert!(!line.chars().any(|c| u32::from(c) < 0x20));
        assert_eq!(Refusal::parse(&line), Some(refusal));
    }

    #[test]
    fn a_missing_waiver_is_null_and_parses_back() {
        let refusal = Refusal::new("push.signature", "unsigned", "git log").evidence("abc");
        assert!(refusal.line().ends_with(",\"waiver\":null}"));
        assert_eq!(Refusal::parse(&refusal.line()), Some(refusal));
    }

    #[test]
    fn anything_but_the_exact_format_is_not_a_refusal() {
        let line = sample().line();
        assert_eq!(Refusal::parse(&line[1..]), None);
        assert_eq!(Refusal::parse(&format!("{line} ")), None);
        assert_eq!(Refusal::parse(&line.replace("\"rule\"", "\"kind\"")), None);
        assert_eq!(Refusal::parse(&format!("{PREFIX}{{}}")), None);
    }

    #[test]
    fn completeness_requires_every_field() {
        assert!(sample().is_complete());
        let mut incomplete = sample();
        incomplete.evidence.clear();
        assert!(!incomplete.is_complete());
        assert!(
            !Refusal {
                rule: "Git force".into(),
                ..sample()
            }
            .is_complete()
        );
        assert!(
            !Refusal {
                rule: "git..force".into(),
                ..sample()
            }
            .is_complete()
        );
        assert!(
            !Refusal {
                cause: " ".into(),
                ..sample()
            }
            .is_complete()
        );
        assert!(
            !Refusal {
                next: "a\nb".into(),
                ..sample()
            }
            .is_complete()
        );
        assert!(
            !Refusal {
                waiver: Some(String::new()),
                ..sample()
            }
            .is_complete()
        );
        assert!(
            Refusal {
                waiver: None,
                ..sample()
            }
            .is_complete()
        );
    }

    #[test]
    fn arguments_are_quoted_only_when_a_shell_would_split_or_expand_them() {
        assert_eq!(quote("refs/heads/main"), "refs/heads/main");
        assert_eq!(quote("a b"), "'a b'");
        assert_eq!(quote("it's"), r"'it'\''s'");
        assert_eq!(quote(""), "''");
        assert_eq!(quote("<body-file>"), "<body-file>");
        assert_eq!(quote("<a b>"), "'<a b>'");
        assert_eq!(quote("<>"), "'<>'");
        assert_eq!(
            command(&["git", "commit", "-m", "two words"]),
            "git commit -m 'two words'"
        );
    }
}

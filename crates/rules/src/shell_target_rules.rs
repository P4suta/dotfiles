//! Shell target selection and dialect-specific checking.

/// The dialect of a rendered shell target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Dialect {
    Sh,
    Bash,
    Zsh,
}

impl Dialect {
    /// The name `shellcheck --shell` takes and the parser that checks the syntax.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sh => "sh",
            Self::Bash => "bash",
            Self::Zsh => "zsh",
        }
    }
}

/// The evidence that selects a shell target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Evidence {
    /// The first line names the interpreter, which `shellcheck` reads itself.
    Shebang,
    /// A `# shellcheck shell=` directive names the dialect, which `shellcheck` reads itself.
    Directive,
    /// A startup file the shell sources by name, whose text names no dialect.
    StartupName,
    /// A `.sh` or `.bash` extension, whose text names no dialect.
    Extension,
}

/// A rendered shell target and its dialect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Selection {
    pub dialect: Dialect,
    pub evidence: Evidence,
}

impl Selection {
    /// Whether `shellcheck` needs `--shell`, because the text names no dialect.
    #[must_use]
    pub const fn needs_shell_flag(self) -> bool {
        matches!(self.evidence, Evidence::StartupName | Evidence::Extension)
    }
}

/// The checker that reads a shell target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Checker {
    /// `shellcheck` at style severity after the dialect's own parser.
    Shellcheck,
    /// `ShellCheck`'s SC1071 excludes zsh, so parsing and the startup ceiling stand in.
    ZshParse,
}

/// The checker of a dialect.
#[must_use]
pub const fn checker(dialect: Dialect) -> Checker {
    match dialect {
        Dialect::Sh | Dialect::Bash => Checker::Shellcheck,
        Dialect::Zsh => Checker::ZshParse,
    }
}

/// The dialect an interpreter or directive word names, such as `sh`, `dash`, `bash`, or `zsh`.
#[must_use]
pub fn dialect(word: &[u8]) -> Option<Dialect> {
    match word {
        b"sh" | b"dash" => Some(Dialect::Sh),
        b"bash" => Some(Dialect::Bash),
        b"zsh" => Some(Dialect::Zsh),
        _ => None,
    }
}

/// The interpreter a first line names, after `#!`, an optional `/usr/bin/env`, and the interpreter's directory, without its arguments.
#[must_use]
pub fn interpreter(first_line: &[u8]) -> Option<&[u8]> {
    let command = first_line.strip_prefix(b"#!")?;
    let command = command.strip_prefix(b"/usr/bin/env ").unwrap_or(command);
    let program = command.split(|byte| *byte == b' ').next()?;
    program.rsplit(|byte| *byte == b'/').next()
}

/// The startup files a shell sources by name, with their dialect.
const STARTUP: [(&[u8], Dialect); 10] = [
    (b".profile", Dialect::Sh),
    (b".bashrc", Dialect::Bash),
    (b".bash_profile", Dialect::Bash),
    (b".bash_login", Dialect::Bash),
    (b".bash_logout", Dialect::Bash),
    (b".zshrc", Dialect::Zsh),
    (b".zprofile", Dialect::Zsh),
    (b".zshenv", Dialect::Zsh),
    (b".zlogin", Dialect::Zsh),
    (b".zlogout", Dialect::Zsh),
];

/// Selects a target's dialect from its filename, interpreter, or leading directive.
/// A first line that names another interpreter, such as `nu`, keeps the target out even when its name or extension says shell.
#[must_use]
pub fn select(file_name: &[u8], first_line: &[u8], directive: &[u8]) -> Option<Selection> {
    if let Some(program) = interpreter(first_line) {
        return dialect(program).map(|dialect| Selection {
            dialect,
            evidence: Evidence::Shebang,
        });
    }
    if let Some(dialect) = dialect(directive) {
        return Some(Selection {
            dialect,
            evidence: Evidence::Directive,
        });
    }
    for (name, dialect) in STARTUP {
        if file_name == name {
            return Some(Selection {
                dialect,
                evidence: Evidence::StartupName,
            });
        }
    }
    let by_extension = if file_name.ends_with(b".sh") {
        Some(Dialect::Sh)
    } else if file_name.ends_with(b".bash") {
        Some(Dialect::Bash)
    } else {
        None
    };
    by_extension.map(|dialect| Selection {
        dialect,
        evidence: Evidence::Extension,
    })
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_select(file_name: &[u8], first_line: &[u8], directive: &[u8]) {
        let result = select(file_name, first_line, directive);
        if let Some(program) = interpreter(first_line) {
            assert_eq!(result.map(|selection| selection.dialect), dialect(program));
            assert!(result.is_none_or(|selection| selection.evidence == Evidence::Shebang));
        } else if let Some(named) = dialect(directive) {
            assert_eq!(
                result,
                Some(Selection {
                    dialect: named,
                    evidence: Evidence::Directive
                })
            );
        } else if let Some(selection) = result {
            assert!(selection.needs_shell_flag());
        }
        if let Some(selection) = result {
            assert_eq!(
                checker(selection.dialect) == Checker::ZshParse,
                selection.dialect == Dialect::Zsh
            );
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(12)]
    fn a_named_interpreter_decides_before_a_directive_and_a_name() {
        let line: [u8; 6] = kani::any();
        let directive: [u8; 3] = kani::any();
        let name: [u8; 4] = kani::any();
        spec_select(&name, &line, &directive);
        let result = select(&name, &line, &directive);
        if let Some(selection) = result {
            kani::cover!(selection.evidence == Evidence::Shebang);
            kani::cover!(selection.evidence == Evidence::Directive);
            kani::cover!(selection.evidence == Evidence::Extension);
        } else {
            kani::cover!(true);
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(15)]
    fn every_startup_file_is_selected_with_the_shell_flag() {
        let index: usize = kani::any();
        kani::assume(index < STARTUP.len());
        let Some((name, expected)) = STARTUP.get(index) else {
            return;
        };
        let result = select(name, b"", b"");
        assert_eq!(
            result,
            Some(Selection {
                dialect: *expected,
                evidence: Evidence::StartupName
            })
        );
        assert!(result.is_some_and(Selection::needs_shell_flag));
        kani::cover!(*expected == Dialect::Sh);
        kani::cover!(*expected == Dialect::Bash);
        kani::cover!(*expected == Dialect::Zsh);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_targets_are_selected_by_interpreter_directive_startup_name_or_extension() {
        let selected = |name: &[u8], line: &[u8], directive: &[u8]| {
            select(name, line, directive).map(|selection| (selection.dialect, selection.evidence))
        };
        assert_eq!(
            selected(b"git", b"#!/bin/sh", b""),
            Some((Dialect::Sh, Evidence::Shebang))
        );
        assert_eq!(
            selected(b"tool", b"#!/usr/bin/env bash", b""),
            Some((Dialect::Bash, Evidence::Shebang))
        );
        assert_eq!(
            selected(b"run.sh", b"#!/usr/bin/env nu", b""),
            None,
            "another interpreter wins over the extension"
        );
        assert_eq!(
            selected(b"x", b"# managed", b"bash"),
            Some((Dialect::Bash, Evidence::Directive))
        );
        assert_eq!(
            selected(b".bashrc", b"case $- in", b""),
            Some((Dialect::Bash, Evidence::StartupName))
        );
        assert_eq!(
            selected(b".profile", b"", b""),
            Some((Dialect::Sh, Evidence::StartupName))
        );
        assert_eq!(
            selected(b".zshrc", b"", b""),
            Some((Dialect::Zsh, Evidence::StartupName))
        );
        assert_eq!(
            selected(b"bashenv.sh", b"export A=1", b""),
            Some((Dialect::Sh, Evidence::Extension))
        );
        assert_eq!(selected(b"fzfrc", b"--height 40%", b""), None);
        assert_eq!(interpreter(b"#!/bin/bash -eu"), Some(&b"bash"[..]));
        assert_eq!(checker(Dialect::Zsh), Checker::ZshParse);
        assert_eq!(checker(Dialect::Sh), Checker::Shellcheck);
        assert_eq!(Dialect::Bash.name(), "bash");
        for (name, _) in STARTUP {
            for line in [&b""[..], b"#!/bin/zsh", b"#!/bin/sh", b"# x"] {
                for directive in [&b""[..], b"sh", b"ksh"] {
                    proofs::spec_select(name, line, directive);
                }
            }
        }
    }
}

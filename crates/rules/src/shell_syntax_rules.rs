extern crate alloc;
use self::alloc::string::String;
use self::alloc::vec::Vec;

/// Why the shared hook refuses a malformed shell command before it runs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reason {
    /// A quote opens and never closes, so the words after it change meaning.
    UnbalancedQuotes,
    /// A backslash escape removes a Windows path separator or hides an expansion without any error.
    BrokenWindowsPath,
    /// A `$` sits where expansion must happen but can't.
    FrozenExpansion,
    /// `ShellCheck` reported an error in the command.
    ShellCheck,
    /// The mandatory syntax validator couldn't inspect the command.
    ValidatorUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Admit,
    Refuse(Reason),
}

#[must_use]
pub fn verdict(unbalanced: bool, broken_path: bool, frozen: bool) -> Verdict {
    if unbalanced {
        Verdict::Refuse(Reason::UnbalancedQuotes)
    } else if broken_path {
        Verdict::Refuse(Reason::BrokenWindowsPath)
    } else if frozen {
        Verdict::Refuse(Reason::FrozenExpansion)
    } else {
        Verdict::Admit
    }
}

/// A command passes only after the repository parser and the mandatory validator both pass.
#[must_use]
pub const fn checked(parsed: Verdict, available: bool, errors: bool) -> Verdict {
    match parsed {
        Verdict::Refuse(reason) => Verdict::Refuse(reason),
        Verdict::Admit if !available => Verdict::Refuse(Reason::ValidatorUnavailable),
        Verdict::Admit if errors => Verdict::Refuse(Reason::ShellCheck),
        Verdict::Admit => Verdict::Admit,
    }
}

#[must_use]
pub const fn next_action(reason: Reason) -> &'static str {
    match reason {
        Reason::UnbalancedQuotes => {
            "Close the quote, or split the command so each part stands on its own."
        }
        Reason::BrokenWindowsPath => {
            "Write the Windows path with forward slashes (`D:/cargo/$name`), so no backslash escape can eat a separator or an expansion."
        }
        Reason::FrozenExpansion => {
            "Close the `${`, or use double quotes where the variable must expand."
        }
        Reason::ShellCheck => "Correct the ShellCheck error, then retry the command.",
        Reason::ValidatorUnavailable => "Run `mise install shellcheck`, then retry the command.",
    }
}

/// The heading a refusal names for each malformed form.
#[must_use]
pub const fn cause(reason: Reason) -> &'static str {
    match reason {
        Reason::UnbalancedQuotes => "a quote opens and never closes",
        Reason::BrokenWindowsPath => {
            "a backslash escape eats a Windows path separator or an expansion"
        }
        Reason::FrozenExpansion => "a `$` sits where expansion cannot happen",
        Reason::ShellCheck => "ShellCheck reported a syntax error",
        Reason::ValidatorUnavailable => "ShellCheck could not validate the command",
    }
}

/// Whether a text holds a Windows drive path such as `D:\cargo` or `C:/Users`.
///
/// The letter must stand alone: a colon after a word, as in `https://`, marks no drive.
///
/// Exempt as disproportionate: Kani spends past its budget on the string search of even a three-byte word.
/// Evidence: `crates/rules/src/shell_syntax_rules.rs::tests::drive_paths_are_recognized_in_every_quoting`.
/// Reconsider when the bolero pilot of issue 105 adopts fuzzing.
#[must_use]
pub fn windows_path(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let starts = matches!(chars.first(), Some(letter) if letter.is_ascii_alphabetic())
        && matches!(chars.get(1), Some(':'))
        && matches!(chars.get(DRIVE_SEPARATOR), Some('\\' | '/'));
    let inside = chars.windows(DRIVE_WINDOW).any(|window| {
        matches!(
            window,
            [before, letter, ':', separator]
                if !before.is_ascii_alphabetic()
                    && letter.is_ascii_alphabetic()
                    && matches!(separator, '\\' | '/')
        )
    });
    starts || inside
}

/// The escape letters a `$'…'` quote form interprets, each of which can remove a path separator.
const ANSI_C_ESCAPES: [char; 11] = ['n', 't', 'r', 'b', 'f', 'v', 'a', 'e', 'c', 'x', 'u'];

/// The position of the separator in a drive path such as `C:/x`: after the drive letter and the colon.
const DRIVE_SEPARATOR: usize = 2;

/// The characters a drive path takes inside a word: the boundary, the drive letter, the colon, and the separator.
const DRIVE_WINDOW: usize = 4;

/// The characters of a bare drive word such as `C:`, which a following escape would cut.
const DRIVE_WORD: usize = 2;

/// Whether a word starts a variable assignment, such as `DIR=` before a quoted value.
fn assignment_prefix(word: &str) -> bool {
    let Some((name, _)) = word.rsplit_once('=') else {
        return false;
    };
    !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.starts_with(|c: char| c.is_ascii_digit())
}

/// The malformed forms of one shell command: quoting that never closes, escapes that eat a Windows path, and expansions that can't happen.
///
/// Exempt as disproportionate: a shell parser over unbounded text needs fuzzing, which no verifier here owns yet.
/// Evidence: `crates/rules/src/shell_syntax_rules.rs::tests::the_incident_forms_are_refused`.
/// Reconsider when the bolero pilot of issue 105 adopts fuzzing.
#[must_use]
pub fn inspect(command: &str) -> Verdict {
    let mut unbalanced = false;
    let mut broken_path = false;
    let mut frozen = false;
    let mut single = false;
    let mut double = false;
    let mut ansi = false;
    let mut escaped = false;
    let mut drive_escape = false;
    let mut previous = '\0';
    let mut body = String::new();
    let mut word = String::new();
    let mut assigning = false;
    let chars: Vec<char> = command.chars().collect();
    for &c in &chars {
        if ansi {
            if c == '\'' && !escaped {
                ansi = false;
                broken_path |= windows_path(&body)
                    && body
                        .chars()
                        .zip(body.chars().skip(1))
                        .any(|(first, second)| first == '\\' && ANSI_C_ESCAPES.contains(&second));
                body.clear();
            } else {
                escaped = c == '\\' && !escaped;
                body.push(c);
            }
        } else if double {
            if c == '"' && !escaped {
                double = false;
                broken_path |= windows_path(&body) && body.contains("\\$");
                frozen |= body.contains("${") && !body.contains('}');
                frozen |= body.ends_with('$') && !body.ends_with("\\$");
                body.clear();
            } else {
                escaped = c == '\\' && !escaped;
                body.push(c);
            }
        } else if single {
            if c == '\'' {
                single = false;
                frozen |= assigning && windows_path(&body) && body.contains('$');
                body.clear();
            } else {
                body.push(c);
            }
        } else if escaped {
            escaped = false;
            broken_path |= drive_escape && c.is_ascii_alphanumeric();
            word.push(c);
        } else {
            match c {
                '\'' => {
                    if previous == '$' {
                        ansi = true;
                    } else {
                        assigning = assignment_prefix(&word);
                        single = true;
                    }
                    word.clear();
                }
                '"' => {
                    double = true;
                    word.clear();
                }
                '\\' => {
                    drive_escape = word.len() == DRIVE_WORD
                        && word.starts_with(|drive: char| drive.is_ascii_alphabetic())
                        && word.ends_with(':');
                    escaped = true;
                }
                ' ' | '\t' | '\r' | '\n' => word.clear(),
                _ => word.push(c),
            }
        }
        previous = c;
    }
    if single || double || ansi {
        unbalanced = true;
    }
    verdict(unbalanced, broken_path, frozen)
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_checked(refused: bool, available: bool, errors: bool) {
        let parsed = if refused {
            Verdict::Refuse(Reason::UnbalancedQuotes)
        } else {
            Verdict::Admit
        };
        let result = checked(parsed, available, errors);
        assert_eq!(result == Verdict::Admit, !refused && available && !errors);
        if refused {
            assert_eq!(result, parsed);
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    fn missing_or_failed_validators_never_admit_commands() {
        let refused: bool = kani::any();
        let available: bool = kani::any();
        let errors: bool = kani::any();
        spec_checked(refused, available, errors);
        kani::cover!(checked(Verdict::Admit, available, errors) == Verdict::Admit);
        kani::cover!(
            checked(Verdict::Admit, available, errors)
                == Verdict::Refuse(Reason::ValidatorUnavailable)
        );
        kani::cover!(
            checked(Verdict::Admit, available, errors) == Verdict::Refuse(Reason::ShellCheck)
        );
    }

    /// The verdict names the first malformed form by its precedence and each refusal names a next action.
    pub fn spec_verdict(unbalanced: bool, broken_path: bool, frozen: bool) {
        let result = verdict(unbalanced, broken_path, frozen);
        let expected = if unbalanced {
            Verdict::Refuse(Reason::UnbalancedQuotes)
        } else if broken_path {
            Verdict::Refuse(Reason::BrokenWindowsPath)
        } else if frozen {
            Verdict::Refuse(Reason::FrozenExpansion)
        } else {
            Verdict::Admit
        };
        assert_eq!(result, expected);
        if let Verdict::Refuse(reason) = result {
            assert!(!next_action(reason).is_empty() && !cause(reason).is_empty());
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    fn malformed_commands_are_never_admitted_and_every_refusal_names_a_next_action() {
        let unbalanced: bool = kani::any();
        let broken_path: bool = kani::any();
        let frozen: bool = kani::any();
        spec_verdict(unbalanced, broken_path, frozen);
        let result = verdict(unbalanced, broken_path, frozen);
        kani::cover!(result == Verdict::Admit);
        kani::cover!(result == Verdict::Refuse(Reason::UnbalancedQuotes));
        kani::cover!(result == Verdict::Refuse(Reason::BrokenWindowsPath));
        kani::cover!(result == Verdict::Refuse(Reason::FrozenExpansion));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REFUSED_QUOTES: Verdict = Verdict::Refuse(Reason::UnbalancedQuotes);
    const REFUSED_PATH: Verdict = Verdict::Refuse(Reason::BrokenWindowsPath);
    const REFUSED_FROZEN: Verdict = Verdict::Refuse(Reason::FrozenExpansion);

    #[test]
    fn both_parsers_must_pass() {
        for [refused, available, errors] in crate::domain::flags() {
            super::proofs::spec_checked(refused, available, errors);
        }
    }

    #[test]
    fn every_input_meets_the_verdict_specification() {
        for [unbalanced, broken_path, frozen] in crate::domain::flags() {
            super::proofs::spec_verdict(unbalanced, broken_path, frozen);
        }
        for (reason, start) in [
            (Reason::UnbalancedQuotes, "Close the quote"),
            (Reason::BrokenWindowsPath, "Write the Windows path"),
            (Reason::FrozenExpansion, "Close the `${"),
        ] {
            assert!(next_action(reason).starts_with(start), "{reason:?}");
        }
        for (reason, text) in [
            (Reason::UnbalancedQuotes, "never closes"),
            (Reason::BrokenWindowsPath, "eats a Windows path separator"),
            (Reason::FrozenExpansion, "cannot happen"),
        ] {
            assert!(cause(reason).contains(text), "{reason:?}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "shell text regressions allocate parsed command text")]
    fn the_incident_forms_are_refused() {
        for (command, expected) in [
            ("export CARGO_TARGET_DIR=\"D:\\cargo\\$k\"", REFUSED_PATH),
            ("echo $'C:\\new\\path'", REFUSED_PATH),
            ("echo D:\\cargo\\$k", REFUSED_PATH),
            (
                "cargo build 2>&1 | tee \"D:\\out\\$name.log\"",
                REFUSED_PATH,
            ),
            ("echo \"unterminated", REFUSED_QUOTES),
            ("echo 'never closed", REFUSED_QUOTES),
            ("echo $'open", REFUSED_QUOTES),
            ("echo \"${name\"", REFUSED_FROZEN),
            ("echo \"prefix $\"", REFUSED_FROZEN),
            ("export DIR='D:\\cargo$k'", REFUSED_FROZEN),
        ] {
            assert_eq!(inspect(command), expected, "{command}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "shell text regressions allocate parsed command text")]
    fn well_formed_commands_are_admitted() {
        for command in [
            r"echo $'line\n'",
            r"echo $'C:/new/path'",
            r"echo $'it\'s'",
            r#"echo "a\"b""#,
            r"echo 'D:\cargo$k'",
            r"echo a\b",
            r"echo 1:\x",
            r"echo ab\c",
            "just check",
            "cargo build 2>&1",
            "echo hi",
            "awk '{print $1}' data",
            "awk \"{print \\$1}\" data",
            "echo \"cost: \\$5\"",
            "echo 'D:\\cargo'",
            "echo \"D:/cargo/$name\"",
            "echo \"${name}\"",
            "git commit -m \"quotes: it's fine\"",
            "rg \"pattern\" src",
            "echo a; echo b",
            "MSYS_NO_PATHCONV=1 echo /c/Users",
            "",
        ] {
            assert_eq!(inspect(command), Verdict::Admit, "{command}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "drive text regressions allocate character windows")]
    fn drive_paths_are_recognized_in_every_quoting() {
        assert!(windows_path("D:\\cargo"));
        assert!(windows_path("C:/Users"));
        assert!(windows_path("prefix E:\\x suffix"));
        assert!(windows_path("run (F:/tools) now"));
        assert!(!windows_path("cargo"));
        assert!(!windows_path("https://example.com"));
        assert!(!windows_path("a:b"));
        assert!(!windows_path("lorem w:x/y"));
    }

    #[test]
    fn only_variable_names_open_an_assignment() {
        assert!(assignment_prefix("DIR="));
        assert!(assignment_prefix("A_1="));
        assert!(!assignment_prefix("echo "));
        assert!(!assignment_prefix("1X="));
        assert!(!assignment_prefix("A-B="));
        assert!(!assignment_prefix("plain"));
    }
}

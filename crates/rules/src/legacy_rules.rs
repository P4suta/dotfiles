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

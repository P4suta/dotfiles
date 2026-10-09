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

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

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
    fn conditional_flags_match_clusters_and_values_without_prefix_collisions() {
        assert!(invokes(&["file"], &[]));
        for argument in ["--lines", "--lines=1", "-l", "-lc"] {
            assert!(invokes(&[argument], &["--lines", "-l"]), "{argument}");
        }
        for argument in ["file", "--linesx", "--l", "-c", "--lc"] {
            assert!(!invokes(&[argument], &["--lines", "-l"]), "{argument}");
        }
    }
}

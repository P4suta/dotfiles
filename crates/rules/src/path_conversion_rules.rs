//! Git Bash rewrites an argument that looks like an absolute POSIX path into a Windows path before it starts a native program.
//! A domyjob command line carries arguments for another host, so the rewrite turns `/bin/pwd` into `C:/Program Files/Git/usr/bin/pwd`, which that host lacks.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Admit,
    Refuse,
}

/// Refuses a remote command line that the shell would rewrite.
/// That takes a shell that converts paths, a remote argument that looks like an absolute path, and no command that turned the conversion off.
#[must_use]
pub const fn verdict(converting_shell: bool, remote_path: bool, conversion_off: bool) -> Verdict {
    if converting_shell && remote_path && !conversion_off {
        Verdict::Refuse
    } else {
        Verdict::Admit
    }
}

pub const NEXT_ACTION: &str = "Prefix the command with `MSYS_NO_PATHCONV=1` so Git Bash passes the remote arguments unchanged, or run it from PowerShell.";

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_verdict(converting_shell: bool, remote_path: bool, conversion_off: bool) {
        let result = verdict(converting_shell, remote_path, conversion_off);
        assert_eq!(
            result == Verdict::Refuse,
            converting_shell && remote_path && !conversion_off
        );
        assert_eq!(verdict(false, remote_path, conversion_off), Verdict::Admit);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_a_converted_remote_path_is_refused() {
        let converting_shell: bool = kani::any();
        let remote_path: bool = kani::any();
        let conversion_off: bool = kani::any();
        spec_verdict(converting_shell, remote_path, conversion_off);
        let result = verdict(converting_shell, remote_path, conversion_off);
        kani::cover!(result == Verdict::Refuse);
        kani::cover!(result == Verdict::Admit && converting_shell && remote_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_input_meets_the_specification() {
        for [converting_shell, remote_path, conversion_off] in crate::domain::flags() {
            proofs::spec_verdict(converting_shell, remote_path, conversion_off);
        }
    }

    #[test]
    fn a_converted_remote_path_is_refused_unless_the_conversion_is_off() {
        assert_eq!(verdict(true, true, false), Verdict::Refuse);
        assert_eq!(verdict(true, true, true), Verdict::Admit);
        assert_eq!(verdict(true, false, false), Verdict::Admit);
        assert_eq!(verdict(false, true, false), Verdict::Admit);
        assert!(NEXT_ACTION.contains("MSYS_NO_PATHCONV=1"));
    }
}

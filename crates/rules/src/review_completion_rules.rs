//! A reserved or skipped review supplies no completed review evidence.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Process {
    Success,
    Failed,
    Interrupted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Outcome {
    Legacy,
    Completed,
    Warnings,
    Failed,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Completion {
    pub process: Process,
    pub contexts: u64,
    pub completions: u64,
    pub rejected: bool,
    pub outcome: Outcome,
    pub unreviewed_files: Option<u64>,
    pub findings_match: bool,
}

#[must_use]
pub fn completed(evidence: Completion) -> bool {
    evidence.process == Process::Success
        && evidence.contexts == 1
        && evidence.completions == 1
        && !evidence.rejected
        && matches!(
            evidence.outcome,
            Outcome::Legacy | Outcome::Completed | Outcome::Warnings
        )
        && matches!(evidence.unreviewed_files, None | Some(0))
        && evidence.findings_match
}

#[cfg(kani)]
mod proofs {
    use super::*;

    #[kani::proof]
    fn completion_requires_the_original_success_and_complete_nonfailed_scope() {
        let evidence: Completion = kani::any();
        if completed(evidence) {
            assert_eq!(evidence.process, Process::Success);
            assert_eq!(evidence.contexts, 1);
            assert_eq!(evidence.completions, 1);
            assert!(!evidence.rejected);
            assert!(matches!(
                evidence.outcome,
                Outcome::Legacy | Outcome::Completed | Outcome::Warnings
            ));
            assert!(matches!(evidence.unreviewed_files, None | Some(0)));
            assert!(evidence.findings_match);
        }
        kani::cover!(completed(evidence));
        kani::cover!(!completed(evidence));
    }

    #[kani::proof]
    fn every_successful_supported_outcome_without_unreviewed_files_is_admitted() {
        let process: Process = kani::any();
        let outcome: Outcome = kani::any();
        let unreviewed_files: Option<u64> = kani::any();
        kani::assume(matches!(
            outcome,
            Outcome::Legacy | Outcome::Completed | Outcome::Warnings
        ));
        kani::assume(matches!(unreviewed_files, None | Some(0)));
        let evidence = Completion {
            process,
            contexts: 1,
            completions: 1,
            rejected: false,
            outcome,
            unreviewed_files,
            findings_match: true,
        };
        assert_eq!(completed(evidence), process == Process::Success);
        kani::cover!(completed(evidence));
        kani::cover!(!completed(evidence));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skipped_failed_partial_and_reserved_only_reviews_stay_incomplete() {
        let valid = Completion {
            process: Process::Success,
            contexts: 1,
            completions: 1,
            rejected: false,
            outcome: Outcome::Legacy,
            unreviewed_files: None,
            findings_match: true,
        };
        assert!(completed(valid));
        assert!(completed(Completion {
            outcome: Outcome::Warnings,
            unreviewed_files: Some(0),
            ..valid
        }));
        for invalid in [
            Completion {
                process: Process::Failed,
                ..valid
            },
            Completion {
                process: Process::Interrupted,
                ..valid
            },
            Completion {
                contexts: 0,
                ..valid
            },
            Completion {
                contexts: 2,
                ..valid
            },
            Completion {
                completions: 0,
                ..valid
            },
            Completion {
                completions: 2,
                ..valid
            },
            Completion {
                rejected: true,
                ..valid
            },
            Completion {
                outcome: Outcome::Failed,
                ..valid
            },
            Completion {
                outcome: Outcome::Unknown,
                ..valid
            },
            Completion {
                unreviewed_files: Some(1),
                ..valid
            },
            Completion {
                findings_match: false,
                ..valid
            },
        ] {
            assert!(!completed(invalid));
        }
    }
}

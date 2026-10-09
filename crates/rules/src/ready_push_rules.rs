/// The open PR with the pushed branch as head, as GitHub reports it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pr {
    Absent,
    Draft,
    Ready,
    /// GitHub gave no answer, so nothing shows that the branch lacks a ready PR.
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Admit,
    /// A ready PR takes no more pushes until it returns to draft.
    ReturnToDraft,
    /// The push must wait until the check establishes the PR state.
    Inspect,
}

/// The gate admits a push to a GitHub branch only when that branch has no ready PR.
/// It doesn't judge pushes elsewhere.
#[must_use]
pub const fn push(gated: bool, pr: Pr) -> Verdict {
    if !gated {
        return Verdict::Admit;
    }
    match pr {
        Pr::Absent | Pr::Draft => Verdict::Admit,
        Pr::Ready => Verdict::ReturnToDraft,
        Pr::Unknown => Verdict::Inspect,
    }
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub const PRS: [Pr; 4] = [Pr::Absent, Pr::Draft, Pr::Ready, Pr::Unknown];

    pub fn spec_push(gated: bool, pr: Pr) {
        let verdict = push(gated, pr);
        assert_eq!(verdict == Verdict::ReturnToDraft, gated && pr == Pr::Ready);
        assert_eq!(verdict == Verdict::Inspect, gated && pr == Pr::Unknown);
        assert_eq!(
            verdict == Verdict::Admit,
            !gated || matches!(pr, Pr::Absent | Pr::Draft)
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_ready_pr_never_takes_a_push_and_only_a_draft_absent_or_ungated_push_is_admitted() {
        let gated: bool = kani::any();
        let pr = PRS[usize::from(kani::any::<u8>() % 4)];
        spec_push(gated, pr);
        let verdict = push(gated, pr);
        kani::cover!(verdict == Verdict::ReturnToDraft);
        kani::cover!(verdict == Verdict::Inspect);
        kani::cover!((verdict == Verdict::Admit) & gated);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_input_meets_the_specification() {
        for gated in [false, true] {
            for pr in proofs::PRS {
                proofs::spec_push(gated, pr);
            }
        }
    }

    #[test]
    fn a_push_to_a_ready_pr_is_refused_until_it_is_a_draft_again() {
        assert_eq!(push(true, Pr::Ready), Verdict::ReturnToDraft);
        assert_eq!(push(true, Pr::Draft), Verdict::Admit);
        assert_eq!(push(true, Pr::Absent), Verdict::Admit);
    }

    #[test]
    fn an_unknown_state_is_inspected_and_ungated_pushes_are_admitted() {
        assert_eq!(push(true, Pr::Unknown), Verdict::Inspect);
        for pr in [Pr::Absent, Pr::Draft, Pr::Ready, Pr::Unknown] {
            assert_eq!(push(false, pr), Verdict::Admit);
        }
    }
}

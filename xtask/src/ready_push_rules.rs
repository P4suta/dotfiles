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

#[cfg(kani)]
fn any_pr() -> Pr {
    match kani::any::<u8>() % 4 {
        0 => Pr::Absent,
        1 => Pr::Draft,
        2 => Pr::Ready,
        _ => Pr::Unknown,
    }
}

#[cfg(kani)]
#[kani::proof]
fn a_ready_pr_never_takes_a_push() {
    let gated: bool = kani::any();
    let pr = any_pr();
    let verdict = push(gated, pr);
    if gated && pr == Pr::Ready {
        assert_eq!(verdict, Verdict::ReturnToDraft);
    }
    if gated && pr == Pr::Unknown {
        assert_eq!(verdict, Verdict::Inspect);
    }
    kani::cover!(verdict == Verdict::ReturnToDraft);
    kani::cover!(verdict == Verdict::Inspect);
}

#[cfg(kani)]
#[kani::proof]
fn a_draft_or_absent_pr_and_ungated_pushes_are_admitted() {
    let gated: bool = kani::any();
    let pr = any_pr();
    let verdict = push(gated, pr);
    assert_eq!(
        verdict == Verdict::Admit,
        !gated || matches!(pr, Pr::Absent | Pr::Draft)
    );
    kani::cover!(verdict == Verdict::Admit && gated);
    kani::cover!(verdict == Verdict::Admit && !gated);
}

#[cfg(test)]
mod tests {
    use super::*;

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

//! Which follow-up a refused push or commit suggests, kept free of I/O so Kani checks every case.

/// How to sign the unsigned commits a push would publish.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signing {
    /// The pushed ref is the checked-out one and its only unsigned commit is the tip: amend it.
    Amend,
    /// One branch carries a linear unsigned range: rebase that branch onto the range's base and sign each commit.
    Rebase,
    /// No single command signs exactly the unsigned commits: list them first.
    Inspect,
}

/// What a pushed ref is, as far as signing its commits is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reference {
    /// The local branch `HEAD` points at.
    CheckedOutBranch,
    /// `HEAD` itself.
    Head,
    /// Another local branch.
    Branch,
    /// A tag or any other ref.
    Other,
}

impl Reference {
    /// An amend moves only the checked-out ref.
    pub fn checked_out(self) -> bool {
        matches!(self, Self::CheckedOutBranch | Self::Head)
    }

    /// A rebase needs a branch to rewrite.
    pub fn branch(self) -> bool {
        matches!(self, Self::CheckedOutBranch | Self::Branch)
    }
}

/// `refs` counts the pushed refs that carry unsigned commits.
/// `reference` is that ref, `merges` says its pushed range contains a merge, and `only_tip` that its only unsigned commit is the checked-out commit.
pub fn signing(refs: usize, reference: Reference, merges: bool, only_tip: bool) -> Signing {
    if refs != 1 {
        Signing::Inspect
    } else if reference.checked_out() && only_tip {
        Signing::Amend
    } else if reference.branch() && !merges {
        Signing::Rebase
    } else {
        Signing::Inspect
    }
}

/// The step a refused history change suggests, chosen by the worst refused update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum History {
    /// The remote commit is not in this clone: fetch before anything else can be judged.
    Fetch,
    /// A rewrite: integrate the remote history instead of replacing it.
    Integrate,
    /// Only deletions: delete through the forge, which keeps the ref recoverable.
    ForgeDelete,
}

pub fn history(undecidable: bool, rewrite: bool) -> History {
    if undecidable {
        History::Fetch
    } else if rewrite {
        History::Integrate
    } else {
        History::ForgeDelete
    }
}

#[cfg(kani)]
#[kani::proof]
fn a_rebase_signs_one_linear_branch_and_an_amend_only_the_checked_out_tip() {
    let refs: usize = kani::any();
    let reference = match kani::any::<u8>() {
        0 => Reference::CheckedOutBranch,
        1 => Reference::Head,
        2 => Reference::Branch,
        _ => Reference::Other,
    };
    let checked_out = matches!(reference, Reference::CheckedOutBranch | Reference::Head);
    let branch = matches!(reference, Reference::CheckedOutBranch | Reference::Branch);
    let merges: bool = kani::any();
    let only_tip: bool = kani::any();
    let plan = signing(refs, reference, merges, only_tip);
    assert_eq!(plan == Signing::Amend, refs == 1 && checked_out && only_tip);
    assert_eq!(
        plan == Signing::Rebase,
        refs == 1 && !(checked_out && only_tip) && branch && !merges
    );
    assert!(refs == 1 || plan == Signing::Inspect);
    kani::cover!(plan == Signing::Amend);
    kani::cover!(plan == Signing::Rebase && only_tip && !checked_out);
    kani::cover!(
        plan == Signing::Inspect && refs == 1 && only_tip && reference == Reference::Other
    );
    kani::cover!(plan == Signing::Rebase);
    kani::cover!(plan == Signing::Inspect && refs == 1 && merges);
    kani::cover!(plan == Signing::Inspect && refs > 1);
}

#[cfg(kani)]
#[kani::proof]
fn a_history_refusal_suggests_the_step_for_its_worst_update() {
    let undecidable: bool = kani::any();
    let rewrite: bool = kani::any();
    let step = history(undecidable, rewrite);
    assert_eq!(step == History::Fetch, undecidable);
    assert_eq!(step == History::Integrate, !undecidable && rewrite);
    assert_eq!(step == History::ForgeDelete, !undecidable && !rewrite);
    kani::cover!(step == History::Fetch && rewrite);
    kani::cover!(step == History::Integrate);
    kani::cover!(step == History::ForgeDelete);
}

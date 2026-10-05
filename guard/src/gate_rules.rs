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

/// What a long option's name means among the spellings a Git command accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spelling {
    /// The spelling at this index, written in full or as its only completion.
    Is(usize),
    /// A prefix of several spellings and none in full: Git refuses the command.
    Ambiguous,
    /// No spelling starts with it.
    Unknown,
}

/// Reads `name`, a long option without its `--` and `=value`, as Git does: an exact spelling first, otherwise a unique prefix.
pub fn spelling(name: &[u8], spellings: &[&[u8]]) -> Spelling {
    let mut found = Spelling::Unknown;
    let mut index = 0;
    while index < spellings.len() {
        let candidate = spellings[index];
        if candidate == name {
            return Spelling::Is(index);
        }
        if candidate.starts_with(name) {
            found = match found {
                Spelling::Unknown => Spelling::Is(index),
                _ => Spelling::Ambiguous,
            };
        }
        index += 1;
    }
    found
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(4)]
fn a_long_option_is_its_exact_spelling_or_its_only_completion() {
    let first = [kani::any(), kani::any()];
    let second = [kani::any(), kani::any()];
    let third = [kani::any(), kani::any()];
    let word: [u8; 2] = [kani::any(), kani::any()];
    let first_length: usize = kani::any();
    let second_length: usize = kani::any();
    let third_length: usize = kani::any();
    let length: usize = kani::any();
    kani::assume(length <= 2 && first_length <= 2 && second_length <= 2 && third_length <= 2);
    let name = &word[..length];
    let spellings: [&[u8]; 3] = [
        &first[..first_length],
        &second[..second_length],
        &third[..third_length],
    ];
    let prefix_0 = spellings[0].starts_with(name);
    let prefix_1 = spellings[1].starts_with(name);
    let prefix_2 = spellings[2].starts_with(name);
    let completions = usize::from(prefix_0) + usize::from(prefix_1) + usize::from(prefix_2);
    let first_exact = if spellings[0] == name {
        Some(0)
    } else if spellings[1] == name {
        Some(1)
    } else if spellings[2] == name {
        Some(2)
    } else {
        None
    };
    let read = spelling(name, &spellings);
    match (first_exact, read) {
        (Some(index), _) => assert_eq!(read, Spelling::Is(index)),
        (None, Spelling::Is(index)) => assert!(
            completions == 1
                && (index == 0 && prefix_0 || index == 1 && prefix_1 || index == 2 && prefix_2)
        ),
        (None, Spelling::Ambiguous) => assert!(completions > 1),
        (None, Spelling::Unknown) => assert_eq!(completions, 0),
    }
    kani::cover!(first_exact.is_some() && completions > 1);
    kani::cover!(first_exact.is_none() && matches!(read, Spelling::Is(_)));
    kani::cover!(read == Spelling::Ambiguous);
    kani::cover!(read == Spelling::Unknown);
}

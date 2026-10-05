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

/// Whether a rebase may rewrite a commit a remote-tracking ref holds.
/// `control` is a `--continue`, `--abort`, or another step of a rebase already started, and `rewrites` is `None` when the gate could not tell.
pub fn rebase_refused(control: bool, rewrites: Option<bool>) -> bool {
    !control && rewrites != Some(false)
}

/// How a refused rewrite of published history may still run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    /// The stack tooling runs it while it holds the repository's stack lease.
    Tooling,
    /// The owner's waiver, recorded in the bypass log.
    Waived,
    Refused,
}

/// `stack_rule` says the refusal is a rebase or a leased force push, the only rewrites the stack tooling makes.
pub fn admission(stack_rule: bool, tooling: bool, waived: bool) -> Admission {
    if stack_rule && tooling {
        Admission::Tooling
    } else if waived {
        Admission::Waived
    } else {
        Admission::Refused
    }
}

/// The stack tooling holds the lease when the token it exported matches the lease file it wrote in the repository.
pub fn stack_tooling(token: Option<&[u8]>, lease: Option<&[u8]>) -> bool {
    matches!((token, lease), (Some(token), Some(lease)) if !token.is_empty() && token == lease)
}

#[cfg(kani)]
#[kani::proof]
fn a_rebase_is_refused_whenever_it_may_rewrite_a_published_commit() {
    let control: bool = kani::any();
    let rewrites: Option<bool> = kani::any();
    let refused = rebase_refused(control, rewrites);
    assert_eq!(refused, !control && rewrites != Some(false));
    assert!(control || rewrites == Some(false) || refused);
    kani::cover!(refused && rewrites.is_none());
    kani::cover!(refused && rewrites == Some(true));
    kani::cover!(!refused && control && rewrites == Some(true));
    kani::cover!(!refused && rewrites == Some(false));
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(3)]
fn a_published_rewrite_runs_only_through_the_leased_stack_tooling_or_a_waiver() {
    let token: [u8; 2] = kani::any();
    let lease: [u8; 2] = kani::any();
    let token_length: usize = kani::any();
    let lease_length: usize = kani::any();
    kani::assume(token_length <= 2 && lease_length <= 2);
    let token = if kani::any() {
        Some(&token[..token_length])
    } else {
        None
    };
    let lease = if kani::any() {
        Some(&lease[..lease_length])
    } else {
        None
    };
    let tooling = stack_tooling(token, lease);
    assert_eq!(
        tooling,
        token.is_some_and(|token| !token.is_empty() && Some(token) == lease)
    );
    let stack_rule: bool = kani::any();
    let waived: bool = kani::any();
    let admitted = admission(stack_rule, tooling, waived);
    assert_eq!(admitted == Admission::Tooling, stack_rule && tooling);
    assert_eq!(
        admitted == Admission::Refused,
        !waived && !(stack_rule && tooling)
    );
    kani::cover!(admitted == Admission::Tooling);
    kani::cover!(admitted == Admission::Waived && !stack_rule);
    kani::cover!(
        admitted == Admission::Refused && stack_rule && token.is_some() && lease.is_none()
    );
    kani::cover!(admitted == Admission::Refused && tooling && !stack_rule);
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

/// What a gate does with a command line, given whether a rule refuses it and whether every option was read as Git reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    /// Refuse and suggest the same command with the refused part changed.
    Suggest,
    /// Refuse without a suggestion: a command rebuilt from words the gate did not read could do something else.
    Withhold,
}

pub fn verdict(refused: bool, read: bool) -> Verdict {
    if !refused {
        Verdict::Allow
    } else if read {
        Verdict::Suggest
    } else {
        Verdict::Withhold
    }
}

#[cfg(kani)]
#[kani::proof]
fn a_suggestion_is_rebuilt_only_from_a_line_read_in_full() {
    let refused: bool = kani::any();
    let read: bool = kani::any();
    let verdict = verdict(refused, read);
    assert_eq!(verdict == Verdict::Allow, !refused);
    assert_eq!(verdict == Verdict::Suggest, refused && read);
    assert_eq!(verdict == Verdict::Withhold, refused && !read);
    kani::cover!(verdict == Verdict::Suggest);
    kani::cover!(verdict == Verdict::Withhold);
    kani::cover!(verdict == Verdict::Allow && !read);
}

/// How `git commit --fixup` was given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fixup {
    Absent,
    /// `--fixup=<commit>`.
    Plain,
    /// `--fixup=amend:<commit>`, which may be empty.
    Amend,
    /// `--fixup=reword:<commit>`, which ignores staged changes and is empty.
    Reword,
}

/// Where the refused commit took its message and authorship from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Source {
    pub fixup: Fixup,
    /// `-C` or `-c` named a commit.
    pub reuse: bool,
    /// `--reset-author` was in effect.
    pub renew: bool,
    /// `--amend` was in effect.
    pub amend: bool,
}

/// What a retry from the saved message adds so that it makes the commit the refused one would have made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is one independent option the retry adds"
)]
pub struct Retry {
    pub allow_empty: bool,
    pub only: bool,
    /// The author and date of the commit whose message `-C` or `-c` reused.
    pub authorship: bool,
    /// The `--reset-author` options stay, which Git accepts only with `-C`, `-c`, `--amend`, or during a pick.
    pub reset_author: bool,
}

/// Why no retry repeats a refused commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Withheld {
    /// The reused commit's authorship could not be read.
    Authorship,
    /// Whether a pick was in progress, which decides whose authorship a reuse takes, could not be read.
    Picking,
}

/// The retry of a refused commit.
/// `picking` says a cherry-pick or rebase pick was in progress, when the gate could tell, and `known` that the gate read the reused commit's authorship.
/// During a pick Git takes the authorship from the picked commit unless `--reset-author` is given, whatever `-C` or `-c` named.
///
/// # Errors
///
/// The authorship a retry must name, or whether a pick decides it, is unknown.
pub fn commit_retry(source: Source, picking: Option<bool>, known: bool) -> Result<Retry, Withheld> {
    let picking = match picking {
        Some(picking) => picking,
        None if source.reuse => return Err(Withheld::Picking),
        None => false,
    };
    let authorship = source.reuse && !source.renew && !picking;
    if authorship && !known {
        return Err(Withheld::Authorship);
    }
    Ok(Retry {
        allow_empty: matches!(source.fixup, Fixup::Amend | Fixup::Reword),
        only: source.fixup == Fixup::Reword,
        authorship,
        // Without the reuse and `--amend` the author is the committer, which is what the reset made it.
        reset_author: !source.reuse || !source.renew || source.amend || picking,
    })
}

#[cfg(kani)]
#[kani::proof]
fn a_commit_retry_keeps_what_its_message_source_implied() {
    let fixup = match kani::any::<u8>() {
        0 => Fixup::Absent,
        1 => Fixup::Plain,
        2 => Fixup::Amend,
        _ => Fixup::Reword,
    };
    let source = Source {
        fixup,
        reuse: kani::any(),
        renew: kani::any(),
        amend: kani::any(),
    };
    let picking: Option<bool> = kani::any();
    let known: bool = kani::any();
    let retry = commit_retry(source, picking, known);
    if source.reuse && picking.is_none() {
        assert_eq!(retry, Err(Withheld::Picking));
    }
    let takes = source.reuse && !source.renew && picking == Some(false);
    assert_eq!(retry == Err(Withheld::Authorship), takes && !known);
    if let Ok(retry) = retry {
        assert_eq!(
            retry.allow_empty,
            fixup == Fixup::Amend || fixup == Fixup::Reword
        );
        assert_eq!(retry.only, fixup == Fixup::Reword);
        assert_eq!(retry.authorship, takes);
        // A kept reset is one Git accepts, and a dropped one leaves the committer as the author.
        if retry.reset_author && source.renew {
            assert!(source.amend || picking == Some(true) || !source.reuse);
        }
        if !retry.reset_author {
            assert!(source.renew && !source.amend && !retry.authorship);
        }
    }
    kani::cover!(retry == Err(Withheld::Picking));
    kani::cover!(retry == Err(Withheld::Authorship));
    kani::cover!(retry.is_ok_and(|retry| retry.only && retry.allow_empty));
    kani::cover!(retry.is_ok_and(|retry| retry.authorship));
    kani::cover!(retry.is_ok_and(|retry| !retry.reset_author));
    kani::cover!(retry.is_ok_and(|retry| retry.reset_author && source.reuse && source.renew));
}

#[cfg(test)]
mod tests {
    use super::{Admission, admission, rebase_refused, stack_tooling};

    #[test]
    fn a_rebase_is_refused_unless_it_continues_or_keeps_published_commits() {
        assert!(rebase_refused(false, Some(true)));
        assert!(rebase_refused(false, None));
        assert!(!rebase_refused(false, Some(false)));
        assert!(!rebase_refused(true, Some(true)));
    }

    #[test]
    fn only_the_leased_stack_tooling_or_a_waiver_admits_a_published_rewrite() {
        let leased = stack_tooling(Some(b"token"), Some(b"token"));
        assert!(leased);
        assert!(!stack_tooling(Some(b"token"), None));
        assert!(!stack_tooling(None, Some(b"token")));
        assert!(!stack_tooling(Some(b""), Some(b"")));
        assert!(!stack_tooling(Some(b"token"), Some(b"other")));
        assert_eq!(admission(true, leased, false), Admission::Tooling);
        assert_eq!(admission(false, leased, false), Admission::Refused);
        assert_eq!(admission(false, leased, true), Admission::Waived);
        assert_eq!(admission(true, false, true), Admission::Waived);
        assert_eq!(admission(true, false, false), Admission::Refused);
    }
}

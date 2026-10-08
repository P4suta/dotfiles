//! Which follow-up a refused push or commit suggests, kept free of I/O so Kani checks every case.

/// How to sign the unsigned commits a push would publish.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signing {
    /// The pushed ref has the checked-out commit as its only unsigned commit: amend it.
    Amend,
    /// One branch carries a linear unsigned range: rebase that branch onto the range's base and sign each commit.
    Rebase,
    /// No single command signs exactly the unsigned commits: list them first.
    Inspect,
}

/// The facts about a pushed ref that decide how to sign its commits.
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
    #[must_use]
    pub fn checked_out(self) -> bool {
        matches!(self, Self::CheckedOutBranch | Self::Head)
    }

    /// A rebase needs a branch to rewrite.
    #[must_use]
    pub fn branch(self) -> bool {
        matches!(self, Self::CheckedOutBranch | Self::Branch)
    }
}

/// `refs` counts the pushed refs that carry unsigned commits.
/// `reference` names that ref, `merges` says its pushed range contains a merge, and `only_tip` says its only unsigned commit sits at the checked-out commit.
#[must_use]
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
    /// This clone lacks the remote commit, so fetch before judging anything else.
    Fetch,
    /// A rewrite: integrate the remote history instead of replacing it.
    Integrate,
    /// Only deletions: delete through the forge, which keeps the ref recoverable.
    ForgeDelete,
}

#[must_use]
pub fn history(undecidable: bool, rewrite: bool) -> History {
    if undecidable {
        History::Fetch
    } else if rewrite {
        History::Integrate
    } else {
        History::ForgeDelete
    }
}

/// Whether a rebase may rewrite a commit a remote-tracking ref holds.
/// `control` marks a `--continue`, `--abort`, or another step of a rebase already started.
/// `rewrites` holds `None` when the gate couldn't tell.
#[must_use]
pub fn rebase_refused(control: bool, rewrites: Option<bool>) -> bool {
    !control && rewrites != Some(false)
}

/// How a refused rewrite of published history may still run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    /// The stack tooling runs it while it holds the repository's stack lease.
    Tooling,
    /// The owner's bypass, recorded in the bypass log.
    Waived,
    Refused,
}

/// `stack_rule` says the refusal concerns a rebase or a leased force push, the only rewrites the stack tooling makes.
#[must_use]
pub fn admission(stack_rule: bool, tooling: bool, bypassed: bool) -> Admission {
    if stack_rule && tooling {
        Admission::Tooling
    } else if bypassed {
        Admission::Waived
    } else {
        Admission::Refused
    }
}

/// The stack tooling holds the lease when the token it exported matches the lease file it wrote in the repository.
#[must_use]
pub fn stack_tooling(token: Option<&[u8]>, lease: Option<&[u8]>) -> bool {
    matches!((token, lease), (Some(token), Some(lease)) if !token.is_empty() && token == lease)
}

/// What a long option's name means among the spellings a Git command accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spelling {
    /// The spelling at this index, written in full, or as its only completion.
    Is(usize),
    /// A prefix of more than one spelling and none in full: Git refuses the command.
    Ambiguous,
    /// No spelling starts with it.
    Unknown,
}

/// Reads `name`, a long option without its `--` and `=value`, as Git does: an exact spelling first, otherwise a unique prefix.
#[must_use]
pub fn spelling(name: &[u8], spellings: &[&[u8]]) -> Spelling {
    let mut found = Spelling::Unknown;
    let mut rest = spellings;
    let mut index = 0;
    while let [candidate, tail @ ..] = rest {
        if *candidate == name {
            return Spelling::Is(index);
        }
        if candidate.starts_with(name) {
            found = match found {
                Spelling::Unknown => Spelling::Is(index),
                _ => Spelling::Ambiguous,
            };
        }
        rest = tail;
        index += 1;
    }
    found
}

/// What a gate does with a command line, given whether a rule refuses it and whether the gate read every option as Git reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    /// Refuse and suggest the same command with the refused part changed.
    Suggest,
    /// Refuse without a suggestion, because a command rebuilt from words the gate didn't read could do something else.
    Withhold,
}

#[must_use]
pub fn verdict(refused: bool, read: bool) -> Verdict {
    if !refused {
        Verdict::Allow
    } else if read {
        Verdict::Suggest
    } else {
        Verdict::Withhold
    }
}

/// How the command line gave `git commit --fixup`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fixup {
    Absent,
    /// `--fixup=<commit>`.
    Plain,
    /// `--fixup=amend:<commit>`, which may have no changes.
    Amend,
    /// `--fixup=reword:<commit>`, which ignores staged changes and has none of its own.
    Reword,
}

/// Where the refused commit took its message and authorship from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Source {
    pub fixup: Fixup,
    /// `-C` or `-c` named a commit.
    pub reuse: bool,
    /// The command line carried `--reset-author`.
    pub renew: bool,
    /// The command line carried `--amend`.
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
    /// The authorship and date of the commit whose message `-C` or `-c` reused.
    pub authorship: bool,
    /// The `--reset-author` options stay, which Git accepts only with `-C`, `-c`, `--amend`, or during a pick.
    pub reset_author: bool,
}

/// Why no retry repeats a refused commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Withheld {
    /// The gate couldn't read the reused commit's authorship.
    Authorship,
    /// The gate couldn't tell whether a pick ran, which decides whose authorship a reuse takes.
    Picking,
}

/// The retry of a refused commit.
/// `picking` says a cherry-pick or rebase pick ran, when the gate could tell, and `known` says the gate read the reused commit's authorship.
/// During a pick Git takes the authorship from the picked commit unless the line has `--reset-author`, whatever `-C` or `-c` named.
///
/// # Errors
///
/// The gate doesn't know the authorship a retry must name, or whether a pick decides it.
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
        // Without the reuse and `--amend` the authorship already matches the committer, as the reset made it.
        reset_author: !source.reuse || !source.renew || source.amend || picking,
    })
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub const REFERENCES: [Reference; 4] = [
        Reference::CheckedOutBranch,
        Reference::Head,
        Reference::Branch,
        Reference::Other,
    ];
    pub const FIXUPS: [Fixup; 4] = [Fixup::Absent, Fixup::Plain, Fixup::Amend, Fixup::Reword];

    /// A rebase signs one linear branch, an amend only the checked-out tip, and every other push goes to inspection.
    pub fn spec_signing(refs: usize, reference: Reference, merges: bool, only_tip: bool) {
        let checked_out = matches!(reference, Reference::CheckedOutBranch | Reference::Head);
        let branch = matches!(reference, Reference::CheckedOutBranch | Reference::Branch);
        assert_eq!(reference.checked_out(), checked_out);
        assert_eq!(reference.branch(), branch);
        let plan = signing(refs, reference, merges, only_tip);
        assert_eq!(plan == Signing::Amend, refs == 1 && checked_out && only_tip);
        assert_eq!(
            plan == Signing::Rebase,
            refs == 1 && !(checked_out && only_tip) && branch && !merges
        );
    }

    pub fn spec_history(undecidable: bool, rewrite: bool) {
        let step = history(undecidable, rewrite);
        assert_eq!(step == History::Fetch, undecidable);
        assert_eq!(step == History::Integrate, !undecidable && rewrite);
        assert_eq!(step == History::ForgeDelete, !undecidable && !rewrite);
    }

    pub fn spec_rebase_refused(control: bool, rewrites: Option<bool>) {
        assert_eq!(
            rebase_refused(control, rewrites),
            !control && rewrites != Some(false)
        );
    }

    pub fn spec_verdict(refused: bool, read: bool) {
        let result = verdict(refused, read);
        assert_eq!(result == Verdict::Allow, !refused);
        assert_eq!(result == Verdict::Suggest, refused && read);
        assert_eq!(result == Verdict::Withhold, refused && !read);
    }

    /// A published rewrite runs only through the leased stack tooling or a bypass.
    pub fn spec_admission(
        token: Option<&[u8]>,
        lease: Option<&[u8]>,
        stack_rule: bool,
        bypassed: bool,
    ) {
        let tooling = stack_tooling(token, lease);
        assert_eq!(
            tooling,
            token.is_some_and(|token| !token.is_empty() && Some(token) == lease)
        );
        let admitted = admission(stack_rule, tooling, bypassed);
        assert_eq!(admitted == Admission::Tooling, stack_rule && tooling);
        assert_eq!(
            admitted == Admission::Waived,
            bypassed && !(stack_rule && tooling)
        );
        assert_eq!(
            admitted == Admission::Refused,
            !(bypassed || stack_rule && tooling)
        );
    }

    /// A long option reads as its first exact spelling, else its only completion, else as ambiguous or unknown by how many spellings it starts.
    pub fn spec_spelling(name: &[u8], spellings: &[&[u8]]) {
        let mut exact = None;
        let mut completions = 0;
        let mut only = 0;
        let mut index = 0;
        while let Some(&candidate) = spellings.get(index) {
            if exact.is_none() && candidate == name {
                exact = Some(index);
            }
            if candidate.starts_with(name) {
                completions += 1;
                only = index;
            }
            index += 1;
        }
        let expected = match (exact, completions) {
            (Some(index), _) => Spelling::Is(index),
            (None, 0) => Spelling::Unknown,
            (None, 1) => Spelling::Is(only),
            (None, _) => Spelling::Ambiguous,
        };
        assert_eq!(spelling(name, spellings), expected);
    }

    /// A retry keeps what its message source implied, and refuses when the gate can't tell whose authorship a reuse takes.
    pub fn spec_commit_retry(source: Source, picking: Option<bool>, known: bool) {
        let retry = commit_retry(source, picking, known);
        let takes = source.reuse && !source.renew && picking == Some(false);
        let expected = if source.reuse && picking.is_none() {
            Err(Withheld::Picking)
        } else if takes && !known {
            Err(Withheld::Authorship)
        } else {
            Ok(Retry {
                allow_empty: matches!(source.fixup, Fixup::Amend | Fixup::Reword),
                only: source.fixup == Fixup::Reword,
                authorship: takes,
                reset_author: !source.reuse
                    || !source.renew
                    || source.amend
                    || picking == Some(true),
            })
        };
        assert_eq!(retry, expected);
        // Git accepts a kept reset, and a dropped one leaves the committer's authorship.
        if let Ok(retry) = retry {
            assert!(retry.reset_author || (source.renew && !source.amend && !retry.authorship));
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_rebase_signs_one_linear_branch_and_an_amend_only_the_checked_out_tip() {
        let refs: usize = kani::any();
        let reference = REFERENCES[usize::from(kani::any::<u8>() % 4)];
        let merges: bool = kani::any();
        let only_tip: bool = kani::any();
        spec_signing(refs, reference, merges, only_tip);
        let plan = signing(refs, reference, merges, only_tip);
        kani::cover!(plan == Signing::Amend);
        kani::cover!(plan == Signing::Rebase && only_tip && !reference.checked_out());
        kani::cover!(plan == Signing::Inspect && refs == 1 && merges);
        kani::cover!(plan == Signing::Inspect && refs > 1);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_refusal_suggests_the_step_for_its_worst_update_and_only_from_a_line_read_in_full() {
        let first: bool = kani::any();
        let second: bool = kani::any();
        let rewrites: Option<bool> = kani::any();
        spec_history(first, second);
        spec_verdict(first, second);
        spec_rebase_refused(first, rewrites);
        kani::cover!(history(first, second) == History::Integrate);
        kani::cover!(verdict(first, second) == Verdict::Withhold);
        kani::cover!(rebase_refused(first, rewrites) && rewrites.is_none());
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
        let stack_rule: bool = kani::any();
        let bypassed: bool = kani::any();
        spec_admission(token, lease, stack_rule, bypassed);
        let admitted = admission(stack_rule, stack_tooling(token, lease), bypassed);
        kani::cover!(admitted == Admission::Tooling);
        kani::cover!(admitted == Admission::Waived && !stack_rule);
        kani::cover!(
            admitted == Admission::Refused && stack_rule && token.is_some() && lease.is_none()
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(4)]
    fn a_long_option_is_its_exact_spelling_or_its_only_completion() {
        let first: [u8; 2] = kani::any();
        let second: [u8; 2] = kani::any();
        let third: [u8; 2] = kani::any();
        let word: [u8; 2] = kani::any();
        let lengths: [usize; 4] = [kani::any(), kani::any(), kani::any(), kani::any()];
        kani::assume(lengths[0] <= 2 && lengths[1] <= 2 && lengths[2] <= 2 && lengths[3] <= 2);
        let name = &word[..lengths[3]];
        let spellings: [&[u8]; 3] = [
            &first[..lengths[0]],
            &second[..lengths[1]],
            &third[..lengths[2]],
        ];
        spec_spelling(name, &spellings);
        let read = spelling(name, &spellings);
        kani::cover!(read == Spelling::Ambiguous);
        kani::cover!(read == Spelling::Unknown);
        kani::cover!(
            matches!(read, Spelling::Is(_))
                && spellings[0] != name
                && spellings[1] != name
                && spellings[2] != name
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_commit_retry_keeps_what_its_message_source_implied() {
        let source = Source {
            fixup: FIXUPS[usize::from(kani::any::<u8>() % 4)],
            reuse: kani::any(),
            renew: kani::any(),
            amend: kani::any(),
        };
        let picking: Option<bool> = kani::any();
        let known: bool = kani::any();
        spec_commit_retry(source, picking, known);
        let retry = commit_retry(source, picking, known);
        kani::cover!(retry == Err(Withheld::Picking));
        kani::cover!(retry == Err(Withheld::Authorship));
        kani::cover!(retry.is_ok_and(|retry| retry.only && retry.allow_empty));
        kani::cover!(retry.is_ok_and(|retry| !retry.reset_author));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{flags, words};
    use alloc::vec::Vec;

    #[test]
    fn every_input_meets_the_signing_history_rebase_and_verdict_specifications() {
        let options = [None, Some(false), Some(true)];
        for refs in [0, 1, 2, usize::MAX] {
            for reference in proofs::REFERENCES {
                for [merges, only_tip] in flags() {
                    proofs::spec_signing(refs, reference, merges, only_tip);
                }
            }
        }
        for [first, second] in flags() {
            proofs::spec_history(first, second);
            proofs::spec_verdict(first, second);
        }
        for control in [false, true] {
            for rewrites in options {
                proofs::spec_rebase_refused(control, rewrites);
            }
        }
        for fixup in proofs::FIXUPS {
            for [reuse, renew, amend, known] in flags() {
                for picking in options {
                    proofs::spec_commit_retry(
                        Source {
                            fixup,
                            reuse,
                            renew,
                            amend,
                        },
                        picking,
                        known,
                    );
                }
            }
        }
    }

    #[test]
    fn every_short_token_and_lease_meets_the_admission_specification() {
        let values = words(b"ab", 2);
        let options: Vec<Option<&[u8]>> = core::iter::once(None)
            .chain(values.iter().map(|value| Some(value.as_slice())))
            .collect();
        for &token in &options {
            for &lease in &options {
                for [stack_rule, bypassed] in flags() {
                    proofs::spec_admission(token, lease, stack_rule, bypassed);
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every short name")]
    fn every_short_name_against_three_spellings_meets_the_spelling_specification() {
        let values = words(b"ab", 2);
        for name in &values {
            for first in &values {
                for second in &values {
                    for third in &values {
                        proofs::spec_spelling(
                            name,
                            &[first.as_slice(), second.as_slice(), third.as_slice()],
                        );
                    }
                }
            }
        }
    }

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

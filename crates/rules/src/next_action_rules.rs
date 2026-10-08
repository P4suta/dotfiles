/// Whether the host provides the directories every build and check writes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    Ready,
    /// `TEMP` has no value, so tools fall back to a directory the host didn't choose.
    TempUnset,
    /// The volume that should hold a required directory, such as the Dev Drive, has no mount.
    VolumeMissing,
    /// The volume has mounted but the temporary directory doesn't exist.
    TempMissing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Head {
    Detached,
    Default,
    Feature,
}

/// How `HEAD` relates to the branch it pushes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Upstream {
    /// The branch has no upstream.
    Absent,
    Current,
    /// Only `HEAD` has commits the upstream lacks.
    Ahead,
    /// Only the upstream has commits `HEAD` lacks.
    Behind,
    /// Both sides have their own commits and the local branch once held the upstream tip after a rebase or an amend of a pushed branch.
    Rewritten,
    /// Both sides have their own commits, and the upstream holds commits the local branch never had, such as a push from another clone.
    Diverged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pr {
    /// The inspection didn't consult GitHub.
    Unknown,
    None,
    Draft,
    Ready,
    Merged,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Checks {
    None,
    Pending,
    Failure,
    Success,
}

impl Checks {
    /// The name a report prints.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Pending => "pending",
            Self::Failure => "failure",
            Self::Success => "success",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is an independent fact, and the proofs range over every combination"
)]
pub struct State {
    pub host: Host,
    pub head: Head,
    pub dirty: bool,
    pub undecided: bool,
    pub incomplete: bool,
    pub behind_base: bool,
    pub ahead_base: bool,
    pub upstream: Upstream,
    pub push_paused: bool,
    pub pr: Pr,
    pub checks: Checks,
    /// The repository defines CI workflows, so an empty check rollup means they haven't reported yet.
    pub workflows: bool,
    pub review_paused: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    SetTemp,
    MountVolume,
    CreateTemp,
    AttachBranch,
    BranchFromDefault,
    FastForward,
    StartBranch,
    DraftDecisions,
    CompleteDecisions,
    Commit,
    Finished,
    Reopen,
    SyncUpstream,
    IntegrateUpstream,
    Rebase,
    StartWork,
    AwaitPushResume,
    Push,
    ForcePush,
    InspectGithub,
    CreatePr,
    FixChecks,
    AwaitChecks,
    AwaitReviewResume,
    MarkReady,
    AwaitReview,
}

impl Step {
    /// The name a report prints.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::SetTemp => "set-temp",
            Self::MountVolume => "mount-volume",
            Self::CreateTemp => "create-temp",
            Self::AttachBranch => "attach-branch",
            Self::BranchFromDefault => "branch-from-default",
            Self::FastForward => "fast-forward",
            Self::StartBranch => "start-branch",
            Self::DraftDecisions => "draft-decisions",
            Self::CompleteDecisions => "complete-decisions",
            Self::Commit => "commit",
            Self::Finished => "finished",
            Self::Reopen => "reopen",
            Self::SyncUpstream => "sync-upstream",
            Self::IntegrateUpstream => "integrate-upstream",
            Self::Rebase => "rebase",
            Self::StartWork => "start-work",
            Self::AwaitPushResume => "await-push-resume",
            Self::Push => "push",
            Self::ForcePush => "force-push",
            Self::InspectGithub => "inspect-github",
            Self::CreatePr => "create-pr",
            Self::FixChecks => "fix-checks",
            Self::AwaitChecks => "await-checks",
            Self::AwaitReviewResume => "await-review-resume",
            Self::MarkReady => "mark-ready",
            Self::AwaitReview => "await-review",
        }
    }
}

/// The single next step for a state.
/// Earlier prerequisites always win over later publication steps.
#[must_use]
pub fn decide(state: State) -> Step {
    match state.host {
        Host::TempUnset => return Step::SetTemp,
        Host::VolumeMissing => return Step::MountVolume,
        Host::TempMissing => return Step::CreateTemp,
        Host::Ready => {}
    }
    match state.head {
        Head::Detached => return Step::AttachBranch,
        Head::Default if state.dirty || state.ahead_base => return Step::BranchFromDefault,
        Head::Default if state.behind_base => return Step::FastForward,
        Head::Default => return Step::StartBranch,
        Head::Feature => {}
    }
    if state.undecided {
        return Step::DraftDecisions;
    }
    if state.incomplete {
        return Step::CompleteDecisions;
    }
    if state.dirty {
        return Step::Commit;
    }
    match state.pr {
        Pr::Merged => return Step::Finished,
        Pr::Closed => return Step::Reopen,
        _ => {}
    }
    match state.upstream {
        Upstream::Behind => return Step::SyncUpstream,
        Upstream::Diverged => return Step::IntegrateUpstream,
        _ => {}
    }
    if state.behind_base {
        return Step::Rebase;
    }
    if !state.ahead_base {
        return Step::StartWork;
    }
    if state.upstream != Upstream::Current {
        // A push to a PR in review starts a CodeRabbit review, so the owner's review pause also holds it.
        return match (state.push_paused, state.review_paused, state.pr) {
            (true, _, _) => Step::AwaitPushResume,
            (false, true, Pr::Unknown) => Step::InspectGithub,
            (false, true, Pr::Ready) => Step::AwaitReviewResume,
            _ if state.upstream == Upstream::Rewritten => Step::ForcePush,
            _ => Step::Push,
        };
    }
    let draft = match state.pr {
        Pr::Unknown => return Step::InspectGithub,
        Pr::None => return Step::CreatePr,
        Pr::Draft => true,
        _ => false,
    };
    match state.checks {
        Checks::Failure => Step::FixChecks,
        Checks::None if !state.workflows => settled(state.review_paused, draft),
        Checks::None | Checks::Pending => Step::AwaitChecks,
        Checks::Success => settled(state.review_paused, draft),
    }
}

/// The step for a pushed PR whose checks have nothing left to report.
fn settled(review_paused: bool, draft: bool) -> Step {
    match (review_paused, draft) {
        (true, _) => Step::AwaitReviewResume,
        (false, true) => Step::MarkReady,
        (false, false) => Step::AwaitReview,
    }
}

/// Steps `--execute` may run: each only writes local state that a single command restores, and none publishes, commits, or rewrites history.
#[must_use]
pub fn executable(step: Step) -> bool {
    matches!(
        step,
        Step::CreateTemp | Step::FastForward | Step::SyncUpstream | Step::DraftDecisions
    )
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub const HOSTS: [Host; 4] = [
        Host::Ready,
        Host::TempUnset,
        Host::VolumeMissing,
        Host::TempMissing,
    ];
    pub const HEADS: [Head; 3] = [Head::Detached, Head::Default, Head::Feature];
    pub const UPSTREAMS: [Upstream; 6] = [
        Upstream::Absent,
        Upstream::Current,
        Upstream::Ahead,
        Upstream::Behind,
        Upstream::Rewritten,
        Upstream::Diverged,
    ];
    pub const PRS: [Pr; 6] = [
        Pr::Unknown,
        Pr::None,
        Pr::Draft,
        Pr::Ready,
        Pr::Merged,
        Pr::Closed,
    ];
    pub const CHECKS: [Checks; 4] = [
        Checks::None,
        Checks::Pending,
        Checks::Failure,
        Checks::Success,
    ];

    const fn publishes(step: Step) -> bool {
        matches!(
            step,
            Step::Push | Step::ForcePush | Step::CreatePr | Step::MarkReady
        )
    }

    /// Host prerequisites come first, executed steps run only on their own preconditions, owner pauses hold, publication needs committed and decided work, and no step overwrites foreign upstream commits.
    pub fn spec_decide(state: State) {
        let step = decide(state);
        let synced = spec_prerequisites(state, step);
        spec_publication(state, step, synced);
        if state.upstream == Upstream::Diverged {
            assert!(!publishes(step));
        }
        if state.push_paused {
            assert!(!matches!(step, Step::Push | Step::ForcePush));
        }
        if state.review_paused {
            assert_ne!(step, Step::MarkReady);
        }
        assert_eq!(
            executable(step),
            matches!(
                step,
                Step::CreateTemp | Step::FastForward | Step::SyncUpstream | Step::DraftDecisions
            )
        );
    }

    /// The host, the branch, the decisions, the commit, the PR's life, and the upstream come first, in that order, and the result says whether the branch reached publication.
    fn spec_prerequisites(state: State, step: Step) -> bool {
        assert_eq!(state.host == Host::TempUnset, step == Step::SetTemp);
        assert_eq!(state.host == Host::VolumeMissing, step == Step::MountVolume);
        assert_eq!(state.host == Host::TempMissing, step == Step::CreateTemp);
        let ready = state.host == Host::Ready;
        assert_eq!(
            ready && state.head == Head::Detached,
            step == Step::AttachBranch
        );
        let on_default = ready && state.head == Head::Default;
        assert_eq!(
            on_default && (state.dirty || state.ahead_base),
            step == Step::BranchFromDefault
        );
        assert_eq!(
            on_default && !state.dirty && !state.ahead_base && state.behind_base,
            step == Step::FastForward
        );
        assert_eq!(
            on_default && !state.dirty && !state.ahead_base && !state.behind_base,
            step == Step::StartBranch
        );
        let feature = ready && state.head == Head::Feature;
        assert_eq!(feature && state.undecided, step == Step::DraftDecisions);
        assert_eq!(
            feature && !state.undecided && state.incomplete,
            step == Step::CompleteDecisions
        );
        let decided = feature && !state.undecided && !state.incomplete;
        assert_eq!(decided && state.dirty, step == Step::Commit);
        let committed = decided && !state.dirty;
        assert_eq!(committed && state.pr == Pr::Merged, step == Step::Finished);
        assert_eq!(committed && state.pr == Pr::Closed, step == Step::Reopen);
        let open = committed && !matches!(state.pr, Pr::Merged | Pr::Closed);
        assert_eq!(
            open && state.upstream == Upstream::Behind,
            step == Step::SyncUpstream
        );
        assert_eq!(
            open && state.upstream == Upstream::Diverged,
            step == Step::IntegrateUpstream
        );
        open && !matches!(state.upstream, Upstream::Behind | Upstream::Diverged)
    }

    /// Publication rebases, pushes, opens, waits for checks and reviews, and marks ready in that order, and each pause holds its steps.
    fn spec_publication(state: State, step: Step, synced: bool) {
        assert_eq!(synced && state.behind_base, step == Step::Rebase);
        assert_eq!(
            synced && !state.behind_base && !state.ahead_base,
            step == Step::StartWork
        );
        let pushing =
            synced && !state.behind_base && state.ahead_base && state.upstream != Upstream::Current;
        assert_eq!(pushing && state.push_paused, step == Step::AwaitPushResume);
        let held = pushing && !state.push_paused && state.review_paused;
        assert_eq!(
            pushing
                && !state.push_paused
                && !(state.review_paused && matches!(state.pr, Pr::Unknown | Pr::Ready)),
            matches!(step, Step::Push | Step::ForcePush)
        );
        if matches!(step, Step::Push | Step::ForcePush) {
            assert_eq!(
                step == Step::ForcePush,
                state.upstream == Upstream::Rewritten
            );
        }
        let published =
            synced && !state.behind_base && state.ahead_base && state.upstream == Upstream::Current;
        assert_eq!(
            state.pr == Pr::Unknown && (held || published),
            step == Step::InspectGithub
        );
        assert_eq!(published && state.pr == Pr::None, step == Step::CreatePr);
        let reviewed = published && matches!(state.pr, Pr::Draft | Pr::Ready);
        assert_eq!(
            reviewed && state.checks == Checks::Failure,
            step == Step::FixChecks
        );
        let reported =
            state.checks == Checks::Success || (state.checks == Checks::None && !state.workflows);
        assert_eq!(
            reviewed && !reported && state.checks != Checks::Failure,
            step == Step::AwaitChecks
        );
        assert_eq!(
            (held && state.pr == Pr::Ready) || (reviewed && reported && state.review_paused),
            step == Step::AwaitReviewResume
        );
        assert_eq!(
            reviewed && reported && !state.review_paused && state.pr == Pr::Draft,
            step == Step::MarkReady
        );
        assert_eq!(
            reviewed && reported && !state.review_paused && state.pr == Pr::Ready,
            step == Step::AwaitReview
        );
        if publishes(step) {
            assert!(synced && !state.behind_base && state.ahead_base);
        }
    }

    #[cfg(kani)]
    fn any_state() -> State {
        State {
            host: HOSTS[usize::from(kani::any::<u8>() % 4)],
            head: HEADS[usize::from(kani::any::<u8>() % 3)],
            dirty: kani::any(),
            undecided: kani::any(),
            incomplete: kani::any(),
            behind_base: kani::any(),
            ahead_base: kani::any(),
            upstream: UPSTREAMS[usize::from(kani::any::<u8>() % 6)],
            push_paused: kani::any(),
            pr: PRS[usize::from(kani::any::<u8>() % 6)],
            checks: CHECKS[usize::from(kani::any::<u8>() % 4)],
            workflows: kani::any(),
            review_paused: kani::any(),
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    fn every_state_takes_the_one_step_its_prerequisites_and_pauses_allow() {
        let state = any_state();
        spec_decide(state);
        let step = decide(state);
        kani::cover!(step == Step::MarkReady);
        kani::cover!(step == Step::ForcePush);
        kani::cover!(state.review_paused && state.pr == Pr::Draft && step == Step::Push);
        kani::cover!(state.push_paused && step == Step::AwaitPushResume);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::flags;

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes minutes over every state")]
    fn every_state_meets_the_decision_specification() {
        for host in proofs::HOSTS {
            for head in proofs::HEADS {
                for upstream in proofs::UPSTREAMS {
                    for pr in proofs::PRS {
                        for checks in proofs::CHECKS {
                            for [
                                dirty,
                                undecided,
                                incomplete,
                                behind_base,
                                ahead_base,
                                push_paused,
                                workflows,
                                review_paused,
                            ] in flags()
                            {
                                proofs::spec_decide(State {
                                    host,
                                    head,
                                    dirty,
                                    undecided,
                                    incomplete,
                                    behind_base,
                                    ahead_base,
                                    upstream,
                                    push_paused,
                                    pr,
                                    checks,
                                    workflows,
                                    review_paused,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn each_check_state_and_step_has_its_own_key() {
        assert_eq!(
            proofs::CHECKS.map(Checks::key),
            ["none", "pending", "failure", "success"]
        );
        let keys = [
            Step::SetTemp,
            Step::MountVolume,
            Step::CreateTemp,
            Step::AttachBranch,
            Step::BranchFromDefault,
            Step::FastForward,
            Step::StartBranch,
            Step::DraftDecisions,
            Step::CompleteDecisions,
            Step::Commit,
            Step::Finished,
            Step::Reopen,
            Step::SyncUpstream,
            Step::IntegrateUpstream,
            Step::Rebase,
            Step::StartWork,
            Step::AwaitPushResume,
            Step::Push,
            Step::ForcePush,
            Step::InspectGithub,
            Step::CreatePr,
            Step::FixChecks,
            Step::AwaitChecks,
            Step::AwaitReviewResume,
            Step::MarkReady,
            Step::AwaitReview,
        ]
        .map(Step::key);
        assert_eq!(
            keys,
            [
                "set-temp",
                "mount-volume",
                "create-temp",
                "attach-branch",
                "branch-from-default",
                "fast-forward",
                "start-branch",
                "draft-decisions",
                "complete-decisions",
                "commit",
                "finished",
                "reopen",
                "sync-upstream",
                "integrate-upstream",
                "rebase",
                "start-work",
                "await-push-resume",
                "push",
                "force-push",
                "inspect-github",
                "create-pr",
                "fix-checks",
                "await-checks",
                "await-review-resume",
                "mark-ready",
                "await-review"
            ]
        );
    }
}

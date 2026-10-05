/// Whether the host provides the directories every build and check writes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    Ready,
    /// `TEMP` is not set, so tools fall back to a directory the host did not choose.
    TempUnset,
    /// The volume that should hold a required directory, such as the Dev Drive, is not mounted.
    VolumeMissing,
    /// The volume is mounted but the temporary directory does not exist.
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
    /// No upstream is configured.
    Absent,
    Current,
    /// Only `HEAD` has commits the upstream lacks.
    Ahead,
    /// Only the upstream has commits `HEAD` lacks.
    Behind,
    /// Both sides have their own commits, and the upstream tip was once on the local branch, as after rebasing or amending a pushed branch.
    Rewritten,
    /// Both sides have their own commits, and the upstream holds commits the local branch never had, such as a push from another clone.
    Diverged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pr {
    /// GitHub was not consulted.
    Unknown,
    None,
    Draft,
    Ready,
    Merged,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(kani), derive(serde::Serialize), serde(rename_all = "kebab-case"))]
pub enum Checks {
    None,
    Pending,
    Failing,
    Passing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /// The repository defines CI workflows, so an empty check rollup means they have not reported yet.
    pub workflows: bool,
    pub review_paused: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(kani), derive(serde::Serialize), serde(rename_all = "kebab-case"))]
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

/// The single next step for a state; earlier prerequisites always win over later publication steps.
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
        Checks::Failing => Step::FixChecks,
        Checks::None if !state.workflows => settled(state.review_paused, draft),
        Checks::None | Checks::Pending => Step::AwaitChecks,
        Checks::Passing => settled(state.review_paused, draft),
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
pub fn executable(step: Step) -> bool {
    matches!(
        step,
        Step::CreateTemp | Step::FastForward | Step::SyncUpstream | Step::DraftDecisions
    )
}

#[cfg(kani)]
fn any_state() -> State {
    let host: u8 = kani::any();
    let head: u8 = kani::any();
    let pr: u8 = kani::any();
    let checks: u8 = kani::any();
    let upstream: u8 = kani::any();
    kani::assume(host < 4 && head < 3 && pr < 6 && checks < 4 && upstream < 6);
    State {
        host: match host {
            0 => Host::Ready,
            1 => Host::TempUnset,
            2 => Host::VolumeMissing,
            _ => Host::TempMissing,
        },
        head: match head {
            0 => Head::Detached,
            1 => Head::Default,
            _ => Head::Feature,
        },
        dirty: kani::any(),
        undecided: kani::any(),
        incomplete: kani::any(),
        behind_base: kani::any(),
        ahead_base: kani::any(),
        upstream: match upstream {
            0 => Upstream::Absent,
            1 => Upstream::Current,
            2 => Upstream::Ahead,
            3 => Upstream::Behind,
            4 => Upstream::Rewritten,
            _ => Upstream::Diverged,
        },
        push_paused: kani::any(),
        pr: match pr {
            0 => Pr::Unknown,
            1 => Pr::None,
            2 => Pr::Draft,
            3 => Pr::Ready,
            4 => Pr::Merged,
            _ => Pr::Closed,
        },
        checks: match checks {
            0 => Checks::None,
            1 => Checks::Pending,
            2 => Checks::Failing,
            _ => Checks::Passing,
        },
        workflows: kani::any(),
        review_paused: kani::any(),
    }
}

#[cfg(kani)]
fn publishes(step: Step) -> bool {
    matches!(
        step,
        Step::Push | Step::ForcePush | Step::CreatePr | Step::MarkReady
    )
}

#[cfg(kani)]
#[kani::proof]
fn host_prerequisites_precede_every_repository_step() {
    let state = any_state();
    let step = decide(state);
    assert_eq!(state.host == Host::TempUnset, step == Step::SetTemp);
    assert_eq!(state.host == Host::VolumeMissing, step == Step::MountVolume);
    assert_eq!(state.host == Host::TempMissing, step == Step::CreateTemp);
    kani::cover!(step == Step::SetTemp);
    kani::cover!(state.host == Host::Ready && step == Step::Push);
}

#[cfg(kani)]
#[kani::proof]
fn executed_steps_run_only_on_their_own_preconditions() {
    let state = any_state();
    let step = decide(state);
    match step {
        Step::CreateTemp => assert!(state.host == Host::TempMissing),
        Step::FastForward => {
            assert!(state.head == Head::Default && !state.dirty && !state.ahead_base);
            assert!(state.behind_base);
        }
        Step::SyncUpstream => {
            assert!(state.head == Head::Feature && !state.dirty);
            assert!(state.upstream == Upstream::Behind);
        }
        Step::DraftDecisions => {
            assert!(state.host == Host::Ready && state.head == Head::Feature);
        }
        _ => assert!(!executable(step)),
    }
    kani::cover!(step == Step::FastForward);
    kani::cover!(step == Step::SyncUpstream);
    kani::cover!(step == Step::DraftDecisions);
}

#[cfg(kani)]
#[kani::proof]
fn owner_pauses_are_never_bypassed() {
    let state = any_state();
    let step = decide(state);
    if state.push_paused {
        assert!(step != Step::Push && step != Step::ForcePush);
    }
    if state.review_paused {
        assert!(step != Step::MarkReady);
        // A push starts a review unless the PR is known to be a draft or absent.
        if matches!(step, Step::Push | Step::ForcePush) {
            assert!(matches!(state.pr, Pr::None | Pr::Draft));
        }
    }
    kani::cover!(state.push_paused && step == Step::AwaitPushResume);
    kani::cover!(state.review_paused && step == Step::AwaitReviewResume);
    kani::cover!(state.review_paused && state.pr == Pr::Draft && step == Step::Push);
}

#[cfg(kani)]
#[kani::proof]
fn publication_requires_committed_decided_current_work() {
    let state = any_state();
    let step = decide(state);
    if publishes(step) {
        assert!(state.host == Host::Ready && state.head == Head::Feature);
        assert!(!state.dirty && !state.undecided && !state.incomplete);
        assert!(!state.behind_base && state.ahead_base);
    }
    if step == Step::Push {
        assert!(matches!(state.upstream, Upstream::Absent | Upstream::Ahead));
    }
    if step == Step::ForcePush {
        assert!(state.upstream == Upstream::Rewritten);
    }
    if matches!(step, Step::CreatePr | Step::MarkReady | Step::AwaitReview) {
        assert!(state.upstream == Upstream::Current);
    }
    if step == Step::MarkReady {
        assert!(state.pr == Pr::Draft);
        assert!(
            state.checks == Checks::Passing || (state.checks == Checks::None && !state.workflows)
        );
    }
    kani::cover!(step == Step::MarkReady);
    kani::cover!(step == Step::ForcePush);
    kani::cover!(state.dirty && step == Step::Commit);
}

#[cfg(kani)]
#[kani::proof]
fn upstream_commits_the_branch_never_held_are_never_overwritten() {
    let state = any_state();
    let step = decide(state);
    if step == Step::ForcePush {
        assert!(state.upstream == Upstream::Rewritten);
    }
    if state.upstream == Upstream::Diverged {
        assert!(!publishes(step));
    }
    // Once the branch's own work is committed, foreign upstream commits are integrated before anything else.
    let settled = state.host == Host::Ready
        && state.head == Head::Feature
        && !state.dirty
        && !state.undecided
        && !state.incomplete
        && !matches!(state.pr, Pr::Merged | Pr::Closed);
    if settled {
        assert_eq!(
            state.upstream == Upstream::Diverged,
            step == Step::IntegrateUpstream
        );
    }
    kani::cover!(state.upstream == Upstream::Diverged && step == Step::IntegrateUpstream);
    kani::cover!(state.upstream == Upstream::Rewritten && step == Step::ForcePush);
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Generation {
    Local,
    Coderabbit,
}

impl Generation {
    pub const ALL: [Self; 2] = [Self::Local, Self::Coderabbit];

    /// The name a command line gives.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Coderabbit => "coderabbit",
        }
    }

    /// The generation a command line names, or `None` for a name no generation carries.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|generation| generation.key() == key)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Operation {
    Create,
    Edit,
    Ready,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    CreateDraft,
    CreateReady,
    Edit,
    Ready,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IssueGate {
    NotRequired,
    Linked,
    Missing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceRelation {
    Same,
    ForkNetwork,
    Refused,
}

#[derive(Clone, Copy, Debug)]
pub struct RepositoryIdentities {
    pub source: u64,
    pub target: u64,
    pub source_network: u64,
    pub target_network: u64,
}

#[must_use]
pub const fn source_relation(identities: RepositoryIdentities) -> SourceRelation {
    if identities.source == 0
        || identities.target == 0
        || identities.source_network == 0
        || identities.source_network != identities.target_network
    {
        SourceRelation::Refused
    } else if identities.source == identities.target {
        SourceRelation::Same
    } else {
        SourceRelation::ForkNetwork
    }
}

/// Apply personal issue rules only to owned non-fork repositories.
#[must_use]
pub fn issue_gate(personal_owner: bool, fork: bool, checked_issue: bool) -> IssueGate {
    if !personal_owner || fork {
        IssueGate::NotRequired
    } else if checked_issue {
        IssueGate::Linked
    } else {
        IssueGate::Missing
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextGate {
    NotRequired,
    Missing,
    Unauthenticated,
    Destination,
    Branch,
    Source,
    Closed,
    Scope,
    Accepted,
}

#[derive(Clone, Copy, Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Independent observed predicates retain every input combination for the issue decision and its proof."
)]
pub struct ContextFacts {
    pub required: bool,
    pub present: bool,
    pub authentic: bool,
    pub destination: bool,
    pub branch: bool,
    pub source: bool,
    pub open: bool,
    pub scoped: bool,
}

#[must_use]
pub const fn context_gate(facts: ContextFacts) -> ContextGate {
    if !facts.required {
        ContextGate::NotRequired
    } else if !facts.present {
        ContextGate::Missing
    } else if !facts.authentic {
        ContextGate::Unauthenticated
    } else if !facts.destination {
        ContextGate::Destination
    } else if !facts.branch {
        ContextGate::Branch
    } else if !facts.source {
        ContextGate::Source
    } else if !facts.open {
        ContextGate::Closed
    } else if !facts.scoped {
        ContextGate::Scope
    } else {
        ContextGate::Accepted
    }
}

#[must_use]
pub fn api_quota_available(remaining: u64, reserve: u64) -> bool {
    reserve > 0 && remaining >= reserve
}

#[must_use]
pub fn registered_path(scope: &[u8], path: &[u8]) -> bool {
    !scope.is_empty()
        && (scope == path
            || path
                .strip_prefix(scope)
                .is_some_and(|rest| rest.first() == Some(&b'/')))
}

#[must_use]
#[expect(
    clippy::fn_params_excessive_bools,
    reason = "Independent prerequisite predicates preserve the complete admission domain checked by the proof."
)]
pub const fn initial_change_allowed(
    required: bool,
    dirty: bool,
    covered: bool,
    owner_adoption: bool,
) -> bool {
    !required || !dirty || covered || owner_adoption
}

#[must_use]
pub fn coderabbit_body_marker_allowed(request: bool, summary: bool, ignore: bool) -> bool {
    ignore || request && summary
}

/// The head commit's reported checks, which `pr_ready_rules::complete` decides from its report.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Checks {
    Success,
    Incomplete,
}

/// The first unmet condition that refuses an operation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Blocker {
    Unvalidated,
    MissingIssue,
    NotDraft,
    UnfinishedChecks,
}

/// Why a PR body excludes `CodeRabbit`'s automatic review.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Exclusion {
    None,
    /// A standing exclusion chosen for the PR.
    Owner,
    /// An exclusion that lasts only while the owner pauses PR reviews.
    Pause,
}

#[must_use]
pub fn exclusion(ignore: bool, pause_marker: bool) -> Exclusion {
    match (ignore, pause_marker) {
        (false, _) => Exclusion::None,
        (true, false) => Exclusion::Owner,
        (true, true) => Exclusion::Pause,
    }
}

/// Whether an operation asks `CodeRabbit` for a PR review or generation.
/// An edit requests a review when it removes the exclusion of a ready PR.
#[must_use]
pub fn requests_review(
    operation: Operation,
    generation: Generation,
    draft: bool,
    was_excluded: bool,
    excluded: bool,
) -> bool {
    generation == Generation::Coderabbit
        || match operation {
            Operation::Create => false,
            Operation::Edit => !draft && was_excluded && !excluded,
            Operation::Ready => !excluded,
        }
}

/// Whether an operation during a PR pause keeps the review owed after resumption.
/// A PR becomes ready, or stays ready, only with the exclusion made for the pause.
#[must_use]
pub fn keeps_pause_exclusion(
    operation: Operation,
    draft: bool,
    was: Exclusion,
    now: Exclusion,
) -> bool {
    match operation {
        Operation::Create => true,
        Operation::Edit => draft || was != Exclusion::Pause || now == Exclusion::Pause,
        Operation::Ready => now == Exclusion::Pause,
    }
}

/// Refuse every review request while the owner pauses `CodeRabbit` PR reviews.
#[must_use]
pub fn review_request_allowed(paused: bool, request: bool) -> bool {
    !paused || !request
}

/// The `CodeRabbit` PR review a published PR still owes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReviewRequirement {
    Required,
    /// Required, but the body still carries the exclusion added for the pause.
    Resumed,
    Excluded,
    Paused,
}

#[must_use]
pub fn review_requirement(paused: bool, exclusion: Exclusion) -> ReviewRequirement {
    match (paused, exclusion) {
        (true, _) => ReviewRequirement::Paused,
        (false, Exclusion::None) => ReviewRequirement::Required,
        (false, Exclusion::Pause) => ReviewRequirement::Resumed,
        (false, Exclusion::Owner) => ReviewRequirement::Excluded,
    }
}

/// The checks reported for the head commit while a merge waits for them.
/// An empty report counts as pending.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HeadChecks {
    Success,
    Pending,
    Failed,
}

#[must_use]
pub fn head_checks(total: usize, unfinished: usize, failed: usize) -> HeadChecks {
    if failed > 0 {
        HeadChecks::Failed
    } else if total == 0 || unfinished > 0 {
        HeadChecks::Pending
    } else {
        HeadChecks::Success
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lifecycle {
    Draft,
    Ready,
    Merged,
    Closed,
}

/// What GitHub reports about one PR of a merge, read in one response so the checks belong to its head.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MergeState {
    pub lifecycle: Lifecycle,
    pub conflict: bool,
    /// The base branch has commits the head lacks.
    pub behind: bool,
    pub checks: HeadChecks,
    /// GitHub reports that the PR meets all merge rules of the base.
    pub clean: bool,
    pub exclusion: Exclusion,
}

/// Why a merge stops before it lands a PR.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stop {
    Closed,
    Conflict,
    FailedChecks,
    /// The head still needs a `CodeRabbit` review.
    /// Only coderabbit-review can establish that the review's findings have a resolution.
    Review,
}

/// The next step of a merge for one PR.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    Done,
    /// Merge the base into an independent PR's head with GitHub's branch update, which GitHub signs.
    Update,
    /// Rebase a stacked PR onto its parent with the stack tooling.
    /// A stack needs a linear history.
    Restack,
    Wait,
    /// Add the `CodeRabbit` pause lines before a draft becomes ready during the owner's pause.
    Exclude,
    Ready,
    /// Merge the head whose checks the tool read.
    Merge,
    Stop(Stop),
}

#[must_use]
pub fn merge_step(state: MergeState, stacked: bool, paused: bool) -> Step {
    match state.lifecycle {
        Lifecycle::Merged => return Step::Done,
        Lifecycle::Closed => return Step::Stop(Stop::Closed),
        Lifecycle::Draft | Lifecycle::Ready => {}
    }
    if state.conflict {
        return Step::Stop(Stop::Conflict);
    }
    if state.behind {
        return if stacked { Step::Restack } else { Step::Update };
    }
    match state.checks {
        HeadChecks::Failed => return Step::Stop(Stop::FailedChecks),
        HeadChecks::Pending => return Step::Wait,
        HeadChecks::Success => {}
    }
    if state.lifecycle == Lifecycle::Draft {
        return if paused && state.exclusion != Exclusion::Pause {
            Step::Exclude
        } else {
            Step::Ready
        };
    }
    match review_requirement(paused, state.exclusion) {
        ReviewRequirement::Required | ReviewRequirement::Resumed => Step::Stop(Stop::Review),
        ReviewRequirement::Paused | ReviewRequirement::Excluded if !state.clean => Step::Wait,
        ReviewRequirement::Paused | ReviewRequirement::Excluded => Step::Merge,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathMerge {
    Disjoint,
    Clean,
    Conflict,
}

/// Why a new branch must open on another open PR's branch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dependency {
    Commits,
    Conflict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DependencySkip {
    Bot,
    BaseConflict,
}

const APP_AUTHOR_PREFIX: &str = "app/";
const BOT_AUTHOR_SUFFIX: &str = "[bot]";
const BOT_AUTHOR_TYPE: &str = "Bot";
const CONFLICTING_MERGEABILITY: &str = "CONFLICTING";

#[must_use]
pub fn dependency_skip(
    login: &str,
    author_type: Option<&str>,
    is_bot: bool,
    mergeable: Option<&str>,
) -> Option<DependencySkip> {
    if is_bot
        || author_type == Some(BOT_AUTHOR_TYPE)
        || login.starts_with(APP_AUTHOR_PREFIX)
        || login.ends_with(BOT_AUTHOR_SUFFIX)
    {
        Some(DependencySkip::Bot)
    } else if mergeable == Some(CONFLICTING_MERGEABILITY) {
        Some(DependencySkip::BaseConflict)
    } else {
        None
    }
}

/// A branch that shares commits with another open PR opens only stacked on that PR.
/// Overlapping paths require a stack only when merging the two heads conflicts.
#[must_use]
pub fn dependency(shared_commits: bool, merge: PathMerge) -> Option<Dependency> {
    if shared_commits {
        Some(Dependency::Commits)
    } else if merge == PathMerge::Conflict {
        Some(Dependency::Conflict)
    } else {
        None
    }
}

/// The effect a PR operation may take.
///
/// # Errors
///
/// Returns the blocker that refuses the operation.
pub fn plan(
    operation: Operation,
    generation: Generation,
    validated: bool,
    draft: bool,
    issue: IssueGate,
    checks: Checks,
) -> Result<Effect, Blocker> {
    if !validated {
        return Err(Blocker::Unvalidated);
    }
    if issue == IssueGate::Missing {
        return Err(Blocker::MissingIssue);
    }
    match operation {
        Operation::Create => Ok(if generation == Generation::Local || draft {
            Effect::CreateDraft
        } else {
            Effect::CreateReady
        }),
        Operation::Edit => Ok(Effect::Edit),
        Operation::Ready if !draft => Err(Blocker::NotDraft),
        Operation::Ready if checks != Checks::Success => Err(Blocker::UnfinishedChecks),
        Operation::Ready => Ok(Effect::Ready),
    }
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_source_relation(identities: RepositoryIdentities) {
        let relation = source_relation(identities);
        let checked = identities.source != 0
            && identities.target != 0
            && identities.source_network != 0
            && identities.target_network != 0
            && identities.source_network == identities.target_network;
        assert_eq!(relation != SourceRelation::Refused, checked);
        assert_eq!(
            relation == SourceRelation::Same,
            checked && identities.source == identities.target
        );
        assert_eq!(
            relation == SourceRelation::ForkNetwork,
            checked && identities.source != identities.target
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    fn source_and_destination_require_a_known_common_repository_network() {
        let identities = RepositoryIdentities {
            source: kani::any(),
            target: kani::any(),
            source_network: kani::any(),
            target_network: kani::any(),
        };
        spec_source_relation(identities);
        kani::cover!(source_relation(identities) == SourceRelation::ForkNetwork);
        kani::cover!(source_relation(identities) == SourceRelation::Refused);
    }

    pub fn spec_initial_change([required, dirty, covered, owner_adoption]: [bool; 4]) {
        let allowed = initial_change_allowed(required, dirty, covered, owner_adoption);
        assert_eq!(!allowed, required && dirty && !covered && !owner_adoption);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_late_start_cannot_adopt_unregistered_changes_without_owner_approval() {
        let flags = kani::any();
        spec_initial_change(flags);
        let [required, dirty, covered, owner_adoption] = flags;
        let allowed = initial_change_allowed(required, dirty, covered, owner_adoption);
        kani::cover!(allowed);
        kani::cover!(!allowed);
    }

    pub fn spec_context(flags: [bool; 8]) {
        let [
            required,
            present,
            authentic,
            destination,
            branch,
            source,
            open,
            scoped,
        ] = flags;
        let result = context_gate(ContextFacts {
            required,
            present,
            authentic,
            destination,
            branch,
            source,
            open,
            scoped,
        });
        assert_eq!(
            result == ContextGate::Accepted,
            required && present && authentic && destination && branch && source && open && scoped
        );
        assert_eq!(result == ContextGate::NotRequired, !required);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn implementation_requires_every_checked_context_fact() {
        let flags = kani::any();
        spec_context(flags);
        let [
            required,
            present,
            authentic,
            destination,
            branch,
            source,
            open,
            scoped,
        ] = flags;
        let result = context_gate(ContextFacts {
            required,
            present,
            authentic,
            destination,
            branch,
            source,
            open,
            scoped,
        });
        kani::cover!(result == ContextGate::Accepted);
        kani::cover!(required && result != ContextGate::Accepted);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn registered_paths_require_a_complete_scope_component() {
        let scope: [u8; 8] = kani::any();
        let path: [u8; 8] = kani::any();
        let sl: usize = kani::any();
        let pl: usize = kani::any();
        kani::assume(sl <= scope.len() && pl <= path.len());
        let scope = &scope[..sl];
        let path = &path[..pl];
        let prefix = sl <= pl && scope.iter().zip(path).all(|(a, b)| a == b);
        assert_eq!(
            registered_path(scope, path),
            sl > 0 && prefix && (sl == pl || path.get(sl) == Some(&b'/'))
        );
        kani::cover!(registered_path(scope, path));
        kani::cover!(!registered_path(scope, path));
    }

    pub const OPERATIONS: [Operation; 3] = [Operation::Create, Operation::Edit, Operation::Ready];
    pub const ISSUES: [IssueGate; 3] = [
        IssueGate::NotRequired,
        IssueGate::Linked,
        IssueGate::Missing,
    ];
    pub const EXCLUSIONS: [Exclusion; 3] = [Exclusion::None, Exclusion::Owner, Exclusion::Pause];
    pub const LIFECYCLES: [Lifecycle; 4] = [
        Lifecycle::Draft,
        Lifecycle::Ready,
        Lifecycle::Merged,
        Lifecycle::Closed,
    ];
    pub const HEAD_CHECKS: [HeadChecks; 3] =
        [HeadChecks::Success, HeadChecks::Pending, HeadChecks::Failed];

    /// A rejected document never mutates a PR, local creation starts as a draft, ready needs a validated draft with passing checks, and a refusal names the first unmet condition.
    pub fn spec_plan(
        operation: Operation,
        generation: Generation,
        validated: bool,
        draft: bool,
        issue: IssueGate,
        checks: Checks,
    ) {
        let expected = if !validated {
            Err(Blocker::Unvalidated)
        } else if issue == IssueGate::Missing {
            Err(Blocker::MissingIssue)
        } else {
            match operation {
                Operation::Create if generation == Generation::Local || draft => {
                    Ok(Effect::CreateDraft)
                }
                Operation::Create => Ok(Effect::CreateReady),
                Operation::Edit => Ok(Effect::Edit),
                Operation::Ready if !draft => Err(Blocker::NotDraft),
                Operation::Ready if checks != Checks::Success => Err(Blocker::UnfinishedChecks),
                Operation::Ready => Ok(Effect::Ready),
            }
        };
        assert_eq!(
            plan(operation, generation, validated, draft, issue, checks),
            expected
        );
    }

    /// Any failure fails a merge, and an empty or unfinished report waits.
    pub fn spec_checks(total: usize, unfinished: usize, failed: usize) {
        let head = head_checks(total, unfinished, failed);
        assert_eq!(head == HeadChecks::Failed, failed != 0);
        assert_eq!(
            head == HeadChecks::Pending,
            failed == 0 && (total == 0 || unfinished != 0)
        );
        assert_eq!(
            head == HeadChecks::Success,
            failed == 0 && total != 0 && unfinished == 0
        );
    }

    /// The flag decisions admit exactly what their rules name.
    pub fn spec_flags(first: bool, second: bool, third: bool) {
        let gate = issue_gate(first, second, third);
        assert_eq!(gate == IssueGate::NotRequired, !first || second);
        assert_eq!(gate == IssueGate::Linked, first && !second && third);
        assert_eq!(gate == IssueGate::Missing, first && !second && !third);
        assert_eq!(
            coderabbit_body_marker_allowed(first, second, third),
            third || first && second
        );
        assert_eq!(review_request_allowed(first, second), !first || !second);
        let kind = exclusion(first, second);
        assert_eq!(kind == Exclusion::None, !first);
        assert_eq!(kind == Exclusion::Owner, first && !second);
        assert_eq!(kind == Exclusion::Pause, first && second);
        let merge = if !second {
            PathMerge::Disjoint
        } else if third {
            PathMerge::Clean
        } else {
            PathMerge::Conflict
        };
        assert_eq!(
            dependency(first, merge),
            if first {
                Some(Dependency::Commits)
            } else if second && !third {
                Some(Dependency::Conflict)
            } else {
                None
            }
        );
    }

    pub fn spec_quota(remaining: u64, reserve: u64) {
        assert_eq!(
            api_quota_available(remaining, reserve),
            reserve != 0 && remaining >= reserve
        );
    }

    pub fn spec_dependency_skip(
        login: &str,
        author_type: Option<&str>,
        is_bot: bool,
        mergeable: Option<&str>,
    ) {
        let bytes = login.as_bytes();
        let bot = is_bot
            || author_type == Some("Bot")
            || bytes.get(..APP_AUTHOR_PREFIX.len()) == Some(APP_AUTHOR_PREFIX.as_bytes())
            || bytes.get(bytes.len().saturating_sub(BOT_AUTHOR_SUFFIX.len())..)
                == Some(BOT_AUTHOR_SUFFIX.as_bytes());
        let skip = dependency_skip(login, author_type, is_bot, mergeable);
        assert_eq!(skip == Some(DependencySkip::Bot), bot);
        assert_eq!(
            skip == Some(DependencySkip::BaseConflict),
            !bot && mergeable == Some("CONFLICTING")
        );
        assert_eq!(skip.is_none(), !bot && mergeable != Some("CONFLICTING"));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(17)]
    fn dependency_skips_only_bots_and_prs_conflicting_with_their_base() {
        const LOGIN_BYTES: usize = 16;
        let bytes: [u8; LOGIN_BYTES] = kani::any();
        let length: usize = kani::any();
        kani::assume(length <= bytes.len());
        let Ok(login) = core::str::from_utf8(&bytes[..length]) else {
            return;
        };
        let author_type = if kani::any() { Some("Bot") } else { None };
        let is_bot = kani::any();
        let mergeable = if kani::any() {
            Some("CONFLICTING")
        } else {
            None
        };
        spec_dependency_skip(login, author_type, is_bot, mergeable);
        let skip = dependency_skip(login, author_type, is_bot, mergeable);
        kani::cover!(skip == Some(DependencySkip::Bot));
        kani::cover!(skip == Some(DependencySkip::BaseConflict));
        kani::cover!(skip.is_none());
    }

    /// No operation requests a paused PR review, and an edit requests one only by removing the exclusion of a ready PR.
    pub fn spec_review(operation: Operation, generation: Generation, flags: [bool; 4]) {
        let [draft, was_excluded, excluded, paused] = flags;
        let request = requests_review(operation, generation, draft, was_excluded, excluded);
        let expected = generation == Generation::Coderabbit
            || (operation == Operation::Edit && !draft && was_excluded && !excluded)
            || (operation == Operation::Ready && !excluded);
        assert_eq!(request, expected);
        let accepted = review_request_allowed(paused, request);
        assert!(!paused || generation == Generation::Local || !accepted);
        assert!(paused || accepted);
    }

    /// A PR ready during the pause keeps its pause exclusion, and a review comes back once the pause ends.
    pub fn spec_pause(
        operation: Operation,
        draft: bool,
        was: Exclusion,
        now: Exclusion,
        paused: bool,
    ) {
        let kept = keeps_pause_exclusion(operation, draft, was, now);
        let expected = match operation {
            Operation::Create => true,
            Operation::Edit => draft || was != Exclusion::Pause || now == Exclusion::Pause,
            Operation::Ready => now == Exclusion::Pause,
        };
        assert_eq!(kept, expected);
        let requirement = review_requirement(paused, now);
        assert_eq!(requirement == ReviewRequirement::Paused, paused);
        assert_eq!(
            requirement == ReviewRequirement::Required,
            !paused && now == Exclusion::None
        );
        assert_eq!(
            requirement == ReviewRequirement::Resumed,
            !paused && now == Exclusion::Pause
        );
        assert_eq!(
            requirement == ReviewRequirement::Excluded,
            !paused && now == Exclusion::Owner
        );
    }

    /// A PR merges only ready, current, passing, clean, and without an owed review, and each other state takes its own next step.
    pub fn spec_merge(state: MergeState, stacked: bool, paused: bool) {
        let owed = !paused && state.exclusion != Exclusion::Owner;
        let expected = match state.lifecycle {
            Lifecycle::Merged => Step::Done,
            Lifecycle::Closed => Step::Stop(Stop::Closed),
            _ if state.conflict => Step::Stop(Stop::Conflict),
            _ if state.behind && stacked => Step::Restack,
            _ if state.behind => Step::Update,
            _ if state.checks == HeadChecks::Failed => Step::Stop(Stop::FailedChecks),
            _ if state.checks == HeadChecks::Pending => Step::Wait,
            Lifecycle::Draft if paused && state.exclusion != Exclusion::Pause => Step::Exclude,
            Lifecycle::Draft => Step::Ready,
            Lifecycle::Ready if owed => Step::Stop(Stop::Review),
            Lifecycle::Ready if !state.clean => Step::Wait,
            Lifecycle::Ready => Step::Merge,
        };
        assert_eq!(merge_step(state, stacked, paused), expected);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_pr_operation_takes_its_effect_only_after_every_condition_and_names_the_first_unmet_one() {
        let operation = OPERATIONS[usize::from(kani::any::<u8>() % 3)];
        let generation = Generation::ALL[usize::from(kani::any::<u8>() % 2)];
        let issue = ISSUES[usize::from(kani::any::<u8>() % 3)];
        let checks = if kani::any() {
            Checks::Success
        } else {
            Checks::Incomplete
        };
        let validated: bool = kani::any();
        let draft: bool = kani::any();
        spec_plan(operation, generation, validated, draft, issue, checks);
        let result = plan(operation, generation, validated, draft, issue, checks);
        kani::cover!(result == Ok(Effect::CreateReady));
        kani::cover!(result == Ok(Effect::Ready));
        kani::cover!(result == Err(Blocker::UnfinishedChecks));
    }

    #[cfg(kani)]
    #[kani::proof]
    fn checks_flags_and_quotas_decide_exactly_what_their_rules_name() {
        let total: usize = kani::any();
        let unfinished: usize = kani::any();
        let failed: usize = kani::any();
        spec_checks(total, unfinished, failed);
        let flags: [bool; 3] = [kani::any(), kani::any(), kani::any()];
        let [first, second, third] = flags;
        spec_flags(first, second, third);
        let remaining: u64 = kani::any();
        let reserve: u64 = kani::any();
        spec_quota(remaining, reserve);
        let checks = head_checks(total, unfinished, failed);
        kani::cover!(checks == HeadChecks::Success);
        kani::cover!((checks == HeadChecks::Pending) & (failed == 0));
        kani::cover!(!api_quota_available(remaining, reserve) & (reserve > 0));
        kani::cover!(dependency(false, PathMerge::Clean).is_none());
        kani::cover!(dependency(false, PathMerge::Conflict) == Some(Dependency::Conflict));
    }

    #[cfg(kani)]
    #[kani::proof]
    fn paused_reviews_are_never_requested_and_return_after_resumption() {
        let operation = OPERATIONS[usize::from(kani::any::<u8>() % 3)];
        let generation = Generation::ALL[usize::from(kani::any::<u8>() % 2)];
        let flags: [bool; 4] = [kani::any(), kani::any(), kani::any(), kani::any()];
        spec_review(operation, generation, flags);
        let was = EXCLUSIONS[usize::from(kani::any::<u8>() % 3)];
        let now = EXCLUSIONS[usize::from(kani::any::<u8>() % 3)];
        spec_pause(operation, flags[0], was, now, flags[3]);
        kani::cover!(
            !keeps_pause_exclusion(operation, flags[0], was, now) && operation == Operation::Edit
        );
        kani::cover!(review_requirement(flags[3], now) == ReviewRequirement::Resumed);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_pr_merges_only_ready_current_passing_clean_and_without_an_owed_review() {
        let flags: [bool; 5] = [
            kani::any(),
            kani::any(),
            kani::any(),
            kani::any(),
            kani::any(),
        ];
        let [conflict, behind, clean, stacked, paused] = flags;
        let state = MergeState {
            lifecycle: LIFECYCLES[usize::from(kani::any::<u8>() % 4)],
            conflict,
            behind,
            checks: HEAD_CHECKS[usize::from(kani::any::<u8>() % 3)],
            clean,
            exclusion: EXCLUSIONS[usize::from(kani::any::<u8>() % 3)],
        };
        spec_merge(state, stacked, paused);
        let step = merge_step(state, stacked, paused);
        let merge = (step == Step::Merge) & paused;
        let restack = step == Step::Restack;
        let exclude = step == Step::Exclude;
        let wait = (step == Step::Wait) & (state.checks == HeadChecks::Success);
        kani::cover!(merge);
        kani::cover!(restack);
        kani::cover!(exclude);
        kani::cover!(wait);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::flags;

    #[test]
    fn dependency_skips_preserve_all_other_candidates() {
        for (login, bot_login) in [
            ("", false),
            ("contributor", false),
            ("app/p4suta-release-plz", true),
            ("release-plz[bot]", true),
            ("app/", true),
            ("[bot]", true),
            ("app", false),
            ("App/release-plz", false),
            ("myapp/release-plz", false),
            ("release-plz[bot]-human", false),
            ("release-plz[Bot]", false),
            ("日本語", false),
        ] {
            for author_type in [None, Some("Bot"), Some("User"), Some("bot")] {
                for is_bot in [false, true] {
                    for mergeable in [
                        None,
                        Some("MERGEABLE"),
                        Some("UNKNOWN"),
                        Some("CONFLICTING"),
                    ] {
                        let expected = if bot_login || is_bot || author_type == Some("Bot") {
                            Some(DependencySkip::Bot)
                        } else if mergeable == Some("CONFLICTING") {
                            Some(DependencySkip::BaseConflict)
                        } else {
                            None
                        };
                        assert_eq!(
                            dependency_skip(login, author_type, is_bot, mergeable),
                            expected
                        );
                        proofs::spec_dependency_skip(login, author_type, is_bot, mergeable);
                    }
                }
            }
        }
    }

    #[test]
    fn repository_identity_requires_a_known_common_network() {
        for source in [0, 1, 2, u64::MAX] {
            for target in [0, 1, 2, u64::MAX] {
                for source_network in [0, 1, 2, u64::MAX] {
                    for target_network in [0, 1, 2, u64::MAX] {
                        proofs::spec_source_relation(RepositoryIdentities {
                            source,
                            target,
                            source_network,
                            target_network,
                        });
                    }
                }
            }
        }
    }

    #[test]
    fn every_issue_context_fact_is_required_before_admission() {
        for facts in flags() {
            proofs::spec_context(facts);
        }
        for facts in flags() {
            proofs::spec_initial_change(facts);
        }
        for (scope, path, expected) in [
            ("crates/a", "crates/a", true),
            ("crates/a", "crates/a/b.rs", true),
            ("crates/a", "crates/another.rs", false),
            ("", "crates/a", false),
            ("日本語", "日本語/入力.rs", true),
        ] {
            assert_eq!(registered_path(scope.as_bytes(), path.as_bytes()), expected);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every input")]
    fn every_input_meets_the_pr_specifications() {
        for operation in proofs::OPERATIONS {
            for generation in Generation::ALL {
                for issue in proofs::ISSUES {
                    for checks in [Checks::Success, Checks::Incomplete] {
                        for [validated, draft] in flags() {
                            proofs::spec_plan(
                                operation, generation, validated, draft, issue, checks,
                            );
                        }
                    }
                }
                for review in flags() {
                    proofs::spec_review(operation, generation, review);
                }
            }
            for was in proofs::EXCLUSIONS {
                for now in proofs::EXCLUSIONS {
                    for [draft, paused] in flags() {
                        proofs::spec_pause(operation, draft, was, now, paused);
                    }
                }
            }
        }
        for [first, second, third] in flags() {
            proofs::spec_flags(first, second, third);
        }
        for total in [0, 1, 2, usize::MAX] {
            for unfinished in [0, 1, usize::MAX] {
                for failed in [0, 1, usize::MAX] {
                    proofs::spec_checks(total, unfinished, failed);
                }
            }
        }
        for remaining in [0, 1, 2, u64::MAX] {
            for reserve in [0, 1, 2, u64::MAX] {
                proofs::spec_quota(remaining, reserve);
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every merge state")]
    fn every_merge_state_meets_the_merge_specification() {
        for lifecycle in proofs::LIFECYCLES {
            for checks in proofs::HEAD_CHECKS {
                for exclusion in proofs::EXCLUSIONS {
                    for [conflict, behind, clean, stacked, paused] in flags() {
                        let state = MergeState {
                            lifecycle,
                            conflict,
                            behind,
                            checks,
                            clean,
                            exclusion,
                        };
                        proofs::spec_merge(state, stacked, paused);
                    }
                }
            }
        }
    }

    #[test]
    fn each_generation_has_its_own_key() {
        assert_eq!(
            Generation::ALL.map(Generation::key),
            ["local", "coderabbit"]
        );
        for generation in Generation::ALL {
            assert_eq!(Generation::from_key(generation.key()), Some(generation));
        }
        assert_eq!(Generation::from_key("remote"), None);
    }

    const READY: MergeState = MergeState {
        lifecycle: Lifecycle::Ready,
        conflict: false,
        behind: false,
        checks: HeadChecks::Success,
        clean: true,
        exclusion: Exclusion::Pause,
    };

    #[test]
    fn an_empty_or_unfinished_check_report_is_pending_and_any_failure_fails() {
        assert_eq!(head_checks(0, 0, 0), HeadChecks::Pending);
        assert_eq!(head_checks(3, 1, 0), HeadChecks::Pending);
        assert_eq!(head_checks(3, 1, 1), HeadChecks::Failed);
        assert_eq!(head_checks(3, 0, 0), HeadChecks::Success);
    }

    #[test]
    fn each_merge_state_has_one_next_step() {
        let with = |change: fn(&mut MergeState)| {
            let mut state = READY;
            change(&mut state);
            state
        };
        assert_eq!(merge_step(READY, false, true), Step::Merge);
        assert_eq!(merge_step(READY, true, true), Step::Merge);
        let behind = with(|state| state.behind = true);
        assert_eq!(merge_step(behind, false, true), Step::Update);
        assert_eq!(merge_step(behind, true, true), Step::Restack);
        let conflict = with(|state| {
            state.behind = true;
            state.conflict = true;
        });
        assert_eq!(
            merge_step(conflict, false, true),
            Step::Stop(Stop::Conflict)
        );
        let failed = with(|state| state.checks = HeadChecks::Failed);
        assert_eq!(
            merge_step(failed, false, true),
            Step::Stop(Stop::FailedChecks)
        );
        let pending = with(|state| state.checks = HeadChecks::Pending);
        assert_eq!(merge_step(pending, false, true), Step::Wait);
        let blocked = with(|state| state.clean = false);
        assert_eq!(merge_step(blocked, false, true), Step::Wait);
        let draft = with(|state| {
            state.lifecycle = Lifecycle::Draft;
            state.exclusion = Exclusion::None;
        });
        assert_eq!(merge_step(draft, false, true), Step::Exclude);
        assert_eq!(merge_step(draft, false, false), Step::Ready);
        let excluded = with(|state| state.lifecycle = Lifecycle::Draft);
        assert_eq!(merge_step(excluded, false, true), Step::Ready);
        let reviewed = with(|state| state.exclusion = Exclusion::None);
        assert_eq!(merge_step(reviewed, false, false), Step::Stop(Stop::Review));
        assert_eq!(merge_step(READY, false, false), Step::Stop(Stop::Review));
        let standing = with(|state| state.exclusion = Exclusion::Owner);
        assert_eq!(merge_step(standing, false, false), Step::Merge);
        let merged = with(|state| state.lifecycle = Lifecycle::Merged);
        assert_eq!(merge_step(merged, false, true), Step::Done);
        let closed = with(|state| state.lifecycle = Lifecycle::Closed);
        assert_eq!(merge_step(closed, false, true), Step::Stop(Stop::Closed));
    }
}

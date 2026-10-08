//! The gates each global Git hook runs, in order.
//! `dotfiles hook` runs the plan gate by gate and stops at the first refusal, while a notification only warns.

/// A Git hook that the managed global hooks directory installs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Hook {
    PreCommit,
    CommitMsg,
    PrePush,
    PostCommit,
    PostCheckout,
    PostMerge,
    ReferenceTransaction,
}

/// Every hook, in the order Git documents them.
pub const HOOKS: [Hook; 7] = [
    Hook::PreCommit,
    Hook::CommitMsg,
    Hook::PrePush,
    Hook::PostCommit,
    Hook::PostCheckout,
    Hook::PostMerge,
    Hook::ReferenceTransaction,
];

/// The hook name and its filename in the hooks directory.
#[must_use]
pub const fn name(hook: Hook) -> &'static str {
    match hook {
        Hook::PreCommit => "pre-commit",
        Hook::CommitMsg => "commit-msg",
        Hook::PrePush => "pre-push",
        Hook::PostCommit => "post-commit",
        Hook::PostCheckout => "post-checkout",
        Hook::PostMerge => "post-merge",
        Hook::ReferenceTransaction => "reference-transaction",
    }
}

/// One step of a hook.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Gate {
    /// The machine-local push hold, which only the owner lifts.
    PushHold,
    /// The host's process-start health check.
    Health,
    /// Reads the hook's standard input once, which every later gate receives unchanged.
    ReadInput,
    /// Records the staged tree before any gate can change it.
    StagedTree,
    Dotguard,
    /// The machine and ready-PR gates of a push to a remote URL.
    PushHosts,
    /// Refuses added code of a shape that nothing verifies.
    Verification,
    /// The language and prose check of the commit message.
    CommitProse,
    GlobalLefthook,
    RepositoryLefthook,
    /// Refuses a commit whose staged tree a gate changed.
    StagedTreeCheck,
    Renovate,
    /// Reinstalls tools built from sources a merge changed.
    Reinstall,
    /// Runs storage-scout after the event.
    Storage,
}

/// What the hook found before it plans, none of which needs a gate to run.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each field is one independent fact the hook finds before it plans"
)]
pub struct Configured {
    /// `~/.config/lefthook/global.yml` exists.
    pub global: bool,
    /// The repository root holds a Lefthook configuration.
    pub repository: bool,
    /// The push names a remote URL.
    pub url: bool,
    /// The reference transaction committed.
    pub committed: bool,
    /// The storage-scout executable and automatic policy exist.
    pub storage: bool,
    /// The merge ran in a checkout of this repository.
    pub working_copy: bool,
}

/// The most gates any hook runs.
pub const MOST_GATES: usize = 8;

/// The gates of one hook in the order they run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Plan {
    gates: [Gate; MOST_GATES],
    len: usize,
}

impl Plan {
    const EMPTY: Self = Self {
        gates: [Gate::Health; MOST_GATES],
        len: 0,
    };

    fn then(mut self, gate: Gate, when: bool) -> Self {
        if let (true, Some(slot)) = (when, self.gates.get_mut(self.len)) {
            *slot = gate;
            self.len += 1;
        }
        self
    }

    /// The gates in the order they run.
    #[must_use]
    pub fn gates(&self) -> &[Gate] {
        self.gates.get(..self.len).unwrap_or_default()
    }
}

/// The gates `hook` runs on a host and repository as `configured` describes them.
#[must_use]
pub fn plan(hook: Hook, configured: Configured) -> Plan {
    let plan = Plan::EMPTY;
    match hook {
        Hook::PreCommit => plan
            .then(Gate::Health, true)
            .then(Gate::StagedTree, true)
            .then(Gate::Dotguard, true)
            .then(Gate::Verification, true)
            .then(Gate::GlobalLefthook, configured.global)
            .then(Gate::RepositoryLefthook, configured.repository)
            .then(Gate::StagedTreeCheck, true),
        Hook::CommitMsg => plan
            .then(Gate::Health, true)
            .then(Gate::Dotguard, true)
            .then(Gate::CommitProse, true)
            .then(Gate::RepositoryLefthook, configured.repository),
        Hook::PrePush => plan
            .then(Gate::PushHold, true)
            .then(Gate::Health, true)
            .then(Gate::ReadInput, true)
            .then(Gate::Dotguard, true)
            .then(Gate::PushHosts, configured.url)
            .then(Gate::Verification, true)
            .then(Gate::RepositoryLefthook, configured.repository)
            .then(Gate::Renovate, true),
        Hook::PostCommit => plan.then(Gate::Dotguard, true),
        Hook::PostCheckout => plan
            .then(Gate::RepositoryLefthook, configured.repository)
            .then(Gate::Storage, configured.storage),
        Hook::PostMerge => plan
            .then(Gate::RepositoryLefthook, configured.repository)
            .then(Gate::Reinstall, configured.working_copy)
            .then(Gate::Storage, configured.storage),
        Hook::ReferenceTransaction => plan
            .then(Gate::ReadInput, true)
            .then(Gate::Storage, configured.committed && configured.storage),
    }
}

/// Whether a failure of `gate` only warns, so it never changes the hook's verdict.
#[must_use]
pub const fn notification(gate: Gate) -> bool {
    matches!(gate, Gate::Reinstall | Gate::Storage)
}

/// Whether `gate` receives the hook's standard input.
#[must_use]
pub const fn takes_input(gate: Gate) -> bool {
    matches!(
        gate,
        Gate::Dotguard
            | Gate::PushHosts
            | Gate::Verification
            | Gate::GlobalLefthook
            | Gate::RepositoryLefthook
            | Gate::Renovate
            | Gate::Storage
    )
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// Whether `gate` runs code that could change the index of a commit in progress.
    pub const fn touches_index(gate: Gate) -> bool {
        matches!(
            gate,
            Gate::Dotguard | Gate::Verification | Gate::GlobalLefthook | Gate::RepositoryLefthook
        )
    }

    /// The facts as six flags, in field order.
    pub fn configured(flags: [bool; 6]) -> Configured {
        let [global, repository, url, committed, storage, working_copy] = flags;
        Configured {
            global,
            repository,
            url,
            committed,
            storage,
            working_copy,
        }
    }

    /// One bit for each gate.
    fn bit(gate: Gate) -> u16 {
        1 << (gate as u16)
    }

    /// Runs `gates` with the outcome `passed` gives each and returns the gates that ran and the verdict.
    pub fn execute(gates: &[Gate], passed: impl Fn(usize) -> bool) -> (usize, bool) {
        let mut ran = 0;
        for (index, gate) in gates.iter().enumerate() {
            ran += 1;
            if !passed(index) && !notification(*gate) {
                return (ran, false);
            }
        }
        (ran, true)
    }

    /// The hold comes first, and the hook reads its input once before any gate receives it.
    /// Index snapshots bracket every gate that can change the index, and notifications come last.
    pub fn spec_plan(hook: Hook, configured: Configured) -> Plan {
        let plan = plan(hook, configured);
        let gates = plan.gates();
        assert!(gates.len() <= MOST_GATES);
        let reads = matches!(hook, Hook::PrePush | Hook::ReferenceTransaction);
        let commits = hook == Hook::PreCommit;
        // One pass, so Kani unrolls each loop over at most eight gates once.
        let mut seen: u16 = 0;
        let mut read = false;
        let mut staged = false;
        let mut checked = false;
        let mut notified = false;
        for (index, gate) in gates.iter().enumerate() {
            assert_eq!(seen & bit(*gate), 0, "each gate once");
            seen |= bit(*gate);
            assert_eq!(*gate == Gate::PushHold, hook == Hook::PrePush && index == 0);
            if takes_input(*gate) && reads {
                assert!(read);
            }
            if touches_index(*gate) && commits {
                assert!(staged && !checked);
            }
            if notification(*gate) {
                notified = true;
            } else {
                assert!(!notified);
            }
            match gate {
                Gate::ReadInput => read = true,
                Gate::StagedTree => staged = true,
                Gate::StagedTreeCheck => {
                    checked = true;
                    assert_eq!(index + 1, gates.len());
                }
                _ => {}
            }
        }
        let has = |gate: Gate| seen & bit(gate) != 0;
        assert_eq!(has(Gate::ReadInput), reads);
        assert_eq!(has(Gate::StagedTree), commits);
        assert_eq!(has(Gate::StagedTreeCheck), commits);
        assert_eq!(
            has(Gate::Dotguard),
            matches!(
                hook,
                Hook::PreCommit | Hook::CommitMsg | Hook::PrePush | Hook::PostCommit
            )
        );
        assert_eq!(has(Gate::GlobalLefthook), commits && configured.global);
        assert_eq!(
            has(Gate::RepositoryLefthook),
            configured.repository && !matches!(hook, Hook::PostCommit | Hook::ReferenceTransaction)
        );
        assert_eq!(
            has(Gate::Storage),
            configured.storage
                && match hook {
                    Hook::PostCheckout | Hook::PostMerge => true,
                    Hook::ReferenceTransaction => configured.committed,
                    _ => false,
                }
        );
        plan
    }

    /// The hook stops at the first refused verdict gate, and a failed notification never changes the verdict.
    pub fn spec_execute(gates: &[Gate], outcomes: u8) -> bool {
        let passed = |index: usize| outcomes & (1 << index) != 0;
        let (ran, verdict) = execute(gates, passed);
        let refused = gates
            .iter()
            .enumerate()
            .position(|(index, gate)| !notification(*gate) && !passed(index));
        assert_eq!(verdict, refused.is_none());
        assert_eq!(ran, refused.map_or(gates.len(), |index| index + 1));
        let neutral = gates
            .iter()
            .enumerate()
            .fold(outcomes, |bits, (index, gate)| {
                if notification(*gate) {
                    bits | (1 << index)
                } else {
                    bits
                }
            });
        assert_eq!(
            execute(gates, |index| neutral & (1 << index) != 0).1,
            verdict
        );
        verdict
    }

    /// Equal hook filenames identify the same hook.
    /// Every name uses kebab case.
    pub fn spec_name(left: Hook, right: Hook) -> bool {
        let same = name(left) == name(right);
        assert_eq!(same, left == right);
        assert!(
            name(left)
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
        );
        same
    }

    #[cfg(kani)]
    fn any_hook() -> Hook {
        HOOKS[usize::from(kani::any::<u8>() % 7)]
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(24)]
    fn every_hook_has_its_own_git_hook_name() {
        let same = spec_name(any_hook(), any_hook());
        kani::cover!(same);
        kani::cover!(!same);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(10)]
    fn every_plan_holds_first_reads_once_and_brackets_the_index() {
        let hook = any_hook();
        let flags: [bool; 6] = kani::any();
        let plan = spec_plan(hook, configured(flags));
        kani::cover!(plan.gates().len() == MOST_GATES);
        kani::cover!(plan.gates().is_empty());
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(10)]
    fn every_plan_stops_at_its_first_refusal_and_ignores_notifications() {
        let hook = any_hook();
        let flags: [bool; 6] = kani::any();
        let plan = plan(hook, configured(flags));
        let verdict = spec_execute(plan.gates(), kani::any());
        kani::cover!(verdict);
        kani::cover!(!verdict);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every plan and outcome")]
    fn every_plan_meets_its_specification_for_every_outcome_of_every_gate() {
        let mut names = alloc::vec::Vec::new();
        for hook in HOOKS {
            names.push(name(hook));
            for other in HOOKS {
                proofs::spec_name(hook, other);
            }
            for flags in crate::domain::flags() {
                let plan = proofs::spec_plan(hook, proofs::configured(flags));
                for outcomes in 0..=u8::MAX {
                    proofs::spec_execute(plan.gates(), outcomes);
                }
            }
        }
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), HOOKS.len());
        let full = Configured {
            global: true,
            repository: true,
            url: true,
            committed: true,
            storage: true,
            working_copy: true,
        };
        assert_eq!(
            plan(Hook::PreCommit, full).gates(),
            [
                Gate::Health,
                Gate::StagedTree,
                Gate::Dotguard,
                Gate::Verification,
                Gate::GlobalLefthook,
                Gate::RepositoryLefthook,
                Gate::StagedTreeCheck,
            ]
        );
        assert_eq!(
            plan(Hook::PrePush, full).gates(),
            [
                Gate::PushHold,
                Gate::Health,
                Gate::ReadInput,
                Gate::Dotguard,
                Gate::PushHosts,
                Gate::Verification,
                Gate::RepositoryLefthook,
                Gate::Renovate,
            ]
        );
        assert_eq!(
            plan(Hook::PostMerge, full).gates(),
            [Gate::RepositoryLefthook, Gate::Reinstall, Gate::Storage]
        );
        assert_eq!(plan(Hook::PostCheckout, Configured::default()).gates(), []);
    }
}

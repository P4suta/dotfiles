const MIB: u64 = 1024 * 1024;
const MAX_BYTES: u64 = i64::MAX as u64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub memory: u64,
    pub memory_swap: u64,
    pub cpus: u16,
    pub pids: u32,
    pub seconds: u32,
}

#[must_use]
pub const fn limits(
    memory_mib: u64,
    swap_mib: u64,
    cpus: u16,
    pids: u32,
    seconds: u32,
) -> Option<Limits> {
    let Some(memory) = memory_mib.checked_mul(MIB) else {
        return None;
    };
    let Some(total_mib) = memory_mib.checked_add(swap_mib) else {
        return None;
    };
    let Some(memory_swap) = total_mib.checked_mul(MIB) else {
        return None;
    };
    if memory == 0 || memory_swap > MAX_BYTES || cpus == 0 || pids == 0 || seconds == 0 {
        return None;
    }
    Some(Limits {
        memory,
        memory_swap,
        cpus,
        pids,
        seconds,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Route {
    NativeCi,
    Container,
    Refuse,
}

#[must_use]
pub const fn route(github_actions: bool, container: bool) -> Route {
    match (github_actions, container) {
        (true, _) => Route::NativeCi,
        (false, true) => Route::Container,
        (false, false) => Route::Refuse,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Completion {
    Passed,
    Failed,
    MemoryLimit,
    TimeLimit,
}

#[must_use]
pub const fn completion(oom: bool, timed_out: bool, success: bool) -> Completion {
    match (oom, timed_out, success) {
        (true, _, _) => Completion::MemoryLimit,
        (false, true, _) => Completion::TimeLimit,
        (false, false, true) => Completion::Passed,
        (false, false, false) => Completion::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_tools_refuse_and_ci_stays_native() {
        assert_eq!(route(false, false), Route::Refuse);
        assert_eq!(route(false, true), Route::Container);
        assert_eq!(route(true, false), Route::NativeCi);
        assert_eq!(route(true, true), Route::NativeCi);
    }

    #[test]
    fn resource_limits_reject_zero_and_overflow() {
        for memory in [0, u64::MAX, MAX_BYTES / MIB + 1] {
            assert_eq!(limits(memory, 0, 1, 1, 1), None);
        }
        assert_eq!(limits(1, u64::MAX, 1, 1, 1), None);
        assert_eq!(limits(1, 0, 0, 1, 1), None);
        assert_eq!(limits(1, 0, 1, 0, 1), None);
        assert_eq!(limits(1, 0, 1, 1, 0), None);
        let admitted = limits(2, 1, 2, 256, 10).unwrap();
        assert_eq!(admitted.memory, 2 * MIB);
        assert_eq!(admitted.memory_swap, 3 * MIB);
    }

    #[test]
    fn stopped_runs_never_supply_passing_evidence() {
        for oom in [false, true] {
            for timed_out in [false, true] {
                for success in [false, true] {
                    assert_eq!(
                        completion(oom, timed_out, success) == Completion::Passed,
                        !oom && !timed_out && success
                    );
                }
            }
        }
    }
}

#[cfg(kani)]
mod proofs {
    use super::*;

    #[kani::proof]
    fn limits_are_finite_and_include_swap() {
        let memory: u64 = kani::any();
        let swap: u64 = kani::any();
        let cpus: u16 = kani::any();
        let pids: u32 = kani::any();
        let seconds: u32 = kani::any();
        let admitted = limits(memory, swap, cpus, pids, seconds);
        if let Some(value) = admitted {
            assert!(
                value.memory > 0
                    && value.memory <= value.memory_swap
                    && value.memory_swap <= MAX_BYTES
            );
            assert!(value.cpus > 0 && value.pids > 0 && value.seconds > 0);
            assert_eq!(value.memory / MIB, memory);
            assert_eq!((value.memory_swap - value.memory) / MIB, swap);
        }
        kani::cover!(admitted.is_some());
        kani::cover!(admitted.is_none());
    }

    #[kani::proof]
    fn owner_native_runs_refuse() {
        let ci: bool = kani::any();
        let container: bool = kani::any();
        assert_eq!(route(ci, container) == Route::Refuse, !ci && !container);
        assert_eq!(route(ci, container) == Route::NativeCi, ci);
        kani::cover!(route(ci, container) == Route::Refuse);
        kani::cover!(route(ci, container) != Route::Refuse);
    }

    #[kani::proof]
    fn limit_stops_are_inconclusive() {
        let oom: bool = kani::any();
        let timed_out: bool = kani::any();
        let success: bool = kani::any();
        assert_eq!(
            completion(oom, timed_out, success) == Completion::Passed,
            !oom && !timed_out && success
        );
        if oom {
            assert_eq!(completion(oom, timed_out, success), Completion::MemoryLimit);
        }
        if !oom && timed_out {
            assert_eq!(completion(oom, timed_out, success), Completion::TimeLimit);
        }
        kani::cover!(completion(oom, timed_out, success) == Completion::Passed);
        kani::cover!(completion(oom, timed_out, success) != Completion::Passed);
    }
}

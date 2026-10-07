#[must_use]
pub fn age(uptime: u64, start_ticks: u64, ticks_per_second: u64) -> Option<u64> {
    if ticks_per_second == 0 {
        None
    } else {
        Some(uptime.saturating_sub(elapsed_seconds(start_ticks, ticks_per_second)))
    }
}

fn elapsed_seconds(ticks: u64, frequency: u64) -> u64 {
    ticks / frequency
}

#[expect(
    clippy::fn_params_excessive_bools,
    reason = "Independent observed predicates form the source-bound Kani decision table"
)]
#[must_use]
pub fn killable(
    enabled: bool,
    stopped: bool,
    orphan: bool,
    old: bool,
    grace_complete: bool,
    same_identity: bool,
) -> bool {
    enabled && stopped && orphan && old && grace_complete && same_identity
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// A process age never underflows, needs a known tick rate, and never exceeds the uptime.
    pub fn spec_age(uptime: u64, ticks: u64, hz: u64) {
        match age(uptime, ticks, hz) {
            None => assert_eq!(hz, 0),
            Some(age) => {
                assert_ne!(hz, 0);
                assert!(age <= uptime);
                if uptime == 0 {
                    assert_eq!(age, 0);
                }
            }
        }
    }

    #[cfg(kani)]
    fn arbitrary_elapsed(_ticks: u64, frequency: u64) -> u64 {
        assert_ne!(frequency, 0);
        kani::any()
    }

    /// Each fact `killable` needs holds whenever it admits a process, and losing any one of them refuses it.
    pub fn spec_killable(facts: [bool; 6]) {
        let [enabled, stopped, orphan, old, grace, identity] = facts;
        let admitted = killable(enabled, stopped, orphan, old, grace, identity);
        if admitted {
            assert!(enabled && stopped && orphan && old && grace && identity);
        }
        let mut index = 0;
        while index < facts.len() {
            let mut weakened = facts;
            if let Some(fact) = weakened.get_mut(index) {
                *fact = false;
            }
            let [enabled, stopped, orphan, old, grace, identity] = weakened;
            assert!(!killable(enabled, stopped, orphan, old, grace, identity));
            index += 1;
        }
        assert!(killable(true, true, true, true, true, true));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::stub(super::elapsed_seconds, arbitrary_elapsed)]
    fn process_age_never_underflows_or_divides_by_zero() {
        let uptime: u64 = kani::any();
        let ticks: u64 = kani::any();
        let hz: u64 = kani::any();
        spec_age(uptime, ticks, hz);
        kani::cover!(age(uptime, ticks, hz).is_none());
        kani::cover!(age(uptime, ticks, hz).is_some_and(|value| value > 0));
    }

    #[cfg(kani)]
    #[kani::proof]
    fn running_or_reused_processes_are_never_killable() {
        let facts: [bool; 6] = [
            kani::any(),
            kani::any(),
            kani::any(),
            kani::any(),
            kani::any(),
            kani::any(),
        ];
        spec_killable(facts);
        let [enabled, stopped, orphan, old, grace, identity] = facts;
        kani::cover!(killable(enabled, stopped, orphan, old, grace, identity));
        kani::cover!(
            !killable(enabled, stopped, orphan, old, grace, identity) && stopped && orphan
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALUES: [u64; 7] = [0, 1, 2, 99, 100, 101, u64::MAX];

    #[test]
    fn every_boundary_meets_the_age_specification() {
        for uptime in VALUES {
            for ticks in VALUES {
                for hz in VALUES {
                    proofs::spec_age(uptime, ticks, hz);
                }
            }
        }
        assert_eq!(age(10, 250, 100), Some(8));
        assert_eq!(age(1, 250, 100), Some(0));
        assert_eq!(age(10, 250, 0), None);
    }

    #[test]
    fn every_set_of_facts_meets_the_kill_specification() {
        for facts in crate::domain::flags() {
            proofs::spec_killable(facts);
        }
    }
}

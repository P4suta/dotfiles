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

#[cfg(kani)]
fn arbitrary_elapsed(_ticks: u64, frequency: u64) -> u64 {
    assert_ne!(frequency, 0);
    kani::any()
}

#[expect(
    clippy::fn_params_excessive_bools,
    reason = "Independent observed predicates form the source-bound Kani decision table"
)]
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

#[cfg(kani)]
#[kani::proof]
#[kani::stub(elapsed_seconds, arbitrary_elapsed)]
fn process_age_never_underflows_or_divides_by_zero() {
    let uptime: u64 = kani::any();
    let ticks: u64 = kani::any();
    let hz: u64 = kani::any();
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
    kani::cover!(age(uptime, ticks, hz).is_none());
    kani::cover!(age(uptime, ticks, hz).is_some_and(|value| value > 0));
}

#[cfg(kani)]
#[kani::proof]
fn tick_conversion_rejects_zero_frequency() {
    let ticks: u64 = kani::any();
    let hz: u64 = kani::any();
    assert_eq!(age(0, ticks, hz), if hz == 0 { None } else { Some(0) });
    kani::cover!(age(0, ticks, hz).is_none());
    kani::cover!(age(0, ticks, hz).is_some());
}

#[cfg(kani)]
#[kani::proof]
fn running_or_reused_processes_are_never_killable() {
    let enabled: bool = kani::any();
    let stopped: bool = kani::any();
    let orphan: bool = kani::any();
    let old: bool = kani::any();
    let grace: bool = kani::any();
    let identity: bool = kani::any();
    let admitted = killable(enabled, stopped, orphan, old, grace, identity);
    assert_eq!(
        admitted,
        enabled && stopped && orphan && old && grace && identity
    );
    kani::cover!(admitted);
    kani::cover!(!admitted);
}

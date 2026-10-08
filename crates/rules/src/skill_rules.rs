#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    Keep,
    Implemented,
    Rejected,
    Deferred,
}

impl Outcome {
    pub const ALL: [Self; 4] = [
        Self::Keep,
        Self::Implemented,
        Self::Rejected,
        Self::Deferred,
    ];

    /// The name a decision record stores.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Keep => "keep",
            Self::Implemented => "implemented",
            Self::Rejected => "rejected",
            Self::Deferred => "deferred",
        }
    }

    /// The outcome a decision record names, or `None` for a name no outcome carries.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|outcome| outcome.key() == key)
    }
}

#[must_use]
pub fn decision_complete(outcome: Outcome, reason: bool, evidence: bool, revisit: bool) -> bool {
    reason && evidence && (outcome != Outcome::Deferred || revisit)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tracked {
    Current,
    Missing,
    Stale,
    Incomplete,
}

#[must_use]
pub fn tracked_decision(present: bool, bound: bool, complete: bool) -> Tracked {
    if !present {
        Tracked::Missing
    } else if !bound {
        Tracked::Stale
    } else if !complete {
        Tracked::Incomplete
    } else {
        Tracked::Current
    }
}

#[must_use]
pub fn adopted(bound: bool, outcome: Outcome) -> bool {
    bound && outcome != Outcome::Deferred
}

#[must_use]
pub fn triage_complete(unadopted: usize, review_verified: bool) -> bool {
    unadopted == 0 || review_verified
}

/// Settles maintenance by the recorded analysis or by a current analysis with nothing to triage.
#[must_use]
pub fn gate_complete(recorded_current: bool, open_findings: usize) -> bool {
    recorded_current || open_findings == 0
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sizing {
    Within,
    Settled,
    Missing,
    Incomplete,
    Unexpected,
}

#[must_use]
pub fn size_disposition(over_limit: bool, recorded: bool, complete: bool) -> Sizing {
    match (over_limit, recorded, complete) {
        (false, false, _) => Sizing::Within,
        (false, true, _) => Sizing::Unexpected,
        (true, false, _) => Sizing::Missing,
        (true, true, false) => Sizing::Incomplete,
        (true, true, true) => Sizing::Settled,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DiffBase {
    Whole,
    Assessed,
    Unavailable,
}

/// Shows a diff only from a commit that holds the content the previous decision assessed.
#[must_use]
pub fn diff_base(previous: bool, assessed_commit: bool) -> DiffBase {
    match (previous, assessed_commit) {
        (false, _) => DiffBase::Whole,
        (true, true) => DiffBase::Assessed,
        (true, false) => DiffBase::Unavailable,
    }
}

/// The hexadecimal digits of a SHA-256 digest.
pub const DIGEST_LENGTH: usize = 64;

/// A pair composes only when it shares at least this many sessions.
pub const MINIMUM_PAIR_FLOOR: usize = 2;

pub const PERCENT: usize = 100;

#[must_use]
pub fn immutable_identity_valid(length: usize, hexadecimal: bool) -> bool {
    length == DIGEST_LENGTH && hexadecimal
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InstallationStage {
    Policy,
    Runtime,
    Adapter,
}

#[must_use]
pub fn installation_stage(policy_applied: bool, runtime_installed: bool) -> InstallationStage {
    if !policy_applied {
        InstallationStage::Policy
    } else if !runtime_installed {
        InstallationStage::Runtime
    } else {
        InstallationStage::Adapter
    }
}

#[must_use]
pub fn composition_candidate(
    shared: usize,
    first: usize,
    second: usize,
    minimum: usize,
    percent: usize,
) -> bool {
    shared <= first
        && shared <= second
        && minimum >= MINIMUM_PAIR_FLOOR
        && (1..=PERCENT).contains(&percent)
        && shared >= minimum
        && (shared as u128) * (PERCENT as u128) >= (first.min(second) as u128) * (percent as u128)
}

#[derive(Clone, Copy, Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is an independent fact, and the proofs range over every combination"
)]
pub struct Maintenance {
    pub catalog_current: bool,
    pub policy_current: bool,
    pub observations_preserved: bool,
    pub notes_current: bool,
    pub review_complete: bool,
    pub new_observations: usize,
    pub threshold: usize,
}

#[must_use]
pub fn maintenance_current(value: Maintenance) -> bool {
    value.catalog_current
        && value.policy_current
        && value.observations_preserved
        && value.notes_current
        && value.review_complete
        && value.new_observations < value.threshold
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Completion {
    Allow,
    RequestMaintenance,
    EndIncomplete,
}

#[must_use]
pub fn completion(current: bool, already_continued: bool) -> Completion {
    if current {
        Completion::Allow
    } else if already_continued {
        Completion::EndIncomplete
    } else {
        Completion::RequestMaintenance
    }
}

/// The rest of `text` after `prefix`, or `None` when `text` doesn't start with it.
fn after<'a>(mut text: &'a [u8], mut prefix: &[u8]) -> Option<&'a [u8]> {
    while let [expected, rest @ ..] = prefix {
        let [actual, tail @ ..] = text else {
            return None;
        };
        if actual != expected {
            return None;
        }
        (text, prefix) = (tail, rest);
    }
    Some(text)
}

/// Whether `text` names `term`, such as a pinned version, as a whole: no digit or `.` comes right before it, and no digit or `.` with a digit after it comes right after.
/// A skill that names what a gate already pins drifts from the pin, so the skill check refuses it.
#[must_use]
pub fn names(text: &[u8], term: &[u8]) -> bool {
    if term.is_empty() {
        return false;
    }
    let mut extends = false;
    let mut rest = text;
    loop {
        let continued = match after(rest, term) {
            Some(following) => matches!(following, [b'0'..=b'9', ..] | [b'.', b'0'..=b'9', ..]),
            None => true,
        };
        if !extends && !continued {
            return true;
        }
        let [byte, tail @ ..] = rest else {
            return false;
        };
        extends = matches!(byte, b'0'..=b'9' | b'.');
        rest = tail;
    }
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// Whether `term` occurs in `text` at the front of `rest`, with the byte `previous` before it.
    fn occurs_at(previous: Option<u8>, rest: &[u8], term: &[u8]) -> bool {
        let mut text = rest;
        let mut wanted = term;
        loop {
            match (text, wanted) {
                (following, []) => {
                    return !matches!(previous, Some(b'0'..=b'9' | b'.'))
                        && !matches!(following, [b'0'..=b'9', ..] | [b'.', b'0'..=b'9', ..]);
                }
                ([], [_, ..]) => return false,
                ([a, text_rest @ ..], [b, wanted_rest @ ..]) => {
                    if a != b {
                        return false;
                    }
                    (text, wanted) = (text_rest, wanted_rest);
                }
            }
        }
    }

    /// A term counts at some position whose neighbors don't continue a number, and an empty term never counts.
    #[expect(
        clippy::manual_assert_eq,
        reason = "assert_eq formats its operands, which leaves Kani an unreachable formatting check that fails the proof"
    )]
    pub fn spec_names(text: &[u8], term: &[u8]) {
        let mut expected = false;
        let mut previous = None;
        let mut rest = text;
        loop {
            expected |= !term.is_empty() && occurs_at(previous, rest, term);
            let [byte, tail @ ..] = rest else {
                break;
            };
            previous = Some(*byte);
            rest = tail;
        }
        assert!(names(text, term) == expected);
    }

    /// Each two-flag decision takes exactly the state its rule names.
    pub fn spec_pairs(first: bool, second: bool) {
        let stage = installation_stage(first, second);
        assert_eq!(stage == InstallationStage::Policy, !first);
        assert_eq!(stage == InstallationStage::Runtime, first && !second);
        assert_eq!(stage == InstallationStage::Adapter, first && second);
        let action = completion(first, second);
        assert_eq!(action == Completion::Allow, first);
        assert_eq!(action == Completion::RequestMaintenance, !first && !second);
        assert_eq!(action == Completion::EndIncomplete, !first && second);
        let base = diff_base(first, second);
        assert_eq!(base == DiffBase::Whole, !first);
        assert_eq!(base == DiffBase::Assessed, first && second);
        assert_eq!(base == DiffBase::Unavailable, first && !second);
    }

    pub fn spec_triples(present: bool, bound: bool, complete: bool) {
        let state = tracked_decision(present, bound, complete);
        assert_eq!(state == Tracked::Current, present && bound && complete);
        assert_eq!(state == Tracked::Missing, !present);
        assert_eq!(state == Tracked::Stale, present && !bound);
        assert_eq!(state == Tracked::Incomplete, present && bound && !complete);
        let (over_limit, recorded) = (present, bound);
        let sizing = size_disposition(over_limit, recorded, complete);
        assert_eq!(sizing == Sizing::Within, !over_limit && !recorded);
        assert_eq!(sizing == Sizing::Unexpected, !over_limit && recorded);
        assert_eq!(sizing == Sizing::Missing, over_limit && !recorded);
        assert_eq!(
            sizing == Sizing::Incomplete,
            over_limit && recorded && !complete
        );
        assert_eq!(
            sizing == Sizing::Settled,
            over_limit && recorded && complete
        );
    }

    pub fn spec_outcome(outcome: Outcome, reason: bool, evidence: bool, revisit: bool) {
        assert_eq!(
            decision_complete(outcome, reason, evidence, revisit),
            reason && evidence && (outcome != Outcome::Deferred || revisit)
        );
        assert_eq!(
            adopted(reason, outcome),
            reason && outcome != Outcome::Deferred
        );
    }

    pub fn spec_counts(count: usize, flag: bool) {
        assert_eq!(triage_complete(count, flag), count == 0 || flag);
        assert_eq!(gate_complete(flag, count), flag || count == 0);
        assert_eq!(
            immutable_identity_valid(count, flag),
            count == DIGEST_LENGTH && flag
        );
    }

    /// A pair composes only from enough shared sessions whose share meets the ratio, and the arithmetic never overflows.
    pub fn spec_composition_sound(
        shared: usize,
        first: usize,
        second: usize,
        minimum: usize,
        percent: usize,
    ) -> bool {
        let composes = composition_candidate(shared, first, second, minimum, percent);
        if composes {
            assert!(shared <= first && shared <= second && shared >= minimum && minimum >= 2);
            assert!((1..=100).contains(&percent));
            assert!(shared as u128 * 100 >= first.min(second) as u128 * percent as u128);
        }
        composes
    }

    /// Every pair with enough shared sessions and share composes, which the tests check since Kani can't finish this direction within the budget.
    pub fn spec_composition(
        shared: usize,
        first: usize,
        second: usize,
        minimum: usize,
        percent: usize,
    ) {
        let composes = spec_composition_sound(shared, first, second, minimum, percent);
        let smaller = if first < second { first } else { second };
        let expected = shared <= first
            && shared <= second
            && minimum >= 2
            && (1..=100).contains(&percent)
            && shared >= minimum
            && shared as u128 * 100 >= smaller as u128 * percent as u128;
        assert_eq!(composes, expected);
    }

    pub fn spec_maintenance(value: Maintenance) {
        assert_eq!(
            maintenance_current(value),
            value.catalog_current
                && value.policy_current
                && value.observations_preserved
                && value.notes_current
                && value.review_complete
                && value.threshold > 0
                && value.new_observations < value.threshold
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    fn every_flag_decision_takes_the_state_its_rule_names() {
        let flags: [bool; 3] = [kani::any(), kani::any(), kani::any()];
        let [first, second, third] = flags;
        spec_pairs(first, second);
        spec_triples(first, second, third);
        let outcome = Outcome::ALL[usize::from(kani::any::<u8>() % 4)];
        spec_outcome(outcome, first, second, third);
        let count: usize = kani::any();
        spec_counts(count, third);
        kani::cover!(adopted(first, outcome) && decision_complete(outcome, first, second, third));
        kani::cover!(size_disposition(first, second, third) == Sizing::Unexpected);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn composition_respects_sample_and_ratio_without_overflow() {
        let shared: usize = kani::any();
        let first: usize = kani::any();
        let second: usize = kani::any();
        let minimum: usize = kani::any();
        let percent: usize = kani::any();
        let composes = spec_composition_sound(shared, first, second, minimum, percent);
        kani::cover!(composes);
        kani::cover!(!composes);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn stale_or_unprocessed_maintenance_never_completes() {
        let value = Maintenance {
            catalog_current: kani::any(),
            policy_current: kani::any(),
            observations_preserved: kani::any(),
            notes_current: kani::any(),
            review_complete: kani::any(),
            new_observations: kani::any(),
            threshold: kani::any(),
        };
        spec_maintenance(value);
        kani::cover!(maintenance_current(value));
        kani::cover!(!maintenance_current(value) && value.catalog_current);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(7)]
    fn a_term_counts_only_where_no_number_continues_it() {
        let text: [u8; 5] = kani::any();
        let term: [u8; 2] = kani::any();
        spec_names(&text, &term);
        kani::cover!(names(&text, &term));
        kani::cover!(!names(&text, &term) && text[1] == term[0] && text[2] == term[1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::flags;

    const COUNTS: [usize; 7] = [0, 1, 2, 3, DIGEST_LENGTH, 100, usize::MAX];

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every short text and term")]
    fn every_short_text_and_term_meets_the_naming_specification() {
        for text in crate::domain::words(b"0.a", 4) {
            for term in crate::domain::words(b"0.", 2) {
                proofs::spec_names(&text, &term);
            }
        }
    }

    #[test]
    fn a_pinned_version_counts_only_as_a_whole() {
        assert!(names(b"Kani 0.68.0 runs", b"0.68.0"));
        assert!(names(b"pinned at 0.68.0.", b"0.68.0"));
        assert!(names(b"kani-verifier@0.68.0", b"0.68.0"));
        assert!(names(b"v0.68.0", b"0.68.0"));
        assert!(names(b"0.68.0", b"0.68.0"));
        assert!(!names(b"10.68.0", b"0.68.0"));
        assert!(!names(b"0.68.01", b"0.68.0"));
        assert!(!names(b"0.68.0.1", b"0.68.0"));
        assert!(!names(b"1.0.68.0", b"0.68.0"));
        assert!(!names(b"0.6", b"0.68.0"));
        assert!(!names(b"anything", b""));
        assert!(names(b"a 10.68.0 and 0.68.0", b"0.68.0"));
    }

    #[test]
    fn every_input_meets_the_decision_specifications() {
        for [first, second, third] in flags() {
            proofs::spec_pairs(first, second);
            proofs::spec_triples(first, second, third);
            for outcome in Outcome::ALL {
                proofs::spec_outcome(outcome, first, second, third);
            }
        }
        for count in COUNTS
            .into_iter()
            .chain([DIGEST_LENGTH - 1, DIGEST_LENGTH + 1])
        {
            proofs::spec_counts(count, false);
            proofs::spec_counts(count, true);
        }
        for [
            catalog_current,
            policy_current,
            observations_preserved,
            notes_current,
            review_complete,
        ] in flags()
        {
            for new_observations in [0, 1, 2, usize::MAX] {
                for threshold in [0, 1, 2, usize::MAX] {
                    proofs::spec_maintenance(Maintenance {
                        catalog_current,
                        policy_current,
                        observations_preserved,
                        notes_current,
                        review_complete,
                        new_observations,
                        threshold,
                    });
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every boundary")]
    fn every_boundary_meets_the_composition_specification() {
        for shared in COUNTS {
            for first in COUNTS {
                for second in COUNTS {
                    for minimum in [0, 1, 2, 3] {
                        for percent in [0, 1, 50, 99, 100, 101] {
                            proofs::spec_composition(shared, first, second, minimum, percent);
                        }
                    }
                }
            }
        }
        for shared in 0..=10 {
            for first in 0..=10 {
                for percent in [1, 30, 50, 70, 100] {
                    proofs::spec_composition(shared, first, first + 1, 2, percent);
                    proofs::spec_composition(shared, first + 1, first, 2, percent);
                }
            }
        }
    }

    #[test]
    fn each_outcome_has_its_own_key() {
        assert_eq!(
            Outcome::ALL.map(Outcome::key),
            ["keep", "implemented", "rejected", "deferred"]
        );
        assert_eq!(Outcome::from_key("skip"), None);
        for outcome in Outcome::ALL {
            assert_eq!(Outcome::from_key(outcome.key()), Some(outcome));
        }
    }
}

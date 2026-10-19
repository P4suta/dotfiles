#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Evidence {
    Absent,
    Untrusted,
    Incompatible,
    Incomplete,
    Verified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Disposition {
    Measure,
    Reuse,
    Refuse,
}

#[must_use]
pub const fn disposition(producer: bool, evidence: Evidence, pending: bool) -> Disposition {
    if !producer {
        return Disposition::Measure;
    }
    match evidence {
        Evidence::Absent => Disposition::Measure,
        Evidence::Verified if pending => Disposition::Measure,
        Evidence::Verified => Disposition::Reuse,
        Evidence::Untrusted | Evidence::Incompatible | Evidence::Incomplete => Disposition::Refuse,
    }
}

#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    #[cfg(kani)]
    #[kani::proof]
    fn every_reuse_requires_an_authorized_complete_origin_without_a_pending_attempt() {
        let producer: bool = kani::any();
        let evidence: Evidence = kani::any();
        let pending: bool = kani::any();
        let result = disposition(producer, evidence, pending);
        assert_eq!(
            result == Disposition::Reuse,
            producer && evidence == Evidence::Verified && !pending
        );
        kani::cover!(result == Disposition::Reuse);
        kani::cover!(result != Disposition::Reuse);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn every_invalid_existing_origin_is_refused_before_measurement() {
        let producer: bool = kani::any();
        let evidence: Evidence = kani::any();
        let pending: bool = kani::any();
        let invalid = matches!(
            evidence,
            Evidence::Untrusted | Evidence::Incompatible | Evidence::Incomplete
        );
        let result = disposition(producer, evidence, pending);
        assert_eq!(result == Disposition::Refuse, producer && invalid);
        kani::cover!(result == Disposition::Refuse);
        kani::cover!(result != Disposition::Refuse);
    }

    #[test]
    fn a_new_unverified_attempt_removes_previous_success_credit() {
        assert_eq!(
            disposition(true, Evidence::Verified, false),
            Disposition::Reuse
        );
        assert_eq!(
            disposition(true, Evidence::Verified, true),
            Disposition::Measure
        );
        assert_eq!(
            disposition(false, Evidence::Verified, false),
            Disposition::Measure
        );
        assert_eq!(
            disposition(true, Evidence::Absent, false),
            Disposition::Measure
        );
        for evidence in [
            Evidence::Untrusted,
            Evidence::Incompatible,
            Evidence::Incomplete,
        ] {
            assert_eq!(disposition(true, evidence, false), Disposition::Refuse);
            assert_eq!(disposition(true, evidence, true), Disposition::Refuse);
        }
    }
}

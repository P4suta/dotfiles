//! Whether a rendered third-party configuration keeps the contract its consumer's vendored schema declares.

/// What the gate measured for one rendered target that a vendored schema governs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Target {
    /// The target conforms to the schema.
    pub conforms: bool,
    /// The target with an unknown key planted at its top level fails the schema.
    pub planted_refused: bool,
}

/// The decision for one rendered target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Conforms,
    /// The target breaks the schema, so its consumer would read a key it doesn't know or a value it rejects.
    Violates,
    /// The schema admits an unknown key, so a misspelled key can render and apply.
    Open,
}

/// A target conforms only when it validates and the schema refuses a key it doesn't declare.
#[must_use]
pub const fn verdict(target: Target) -> Verdict {
    if !target.conforms {
        Verdict::Violates
    } else if target.planted_refused {
        Verdict::Conforms
    } else {
        Verdict::Open
    }
}

/// The decision for one vendored schema over every rendered profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Standing {
    Holds,
    /// The vendored file differs from the digest the manifest records.
    Drifted,
    /// No profile renders a target the schema governs, so the schema checks nothing.
    Unwitnessed,
}

/// A schema holds only when its bytes keep the recorded digest and at least one rendered target witnesses it.
#[must_use]
pub const fn standing(digest_matches: bool, witnesses: u32) -> Standing {
    if !digest_matches {
        Standing::Drifted
    } else if witnesses == 0 {
        Standing::Unwitnessed
    } else {
        Standing::Holds
    }
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_verdict(conforms: bool, planted_refused: bool) {
        let result = verdict(Target {
            conforms,
            planted_refused,
        });
        assert_eq!(result == Verdict::Conforms, conforms && planted_refused);
        assert_eq!(result == Verdict::Violates, !conforms);
    }

    pub fn spec_standing(digest_matches: bool, witnesses: u32) {
        let result = standing(digest_matches, witnesses);
        assert_eq!(result == Standing::Holds, digest_matches && witnesses > 0);
        assert_eq!(result == Standing::Drifted, !digest_matches);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_a_conforming_target_of_a_closed_schema_conforms() {
        let conforms: bool = kani::any();
        let planted_refused: bool = kani::any();
        spec_verdict(conforms, planted_refused);
        let result = verdict(Target {
            conforms,
            planted_refused,
        });
        kani::cover!(result == Verdict::Conforms);
        kani::cover!(result == Verdict::Violates);
        kani::cover!(result == Verdict::Open);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_an_unchanged_witnessed_schema_holds() {
        let digest_matches: bool = kani::any();
        let witnesses: u32 = kani::any();
        spec_standing(digest_matches, witnesses);
        let result = standing(digest_matches, witnesses);
        kani::cover!(result == Standing::Holds);
        kani::cover!(result == Standing::Drifted);
        kani::cover!(result == Standing::Unwitnessed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_input_meets_the_specification_and_a_planted_key_or_a_drift_is_refused() {
        for [conforms, planted_refused] in crate::domain::flags() {
            proofs::spec_verdict(conforms, planted_refused);
        }
        for [digest_matches] in crate::domain::flags() {
            for witnesses in [0, 1, 2, u32::MAX] {
                proofs::spec_standing(digest_matches, witnesses);
            }
        }
        let open = Target {
            conforms: true,
            planted_refused: false,
        };
        assert_eq!(verdict(open), Verdict::Open);
        assert_eq!(standing(false, 3), Standing::Drifted);
        assert_eq!(standing(true, 0), Standing::Unwitnessed);
    }
}

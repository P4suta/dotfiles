/// How a repository differs from what its metadata file generates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Drift {
    Current,
    /// The license expression or a package manifest names another license.
    License,
    /// A contributor besides the copyright holder and bots has no recorded consent to the dual license.
    Consent,
    /// The README or a license file differs from the generated text.
    Generated,
    /// The GitHub repository description differs from the summary.
    Description,
}

/// The first drift to fix.
/// The license and the consent decide what the generated files may say, so they come first.
/// The description comes last, because only the owner's account changes it.
#[must_use]
#[expect(
    clippy::fn_params_excessive_bools,
    reason = "each flag reports one independent check of the repository, and the verdict orders them"
)]
pub fn drift(license: bool, consent: bool, generated: bool, description: bool) -> Drift {
    if !license {
        Drift::License
    } else if !consent {
        Drift::Consent
    } else if !generated {
        Drift::Generated
    } else if !description {
        Drift::Description
    } else {
        Drift::Current
    }
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// A repository passes only when every part matches, and a failing license or consent always names itself first.
    #[expect(
        clippy::fn_params_excessive_bools,
        reason = "the specification takes the same four checks as the verdict"
    )]
    pub fn spec_drift(license: bool, consent: bool, generated: bool, description: bool) {
        let result = drift(license, consent, generated, description);
        assert_eq!(
            result == Drift::Current,
            license && consent && generated && description
        );
        assert_eq!(result == Drift::License, !license);
        assert_eq!(result == Drift::Consent, license && !consent);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_repository_passes_only_when_every_generated_part_matches() {
        let (license, consent, generated, description): (bool, bool, bool, bool) =
            (kani::any(), kani::any(), kani::any(), kani::any());
        spec_drift(license, consent, generated, description);
        let result = drift(license, consent, generated, description);
        kani::cover!(result == Drift::Current);
        kani::cover!(result == Drift::Description);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_input_meets_the_specification() {
        for [license, consent, generated, description] in crate::domain::flags() {
            super::proofs::spec_drift(license, consent, generated, description);
        }
        assert_eq!(
            super::drift(true, true, false, false),
            super::Drift::Generated
        );
    }
}

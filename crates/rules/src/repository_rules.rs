/// How a repository differs from what its metadata file generates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Drift {
    Current,
    /// Package manifests differ from the license state declared in metadata.
    License,
    /// A contributor besides the copyright holder and bots has no recorded consent to the dual license.
    Consent,
    /// The README or a license file differs from the generated text.
    Generated,
    /// The GitHub repository description differs from the summary.
    Description,
}

/// Whether the repository generates license texts or preserves reviewed assets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum LicenseAssets {
    Generated,
    PreservedDual {
        mit_matches: bool,
        apache_matches: bool,
    },
    PreservedExisting {
        all_match: bool,
    },
    Unlicensed,
}

/// The files generation may write after checking the license assets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Generation {
    All,
    Readme,
    Refused,
}

/// The exact license state a package manifest may declare.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum LicenseExpression {
    MitOrApache,
    ApacheOnly,
    MitOnly,
    AgplOnly,
    AgplOrLater,
    Unlicensed,
}

const MAX_LICENSE_EXPRESSION_BYTES: usize = 17;

/// Whether an expression length is within the longest recognized declaration.
#[must_use]
const fn supported_license_expression_length(length: usize) -> bool {
    length <= MAX_LICENSE_EXPRESSION_BYTES
}

/// Whether a package manifest declares the exact expression for its license state.
#[must_use]
pub fn matches_license_expression(state: LicenseExpression, expression: &[u8]) -> bool {
    if !supported_license_expression_length(expression.len()) {
        return false;
    }

    match state {
        LicenseExpression::MitOrApache => {
            expression == b"MIT OR Apache-2.0" || expression == b"Apache-2.0 OR MIT"
        }
        LicenseExpression::ApacheOnly => expression == b"Apache-2.0",
        LicenseExpression::MitOnly => expression == b"MIT",
        LicenseExpression::AgplOnly => expression == b"AGPL-3.0-only",
        LicenseExpression::AgplOrLater => expression == b"AGPL-3.0-or-later",
        LicenseExpression::Unlicensed => false,
    }
}

/// Selects generated artifacts from the license state and its reviewed assets.
#[must_use]
pub fn generation(assets: LicenseAssets) -> Generation {
    match assets {
        LicenseAssets::Generated => Generation::All,
        LicenseAssets::PreservedDual {
            mit_matches: true,
            apache_matches: true,
        } => Generation::Readme,
        LicenseAssets::PreservedExisting { all_match: true } | LicenseAssets::Unlicensed => {
            Generation::Readme
        }
        LicenseAssets::PreservedDual { .. }
        | LicenseAssets::PreservedExisting { all_match: false } => Generation::Refused,
    }
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

    pub fn spec_license_expression(state: LicenseExpression, expression: &[u8]) {
        let expected = match state {
            LicenseExpression::MitOrApache => {
                expression == b"MIT OR Apache-2.0" || expression == b"Apache-2.0 OR MIT"
            }
            LicenseExpression::ApacheOnly => expression == b"Apache-2.0",
            LicenseExpression::MitOnly => expression == b"MIT",
            LicenseExpression::AgplOnly => expression == b"AGPL-3.0-only",
            LicenseExpression::AgplOrLater => expression == b"AGPL-3.0-or-later",
            LicenseExpression::Unlicensed => false,
        };
        assert_eq!(matches_license_expression(state, expression), expected);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn every_bounded_expression_matches_only_its_exact_license_state() {
        let state: LicenseExpression = kani::any();
        let bytes: [u8; MAX_LICENSE_EXPRESSION_BYTES] = kani::any();
        let length: usize = kani::any();
        kani::assume(length <= MAX_LICENSE_EXPRESSION_BYTES);
        let expression = &bytes[..length];
        spec_license_expression(state, expression);
        kani::cover!(matches_license_expression(state, expression));
        kani::cover!(!matches_license_expression(state, expression));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn expressions_longer_than_the_supported_bound_are_rejected() {
        let state: LicenseExpression = kani::any();
        let bytes: [u8; MAX_LICENSE_EXPRESSION_BYTES + 1] = kani::any();
        assert!(!matches_license_expression(state, &bytes));

        let length: usize = kani::any();
        assert_eq!(
            supported_license_expression_length(length),
            length <= MAX_LICENSE_EXPRESSION_BYTES
        );
        kani::cover!(length > MAX_LICENSE_EXPRESSION_BYTES);
    }

    pub fn spec_generation(assets: LicenseAssets) {
        let result = generation(assets);
        assert_eq!(
            result == Generation::All,
            assets == LicenseAssets::Generated
        );
        assert_eq!(
            result == Generation::Readme,
            matches!(
                assets,
                LicenseAssets::PreservedDual {
                    mit_matches: true,
                    apache_matches: true
                } | LicenseAssets::PreservedExisting { all_match: true }
                    | LicenseAssets::Unlicensed
            )
        );
        assert_eq!(
            result == Generation::Refused,
            matches!(
                assets,
                LicenseAssets::PreservedDual {
                    mit_matches: false,
                    ..
                } | LicenseAssets::PreservedDual {
                    apache_matches: false,
                    ..
                } | LicenseAssets::PreservedExisting { all_match: false }
            )
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_matching_reviewed_licenses_allow_readme_generation() {
        let assets: LicenseAssets = kani::any();
        spec_generation(assets);
        kani::cover!(assets == LicenseAssets::Generated);
        kani::cover!(
            assets
                == LicenseAssets::PreservedDual {
                    mit_matches: true,
                    apache_matches: true
                }
        );
        kani::cover!(assets == LicenseAssets::PreservedExisting { all_match: true });
        kani::cover!(assets == LicenseAssets::Unlicensed);
        kani::cover!(generation(assets) == Generation::Refused);
    }

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
    fn every_license_asset_state_meets_the_specification() {
        super::proofs::spec_generation(super::LicenseAssets::Generated);
        for [mit_matches, apache_matches] in crate::domain::flags() {
            super::proofs::spec_generation(super::LicenseAssets::PreservedDual {
                mit_matches,
                apache_matches,
            });
            super::proofs::spec_generation(super::LicenseAssets::PreservedExisting {
                all_match: mit_matches,
            });
        }
        super::proofs::spec_generation(super::LicenseAssets::Unlicensed);
    }

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

    #[test]
    fn every_license_expression_fixture_meets_the_specification() {
        for state in [
            super::LicenseExpression::MitOrApache,
            super::LicenseExpression::ApacheOnly,
            super::LicenseExpression::MitOnly,
            super::LicenseExpression::AgplOnly,
            super::LicenseExpression::AgplOrLater,
            super::LicenseExpression::Unlicensed,
        ] {
            let expressions: [&[u8]; 12] = [
                b"MIT OR Apache-2.0".as_slice(),
                b"Apache-2.0 OR MIT",
                b"Apache-2.0",
                b"MIT",
                b"AGPL-3.0-only",
                b"AGPL-3.0-or-later",
                b"agpl-3.0-or-later",
                b"MIT ",
                b"AGPL-3.0-or-later ",
                b"",
                &[0xff],
                b"unknown license expression",
            ];
            for expression in expressions {
                super::proofs::spec_license_expression(state, expression);
            }
        }
        assert!(!super::matches_license_expression(
            super::LicenseExpression::MitOnly,
            b"AGPL-3.0-or-later extra"
        ));
    }
}

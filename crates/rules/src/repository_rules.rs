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

/// Whether the repository generates license texts or preserves reviewed assets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum LicenseAssets {
    Generated,
    Preserved {
        mit_matches: bool,
        apache_matches: bool,
    },
}

/// The files generation may write after checking the license assets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Generation {
    All,
    Readme,
    Refused,
}

/// README generation requires both preserved assets to match.
#[must_use]
pub fn generation(assets: LicenseAssets) -> Generation {
    match assets {
        LicenseAssets::Generated => Generation::All,
        LicenseAssets::Preserved {
            mit_matches: true,
            apache_matches: true,
        } => Generation::Readme,
        LicenseAssets::Preserved { .. } => Generation::Refused,
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

    pub fn spec_generation(assets: LicenseAssets) {
        let result = generation(assets);
        assert_eq!(
            result == Generation::All,
            assets == LicenseAssets::Generated
        );
        assert_eq!(
            result == Generation::Readme,
            assets
                == LicenseAssets::Preserved {
                    mit_matches: true,
                    apache_matches: true
                }
        );
        assert_eq!(
            result == Generation::Refused,
            matches!(
                assets,
                LicenseAssets::Preserved {
                    mit_matches: false,
                    ..
                } | LicenseAssets::Preserved {
                    apache_matches: false,
                    ..
                }
            )
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_matching_reviewed_licenses_allow_readme_generation() {
        let assets: LicenseAssets = kani::any();
        spec_generation(assets);
        kani::cover!(generation(assets) == Generation::All);
        kani::cover!(generation(assets) == Generation::Readme);
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
            super::proofs::spec_generation(super::LicenseAssets::Preserved {
                mit_matches,
                apache_matches,
            });
        }
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
}

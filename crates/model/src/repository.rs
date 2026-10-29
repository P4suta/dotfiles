//! The metadata file every owner repository carries, from which `dotfiles repository` generates the README, the GitHub description, and the license files.

use rules::repository_rules::{LicenseExpression, matches_license_expression};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The file at the root of a repository, relative to that root.
pub const FILE: &str = "repository.toml";

/// The canonical dual-license expression for repositories that generate license files.
pub const LICENSE: &str = "MIT OR Apache-2.0";

/// The license state the repository already declares or the generator creates.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub enum License {
    /// Generate the owner's existing dual-license files.
    #[serde(rename = "MIT OR Apache-2.0")]
    MitOrApache,
    /// Preserve the existing Apache-2.0-only license assets.
    #[serde(rename = "Apache-2.0")]
    ApacheOnly,
    /// Preserve the existing Massachusetts Institute of Technology (MIT) license assets.
    #[serde(rename = "MIT")]
    MitOnly,
    /// Preserve the existing Affero General Public License version 3 assets.
    #[serde(rename = "AGPL-3.0-only")]
    AgplOnly,
    /// Preserve the existing Affero General Public License version 3 or later assets.
    #[serde(rename = "AGPL-3.0-or-later")]
    AgplOrLater,
    /// Keep the repository unlicensed and make no license claim.
    #[serde(rename = "unlicensed")]
    Unlicensed,
}

impl License {
    /// Whether a package manifest declares this exact existing license state.
    #[must_use]
    pub fn matches_expression(self, expression: &str) -> bool {
        let state = match self {
            Self::MitOrApache => LicenseExpression::MitOrApache,
            Self::ApacheOnly => LicenseExpression::ApacheOnly,
            Self::MitOnly => LicenseExpression::MitOnly,
            Self::AgplOnly => LicenseExpression::AgplOnly,
            Self::AgplOrLater => LicenseExpression::AgplOrLater,
            Self::Unlicensed => LicenseExpression::Unlicensed,
        };
        matches_license_expression(state, expression.as_bytes())
    }
}

/// What a repository says about itself.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Repository {
    /// The project name, which titles the README.
    pub name: String,
    /// One line without a final period, which serves as the GitHub description and the README's first sentence.
    pub summary: String,
    /// The commands that install the project, one per line.
    pub install: String,
    /// The smallest useful example of use.
    pub usage: String,
    /// The language of the usage block, such as `sh` or `rust`.
    #[serde(default = "shell")]
    pub usage_language: String,
    /// Link the generated README to `docs/README.md`.
    #[serde(default)]
    pub documentation_index: bool,
    /// Preserve reviewed existing license assets at these SHA-256 digests.
    #[serde(default)]
    pub preserved_licenses: Option<PreservedLicenses>,
    /// The copyright holder that the generated MIT license names.
    #[serde(default)]
    pub copyright: Option<String>,
    /// The existing license state with no new license claim for `unlicensed`.
    pub license: License,
    /// Each contributor besides the copyright holder and bots, with the record of their consent to the dual license.
    #[serde(default)]
    pub consent: Vec<Consent>,
}

/// The reviewed bytes of existing license assets.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreservedLicenses {
    /// The SHA-256 digest of `LICENSE-MIT` for the existing dual-license format.
    #[serde(default)]
    pub mit_sha256: Option<String>,
    /// The SHA-256 digest of `LICENSE-APACHE` for the existing dual-license format.
    #[serde(default)]
    pub apache_sha256: Option<String>,
    /// Root-level existing license files with their reviewed SHA-256 digests.
    #[serde(default)]
    pub assets: Vec<PreservedLicenseAsset>,
}

/// One root-level existing license asset whose bytes must stay unchanged.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreservedLicenseAsset {
    /// The root-level path of the existing license file.
    pub path: String,
    /// The SHA-256 digest of its exact bytes.
    pub sha256: String,
}

/// One contributor's consent to license their contributions under both licenses.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Consent {
    /// The name the Git history records for the contributor.
    pub contributor: String,
    /// Where the contributor agreed, such as an issue comment URL.
    pub evidence: String,
}

fn shell() -> String {
    "sh".to_owned()
}

/// Parses a metadata file, naming the key that fails.
///
/// # Errors
///
/// Returns the parser's message when the text holds invalid TOML, an unknown key, or a value of the wrong type.
pub fn parse(text: &str) -> Result<Repository, String> {
    let value: Repository = toml::from_str(text).map_err(|error| format!("{FILE}: {error}"))?;
    for (name, text) in [
        ("name", &value.name),
        ("summary", &value.summary),
        ("install", &value.install),
        ("usage", &value.usage),
    ] {
        if text.trim().is_empty() {
            return Err(format!("{FILE}: {name} must not be empty"));
        }
    }
    for (name, text) in [("name", &value.name), ("summary", &value.summary)] {
        if text.contains(['\n', '\r']) {
            return Err(format!("{FILE}: {name} must fit on one line"));
        }
    }
    if let Some(copyright) = &value.copyright {
        if copyright.trim().is_empty() {
            return Err(format!("{FILE}: copyright must not be empty when present"));
        }
        if copyright.contains(['\n', '\r']) {
            return Err(format!("{FILE}: copyright must fit on one line"));
        }
    }
    if !value
        .usage_language
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        || value.usage_language.is_empty()
    {
        return Err(format!(
            "{FILE}: usage_language must name a code-block language"
        ));
    }
    validate_license(&value)?;
    Ok(value)
}

fn validate_license(value: &Repository) -> Result<(), String> {
    let preserved = value.preserved_licenses.as_ref();
    match value.license {
        License::MitOrApache => {
            if value.copyright.as_deref().is_none_or(str::is_empty) {
                return Err(format!("{FILE}: copyright is required for {LICENSE}"));
            }
            if let Some(preserved) = preserved {
                if preserved.mit_sha256.is_none()
                    || preserved.apache_sha256.is_none()
                    || !preserved.assets.is_empty()
                {
                    return Err(format!(
                        "{FILE}: dual-license preservation requires only mit_sha256 and apache_sha256"
                    ));
                }
                if !preserved.mit_sha256.as_deref().is_some_and(sha256)
                    || !preserved.apache_sha256.as_deref().is_some_and(sha256)
                {
                    return Err(format!(
                        "{FILE}: dual-license preservation requires SHA-256 digests"
                    ));
                }
            }
        }
        License::ApacheOnly | License::MitOnly | License::AgplOnly | License::AgplOrLater => {
            let Some(preserved) = preserved else {
                return Err(format!(
                    "{FILE}: {} requires reviewed preserved license assets",
                    license_name(value.license)
                ));
            };
            if preserved.mit_sha256.is_some()
                || preserved.apache_sha256.is_some()
                || preserved.assets.is_empty()
            {
                return Err(format!(
                    "{FILE}: {} requires only one or more preserved asset records",
                    license_name(value.license)
                ));
            }
            if !value.consent.is_empty() {
                return Err(format!(
                    "{FILE}: consent records apply only when generating {LICENSE}"
                ));
            }
            let mut paths = Vec::new();
            for asset in &preserved.assets {
                if !root_file_name(&asset.path) {
                    return Err(format!(
                        "{FILE}: preserved license path must be a safe root-level file name"
                    ));
                }
                if asset.path.eq_ignore_ascii_case("README.md") {
                    return Err(format!(
                        "{FILE}: README.md can't be a preserved license asset because generation rewrites it"
                    ));
                }
                if !sha256(&asset.sha256) {
                    return Err(format!(
                        "{FILE}: preserved license asset {} needs a SHA-256 digest",
                        asset.path
                    ));
                }
                if paths.contains(&asset.path.as_str()) {
                    return Err(format!(
                        "{FILE}: duplicate preserved license path {}",
                        asset.path
                    ));
                }
                paths.push(asset.path.as_str());
            }
        }
        License::Unlicensed => {
            if preserved.is_some() || !value.consent.is_empty() {
                return Err(format!(
                    "{FILE}: unlicensed state can't carry license assets or consent records"
                ));
            }
        }
    }
    Ok(())
}

fn license_name(license: License) -> &'static str {
    match license {
        License::MitOrApache => LICENSE,
        License::ApacheOnly => "Apache-2.0",
        License::MitOnly => "MIT",
        License::AgplOnly => "AGPL-3.0-only",
        License::AgplOrLater => "AGPL-3.0-or-later",
        License::Unlicensed => "unlicensed",
    }
}

fn root_file_name(path: &str) -> bool {
    !path.is_empty()
        && path != "."
        && path != ".."
        && !path.contains(['/', '\\', ':'])
        && path
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

const SHA256_HEX_DIGEST_LENGTH: usize = 64;

fn sha256(digest: &str) -> bool {
    digest.len() == SHA256_HEX_DIGEST_LENGTH && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// The JSON schema of the metadata file, which `just gen` writes to `schemas/repository.schema.json`.
#[must_use]
pub fn schema() -> String {
    let schema = schemars::schema_for!(Repository);
    format!(
        "{}\n",
        serde_json::to_string_pretty(&schema).unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::{License, parse};

    const DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn metadata(license: &str, extra: &str) -> String {
        format!(
            "name = \"fixture\"\nsummary = \"A fixture repository\"\ninstall = \"run\"\nusage = \"run --help\"\n{license}{extra}"
        )
    }

    #[test]
    fn existing_dual_license_metadata_keeps_its_source_shape() {
        let parsed = parse(&metadata(
            "copyright = \"Owner\"\nlicense = \"MIT OR Apache-2.0\"\n",
            "",
        ))
        .unwrap();
        assert_eq!(parsed.license, License::MitOrApache);
        assert_eq!(parsed.copyright.as_deref(), Some("Owner"));
        assert_eq!(parsed.preserved_licenses, None);
    }

    #[test]
    fn existing_single_license_states_require_reviewed_root_assets() {
        for (expression, expected) in [
            ("Apache-2.0", License::ApacheOnly),
            ("MIT", License::MitOnly),
            ("AGPL-3.0-only", License::AgplOnly),
            ("AGPL-3.0-or-later", License::AgplOrLater),
        ] {
            let text = metadata(
                &format!("license = \"{expression}\"\n"),
                &format!(
                    "\n[preserved_licenses]\n[[preserved_licenses.assets]]\npath = \"LICENSE\"\nsha256 = \"{DIGEST}\"\n"
                ),
            );
            let parsed = parse(&text).unwrap();
            assert_eq!(parsed.license, expected);
            assert_eq!(parsed.copyright, None);
        }
    }

    #[test]
    fn unlicensed_state_needs_no_copyright_or_preserved_assets() {
        let parsed = parse(&metadata("license = \"unlicensed\"\n", "")).unwrap();
        assert_eq!(parsed.license, License::Unlicensed);
        assert_eq!(parsed.copyright, None);
    }

    #[test]
    fn single_license_assets_reject_unsafe_paths_and_malformed_digests() {
        for (path, digest) in [
            ("../LICENSE", DIGEST),
            ("README.md", DIGEST),
            ("readme.MD", DIGEST),
            ("LICENSE", "bad"),
        ] {
            let text = metadata(
                "license = \"Apache-2.0\"\n",
                &format!(
                    "\n[preserved_licenses]\n[[preserved_licenses.assets]]\npath = \"{path}\"\nsha256 = \"{digest}\"\n"
                ),
            );
            assert!(parse(&text).is_err());
        }
    }

    #[test]
    fn unlicensed_state_rejects_license_assets() {
        let text = metadata(
            "license = \"unlicensed\"\n",
            &format!("\n[preserved_licenses]\nmit_sha256 = \"{DIGEST}\"\n"),
        );
        assert!(parse(&text).is_err());
    }

    #[test]
    fn dual_license_manifest_order_is_equivalent() {
        assert!(License::MitOrApache.matches_expression("MIT OR Apache-2.0"));
        assert!(License::MitOrApache.matches_expression("Apache-2.0 OR MIT"));
        assert!(!License::MitOrApache.matches_expression("MIT"));
    }

    #[test]
    fn single_license_manifest_expressions_match_only_their_exact_state() {
        for (license, expression) in [
            (License::ApacheOnly, "Apache-2.0"),
            (License::MitOnly, "MIT"),
            (License::AgplOnly, "AGPL-3.0-only"),
            (License::AgplOrLater, "AGPL-3.0-or-later"),
        ] {
            assert!(license.matches_expression(expression));
            for other in ["Apache-2.0", "MIT", "AGPL-3.0-only", "AGPL-3.0-or-later"] {
                assert_eq!(
                    license.matches_expression(other),
                    other == expression,
                    "license={license:?}, expression={other}"
                );
            }
        }
    }
}

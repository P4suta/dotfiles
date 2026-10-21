//! The metadata file every owner repository carries, from which `dotfiles repository` generates the README, the GitHub description, and the license files.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The file at the root of a repository, relative to that root.
pub const FILE: &str = "repository.toml";

/// The one license every owner repository takes, as the Rust API Guidelines recommend in C-PERMISSIVE.
pub const LICENSE: &str = "MIT OR Apache-2.0";

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
    /// The copyright holder that `LICENSE-MIT` names.
    pub copyright: String,
    /// The license expression, which must read `MIT OR Apache-2.0`.
    pub license: String,
    /// Each contributor besides the copyright holder and bots, with the record of their consent to the dual license.
    #[serde(default)]
    pub consent: Vec<Consent>,
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
        ("copyright", &value.copyright),
    ] {
        if text.trim().is_empty() {
            return Err(format!("{FILE}: {name} must not be empty"));
        }
    }
    for (name, text) in [
        ("name", &value.name),
        ("summary", &value.summary),
        ("copyright", &value.copyright),
    ] {
        if text.contains(['\n', '\r']) {
            return Err(format!("{FILE}: {name} must fit on one line"));
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
    Ok(value)
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

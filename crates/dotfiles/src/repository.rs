//! The README, the license files, and the GitHub description that every owner repository generates from its `repository.toml`, and the gate that refuses drift from them.

use crate::refusal::Refusal;
use crate::repository_rules::{Drift, drift};
use crate::tool::Tool;
use anyhow::{Context, Result};
use model::repository::{FILE, LICENSE, Repository};
use std::fs;
use std::path::Path;

/// The one README template of every owner repository.
const README: &str = include_str!("../assets/repository/README.template");

/// The license texts, which the Rust API Guidelines recommend shipping side by side.
const MIT: &str = include_str!("../assets/repository/LICENSE-MIT");
const APACHE: &str = include_str!("../assets/repository/LICENSE-APACHE");

/// The command that rewrites the generated files.
pub const GENERATE: &str = "dotfiles repository generate";

/// The generated files, by path relative to the repository root.
pub type Generated = Vec<(&'static str, String)>;

/// Reads `repository.toml` at `root`.
///
/// # Errors
///
/// Returns an error when the repository lacks the file or the file fails to parse.
pub fn metadata(root: &Path) -> Result<Repository> {
    let text = fs::read_to_string(root.join(FILE)).with_context(|| {
        format!(
            "{} carries no {FILE}; write one with the name, summary, install, usage, copyright, and license, as schemas/repository.schema.json in the dotfiles repository describes",
            root.display()
        )
    })?;
    model::repository::parse(&text).map_err(|error| anyhow::anyhow!(error))
}

/// The README and the license files that `metadata` generates.
#[must_use]
pub fn generate(metadata: &Repository) -> Generated {
    let heading = match metadata.name.char_indices().next() {
        Some((_, first)) => {
            let mut upper = first.to_uppercase().to_string();
            upper.push_str(&metadata.name[first.len_utf8()..]);
            upper
        }
        None => metadata.name.clone(),
    };
    let readme = README
        .replace("{heading}", &heading)
        .replace("{summary}", metadata.summary.trim_end_matches('.'))
        .replace("{install}", metadata.install.trim_end())
        .replace("{usage_language}", &metadata.usage_language)
        .replace("{usage}", metadata.usage.trim_end());
    vec![
        ("README.md", readme),
        (
            "LICENSE-MIT",
            MIT.replace("{copyright}", &metadata.copyright),
        ),
        ("LICENSE-APACHE", APACHE.to_owned()),
    ]
}

/// Writes the generated files at `root`.
///
/// # Errors
///
/// Returns an error when the metadata fails to read or a file fails to write.
pub fn write(root: &Path) -> Result<()> {
    let metadata = metadata(root)?;
    anyhow::ensure!(
        metadata.license == LICENSE,
        "{FILE} must declare {LICENSE} before generating license files"
    );
    let missing = contributors(root, &metadata)?;
    anyhow::ensure!(
        missing.is_empty(),
        "record contributor consent before generating license files: {}",
        missing.join(", ")
    );
    for (path, text) in generate(&metadata) {
        fs::write(root.join(path), text).with_context(|| format!("write {path}"))?;
    }
    Ok(())
}

/// Sets the remote description from the same metadata as the generated README.
///
/// # Errors
///
/// Returns an error when the metadata or the GitHub update fails.
pub fn write_description(root: &Path, remote: &str) -> Result<()> {
    let metadata = metadata(root)?;
    let status = Tool::Gh
        .command()
        .args([
            "repo",
            "edit",
            remote,
            "--description",
            metadata.summary.trim_end_matches('.'),
        ])
        .status()?;
    anyhow::ensure!(
        status.success(),
        "gh could not set the generated description"
    );
    Ok(())
}

/// The package manifests whose license differs from the metadata's: each tracked `Cargo.toml` with a package, and `package.json` when it names a license.
fn manifests_off_license(root: &Path, license: &str) -> Result<Vec<String>> {
    let listed = Tool::Git
        .command()
        .current_dir(root)
        .args(["ls-files", "-z", "--", "*Cargo.toml", "package.json"])
        .output()?;
    let mut off = Vec::new();
    anyhow::ensure!(
        listed.status.success(),
        "Git could not enumerate package manifests"
    );
    for path in String::from_utf8_lossy(&listed.stdout)
        .split('\0')
        .filter(|path| !path.is_empty())
    {
        let text = fs::read_to_string(root.join(path))?;
        if path.ends_with("package.json") {
            let value: serde_json::Value = serde_json::from_str(&text)?;
            if value
                .get("license")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|named| named != license)
            {
                off.push(path.to_owned());
            }
            continue;
        }
        let value: toml::Table = toml::from_str(&text).with_context(|| format!("parse {path}"))?;
        let declared = |table: Option<&toml::Value>| -> Option<bool> {
            table
                .and_then(|table| table.get("license"))
                .map(|license_value| {
                    license_value.as_str() == Some(license)
                        || license_value
                            .get("workspace")
                            .and_then(toml::Value::as_bool)
                            .unwrap_or(false)
                })
        };
        let package = value.get("package");
        let workspace = value
            .get("workspace")
            .and_then(|workspace| workspace.get("package"));
        let fine = match (package, workspace) {
            (None, None) => true,
            (_, Some(_)) if declared(workspace) == Some(false) => false,
            (Some(_), _) => declared(package).unwrap_or(false),
            (None, Some(_)) => declared(workspace).unwrap_or(false),
        };
        if !fine {
            off.push(path.to_owned());
        }
    }
    Ok(off)
}

/// The authors in the history besides the copyright holder and bots, which need recorded consent before relicensing.
fn contributors(root: &Path, metadata: &Repository) -> Result<Vec<String>> {
    let log = Tool::Git
        .command()
        .current_dir(root)
        .args(["log", "--all", "--format=%an"])
        .output()?;
    let mut found: Vec<String> = String::from_utf8_lossy(&log.stdout)
        .lines()
        .filter(|author| *author != metadata.copyright && !author.contains("[bot]"))
        .map(str::to_owned)
        .collect();
    anyhow::ensure!(log.status.success(), "Git could not enumerate contributors");
    found.sort();
    found.dedup();
    Ok(found
        .into_iter()
        .filter(|author| {
            !metadata.consent.iter().any(|consent| {
                &consent.contributor == author && !consent.evidence.trim().is_empty()
            })
        })
        .collect())
}

/// The GitHub description of `repository`, through `gh`.
fn description(repository: &str) -> Result<String> {
    let output = Tool::Gh
        .command()
        .args([
            "repo",
            "view",
            repository,
            "--json",
            "description",
            "--jq",
            ".description",
        ])
        .output()
        .context("read the GitHub description through gh")?;
    anyhow::ensure!(
        output.status.success(),
        "gh could not read {repository}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Refuses a repository whose README, license files, manifests, consent, or, with `remote`, GitHub description differ from what `repository.toml` generates.
///
/// # Errors
///
/// Returns a structured refusal naming the first drift and its next action, or an error when an input fails to read.
pub fn check(root: &Path, remote: Option<&str>) -> Result<()> {
    let metadata = metadata(root)?;
    let mut stale = Vec::new();
    for (path, text) in generate(&metadata) {
        let current = fs::read_to_string(root.join(path))
            .unwrap_or_default()
            .replace("\r\n", "\n");
        if current != text {
            stale.push(path.to_owned());
        }
    }
    let mut off = manifests_off_license(root, LICENSE)?;
    if metadata.license != LICENSE {
        off.insert(0, FILE.to_owned());
    }
    let missing = contributors(root, &metadata)?;
    let described = match remote {
        Some(repository) => description(repository)? == metadata.summary.trim_end_matches('.'),
        None => true,
    };
    let (rule, cause, next, evidence) = match drift(off.is_empty(), missing.is_empty(), stale.is_empty(), described) {
        Drift::Current => return Ok(()),
        Drift::License => (
            "repository.license",
            format!("the repository must take {LICENSE}, with both license files and matching manifests"),
            format!("set license = \"{LICENSE}\" in each listed file, then run `{GENERATE}`"),
            off,
        ),
        Drift::Consent => (
            "repository.consent",
            "contributors besides the copyright holder have no recorded consent to the dual license".to_owned(),
            "record each contributor's consent under [[consent]] in repository.toml".to_owned(),
            missing,
        ),
        Drift::Generated => (
            "repository.generated",
            "the README or a license file differs from what repository.toml generates".to_owned(),
            GENERATE.to_owned(),
            stale,
        ),
        Drift::Description => (
            "repository.description",
            "the GitHub description differs from the summary in repository.toml".to_owned(),
            format!(
                "{GENERATE} --remote {}",
                remote.unwrap_or_default(),
            ),
            vec![metadata.summary.clone()],
        ),
    };
    let mut refusal = Refusal::new(rule, cause, next);
    for item in evidence {
        refusal = refusal.evidence(item);
    }
    Err(refusal.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let git = |args: &[&str]| {
            let status = Tool::Git
                .command()
                .current_dir(root)
                .args([
                    "-c",
                    "user.name=Owner",
                    "-c",
                    "user.email=owner@example.invalid",
                ])
                .args(args)
                .status()
                .unwrap();
            assert!(status.success(), "{args:?}");
        };
        git(&["init", "-q"]);
        fs::write(
            root.join(FILE),
            "name = \"fixture\"\nsummary = \"A fixture repository\"\ninstall = \"cargo install fixture\"\nusage = \"fixture --help\"\ncopyright = \"Owner\"\nlicense = \"MIT OR Apache-2.0\"\n",
        )
        .unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nlicense = \"MIT OR Apache-2.0\"\n",
        )
        .unwrap();
        write(root).unwrap();
        git(&["add", "."]);
        directory
    }

    fn refused(root: &Path) -> Refusal {
        let error = check(root, None).unwrap_err();
        error
            .downcast_ref::<Refusal>()
            .expect("a structured refusal")
            .clone()
    }

    #[test]
    fn a_generated_repository_passes_and_each_drift_names_the_regenerate_command() {
        let directory = fixture();
        let root = directory.path();
        check(root, None).unwrap();
        let readme = fs::read_to_string(root.join("README.md")).unwrap();
        assert!(
            readme.starts_with("# Fixture\n\nA fixture repository.\n"),
            "{readme}"
        );
        fs::write(root.join("README.md"), format!("{readme}\n## Roadmap\n")).unwrap();
        let drift = refused(root);
        assert_eq!(
            (drift.rule.as_str(), drift.next.as_deref()),
            ("repository.generated", Some(GENERATE))
        );
        assert!(drift.is_complete(), "{drift:?}");
        write(root).unwrap();
        check(root, None).unwrap();
        fs::remove_file(root.join("LICENSE-APACHE")).unwrap();
        assert_eq!(refused(root).evidence, ["LICENSE-APACHE"]);
        write(root).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nlicense = \"MIT\"\n",
        )
        .unwrap();
        let single = refused(root);
        assert_eq!(
            (single.rule.as_str(), single.evidence.as_slice()),
            ("repository.license", &["Cargo.toml".to_owned()][..])
        );
    }
}

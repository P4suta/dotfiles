//! The README and GitHub description that every owner repository generates from its `repository.toml`, and its license-state checks.

use crate::refusal::Refusal;
use crate::repository_rules::{Drift, Generation, LicenseAssets, drift, generation};
use crate::tool::Tool;
use anyhow::{Context, Result};
use model::repository::{FILE, License, Repository};
use platform::build_inputs::hex;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// The one README template of every owner repository.
const README: &str = include_str!("../assets/repository/README.template");

const DOCUMENTATION_INDEX: &str = "docs/README.md";
const README_PATH: &str = "README.md";
const MIT_PATH: &str = "LICENSE-MIT";
const APACHE_PATH: &str = "LICENSE-APACHE";
const GITHUB_REPOSITORY_PREFIX: &str = "https://github.com/";

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
            "{} carries no {FILE}; write one with the name, summary, install, usage, and existing license state, as schemas/repository.schema.json in the dotfiles repository describes",
            root.display()
        )
    })?;
    let parsed = model::repository::parse(&text).map_err(|error| anyhow::anyhow!(error))?;
    anyhow::ensure!(
        !parsed.documentation_index || root.join(DOCUMENTATION_INDEX).is_file(),
        "{FILE}: documentation_index requires {DOCUMENTATION_INDEX} to be a file"
    );
    Ok(parsed)
}

/// The README and the license files that `metadata` generates.
#[must_use]
fn generate(metadata: &Repository) -> Generated {
    let heading = match metadata.name.char_indices().next() {
        Some((_, first)) => {
            let mut upper = first.to_uppercase().to_string();
            upper.push_str(&metadata.name[first.len_utf8()..]);
            upper
        }
        None => metadata.name.clone(),
    };
    let documentation = if metadata.documentation_index {
        format!("## Documentation\n\n[Read the documentation]({DOCUMENTATION_INDEX}).\n\n")
    } else {
        String::new()
    };
    let license = license_section(metadata);
    let readme = README
        .replace("{documentation}", &documentation)
        .replace("{license}", &license)
        .replace("{heading}", &heading)
        .replace("{summary}", metadata.summary.trim_end_matches('.'))
        .replace("{install}", metadata.install.trim_end())
        .replace("{usage_language}", &metadata.usage_language)
        .replace("{usage}", metadata.usage.trim_end());
    let readme = format!("{}\n", readme.trim_end_matches('\n'));
    let mut generated = vec![(README_PATH, readme)];
    if metadata.license == License::MitOrApache {
        generated.push((
            MIT_PATH,
            MIT.replace(
                "{copyright}",
                metadata
                    .copyright
                    .as_deref()
                    .expect("validated dual-license metadata has a copyright holder"),
            ),
        ));
        generated.push((APACHE_PATH, APACHE.to_owned()));
    }
    generated
}

fn license_section(metadata: &Repository) -> String {
    match metadata.license {
        License::MitOrApache => {
            let mut section = concat!(
                "## License\n\n",
                "Licensed under the [Apache License, Version 2.0](LICENSE-APACHE) or the ",
                "[Massachusetts Institute of Technology (MIT) license](LICENSE-MIT), at your option."
            )
            .to_owned();
            if metadata.preserved_licenses.is_none() {
                section
                    .push_str("\n\nContributions use both licenses unless you state other terms.");
            }
            section
        }
        License::ApacheOnly | License::AgplOnly => {
            let links = metadata
                .preserved_licenses
                .as_ref()
                .into_iter()
                .flat_map(|preserved| &preserved.assets)
                .map(|asset| format!("[{}]({})", asset.path, asset.path))
                .collect::<Vec<_>>();
            format!(
                "## License\n\nThe existing license terms are in {}.",
                links.join(" and ")
            )
        }
        License::Unlicensed => String::new(),
    }
}

fn license_matches(root: &Path, path: &str, expected: &str) -> bool {
    let path = root.join(path);
    let Ok(metadata) = fs::symlink_metadata(&path) else {
        return false;
    };
    if !metadata.file_type().is_file() {
        return false;
    }
    let Ok(bytes) = fs::read(&path) else {
        return false;
    };
    let readme = root.join(README_PATH);
    match fs::symlink_metadata(&readme) {
        Ok(metadata) if metadata.file_type().is_file() => {
            if same_file::is_same_file(&path, &readme).unwrap_or(true) {
                return false;
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        _ => return false,
    }
    hex(Sha256::digest(bytes).as_ref()).eq_ignore_ascii_case(expected)
}

fn generated_readme_is_safe(root: &Path) -> Result<()> {
    let path = root.join(README_PATH);
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            anyhow::ensure!(
                metadata.file_type().is_file(),
                "refusing to follow a non-regular generated README path"
            );
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn artifacts(root: &Path, metadata: &Repository) -> Result<Generated> {
    let mut mismatched = Vec::new();
    let tracked =
        if metadata.preserved_licenses.is_some() && metadata.license != License::Unlicensed {
            Some(tracked_paths(root)?)
        } else {
            None
        };
    let assets = match metadata.license {
        License::MitOrApache => {
            metadata
                .preserved_licenses
                .as_ref()
                .map_or(LicenseAssets::Generated, |reviewed| {
                    let mit_matches = reviewed.mit_sha256.as_deref().is_some_and(|digest| {
                        tracked
                            .as_ref()
                            .is_some_and(|paths| paths.contains(MIT_PATH.as_bytes()))
                            && license_matches(root, MIT_PATH, digest)
                    });
                    let apache_matches = reviewed.apache_sha256.as_deref().is_some_and(|digest| {
                        tracked
                            .as_ref()
                            .is_some_and(|paths| paths.contains(APACHE_PATH.as_bytes()))
                            && license_matches(root, APACHE_PATH, digest)
                    });
                    if !mit_matches {
                        mismatched.push(MIT_PATH.to_owned());
                    }
                    if !apache_matches {
                        mismatched.push(APACHE_PATH.to_owned());
                    }
                    LicenseAssets::PreservedDual {
                        mit_matches,
                        apache_matches,
                    }
                })
        }
        License::ApacheOnly | License::AgplOnly => {
            if let Some(preserved) = &metadata.preserved_licenses {
                for asset in &preserved.assets {
                    if !tracked
                        .as_ref()
                        .is_some_and(|paths| paths.contains(asset.path.as_bytes()))
                        || !license_matches(root, &asset.path, &asset.sha256)
                    {
                        mismatched.push(asset.path.clone());
                    }
                }
            }
            LicenseAssets::PreservedExisting {
                all_match: mismatched.is_empty(),
            }
        }
        License::Unlicensed => LicenseAssets::Unlicensed,
    };
    match generation(assets) {
        Generation::All => Ok(generate(metadata)),
        Generation::Readme => Ok(generate(metadata)
            .into_iter()
            .filter(|(path, _)| *path == README_PATH)
            .collect()),
        Generation::Refused => anyhow::bail!(
            "{FILE}: preserved license assets are missing or differ from their reviewed SHA-256 digests: {}",
            mismatched.join(", ")
        ),
    }
}

fn tracked_paths(root: &Path) -> Result<HashSet<Vec<u8>>> {
    let output = Tool::Git
        .command()
        .current_dir(root)
        .args(["ls-files", "--cached", "-z"])
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "Git could not enumerate tracked files"
    );
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(<[u8]>::to_vec)
        .collect())
}

/// Writes the generated files at `root`.
///
/// # Errors
///
/// Returns an error when the metadata fails to read or a file fails to write.
pub fn write(root: &Path) -> Result<()> {
    let metadata = metadata(root)?;
    generated_readme_is_safe(root)?;
    let off = manifests_off_license(root, &metadata)?;
    anyhow::ensure!(
        off.is_empty(),
        "{FILE}: package manifests do not match the declared license state: {}",
        off.join(", ")
    );
    let missing = if metadata.license == License::MitOrApache {
        contributors(root, &metadata)?
    } else {
        Vec::new()
    };
    anyhow::ensure!(
        missing.is_empty(),
        "record contributor consent before generating license files: {}",
        missing.join(", ")
    );
    for (path, text) in artifacts(root, &metadata)? {
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
    let repository = github_repository(root, remote)?;
    let status = Tool::Gh
        .command()
        .args([
            "repo",
            "edit",
            &repository,
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
fn manifests_off_license(root: &Path, metadata: &Repository) -> Result<Vec<String>> {
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
            let declared = value.get("license").filter(|value| !value.is_null());
            let matches = match (metadata.license, declared) {
                (License::Unlicensed, Some(value)) => value.as_str() == Some("UNLICENSED"),
                (_, None) => true,
                (_, Some(value)) => value
                    .as_str()
                    .is_some_and(|named| metadata.license.matches_expression(named)),
            };
            if !matches {
                off.push(path.to_owned());
            }
            continue;
        }
        let value: toml::Table = toml::from_str(&text).with_context(|| format!("parse {path}"))?;
        let package = value.get("package");
        let local_workspace = value
            .get("workspace")
            .and_then(|workspace| workspace.get("package"))
            .cloned();
        if path != "Cargo.toml"
            && local_workspace
                .as_ref()
                .is_some_and(|table| table.get("license-file").is_some())
        {
            off.push(path.to_owned());
            continue;
        }
        let workspace = local_workspace.map_or_else(
            || inherited_workspace_package(root, path, package),
            |table| Ok(Some(table)),
        )?;
        let workspace = workspace.as_ref();
        let package_state =
            cargo_declaration_matches(root, path, package, workspace, metadata, false);
        let workspace_state =
            cargo_declaration_matches(root, path, workspace, None, metadata, true);
        let fine = if metadata.license == License::Unlicensed {
            match (package, workspace) {
                (None, None) => true,
                (_, Some(_)) if workspace_state.is_some() => false,
                (Some(_), _) => package_state.is_none(),
                (None, Some(_)) => workspace_state.is_none(),
            }
        } else {
            match (package, workspace) {
                (None, None) => true,
                (_, Some(_)) if workspace_state == Some(false) => false,
                (Some(_), _) => package_state.unwrap_or(false),
                (None, Some(_)) => workspace_state.unwrap_or(false),
            }
        };
        if !fine {
            off.push(path.to_owned());
        }
    }
    Ok(off)
}

fn cargo_declaration_matches(
    root: &Path,
    manifest_path: &str,
    table: Option<&toml::Value>,
    workspace: Option<&toml::Value>,
    metadata: &Repository,
    workspace_package: bool,
) -> Option<bool> {
    let table = table?;
    let license = table.get("license");
    let license_file = table.get("license-file");
    if license.is_some() && license_file.is_some() {
        return Some(false);
    }
    if let Some(license_file) = license_file {
        let (path, inherited) = match license_file {
            toml::Value::String(path) => (path.as_str(), false),
            toml::Value::Table(values)
                if values.get("workspace").and_then(toml::Value::as_bool) == Some(true) =>
            {
                let Some(path) = workspace
                    .and_then(|table| table.get("license-file"))
                    .and_then(toml::Value::as_str)
                else {
                    return Some(false);
                };
                (path, true)
            }
            _ => return Some(false),
        };
        let Some(path) =
            root_relative_license_file(manifest_path, path, inherited || workspace_package)
        else {
            return Some(false);
        };
        let preserved = metadata.preserved_licenses.as_ref();
        return Some(
            matches!(metadata.license, License::ApacheOnly | License::AgplOnly)
                && preserved.is_some_and(|preserved| {
                    preserved.assets.iter().any(|asset| {
                        asset.path == path && license_matches(root, &asset.path, &asset.sha256)
                    })
                }),
        );
    }
    license.map(|value| {
        if value.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
            workspace
                .and_then(|table| table.get("license"))
                .and_then(toml::Value::as_str)
                .is_some_and(|expression| metadata.license.matches_expression(expression))
        } else {
            value
                .as_str()
                .is_some_and(|expression| metadata.license.matches_expression(expression))
        }
    })
}

fn root_relative_license_file(
    manifest_path: &str,
    license_file: &str,
    workspace_relative: bool,
) -> Option<String> {
    let base = if workspace_relative {
        ""
    } else {
        manifest_path.rsplit_once('/').map_or("", |parent| parent.0)
    };
    let components = root_relative_path(base, license_file)?;
    match components.as_slice() {
        [name] => Some(name.clone()),
        _ => None,
    }
}

fn root_relative_path(base: &str, path: &str) -> Option<Vec<String>> {
    if path.is_empty() || path.contains(['\\', ':']) || path.starts_with('/') {
        return None;
    }
    let mut components: Vec<String> = base
        .split('/')
        .filter(|component| !component.is_empty())
        .map(str::to_owned)
        .collect();
    for component in path.split('/') {
        match component {
            "" => return None,
            "." => {}
            ".." => {
                components.pop()?;
            }
            value => components.push(value.to_owned()),
        }
    }
    Some(components)
}

fn inherited_workspace_package(
    root: &Path,
    manifest_path: &str,
    package: Option<&toml::Value>,
) -> Result<Option<toml::Value>> {
    let parent = manifest_path.rsplit_once('/').map_or("", |parent| parent.0);
    let explicit = package
        .and_then(|package| package.get("workspace"))
        .and_then(toml::Value::as_str);
    let mut candidates = Vec::new();
    if let Some(workspace) = explicit {
        let Some(directory) = root_relative_path(parent, workspace) else {
            return Ok(None);
        };
        candidates.push(if directory.is_empty() {
            "Cargo.toml".to_owned()
        } else {
            format!("{}/Cargo.toml", directory.join("/"))
        });
    } else {
        let mut directory = parent.to_owned();
        loop {
            candidates.push(if directory.is_empty() {
                "Cargo.toml".to_owned()
            } else {
                format!("{directory}/Cargo.toml")
            });
            if directory.is_empty() {
                break;
            }
            directory = directory
                .rsplit_once('/')
                .map_or_else(String::new, |(next, _)| next.to_owned());
        }
    }
    for candidate in candidates {
        let path = root.join(&candidate);
        if !path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&path).with_context(|| format!("read {candidate}"))?;
        let manifest: toml::Table =
            toml::from_str(&text).with_context(|| format!("parse {candidate}"))?;
        if let Some(workspace) = manifest.get("workspace") {
            if candidate != "Cargo.toml"
                && workspace
                    .get("package")
                    .and_then(|package| package.get("license-file"))
                    .is_some()
            {
                return Ok(None);
            }
            return Ok(workspace.get("package").cloned());
        }
    }
    Ok(None)
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
        .filter(|author| {
            Some(*author) != metadata.copyright.as_deref() && !author.contains("[bot]")
        })
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

fn github_repository(root: &Path, remote: &str) -> Result<String> {
    let url = platform::signed_git::git(root, &["remote", "get-url", "--", remote], None);
    let candidate = match &url {
        Ok(url) => url.trim().to_owned(),
        Err(_) => format!("{GITHUB_REPOSITORY_PREFIX}{remote}"),
    };
    platform::source_identity::github_slug(&candidate).ok_or_else(|| {
        Refusal::new(
            "repository.remote",
            "--remote requires a Git remote naming a GitHub repository or an explicit owner/name",
            "choose a configured GitHub remote or an explicit owner/name",
        )
        .evidence(format!("remote: {remote}"))
        .into()
    })
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
    for (path, text) in artifacts(root, &metadata)? {
        let current = fs::read_to_string(root.join(path))
            .unwrap_or_default()
            .replace("\r\n", "\n");
        if current != text {
            stale.push(path.to_owned());
        }
    }
    let off = manifests_off_license(root, &metadata)?;
    let missing = if metadata.license == License::MitOrApache {
        contributors(root, &metadata)?
    } else {
        Vec::new()
    };
    let described = match remote {
        Some(remote) => {
            description(&github_repository(root, remote)?)?
                == metadata.summary.trim_end_matches('.')
        }
        None => true,
    };
    let (rule, cause, next, evidence) = match drift(off.is_empty(), missing.is_empty(), stale.is_empty(), described) {
        Drift::Current => return Ok(()),
        Drift::License => (
            "repository.license",
            "package manifests do not match the existing license state".to_owned(),
            format!("correct the listed manifest license declarations, then run `{GENERATE}`"),
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
    use model::repository::{LICENSE, PreservedLicenseAsset, PreservedLicenses};
    use std::fmt::Write as _;

    const INDEX: &str = "docs/README.md";
    const README_PATH: &str = "README.md";
    const DEFAULT_README: &str = concat!(
        "# Fixture\n\nA fixture repository.\n\n## Installation\n\n",
        "```sh\ncargo install fixture\n```\n\n## Usage\n\n",
        "```sh\nfixture --help\n```\n\n## License\n\n",
        "Licensed under the [Apache License, Version 2.0](LICENSE-APACHE) or the ",
        "[Massachusetts Institute of Technology (MIT) license](LICENSE-MIT), at your option.\n\n",
        "Contributions use both licenses unless you state other terms.\n",
    );

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

    fn preserve_licenses(root: &Path) -> [(&'static str, Vec<u8>); 2] {
        let assets = [
            (
                "LICENSE-MIT",
                b"Reviewed MIT grant\r\nCopyright Owner\r\n".to_vec(),
            ),
            (
                "LICENSE-APACHE",
                b"Reviewed Apache grant\nOriginal notice\n".to_vec(),
            ),
        ];
        let mut config = fs::read_to_string(root.join(FILE)).unwrap();
        config.push_str("\n[preserved_licenses]\n");
        for ((path, bytes), key) in assets.iter().zip(["mit_sha256", "apache_sha256"]) {
            fs::write(root.join(path), bytes).unwrap();
            let digest = platform::build_inputs::hex(Sha256::digest(bytes).as_ref());
            writeln!(config, "{key} = \"{digest}\"").unwrap();
        }
        fs::write(root.join(FILE), config).unwrap();
        stage(root, "LICENSE-MIT");
        stage(root, "LICENSE-APACHE");
        assets
    }

    fn preserved_license_fixture(expression: &str) -> tempfile::TempDir {
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
        let license = b"Reviewed existing license grant\nCopyright remains unchanged\n";
        fs::write(root.join("LICENSE"), license).unwrap();
        let digest = platform::build_inputs::hex(Sha256::digest(license).as_ref());
        fs::write(
            root.join(FILE),
            format!(
                "name = \"fixture\"\nsummary = \"A fixture repository\"\ninstall = \"cargo install fixture\"\nusage = \"fixture --help\"\nlicense = \"{expression}\"\n\n[preserved_licenses]\n[[preserved_licenses.assets]]\npath = \"LICENSE\"\nsha256 = \"{digest}\"\n"
            ),
        )
        .unwrap();
        fs::write(
            root.join("Cargo.toml"),
            format!(
                "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nlicense = \"{expression}\"\n"
            ),
        )
        .unwrap();
        git(&["add", "."]);
        write(root).unwrap();
        directory
    }

    fn stage(root: &Path, path: &str) {
        let status = Tool::Git
            .command()
            .current_dir(root)
            .args(["add", path])
            .status()
            .unwrap();
        assert!(status.success(), "git add {path}");
    }

    fn unstage(root: &Path, path: &str) {
        let status = Tool::Git
            .command()
            .current_dir(root)
            .args(["rm", "--cached", "--quiet", "--", path])
            .status()
            .unwrap();
        assert!(status.success(), "git rm --cached {path}");
    }

    fn unlicensed_fixture() -> tempfile::TempDir {
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
            "name = \"fixture\"\nsummary = \"A fixture repository\"\ninstall = \"run\"\nusage = \"run --help\"\nlicense = \"unlicensed\"\n",
        )
        .unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        git(&["add", "."]);
        write(root).unwrap();
        directory
    }

    #[test]
    fn reviewed_license_bytes_survive_readme_generation() {
        let directory = fixture();
        let root = directory.path();
        let assets = preserve_licenses(root);
        fs::write(root.join(README_PATH), "An obsolete README\n").unwrap();
        write(root).unwrap();
        let readme = fs::read_to_string(root.join(README_PATH)).unwrap();
        assert!(readme.contains("[Apache License, Version 2.0](LICENSE-APACHE)"));
        assert!(
            readme.contains("[Massachusetts Institute of Technology (MIT) license](LICENSE-MIT)")
        );
        assert!(!readme.contains("Contributions"));
        assert!(!readme.lines().any(|line| line.starts_with("- ")));
        for (path, bytes) in assets {
            assert_eq!(fs::read(root.join(path)).unwrap(), bytes);
        }
        check(root, None).unwrap();
    }

    #[test]
    fn missing_or_changed_reviewed_licenses_refuse_before_any_generated_write() {
        for path in ["LICENSE-MIT", "LICENSE-APACHE"] {
            for missing in [false, true] {
                let directory = fixture();
                let root = directory.path();
                preserve_licenses(root);
                let marker = "Do not replace this README\n";
                fs::write(root.join(README_PATH), marker).unwrap();
                if missing {
                    fs::remove_file(root.join(path)).unwrap();
                } else {
                    fs::write(root.join(path), "Unreviewed replacement\n").unwrap();
                }
                let error = write(root).unwrap_err().to_string();
                assert!(error.contains(path), "{error}");
                assert_eq!(fs::read_to_string(root.join(README_PATH)).unwrap(), marker);
                assert!(check(root, None).is_err());
            }
        }
    }

    #[test]
    fn preservation_requires_both_digest_fields_and_keeps_the_manifest_gate() {
        let directory = fixture();
        let root = directory.path();
        preserve_licenses(root);
        let config = fs::read_to_string(root.join(FILE)).unwrap();
        for key in ["mit_sha256", "apache_sha256"] {
            let incomplete = config
                .lines()
                .filter(|line| !line.starts_with(key))
                .collect::<Vec<_>>()
                .join("\n");
            fs::write(root.join(FILE), incomplete).unwrap();
            assert!(metadata(root).is_err(), "{key}");
        }
        fs::write(root.join(FILE), config).unwrap();
        write(root).unwrap();
        let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            manifest.replace(LICENSE, "Apache-2.0 OR MIT"),
        )
        .unwrap();
        write(root).unwrap();
        check(root, None).unwrap();
        let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            manifest.replace("Apache-2.0 OR MIT", "MIT"),
        )
        .unwrap();
        assert_eq!(refused(root).rule, "repository.license");
    }

    #[test]
    fn existing_apache_and_agpl_assets_survive_readme_generation() {
        for expression in ["Apache-2.0", "AGPL-3.0-only"] {
            let directory = preserved_license_fixture(expression);
            let root = directory.path();
            let license = fs::read(root.join("LICENSE")).unwrap();
            let readme = fs::read_to_string(root.join(README_PATH)).unwrap();
            assert!(readme.contains("The existing license terms are in [LICENSE](LICENSE)."));
            assert!(!readme.contains("- [LICENSE](LICENSE)"));
            fs::write(root.join(README_PATH), "Old README\n").unwrap();
            write(root).unwrap();
            assert_eq!(fs::read(root.join("LICENSE")).unwrap(), license);
            assert!(!root.join("LICENSE-MIT").exists());
            assert!(!root.join("LICENSE-APACHE").exists());
            check(root, None).unwrap();
        }
    }

    #[test]
    fn untracked_preserved_assets_refuse_before_readme_write_and_staged_assets_pass() {
        for expression in ["Apache-2.0", "MIT OR Apache-2.0"] {
            let directory = if expression == "MIT OR Apache-2.0" {
                fixture()
            } else {
                preserved_license_fixture(expression)
            };
            let root = directory.path();
            let preserved = if expression == "MIT OR Apache-2.0" {
                preserve_licenses(root)
                    .into_iter()
                    .map(|(path, _)| path)
                    .collect::<Vec<_>>()
            } else {
                vec!["LICENSE"]
            };
            for path in &preserved {
                unstage(root, path);
            }
            let marker = "Keep this README unchanged\n";
            fs::write(root.join(README_PATH), marker).unwrap();

            let error = write(root).unwrap_err().to_string();

            for path in &preserved {
                assert!(error.contains(path), "{error}");
                stage(root, path);
            }
            assert_eq!(fs::read_to_string(root.join(README_PATH)).unwrap(), marker);
            write(root).unwrap();
            check(root, None).unwrap();
        }
    }

    #[test]
    fn changed_single_license_asset_refuses_before_rewriting_the_readme() {
        let directory = preserved_license_fixture("Apache-2.0");
        let root = directory.path();
        let marker = "Keep this README\n";
        fs::write(root.join(README_PATH), marker).unwrap();
        fs::write(root.join("LICENSE"), "Changed grant\n").unwrap();
        let error = write(root).unwrap_err().to_string();
        assert!(error.contains("LICENSE"), "{error}");
        assert_eq!(fs::read_to_string(root.join(README_PATH)).unwrap(), marker);
    }

    #[test]
    fn preserved_license_asset_cannot_hard_link_to_generated_readme() {
        let directory = preserved_license_fixture("Apache-2.0");
        let root = directory.path();
        let readme_path = root.join(README_PATH);
        let readme = fs::read(&readme_path).unwrap();
        let old_license = b"Reviewed existing license grant\nCopyright remains unchanged\n";
        let old_digest = platform::build_inputs::hex(Sha256::digest(old_license).as_ref());
        let readme_digest = platform::build_inputs::hex(Sha256::digest(&readme).as_ref());
        let metadata = fs::read_to_string(root.join(FILE))
            .unwrap()
            .replace(&old_digest, &readme_digest);
        fs::write(root.join(FILE), metadata).unwrap();
        fs::remove_file(root.join("LICENSE")).unwrap();
        fs::hard_link(&readme_path, root.join("LICENSE")).unwrap();

        let error = write(root).unwrap_err().to_string();

        assert!(error.contains("LICENSE"), "{error}");
        assert_eq!(fs::read(readme_path).unwrap(), readme);
    }

    #[test]
    fn preserved_license_asset_must_be_a_regular_file() {
        let directory = preserved_license_fixture("Apache-2.0");
        let root = directory.path();
        let readme_path = root.join(README_PATH);
        let readme = fs::read(&readme_path).unwrap();
        fs::remove_file(root.join("LICENSE")).unwrap();
        fs::create_dir(root.join("LICENSE")).unwrap();

        let error = write(root).unwrap_err().to_string();

        assert!(error.contains("LICENSE"), "{error}");
        assert_eq!(fs::read(readme_path).unwrap(), readme);
    }

    #[cfg(unix)]
    #[test]
    fn preserved_license_asset_cannot_symlink_to_the_generated_readme() {
        let directory = preserved_license_fixture("Apache-2.0");
        let root = directory.path();
        let readme_path = root.join(README_PATH);
        let readme = fs::read(&readme_path).unwrap();
        fs::remove_file(root.join("LICENSE")).unwrap();
        std::os::unix::fs::symlink(&readme_path, root.join("LICENSE")).unwrap();

        let error = write(root).unwrap_err().to_string();

        assert!(error.contains("LICENSE"), "{error}");
        assert_eq!(fs::read(readme_path).unwrap(), readme);
    }

    #[cfg(unix)]
    #[test]
    fn generated_readme_symlink_is_not_followed() {
        let directory = preserved_license_fixture("Apache-2.0");
        let root = directory.path();
        let license_path = root.join("LICENSE");
        let license = fs::read(&license_path).unwrap();
        fs::remove_file(root.join(README_PATH)).unwrap();
        std::os::unix::fs::symlink(&license_path, root.join(README_PATH)).unwrap();

        let error = write(root).unwrap_err().to_string();

        assert!(error.contains("README"), "{error}");
        assert_eq!(fs::read(license_path).unwrap(), license);
    }

    #[test]
    fn unlicensed_generation_writes_no_license_files_or_readme_claim() {
        let directory = unlicensed_fixture();
        let root = directory.path();
        let readme = fs::read_to_string(root.join(README_PATH)).unwrap();
        assert!(!readme.contains("## License"));
        assert!(!root.join("LICENSE-MIT").exists());
        assert!(!root.join("LICENSE-APACHE").exists());
        assert!(!root.join("LICENSE").exists());
        check(root, None).unwrap();
    }

    #[test]
    fn unlicensed_state_refuses_declared_manifest_terms_before_writing() {
        let directory = unlicensed_fixture();
        let root = directory.path();
        let marker = "Keep this README\n";
        fs::write(root.join(README_PATH), marker).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nlicense = \"MIT\"\n",
        )
        .unwrap();
        let error = write(root).unwrap_err().to_string();
        assert!(error.contains("Cargo.toml"), "{error}");
        assert_eq!(fs::read_to_string(root.join(README_PATH)).unwrap(), marker);
    }

    #[test]
    fn npm_unlicensed_sentinel_matches_only_the_unlicensed_state() {
        let directory = unlicensed_fixture();
        let root = directory.path();
        let package_json = root.join("package.json");
        fs::write(
            &package_json,
            "{\"name\":\"fixture\",\"license\":\"UNLICENSED\"}\n",
        )
        .unwrap();
        stage(root, "package.json");
        write(root).unwrap();

        let marker = "Keep the README unchanged\n";
        fs::write(root.join(README_PATH), marker).unwrap();
        fs::write(
            &package_json,
            "{\"name\":\"fixture\",\"license\":\"MIT\"}\n",
        )
        .unwrap();
        stage(root, "package.json");

        let error = write(root).unwrap_err().to_string();

        assert!(error.contains("package.json"), "{error}");
        assert_eq!(fs::read_to_string(root.join(README_PATH)).unwrap(), marker);
    }

    #[test]
    fn cargo_license_file_is_preserved_only_when_its_asset_digest_is_reviewed() {
        let directory = preserved_license_fixture("Apache-2.0");
        let root = directory.path();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nlicense-file = \"LICENSE\"\n",
        )
        .unwrap();
        write(root).unwrap();
        check(root, None).unwrap();

        let marker = "Keep the README unchanged\n";
        fs::write(root.join(README_PATH), marker).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nlicense-file = \"COPYING\"\n",
        )
        .unwrap();

        let error = write(root).unwrap_err().to_string();

        assert!(error.contains("Cargo.toml"), "{error}");
        assert_eq!(fs::read_to_string(root.join(README_PATH)).unwrap(), marker);
    }

    #[test]
    fn cargo_workspace_license_file_inheritance_uses_the_workspace_root_asset() {
        let directory = preserved_license_fixture("Apache-2.0");
        let root = directory.path();
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/fixture\"]\n\n[workspace.package]\nlicense-file = \"LICENSE\"\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("crates/fixture")).unwrap();
        fs::write(
            root.join("crates/fixture/Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nlicense-file.workspace = true\n",
        )
        .unwrap();
        stage(root, "crates/fixture/Cargo.toml");

        write(root).unwrap();
        check(root, None).unwrap();
    }

    #[test]
    fn virtual_workspaces_accept_members_with_direct_license_declarations() {
        for (workspace_path, member_path, root_manifest, workspace_manifest) in [
            (
                "Cargo.toml",
                "crates/fixture/Cargo.toml",
                "[workspace]\nmembers = [\"crates/fixture\"]\n",
                None,
            ),
            (
                "sub/Cargo.toml",
                "sub/crates/fixture/Cargo.toml",
                "[workspace]\nmembers = []\n",
                Some("[workspace]\nmembers = [\"crates/fixture\"]\n"),
            ),
        ] {
            let directory = preserved_license_fixture("Apache-2.0");
            let root = directory.path();
            fs::write(root.join("Cargo.toml"), root_manifest).unwrap();
            stage(root, "Cargo.toml");
            if let Some(workspace_manifest) = workspace_manifest {
                fs::create_dir_all(root.join("sub/crates/fixture")).unwrap();
                fs::write(root.join(workspace_path), workspace_manifest).unwrap();
                stage(root, workspace_path);
            } else {
                fs::create_dir_all(root.join("crates/fixture")).unwrap();
            }
            fs::write(
                root.join(member_path),
                "[package]\nname = \"fixture-member\"\nversion = \"0.1.0\"\nlicense = \"Apache-2.0\"\n",
            )
            .unwrap();
            stage(root, member_path);

            write(root).unwrap();
            check(root, None).unwrap();
        }
    }

    #[test]
    fn workspace_license_is_not_inherited_without_explicit_member_declaration() {
        let directory = preserved_license_fixture("Apache-2.0");
        let root = directory.path();
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/fixture\"]\n\n[workspace.package]\nlicense = \"Apache-2.0\"\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("crates/fixture")).unwrap();
        fs::write(
            root.join("crates/fixture/Cargo.toml"),
            "[package]\nname = \"fixture-member\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        stage(root, "Cargo.toml");
        stage(root, "crates/fixture/Cargo.toml");
        let marker = "Keep the README unchanged\n";
        fs::write(root.join(README_PATH), marker).unwrap();

        let error = write(root).unwrap_err().to_string();

        assert!(error.contains("crates/fixture/Cargo.toml"), "{error}");
        assert_eq!(fs::read_to_string(root.join(README_PATH)).unwrap(), marker);
    }

    #[test]
    fn nested_workspace_license_file_cannot_be_mistaken_for_the_repository_license() {
        let directory = preserved_license_fixture("Apache-2.0");
        let root = directory.path();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nlicense-file = \"LICENSE\"\n",
        )
        .unwrap();
        fs::write(
            root.join("LICENSE"),
            b"Reviewed existing license grant\nCopyright remains unchanged\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("sub/crates/fixture")).unwrap();
        fs::write(
            root.join("sub/Cargo.toml"),
            "[workspace]\nmembers = [\"crates/fixture\"]\n\n[workspace.package]\nlicense-file = \"LICENSE\"\n",
        )
        .unwrap();
        fs::write(
            root.join("sub/crates/fixture/Cargo.toml"),
            "[package]\nname = \"nested-fixture\"\nversion = \"0.1.0\"\nlicense-file.workspace = true\n",
        )
        .unwrap();
        fs::write(root.join("sub/LICENSE"), "Different nested grant\n").unwrap();
        stage(root, "sub/Cargo.toml");
        stage(root, "sub/crates/fixture/Cargo.toml");
        let marker = "Keep the README unchanged\n";
        fs::write(root.join(README_PATH), marker).unwrap();

        let error = write(root).unwrap_err().to_string();

        assert!(error.contains("sub/Cargo.toml"), "{error}");
        assert!(error.contains("sub/crates/fixture/Cargo.toml"), "{error}");
        assert_eq!(fs::read_to_string(root.join(README_PATH)).unwrap(), marker);
    }

    #[test]
    fn nested_virtual_workspace_license_file_is_refused_before_writing() {
        let directory = preserved_license_fixture("Apache-2.0");
        let root = directory.path();
        let license = b"Reviewed existing license grant\nCopyright remains unchanged\n";
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nlicense-file = \"LICENSE\"\n",
        )
        .unwrap();
        fs::write(root.join("LICENSE"), license).unwrap();
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(
            root.join("sub/Cargo.toml"),
            "[workspace]\n\n[workspace.package]\nlicense-file = \"LICENSE\"\n",
        )
        .unwrap();
        fs::write(root.join("sub/LICENSE"), "Different nested grant\n").unwrap();
        stage(root, "sub/Cargo.toml");
        let marker = "Keep the README unchanged\n";
        fs::write(root.join(README_PATH), marker).unwrap();

        let error = write(root).unwrap_err().to_string();

        assert!(error.contains("sub/Cargo.toml"), "{error}");
        assert_eq!(fs::read_to_string(root.join(README_PATH)).unwrap(), marker);
        assert_eq!(fs::read(root.join("LICENSE")).unwrap(), license);
    }

    #[test]
    fn unlicensed_state_refuses_a_cargo_license_file() {
        let directory = unlicensed_fixture();
        let root = directory.path();
        fs::write(root.join("LICENSE"), "Existing license text\n").unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nlicense-file = \"LICENSE\"\n",
        )
        .unwrap();
        let marker = "Keep the README unchanged\n";
        fs::write(root.join(README_PATH), marker).unwrap();

        let error = write(root).unwrap_err().to_string();

        assert!(error.contains("Cargo.toml"), "{error}");
        assert_eq!(fs::read_to_string(root.join(README_PATH)).unwrap(), marker);
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

    #[test]
    fn the_documentation_index_is_optional_and_preserves_existing_output() {
        let directory = fixture();
        let root = directory.path();
        let config = fs::read_to_string(root.join(FILE)).unwrap();
        let original = generate(&metadata(root).unwrap());
        assert_eq!(
            fs::read_to_string(root.join(README_PATH)).unwrap(),
            DEFAULT_README
        );
        fs::create_dir(root.join("docs")).unwrap();
        fs::write(root.join(INDEX), "# Documentation\n").unwrap();
        for enabled in [true, false] {
            fs::write(
                root.join(FILE),
                format!("{config}documentation_index = {enabled}\n"),
            )
            .unwrap();
            write(root).unwrap();
            check(root, None).unwrap();
            let readme = fs::read_to_string(root.join(README_PATH)).unwrap();
            assert_eq!(readme.matches(INDEX).count(), usize::from(enabled));
            if enabled {
                assert!(readme.contains(
                    "## Documentation\n\n[Read the documentation](docs/README.md).\n\n## License\n"
                ));
            } else {
                assert_eq!(readme, DEFAULT_README);
            }
            for (path, text) in &original {
                if *path != README_PATH {
                    assert_eq!(fs::read_to_string(root.join(path)).unwrap(), *text);
                }
            }
        }
    }

    #[test]
    fn an_invalid_requested_index_leaves_generated_files_unchanged() {
        let directory = fixture();
        let root = directory.path();
        let config = fs::read_to_string(root.join(FILE)).unwrap();
        let original = generate(&metadata(root).unwrap());
        fs::write(
            root.join(FILE),
            format!("{config}documentation_index = true\n"),
        )
        .unwrap();
        fs::create_dir(root.join("docs")).unwrap();
        for directory_at_index in [false, true] {
            if directory_at_index {
                fs::create_dir(root.join(INDEX)).unwrap();
            }
            let error = write(root).unwrap_err().to_string();
            assert!(error.contains(INDEX), "{error}");
            for (path, text) in &original {
                assert_eq!(fs::read_to_string(root.join(path)).unwrap(), *text);
            }
        }
    }

    #[test]
    fn a_non_boolean_documentation_setting_refuses_generation() {
        let directory = fixture();
        let root = directory.path();
        let config = fs::read_to_string(root.join(FILE)).unwrap();
        for value in ["\"true\"", "1", "[]"] {
            fs::write(
                root.join(FILE),
                format!("{config}documentation_index = {value}\n"),
            )
            .unwrap();
            assert!(metadata(root).is_err(), "{value}");
        }
    }

    #[test]
    fn a_documentation_placeholder_inside_project_text_stays_literal() {
        let directory = fixture();
        let root = directory.path();
        let config = fs::read_to_string(root.join(FILE))
            .unwrap()
            .replace(
                "A fixture repository",
                "A fixture {documentation} repository",
            )
            .replace("fixture --help", "fixture {documentation}");
        fs::create_dir(root.join("docs")).unwrap();
        fs::write(root.join(INDEX), "# Documentation\n").unwrap();
        for enabled in [true, false] {
            fs::write(
                root.join(FILE),
                format!("{config}documentation_index = {enabled}\n"),
            )
            .unwrap();
            write(root).unwrap();
            let readme = fs::read_to_string(root.join(README_PATH)).unwrap();
            assert!(readme.contains("A fixture {documentation} repository.\n"));
            assert!(readme.contains("```sh\nfixture {documentation}\n```"));
        }
    }

    #[test]
    fn generated_readme_has_one_final_line_feed_for_every_license_and_index_state() {
        let dual_preserved = PreservedLicenses {
            mit_sha256: Some("a".repeat(64)),
            apache_sha256: Some("b".repeat(64)),
            assets: Vec::new(),
        };
        let single_preserved = |path: &'static str| PreservedLicenses {
            mit_sha256: None,
            apache_sha256: None,
            assets: vec![PreservedLicenseAsset {
                path: path.to_owned(),
                sha256: "c".repeat(64),
            }],
        };
        let cases = [
            (License::MitOrApache, None),
            (License::MitOrApache, Some(dual_preserved)),
            (
                License::ApacheOnly,
                Some(single_preserved("LICENSE-APACHE")),
            ),
            (License::AgplOnly, Some(single_preserved("LICENSE"))),
            (License::Unlicensed, None),
        ];

        for (license, preserved_licenses) in cases {
            for documentation_index in [false, true] {
                let metadata = Repository {
                    name: "fixture".to_owned(),
                    summary: "A fixture repository".to_owned(),
                    install: "cargo install fixture".to_owned(),
                    usage: "fixture --help".to_owned(),
                    usage_language: "sh".to_owned(),
                    documentation_index,
                    preserved_licenses: preserved_licenses.clone(),
                    copyright: Some("Owner".to_owned()),
                    license,
                    consent: Vec::new(),
                };
                let generated = generate(&metadata);
                let readme = &generated[0].1;
                let trailing_line_feeds = readme
                    .as_bytes()
                    .iter()
                    .rev()
                    .take_while(|byte| **byte == b'\n')
                    .count();

                assert_eq!(
                    trailing_line_feeds, 1,
                    "license={license:?}, documentation_index={documentation_index}"
                );
                assert_eq!(generate(&metadata), generated);
            }
        }
    }
}

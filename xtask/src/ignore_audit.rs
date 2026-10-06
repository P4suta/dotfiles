//! Every `.chezmoiignore` pattern must ignore something the source declares or appear in the shared host state listing.
//! No `.chezmoiremove` pattern may remove a managed target, and neither listing may repeat a pattern.

use crate::ignore_rules::{Verdict, matches, verdict};
use crate::profile_rules::Profile;
use crate::profiles::{chezmoi, chezmoi_path, fixture, managed_paths};
use anyhow::{Context, Result, ensure};
use serde_json::json;
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Build outputs that exist in a working checkout but never in Git, which host-state-ignore keeps out of a home directory.
const BUILD_OUTPUTS: [&str; 2] = ["node_modules", "target"];

/// The shared listing of paths that applications write into a home directory, which no source entry declares.
const HOST_STATE: &str = "{{ includeTemplate \"host-state-ignore\" . }}";

fn source_paths(root: &Path) -> Result<Vec<String>> {
    let mut paths = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)?
                .to_str()
                .context("non-UTF-8 source path")?
                .replace('\\', "/");
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if relative.starts_with(".chezmoi") || name == ".git" {
                continue;
            }
            if BUILD_OUTPUTS.contains(&name.as_ref()) {
                continue;
            }
            if entry.file_type()?.is_dir() {
                stack.push(path);
            }
            paths.push(relative);
        }
    }
    paths.sort();
    Ok(paths)
}

/// Every target name the source declares for any profile, with each of its parent directories.
pub fn source_targets(root: &Path, scope: &Path) -> Result<BTreeSet<String>> {
    let (config, destination) = configure(root, scope, Profile::Mac)?;
    let prefix = format!("{}/", chezmoi_path(&destination));
    let mut targets = BTreeSet::new();
    for batch in source_paths(root)?.chunks(200) {
        let output = chezmoi(root, scope, &config, &destination)
            .current_dir(root)
            .args(["target-path", "--source-path"])
            .args(batch)
            .output()?;
        ensure!(
            output.status.success(),
            "chezmoi target-path failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        for line in String::from_utf8(output.stdout)?.lines() {
            let target = line.replace('\\', "/");
            let target = target.strip_prefix(&prefix).unwrap_or(&target);
            let mut parent = String::new();
            for segment in target.split('/') {
                if !parent.is_empty() {
                    parent.push('/');
                }
                parent.push_str(segment);
                targets.insert(parent.clone());
            }
        }
    }
    Ok(targets)
}

fn configure(
    root: &Path,
    scope: &Path,
    profile: Profile,
) -> Result<(std::path::PathBuf, std::path::PathBuf)> {
    let destination = scope.join(profile.name()).join("home");
    fs::create_dir_all(&destination)?;
    let config = scope.join(profile.name()).join("config.json");
    fs::write(
        &config,
        serde_json::to_vec(&json!({"data": fixture(profile, root, &destination)}))?,
    )?;
    Ok((config, destination))
}

fn render(mut command: Command, template: &[u8]) -> Result<String> {
    let mut child = command
        .arg("execute-template")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .context("chezmoi input is unavailable")?
        .write_all(template)?;
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "chezmoi execute-template failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?)
}

/// The patterns of a rendered listing, which skip blank lines and comments.
pub fn patterns(rendered: &str) -> Vec<String> {
    rendered
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// The findings for one listing: repeated patterns, and patterns that `keep` refuses.
pub fn audit(listing: &[String], keep: impl Fn(&str) -> bool) -> Vec<(String, Verdict)> {
    let mut findings = Vec::new();
    for (index, pattern) in listing.iter().enumerate() {
        let earlier = listing[..index]
            .iter()
            .filter(|seen| *seen == pattern)
            .count();
        let result = verdict(keep(pattern), earlier);
        if result != Verdict::Kept {
            findings.push((pattern.clone(), result));
        }
    }
    findings
}

/// Refuses an ignore pattern that matches no source target, a remove pattern that matches a managed target, and any repeated pattern.
pub fn check(root: &Path) -> Result<()> {
    let scope = tempfile::tempdir()?;
    let targets = source_targets(root, scope.path())?;
    let mut findings = Vec::new();
    for profile in Profile::ALL {
        let (config, destination) = configure(root, scope.path(), profile)?;
        let command = || chezmoi(root, scope.path(), &config, &destination);
        let ignore = patterns(&render(command(), &fs::read(root.join(".chezmoiignore"))?)?);
        let host_state = patterns(&render(command(), HOST_STATE.as_bytes())?);
        for (pattern, result) in audit(&ignore, |pattern| {
            let negated = pattern.strip_prefix('!').unwrap_or(pattern);
            host_state.iter().any(|state| state == pattern)
                || targets
                    .iter()
                    .any(|target| matches(negated.as_bytes(), target.as_bytes()))
        }) {
            findings.push(format!(
                "{} .chezmoiignore {pattern}: {result:?}",
                profile.name()
            ));
        }
        let listed = command()
            .args(["managed", "--include", "all", "--nul-path-separator"])
            .output()?;
        ensure!(
            listed.status.success(),
            "chezmoi managed failed: {}",
            String::from_utf8_lossy(&listed.stderr)
        );
        let managed = managed_paths(&listed.stdout)?;
        let remove = patterns(&render(command(), &fs::read(root.join(".chezmoiremove"))?)?);
        for (pattern, result) in audit(&remove, |pattern| {
            !managed
                .iter()
                .any(|target| matches(pattern.as_bytes(), target.as_bytes()))
        }) {
            findings.push(format!(
                "{} .chezmoiremove {pattern}: {result:?}",
                profile.name()
            ));
        }
    }
    ensure!(
        findings.is_empty(),
        "delete each repeated pattern, each ignore pattern that matches no source target, and each remove pattern that matches a managed target:\n{}",
        findings.join("\n")
    );
    println!("Every ignore and remove pattern is distinct and earns its place");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_and_blank_lines_are_not_patterns() {
        assert_eq!(
            patterns("# note\n\n  .config/bat  \n*.md\n"),
            [".config/bat", "*.md"]
        );
    }

    #[test]
    fn a_repeat_and_an_unmatched_pattern_are_both_findings() {
        let listing: Vec<String> = ["a", "b", "a", "c"].map(str::to_owned).into();
        assert_eq!(
            audit(&listing, |pattern| pattern != "c"),
            [
                ("a".to_owned(), Verdict::Repeated),
                ("c".to_owned(), Verdict::Unjustified)
            ]
        );
    }
}

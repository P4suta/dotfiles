//! The structural gates: the decision core keeps its boundary, and the code keeps the structural catalog every repository on these machines shares.

use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use rules::boundary_rules::{Core, DENIED, breach};
use std::fs;
use std::path::Path;

/// The shared structural catalog, which every host receives at `~/.config/assure`.
pub const CATALOG: &str = "home/dot_config/assure";

/// The crate that holds every pure decision of the tools.
pub const CORE: &str = "crates/rules";

/// The lints a `#![deny(...)]` attribute in `root` names, as bits of `DENIED`.
#[must_use]
pub fn denied(root: &str) -> u16 {
    let mut bits = 0;
    for attribute in root.split("#![deny(").skip(1) {
        let list = attribute.split(")]").next().unwrap_or_default();
        for lint in list.split(',').map(str::trim) {
            if let Some(index) = DENIED.iter().position(|name| *name == lint) {
                bits |= 1 << index;
            }
        }
    }
    bits
}

/// How many dependencies a manifest declares in every dependency table, including the per-target ones.
///
/// # Errors
///
/// Returns an error when the manifest fails to parse as TOML.
pub fn dependencies(manifest: &str) -> Result<usize> {
    let manifest: toml::Table = toml::from_str(manifest)?;
    let count = |table: &toml::Table| {
        ["dependencies", "dev-dependencies", "build-dependencies"]
            .iter()
            .filter_map(|kind| table.get(*kind).and_then(toml::Value::as_table))
            .map(toml::Table::len)
            .sum::<usize>()
    };
    let targets = manifest
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(toml::Table::values)
        .filter_map(toml::Value::as_table)
        .map(count)
        .sum::<usize>();
    Ok(count(&manifest) + targets)
}

fn rule_modules(directory: &Path, found: &mut Vec<String>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            rule_modules(&path, found)?;
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("_rules.rs"))
        {
            found.push(path.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

/// The rule modules of the workspace that live outside the core.
///
/// # Errors
///
/// Returns an error when listing a source directory fails.
pub fn strays(root: &Path) -> Result<Vec<String>> {
    let mut found = Vec::new();
    rule_modules(&root.join("xtask/src"), &mut found)?;
    for entry in fs::read_dir(root.join("crates"))? {
        let source = entry?.path().join("src");
        if source.is_dir() && !source.starts_with(root.join(CORE)) {
            rule_modules(&source, &mut found)?;
        }
    }
    Ok(found)
}

/// The facts the boundary rule decides on.
///
/// # Errors
///
/// Returns an error when reading or parsing the core's manifest or root fails, or listing a source directory fails.
pub fn core(root: &Path) -> Result<Core> {
    let core = root.join(CORE);
    let library = fs::read_to_string(core.join("src/lib.rs"))?;
    Ok(Core {
        dependencies: dependencies(&fs::read_to_string(core.join("Cargo.toml"))?)?,
        no_std: library.lines().any(|line| line.trim() == "#![no_std]"),
        denied: denied(&library),
        strays: strays(root)?.len(),
    })
}

/// Refuses a core that breaks its boundary, a catalog rule that no longer rejects its violating fixtures, and code that the catalog refuses.
///
/// # Errors
///
/// Returns an error naming the breach and its next action, or when ast-grep can't start or refuses.
pub fn check(root: &Path) -> Result<()> {
    let facts = core(root)?;
    if let Some(found) = breach(facts) {
        let detail = match found {
            rules::boundary_rules::Breach::Undenied(index) => format!(
                "the core no longer denies {}",
                DENIED.get(index).copied().unwrap_or_default()
            ),
            rules::boundary_rules::Breach::Stray => {
                format!(
                    "rule modules outside the core: {}",
                    strays(root)?.join(", ")
                )
            }
            other => format!("{other:?}"),
        };
        anyhow::bail!(
            "{CORE} breaks the pure core boundary: {detail}\nnext: {}",
            found.next_action()
        );
    }
    let catalog = root.join(CATALOG);
    let status = Tool::AstGrep
        .command()
        .current_dir(&catalog)
        .args(["test", "--skip-snapshot-tests"])
        .status()
        .context("start the pinned ast-grep")?;
    ensure!(
        status.success(),
        "a rule in {CATALOG} no longer refuses its violating fixtures or admits its corrected ones; fix the rule or its test, then run `just check`"
    );
    let status = Tool::AstGrep
        .command()
        .current_dir(root)
        .arg("scan")
        .arg("--config")
        .arg(catalog.join("sgconfig.yml"))
        .args(["xtask", "crates"])
        .status()
        .context("start the pinned ast-grep")?;
    ensure!(
        status.success(),
        "the structural catalog in {CATALOG} refused the code above; apply each finding's note, then run `just check`"
    );
    println!("The decision core keeps its boundary and the code keeps the structural catalog");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_deny_list_is_read_lint_by_lint() {
        let every = (1u16 << DENIED.len()) - 1;
        let root = format!("#![no_std]\n#![deny(\n    {}\n)]\n", DENIED.join(",\n    "));
        assert_eq!(denied(&root), every);
        let fewer = root.replace("clippy::indexing_slicing,", "");
        assert_eq!(denied(&fewer), every & !(1 << 6));
        assert_eq!(denied("#![allow(clippy::panic)]"), 0);
    }

    #[test]
    fn every_dependency_table_counts() -> Result<()> {
        assert_eq!(dependencies("[package]\nname = \"rules\"\n")?, 0);
        assert_eq!(
            dependencies(
                "[dependencies]\nserde = \"1\"\n[dev-dependencies]\nx = \"1\"\n[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n"
            )?,
            3
        );
        Ok(())
    }
}

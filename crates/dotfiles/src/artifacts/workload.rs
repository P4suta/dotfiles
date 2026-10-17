use super::plan::AdditionalCargoLock;
use super::rustup_consume::AuthenticatedStdlibLock;
use super::{Plan, PreparedRustKani};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const MANIFEST: &str = "Cargo.toml";
const LOCK: &str = "Cargo.lock";
const CARGO_CONFIG: &str = "config.toml";
const LEGACY_CARGO_CONFIG: &str = "config";
const CARGO_DIRECTORY: &str = ".cargo";
const INPUT_BYTE_LIMIT: usize = 8 * 1024 * 1024;
const INPUT_COUNT_LIMIT: usize = 4096;
const DIRECTORY_COUNT_LIMIT: usize = 65_536;
const DIRECTORY_DEPTH_LIMIT: usize = 128;
const TOTAL_INPUT_BYTE_LIMIT: usize = 32 * 1024 * 1024;
const GENERATED_DIRECTORIES: [&str; 4] = [".git", "target", "node_modules", ".venv"];

/// The manifests and locks of every Cargo consumer in one captured source tree.
#[derive(Debug)]
pub struct CargoWorkload {
    root: PathBuf,
    inputs: BTreeMap<String, String>,
    stdlib: Option<AuthenticatedStdlibLock>,
}

impl CargoWorkload {
    /// Capture independent workspaces such as fuzz and verification fixtures.
    ///
    /// # Errors
    ///
    /// Refuses links, missing locks, external path dependencies, and source overrides.
    pub fn capture(root: &Path) -> Result<Self> {
        let metadata = fs::symlink_metadata(root).context("inspect the Cargo source root")?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "Cargo source root must be a real directory"
        );
        let root = platform::paths::canonical(root)?;
        let mut inputs = BTreeMap::new();
        let mut directories = 0;
        scan(&root, &root, &mut inputs, &mut directories)?;
        ensure!(
            inputs.contains_key(MANIFEST) && inputs.contains_key(LOCK),
            "Cargo source requires its root manifest and lock"
        );
        for (path, text) in &inputs {
            if path.rsplit('/').next() == Some(MANIFEST) {
                validate_manifest(&root, path, text, &inputs)?;
            }
        }
        Ok(Self {
            root,
            inputs,
            stdlib: None,
        })
    }

    /// Include the original lock captured from this Source's verified Rust component.
    ///
    /// # Errors
    /// Rejects unrelated, changed, missing, or conflicting inputs.
    pub fn with_stdlib(mut self, prepared: &PreparedRustKani) -> Result<Self> {
        ensure!(
            self.stdlib.is_none(),
            "stdlib Cargo materials are already bound"
        );
        self.require_current(&self.root)?;
        self.stdlib = prepared.stdlib_input(&self.root)?;
        self.plan()?.complete()?;
        Ok(self)
    }

    /// Derive one checksum-bound package union without consulting a registry index.
    ///
    /// # Errors
    ///
    /// Refuses malformed locks and conflicting package identities.
    pub fn plan(&self) -> Result<Plan> {
        match &self.stdlib {
            None => Plan::cargo_closure(&self.inputs),
            Some(lock) => {
                lock.require_current()?;
                Plan::cargo_closure_with(
                    &self.inputs,
                    &[AdditionalCargoLock {
                        name: lock.name(),
                        text: lock.text(),
                        origin: lock.origin(),
                    }],
                )
            }
        }
    }

    /// List independent workspaces from the captured manifests without requiring Git metadata.
    ///
    /// # Errors
    ///
    /// Refuses malformed manifests or a source with no independent Cargo workspace.
    pub fn workspace_manifests(&self) -> Result<Vec<String>> {
        let mut manifests = Vec::new();
        for (path, text) in &self.inputs {
            if path.rsplit('/').next() == Some(MANIFEST) {
                let manifest: toml::Table = toml::from_str(text)?;
                if manifest.contains_key("workspace") {
                    manifests.push(path.clone());
                }
            }
        }
        ensure!(
            !manifests.is_empty(),
            "captured source has no independent Cargo workspace"
        );
        Ok(manifests)
    }

    /// Check that no manifest, lock, or Cargo source configuration has changed.
    ///
    /// # Errors
    ///
    /// Refuses changed inputs or a different source root.
    pub fn require_current(&self, root: &Path) -> Result<()> {
        let current = Self::capture(root)?;
        ensure!(
            current.root == self.root && current.inputs == self.inputs,
            "Cargo workload inputs changed after acquisition planning"
        );
        if let Some(lock) = &self.stdlib {
            lock.require_current()?;
        }
        Ok(())
    }

    /// Require an environment-only consumer without unknown ancestor Cargo configuration.
    ///
    /// # Errors
    ///
    /// Refuses an ancestor configuration without reading its private contents.
    pub fn require_environment_scope(&self) -> Result<()> {
        for parent in self.root.ancestors().skip(1) {
            for name in [CARGO_CONFIG, LEGACY_CARGO_CONFIG] {
                let file = parent.join(CARGO_DIRECTORY).join(name);
                match fs::symlink_metadata(file) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        return Err(error).context("inspect ancestor Cargo configuration");
                    }
                    Ok(_) => anyhow::bail!(
                        "environment-only Cargo consumption has an unknown ancestor configuration"
                    ),
                }
            }
        }
        Ok(())
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }
}

fn scan(
    root: &Path,
    directory: &Path,
    inputs: &mut BTreeMap<String, String>,
    directories: &mut usize,
) -> Result<()> {
    ensure!(
        directory.strip_prefix(root)?.components().count() <= DIRECTORY_DEPTH_LIMIT,
        "Cargo source directory depth exceeds its bound"
    );
    *directories += 1;
    ensure!(
        *directories <= DIRECTORY_COUNT_LIMIT,
        "Cargo source directory inventory exceeds its bound"
    );
    for item in fs::read_dir(directory)? {
        let item = item?;
        let name = item.file_name();
        if GENERATED_DIRECTORIES
            .iter()
            .any(|excluded| name == *excluded)
        {
            continue;
        }
        let metadata = fs::symlink_metadata(item.path())?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "Cargo source inventory contains a symlink"
        );
        if metadata.is_dir() {
            scan(root, &item.path(), inputs, directories)?;
        } else if name == MANIFEST
            || name == LOCK
            || ((name == CARGO_CONFIG || name == LEGACY_CARGO_CONFIG)
                && directory
                    .file_name()
                    .is_some_and(|name| name == CARGO_DIRECTORY))
        {
            ensure!(
                metadata.is_file() && metadata.len() <= INPUT_BYTE_LIMIT as u64,
                "Cargo source input is not a bounded regular file"
            );
            ensure!(
                inputs.len() < INPUT_COUNT_LIMIT,
                "Cargo source input inventory exceeds its bound"
            );
            let relative = item
                .path()
                .strip_prefix(root)?
                .to_str()
                .context("Cargo input path requires Unicode")?
                .replace('\\', "/");
            let text = fs::read_to_string(item.path())?;
            ensure!(
                text.len() <= INPUT_BYTE_LIMIT,
                "Cargo source input grew beyond its bound"
            );
            let total = inputs
                .values()
                .try_fold(text.len(), |sum, input| sum.checked_add(input.len()))
                .context("Cargo source input byte count overflow")?;
            ensure!(
                total <= TOTAL_INPUT_BYTE_LIMIT,
                "Cargo source input set exceeds its byte bound"
            );
            if name == CARGO_CONFIG || name == LEGACY_CARGO_CONFIG {
                let config: toml::Table = toml::from_str(&text)?;
                ensure!(
                    !config.contains_key("source"),
                    "Cargo source replacement must come from the verified workload"
                );
            }
            inputs.insert(relative, text);
        }
    }
    Ok(())
}

fn validate_manifest(
    root: &Path,
    path: &str,
    text: &str,
    inputs: &BTreeMap<String, String>,
) -> Result<()> {
    let manifest: toml::Table = toml::from_str(text).context("parse a Cargo consumer manifest")?;
    let directory = Path::new(path).parent().context("Cargo manifest parent")?;
    if manifest.contains_key("workspace") {
        let lock = directory.join(LOCK).to_string_lossy().replace('\\', "/");
        ensure!(
            inputs.contains_key(&lock),
            "{path}: independent workspace has no pinned Cargo lock"
        );
    }
    validate_paths(root, &root.join(directory), &toml::Value::Table(manifest))
}

fn validate_paths(root: &Path, directory: &Path, value: &toml::Value) -> Result<()> {
    match value {
        toml::Value::Table(table) => {
            if let Some(path) = table.get("path").and_then(toml::Value::as_str) {
                let dependency = platform::paths::canonical(&directory.join(path))
                    .context("resolve a Cargo path dependency")?;
                ensure!(
                    dependency.starts_with(root),
                    "Cargo path dependency leaves the captured source"
                );
            }
            for nested in table.values() {
                validate_paths(root, directory, nested)?;
            }
        }
        toml::Value::Array(values) => {
            for nested in values {
                validate_paths(root, directory, nested)?;
            }
        }
        _ => {}
    }
    Ok(())
}

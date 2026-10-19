//! Committed declarations bind Cargo workspace metadata to report records.

use super::report::{Dependency, Record};
use super::{Found, checkout_snapshot};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

const MANIFEST: &str = "Cargo.toml";
const LOCKFILE: &str = "Cargo.lock";
const DEFAULT_REGISTRY: &str = "https://github.com/rust-lang/crates.io-index";
const SPARSE_REGISTRY: &str = "sparse+https://index.crates.io/";
const INHERITED: &str = "inherited-dependency";

#[derive(Default)]
pub(super) struct Bindings {
    pub(super) inherited: BTreeSet<usize>,
    pub(super) touched: BTreeSet<usize>,
}

#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize)]
#[serde(untagged)]
enum Declaration {
    Version(String),
    Detailed(Details),
}

#[derive(Debug, Clone, Default, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Details {
    workspace: Option<bool>,
    version: Option<String>,
    package: Option<String>,
    registry: Option<String>,
    path: Option<String>,
    git: Option<String>,
    branch: Option<String>,
    tag: Option<String>,
    rev: Option<String>,
    optional: Option<bool>,
    default_features: Option<bool>,
    #[serde(default)]
    features: Vec<String>,
}

impl Declaration {
    fn inherited(&self) -> bool {
        matches!(self, Self::Detailed(details) if details.workspace == Some(true)
            && details.version.is_none() && details.package.is_none()
            && details.registry.is_none() && details.path.is_none() && details.git.is_none()
            && details.branch.is_none() && details.tag.is_none() && details.rev.is_none())
    }

    fn provider<'a>(&'a self, key: &'a str) -> Option<(&'a str, &'a str)> {
        match self {
            Self::Version(version) => Some((key, version)),
            Self::Detailed(details)
                if details.workspace.is_none()
                    && details.path.is_none()
                    && details.git.is_none() =>
            {
                Some((
                    details.package.as_deref().unwrap_or(key),
                    details.version.as_deref()?,
                ))
            }
            Self::Detailed(_) => None,
        }
    }
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
struct Sections {
    dependencies: BTreeMap<String, Declaration>,
    dev_dependencies: BTreeMap<String, Declaration>,
    build_dependencies: BTreeMap<String, Declaration>,
}

impl Sections {
    fn section(&self, kind: Option<&str>) -> Option<&BTreeMap<String, Declaration>> {
        match kind {
            None => Some(&self.dependencies),
            Some("dev") => Some(&self.dev_dependencies),
            Some("build") => Some(&self.build_dependencies),
            Some(_) => None,
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct Workspace {
    #[serde(default)]
    dependencies: BTreeMap<String, Declaration>,
}

#[derive(Debug, serde::Deserialize)]
struct Manifest {
    #[serde(flatten)]
    sections: Sections,
    #[serde(default)]
    target: BTreeMap<String, Sections>,
    workspace: Option<Workspace>,
}

impl Manifest {
    fn reference(
        &self,
        key: &str,
        kind: Option<&str>,
        target: Option<&str>,
    ) -> Option<&Declaration> {
        let sections = match target {
            Some(target) => self.target.get(target)?,
            None => &self.sections,
        };
        sections.section(kind)?.get(key)
    }
}

#[derive(Debug, serde::Deserialize)]
struct Metadata {
    version: u64,
    workspace_root: PathBuf,
    workspace_members: Vec<String>,
    packages: Vec<Package>,
}

#[derive(Debug, serde::Deserialize)]
struct Package {
    id: String,
    manifest_path: PathBuf,
    dependencies: Vec<CargoDependency>,
}

#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize)]
struct CargoDependency {
    name: String,
    rename: Option<String>,
    req: String,
    kind: Option<String>,
    target: Option<String>,
    registry: Option<String>,
    path: Option<PathBuf>,
    optional: bool,
    uses_default_features: bool,
    features: Vec<String>,
}

#[derive(Debug, Eq, PartialEq)]
struct Reference {
    owner: String,
    lockfile: String,
    declaration: Declaration,
    provider: Declaration,
    dependency: CargoDependency,
}

struct Snapshot {
    root: PathBuf,
    metadata: BTreeMap<String, Metadata>,
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        platform::cleanup::remove_dir_all(&self.root);
    }
}

impl Snapshot {
    fn new(repository: &str, revision: &str) -> Result<Self, &'static str> {
        Ok(Self {
            root: checkout_snapshot(repository, Some(revision))
                .ok_or("committed Cargo snapshot is unavailable")?,
            metadata: BTreeMap::new(),
        })
    }

    fn path(&self, relative: &str) -> Result<PathBuf, &'static str> {
        let path = Path::new(relative);
        if path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err("Cargo manifest path is not repository relative");
        }
        let path = self.root.join(path);
        self.relative(&path)?;
        Ok(path)
    }

    fn relative(&self, path: &Path) -> Result<String, &'static str> {
        let root =
            platform::paths::canonical(&self.root).map_err(|_| "Cargo snapshot is unavailable")?;
        let path = platform::paths::canonical(path)
            .map_err(|_| "Cargo manifest or lockfile is unavailable")?;
        path.strip_prefix(root)
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .map_err(|_| "Cargo metadata escapes the committed snapshot")
    }

    fn manifest(&self, relative: &str) -> Result<Manifest, &'static str> {
        let text = std::fs::read_to_string(self.path(relative)?)
            .map_err(|_| "Cargo manifest is unreadable")?;
        toml::from_str(&text).map_err(|_| "Cargo manifest declarations are malformed")
    }

    fn load(&mut self, manifest: &str) -> Result<(), &'static str> {
        if self.metadata.contains_key(manifest) {
            return Ok(());
        }
        let path = self.path(manifest)?;
        let output = crate::locate::child("cargo")
            .current_dir(&self.root)
            .args([
                "metadata",
                "--locked",
                "--offline",
                "--no-deps",
                "--format-version",
                "1",
                "--manifest-path",
            ])
            .arg(path)
            .output()
            .map_err(|_| "Cargo metadata could not run")?;
        if !output.status.success() {
            return Err("Cargo metadata failed for a committed snapshot");
        }
        let metadata: Metadata =
            serde_json::from_slice(&output.stdout).map_err(|_| "Cargo metadata is malformed")?;
        if metadata.version != 1 {
            return Err("Cargo metadata format is unsupported");
        }
        self.relative(&metadata.workspace_root)?;
        self.metadata.insert(manifest.to_owned(), metadata);
        Ok(())
    }

    fn reference(
        &mut self,
        record: &Record,
        fallback: Option<&str>,
    ) -> Result<Option<Reference>, &'static str> {
        let exists = self.root.join(&record.file).is_file();
        let manifest = if exists {
            record.file.as_str()
        } else if let Some(fallback) = fallback {
            if !self.root.join(fallback).is_file() {
                return Ok(None);
            }
            fallback
        } else {
            return Err("inherited Cargo member manifest is absent");
        };
        self.load(manifest)?;
        if !exists {
            return Ok(None);
        }
        let metadata = self
            .metadata
            .get(manifest)
            .ok_or("Cargo metadata is unavailable")?;
        let mut packages = metadata.packages.iter().filter(|package| {
            self.relative(&package.manifest_path).ok().as_deref() == Some(record.file.as_str())
                && metadata.workspace_members.contains(&package.id)
        });
        let package = packages
            .next()
            .ok_or("inherited Cargo record is not a workspace member")?;
        if packages.next().is_some() {
            return Err("Cargo workspace member is ambiguous");
        }
        let kind = match record.dependency.dep_type.as_deref() {
            Some("dependencies") => None,
            Some("dev-dependencies") => Some("dev"),
            Some("build-dependencies") => Some("build"),
            _ => return Err("inherited Cargo dependency kind is invalid"),
        };
        let source = self.manifest(&record.file)?;
        let Some(declaration) = source.reference(
            &record.dependency.dep_name,
            kind,
            record.dependency.target.as_deref(),
        ) else {
            return Ok(None);
        };
        if !declaration.inherited() {
            return Ok(None);
        }
        let mut dependencies = package.dependencies.iter().filter(|dependency| {
            dependency.rename.as_deref().unwrap_or(&dependency.name) == record.dependency.dep_name
                && dependency.kind.as_deref() == kind
                && dependency.target == record.dependency.target
        });
        let dependency = dependencies
            .next()
            .ok_or("inherited Cargo metadata dependency is absent")?;
        if dependencies.next().is_some() || dependency.path.is_some() {
            return Err("inherited Cargo metadata dependency is ambiguous or local");
        }
        let owner = self.relative(&metadata.workspace_root.join(MANIFEST))?;
        let lockfile = self.relative(&metadata.workspace_root.join(LOCKFILE))?;
        let provider = self
            .manifest(&owner)?
            .workspace
            .and_then(|workspace| {
                workspace
                    .dependencies
                    .get(&record.dependency.dep_name)
                    .cloned()
            })
            .ok_or("workspace dependency declaration is absent")?;
        let (name, _) = provider
            .provider(&record.dependency.dep_name)
            .ok_or("workspace dependency is not a registry provider")?;
        if dependency.name != name {
            return Err("Cargo package identity is inconsistent");
        }
        Ok(Some(Reference {
            owner,
            lockfile,
            declaration: declaration.clone(),
            provider,
            dependency: dependency.clone(),
        }))
    }
}

fn registry_matches(record: &Dependency, dependency: &CargoDependency) -> bool {
    let registry = dependency.registry.as_deref().unwrap_or(DEFAULT_REGISTRY);
    match record.registry_urls.as_deref() {
        None | Some([]) => registry == DEFAULT_REGISTRY || registry == SPARSE_REGISTRY,
        Some([found]) => found == registry,
        Some(_) => false,
    }
}

fn lock_matches(record: &Record, lockfile: &str) -> bool {
    matches!(record.lock_files.as_deref(), Some([found]) if found == lockfile)
}

fn validate_inherited(record: &Record) -> Result<(), &'static str> {
    let dependency = &record.dependency;
    if record.manager != "cargo"
        || !dependency.enabled
        || dependency.datasource.as_deref() != Some("crate")
        || dependency.current_value.as_deref() != Some("")
        || dependency.current_digest.is_some()
        || dependency.current_version.is_some()
        || dependency.locked_version.is_some()
        || !dependency.updates.is_empty()
        || !dependency.warnings.is_empty()
    {
        return Err("inherited Cargo record contains independent or failed lookup evidence");
    }
    Ok(())
}

pub(super) fn bind(records: &[Record], found: Option<&Found>) -> Result<Bindings, &'static str> {
    let inherited: Vec<_> = records
        .iter()
        .enumerate()
        .filter(|(_, record)| record.dependency.skip_reason.as_deref() == Some(INHERITED))
        .collect();
    if inherited.is_empty() {
        return Ok(Bindings::default());
    }
    let found = found.ok_or("inherited Cargo records require committed metadata evidence")?;
    let offered = found
        .offered
        .as_deref()
        .ok_or("offered Cargo revision is unavailable")?;
    let mut snapshot = Snapshot::new(&found.repository, offered)?;
    let mut baselines = BTreeMap::new();
    let mut bindings = Bindings::default();
    for (index, record) in inherited {
        validate_inherited(record)?;
        let dependency = &record.dependency;
        let reference = snapshot
            .reference(record, None)?
            .ok_or("inherited Cargo declaration is absent or inconsistent")?;
        if !lock_matches(record, &reference.lockfile)
            || !registry_matches(dependency, &reference.dependency)
            || dependency
                .package_name
                .as_deref()
                .is_some_and(|name| name != reference.dependency.name)
        {
            return Err("inherited Cargo package, registry, or lockfile owner is inconsistent");
        }
        let (_, version) = reference
            .provider
            .provider(&dependency.dep_name)
            .ok_or("Cargo provider version is absent")?;
        let mut providers = records.iter().enumerate().filter(|(_, candidate)| {
            let provider = &candidate.dependency;
            candidate.manager == "cargo"
                && candidate.file == reference.owner
                && provider.dep_name == dependency.dep_name
                && provider.dep_type.as_deref() == Some("workspace.dependencies")
                && provider.target.is_none()
        });
        let (provider_index, provider) = providers
            .next()
            .ok_or("verified Cargo workspace provider is absent")?;
        if providers.next().is_some() {
            return Err("Cargo workspace provider is ambiguous");
        }
        let selected = &provider.dependency;
        if !selected.enabled
            || selected.skip_reason.is_some()
            || !selected.warnings.is_empty()
            || selected.datasource.as_deref() != Some("crate")
            || selected.current_value.as_deref() != Some(version)
            || selected
                .package_name
                .as_deref()
                .unwrap_or(&selected.dep_name)
                != reference.dependency.name
            || !registry_matches(selected, &reference.dependency)
            || !lock_matches(provider, &reference.lockfile)
        {
            return Err("Cargo workspace provider does not match committed lookup evidence");
        }
        let changed = found.scope_unknown
            || if let Some(baseline) = found
                .baselines_by_file
                .get(&record.file)
                .or_else(|| found.baselines_by_file.get(&reference.owner))
                .or_else(|| found.baselines_by_file.values().next())
            {
                if !baselines.contains_key(baseline) {
                    baselines.insert(
                        baseline.clone(),
                        Snapshot::new(&found.repository, baseline)?,
                    );
                }
                let previous = baselines
                    .get_mut(baseline)
                    .ok_or("Cargo baseline is unavailable")?
                    .reference(record, Some(&reference.owner))?;
                previous.as_ref() != Some(&reference)
            } else {
                false
            };
        bindings.inherited.insert(index);
        if changed {
            bindings.touched.insert(provider_index);
        }
    }
    Ok(bindings)
}

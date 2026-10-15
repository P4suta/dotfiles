use super::{Candidate, Stage, code_environment, git_text, input_hashes};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const CHECKOUT: &str = "checkout";
const MANIFEST: &str = "execution.json";
const PROBE: &str = ".hosts-write-probe";
const MOVED_PROBE: &str = ".hosts-moved-probe";
const SCHEMA: u32 = 1;
const MATERIAL_PATH_LIMIT: usize = 262_144;
const MATERIAL_DEPTH_LIMIT: usize = 128;
#[cfg(unix)]
const UNIX_WRITE_BITS: u32 = 0o222;
const WRITABLE_OUTPUTS: [&str; 3] = [
    "target",
    "fuzz/artifacts",
    "xtask/proofs/fuzz-fixture/artifacts",
];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    schema: u32,
    commit: String,
    tree: String,
    build_hash: String,
    executable_hash: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Execution {
    schema: u32,
    commit: String,
    tree: String,
    inputs: [String; 2],
    gate: String,
    source: PathBuf,
    worker: PathBuf,
    build_hash: String,
    executable_hash: String,
}

impl Execution {
    pub(super) fn receipt(&self) -> Receipt {
        Receipt {
            schema: self.schema,
            commit: self.commit.clone(),
            tree: self.tree.clone(),
            build_hash: self.build_hash.clone(),
            executable_hash: self.executable_hash.clone(),
        }
    }
    pub(super) fn matches(&self, candidate: &Candidate<'_>) -> bool {
        self.schema == SCHEMA
            && self.commit == candidate.commit
            && self.tree == candidate.tree
            && self.inputs == *candidate.inputs
            && self.gate == candidate.gate
    }

    fn verify(&self, scope: &Path, gate: &str) -> Result<()> {
        let source = platform::paths::canonical(&std::env::current_dir()?)?;
        let worker = platform::paths::canonical(&std::env::current_exe()?)?;
        let built = platform::freshness::embedded();
        ensure!(
            self.schema == SCHEMA
                && self.source == scope.join(CHECKOUT)
                && self.source == source
                && self.worker.parent() == Some(scope)
                && self.worker == worker
                && self.gate == gate,
            "local worker has another owned execution identity"
        );
        ensure!(
            built.revision == self.commit
                && platform::paths::canonical(&built.root)? == source
                && built.hash == self.build_hash
                && platform::build_inputs::content_hash(&source)? == self.build_hash
                && digest(&fs::read(&worker)?) == self.executable_hash,
            "local worker does not originate from its protected captured Source"
        );
        ensure!(
            git_text(&source, &["rev-parse", "HEAD"])? == self.commit
                && git_text(&source, &["rev-parse", "HEAD^{tree}"])? == self.tree
                && input_hashes(&source, &self.commit, gate)? == self.inputs,
            "local worker reads another committed candidate"
        );
        verify_protection(&source, &source.join("Cargo.toml"), &source.join(".git"))?;
        verify_protection(scope, &worker, scope)?;
        Ok(())
    }
}

pub(super) fn worker(manifest: &Path, gate: &str) -> Result<Execution> {
    ensure!(
        fs::symlink_metadata(manifest)?.file_type().is_file(),
        "local capture manifest must be a regular file"
    );
    let manifest = platform::paths::canonical(manifest)?;
    let scope = manifest.parent().context("local capture scope")?;
    ensure!(
        manifest.file_name().is_some_and(|name| name == MANIFEST),
        "local capture manifest identity"
    );
    let execution: Execution = serde_json::from_slice(&fs::read(&manifest)?)?;
    execution.verify(scope, gate)?;
    Ok(execution)
}

#[derive(Debug)]
pub(super) struct Owned {
    protections: Vec<Protection>,
    scope: Option<tempfile::TempDir>,
    pub(super) execution: Execution,
}

impl Owned {
    pub(super) fn capture(candidate: &Candidate<'_>, bundle: &Path, home: &Path) -> Result<Self> {
        let scope = tempfile::tempdir()?;
        let directory = platform::paths::canonical(scope.path())?;
        let source = captured_source(&directory, bundle, candidate)?;
        let protection = Protection::seal(&source)?;
        verify_protection(&source, &source.join("Cargo.toml"), &source.join(".git"))?;
        let build_hash = platform::build_inputs::content_hash(&source)?;
        let environment = code_environment(&source, home)?;
        let output = environment
            .command(Tool::Cargo.program())?
            .args([
                "build",
                "--locked",
                "--package",
                "dotfiles",
                "--bin",
                "dotfiles",
                "--message-format=json-render-diagnostics",
            ])
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()
            .context("build the captured local worker")?;
        ensure!(
            output.status.success(),
            "build the captured local worker: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let artifacts: Vec<serde_json::Value> = std::str::from_utf8(&output.stdout)?
            .lines()
            .map(serde_json::from_str)
            .collect::<serde_json::Result<_>>()?;
        let executable = artifacts
            .iter()
            .find_map(|artifact| {
                (artifact["reason"] == "compiler-artifact"
                    && artifact["target"]["name"] == "dotfiles")
                    .then(|| artifact["executable"].as_str())
                    .flatten()
            })
            .context("the captured build returned no worker executable")?;
        let worker = directory.join(format!("dotfiles{}", std::env::consts::EXE_SUFFIX));
        fs::copy(executable, &worker)?;
        let executable_hash = digest(&fs::read(&worker)?);
        let execution = Execution {
            schema: SCHEMA,
            commit: candidate.commit.to_owned(),
            tree: candidate.tree.to_owned(),
            inputs: candidate.inputs.clone(),
            gate: candidate.gate.to_owned(),
            source,
            worker,
            build_hash,
            executable_hash,
        };
        fs::write(directory.join(MANIFEST), serde_json::to_vec(&execution)?)?;
        fs::write(directory.join(PROBE), [])?;
        let outer = Protection::seal(&directory)?;
        verify_protection(&directory, &execution.worker, &directory)?;
        Ok(Self {
            protections: vec![protection, outer],
            scope: Some(scope),
            execution,
        })
    }

    pub(super) fn command(&self, home: &Path, skipped: [bool; 2]) -> Result<std::process::Command> {
        let mut command = crate::host_checks::unattended(&self.execution.worker);
        command
            .args(["hosts", "worker", "--gate", &self.execution.gate, "--home"])
            .arg(home)
            .arg("--capture")
            .arg(
                self.execution
                    .worker
                    .parent()
                    .context("captured worker scope")?
                    .join(MANIFEST),
            )
            .env("GIT_OPTIONAL_LOCKS", "0")
            .current_dir(&self.execution.source);
        for (stage, skip) in Stage::ALL.into_iter().zip(skipped) {
            if skip {
                command.arg(match stage {
                    Stage::Code => "--skip-code",
                    Stage::Real => "--skip-real",
                });
            }
        }
        Ok(command)
    }

    pub(super) fn close(&mut self) -> Result<()> {
        for protection in self.protections.iter_mut().rev() {
            protection.release()?;
        }
        self.protections.clear();
        if let Some(scope) = self.scope.take() {
            let directory = scope.path().to_owned();
            scope.close().with_context(|| {
                format!("remove the owned local candidate {}", directory.display())
            })?;
        }
        Ok(())
    }
}

fn captured_source(directory: &Path, bundle: &Path, candidate: &Candidate<'_>) -> Result<PathBuf> {
    let source = directory.join(CHECKOUT);
    let empty = directory.join("empty-git-config");
    fs::write(&empty, [])?;
    let clone = Tool::Git
        .command()
        .arg("-C")
        .arg(directory)
        .args(["-c", "advice.detachedHead=false", "clone", "--quiet"])
        .arg(bundle)
        .arg(CHECKOUT)
        .env("GIT_CONFIG_GLOBAL", &empty)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()?;
    ensure!(
        clone.status.success(),
        "capture the local candidate: {}",
        String::from_utf8_lossy(&clone.stderr)
    );
    ensure!(
        git_text(&source, &["rev-parse", "HEAD"])? == candidate.commit
            && git_text(&source, &["rev-parse", "HEAD^{tree}"])? == candidate.tree
            && input_hashes(&source, candidate.commit, candidate.gate)? == *candidate.inputs,
        "captured local bundle holds another candidate"
    );
    fs::write(source.join(".git").join(PROBE), [])?;
    prepare_outputs(&source)?;
    Ok(source)
}

pub(super) fn protect_remote(source: &Path) -> Result<Protection> {
    prepare_outputs(source)?;
    fs::write(source.join(".git").join(PROBE), [])?;
    let protection = Protection::seal(source)?;
    verify_protection(source, &source.join("Cargo.toml"), &source.join(".git"))?;
    Ok(protection)
}

fn prepare_outputs(source: &Path) -> Result<()> {
    for relative in WRITABLE_OUTPUTS {
        ensure!(
            super::git(source, &["ls-files", "-z", "--", relative])?.is_empty(),
            "writable verifier output contains tracked Source: {relative}"
        );
        let output = source.join(relative);
        fs::create_dir_all(&output)?;
        ensure!(
            !fs::symlink_metadata(output)?.file_type().is_symlink(),
            "writable verifier output must be an owned directory"
        );
    }
    Ok(())
}

fn writable_output(path: &Path) -> bool {
    path.ancestors()
        .find(|ancestor| ancestor.join(".git").is_dir() && ancestor.join("Cargo.toml").is_file())
        .is_some_and(|source| {
            WRITABLE_OUTPUTS
                .iter()
                .any(|relative| path == source.join(relative))
        })
}

impl Drop for Owned {
    fn drop(&mut self) {
        platform::cleanup::report(self.close(), "remove the owned local candidate");
    }
}

fn digest(bytes: &[u8]) -> String {
    crate::build_inputs::hex(&Sha256::digest(bytes))
}

fn verify_protection(root: &Path, file: &Path, directory: &Path) -> Result<()> {
    let mut paths = Vec::new();
    collect(root, &mut paths)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ensure!(
            paths
                .iter()
                .all(|path| fs::metadata(path)
                    .is_ok_and(|metadata| metadata.permissions().mode() & UNIX_WRITE_BITS == 0)),
            "captured Source has writable permissions"
        );
    }
    ensure!(
        fs::OpenOptions::new()
            .write(true)
            .open(file)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::PermissionDenied),
        "captured file protection does not reject writes"
    );
    let probe = directory.join(PROBE);
    let moved = directory.join(MOVED_PROBE);
    match fs::rename(&probe, &moved) {
        Ok(()) => {
            fs::rename(&moved, &probe).context("restore the owned protection control")?;
            anyhow::bail!("captured directory protection does not reject entry replacement");
        }
        Err(error) => ensure!(
            error.kind() == std::io::ErrorKind::PermissionDenied,
            "captured directory rejection has another cause: {error}"
        ),
    }
    Ok(())
}

#[derive(Debug)]
pub(super) struct Protection {
    #[cfg(unix)]
    permissions: Vec<(PathBuf, fs::Permissions)>,
    #[cfg(windows)]
    root: PathBuf,
    #[cfg(windows)]
    owner: Option<String>,
    #[cfg(windows)]
    material: Option<Vec<MaterialAcl>>,
}

#[cfg(windows)]
#[derive(Debug)]
enum MaterialAcl {
    Tree(PathBuf),
    Node(PathBuf),
}

impl Protection {
    pub(super) fn seal_materials(namespace: &Path, roots: &[PathBuf]) -> Result<Vec<Self>> {
        let namespace = platform::paths::canonical(namespace)?;
        let inventory = material_inventory(&namespace, roots)?;
        let mut protection = Self {
            #[cfg(unix)]
            permissions: Vec::new(),
            #[cfg(windows)]
            root: namespace,
            #[cfg(windows)]
            owner: None,
            #[cfg(windows)]
            material: Some(inventory.acls()),
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for path in inventory.paths {
                let permissions = fs::metadata(&path)?.permissions();
                let mode = permissions.mode();
                protection.permissions.push((path.clone(), permissions));
                fs::set_permissions(path, fs::Permissions::from_mode(mode & !UNIX_WRITE_BITS))?;
            }
        }
        #[cfg(windows)]
        {
            let owner = material_owner()?;
            protection.owner = Some(owner.clone());
            for target in protection.material.as_deref().unwrap_or_default() {
                let mut command = material_acl(target);
                command
                    .arg("/deny")
                    .arg(format!("*{owner}:(WD,AD,WEA,WA,DE,DC)"));
                acl(&mut command)?;
            }
        }
        Ok(vec![protection])
    }

    pub(super) fn seal(root: &Path) -> Result<Self> {
        let mut paths = Vec::new();
        collect(root, &mut paths)?;
        let mut protection = Self {
            #[cfg(unix)]
            permissions: Vec::new(),
            #[cfg(windows)]
            root: root.to_owned(),
            #[cfg(windows)]
            owner: None,
            #[cfg(windows)]
            material: None,
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for path in paths {
                let permissions = fs::metadata(&path)?.permissions();
                let mode = permissions.mode();
                protection.permissions.push((path.clone(), permissions));
                fs::set_permissions(path, fs::Permissions::from_mode(mode & !UNIX_WRITE_BITS))?;
            }
        }
        #[cfg(windows)]
        {
            let output = Tool::Whoami
                .command()
                .args(["/user", "/fo", "csv", "/nh"])
                .output()?;
            ensure!(
                output.status.success(),
                "resolve the owned Source protection SID"
            );
            let text = String::from_utf8(output.stdout)?;
            let owner = text
                .split([',', '"', ' ', '\r', '\n'])
                .find(|field| {
                    field.strip_prefix("S-1-").is_some_and(|suffix| {
                        !suffix.is_empty()
                            && suffix
                                .bytes()
                                .all(|byte| byte.is_ascii_digit() || byte == b'-')
                    })
                })
                .context("Source protection SID")?;
            protection.owner = Some(owner.to_owned());
            let mut command = Tool::Icacls.command();
            command
                .arg(root)
                .arg("/deny")
                .arg(format!("*{owner}:(WD,AD,WEA,WA,DE,DC)"))
                .args(["/T", "/Q"]);
            acl(&mut command)?;
            for path in output_directories(root)? {
                let mut command = Tool::Icacls.command();
                command
                    .arg(path)
                    .arg("/remove:d")
                    .arg(format!("*{owner}"))
                    .args(["/T", "/Q"]);
                acl(&mut command)?;
            }
        }
        Ok(protection)
    }

    fn release(&mut self) -> Result<()> {
        #[cfg(unix)]
        {
            for (path, permissions) in &self.permissions {
                fs::set_permissions(path, permissions.clone())?;
            }
            self.permissions.clear();
        }
        #[cfg(windows)]
        if let Some(owner) = &self.owner {
            if let Some(targets) = &self.material {
                for target in targets {
                    let mut command = material_acl(target);
                    command.arg("/remove:d").arg(format!("*{owner}"));
                    acl(&mut command)?;
                }
                self.owner = None;
                return Ok(());
            }
            let mut command = Tool::Icacls.command();
            command
                .arg(&self.root)
                .arg("/remove:d")
                .arg(format!("*{owner}"))
                .args(["/T", "/Q"]);
            acl(&mut command)?;
            self.owner = None;
        }
        Ok(())
    }
}

impl Drop for Protection {
    fn drop(&mut self) {
        platform::cleanup::report(self.release(), "restore the owned Source permissions");
    }
}

fn collect(root: &Path, found: &mut Vec<PathBuf>) -> Result<()> {
    if writable_output(root) {
        return Ok(());
    }
    let kind = fs::symlink_metadata(root)?.file_type();
    ensure!(
        !kind.is_symlink() && (kind.is_dir() || kind.is_file()),
        "captured Source cannot read through a filesystem link or special node"
    );
    found.push(root.to_owned());
    if kind.is_dir() {
        for entry in fs::read_dir(root)? {
            collect(&entry?.path(), found)?;
        }
    }
    Ok(())
}

struct MaterialInventory {
    paths: BTreeSet<PathBuf>,
    #[cfg(windows)]
    trees: BTreeSet<PathBuf>,
    #[cfg(windows)]
    links: BTreeSet<PathBuf>,
}

#[cfg(windows)]
impl MaterialInventory {
    fn acls(&self) -> Vec<MaterialAcl> {
        let mut targets: Vec<_> = self.trees.iter().cloned().map(MaterialAcl::Tree).collect();
        targets.extend(
            self.paths
                .iter()
                .chain(&self.links)
                .filter(|path| !self.trees.iter().any(|root| path.starts_with(root)))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .cloned()
                .map(MaterialAcl::Node),
        );
        targets
    }
}

fn material_inventory(namespace: &Path, roots: &[PathBuf]) -> Result<MaterialInventory> {
    ensure!(
        !roots.is_empty(),
        "native materials require nonempty immutable roots"
    );
    for ancestor in namespace.ancestors() {
        let metadata = fs::symlink_metadata(ancestor)?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "native material namespace has a linked or special ancestor"
        );
    }
    let mut physical = BTreeSet::new();
    for root in roots {
        ensure!(
            root.is_absolute() && root.starts_with(namespace),
            "native material roots must remain in their owned namespace"
        );
        let canonical = platform::paths::canonical(root)?;
        ensure!(
            canonical.starts_with(namespace),
            "native material root escapes its owned namespace"
        );
        if !fs::symlink_metadata(root)?.file_type().is_symlink() {
            physical.insert(canonical);
        }
    }
    let mut inventory = MaterialInventory {
        paths: BTreeSet::new(),
        #[cfg(windows)]
        trees: physical.clone(),
        #[cfg(windows)]
        links: BTreeSet::new(),
    };
    let mut visited = BTreeSet::new();
    for root in roots {
        material_collect(namespace, root, &physical, 0, &mut visited, &mut inventory)?;
        for ancestor in root.ancestors().skip(1) {
            if !ancestor.starts_with(namespace) {
                break;
            }
            let canonical = platform::paths::canonical(ancestor)?;
            ensure!(
                canonical.starts_with(namespace) && fs::metadata(&canonical)?.is_dir(),
                "native material ancestor escapes its namespace"
            );
            inventory.paths.insert(canonical);
        }
    }
    Ok(inventory)
}

fn material_collect(
    namespace: &Path,
    path: &Path,
    roots: &BTreeSet<PathBuf>,
    depth: usize,
    visited: &mut BTreeSet<PathBuf>,
    inventory: &mut MaterialInventory,
) -> Result<()> {
    ensure!(
        depth <= MATERIAL_DEPTH_LIMIT && inventory.paths.len() < MATERIAL_PATH_LIMIT,
        "native material protection inventory exceeds its bound"
    );
    let kind = fs::symlink_metadata(path)?.file_type();
    let canonical = platform::paths::canonical(path)?;
    ensure!(
        canonical.starts_with(namespace) && roots.iter().any(|root| canonical.starts_with(root)),
        "native material link escapes its protected immutable roots"
    );
    if kind.is_symlink() {
        let parent = path
            .parent()
            .context("native material link has no parent")?;
        inventory.paths.insert(platform::paths::canonical(parent)?);
        #[cfg(windows)]
        {
            ensure!(
                inventory.links.len() < MATERIAL_PATH_LIMIT,
                "native material link inventory exceeds its bound"
            );
            inventory.links.insert(path.to_owned());
        }
    } else {
        ensure!(
            kind.is_file() || kind.is_dir(),
            "native materials contain a special node"
        );
    }
    if !visited.insert(canonical.clone()) {
        return Ok(());
    }
    inventory.paths.insert(canonical.clone());
    let metadata = fs::metadata(&canonical)?;
    if metadata.is_dir() {
        for entry in fs::read_dir(&canonical)? {
            material_collect(
                namespace,
                &entry?.path(),
                roots,
                depth + 1,
                visited,
                inventory,
            )?;
        }
    } else {
        ensure!(
            metadata.is_file(),
            "native material endpoint is not regular"
        );
    }
    Ok(())
}

#[cfg(windows)]
fn material_owner() -> Result<String> {
    let output = Tool::Whoami
        .command()
        .args(["/user", "/fo", "csv", "/nh"])
        .output()?;
    ensure!(
        output.status.success(),
        "resolve the native material protection SID"
    );
    let text = String::from_utf8(output.stdout)?;
    text.split([',', '"', ' ', '\r', '\n'])
        .find(|field| {
            field.strip_prefix("S-1-").is_some_and(|suffix| {
                !suffix.is_empty()
                    && suffix
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || byte == b'-')
            })
        })
        .map(str::to_owned)
        .context("native material protection SID")
}

#[cfg(windows)]
fn material_acl(target: &MaterialAcl) -> std::process::Command {
    let mut command = Tool::Icacls.command();
    match target {
        MaterialAcl::Tree(path) => {
            command.arg(path).arg("/T");
        }
        MaterialAcl::Node(path) => {
            command.arg(path);
        }
    }
    command.args(["/L", "/Q"]);
    command
}

#[cfg(windows)]
fn output_directories(root: &Path) -> Result<Vec<PathBuf>> {
    let source = if root.join(".git").is_dir() {
        root.to_owned()
    } else {
        root.join(CHECKOUT)
    };
    let mut outputs = Vec::new();
    for relative in WRITABLE_OUTPUTS {
        let path = source.join(relative);
        if path.is_dir() {
            ensure!(
                super::git(&source, &["ls-files", "-z", "--", relative])?.is_empty(),
                "writable output contains tracked Source"
            );
            outputs.push(path);
        }
    }
    Ok(outputs)
}

#[cfg(windows)]
fn acl(command: &mut std::process::Command) -> Result<()> {
    let output = command.output()?;
    ensure!(
        output.status.success(),
        "Source protection failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{GateRun, STAGE_REPORT, Target, git, reported};
    use super::*;
    use crate::hosts_rules::Family;

    const CARGO_FIXTURE_DEPENDENCY: &str = "example";
    const CARGO_FIXTURE_VERSION: &str = "1.2.3";
    const CARGO_FIXTURE_REGISTRY: &str = "registry+https://github.com/rust-lang/crates.io-index";
    const PINNED_MISE_FIXTURE: &str = "DOTFILES_VERIFIED_MISE_BOOTSTRAP";
    const MISE_FIXTURE_RELEASES: &str = "home/.chezmoidata/versions.toml";

    #[test]
    #[ignore = "requires the source-authenticated pinned mise bootstrap"]
    fn failed_protected_code_resolution_releases_guards_before_owned_cleanup() -> Result<()> {
        let source = tempfile::tempdir()?;
        let materials = tempfile::tempdir()?;
        let namespace = platform::paths::canonical(materials.path())?;
        fs::write(source.path().join("mise.toml"), "[tools]\n")?;
        fs::write(source.path().join("mise.lock"), "lockfile_version=3\n")?;
        let releases = source.path().join(MISE_FIXTURE_RELEASES);
        fs::create_dir_all(releases.parent().context("fixture release parent")?)?;
        fs::write(
            releases,
            include_bytes!("../../../../home/.chezmoidata/versions.toml"),
        )?;
        let installed = platform::code_environment::Installations {
            mise: platform::paths::canonical(Path::new(
                &std::env::var_os(PINNED_MISE_FIXTURE)
                    .context("provide the verified pinned mise bootstrap")?,
            ))?,
            data: namespace.join("native-data"),
            cargo: namespace.join("native-cargo"),
            rustup: namespace.join("native-rustup"),
            kani: namespace.join("native-kani"),
            cache: namespace.join("cache"),
        };
        let mut preparation = platform::code_environment::CodeEnvironment::prepare_in(
            source.path(),
            &installed,
            &namespace,
        )?;
        let immutable = preparation.immutable_paths();
        let project = immutable
            .first()
            .context("private configuration")?
            .parent()
            .context("private project")?;
        let scope = project.parent().context("owned code scope")?.to_owned();
        let guards = Protection::seal_materials(&namespace, &immutable)?;
        let failure = preparation
            .build(&installed)
            .expect_err("original resolution requires its native store binding");
        assert_eq!(
            failure.to_string(),
            "prepared code environment has no resolver binding"
        );
        drop(guards);
        drop(preparation);
        assert!(
            !scope.exists(),
            "failed consuming resolution left an owned protected scope: {}",
            scope.display()
        );
        platform::cleanup::report(
            materials.close(),
            "remove the refused protected Code fixture",
        );
        Ok(())
    }

    struct CargoFixtureBytes(Vec<u8>);

    impl crate::artifacts::Fetch for CargoFixtureBytes {
        fn fetch(
            &self,
            _source: &str,
            destination: &Path,
        ) -> Result<(), crate::artifacts::FetchFailure> {
            fs::write(destination, &self.0)
                .map_err(|error| crate::artifacts::FetchFailure::Refused(error.to_string()))
        }
    }

    fn cargo_fixture_release() -> Result<CargoFixtureBytes> {
        let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut archive = tar::Builder::new(gzip);
        for (path, contents) in [
            (
                "Cargo.toml",
                "[package]\nname='example'\nversion='1.2.3'\nedition='2024'\n",
            ),
            ("src/lib.rs", "pub fn value() -> u32 { 37 }\n"),
        ] {
            let mut header = tar::Header::new_ustar();
            header.set_size(contents.len() as u64);
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_cksum();
            archive.append_data(
                &mut header,
                format!("{CARGO_FIXTURE_DEPENDENCY}-{CARGO_FIXTURE_VERSION}/{path}"),
                contents.as_bytes(),
            )?;
        }
        Ok(CargoFixtureBytes(archive.into_inner()?.finish()?))
    }

    fn cargo_fixture_materials(
        namespace: &Path,
    ) -> Result<(
        crate::artifacts::CargoWorkload,
        crate::artifacts::PreparedCargo,
        PathBuf,
    )> {
        let source = namespace.join("source");
        fs::create_dir_all(source.join("src"))?;
        fs::write(
            source.join("Cargo.toml"),
            "[package]\nname='consumer'\nversion='0.1.0'\nedition='2024'\n[workspace]\n[dependencies]\nexample='=1.2.3'\n",
        )?;
        fs::write(
            source.join("src/main.rs"),
            "fn main() { println!(\"{}\", example::value()); }\n",
        )?;
        let release = cargo_fixture_release()?;
        fs::write(
            source.join("Cargo.lock"),
            format!(
                "version=4\n[[package]]\nname='consumer'\nversion='0.1.0'\ndependencies=['example']\n[[package]]\nname='{CARGO_FIXTURE_DEPENDENCY}'\nversion='{CARGO_FIXTURE_VERSION}'\nsource='{CARGO_FIXTURE_REGISTRY}'\nchecksum='{}'\n",
                crate::artifacts::Digest::of(&release.0).hex()
            ),
        )?;
        let workload = crate::artifacts::CargoWorkload::capture(&source)?;
        let plan = workload.plan()?;
        let verified =
            crate::artifacts::Cache::new(namespace.join("cache")).acquire(&plan, &release)?;
        let prepared = verified.prepare_cargo(&workload, namespace)?;
        Ok((workload, prepared, source))
    }

    #[test]
    fn material_protection_preserves_fresh_offline_cargo_and_immutable_inputs() -> Result<()> {
        let owned = tempfile::tempdir()?;
        let namespace = platform::paths::canonical(owned.path())?;
        let (workload, prepared, source) = cargo_fixture_materials(&namespace)?;
        let target = namespace.join("target");
        fs::create_dir(&target)?;
        let config = prepared.directory().join("cargo-home/config.toml");
        let cargo_home = config.parent().context("prepared Cargo home")?;
        let vendor = prepared.directory().join("vendor");
        let vendor_source = vendor.join("example-1.2.3/src/lib.rs");
        assert!(!cargo_home.join("registry").exists());
        let protections = Protection::seal_materials(&namespace, &prepared.immutable_paths())?;
        assert!(fs::write(&config, b"replaced source configuration").is_err());
        assert!(fs::write(&vendor_source, b"replaced dependency source").is_err());
        assert!(fs::rename(&config, cargo_home.join("replaced-config.toml")).is_err());
        assert!(fs::rename(cargo_home, prepared.directory().join("replaced-home")).is_err());
        assert!(fs::rename(&vendor, prepared.directory().join("replaced-vendor")).is_err());
        let mut command = Tool::Cargo.command();
        command
            .current_dir(&source)
            .args(["build", "--locked", "--offline", "--jobs", "2"])
            .env("CARGO_TARGET_DIR", &target)
            .env("CARGO_INCREMENTAL", "0")
            .env("CARGO_PROFILE_DEV_DEBUG", "0");
        prepared.configure(&workload, &mut command)?;
        let output = command.output()?;
        assert!(
            output.status.success(),
            "original fresh Cargo failed under material protection: {:?}\nstdout={}\nstderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let binary = target.join(format!("debug/consumer{}", std::env::consts::EXE_SUFFIX));
        let executed = platform::process::command(binary).output()?;
        assert!(executed.status.success());
        assert_eq!(executed.stdout, b"37\n");
        assert!(!cargo_home.join("registry/index").exists());
        drop(protections);
        fs::write(&config, b"released configuration")?;
        drop(prepared);
        platform::cleanup::report(owned.close(), "remove the sealed fresh Cargo fixture");
        Ok(())
    }

    fn material_link(target: &Path, link: &Path) -> Result<()> {
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link)?;
        #[cfg(windows)]
        {
            let mut command = Tool::Pwsh.command();
            let output = command.args(["-NoLogo", "-NoProfile", "-Command", "& { param([string]$Link,[string]$Destination) New-Item -ItemType Junction -Path $Link -Value $Destination -ErrorAction Stop | Out-Null }"]).arg(link).arg(target).output()?;
            ensure!(
                output.status.success(),
                "create native material junction: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }

    #[test]
    fn material_protection_seals_owned_link_targets_and_parent_entries() -> Result<()> {
        let owned = tempfile::tempdir()?;
        let namespace = platform::paths::canonical(owned.path())?;
        let toolchains = namespace.join("home/.rustup/toolchains");
        let compiler = toolchains.join("pinned-compiler");
        let kani = namespace.join("home/.kani/kani-fixture");
        let mutable = namespace.join("tmp/operations");
        for directory in [compiler.join("bin"), kani.clone(), mutable.clone()] {
            fs::create_dir_all(directory)?;
        }
        let program = compiler.join("bin/rustc");
        fs::write(&program, b"owned authenticated fixture bytes")?;
        let linked = kani.join("toolchain");
        material_link(&compiler, &linked)?;
        let logical_parent = namespace.join("data/installs/rust");
        fs::create_dir_all(&logical_parent)?;
        let logical_slot = logical_parent.join("1.99.0");
        material_link(&compiler, &logical_slot)?;
        let guards = Protection::seal_materials(
            &namespace,
            &[toolchains.clone(), kani, logical_slot.clone()],
        )?;
        assert_eq!(
            fs::read(linked.join("bin/rustc"))?,
            b"owned authenticated fixture bytes"
        );
        assert!(fs::write(&program, b"unexpected write").is_err());
        assert!(fs::write(linked.join("bin/rustc"), b"write through owned link").is_err());
        assert!(fs::rename(&compiler, toolchains.join("replaced-compiler")).is_err());
        assert!(fs::rename(namespace.join("home"), namespace.join("replaced-home")).is_err());
        #[cfg(windows)]
        assert!(fs::remove_dir(&logical_slot).is_err());
        #[cfg(unix)]
        assert!(fs::remove_file(&logical_slot).is_err());
        assert_eq!(platform::paths::canonical(&logical_slot)?, compiler);
        fs::write(
            mutable.join("health.stdout"),
            b"ordinary writable health output",
        )?;
        drop(guards);
        fs::write(&program, b"permissions released")?;
        platform::cleanup::report(owned.close(), "remove the owned linked native fixture");
        Ok(())
    }

    #[test]
    fn material_protection_refuses_external_and_unprotected_mutable_link_targets() -> Result<()> {
        for escaped in [false, true] {
            let owned = tempfile::tempdir()?;
            let namespace = platform::paths::canonical(owned.path())?;
            let outside = tempfile::tempdir()?;
            let target = if escaped {
                platform::paths::canonical(outside.path())?
            } else {
                namespace.join("tmp")
            };
            let material = namespace.join("distribution");
            fs::create_dir_all(&target)?;
            fs::create_dir(&material)?;
            let original = material.join("original");
            fs::write(&original, b"original fixture bytes")?;
            material_link(&target, &material.join("redirected"))?;
            assert!(Protection::seal_materials(&namespace, &[material]).is_err());
            fs::write(&original, b"no partially retained guard")?;
            platform::cleanup::report(owned.close(), "remove the refused native material fixture");
            platform::cleanup::report(outside.close(), "remove the foreign material fixture");
        }
        Ok(())
    }

    #[test]
    fn material_links_do_not_relax_the_strict_source_seal() -> Result<()> {
        let owned = tempfile::tempdir()?;
        let namespace = platform::paths::canonical(owned.path())?;
        let target = namespace.join("target-material");
        let source = namespace.join("captured-source");
        fs::create_dir(&target)?;
        fs::create_dir(&source)?;
        material_link(&target, &source.join("owned-link"))?;
        assert!(Protection::seal(&source).is_err());
        platform::cleanup::report(owned.close(), "remove the strict Source link fixture");
        Ok(())
    }

    fn fixture(root: &Path) -> Result<(String, String, [String; 2])> {
        git(root, &["init", "--quiet"])?;
        fs::write(root.join("Cargo.toml"), "captured A")?;
        git(root, &["add", "Cargo.toml"])?;
        let tree = git_text(root, &["write-tree"])?;
        let commit = git_text(
            root,
            &[
                "-c",
                "user.name=fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit-tree",
                &tree,
                "-m",
                "fixture",
            ],
        )?;
        git(root, &["update-ref", "HEAD", &commit])?;
        Ok((
            commit.clone(),
            tree,
            input_hashes(root, &commit, "fixture gate")?,
        ))
    }

    #[test]
    fn captured_source_survives_live_edits_and_rejects_file_and_entry_writes() -> Result<()> {
        let live = tempfile::tempdir()?;
        let (commit, tree, inputs) = fixture(live.path())?;
        let snapshot = tempfile::tempdir()?;
        let bundle = snapshot.path().join("head.bundle");
        git(
            live.path(),
            &["bundle", "create", &bundle.to_string_lossy(), "HEAD"],
        )?;
        fs::write(live.path().join("Cargo.toml"), "live B")?;
        let owned = tempfile::tempdir()?;
        let candidate = Candidate {
            root: live.path(),
            commit: &commit,
            tree: &tree,
            inputs: &inputs,
            gate: "fixture gate",
        };
        let source = captured_source(owned.path(), &bundle, &candidate)?;
        assert_eq!(fs::read_to_string(source.join("Cargo.toml"))?, "captured A");
        assert_eq!(
            fs::read_to_string(live.path().join("Cargo.toml"))?,
            "live B"
        );
        let mut protection = Protection::seal(&source)?;
        verify_protection(&source, &source.join("Cargo.toml"), &source.join(".git"))?;
        assert!(fs::write(source.join("Cargo.toml"), "changed").is_err());
        fs::write(source.join("target/owned-output"), "writable output")?;
        protection.release()?;
        fs::write(source.join("Cargo.toml"), "released owned C")?;
        assert_eq!(
            fs::read_to_string(live.path().join("Cargo.toml"))?,
            "live B"
        );
        let private_worker = owned.path().join("worker");
        fs::write(&private_worker, "private worker bytes")?;
        fs::write(owned.path().join(PROBE), [])?;
        let mut outer = Protection::seal(owned.path())?;
        verify_protection(owned.path(), &private_worker, owned.path())?;
        assert!(fs::write(&private_worker, "replacement worker").is_err());
        assert!(fs::rename(&source, owned.path().join("replaced-checkout")).is_err());
        outer.release()?;
        Ok(())
    }

    #[test]
    fn captured_bundle_identity_refuses_another_candidate() -> Result<()> {
        let live = tempfile::tempdir()?;
        let (commit, _, inputs) = fixture(live.path())?;
        let snapshot = tempfile::tempdir()?;
        let bundle = snapshot.path().join("head.bundle");
        git(
            live.path(),
            &["bundle", "create", &bundle.to_string_lossy(), "HEAD"],
        )?;
        let owned = tempfile::tempdir()?;
        let refused = captured_source(
            owned.path(),
            &bundle,
            &Candidate {
                root: live.path(),
                commit: &commit,
                tree: "another tree",
                inputs: &inputs,
                gate: "fixture gate",
            },
        )
        .expect_err("another candidate must be refused");
        assert!(
            refused
                .to_string()
                .contains("captured local bundle holds another candidate")
        );
        Ok(())
    }

    #[test]
    fn local_report_requires_exact_execution_including_private_worker_hash() -> Result<()> {
        let execution = Execution {
            schema: SCHEMA,
            commit: "a".repeat(40),
            tree: "b".repeat(40),
            inputs: ["c".repeat(64), "d".repeat(64)],
            gate: "fixture".into(),
            source: "owned/checkout".into(),
            worker: "owned/dotfiles".into(),
            build_hash: "e".repeat(64),
            executable_hash: "f".repeat(64),
        };
        let report = |execution: &Execution| {
            format!(
                "{STAGE_REPORT}{}",
                serde_json::json!({"code":0,"real":0,"applied":null,"workspace":null,"capture":execution})
            )
        };
        let mut run = GateRun {
            family: Family::Mac,
            target: Target::Local("fixture".into()),
            status: Some(0),
            output: report(&execution),
            reused: [None, None],
            capture: Some(execution.clone()),
        };
        reported(&run)?;
        let mut wrong = execution;
        wrong.executable_hash = "0".repeat(64);
        run.output = report(&wrong);
        assert!(reported(&run).is_err());
        run.capture = None;
        assert!(reported(&run).is_err());
        Ok(())
    }

    #[test]
    fn writable_owned_source_cannot_be_reported_as_protected() -> Result<()> {
        let owned = tempfile::tempdir()?;
        fs::write(owned.path().join(PROBE), [])?;
        let file = owned.path().join("worker");
        fs::write(&file, "fixture")?;
        assert!(verify_protection(owned.path(), &file, owned.path()).is_err());
        Ok(())
    }

    #[test]
    fn a_local_start_preserves_reused_credit_and_blocks_unmeasured_credit_on_recovery() -> Result<()>
    {
        use super::super::{Note, Stage, append_note, pending, reuse_plan};
        let live = tempfile::tempdir()?;
        let (commit, tree, inputs) = fixture(live.path())?;
        for stage in Stage::ALL {
            append_note(
                live.path(),
                &commit,
                &Note {
                    host: "fixture".into(),
                    os: Family::Linux.name().into(),
                    tree: tree.clone(),
                    status: 0,
                    gate: "fixture gate".into(),
                    stage: Some(stage),
                    applied: None,
                    inputs: Some(inputs[stage.index()].clone()),
                    generation: None,
                    reused: None,
                    workspace: None,
                    capture: None,
                },
            )?;
        }
        let mut reused =
            reuse_plan(live.path(), [true, false, false], &inputs, "fixture gate")?[0].clone();
        assert!(reused.iter().all(Option::is_some));
        reused[Stage::Real.index()] = None;
        let snapshot = tempfile::tempdir()?;
        let bundle = snapshot.path().join("head.bundle");
        git(
            live.path(),
            &["bundle", "create", &bundle.to_string_lossy(), "HEAD"],
        )?;
        let lease = pending::Lease::acquire(live.path(), Family::Linux)?;
        lease.begin_local(
            &Candidate {
                root: live.path(),
                commit: &commit,
                tree: &tree,
                inputs: &inputs,
                gate: "fixture gate",
            },
            snapshot.path(),
            "fixture",
            reused,
        )?;
        assert!(
            pending::cutoff(
                live.path(),
                Family::Linux,
                Stage::Code,
                &inputs[0],
                "fixture gate"
            )?
            .is_none()
        );
        assert!(
            pending::cutoff(
                live.path(),
                Family::Linux,
                Stage::Real,
                &inputs[1],
                "fixture gate"
            )?
            .is_some()
        );
        let unavailable = crate::runtime::Real {
            home: live.path().to_owned(),
            path: std::ffi::OsString::default(),
            environment: Vec::new(),
            directory: None,
        };
        assert!(lease.recover(&unavailable, live.path())?.is_none());
        let planned =
            reuse_plan(live.path(), [true, false, false], &inputs, "fixture gate")?[0].clone();
        assert!(planned[0].is_some());
        assert!(planned[1].is_none());
        lease.retire_unverified(live.path())?;
        assert!(!pending::exists(live.path(), Family::Linux)?);
        assert!(
            pending::cutoff(
                live.path(),
                Family::Linux,
                Stage::Real,
                &inputs[1],
                "fixture gate"
            )?
            .is_some()
        );
        Ok(())
    }
}

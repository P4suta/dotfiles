use super::{
    Artifact, Cache, CargoBudget, CargoWorkload, Digest, Fetch, PreparedCargo, VerifiedArtifact,
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const PROCESS_BYTES: u64 = 8 * 1024 * 1024;
const PROCESS_LIMIT: Duration = Duration::from_secs(600);
const POLL: Duration = Duration::from_millis(20);
const TREE_FILES: usize = 262_144;
const TREE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const TREE_DEPTH: usize = 128;
const OPERATION_OUTPUTS: &str = "tmp/operations";

/// Original bounded native output grants neither installation nor verification credit.
#[derive(Clone, Debug, Serialize)]
pub struct NativeMaterialOperation {
    pub name: String,
    pub status: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_complete: bool,
    pub stderr_complete: bool,
}

/// Preserves the original failed native output for the caller's private diagnostics.
#[derive(Debug)]
pub struct NativeMaterialFailure {
    pub operation: NativeMaterialOperation,
}

impl std::fmt::Display for NativeMaterialFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} failed: {:?}; stdout={}; stderr={}",
            self.operation.name,
            self.operation.status,
            String::from_utf8_lossy(&self.operation.stdout),
            String::from_utf8_lossy(&self.operation.stderr)
        )
    }
}

impl std::error::Error for NativeMaterialFailure {}

pub(super) fn operation(
    mut command: Command,
    root: &Path,
    name: &str,
) -> Result<NativeMaterialOperation> {
    let raw = tempfile::tempdir_in(prepare_output_directory(root)?)?;
    let result = (|| {
        let stdout = raw.path().join("stdout");
        let stderr = raw.path().join("stderr");
        command
            .stdin(Stdio::null())
            .stdout(File::create(&stdout)?)
            .stderr(File::create(&stderr)?);
        let mut child = command.spawn()?;
        let started = Instant::now();
        let status = (|| {
            loop {
                ensure!(
                    started.elapsed() <= PROCESS_LIMIT
                        && fs::metadata(&stdout)?.len() <= PROCESS_BYTES
                        && fs::metadata(&stderr)?.len() <= PROCESS_BYTES,
                    "native material operation exceeded its bound"
                );
                if let Some(status) = child.try_wait()? {
                    break Ok::<_, anyhow::Error>(status);
                }
                std::thread::sleep(POLL);
            }
        })();
        if status.is_err() {
            platform::cleanup::report(child.kill(), "stop bounded native material operation");
            platform::cleanup::report(child.wait(), "reap bounded native material operation");
        }
        let read = |path: &Path| -> Result<(Vec<u8>, bool)> {
            let mut bytes = Vec::new();
            File::open(path)?
                .take(PROCESS_BYTES + 1)
                .read_to_end(&mut bytes)?;
            let complete = bytes.len() as u64 <= PROCESS_BYTES;
            Ok((bytes, complete))
        };
        let (stdout, stdout_complete) = read(&stdout)?;
        let (stderr, stderr_complete) = read(&stderr)?;
        let record = NativeMaterialOperation {
            name: name.into(),
            status: status
                .as_ref()
                .ok()
                .and_then(std::process::ExitStatus::code),
            stdout,
            stderr,
            stdout_complete,
            stderr_complete,
        };
        match status {
            Ok(status) if status.success() && stdout_complete && stderr_complete => Ok(record),
            Ok(_) => Err(anyhow::Error::new(NativeMaterialFailure {
                operation: record,
            })),
            Err(error) => {
                Err(anyhow::Error::new(NativeMaterialFailure { operation: record }).context(error))
            }
        }
    })();
    platform::cleanup::report(raw.close(), "remove bounded native operation output");
    result
}

pub(super) fn prepare_output_directory(root: &Path) -> Result<PathBuf> {
    let root = physical(root)?;
    let output = root.join(OPERATION_OUTPUTS);
    fs::create_dir_all(&output)?;
    physical(&output)
}

pub(super) fn physical(path: &Path) -> Result<PathBuf> {
    ensure!(path.is_absolute(), "native material path must be absolute");
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "native material path contains a link"
        );
    }
    Ok(platform::paths::canonical(path)?)
}

pub(super) fn capture(
    identity: &Artifact,
    acquired: VerifiedArtifact,
    destination: &Path,
) -> Result<()> {
    ensure!(
        identity.expected == acquired.expected() && identity.provenance == acquired.provenance(),
        "native material identity or provenance differs"
    );
    let mut input = acquired.into_file();
    ensure!(
        Cache::content(&mut input)? == identity.expected,
        "native material changed after acquisition"
    );
    let mut output = fs::OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .open(destination)?;
    let count = std::io::copy(
        &mut input.take(super::cache::ARTIFACT_BYTE_LIMIT + 1),
        &mut output,
    )?;
    ensure!(
        count <= super::cache::ARTIFACT_BYTE_LIMIT,
        "native material exceeds its acquisition bound"
    );
    output.flush()?;
    ensure!(
        Cache::content(&mut output)? == identity.expected,
        "native material changed during capture"
    );
    Ok(())
}

pub(super) fn tree(root: &Path) -> Result<BTreeMap<String, Digest>> {
    fn visit(
        root: &Path,
        directory: &Path,
        depth: usize,
        bytes: &mut u64,
        entries: &mut BTreeMap<String, Digest>,
    ) -> Result<()> {
        ensure!(
            depth <= TREE_DEPTH,
            "native material tree exceeds its depth bound"
        );
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(
                !metadata.file_type().is_symlink(),
                "native source material tree contains a link"
            );
            if metadata.is_dir() {
                visit(root, &path, depth + 1, bytes, entries)?;
            } else {
                ensure!(
                    metadata.is_file() && entries.len() < TREE_FILES,
                    "native material tree is special or exceeds its file bound"
                );
                *bytes = bytes
                    .checked_add(metadata.len())
                    .context("native material byte count overflows")?;
                ensure!(
                    *bytes <= TREE_BYTES,
                    "native material tree exceeds its byte bound"
                );
                let name = path
                    .strip_prefix(root)?
                    .to_str()
                    .context("native material path requires Unicode")?
                    .replace('\\', "/");
                entries.insert(name, Cache::content(&mut File::open(&path)?)?);
            }
        }
        Ok(())
    }
    physical(root)?;
    let mut entries = BTreeMap::new();
    visit(root, root, 0, &mut 0, &mut entries)?;
    Ok(entries)
}

#[derive(Debug)]
pub(super) struct PreparedCargoTool {
    source: PathBuf,
    files: BTreeMap<String, Digest>,
    workload: CargoWorkload,
    dependencies: PreparedCargo,
    version: String,
}

impl PreparedCargoTool {
    pub(super) fn prepare(
        root: &Path,
        identity: &Artifact,
        acquired: VerifiedArtifact,
        cache: &Cache,
        fetch: &impl Fetch,
    ) -> Result<Self> {
        let source =
            super::consume::unpack_package(root, identity, acquired, &mut 0, CargoBudget::Native)?;
        let manifest: toml::Table =
            toml::from_str(&fs::read_to_string(source.join("Cargo.toml"))?)?;
        let package = manifest
            .get("package")
            .context("Cargo tool has no original package")?;
        let name = package
            .get("name")
            .and_then(toml::Value::as_str)
            .context("Cargo tool has no package name")?;
        let version = package
            .get("version")
            .and_then(toml::Value::as_str)
            .context("Cargo tool has no package version")?;
        ensure!(
            identity.subject == format!("crate:{name}@{version}") && identity.version == version,
            "Cargo tool package differs from the verified release"
        );
        let workload = CargoWorkload::capture(&source)?;
        let dependencies = cache.prepare_cargo(&workload, root, fetch)?;
        let files = tree(&source)?;
        Ok(Self {
            source,
            files,
            workload,
            dependencies,
            version: version.into(),
        })
    }

    pub(super) fn immutable_paths(&self) -> Vec<PathBuf> {
        let mut paths = vec![self.source.clone()];
        paths.extend(self.dependencies.immutable_paths());
        paths
    }

    pub(super) fn current(&self) -> Result<()> {
        ensure!(
            tree(&self.source)? == self.files,
            "Cargo tool source changed after verification"
        );
        self.workload.require_current(&self.source)
    }

    pub(super) fn install(
        &self,
        mut command: Command,
        root: &Path,
        install: &Path,
    ) -> Result<NativeMaterialOperation> {
        self.current()?;
        command
            .args(["install", "--path"])
            .arg(&self.source)
            .arg("--root")
            .arg(install)
            .args(["--locked", "--offline", "--jobs", "2"]);
        self.dependencies.configure(&self.workload, &mut command)?;
        command.env("CARGO_TARGET_DIR", root.join("launcher-build"));
        let result = operation(
            command,
            root,
            "install pinned Kani launcher from verified source",
        )?;
        self.current()?;
        Ok(result)
    }

    pub(super) fn version(&self) -> &str {
        &self.version
    }
}

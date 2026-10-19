use super::cache::ARTIFACT_BYTE_LIMIT;
use super::{Artifact, Cache, CargoWorkload, Digest, Fetch, Plan, VerifiedArtifact, VerifiedPlan};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::process::Command;

const ARCHIVE_BYTE_LIMIT: u64 = 1024 * 1024 * 1024;
const ENTRY_COUNT_LIMIT: usize = 65_536;
const SOURCE_BYTE_LIMIT: u64 = 4 * 1024 * 1024 * 1024;
const CHECKSUM_FILE: &str = ".cargo-checksum.json";
const VENDOR_DIRECTORY: &str = "vendor";
const CARGO_HOME: &str = "cargo-home";
const CONFIG: &str = "config.toml";
const LEGACY_CONFIG: &str = "config";
const PACKAGE_MANIFEST: &str = "Cargo.toml";
const COPY_BUFFER_BYTES: usize = 8192;
const TAR_BLOCK_BYTES: u64 = 512;
const CRATE_PREFIX: &str = "crate:";
const VENDOR_SOURCE: &str = "dotfiles-verified-materials";
pub(super) const ACQUISITION_CACHE: &str = ".cache/dotfiles/artifacts";
const EXTENSION_BYTE_LIMIT: u64 = 64 * 1024;
const PATH_BYTE_LIMIT: usize = 4096;
const PATH_DEPTH_LIMIT: usize = 64;
const PATH_INVENTORY_BYTE_LIMIT: usize = 16 * 1024 * 1024;
const MATERIAL_FILE_COUNT_LIMIT: usize = 262_144;
const MATERIAL_PATH_BYTE_LIMIT: usize = 64 * 1024 * 1024;
const REPLAY_COMPRESSED_BYTES: u64 = 16 * 1024;
const REPLAY_DECODED_BYTES: u64 = 256 * 1024;
const REPLAY_SOURCE_BYTES: u64 = 1024 * 1024;
const REPLAY_ENTRIES: usize = 64;
const REPLAY_PACKAGES: usize = 8;
const REPLAY_EXTENSION_BYTES: u64 = 4096;
#[cfg(unix)]
const UNIX_EXECUTE_BITS: u32 = 0o111;
#[cfg(unix)]
const VENDORED_FILE_MODE: u32 = 0o644;
#[cfg(unix)]
const VENDORED_EXECUTABLE_MODE: u32 = 0o755;

/// Closed native and finite replay bounds for the same material decoder.
#[derive(Clone, Copy, Debug)]
pub enum CargoBudget {
    Native,
    Replay,
}

impl CargoBudget {
    const fn compressed(self) -> u64 {
        match self {
            Self::Native => ARTIFACT_BYTE_LIMIT,
            Self::Replay => REPLAY_COMPRESSED_BYTES,
        }
    }
    const fn decoded(self) -> u64 {
        match self {
            Self::Native => ARCHIVE_BYTE_LIMIT,
            Self::Replay => REPLAY_DECODED_BYTES,
        }
    }
    const fn source(self) -> u64 {
        match self {
            Self::Native => SOURCE_BYTE_LIMIT,
            Self::Replay => REPLAY_SOURCE_BYTES,
        }
    }
    const fn entries(self) -> usize {
        match self {
            Self::Native => ENTRY_COUNT_LIMIT,
            Self::Replay => REPLAY_ENTRIES,
        }
    }
    const fn packages(self) -> usize {
        match self {
            Self::Native => usize::MAX,
            Self::Replay => REPLAY_PACKAGES,
        }
    }
    const fn extension(self) -> u64 {
        match self {
            Self::Native => EXTENSION_BYTE_LIMIT,
            Self::Replay => REPLAY_EXTENSION_BYTES,
        }
    }
}

#[derive(Serialize)]
struct Checksums {
    files: BTreeMap<String, String>,
    package: String,
}

/// Fresh Cargo inputs, without granting a build, installation, or proof result.
#[derive(Debug)]
pub struct PreparedCargo {
    scope: tempfile::TempDir,
    plan: Digest,
    files: BTreeMap<String, Digest>,
    budget: CargoBudget,
}

impl PreparedCargo {
    #[must_use]
    pub const fn digest(&self) -> Digest {
        self.plan
    }

    /// Route a fresh Cargo operation through the verified source directory.
    ///
    /// # Errors
    ///
    /// Refuses changed source or materials before configuring the child process.
    pub fn configure_environment(
        &self,
        workload: &CargoWorkload,
        command: &mut Command,
    ) -> Result<()> {
        workload.require_environment_scope()?;
        self.environment(workload, command)
    }

    fn environment(&self, workload: &CargoWorkload, command: &mut Command) -> Result<()> {
        workload.require_current(workload.root())?;
        ensure!(
            workload.plan()?.digest() == self.plan,
            "Cargo materials belong to another workload"
        );
        ensure!(
            inventory(self.scope.path(), self.budget)? == self.files,
            "prepared Cargo materials changed before consumption"
        );
        command
            .env("CARGO_HOME", self.scope.path().join(CARGO_HOME))
            .env("MISE_CARGO_HOME", self.scope.path().join(CARGO_HOME))
            .env("CARGO_NET_OFFLINE", "true");
        Ok(())
    }

    /// Configure a Cargo operation with both source replacement and frozen resolution.
    ///
    /// # Errors
    ///
    /// Refuses stale inputs or changed materials before configuring Cargo.
    pub fn configure(&self, workload: &CargoWorkload, command: &mut Command) -> Result<()> {
        self.environment(workload, command)?;
        command
            .arg("--config")
            .arg(self.scope.path().join(CARGO_HOME).join(CONFIG))
            .arg("--frozen");
        Ok(())
    }

    #[must_use]
    pub fn immutable_paths(&self) -> [std::path::PathBuf; 2] {
        [
            self.scope.path().join(VENDOR_DIRECTORY),
            self.scope.path().join(CARGO_HOME).join(CONFIG),
        ]
    }

    #[must_use]
    pub fn directory(&self) -> &Path {
        self.scope.path()
    }
}

/// Fetch the current source's Cargo materials and prepare a new isolated consumer.
///
/// # Errors
///
/// Refuses unresolved locks, failed acquisition, unsafe archives, and changed source.
pub fn prepare_source_cargo(
    root: &Path,
    home: &Path,
    parent: &Path,
) -> Result<(CargoWorkload, PreparedCargo)> {
    let workload = CargoWorkload::capture(root)?;
    prepare_cargo_workload(workload, home, parent)
}

/// Consume one complete project and authenticated-component package union.
///
/// # Errors
/// Refuses stale inputs, unsafe archives and incomplete acquisition before creating the vendor.
pub fn prepare_cargo_workload(
    workload: CargoWorkload,
    home: &Path,
    parent: &Path,
) -> Result<(CargoWorkload, PreparedCargo)> {
    workload.require_environment_scope()?;
    let cache = Cache::new(home.join(ACQUISITION_CACHE));
    let prepared = cache.prepare_cargo(&workload, parent, &super::transport::Curl)?;
    Ok((workload, prepared))
}

impl Cache {
    /// Fetch every package and consume one authenticated opened archive at a time.
    ///
    /// # Errors
    ///
    /// Refuses changed inputs or material and removes the candidate on any consumption failure.
    pub fn prepare_cargo(
        &self,
        workload: &CargoWorkload,
        parent: &Path,
        fetcher: &impl Fetch,
    ) -> Result<PreparedCargo> {
        workload.require_current(workload.root())?;
        let plan = workload.plan()?;
        let artifacts = self.acquire_stream(&plan, fetcher)?;
        prepare_cargo(
            workload,
            parent,
            CargoBudget::Native,
            plan.digest(),
            artifacts,
        )
    }
}

impl VerifiedPlan {
    /// Consume reverified package archives into a new owned Cargo directory source.
    ///
    /// # Errors
    ///
    /// Refuses partial plans, changed bytes, unsafe archives, and conflicting packages.
    pub fn prepare_cargo(self, workload: &CargoWorkload, parent: &Path) -> Result<PreparedCargo> {
        self.prepare_cargo_with_budget(workload, parent, CargoBudget::Native)
    }

    /// Consume the same verified inputs under a smaller finite replay budget.
    ///
    /// # Errors
    ///
    /// Preserves native validation and refuses an input or output exceeding the selected bounds.
    pub fn prepare_cargo_with_budget(
        self,
        workload: &CargoWorkload,
        parent: &Path,
        budget: CargoBudget,
    ) -> Result<PreparedCargo> {
        prepare_cargo(
            workload,
            parent,
            budget,
            self.digest(),
            self.into_artifacts().into_iter().map(Ok),
        )
    }
}

fn prepare_cargo(
    workload: &CargoWorkload,
    parent: &Path,
    budget: CargoBudget,
    digest: Digest,
    artifacts: impl ExactSizeIterator<Item = Result<VerifiedArtifact>>,
) -> Result<PreparedCargo> {
    workload.require_current(workload.root())?;
    let plan = workload.plan()?;
    plan.complete()?;
    ensure!(
        digest == plan.digest(),
        "verified acquisition is not this complete Cargo workload"
    );
    ensure!(
        rules::artifact_rules::admission(
            plan.artifacts().len(),
            artifacts.len(),
            plan.diagnostics().len(),
        ) == rules::artifact_rules::Admission::Ready,
        "Cargo acquisition set is incomplete"
    );
    ensure!(
        plan.artifacts().len() <= budget.packages(),
        "Cargo material set exceeds its package budget"
    );
    let metadata = fs::symlink_metadata(parent)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "Cargo material parent must be a real owned directory"
    );
    let parent = platform::paths::canonical(parent)?;
    ensure!(
        !parent.starts_with(workload.root()),
        "Cargo prepared output must stay outside captured source"
    );
    let scope = tempfile::Builder::new()
        .prefix("verified-cargo-")
        .tempdir_in(parent)?;
    let result = prepare(scope.path(), &plan, artifacts, budget);
    match result {
        Ok(files) => Ok(PreparedCargo {
            scope,
            plan: plan.digest(),
            files,
            budget,
        }),
        Err(error) => {
            platform::cleanup::report(scope.close(), "remove refused Cargo materials");
            Err(error)
        }
    }
}

fn prepare(
    root: &Path,
    plan: &Plan,
    mut artifacts: impl Iterator<Item = Result<VerifiedArtifact>>,
    budget: CargoBudget,
) -> Result<BTreeMap<String, Digest>> {
    let vendor = root.join(VENDOR_DIRECTORY);
    fs::create_dir(&vendor)?;
    let mut total = 0;
    let mut packages = BTreeSet::new();
    for identity in plan.artifacts() {
        let acquired = artifacts
            .next()
            .context("Cargo acquisition set ended early")??;
        ensure!(
            identity.expected == acquired.expected()
                && identity.provenance == acquired.provenance(),
            "Cargo artifact identity does not match its planned source"
        );
        let package = identity
            .subject
            .strip_prefix(CRATE_PREFIX)
            .context("Cargo plan contains a non-package material")?;
        let (name, version) = package
            .split_once('@')
            .context("Cargo package identity requires a version")?;
        let directory = format!("{name}-{version}");
        ensure!(
            packages.insert(directory.to_ascii_lowercase()),
            "Cargo package directories collide across platforms"
        );
        unpack_package(root, identity, acquired, &mut total, budget)?;
    }
    if let Some(extra) = artifacts.next() {
        extra?;
        anyhow::bail!("Cargo acquisition set has extra material");
    }
    fs::create_dir(root.join(CARGO_HOME))?;
    let directory = vendor
        .to_str()
        .context("Cargo vendor destination requires Unicode")?;
    let config = toml::toml! {
        [source.crates-io]
        replace-with = VENDOR_SOURCE
        [source.dotfiles-verified-materials]
        directory = directory
        [net]
        offline = true
    };
    fs::write(
        root.join(CARGO_HOME).join(CONFIG),
        toml::to_string(&config)?,
    )?;
    inventory(root, budget)
}

pub(super) fn unpack_package(
    root: &Path,
    identity: &Artifact,
    acquired: VerifiedArtifact,
    total: &mut u64,
    budget: CargoBudget,
) -> Result<std::path::PathBuf> {
    ensure!(
        identity.expected == acquired.expected() && identity.provenance == acquired.provenance(),
        "Cargo artifact identity does not match its planned source"
    );
    let package = identity
        .subject
        .strip_prefix(CRATE_PREFIX)
        .context("Cargo source material requires a package identity")?;
    let (name, version) = package
        .split_once('@')
        .context("Cargo package identity requires a version")?;
    let directory = format!("{name}-{version}");
    let vendor = root.join(VENDOR_DIRECTORY);
    fs::create_dir_all(&vendor)?;
    let mut file = acquired.into_file();
    ensure!(
        Cache::content(&mut file)? == identity.expected,
        "Cargo archive changed after verification"
    );
    let mut captured = tempfile::NamedTempFile::new_in(root)?;
    let captured_result = (|| {
        let count = std::io::copy(
            &mut file.take(budget.compressed() + 1),
            captured.as_file_mut(),
        )?;
        ensure!(
            count <= budget.compressed(),
            "Cargo archive grew beyond its acquisition bound"
        );
        ensure!(
            Cache::content(captured.as_file_mut())? == identity.expected,
            "Cargo archive changed while capturing its consumption bytes"
        );
        Ok::<_, anyhow::Error>(())
    })();
    if let Err(error) = captured_result {
        platform::cleanup::report(captured.close(), "remove refused Cargo archive bytes");
        return Err(error);
    }
    let mut unpacked = tempfile::NamedTempFile::new_in(root)?;
    let result = decode(captured.as_file_mut(), unpacked.as_file_mut(), budget).and_then(|()| {
        extract(
            unpacked.as_file_mut(),
            &vendor,
            &directory,
            identity.expected,
            total,
            budget,
        )
    });
    platform::cleanup::report(captured.close(), "remove captured Cargo archive bytes");
    platform::cleanup::report(unpacked.close(), "remove the decoded Cargo archive");
    result?;
    Ok(vendor.join(directory))
}

fn decode(input: &mut File, destination: &mut File, budget: CargoBudget) -> Result<()> {
    ensure!(
        input.metadata()?.len() <= budget.compressed(),
        "Cargo archive exceeds its acquisition bound"
    );
    let mut gzip = flate2::bufread::GzDecoder::new(BufReader::new(input));
    let bytes = std::io::copy(&mut (&mut gzip).take(budget.decoded() + 1), destination)?;
    ensure!(
        bytes <= budget.decoded(),
        "Cargo archive exceeds its decoded byte bound"
    );
    let mut tail = gzip.into_inner();
    ensure!(
        tail.fill_buf()?.is_empty(),
        "Cargo archive contains trailing data or another gzip member"
    );
    destination.seek(SeekFrom::Start(0))?;
    Ok(())
}

fn extract(
    input: &mut File,
    vendor: &Path,
    directory: &str,
    expected: Digest,
    total: &mut u64,
    budget: CargoBudget,
) -> Result<()> {
    validate_headers(input, budget)?;
    let package = vendor.join(directory);
    fs::create_dir(&package)?;
    let mut archive = tar::Archive::new(input);
    let mut seen = BTreeSet::new();
    let mut prefixes = BTreeMap::new();
    let mut files = BTreeMap::new();
    let mut path_bytes = 0;
    for entry in archive.entries()? {
        let mut entry = entry?;
        ensure!(
            seen.len() < budget.entries(),
            "Cargo archive exceeds its entry bound"
        );
        let kind = entry.header().entry_type();
        ensure!(
            kind.is_file() || kind.is_dir(),
            "Cargo archive contains a link or special entry"
        );
        let relative = entry_path(&entry, directory)?;
        record_prefixes(&relative, &mut prefixes, &mut path_bytes)?;
        ensure!(
            seen.insert(relative.to_ascii_lowercase()),
            "Cargo archive contains duplicate or case-colliding paths"
        );
        if relative.is_empty() {
            continue;
        }
        if let Some(digest) = write_entry(&mut entry, &package.join(&relative), total, budget)? {
            files.insert(relative, digest.hex());
        }
    }
    let tail = archive.into_inner();
    ensure!(
        tail.metadata()?.len() % TAR_BLOCK_BYTES == 0,
        "Cargo tar stream has incomplete padding"
    );
    let mut bytes = [0; COPY_BUFFER_BYTES];
    loop {
        let count = tail.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        ensure!(
            bytes[..count].iter().all(|byte| *byte == 0),
            "Cargo tar stream has trailing entries or data"
        );
    }
    write_checksums(&package, directory, expected, files)
}

fn entry_path(entry: &tar::Entry<'_, impl Read>, directory: &str) -> Result<String> {
    let kind = entry.header().entry_type();
    ensure!(
        !kind.is_dir() || entry.size() == 0,
        "Cargo directory entry contains data"
    );
    let bytes = entry.path_bytes();
    ensure!(
        bytes.len() <= PATH_BYTE_LIMIT,
        "Cargo archive path exceeds its byte bound"
    );
    let raw = std::str::from_utf8(&bytes).context("Cargo archive path requires Unicode")?;
    let raw = if kind.is_dir() {
        raw.strip_suffix('/').unwrap_or(raw)
    } else {
        raw
    };
    let mut parts = raw.split('/');
    ensure!(
        parts.next() == Some(directory),
        "Cargo archive root differs from its pinned package"
    );
    let relative = parts.collect::<Vec<_>>();
    ensure!(
        relative.len() <= PATH_DEPTH_LIMIT,
        "Cargo archive path exceeds its depth bound"
    );
    let relative = relative.join("/");
    ensure!(
        !relative.is_empty() || kind.is_dir(),
        "Cargo archive root must be a directory"
    );
    ensure!(
        relative.is_empty()
            || relative
                .split('/')
                .all(|part| rules::artifact_rules::portable_component(part.as_bytes())),
        "Cargo archive path is not portable or leaves its package"
    );
    ensure!(
        !relative.eq_ignore_ascii_case(CHECKSUM_FILE),
        "Cargo archive supplies its own directory-source checksum"
    );
    Ok(relative)
}

fn record_prefixes(
    relative: &str,
    prefixes: &mut BTreeMap<String, String>,
    path_bytes: &mut usize,
) -> Result<()> {
    let mut prefix = String::new();
    for part in relative.split('/').filter(|part| !part.is_empty()) {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(part);
        match prefixes.entry(prefix.to_ascii_lowercase()) {
            std::collections::btree_map::Entry::Occupied(previous) => {
                ensure!(
                    previous.get() == &prefix,
                    "Cargo archive contains case-colliding path components"
                );
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                *path_bytes = path_bytes
                    .checked_add(prefix.len())
                    .context("Cargo path inventory overflow")?;
                ensure!(
                    *path_bytes <= PATH_INVENTORY_BYTE_LIMIT,
                    "Cargo path inventory exceeds its byte bound"
                );
                entry.insert(prefix.clone());
            }
        }
    }
    Ok(())
}

fn write_entry(
    entry: &mut tar::Entry<'_, impl Read>,
    destination: &Path,
    total: &mut u64,
    budget: CargoBudget,
) -> Result<Option<Digest>> {
    if entry.header().entry_type().is_dir() {
        fs::create_dir_all(destination)?;
        return Ok(None);
    }
    let size = entry.size();
    *total = total
        .checked_add(size)
        .context("Cargo material byte count overflow")?;
    ensure!(
        *total <= budget.source(),
        "Cargo source material exceeds its total byte bound"
    );
    fs::create_dir_all(destination.parent().context("Cargo file parent")?)?;
    let mut output = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(destination)?;
    ensure!(
        std::io::copy(&mut *entry, &mut output)? == size,
        "Cargo archive entry is truncated"
    );
    output.flush()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = if entry.header().mode()? & UNIX_EXECUTE_BITS == 0 {
            VENDORED_FILE_MODE
        } else {
            VENDORED_EXECUTABLE_MODE
        };
        fs::set_permissions(destination, fs::Permissions::from_mode(mode))?;
    }
    Ok(Some(file_digest(&mut output)?))
}

fn write_checksums(
    package: &Path,
    directory: &str,
    expected: Digest,
    files: BTreeMap<String, String>,
) -> Result<()> {
    let text = fs::read_to_string(package.join(PACKAGE_MANIFEST))
        .context("Cargo package manifest is absent")?;
    let manifest: toml::Table = toml::from_str(&text)?;
    let name = manifest
        .get("package")
        .and_then(|value| value.get("name"))
        .and_then(toml::Value::as_str)
        .context("Cargo package name is absent")?;
    let version = manifest
        .get("package")
        .and_then(|value| value.get("version"))
        .and_then(toml::Value::as_str)
        .context("Cargo package version is absent")?;
    ensure!(
        format!("{name}-{version}") == directory,
        "Cargo archive manifest differs from the locked package"
    );
    fs::write(
        package.join(CHECKSUM_FILE),
        serde_json::to_vec(&Checksums {
            files,
            package: expected.hex(),
        })?,
    )?;
    Ok(())
}

fn validate_headers(input: &mut File, budget: CargoBudget) -> Result<()> {
    {
        let mut archive = tar::Archive::new(&mut *input);
        for (count, entry) in archive.entries_with_seek()?.raw(true).enumerate() {
            ensure!(
                count < budget.entries(),
                "Cargo archive exceeds its physical header bound"
            );
            let entry = entry?;
            let kind = entry.header().entry_type();
            let extension = kind.is_gnu_longname() || kind.is_pax_local_extensions();
            ensure!(
                kind.is_file() || kind.is_dir() || extension,
                "Cargo archive contains a link or special header"
            );
            if extension {
                ensure!(
                    entry.size() <= budget.extension(),
                    "Cargo archive path extension exceeds its byte bound"
                );
            }
        }
    }
    input.seek(SeekFrom::Start(0))?;
    Ok(())
}

fn file_digest(file: &mut File) -> Result<Digest> {
    use sha2::{Digest as _, Sha256};
    file.seek(SeekFrom::Start(0))?;
    let mut hasher = Sha256::new();
    let mut bytes = [0; COPY_BUFFER_BYTES];
    loop {
        let count = file.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        hasher.update(&bytes[..count]);
    }
    Digest::try_from(platform::build_inputs::hex(&hasher.finalize())).map_err(anyhow::Error::msg)
}

fn inventory(root: &Path, budget: CargoBudget) -> Result<BTreeMap<String, Digest>> {
    fn visit(
        root: &Path,
        path: &Path,
        files: &mut BTreeMap<String, Digest>,
        bytes: &mut u64,
        paths: &mut usize,
        depth: usize,
        budget: CargoBudget,
    ) -> Result<()> {
        ensure!(
            depth <= PATH_DEPTH_LIMIT + 1,
            "Cargo material inventory exceeds its depth bound"
        );
        let metadata = fs::symlink_metadata(path)?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "Cargo material directory is not a real directory"
        );
        for item in fs::read_dir(path)? {
            let item = item?;
            let metadata = fs::symlink_metadata(item.path())?;
            ensure!(
                !metadata.file_type().is_symlink(),
                "prepared Cargo source contains a link"
            );
            let relative = item
                .path()
                .strip_prefix(root)?
                .to_str()
                .context("Cargo material path requires Unicode")?
                .replace('\\', "/");
            *paths = paths
                .checked_add(relative.len())
                .context("Cargo material inventory path overflow")?;
            ensure!(
                *paths <= MATERIAL_PATH_BYTE_LIMIT,
                "Cargo material inventory exceeds its path byte bound"
            );
            if metadata.is_dir() {
                visit(root, &item.path(), files, bytes, paths, depth + 1, budget)?;
            } else {
                ensure!(
                    metadata.is_file(),
                    "prepared Cargo source contains a special file"
                );
                *bytes = bytes
                    .checked_add(metadata.len())
                    .context("Cargo material inventory byte overflow")?;
                ensure!(
                    *bytes <= budget.source(),
                    "Cargo material inventory exceeds its byte bound"
                );
                ensure!(
                    files.len() < MATERIAL_FILE_COUNT_LIMIT,
                    "Cargo material inventory exceeds its file bound"
                );
                files.insert(relative, file_digest(&mut File::open(item.path())?)?);
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    visit(
        root,
        &root.join(VENDOR_DIRECTORY),
        &mut files,
        &mut 0,
        &mut 0,
        0,
        budget,
    )?;
    let home = fs::symlink_metadata(root.join(CARGO_HOME))?;
    ensure!(
        home.is_dir() && !home.file_type().is_symlink(),
        "prepared Cargo home is not a real owned directory"
    );
    match fs::symlink_metadata(root.join(CARGO_HOME).join(LEGACY_CONFIG)) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("inspect the prepared Cargo configuration"),
        Ok(_) => anyhow::bail!("prepared Cargo home contains an alternate legacy configuration"),
    }
    let config = Path::new(CARGO_HOME).join(CONFIG);
    let metadata = fs::symlink_metadata(root.join(&config))?;
    ensure!(
        metadata.is_file(),
        "prepared Cargo configuration is not a regular file"
    );
    files.insert(
        config.to_string_lossy().replace('\\', "/"),
        file_digest(&mut File::open(root.join(config))?)?,
    );
    Ok(files)
}

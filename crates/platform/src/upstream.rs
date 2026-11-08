use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const FORMAT: u32 = 1;
const MAX_ARCHIVE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 64 * 1024 * 1024;
const MAX_MEMBER_BYTES: u64 = 4 * 1024 * 1024;
const MAX_MEMBERS: usize = 4096;
const DIGEST_DIGITS: usize = 64;
const MAX_MANIFEST_BYTES: usize = 64 * 1024;
const MAX_REGISTRY_BYTES: usize = 16 * 1024 * 1024;
const HTTPS_PREFIX: &str = "https://";
const HTTPS_SCHEME: &str = "https";

fn valid_origin(value: &str) -> bool {
    value.starts_with(HTTPS_PREFIX)
        && !value.contains(['\n', '\r'])
        && url::Url::parse(value)
            .is_ok_and(|url| url.scheme() == HTTPS_SCHEME && url.host_str().is_some())
}

fn matches_digest(bytes: &[u8], expected: &str) -> bool {
    crate::build_inputs::hex(&Sha256::digest(bytes)).eq_ignore_ascii_case(expected)
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Preserved {
    pub repository: PathBuf,
    pub directory: PathBuf,
    pub manifest: PathBuf,
    pub sha256: String,
    pub origin: String,
    pub files: BTreeMap<String, String>,
}

impl Preserved {
    /// # Errors
    /// Rejects invalid identities, paths, or member metadata.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.repository.is_absolute() && relative(&self.directory) && relative(&self.manifest),
            "preserved source paths are invalid"
        );
        ensure!(
            digest_valid(&self.sha256) && !self.files.is_empty() && valid_origin(&self.origin),
            "preserved source requires SHA-256, files, and an HTTPS origin"
        );
        for (path, digest) in &self.files {
            ensure!(
                relative(Path::new(path)) && Path::new(path).starts_with(&self.directory),
                "preserved member leaves its directory"
            );
            ensure!(digest_valid(digest), "preserved member requires SHA-256");
        }
        Ok(())
    }

    /// # Errors
    /// Rejects changed manifests or invalid publisher metadata.
    pub fn verify_manifest(&self, bytes: &[u8]) -> Result<()> {
        self.validate()?;
        ensure!(
            bytes.len() <= MAX_MANIFEST_BYTES && matches_digest(bytes, &self.sha256),
            "preserved source manifest identity changed"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub repository: PathBuf,
    pub directory: PathBuf,
    pub archive: PathBuf,
    pub prefix: PathBuf,
    pub sha256: String,
    pub origin: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    version: u32,
    sources: Vec<Source>,
    #[serde(default)]
    preserved: Vec<Preserved>,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            version: FORMAT,
            sources: Vec::new(),
            preserved: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Registrations(Registry);

fn digest_valid(digest: &str) -> bool {
    digest.len() == DIGEST_DIGITS && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_registry(registry: &Registry) -> Result<()> {
    ensure!(
        registry.version == FORMAT,
        "unsupported upstream registry format"
    );
    let mut directories = Vec::new();
    for source in &registry.sources {
        source.validate()?;
        directories.push((&source.repository, &source.directory));
    }
    for source in &registry.preserved {
        source.validate()?;
        directories.push((&source.repository, &source.directory));
    }
    directories.sort_unstable();
    for (left, right) in directories.iter().zip(directories.iter().skip(1)) {
        ensure!(
            left.0 != right.0 || !right.1.starts_with(left.1),
            "upstream source registrations overlap"
        );
    }
    Ok(())
}

/// # Errors
/// Rejects malformed metadata, unsupported formats, and overlapping ownership.
pub fn parse(bytes: &[u8]) -> Result<Registrations> {
    ensure!(
        bytes.len() <= MAX_REGISTRY_BYTES,
        "upstream registry exceeds its byte budget"
    );
    let registry: Registry = serde_json::from_slice(bytes)?;
    validate_registry(&registry)?;
    Ok(Registrations(registry))
}

fn relative(path: &Path) -> bool {
    path.to_str()
        .is_some_and(|path| rules::third_party_rules::normalized_relative(path.as_bytes()))
}

impl Source {
    /// # Errors
    /// Rejects invalid paths, digests, or origins.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.repository.is_absolute(),
            "upstream repository identity must be absolute"
        );
        ensure!(
            relative(&self.directory) && relative(&self.archive) && relative(&self.prefix),
            "upstream paths must be normalized relative paths"
        );
        ensure!(
            self.archive.starts_with(&self.directory),
            "upstream archive must be inside its source directory"
        );
        ensure!(
            digest_valid(&self.sha256),
            "upstream archive requires a SHA-256 identity"
        );
        ensure!(
            valid_origin(&self.origin),
            "upstream origin must be an HTTPS URL"
        );
        Ok(())
    }

    /// # Errors
    /// Rejects changed archives, unsafe members, duplicates, and exceeded limits.
    pub fn originals(&self, bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>> {
        self.validate()?;
        ensure!(
            u64::try_from(bytes.len())? <= MAX_ARCHIVE_BYTES,
            "upstream archive exceeds its byte budget"
        );
        ensure!(
            matches_digest(bytes, &self.sha256),
            "upstream archive identity changed"
        );
        let decoded = flate2::read::GzDecoder::new(bytes).take(MAX_EXPANDED_BYTES);
        let mut archive = tar::Archive::new(decoded);
        let mut originals = BTreeMap::new();
        let mut total = 0u64;
        for (index, entry) in archive.entries()?.enumerate() {
            ensure!(
                index < MAX_MEMBERS,
                "upstream archive exceeds its member budget"
            );
            let mut entry = entry?;
            let path = entry.path()?.into_owned();
            ensure!(relative(&path), "upstream member path is not normalized");
            let suffix = path
                .strip_prefix(&self.prefix)
                .context("upstream member belongs to another package")?;
            if entry.header().entry_type().is_dir() {
                continue;
            }
            ensure!(
                entry.header().entry_type().is_file() && relative(suffix),
                "upstream members must be regular files"
            );
            let size = entry.size();
            ensure!(
                size <= MAX_MEMBER_BYTES,
                "upstream member exceeds its byte budget"
            );
            total = total
                .checked_add(size)
                .context("upstream archive size overflow")?;
            ensure!(
                total <= MAX_EXPANDED_BYTES,
                "upstream archive exceeds its expanded byte budget"
            );
            let target = self
                .directory
                .join(suffix)
                .to_str()
                .context("upstream member path is not UTF-8")?
                .replace('\\', "/");
            let mut content = Vec::new();
            entry.read_to_end(&mut content)?;
            ensure!(
                u64::try_from(content.len())? == size,
                "upstream member is truncated"
            );
            ensure!(
                originals.insert(target, content).is_none(),
                "upstream archive contains duplicate members"
            );
        }
        let mut decoded = archive.into_inner();
        std::io::copy(&mut decoded, &mut std::io::sink())?;
        ensure!(
            decoded.limit() > 0,
            "upstream archive exhausted its decoding budget"
        );
        ensure!(
            !originals.is_empty(),
            "upstream archive has no regular members"
        );
        Ok(originals)
    }
}

fn registry_path() -> Result<PathBuf> {
    Ok(crate::home::home()
        .context("upstream registry requires a home directory")?
        .join(".config/dotfiles/upstream.json"))
}

/// # Errors
/// Rejects an unavailable Git identity.
pub fn repository(root: &Path) -> Result<PathBuf> {
    repository_identity(root)?.context("cannot identify the upstream source repository")
}

fn repository_identity(root: &Path) -> Result<Option<PathBuf>> {
    let output = crate::tool::Tool::Git
        .command()
        .current_dir(root)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    let path = PathBuf::from(String::from_utf8(output.stdout)?.trim());
    Ok(Some(crate::paths::canonical(&path)?))
}

/// # Errors
/// Rejects malformed owner registrations or unreadable source identities.
pub fn sources(root: &Path) -> Result<Vec<Source>> {
    Ok(registry(root)?.sources)
}

fn registry(root: &Path) -> Result<Registry> {
    let path = registry_path()?;
    if !path.try_exists()? {
        return Ok(Registry::default());
    }
    let mut registry = parse(&std::fs::read(&path)?)?.0;
    let Some(identity) = repository_identity(root)? else {
        return Ok(Registry::default());
    };
    registry
        .sources
        .retain(|source| source.repository == identity);
    registry
        .preserved
        .retain(|source| source.repository == identity);
    Ok(registry)
}

/// # Errors
/// Rejects malformed owner registrations or unreadable source identities.
pub fn preserved_sources(root: &Path) -> Result<Vec<Preserved>> {
    Ok(registry(root)?.preserved)
}

/// # Errors
/// Rejects invalid evidence, overlapping registrations, or storage errors.
pub fn register(root: &Path, source: Source) -> Result<()> {
    source.validate()?;
    ensure!(
        source.repository == repository(root)?,
        "upstream registration belongs to another repository"
    );
    source.originals(&std::fs::read(root.join(&source.archive))?)?;
    let path = registry_path()?;
    update_registry(&path, |registry| {
        registry.sources.retain(|entry| {
            !(entry.repository == source.repository && entry.directory == source.directory)
        });
        registry.sources.push(source);
        Ok(())
    })
}

fn update_registry(path: &Path, edit: impl FnOnce(&mut Registry) -> Result<()>) -> Result<()> {
    let parent = path
        .parent()
        .context("upstream registry has no directory")?;
    std::fs::create_dir_all(parent)?;
    let _lock = RegistryLock::acquire(path)?;
    let mut registry = if path.try_exists()? {
        parse(&std::fs::read(path)?)?.0
    } else {
        Registry::default()
    };
    edit(&mut registry)?;
    validate_registry(&registry)?;
    let mut candidate = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(candidate.as_file_mut(), &registry)?;
    candidate.as_file_mut().write_all(b"\n")?;
    candidate.as_file().sync_all()?;
    candidate.persist(path)?;
    Ok(())
}

struct RegistryLock(std::fs::File);

impl RegistryLock {
    fn acquire(path: &Path) -> Result<Self> {
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path.with_extension("lock"))?;
        lock.lock()?;
        Ok(Self(lock))
    }
}

impl Drop for RegistryLock {
    fn drop(&mut self) {
        crate::cleanup::report(self.0.unlock(), "unlock the upstream registry");
    }
}

/// # Errors
/// Rejects changed evidence and unreadable publisher sources.
pub fn original(root: &Path, relative_path: &str) -> Result<Option<Vec<u8>>> {
    original_with(root, relative_path, |path| {
        Ok(std::fs::read(root.join(path))?)
    })
}

/// # Errors
/// Rejects changed evidence or a failed snapshot read.
pub fn original_with(
    root: &Path,
    relative_path: &str,
    mut read: impl FnMut(&Path) -> Result<Vec<u8>>,
) -> Result<Option<Vec<u8>>> {
    for source in sources(root)? {
        if Path::new(relative_path).starts_with(&source.directory) {
            return Ok(source
                .originals(&read(&source.archive)?)?
                .remove(relative_path));
        }
    }
    for source in preserved_sources(root)? {
        if let Some(expected) = source.files.get(relative_path) {
            source.verify_manifest(&read(&source.manifest)?)?;
            let bytes = read(Path::new(relative_path))?;
            ensure!(
                matches_digest(&bytes, expected),
                "preserved member identity changed: {relative_path}"
            );
            return Ok(Some(bytes));
        }
    }
    Ok(None)
}

/// # Errors
/// Rejects invalid checksum manifests, paths, digests, or duplicate members.
pub fn preserved_members(
    directory: &Path,
    manifest: &Path,
    bytes: &[u8],
) -> Result<BTreeMap<String, String>> {
    ensure!(
        relative(directory) && relative(manifest),
        "preserved registration requires relative paths"
    );
    ensure!(
        bytes.len() <= MAX_MANIFEST_BYTES,
        "preserved source manifest exceeds its byte budget"
    );
    let mut files = BTreeMap::new();
    for line in std::str::from_utf8(bytes)?.lines() {
        let (digest, member) = line
            .split_once("  ")
            .context("preserved manifest requires SHA-256 and two spaces before each path")?;
        ensure!(digest_valid(digest), "preserved member requires SHA-256");
        ensure!(
            relative(Path::new(member)),
            "preserved manifest member leaves its directory"
        );
        let path = manifest
            .parent()
            .context("preserved manifest has no parent")?
            .join(member);
        ensure!(
            path.starts_with(directory),
            "preserved member must be inside its directory"
        );
        ensure!(
            files
                .insert(
                    path.to_str()
                        .context("preserved path is not UTF-8")?
                        .replace('\\', "/"),
                    digest.to_owned()
                )
                .is_none(),
            "preserved manifest contains duplicate paths"
        );
    }
    ensure!(!files.is_empty(), "preserved manifest has no members");
    Ok(files)
}

/// # Errors
/// Rejects invalid manifests, changed files, or registry write failures.
pub fn register_preserved(
    root: &Path,
    directory: PathBuf,
    manifest: PathBuf,
    origin: String,
) -> Result<()> {
    ensure!(
        relative(&directory) && relative(&manifest),
        "preserved registration requires relative paths"
    );
    let bytes = std::fs::read(root.join(&manifest))?;
    let files = preserved_members(&directory, &manifest, &bytes)?;
    for (path, digest) in &files {
        let path = root.join(path);
        ensure!(
            std::fs::symlink_metadata(&path)?.is_file(),
            "preserved member must be a regular file inside its directory"
        );
        ensure!(
            matches_digest(&std::fs::read(&path)?, digest),
            "preserved member identity changed: {}",
            path.display()
        );
    }
    let source = Preserved {
        repository: repository(root)?,
        directory,
        manifest,
        sha256: crate::build_inputs::hex(&Sha256::digest(&bytes)),
        origin,
        files,
    };
    source.verify_manifest(&bytes)?;
    let path = registry_path()?;
    update_registry(&path, |registry| {
        registry.preserved.retain(|entry| {
            !(entry.repository == source.repository && entry.directory == source.directory)
        });
        registry.preserved.push(source);
        Ok(())
    })
}

pub fn changed_findings<'a>(
    baseline: impl IntoIterator<Item = &'a [u8]>,
    current: impl IntoIterator<Item = &'a [u8]>,
) -> Vec<&'a [u8]> {
    let mut remaining = BTreeMap::<&[u8], usize>::new();
    for key in baseline {
        *remaining.entry(key).or_default() += 1;
    }
    current
        .into_iter()
        .filter(|key| {
            if let Some(count) = remaining.get_mut(key)
                && rules::third_party_rules::consume(count)
            {
                return false;
            }
            true
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive() -> (Source, Vec<u8>) {
        let mut archive = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        ));
        let content = b"pub fn publisher() {}\n";
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive
            .append_data(
                &mut header,
                "publisher-1.0.0/src/lib.rs",
                content.as_slice(),
            )
            .unwrap();
        let bytes = archive.into_inner().unwrap().finish().unwrap();
        let source = Source {
            repository: std::env::current_dir().unwrap(),
            directory: "vendor/publisher".into(),
            archive: "vendor/publisher/upstream.crate".into(),
            prefix: "publisher-1.0.0".into(),
            sha256: crate::build_inputs::hex(&Sha256::digest(&bytes)),
            origin: "https://crates.io/crates/publisher/1.0.0".into(),
        };
        (source, bytes)
    }

    #[test]
    fn archive_identity_and_members_bound_the_publisher_baseline() {
        let (source, bytes) = archive();
        assert_eq!(
            source.originals(&bytes).unwrap()["vendor/publisher/src/lib.rs"],
            b"pub fn publisher() {}\n"
        );
        let mut changed = bytes.clone();
        changed[0] ^= 1;
        assert!(source.originals(&changed).is_err());
        let mut invalid = source.clone();
        invalid.prefix = "another-package".into();
        assert!(invalid.originals(&bytes).is_err());
        for path in [
            "",
            "../escape",
            "/absolute",
            "a/../escape",
            "a\\escape",
            "a//b",
            "a/./b",
            "a/b/",
            "c:relative",
        ] {
            assert!(!relative(Path::new(path)), "{path}");
            let mut invalid = source.clone();
            invalid.directory = path.into();
            assert!(invalid.validate().is_err(), "{path}");
        }
    }

    #[test]
    fn registrations_require_parseable_https_origins() {
        let (mut source, _) = archive();
        let mut preserved = Preserved {
            repository: source.repository.clone(),
            directory: source.directory.clone(),
            manifest: source.directory.join("SHA256SUMS"),
            sha256: source.sha256.clone(),
            origin: source.origin.clone(),
            files: BTreeMap::from([("vendor/publisher/src/lib.rs".into(), source.sha256.clone())]),
        };
        source.validate().unwrap();
        preserved.validate().unwrap();
        for origin in [
            "https://",
            "https:// ",
            "https://[invalid",
            "https://example.invalid:invalid",
            "http://example.invalid",
            "https://example.invalid\n",
        ] {
            source.origin = origin.into();
            preserved.origin = origin.into();
            assert!(source.validate().is_err(), "{origin:?}");
            assert!(preserved.validate().is_err(), "{origin:?}");
        }
    }

    #[test]
    fn manifest_digest_identity_accepts_both_hexadecimal_cases() {
        let (source, _) = archive();
        let bytes = b"Reviewed checksum manifest.\n";
        let digest = crate::build_inputs::hex(&Sha256::digest(bytes));
        let mut preserved = Preserved {
            repository: source.repository,
            directory: source.directory.clone(),
            manifest: source.directory.join("SHA256SUMS"),
            sha256: digest.to_ascii_uppercase(),
            origin: source.origin,
            files: BTreeMap::from([("vendor/publisher/src/lib.rs".into(), digest)]),
        };
        preserved.verify_manifest(bytes).unwrap();
        preserved.sha256 = "0".repeat(DIGEST_DIGITS);
        assert!(preserved.verify_manifest(bytes).is_err());
    }

    #[test]
    fn only_existing_occurrences_can_inherit_upstream_findings() {
        assert_eq!(
            changed_findings([b"existing".as_slice()], [b"existing".as_slice()]),
            [] as [&[u8]; 0]
        );
        assert_eq!(
            changed_findings(
                [b"existing".as_slice()],
                [
                    b"existing".as_slice(),
                    b"existing".as_slice(),
                    b"authored".as_slice()
                ]
            ),
            [b"existing".as_slice(), b"authored".as_slice()]
        );
        assert_eq!(
            changed_findings([], [b"authored".as_slice()]),
            [b"authored".as_slice()]
        );
        assert_eq!(
            changed_findings([b"existing".as_slice()], []),
            [] as [&[u8]; 0]
        );
    }

    #[test]
    fn checksum_manifests_require_unique_bounded_members_inside_the_registered_directory() {
        let directory = Path::new("vendor/publisher");
        let manifest = directory.join("SHA256SUMS");
        let digest = crate::build_inputs::hex(&Sha256::digest(b""));
        let valid = format!("{digest}  src/empty.rs\n");
        assert_eq!(
            preserved_members(directory, &manifest, valid.as_bytes()).unwrap(),
            BTreeMap::from([("vendor/publisher/src/empty.rs".into(), digest.clone())])
        );
        for invalid in [
            String::new(),
            format!("{digest} src/empty.rs\n"),
            format!("{digest}  ../escape\n"),
            format!("{digest}  /absolute\n"),
            format!("{valid}{valid}"),
            "bad  src/empty.rs\n".into(),
            format!("{}  src/empty.rs\n", "z".repeat(DIGEST_DIGITS)),
        ] {
            assert!(
                preserved_members(directory, &manifest, invalid.as_bytes()).is_err(),
                "{invalid:?}"
            );
        }
        assert!(preserved_members(directory, &manifest, &[u8::MAX]).is_err());
        assert!(
            preserved_members(directory, &manifest, &vec![b' '; MAX_MANIFEST_BYTES + 1]).is_err()
        );
        assert!(preserved_members(directory, Path::new("SHA256SUMS"), valid.as_bytes()).is_err());
        assert!(preserved_members(Path::new("../escape"), &manifest, valid.as_bytes()).is_err());
        for size in [
            MAX_MANIFEST_BYTES - 1,
            MAX_MANIFEST_BYTES,
            MAX_MANIFEST_BYTES + 1,
        ] {
            let overhead = format!("{digest}  \n").len();
            let record = format!("{digest}  {}\n", "a".repeat(size - overhead));
            assert_eq!(record.len(), size);
            assert_eq!(
                preserved_members(directory, &manifest, record.as_bytes()).is_ok(),
                size <= MAX_MANIFEST_BYTES
            );
        }
        for (directory, manifest) in [
            ("vendor//publisher", "vendor/publisher/SHA256SUMS"),
            ("vendor/publisher", "vendor//publisher/SHA256SUMS"),
            ("vendor/./publisher", "vendor/publisher/SHA256SUMS"),
            ("vendor/publisher", "vendor/./publisher/SHA256SUMS"),
        ] {
            assert!(
                preserved_members(Path::new(directory), Path::new(manifest), valid.as_bytes())
                    .is_err()
            );
        }
    }

    #[test]
    fn registrations_round_trip_and_reject_every_overlapping_owner_family() {
        let (source, _) = archive();
        let preserved = Preserved {
            repository: source.repository.clone(),
            directory: source.directory.join("src"),
            manifest: "sources.sha256".into(),
            sha256: source.sha256.clone(),
            origin: source.origin.clone(),
            files: BTreeMap::from([("vendor/publisher/src/lib.rs".into(), source.sha256.clone())]),
        };
        let mut registry = Registry {
            sources: vec![source.clone()],
            ..Registry::default()
        };
        let checked = parse(&serde_json::to_vec(&registry).unwrap()).unwrap();
        assert_eq!(
            parse(&serde_json::to_vec(&checked).unwrap()).unwrap(),
            checked
        );
        registry.preserved.push(preserved.clone());
        assert!(parse(&serde_json::to_vec(&registry).unwrap()).is_err());
        registry.sources.clear();
        registry.preserved.push(preserved);
        assert!(parse(&serde_json::to_vec(&registry).unwrap()).is_err());
        registry.preserved.clear();
        registry.sources = vec![source.clone(), source.clone()];
        assert!(parse(&serde_json::to_vec(&registry).unwrap()).is_err());
        registry
            .sources
            .last_mut()
            .unwrap()
            .repository
            .push("another-repository");
        assert!(parse(&serde_json::to_vec(&registry).unwrap()).is_ok());
        for bytes in [
            b"{}".as_slice(),
            br#"{"version":0,"sources":[]}"#,
            br#"{"version":1,"sources":[],"extra":true}"#,
        ] {
            assert!(parse(bytes).is_err());
        }
    }

    #[test]
    fn registry_updates_hold_the_process_lock_and_preserve_valid_state_on_failure() {
        let scope = tempfile::tempdir().unwrap();
        let path = scope.path().join("upstream.json");
        let (source, _) = archive();
        update_registry(&path, |registry| {
            let probe = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(path.with_extension("lock"))?;
            assert!(matches!(
                probe.try_lock(),
                Err(std::fs::TryLockError::WouldBlock)
            ));
            registry.sources.push(source);
            Ok(())
        })
        .unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(parse(&before).is_ok());
        let failed = update_registry(&path, |registry| {
            registry.sources.clear();
            anyhow::bail!("injected update failure")
        });
        assert!(failed.unwrap_err().to_string().contains("injected"));
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(
            update_registry(&path, |registry| {
                registry.version = 0;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let probe = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path.with_extension("lock"))
            .unwrap();
        assert!(probe.try_lock().is_ok());
        assert_eq!(std::fs::read_dir(scope.path()).unwrap().count(), 2);
    }

    #[test]
    fn concurrent_registrations_preserve_every_committed_update() {
        const WORKERS: usize = 4;
        let scope = tempfile::tempdir().unwrap();
        let path = scope.path().join("upstream.json");
        let barrier = std::sync::Barrier::new(WORKERS);
        std::thread::scope(|threads| {
            for index in 0..WORKERS {
                let path = &path;
                let barrier = &barrier;
                threads.spawn(move || {
                    let (mut source, _) = archive();
                    source.directory = format!("vendor/publisher-{index}").into();
                    source.archive = source.directory.join("upstream.crate");
                    barrier.wait();
                    update_registry(path, |registry| {
                        registry.sources.push(source);
                        Ok(())
                    })
                    .unwrap();
                });
            }
        });
        assert_eq!(
            parse(&std::fs::read(path).unwrap())
                .unwrap()
                .0
                .sources
                .len(),
            WORKERS
        );
    }

    #[test]
    fn completing_a_registration_releases_the_lock_even_with_a_retained_descriptor() {
        let scope = tempfile::tempdir().unwrap();
        let path = scope.path().join("upstream.json");
        let lock = RegistryLock::acquire(&path).unwrap();
        let retained = lock.0.try_clone().unwrap();
        let probe = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path.with_extension("lock"))
            .unwrap();
        assert!(matches!(
            probe.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        drop(lock);
        assert!(probe.try_lock().is_ok());
        drop(retained);
    }

    #[test]
    fn archive_rejects_traversal_links_duplicate_members_and_corrupt_compression() {
        for (path, kind, duplicate) in [
            ("publisher-1.0.0/../escape", tar::EntryType::Regular, false),
            ("publisher-1.0.0/src/link", tar::EntryType::Symlink, false),
            ("publisher-1.0.0/src/link", tar::EntryType::Link, false),
            ("publisher-1.0.0/src/lib.rs", tar::EntryType::Regular, true),
        ] {
            let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
                Vec::new(),
                flate2::Compression::default(),
            ));
            let mut header = tar::Header::new_gnu();
            for (target, byte) in header.as_mut_bytes().iter_mut().zip(path.bytes()) {
                *target = byte;
            }
            header.set_size(0);
            header.set_entry_type(kind);
            header.set_cksum();
            builder.append(&header, &[][..]).unwrap();
            if duplicate {
                builder.append(&header, &[][..]).unwrap();
            }
            let bytes = builder.into_inner().unwrap().finish().unwrap();
            let (mut source, _) = archive();
            source.sha256 = crate::build_inputs::hex(&Sha256::digest(&bytes));
            assert!(source.originals(&bytes).is_err(), "{path} {kind:?}");
        }
        let (mut source, mut bytes) = archive();
        let checksum = bytes.len() - 8;
        bytes[checksum] ^= 1;
        source.sha256 = crate::build_inputs::hex(&Sha256::digest(&bytes));
        assert!(source.originals(&bytes).is_err());
    }
}

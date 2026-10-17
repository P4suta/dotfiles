use super::{Artifact, Digest, Plan};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const RECEIPT_SCHEMA: u32 = 1;
const RECEIPT_DOMAIN: &[u8] = b"dotfiles.artifact-receipt.v1\0";
const TEMPORARY_PREFIX: &str = ".tmp";
const TEMPORARY_RANDOM_CHARACTERS: usize = 6;
const HASH_BUFFER_BYTES: usize = 8192;
const PROVENANCE_BYTE_LIMIT: usize = 8192;
pub(super) const ARTIFACT_BYTE_LIMIT: u64 = 512 * 1024 * 1024;

#[derive(Debug)]
pub enum FetchFailure {
    Retryable(String),
    Refused(String),
}

pub trait Fetch {
    /// Write the selected public source to the owned acquisition destination.
    ///
    /// # Errors
    ///
    /// Returns a retryable transport failure or a terminal refusal.
    fn fetch(&self, source: &str, destination: &Path) -> Result<(), FetchFailure>;
}

#[derive(Debug)]
pub struct VerifiedArtifact {
    file: File,
    expected: Digest,
    provenance: Digest,
}

impl VerifiedArtifact {
    #[must_use]
    pub const fn expected(&self) -> Digest {
        self.expected
    }

    #[must_use]
    pub const fn provenance(&self) -> Digest {
        self.provenance
    }

    #[must_use]
    pub fn into_file(self) -> File {
        self.file
    }
}

#[derive(Debug)]
pub struct VerifiedPlan {
    digest: Digest,
    artifacts: Vec<VerifiedArtifact>,
}

impl VerifiedPlan {
    #[must_use]
    pub const fn digest(&self) -> Digest {
        self.digest
    }

    #[must_use]
    pub fn artifacts(&self) -> &[VerifiedArtifact] {
        &self.artifacts
    }

    #[must_use]
    pub fn into_artifacts(self) -> Vec<VerifiedArtifact> {
        self.artifacts
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: u32,
    expected: Digest,
    provenance: Digest,
}

#[derive(Debug)]
struct Missing(String);

impl std::fmt::Display for Missing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "missing acquired artifact {}", self.0)
    }
}

impl std::error::Error for Missing {}

#[derive(Debug)]
pub struct Cache {
    root: PathBuf,
}

impl Cache {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Verify cached bytes and return their opened read-only file.
    ///
    /// # Errors
    ///
    /// Refuses absent, corrupt, and provenance-mismatched objects.
    pub fn verify(&self, plan: &Plan) -> Result<VerifiedPlan> {
        let artifacts = self.verify_stream(plan)?.collect::<Result<Vec<_>>>()?;
        ensure!(
            rules::artifact_rules::admission(
                plan.artifacts().len(),
                artifacts.len(),
                plan.diagnostics().len()
            ) == rules::artifact_rules::Admission::Ready,
            "acquired artifact set is incomplete"
        );
        Ok(VerifiedPlan {
            digest: plan.digest(),
            artifacts,
        })
    }

    /// Fetch only missing materials and publish no partial or corrupt object.
    ///
    /// # Errors
    ///
    /// Preserves the first acquisition failure at the bounded retry limit and refuses poison.
    pub fn acquire(&self, plan: &Plan, fetcher: &impl Fetch) -> Result<VerifiedPlan> {
        self.acquire_objects(plan, fetcher)?;
        self.verify(plan)
    }

    /// Authenticate each opened capability when consumed without retaining the other files.
    ///
    /// # Errors
    ///
    /// Rejects an incomplete plan or invalid cache.
    /// Each item must match present and unchanged material.
    pub fn verify_stream<'a>(
        &'a self,
        plan: &'a Plan,
    ) -> Result<impl ExactSizeIterator<Item = Result<VerifiedArtifact>> + 'a> {
        plan.complete()?;
        self.directory(false)?;
        Ok(plan.artifacts().iter().map(|artifact| {
            self.directory(false)?;
            self.object(artifact)
        }))
    }

    /// Load the complete plan before yielding individually authenticated opened capabilities.
    ///
    /// # Errors
    ///
    /// Preserves acquisition refusals and rechecks each receipt and blob when consumed.
    pub fn acquire_stream<'a>(
        &'a self,
        plan: &'a Plan,
        fetcher: &impl Fetch,
    ) -> Result<impl ExactSizeIterator<Item = Result<VerifiedArtifact>> + 'a> {
        self.acquire_objects(plan, fetcher)?;
        self.verify_stream(plan)
    }

    fn acquire_objects(&self, plan: &Plan, fetcher: &impl Fetch) -> Result<()> {
        plan.complete()?;
        self.directory(true)?;
        for artifact in plan.artifacts() {
            self.acquire_object(artifact, fetcher)?;
        }
        Ok(())
    }

    fn directory(&self, create: bool) -> Result<()> {
        ensure!(
            self.root.is_absolute(),
            "acquisition cache requires an absolute path"
        );
        #[cfg(windows)]
        self.windows_paths()?;
        if create {
            fs::create_dir_all(&self.root).context("create the acquisition cache")?;
        }
        let metadata = fs::symlink_metadata(&self.root).context("inspect the acquisition cache")?;
        ensure!(
            metadata.file_type().is_dir() && !metadata.file_type().is_symlink(),
            "acquisition cache must be a real directory"
        );
        Ok(())
    }

    #[cfg(windows)]
    fn windows_paths(&self) -> Result<()> {
        use std::os::windows::ffi::OsStrExt as _;

        let digest = Digest::of(b"").hex();
        let names = [
            format!("{digest}.blob"),
            format!("{digest}.json"),
            format!(
                "{TEMPORARY_PREFIX}{}",
                "x".repeat(TEMPORARY_RANDOM_CHARACTERS)
            ),
        ];
        let longest = names
            .iter()
            .map(|name| self.root.join(name).as_os_str().encode_wide().count())
            .max()
            .context("acquisition cache has fixed output names")?;
        ensure!(
            rules::artifact_rules::cache_paths_fit(
                self.root.as_os_str().encode_wide().count(),
                longest
            ),
            "Windows acquisition cache exceeds its UTF-16 directory or file path budget; select a shorter absolute cache root"
        );
        Ok(())
    }

    fn temporary(&self) -> Result<tempfile::NamedTempFile> {
        Ok(tempfile::Builder::new()
            .prefix(TEMPORARY_PREFIX)
            .rand_bytes(TEMPORARY_RANDOM_CHARACTERS)
            .tempfile_in(&self.root)?)
    }

    fn blob(&self, artifact: &Artifact) -> PathBuf {
        self.root.join(format!("{}.blob", artifact.expected.hex()))
    }

    fn receipt(&self, artifact: &Artifact) -> PathBuf {
        let mut hasher = Sha256::new();
        hasher.update(RECEIPT_DOMAIN);
        hasher.update(artifact.expected.hex().as_bytes());
        hasher.update(artifact.provenance.hex().as_bytes());
        self.root.join(format!(
            "{}.json",
            platform::build_inputs::hex(&hasher.finalize())
        ))
    }

    fn opened(path: &Path, artifact: &Artifact) -> Result<File> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(Missing(artifact.subject.clone()).into());
            }
            Err(error) => return Err(error).context("inspect the acquisition object"),
        };
        ensure!(
            metadata.file_type().is_file(),
            "{}: acquisition object is not a regular file",
            artifact.subject
        );
        let file = File::open(path).context("open the acquisition object")?;
        ensure!(
            file.metadata()?.is_file(),
            "{}: acquisition object changed type",
            artifact.subject
        );
        Ok(file)
    }

    pub(super) fn content(file: &mut File) -> Result<Digest> {
        file.seek(SeekFrom::Start(0))?;
        ensure!(
            file.metadata()?.len() <= ARTIFACT_BYTE_LIMIT,
            "acquisition object exceeds its byte limit"
        );
        let mut hasher = Sha256::new();
        let mut buffer = [0; HASH_BUFFER_BYTES];
        let mut limited = (&mut *file).take(ARTIFACT_BYTE_LIMIT + 1);
        while limited.limit() != 0 {
            let count = limited.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        ensure!(
            limited.limit() != 0,
            "acquisition object grew beyond its byte limit"
        );
        file.seek(SeekFrom::Start(0))?;
        Digest::try_from(platform::build_inputs::hex(&hasher.finalize()))
            .map_err(anyhow::Error::msg)
    }

    fn object(&self, artifact: &Artifact) -> Result<VerifiedArtifact> {
        let provenance = self.provenance(artifact)?;
        let mut file = Self::opened(&self.blob(artifact), artifact)?;
        let observed = Self::content(&mut file)?;
        ensure!(
            observed == artifact.expected,
            "{}: poisoned artifact; SHA-256 mismatch",
            artifact.subject
        );
        if !provenance {
            return Err(Missing(artifact.subject.clone()).into());
        }
        Ok(VerifiedArtifact {
            file,
            expected: observed,
            provenance: artifact.provenance,
        })
    }

    fn provenance(&self, artifact: &Artifact) -> Result<bool> {
        let receipt = match Self::opened(&self.receipt(artifact), artifact) {
            Ok(receipt) => receipt,
            Err(error) if error.downcast_ref::<Missing>().is_some() => return Ok(false),
            Err(error) => return Err(error),
        };
        ensure!(
            receipt.metadata()?.len() <= PROVENANCE_BYTE_LIMIT as u64,
            "{}: acquisition provenance receipt exceeds its byte limit",
            artifact.subject
        );
        let mut bytes = Vec::new();
        receipt
            .take(PROVENANCE_BYTE_LIMIT as u64 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= PROVENANCE_BYTE_LIMIT,
            "{}: acquisition provenance receipt grew beyond its byte limit",
            artifact.subject
        );
        let receipt: Receipt =
            serde_json::from_slice(&bytes).context("parse acquisition provenance receipt")?;
        ensure!(
            rules::artifact_rules::usable(
                receipt.expected == artifact.expected,
                receipt.schema == RECEIPT_SCHEMA && receipt.provenance == artifact.provenance
            ),
            "{}: acquisition provenance does not match its pinned source",
            artifact.subject
        );
        Ok(true)
    }

    fn acquire_object(&self, artifact: &Artifact, fetcher: &impl Fetch) -> Result<()> {
        let mut attempts = 0;
        let mut first_failure: Option<String> = None;
        loop {
            let material = match self.object(artifact) {
                Ok(_) => rules::artifact_rules::Material::Verified,
                Err(error) if error.downcast_ref::<Missing>().is_some() => {
                    match Self::opened(&self.blob(artifact), artifact) {
                        Ok(mut bytes) => {
                            ensure!(
                                Self::content(&mut bytes)? == artifact.expected,
                                "{}: existing artifact is poisoned",
                                artifact.subject
                            );
                            self.publish_receipt(artifact)?;
                            return self.object(artifact).map(|_| ());
                        }
                        Err(error) if error.downcast_ref::<Missing>().is_some() => {
                            rules::artifact_rules::Material::Missing
                        }
                        Err(error) => return Err(error),
                    }
                }
                Err(error) => return Err(error),
            };
            match rules::artifact_rules::acquisition(material, attempts) {
                rules::artifact_rules::Acquisition::Reuse => return Ok(()),
                rules::artifact_rules::Acquisition::RefuseCorrupt => {
                    bail!("{}: poisoned artifact", artifact.subject)
                }
                rules::artifact_rules::Acquisition::RefuseExhausted => {
                    let cause =
                        first_failure.context("acquisition ended without an attempted fetch")?;
                    return Err(anyhow::anyhow!(cause)).context(format!(
                        "{}: acquisition exhausted after {attempts} attempts",
                        artifact.subject
                    ));
                }
                rules::artifact_rules::Acquisition::Fetch => {}
            }
            let mut temporary = self.temporary()?;
            attempts += 1;
            match fetcher.fetch(&artifact.source, temporary.path()) {
                Ok(()) => {}
                Err(FetchFailure::Retryable(cause)) => {
                    if first_failure.is_none() {
                        first_failure = Some(cause);
                    }
                    platform::cleanup::report(
                        temporary.close(),
                        "remove incomplete acquired bytes",
                    );
                    continue;
                }
                Err(FetchFailure::Refused(cause)) => {
                    platform::cleanup::report(
                        temporary.close(),
                        "remove refused acquisition bytes",
                    );
                    return Err(anyhow::anyhow!(cause))
                        .context(format!("{}: acquisition refused", artifact.subject));
                }
            }
            let checked = (|| {
                ensure!(
                    Self::content(temporary.as_file_mut())? == artifact.expected,
                    "{}: fetched artifact SHA-256 mismatch",
                    artifact.subject
                );
                temporary.as_file().sync_all()?;
                Ok::<_, anyhow::Error>(())
            })();
            if let Err(error) = checked {
                platform::cleanup::report(temporary.close(), "remove rejected acquired bytes");
                return Err(error);
            }
            self.publish_blob(temporary, artifact)?;
            self.publish_receipt(artifact)?;
            return self.object(artifact).map(|_| ());
        }
    }

    fn publish_blob(&self, temporary: tempfile::NamedTempFile, artifact: &Artifact) -> Result<()> {
        match temporary.persist_noclobber(self.blob(artifact)) {
            Ok(_) => {}
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                platform::cleanup::report(error.file.close(), "remove duplicate acquired bytes");
                let mut observed = Self::opened(&self.blob(artifact), artifact)?;
                ensure!(
                    Self::content(&mut observed)? == artifact.expected,
                    "{}: concurrently published artifact is poisoned",
                    artifact.subject
                );
            }
            Err(error) => {
                let cause = error.error;
                platform::cleanup::report(error.file.close(), "remove unpublished acquired bytes");
                return Err(cause).context("publish acquired bytes");
            }
        }
        Ok(())
    }

    fn publish_receipt(&self, artifact: &Artifact) -> Result<()> {
        let receipt = Receipt {
            schema: RECEIPT_SCHEMA,
            expected: artifact.expected,
            provenance: artifact.provenance,
        };
        let mut temporary = self.temporary()?;
        let bytes = serde_json::to_vec(&receipt)?;
        let result = temporary
            .write_all(&bytes)
            .and_then(|()| temporary.as_file().sync_all());
        if let Err(error) = result {
            platform::cleanup::report(temporary.close(), "remove incomplete acquisition receipt");
            return Err(error).context("write acquisition provenance");
        }
        match temporary.persist_noclobber(self.receipt(artifact)) {
            Ok(_) => {}
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                platform::cleanup::report(
                    error.file.close(),
                    "remove duplicate acquisition receipt",
                );
            }
            Err(error) => {
                let cause = error.error;
                platform::cleanup::report(
                    error.file.close(),
                    "remove unpublished acquisition receipt",
                );
                return Err(cause).context("publish acquisition provenance");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn complete_artifact_hashes_do_not_depend_on_the_opened_file_position() -> Result<()> {
        let bytes = b"complete acquired artifact";
        let mut captured = tempfile::NamedTempFile::new()?;
        captured.write_all(bytes)?;
        let length = u64::try_from(bytes.len())?;
        for position in [0, 1, length, length + 1] {
            captured.as_file_mut().seek(SeekFrom::Start(position))?;
            assert_eq!(Cache::content(captured.as_file_mut())?, Digest::of(bytes));
            assert_eq!(captured.as_file_mut().stream_position()?, 0);
        }
        Ok(())
    }

    #[derive(Debug)]
    struct Download<'a> {
        bytes: &'a [u8],
        calls: Cell<u8>,
        fail: bool,
    }

    impl Fetch for Download<'_> {
        fn fetch(
            &self,
            _source: &str,
            destination: &Path,
        ) -> std::result::Result<(), FetchFailure> {
            self.calls.set(self.calls.get() + 1);
            fs::write(destination, self.bytes)
                .map_err(|error| FetchFailure::Refused(error.to_string()))?;
            if self.fail {
                Err(FetchFailure::Retryable(format!(
                    "transport failure {}",
                    self.calls.get()
                )))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn matching_bytes_grant_a_bound_read_only_capability() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let bytes = b"verified archive";
        let digest = Digest::of(bytes);
        std::fs::write(scope.path().join(format!("{}.blob", digest.hex())), bytes)?;
        let plan = fixture(bytes)?;
        let artifact = plan
            .artifacts()
            .first()
            .expect("fixture contains its selected artifact");
        let cache = Cache::new(scope.path().to_path_buf());
        std::fs::write(
            cache.receipt(artifact),
            serde_json::to_vec(&serde_json::json!({
                "schema": 1,
                "expected": digest,
                "provenance": artifact.provenance,
            }))?,
        )?;
        let verified_plan = cache.verify(&plan)?;
        assert_eq!(verified_plan.digest, plan.digest());
        let verified = verified_plan
            .artifacts
            .into_iter()
            .next()
            .expect("verified plan contains its artifact");
        assert_eq!(verified.expected, digest);
        assert_eq!(verified.provenance, artifact.provenance);
        assert!(verified.file.set_len(0).is_err());
        Ok(())
    }

    fn fixture(bytes: &[u8]) -> Result<Plan> {
        fixture_source(bytes, "https://example.com/example-1.2.3.zip")
    }

    fn fixture_source(bytes: &[u8], url: &str) -> Result<Plan> {
        let lock = format!(
            "lockfile_version = 3\n[[tools.example]]\nversion = \"1.2.3\"\nbackend = \"aqua:owner/example\"\nspecifiers = [\"1.2.3\"]\n[tools.example.\"platforms.windows-x64\"]\nurl = \"{url}\"\nchecksum = \"sha256:{}\"\n",
            Digest::of(bytes).hex()
        );
        Plan::parse(
            super::super::Inputs {
                project: "[tools]\nexample = \"1.2.3\"\n",
                mise_lock: &lock,
                cargo_lock: "version = 4\n",
                versions: "",
            },
            super::super::Request {
                targets: vec![super::super::Target::WindowsX64],
                tools: vec!["example".into()],
            },
        )
    }

    fn root_with_units(parent: &Path, units: usize, prefix: &str) -> Result<PathBuf> {
        let parent_units = parent
            .to_str()
            .context("fixture path is Unicode")?
            .encode_utf16()
            .count();
        let padding = units
            .checked_sub(parent_units + 1 + prefix.encode_utf16().count())
            .context("fixture temporary root must leave the requested cache-path budget")?;
        Ok(parent.join(format!("{prefix}{}", "x".repeat(padding))))
    }

    #[test]
    fn a_129_unit_cache_root_acquires_and_verifies_without_an_overlong_receipt() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let root = root_with_units(scope.path(), 129, "")?;
        let plan = fixture(b"expected")?;
        let artifact = plan.artifacts().first().expect("selected artifact");
        let legacy = root.join(format!(
            "{}-{}.json",
            artifact.expected.hex(),
            artifact.provenance.hex()
        ));
        assert_eq!(
            legacy
                .to_str()
                .expect("Unicode fixture")
                .encode_utf16()
                .count(),
            264
        );
        let cache = Cache::new(root);
        let fetch = Download {
            bytes: b"expected",
            calls: Cell::new(0),
            fail: false,
        };
        cache.acquire(&plan, &fetch)?;
        cache.verify(&plan)?;
        assert_eq!(fetch.calls.get(), 1);
        assert_eq!(
            cache
                .receipt(artifact)
                .file_name()
                .expect("receipt name")
                .len(),
            69
        );
        assert_eq!(fs::read_dir(&cache.root)?.count(), 2);
        Ok(())
    }

    #[test]
    fn legacy_receipts_never_grant_credit_and_valid_blobs_migrate_without_fetch() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let cache = Cache::new(scope.path().to_path_buf());
        let plan = fixture(b"expected")?;
        let artifact = plan.artifacts().first().expect("selected artifact");
        let legacy = cache.root.join(format!(
            "{}-{}.json",
            artifact.expected.hex(),
            artifact.provenance.hex()
        ));
        fs::write(cache.blob(artifact), b"expected")?;
        fs::write(
            &legacy,
            serde_json::to_vec(&Receipt {
                schema: RECEIPT_SCHEMA,
                expected: artifact.expected,
                provenance: artifact.provenance,
            })?,
        )?;
        let error = cache
            .verify(&plan)
            .expect_err("legacy metadata alone is not current receipt credit");
        assert!(error.downcast_ref::<Missing>().is_some(), "{error:#}");
        let fetch = Download {
            bytes: b"must not fetch",
            calls: Cell::new(0),
            fail: false,
        };
        cache.acquire(&plan, &fetch)?;
        cache.verify(&plan)?;
        assert_eq!(fetch.calls.get(), 0);
        assert!(legacy.is_file());
        assert!(cache.receipt(artifact).is_file());
        Ok(())
    }

    #[test]
    fn receipt_names_bind_both_identities_and_the_domain_prefix() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let cache = Cache::new(scope.path().to_path_buf());
        let first = fixture(b"expected")?;
        let second = fixture_source(b"expected", "https://example.com/other-1.2.3.zip")?;
        let first = first.artifacts().first().expect("first artifact");
        let second = second.artifacts().first().expect("second artifact");
        assert_eq!(first.expected, second.expected);
        assert_ne!(first.provenance, second.provenance);
        assert_ne!(cache.receipt(first), cache.receipt(second));
        let mut vector = first.clone();
        vector.provenance = Digest::of(b"source");
        assert_eq!(
            cache.receipt(&vector).file_name().expect("receipt name"),
            "8299cecaf92f2b361a0b2c10e377093eb97f88e99ccd04e7d05ff430e9b26eac.json"
        );
        vector.expected = Digest::of(b"other bytes");
        assert_ne!(cache.receipt(first), cache.receipt(&vector));
        Ok(())
    }

    #[test]
    fn unknown_receipt_metadata_is_refused_without_fetch_or_repair() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let plan = fixture(b"expected")?;
        let cache = Cache::new(scope.path().to_path_buf());
        let fetch = Download {
            bytes: b"expected",
            calls: Cell::new(0),
            fail: false,
        };
        cache.acquire(&plan, &fetch)?;
        let artifact = plan.artifacts().first().expect("selected artifact");
        let receipt = cache.receipt(artifact);
        let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&receipt)?)?;
        value["unverified_metadata"] = serde_json::json!(true);
        let poison = serde_json::to_vec(&value)?;
        fs::write(&receipt, &poison)?;
        assert!(cache.verify(&plan).is_err());
        assert!(cache.acquire(&plan, &fetch).is_err());
        assert_eq!(fetch.calls.get(), 1);
        assert_eq!(fs::read(receipt)?, poison);
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn windows_cache_budget_counts_utf16_before_directory_creation_and_fetch() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let plan = fixture(b"expected")?;
        let prefix = "😀".repeat(8);
        let supported = Cache::new(root_with_units(scope.path(), 189, &prefix)?);
        let fetch = Download {
            bytes: b"expected",
            calls: Cell::new(0),
            fail: false,
        };
        supported.acquire(&plan, &fetch)?;
        supported.verify(&plan)?;
        let refused_root = root_with_units(scope.path(), 190, &prefix)?;
        let refused = Cache::new(refused_root.clone());
        let error = refused
            .acquire(&plan, &fetch)
            .expect_err("full path plus NUL exceeds the limit");
        assert!(
            format!("{error:#}").contains("Windows acquisition cache"),
            "{error:#}"
        );
        assert_eq!(fetch.calls.get(), 1);
        assert!(!refused_root.exists());
        assert!(refused.verify(&plan).is_err());
        Ok(())
    }

    #[test]
    fn reuse_never_fetches_and_a_poisoned_blob_never_gets_replaced() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let plan = fixture(b"expected")?;
        let cache = Cache::new(scope.path().to_path_buf());
        let fetch = Download {
            bytes: b"expected",
            calls: Cell::new(0),
            fail: false,
        };
        cache.acquire(&plan, &fetch)?;
        cache.acquire(&plan, &fetch)?;
        assert_eq!(fetch.calls.get(), 1);
        let artifact = plan.artifacts().first().expect("selected artifact");
        fs::write(cache.blob(artifact), b"poisoned")?;
        assert!(cache.verify(&plan).is_err());
        assert!(cache.acquire(&plan, &fetch).is_err());
        assert_eq!(fetch.calls.get(), 1);
        assert_eq!(fs::read(cache.blob(artifact))?, b"poisoned");
        Ok(())
    }

    #[test]
    fn retry_exhaustion_preserves_the_first_cause_and_publishes_no_partial_bytes() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let plan = fixture(b"expected")?;
        let cache = Cache::new(scope.path().to_path_buf());
        let fetch = Download {
            bytes: b"partial",
            calls: Cell::new(0),
            fail: true,
        };
        let error = cache
            .acquire(&plan, &fetch)
            .expect_err("exhausted transport cannot grant verified credit");
        assert!(
            format!("{error:#}").contains("transport failure 1"),
            "{error:#}"
        );
        assert_eq!(fetch.calls.get(), rules::artifact_rules::FETCH_ATTEMPTS);
        assert!(cache.verify(&plan).is_err());
        assert_eq!(fs::read_dir(scope.path())?.count(), 0);
        Ok(())
    }

    #[test]
    fn mismatched_download_and_provenance_do_not_grant_verified_credit() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let plan = fixture(b"expected")?;
        let cache = Cache::new(scope.path().to_path_buf());
        let poison = Download {
            bytes: b"poisoned",
            calls: Cell::new(0),
            fail: false,
        };
        assert!(cache.acquire(&plan, &poison).is_err());
        assert_eq!(poison.calls.get(), 1);
        assert!(cache.verify(&plan).is_err());
        let valid = Download {
            bytes: b"expected",
            calls: Cell::new(0),
            fail: false,
        };
        cache.acquire(&plan, &valid)?;
        let artifact = plan.artifacts().first().expect("selected artifact");
        fs::write(
            cache.receipt(artifact),
            serde_json::to_vec(&Receipt {
                schema: RECEIPT_SCHEMA,
                expected: artifact.expected,
                provenance: Digest::of(b"other provenance"),
            })?,
        )?;
        assert!(cache.verify(&plan).is_err());
        assert!(cache.acquire(&plan, &valid).is_err());
        assert_eq!(valid.calls.get(), 1);
        Ok(())
    }

    #[test]
    fn a_missing_receipt_is_reconstructed_without_refetching_verified_bytes() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let plan = fixture(b"expected")?;
        let cache = Cache::new(scope.path().to_path_buf());
        let fetch = Download {
            bytes: b"expected",
            calls: Cell::new(0),
            fail: false,
        };
        cache.acquire(&plan, &fetch)?;
        fs::remove_file(cache.receipt(plan.artifacts().first().expect("selected artifact")))?;
        assert!(cache.verify(&plan).is_err());
        cache.acquire(&plan, &fetch)?;
        assert_eq!(fetch.calls.get(), 1);
        Ok(())
    }

    #[test]
    fn corrupt_receipts_dominate_missing_bytes_and_oversized_receipts_fail_closed() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let plan = fixture(b"expected")?;
        let artifact = plan.artifacts().first().expect("selected artifact");
        let cache = Cache::new(scope.path().to_path_buf());
        let fetch = Download {
            bytes: b"expected",
            calls: Cell::new(0),
            fail: false,
        };
        fs::write(cache.receipt(artifact), "poisoned provenance")?;
        assert!(cache.acquire(&plan, &fetch).is_err());
        assert_eq!(
            fetch.calls.get(),
            0,
            "poison receipt must refuse before fetching missing bytes"
        );
        fs::remove_file(cache.receipt(artifact))?;
        cache.acquire(&plan, &fetch)?;
        let mut oversized = serde_json::to_vec(&Receipt {
            schema: RECEIPT_SCHEMA,
            expected: artifact.expected,
            provenance: artifact.provenance,
        })?;
        oversized.resize(PROVENANCE_BYTE_LIMIT, b' ');
        oversized.extend_from_slice(b"unverified trailing bytes");
        fs::write(cache.receipt(artifact), oversized)?;
        assert!(
            cache.verify(&plan).is_err(),
            "receipt size must not hide trailing bytes"
        );
        Ok(())
    }

    #[test]
    fn concurrent_acquisition_publishes_one_complete_immutable_identity() -> Result<()> {
        const WORKERS: usize = 2;
        const SYNCHRONIZATION_LIMIT: std::time::Duration = std::time::Duration::from_secs(10);

        #[derive(Debug)]
        struct Concurrent {
            ready: std::sync::mpsc::Sender<()>,
            proceed: std::sync::mpsc::Receiver<()>,
        }

        impl Fetch for Concurrent {
            fn fetch(
                &self,
                _source: &str,
                destination: &Path,
            ) -> std::result::Result<(), FetchFailure> {
                self.ready
                    .send(())
                    .map_err(|_| FetchFailure::Refused("fixture coordinator closed".into()))?;
                self.proceed
                    .recv_timeout(SYNCHRONIZATION_LIMIT)
                    .map_err(|_| {
                        FetchFailure::Refused("fixture synchronization timed out".into())
                    })?;
                fs::write(destination, b"expected")
                    .map_err(|error| FetchFailure::Refused(error.to_string()))
            }
        }

        let scope = tempfile::tempdir()?;
        let plan = std::sync::Arc::new(fixture(b"expected")?);
        let (ready, arrived) = std::sync::mpsc::channel();
        let mut workers = Vec::new();
        let mut releases = Vec::new();
        for _ in 0..WORKERS {
            let plan = std::sync::Arc::clone(&plan);
            let ready = ready.clone();
            let (release, proceed) = std::sync::mpsc::channel();
            releases.push(release);
            let root = scope.path().to_path_buf();
            workers.push(std::thread::spawn(move || {
                Cache::new(root).acquire(&plan, &Concurrent { ready, proceed })
            }));
        }
        drop(ready);
        let synchronization = (|| {
            for _ in 0..WORKERS {
                arrived
                    .recv_timeout(SYNCHRONIZATION_LIMIT)
                    .context("wait for concurrent acquisitions")?;
            }
            for release in &releases {
                release
                    .send(())
                    .context("release concurrent acquisitions")?;
            }
            Ok::<_, anyhow::Error>(())
        })();
        drop(releases);
        let completed = workers
            .into_iter()
            .map(|worker| worker.join().expect("acquisition thread must not panic"))
            .collect::<Result<Vec<_>>>();
        synchronization?;
        completed?;
        Cache::new(scope.path().to_path_buf()).verify(&plan)?;
        assert_eq!(
            fs::read_dir(scope.path())?.count(),
            2,
            "one blob and one bound receipt must remain"
        );
        Ok(())
    }
}

use super::{
    Artifact, Cache, Diagnostic, Digest, Inputs, Plan, Reason, Request, Target, VerifiedArtifact,
    VerifiedPlan,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const ENGINE_VERSION: &str = "2026.10.9";
const CONFIGURATION: &str = "mise.toml";
const LOCK: &str = "mise.lock";
const CARGO_LOCK: &str = "Cargo.lock";
const SOURCE_VERSIONS: &str = "home/.chezmoidata/versions.toml";
const INPUT_NAMES: [&str; 4] = [CONFIGURATION, LOCK, CARGO_LOCK, SOURCE_VERSIONS];
const SOURCE_BYTE_LIMIT: u64 = 8 * 1024 * 1024;
const MATERIAL_COUNT_LIMIT: usize = 128;
const SCAN_FILES_PER_MATERIAL_ALLOWANCE: usize = 3;
const SCAN_FIXED_FILE_ALLOWANCE: usize = 3;
const PRIVATE_STORE_SCAN_FILE_LIMIT: usize =
    MATERIAL_COUNT_LIMIT * SCAN_FILES_PER_MATERIAL_ALLOWANCE + SCAN_FIXED_FILE_ALLOWANCE;
const TOTAL_MATERIAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const COPY_BYTES: usize = 8192;
const CAS_DIRECTORY: &str = "downloads-cas";
const PIN_ALGORITHM: &str = "sha256";
const HOME_DIRECTORY: &str = "home";
const CONFIG_DIRECTORY: &str = "configuration";
const CACHE_DIRECTORY: &str = "cache";
const DATA_DIRECTORY: &str = "data";
const STATE_DIRECTORY: &str = "state";
const EMPTY_CONFIGURATION: &str = "empty.toml";
const PROCESS_BYTE_LIMIT: u64 = 8 * 1024 * 1024;
const INSTALL_LIMIT: Duration = Duration::from_secs(120);
const PROCESS_POLL: Duration = Duration::from_millis(20);
const STORE_DEPTH_LIMIT: usize = 6;
const JSON_EXTENSION: &str = "json";
const BOOTSTRAP: &str = "mise-bootstrap";
const ENGINE_DIRECTORY: &str = "engine";
const ENGINE_ARCHIVE: &str = "mise-release.zip";
const ENGINE_ENTRY: &str = "mise/bin/mise.exe";
const ENGINE_FILE: &str = "mise.exe";
const ENGINE_UNIX_FILE: &str = "mise";
const PIN_BYTE_LIMIT: usize = 8192;
const ZIP_MEMBER_LIMIT: usize = 128;
const ZIP_BYTE_LIMIT: u64 = 512 * 1024 * 1024;
const ZIP_INDEX_LIMIT: usize = 128 * 1024;
const ZIP_MODE_MASK: u32 = 0o170_000;
const ZIP_MODE_FILE: u32 = 0o100_000;
const ZIP_MODE_DIRECTORY: u32 = 0o040_000;
const ZIP_DIRECTORY_ATTRIBUTE: u32 = 0x10;
const ZIP_UNIX_MODE_SHIFT: u32 = 16;
#[cfg(unix)]
const PRIVATE_ENGINE_MODE: u32 = 0o700;
#[cfg(windows)]
const ZIP_INDEX: &str = "[string]$Archive=$args[0]\n[int]$Limit=$args[1]\n$ErrorActionPreference='Stop'\nAdd-Type -AssemblyName System.IO.Compression.FileSystem\n$zip=[IO.Compression.ZipFile]::OpenRead($Archive)\ntry {\n  $entries=@(foreach ($entry in ($zip.Entries | Select-Object -First ($Limit+1))) { @{name=$entry.FullName;length=$entry.Length;attributes=([long]$entry.ExternalAttributes -band 4294967295)} })\n  @{count=$zip.Entries.Count;entries=$entries} | ConvertTo-Json -Depth 4 -Compress\n} finally { $zip.Dispose() }";
#[cfg(windows)]
const ZIP_EXTRACT: &str = "[string]$Archive=$args[0]\n[string]$Destination=$args[1]\n[string]$Name=$args[2]\n$ErrorActionPreference='Stop'\nAdd-Type -AssemblyName System.IO.Compression.FileSystem\n$zip=[IO.Compression.ZipFile]::OpenRead($Archive)\ntry {\n  $entry=$zip.GetEntry($Name)\n  if ($null -eq $entry) { throw 'verified bootstrap entry is missing' }\n  [IO.Compression.ZipFileExtensions]::ExtractToFile($entry,$Destination,$false)\n} finally { $zip.Dispose() }";

#[derive(Clone, Debug)]
struct Member {
    subject: String,
    tool: String,
    version: String,
    filename: String,
    expected: Digest,
    provenance: Digest,
}

/// A selected release-archive input scope, without native environment readiness.
#[derive(Debug)]
pub struct MiseArchiveWorkload {
    root: PathBuf,
    target: Target,
    inputs: BTreeMap<String, String>,
    plan: Plan,
    bootstrap: Option<Artifact>,
    members: Vec<Member>,
    unresolved: Vec<Diagnostic>,
}

impl MiseArchiveWorkload {
    /// Capture exact Source requests and locks for explicitly selected archive consumers.
    ///
    /// # Errors
    ///
    /// Refuses unsafe Source paths, empty selection, malformed inputs, and conflicting members.
    pub fn capture(root: &Path, target: Target, tools: &[String]) -> Result<Self> {
        let root = directory(root)?;
        ensure!(
            !tools.is_empty() && tools.len() <= MATERIAL_COUNT_LIMIT,
            "archive consumption needs a bounded explicit tool scope"
        );
        let mut selected = BTreeSet::new();
        for tool in tools {
            ensure!(
                !tool.starts_with('-')
                    && !tool.chars().any(char::is_control)
                    && selected.insert(tool),
                "archive consumption has an invalid or repeated tool"
            );
        }
        let inputs = source_inputs(&root)?;
        let plan = Plan::parse(
            Inputs {
                project: &inputs[CONFIGURATION],
                mise_lock: &inputs[LOCK],
                cargo_lock: &inputs[CARGO_LOCK],
                versions: &inputs[SOURCE_VERSIONS],
            },
            Request {
                targets: vec![target],
                tools: tools
                    .iter()
                    .cloned()
                    .chain(std::iter::once(BOOTSTRAP.into()))
                    .collect(),
            },
        )?;
        let configuration: toml::Table = toml::from_str(&inputs[CONFIGURATION])?;
        let lock: toml::Table = toml::from_str(&inputs[LOCK])?;
        let mut unresolved = Vec::new();
        let pins = model::data::parse(SOURCE_VERSIONS, &inputs[SOURCE_VERSIONS])
            .map_err(anyhow::Error::msg)?;
        if pins.mise.and_then(|mise| mise.version).as_deref() != Some(ENGINE_VERSION) {
            unresolved.push(Diagnostic {
                subject: "mise-download-store-abi".into(),
                reason: Reason::VersionMismatch,
            });
        }
        let members = members(tools, target, &configuration, &lock, &plan, &mut unresolved)?;
        let bootstrap = plan
            .artifacts()
            .iter()
            .find(|artifact| {
                artifact.subject() == format!("{BOOTSTRAP}@{ENGINE_VERSION}:{}", target.name())
            })
            .cloned();
        Ok(Self {
            root,
            target,
            inputs,
            plan,
            bootstrap,
            members,
            unresolved,
        })
    }

    #[must_use]
    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    #[must_use]
    pub fn unresolved(&self) -> &[Diagnostic] {
        &self.unresolved
    }

    /// Require this archive scope without granting readiness to other backends.
    ///
    /// # Errors
    ///
    /// Refuses missing materials, unsupported providers, and unresolved engine identities.
    pub fn complete(&self) -> Result<()> {
        self.plan.complete()?;
        ensure!(
            self.unresolved.is_empty()
                && !self.members.is_empty()
                && self.bootstrap.is_some()
                && self.members.len() + 1 == self.plan.artifacts().len(),
            "archive consumer inputs are unresolved"
        );
        Ok(())
    }

    fn current(&self) -> Result<()> {
        ensure!(
            source_inputs(&self.root)? == self.inputs,
            "archive Source inputs changed before consumption"
        );
        self.complete()
    }
}

fn members(
    tools: &[String],
    target: Target,
    configuration: &toml::Table,
    lock: &toml::Table,
    plan: &Plan,
    unresolved: &mut Vec<Diagnostic>,
) -> Result<Vec<Member>> {
    let mut members = Vec::new();
    let locked = lock
        .get("tools")
        .and_then(toml::Value::as_table)
        .context("archive lock needs tools")?;
    for tool in tools {
        let pin = configuration
            .get("tools")
            .and_then(|tools| tools.get(tool))
            .context("selected archive tool is absent")?;
        let requested = pin
            .as_str()
            .or_else(|| pin.get("version").and_then(toml::Value::as_str))
            .context("selected archive tool requires a version")?;
        let versions = locked.get(tool).and_then(toml::Value::as_array);
        let version = versions.and_then(|versions| {
            versions.iter().find(|version| {
                version.get("version").and_then(toml::Value::as_str) == Some(requested)
            })
        });
        let Some(version) = version else {
            continue;
        };
        let backend = version
            .get("backend")
            .and_then(toml::Value::as_str)
            .context("archive tool requires its backend")?;
        if !matches!(
            backend.split_once(':').map(|(kind, _)| kind),
            Some("github" | "http" | "aqua")
        ) {
            unresolved.push(Diagnostic {
                subject: format!("{tool}:archive-provider"),
                reason: Reason::UnsupportedBackend,
            });
            continue;
        }
        if !backend.starts_with("http:") {
            unresolved.push(Diagnostic {
                subject: format!("{tool}:offline-provider-verification-materials"),
                reason: Reason::MissingDependency,
            });
        }
        if configuration
            .get("tool_alias")
            .and_then(|aliases| aliases.get(tool))
            .is_some()
        {
            unresolved.push(Diagnostic {
                subject: format!("{tool}:provider-revision"),
                reason: Reason::MissingDependency,
            });
            continue;
        }
        let subject = format!("{tool}@{requested}:{}", target.name());
        let Some(artifact) = plan
            .artifacts()
            .iter()
            .find(|artifact| artifact.subject() == subject)
        else {
            continue;
        };
        let url = url::Url::parse(&artifact.source)?;
        let filename = url
            .path_segments()
            .and_then(|mut segments| segments.next_back())
            .context("archive URL requires a filename")?;
        ensure!(
            rules::artifact_rules::portable_component(filename.as_bytes())
                && !filename.contains('%'),
            "archive filename is not a supported portable original name"
        );
        members.push(Member {
            subject,
            tool: tool.clone(),
            version: requested.into(),
            filename: filename.into(),
            expected: artifact.expected,
            provenance: artifact.provenance,
        });
    }
    members.sort_by(|left, right| left.subject.cmp(&right.subject));
    Ok(members)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PinEntry {
    blake3: String,
    effective_filename: Option<String>,
}

/// Decoded download index with no material or installation authority.
#[derive(Debug)]
pub struct MiseDownloadPin(PinEntry);

impl MiseDownloadPin {
    /// Decode the same bounded pin entry consumed by the native store inventory.
    ///
    /// # Errors
    ///
    /// Rejects malformed, repeated, unknown, oversize, or unsafe pin fields.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= PIN_BYTE_LIMIT,
            "download pin index is oversized"
        );
        let entry: PinEntry = serde_json::from_slice(bytes)?;
        ensure!(
            rules::artifact_rules::sha256(entry.blake3.as_bytes()).is_some()
                && entry.effective_filename.as_deref().is_some_and(|name| {
                    rules::artifact_rules::portable_component(name.as_bytes())
                }),
            "private download pin index has invalid fields"
        );
        Ok(Self(entry))
    }

    #[must_use]
    pub fn blake3(&self) -> &str {
        &self.0.blake3
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BootstrapEntry {
    name: String,
    length: u64,
    attributes: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BootstrapIndex {
    count: usize,
    entries: Vec<BootstrapEntry>,
}

/// Audited native ZIP metadata with no archive provenance or executable authority.
#[derive(Debug)]
pub struct MiseBootstrapIndex(BootstrapIndex);

impl MiseBootstrapIndex {
    /// Audit the exact bootstrap member and every native archive index entry.
    ///
    /// # Errors
    ///
    /// Rejects ambiguous, unsafe, linked, special, oversize, or missing entries.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= ZIP_INDEX_LIMIT,
            "bootstrap ZIP index is oversized"
        );
        let index: BootstrapIndex = serde_json::from_slice(bytes)?;
        ensure!(
            index.count == index.entries.len() && index.count <= ZIP_MEMBER_LIMIT,
            "bootstrap ZIP entry inventory is incomplete or oversized"
        );
        let mut names = BTreeSet::new();
        let mut prefixes = BTreeMap::new();
        let mut kinds = BTreeMap::new();
        let mut total = 0u64;
        let mut program = None;
        for entry in &index.entries {
            let (name, directory) = zip_member(entry)?;
            ensure!(
                names.insert(name.to_lowercase()),
                "bootstrap ZIP has repeated paths"
            );
            let mut prefix = String::new();
            for component in name.split('/') {
                if !prefix.is_empty() {
                    ensure!(
                        kinds.get(&prefix.to_lowercase()) != Some(&false),
                        "bootstrap ZIP path crosses a regular member"
                    );
                    prefix.push('/');
                }
                prefix.push_str(component);
                if let Some(previous) = prefixes.insert(prefix.to_lowercase(), prefix.clone()) {
                    ensure!(
                        previous == prefix,
                        "bootstrap ZIP has conflicting path case"
                    );
                }
            }
            if !directory {
                ensure!(
                    !kinds.keys().any(|existing: &String| {
                        existing.starts_with(&format!("{}/", name.to_lowercase()))
                    }),
                    "bootstrap ZIP regular member replaces an existing directory"
                );
            }
            kinds.insert(name.to_lowercase(), directory);
            total = total
                .checked_add(entry.length)
                .context("bootstrap ZIP size overflows")?;
            ensure!(
                total <= ZIP_BYTE_LIMIT,
                "bootstrap ZIP expanded size exceeds its bound"
            );
            if entry.name == ENGINE_ENTRY && !directory {
                program = Some(entry.length);
            }
        }
        ensure!(
            program.is_some_and(|length| length > 0),
            "bootstrap ZIP executable is missing"
        );
        Ok(Self(index))
    }

    #[must_use]
    pub const fn member_count(&self) -> usize {
        self.0.count
    }

    #[cfg(windows)]
    fn program_bytes(&self) -> Result<u64> {
        self.0
            .entries
            .iter()
            .find(|entry| entry.name == ENGINE_ENTRY)
            .map(|entry| entry.length)
            .context("audited bootstrap executable is missing")
    }
}

fn zip_member(entry: &BootstrapEntry) -> Result<(&str, bool)> {
    let directory = entry.name.ends_with('/');
    let name = entry.name.strip_suffix('/').unwrap_or(&entry.name);
    ensure!(
        !name.contains('\\')
            && name
                .split('/')
                .all(|part| rules::artifact_rules::portable_component(part.as_bytes())),
        "bootstrap ZIP member has an unsafe path"
    );
    let mode = (entry.attributes >> ZIP_UNIX_MODE_SHIFT) & ZIP_MODE_MASK;
    ensure!(
        if directory {
            matches!(mode, 0 | ZIP_MODE_DIRECTORY | ZIP_MODE_FILE) && entry.length == 0
        } else {
            matches!(mode, 0 | ZIP_MODE_FILE) && entry.attributes & ZIP_DIRECTORY_ATTRIBUTE == 0
        },
        "bootstrap ZIP member is linked or has a special type"
    );
    Ok((name, directory))
}

#[derive(Debug)]
struct VerifiedMiseEngine {
    program: PathBuf,
    program_digest: Digest,
    archive: PathBuf,
    archive_digest: Digest,
    provenance: Digest,
}

impl VerifiedMiseEngine {
    fn derive(
        root: &Path,
        target: Target,
        expected: &Artifact,
        captured: tempfile::NamedTempFile,
    ) -> Result<Self> {
        ensure!(
            expected.subject() == format!("{BOOTSTRAP}@{ENGINE_VERSION}:{}", target.name())
                && expected.target == Some(target),
            "bootstrap archive differs from the Source engine identity"
        );
        let scope = root.join(ENGINE_DIRECTORY);
        fs::create_dir(&scope)?;
        let archive = scope.join(if target == Target::WindowsX64 {
            ENGINE_ARCHIVE
        } else {
            ENGINE_UNIX_FILE
        });
        captured
            .persist_noclobber(&archive)
            .map_err(|error| error.error)?;
        let program = if target == Target::WindowsX64 {
            let program = scope.join(ENGINE_FILE);
            extract_bootstrap(&archive, &program, root)?;
            program
        } else {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                fs::set_permissions(&archive, fs::Permissions::from_mode(PRIVATE_ENGINE_MODE))?;
            }
            archive.clone()
        };
        ensure!(
            Cache::content(&mut File::open(&archive)?)? == expected.expected,
            "Source bootstrap archive changed during extraction"
        );
        let program = regular_program(&program)?;
        Ok(Self {
            program_digest: Cache::content(&mut File::open(&program)?)?,
            program,
            archive,
            archive_digest: expected.expected,
            provenance: expected.provenance,
        })
    }

    fn current(&self, workload: &MiseArchiveWorkload) -> Result<()> {
        let expected = workload
            .bootstrap
            .as_ref()
            .context("Source bootstrap is missing")?;
        ensure!(
            expected.expected == self.archive_digest
                && expected.provenance == self.provenance
                && Cache::content(&mut File::open(&self.archive)?)? == self.archive_digest
                && Cache::content(&mut File::open(&self.program)?)? == self.program_digest,
            "verified mise engine changed before consumption"
        );
        Ok(())
    }
}

#[cfg(windows)]
fn extract_bootstrap(archive: &Path, program: &Path, root: &Path) -> Result<()> {
    let mut inspect = platform::tool::Tool::Pwsh.command();
    inspect
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-CommandWithArgs",
            ZIP_INDEX,
        ])
        .arg(archive)
        .arg(ZIP_MEMBER_LIMIT.to_string());
    let observed = output(inspect, root, "bootstrap-index")?;
    ensure!(
        observed.stderr.is_empty(),
        "bootstrap index produced diagnostics"
    );
    let index = MiseBootstrapIndex::parse(observed.stdout.as_bytes())?;
    let mut extract = platform::tool::Tool::Pwsh.command();
    extract
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-CommandWithArgs",
            ZIP_EXTRACT,
        ])
        .arg(archive)
        .arg(program)
        .arg(ENGINE_ENTRY);
    output(extract, root, "bootstrap-extract")?;
    ensure!(
        fs::metadata(program)?.len() == index.program_bytes()?,
        "extracted bootstrap executable has a different size"
    );
    Ok(())
}

#[cfg(not(windows))]
fn extract_bootstrap(_archive: &Path, _program: &Path, _root: &Path) -> Result<()> {
    anyhow::bail!("Windows bootstrap extraction requires its native host")
}

/// Owned imported release inputs.
/// Pinned native mise still runs installation.
#[derive(Debug)]
pub struct PreparedMiseArchives {
    scope: tempfile::TempDir,
    plan: Digest,
    files: BTreeMap<String, Digest>,
    engine: VerifiedMiseEngine,
}

impl VerifiedPlan {
    /// Copy and verify the exact consumed bytes into mise's supported private download store.
    ///
    /// # Errors
    ///
    /// Refuses changed Source, wrong scope or target, conflicting names, and corrupt materials.
    pub fn prepare_mise_archives(
        self,
        workload: &MiseArchiveWorkload,
        parent: &Path,
    ) -> Result<PreparedMiseArchives> {
        workload.current()?;
        let parent = directory(parent)?;
        ensure!(
            !parent.starts_with(&workload.root),
            "prepared archive inputs must be outside Source"
        );
        ensure!(
            self.digest() == workload.plan.digest()
                && self.artifacts().len() == workload.plan.artifacts().len(),
            "verified archives belong to a different Source scope"
        );
        let scope = tempfile::tempdir_in(parent)?;
        let result = (|| {
            let configuration = scope.path().join(CONFIG_DIRECTORY);
            fs::create_dir(&configuration)?;
            fs::write(
                configuration.join(CONFIGURATION),
                &workload.inputs[CONFIGURATION],
            )?;
            fs::write(configuration.join(LOCK), &workload.inputs[LOCK])?;
            fs::write(scope.path().join(EMPTY_CONFIGURATION), b"")?;
            fs::create_dir(scope.path().join(HOME_DIRECTORY))?;
            let mut total = 0u64;
            let mut names = BTreeMap::new();
            let mut engine = None;
            for (expected, artifact) in workload.plan.artifacts().iter().zip(self.into_artifacts())
            {
                let mut captured = capture_artifact(artifact, expected, scope.path(), &mut total)?;
                if workload
                    .bootstrap
                    .as_ref()
                    .is_some_and(|bootstrap| bootstrap.subject() == expected.subject())
                {
                    engine = Some(VerifiedMiseEngine::derive(
                        scope.path(),
                        workload.target,
                        expected,
                        captured,
                    )?);
                    continue;
                }
                let member = workload
                    .members
                    .iter()
                    .find(|member| member.subject == expected.subject())
                    .context("verified archive member is unselected")?;
                ensure!(
                    member.expected == expected.expected
                        && member.provenance == expected.provenance,
                    "selected archive member identity differs from its verified artifact"
                );
                let hash = blake3_file(captured.as_file_mut())?;
                if let Some(previous) = names.insert(member.expected.hex(), member.filename.clone())
                {
                    ensure!(
                        previous == member.filename,
                        "identical archive content has conflicting original filenames"
                    );
                }
                store(scope.path(), &hash, member, captured)?;
            }
            Ok((
                inventory(scope.path())?,
                engine.context("verified mise bootstrap is missing")?,
            ))
        })();
        match result {
            Ok((files, engine)) => Ok(PreparedMiseArchives {
                scope,
                plan: workload.plan.digest(),
                files,
                engine,
            }),
            Err(error) => {
                platform::cleanup::report(scope.close(), "remove refused mise archive inputs");
                Err(error)
            }
        }
    }
}

fn capture_artifact(
    artifact: VerifiedArtifact,
    expected: &Artifact,
    root: &Path,
    total: &mut u64,
) -> Result<tempfile::NamedTempFile> {
    ensure!(
        artifact.expected() == expected.expected && artifact.provenance() == expected.provenance,
        "verified archive member identity differs"
    );
    let mut captured = tempfile::NamedTempFile::new_in(root)?;
    let mut source = artifact.into_file();
    source.seek(SeekFrom::Start(0))?;
    let copied = std::io::copy(
        &mut source.take(super::cache::ARTIFACT_BYTE_LIMIT + 1),
        captured.as_file_mut(),
    )?;
    ensure!(
        copied <= super::cache::ARTIFACT_BYTE_LIMIT,
        "archive consumption exceeds its byte bound"
    );
    *total = total
        .checked_add(copied)
        .context("archive material size overflows")?;
    ensure!(
        *total <= TOTAL_MATERIAL_BYTES,
        "archive material set exceeds its byte bound"
    );
    ensure!(
        Cache::content(captured.as_file_mut())? == expected.expected,
        "archive changed while capturing its consumption bytes"
    );
    Ok(captured)
}

fn store(
    root: &Path,
    hash: &str,
    member: &Member,
    captured: tempfile::NamedTempFile,
) -> Result<()> {
    let cas = root.join(CACHE_DIRECTORY).join(CAS_DIRECTORY);
    let directory = cas.join("blobs").join(hash);
    fs::create_dir_all(&directory)?;
    let destination = directory.join(&member.filename);
    if destination.exists() {
        ensure!(
            Cache::content(&mut File::open(&destination)?)? == member.expected,
            "existing private archive object is corrupt"
        );
        platform::cleanup::report(captured.close(), "remove repeated mise archive input");
    } else {
        captured
            .persist_noclobber(destination)
            .map_err(|error| error.error)?;
    }
    let pins = cas.join("pins").join(PIN_ALGORITHM);
    fs::create_dir_all(&pins)?;
    let entry = PinEntry {
        blake3: hash.into(),
        effective_filename: Some(member.filename.clone()),
    };
    fs::write(
        pins.join(format!("{}.json", member.expected.hex())),
        serde_json::to_vec(&entry)?,
    )?;
    Ok(())
}

impl PreparedMiseArchives {
    #[must_use]
    pub const fn digest(&self) -> Digest {
        self.plan
    }

    #[must_use]
    pub fn immutable_paths(&self) -> [PathBuf; 4] {
        [
            self.scope.path().join(CONFIG_DIRECTORY),
            self.scope.path().join(CACHE_DIRECTORY).join(CAS_DIRECTORY),
            self.scope.path().join(EMPTY_CONFIGURATION),
            self.scope.path().join(ENGINE_DIRECTORY),
        ]
    }

    fn current(&self, workload: &MiseArchiveWorkload) -> Result<()> {
        workload.current()?;
        ensure!(
            self.plan == workload.plan.digest() && inventory(self.scope.path())? == self.files,
            "prepared release archive inputs changed before native consumption"
        );
        self.engine.current(workload)?;
        Ok(())
    }

    /// Run a fresh locked offline installation with every native policy and exact request.
    ///
    /// # Errors
    ///
    /// Refuses stale or corrupt inputs, the wrong native platform or engine, and actual installer failure.
    pub fn install(&self, workload: &MiseArchiveWorkload) -> Result<InstalledMiseArchives> {
        self.current(workload)?;
        ensure!(
            native_target()? == workload.target,
            "archive installation target differs from this native host"
        );
        ensure!(
            !self.scope.path().join(DATA_DIRECTORY).exists(),
            "archive installation requires a genuinely fresh data directory"
        );
        let mise = &self.engine.program;
        let program_digest = self.engine.program_digest;
        let mut version = self.command(mise);
        version.arg("--version");
        let observed = output(version, self.scope.path(), "version")?;
        ensure!(
            observed.stdout.split_ascii_whitespace().next() == Some(ENGINE_VERSION),
            "mise does not match the pinned download-store ABI"
        );
        let mut command = self.command(mise);
        command.args(["install", "--locked", "--jobs", "2"]);
        for member in &workload.members {
            command.arg(format!("{}@{}", member.tool, member.version));
        }
        ensure!(
            Cache::content(&mut File::open(mise)?)? == program_digest,
            "native mise program changed before installation"
        );
        let installed = output(command, self.scope.path(), "install")?;
        self.current(workload)?;
        let mut paths = BTreeMap::new();
        for member in &workload.members {
            let path = self.installation_path(member)?;
            paths.insert(member.tool.clone(), path);
        }
        ensure!(
            Cache::content(&mut File::open(mise)?)? == program_digest,
            "native mise program changed during installation"
        );
        Ok(InstalledMiseArchives {
            program: mise.clone(),
            program_digest,
            paths,
            plan: self.plan,
            operations: vec![observed, installed],
        })
    }

    fn command(&self, program: &Path) -> Command {
        archive_command(self.scope.path(), program)
    }

    fn installation_path(&self, member: &Member) -> Result<PathBuf> {
        let data = directory(&self.scope.path().join(DATA_DIRECTORY))?;
        let mut command = self.command(&self.engine.program);
        command.args(["where", &format!("{}@{}", member.tool, member.version)]);
        let resolved = output(command, self.scope.path(), "where")?;
        let selected = platform::paths::canonical(Path::new(resolved.stdout.trim()))?;
        let path = directory(&selected)?;
        ensure!(
            path.starts_with(data),
            "mise selected an ambient installation"
        );
        Ok(path)
    }

    fn require_installation_path(&self, member: &Member, expected: &Path) -> Result<()> {
        let physical = directory(expected)?;
        ensure!(
            physical == expected && self.installation_path(member)? == physical,
            "selected installation path changed before live health"
        );
        Ok(())
    }
}

fn archive_command(root: &Path, program: &Path) -> Command {
    let home = root.join(HOME_DIRECTORY);
    let mut command = platform::tool::external(program);
    for (name, _) in std::env::vars_os() {
        let text = name.to_string_lossy().to_ascii_uppercase();
        if ["MISE_", "__MISE_", "RUSTUP_", "UV_", "DOPPLER_"]
            .iter()
            .any(|prefix| text.starts_with(prefix))
            || matches!(
                text.as_str(),
                "GITHUB_TOKEN" | "GH_TOKEN" | "GH_CONFIG_DIR" | "GITLAB_TOKEN"
            )
        {
            command.env_remove(name);
        }
    }
    command
        .current_dir(root.join(CONFIG_DIRECTORY))
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("APPDATA", home.join("AppData/Roaming"))
        .env("LOCALAPPDATA", home.join("AppData/Local"))
        .env("MISE_DATA_DIR", root.join(DATA_DIRECTORY))
        .env("MISE_CACHE_DIR", root.join(CACHE_DIRECTORY))
        .env("MISE_STATE_DIR", root.join(STATE_DIRECTORY))
        .env("MISE_CONFIG_DIR", home.join(".config/mise"))
        .env("MISE_GLOBAL_CONFIG_FILE", root.join(EMPTY_CONFIGURATION))
        .env("MISE_SYSTEM_CONFIG_FILE", root.join(EMPTY_CONFIGURATION))
        .env("MISE_CEILING_PATHS", root)
        .env("MISE_TRUSTED_CONFIG_PATHS", root.join(CONFIG_DIRECTORY))
        .env("MISE_OFFLINE", "true")
        .env("MISE_LOCKED", "true")
        .env("MISE_AUTO_INSTALL", "false")
        .env("MISE_AUTO_PRUNE", "false")
        .env("MISE_DOWNLOAD_CACHE", "true")
        .env("MISE_PARANOID", "true");
    command
}

/// Actual selected release installations, without Code, Real, or whole-native completion.
#[derive(Debug)]
pub struct InstalledMiseArchives {
    program: PathBuf,
    program_digest: Digest,
    paths: BTreeMap<String, PathBuf>,
    plan: Digest,
    operations: Vec<MiseOperation>,
}

impl InstalledMiseArchives {
    #[must_use]
    pub fn paths(&self) -> &BTreeMap<String, PathBuf> {
        &self.paths
    }

    #[must_use]
    pub fn operations(&self) -> &[MiseOperation] {
        &self.operations
    }

    /// Run an independent live health operation through the freshly installed selected scope.
    ///
    /// # Errors
    ///
    /// Refuses a different prepared scope, unselected tool, changed materials, or failed health operation.
    pub fn health(
        &self,
        prepared: &PreparedMiseArchives,
        workload: &MiseArchiveWorkload,
        tool: &str,
        arguments: &[&str],
    ) -> Result<String> {
        ensure!(
            self.plan == prepared.plan && self.paths.contains_key(tool),
            "health operation belongs to a different archive scope"
        );
        prepared.current(workload)?;
        let member = workload
            .members
            .iter()
            .find(|member| member.tool == tool)
            .context("health tool is unselected")?;
        let expected = self
            .paths
            .get(tool)
            .context("installed health path is missing")?;
        prepared.require_installation_path(member, expected)?;
        let mut command = prepared.command(&self.program);
        command
            .args(["exec", &format!("{tool}@{}", member.version), "--"])
            .args(arguments);
        ensure!(
            Cache::content(&mut File::open(&self.program)?)? == self.program_digest,
            "native mise program changed before health checking"
        );
        let result = output(command, prepared.scope.path(), "health")?;
        prepared.current(workload)?;
        prepared.require_installation_path(member, expected)?;
        Ok(result.stdout)
    }
}

fn source_inputs(root: &Path) -> Result<BTreeMap<String, String>> {
    INPUT_NAMES
        .into_iter()
        .map(|name| Ok((name.into(), text(&root.join(name))?)))
        .collect()
}

fn directory(path: &Path) -> Result<PathBuf> {
    ensure!(
        path.is_absolute(),
        "archive scope requires an absolute path"
    );
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor)?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "archive scope has a nonregular ancestor"
        );
    }
    Ok(platform::paths::canonical(path)?)
}

fn regular_program(path: &Path) -> Result<PathBuf> {
    let parent = path.parent().context("mise program requires its parent")?;
    directory(parent)?;
    ensure!(
        fs::symlink_metadata(path)?.is_file(),
        "mise program requires a regular file"
    );
    Ok(platform::paths::canonical(path)?)
}

fn text(path: &Path) -> Result<String> {
    directory(path.parent().context("archive input requires its parent")?)?;
    ensure!(
        fs::symlink_metadata(path)?.is_file(),
        "archive input requires a regular file"
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take(SOURCE_BYTE_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= SOURCE_BYTE_LIMIT,
        "archive input exceeds its byte bound"
    );
    String::from_utf8(bytes).context("archive input requires UTF-8")
}

fn blake3_file(file: &mut File) -> Result<String> {
    file.seek(SeekFrom::Start(0))?;
    let mut hash = blake3::Hasher::new();
    let mut buffer = [0; COPY_BYTES];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    file.seek(SeekFrom::Start(0))?;
    Ok(hash.finalize().to_hex().to_string())
}

fn inventory(root: &Path) -> Result<BTreeMap<String, Digest>> {
    let mut files = BTreeMap::new();
    for name in [CONFIG_DIRECTORY, CAS_DIRECTORY, ENGINE_DIRECTORY] {
        let path = if name == CAS_DIRECTORY {
            root.join(CACHE_DIRECTORY).join(name)
        } else {
            root.join(name)
        };
        scan(root, &path, &mut files)?;
    }
    files.insert(
        EMPTY_CONFIGURATION.into(),
        Digest::of(text(&root.join(EMPTY_CONFIGURATION))?.as_bytes()),
    );
    Ok(files)
}

fn scan(root: &Path, path: &Path, files: &mut BTreeMap<String, Digest>) -> Result<()> {
    directory(path)?;
    ensure!(
        path.strip_prefix(root)?.components().count() <= STORE_DEPTH_LIMIT,
        "private download store exceeds its depth bound"
    );
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "private download store contains a symlink"
        );
        if metadata.is_dir() {
            scan(root, &entry.path(), files)?;
        } else {
            ensure!(
                metadata.is_file() && files.len() < PRIVATE_STORE_SCAN_FILE_LIMIT,
                "private download store exceeds its file bound"
            );
            let path = entry.path();
            let relative = path
                .strip_prefix(root)?
                .to_str()
                .context("private material path requires Unicode")?
                .replace('\\', "/");
            if path.starts_with(root.join(CACHE_DIRECTORY).join(CAS_DIRECTORY).join("pins"))
                && path
                    .extension()
                    .and_then(std::ffi::OsStr::to_str)
                    .is_some_and(|extension| extension.eq_ignore_ascii_case(JSON_EXTENSION))
            {
                let mut bytes = Vec::new();
                File::open(&path)?
                    .take(PIN_BYTE_LIMIT as u64 + 1)
                    .read_to_end(&mut bytes)?;
                MiseDownloadPin::parse(&bytes)?;
            }
            files.insert(relative, Cache::content(&mut File::open(path)?)?);
        }
    }
    Ok(())
}

/// Natural output from one native operation, without reusable verification credit.
#[derive(Debug, Serialize)]
pub struct MiseOperation {
    name: String,
    status: Option<i32>,
    stdout: String,
    stderr: String,
}

fn output(mut command: Command, scope: &Path, name: &str) -> Result<MiseOperation> {
    let raw = tempfile::tempdir_in(scope)?;
    let result = (|| {
        let stdout = raw.path().join("stdout");
        let stderr = raw.path().join("stderr");
        command
            .stdin(Stdio::null())
            .stdout(File::create(&stdout)?)
            .stderr(File::create(&stderr)?);
        let mut child = platform::process::mise_install(command)?.spawn()?;
        let start = Instant::now();
        let waited: Result<_> = (|| loop {
            ensure!(
                fs::metadata(&stdout)?.len() <= PROCESS_BYTE_LIMIT
                    && fs::metadata(&stderr)?.len() <= PROCESS_BYTE_LIMIT
                    && start.elapsed() <= INSTALL_LIMIT,
                "mise archive operation exceeded its bound"
            );
            if let Some(status) = child.try_wait()? {
                break Ok(status);
            }
            std::thread::sleep(PROCESS_POLL);
        })();
        if waited.is_err() {
            platform::cleanup::report(child.kill(), "stop the bounded mise archive operation");
            platform::cleanup::report(child.wait(), "reap the bounded mise archive operation");
        }
        let status = waited?;
        let stdout = text(&stdout)?;
        let stderr = text(&stderr)?;
        ensure!(
            status.success(),
            "mise archive {name} failed: {status}; stdout={stdout}; stderr={stderr}"
        );
        Ok(MiseOperation {
            name: name.into(),
            status: status.code(),
            stdout,
            stderr,
        })
    })();
    platform::cleanup::report(raw.close(), "remove captured mise archive operation output");
    result
}

fn native_target() -> Result<Target> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Ok(Target::WindowsX64),
        ("linux", "x86_64") => Ok(Target::LinuxX64),
        ("linux", "aarch64") => Ok(Target::LinuxArm64),
        ("macos", "x86_64") => Ok(Target::MacosX64),
        ("macos", "aarch64") => Ok(Target::MacosArm64),
        _ => anyhow::bail!("archive installation has an unsupported native target"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODE: &str = "DOTFILES_ARCHIVE_ENV_TEST_MODE";
    const SCOPE: &str = "DOTFILES_ARCHIVE_ENV_TEST_SCOPE";
    const TEST: &str =
        "artifacts::mise_consume::tests::inherited_override_names_cannot_reach_the_native_child";
    const OVERRIDES: [&str; 6] = [
        "mise_config_file",
        "mise_env",
        "__mise_diff",
        "rustup_home",
        "uv_cache_dir",
        "gh_config_dir",
    ];

    #[test]
    fn inherited_override_names_cannot_reach_the_native_child() -> Result<()> {
        match std::env::var(MODE).ok().as_deref() {
            Some("inspect") => {
                for name in OVERRIDES {
                    ensure!(
                        std::env::var_os(name).is_none(),
                        "inherited override {name} reached the native child"
                    );
                }
                let root =
                    PathBuf::from(std::env::var_os(SCOPE).context("owned scope is missing")?);
                assert_eq!(
                    std::env::var_os("HOME"),
                    Some(root.join(HOME_DIRECTORY).into())
                );
                assert_eq!(
                    std::env::var_os("MISE_GLOBAL_CONFIG_FILE"),
                    Some(root.join(EMPTY_CONFIGURATION).into())
                );
                for (name, expected) in [
                    ("MISE_OFFLINE", "true"),
                    ("MISE_LOCKED", "true"),
                    ("MISE_PARANOID", "true"),
                    ("MISE_AUTO_INSTALL", "false"),
                    ("MISE_AUTO_PRUNE", "false"),
                ] {
                    assert_eq!(std::env::var(name).as_deref(), Ok(expected));
                }
            }
            Some("prepare") => {
                let scope = tempfile::tempdir()?;
                let result: Result<()> = (|| {
                    let root = platform::paths::canonical(scope.path())?;
                    fs::create_dir(root.join(CONFIG_DIRECTORY))?;
                    fs::create_dir(root.join(HOME_DIRECTORY))?;
                    fs::write(root.join(EMPTY_CONFIGURATION), b"")?;
                    let mut command = archive_command(&root, &std::env::current_exe()?);
                    command
                        .args(["--exact", TEST, "--nocapture"])
                        .env(MODE, "inspect")
                        .env(SCOPE, &root);
                    let observed = platform::process::mise_install(command)?.output()?;
                    ensure!(
                        observed.status.success(),
                        "native environment probe failed: {}; stderr={}",
                        observed.status,
                        String::from_utf8_lossy(&observed.stderr)
                    );
                    Ok(())
                })();
                platform::cleanup::report(
                    scope.close(),
                    "remove the native environment test scope",
                );
                result?;
            }
            None => {
                let mut command = platform::process::command(std::env::current_exe()?);
                command
                    .args(["--exact", TEST, "--nocapture"])
                    .env(MODE, "prepare");
                for name in OVERRIDES {
                    command.env(name, "owned-test-override");
                }
                let observed = command.output()?;
                ensure!(
                    observed.status.success(),
                    "controlled inherited environment test failed: {}; stderr={}",
                    observed.status,
                    String::from_utf8_lossy(&observed.stderr)
                );
            }
            Some(_) => anyhow::bail!("native environment test mode is unknown"),
        }
        Ok(())
    }
}

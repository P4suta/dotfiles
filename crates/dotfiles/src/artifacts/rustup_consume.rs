use super::cargo_tool_consume::{
    NativeMaterialFailure, NativeMaterialOperation, PreparedCargoTool, capture, operation, physical,
};
use super::{Artifact, Cache, Digest, Fetch, Plan, Target, VerifiedArtifact};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const CATALOG: &str = "policy/native-materials.toml";
const SOURCE_INPUTS: [&str; 3] = ["mise.toml", "mise.lock", CATALOG];
const INPUT_BYTES: u64 = 8 * 1024 * 1024;
const MATERIALS: usize = 128;
const COMPONENTS: usize = 32;
const RUSTUP_SERVER: &str = "https://static.rust-lang.org";
const CRATE_SERVER: &str = "https://static.crates.io/crates";
const KANI_RELEASES: &str = "https://github.com/model-checking/kani/releases/download";
const KANI_TOOL: &str = "cargo:kani-verifier";
const HOST_STD: &str = "rust-std";
const CORE_COMPONENTS: [&str; 3] = ["cargo", "rustc", HOST_STD];
const NIGHTLY_COMPONENTS: [&str; 2] = ["miri", "rust-src"];
const BUNDLE_ENTRIES: usize = 65_536;
const BUNDLE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const METADATA_BYTES: u64 = 4096;
const TAR: &str = "/usr/bin/tar";
const ARCHIVE_BUFFER: usize = 8192;
const INVENTORY_DEPTH: usize = 128;
const REPLAY_BUNDLE_BYTES: u64 = 256 * 1024;
const REPLAY_BUNDLE_ENTRIES: usize = 64;
const REPLAY_COMPRESSED_BYTES: usize = 16 * 1024;
const STDLIB_CAPTURE: &str = "cargo-lock:rust-src";
const STDLIB_CAPTURE_FILE: &str = "rust-src-library-Cargo.lock";
const RUST_SRC_COMPONENT: &str = "rust-src";
const STDLIB_MEMBER: &str = "rust-src-nightly/rust-src/lib/rustlib/src/rust/library/Cargo.lock";
const INSTALLED_STDLIB_LOCK: &str = "lib/rustlib/src/rust/library/Cargo.lock";
const STDLIB_ORIGIN: &str = "dotfiles-authenticated-rust-src-cargo-lock-v1";
const STDLIB_REPLAY_INPUT: &str = "diagnostic-rust-src";
const DATE_YEAR_DIGITS: usize = 4;
const DATE_MONTH_DIGITS: usize = 2;
const DATE_DAY_DIGITS: usize = 2;
#[cfg(unix)]
const BOOTSTRAP_EXECUTABLE_MODE: u32 = 0o700;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssetPin {
    url: String,
    sha256: Digest,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BootstrapPin {
    version: String,
    targets: BTreeMap<String, AssetPin>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DistributionPin {
    channel: String,
    url: String,
    sha256: Digest,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct KaniPin {
    version: String,
    launcher_sha256: Digest,
    targets: BTreeMap<String, AssetPin>,
}

/// Untrusted policy data.
/// This value proves neither source nor installation authenticity.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeRustCatalog {
    schema: u32,
    rustup: BootstrapPin,
    stable: DistributionPin,
    nightly: DistributionPin,
    kani: KaniPin,
}

impl NativeRustCatalog {
    /// Decode bounded native material metadata without granting origin or readiness.
    ///
    /// # Errors
    ///
    /// Rejects unknown fields, malformed identities, URLs, digests, and empty target sets.
    pub fn parse(text: &str) -> Result<Self> {
        ensure!(
            text.len() as u64 <= INPUT_BYTES,
            "native material catalog exceeds its bound"
        );
        let catalog: Self = toml::from_str(text)?;
        ensure!(
            catalog.schema == 1
                && rules::artifact_rules::pinned(catalog.rustup.version.as_bytes())
                && rules::artifact_rules::pinned(catalog.stable.channel.as_bytes())
                && rules::artifact_rules::pinned(catalog.kani.version.as_bytes()),
            "native material catalog has invalid pinned identities"
        );
        let date = catalog
            .nightly
            .channel
            .strip_prefix("nightly-")
            .context("native compiler nightly requires a date")?;
        ensure!(
            valid_date(date),
            "native compiler nightly has an invalid date"
        );
        ensure!(
            !catalog.rustup.targets.is_empty() && !catalog.kani.targets.is_empty(),
            "native material catalog lacks target identities"
        );
        for (target, asset) in &catalog.rustup.targets {
            let target = selected_target(target)?;
            let suffix = if target == Target::WindowsX64 {
                ".exe"
            } else {
                ""
            };
            ensure!(
                asset.url
                    == format!(
                        "{RUSTUP_SERVER}/rustup/archive/{}/{}/rustup-init{suffix}",
                        catalog.rustup.version,
                        triple(target)
                    ),
                "Rustup bootstrap URL differs from its pinned release and target"
            );
        }
        ensure!(
            catalog.stable.url
                == format!(
                    "{RUSTUP_SERVER}/dist/channel-rust-{}.toml",
                    catalog.stable.channel
                )
                && catalog.nightly.url
                    == format!("{RUSTUP_SERVER}/dist/{date}/channel-rust-nightly.toml"),
            "Rust manifests differ from their immutable channel identities"
        );
        for (target, asset) in &catalog.kani.targets {
            let target = selected_target(target)?;
            ensure!(
                target != Target::WindowsX64
                    && asset.url
                        == format!(
                            "{KANI_RELEASES}/kani-{0}/kani-{0}-{1}.tar.gz",
                            catalog.kani.version,
                            triple(target)
                        ),
                "Kani release URL differs from its pin or native target"
            );
        }
        Ok(catalog)
    }
}

fn valid_date(date: &str) -> bool {
    let mut parts = date.split('-');
    matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some(year), Some(month), Some(day), None)
            if year.len() == DATE_YEAR_DIGITS
                && month.len() == DATE_MONTH_DIGITS
                && day.len() == DATE_DAY_DIGITS
                && year.bytes().chain(month.bytes()).chain(day.bytes()).all(|byte| byte.is_ascii_digit())
                && ("01"..="12").contains(&month)
                && ("01"..="31").contains(&day)
    )
}

fn selected_target(name: &str) -> Result<Target> {
    Target::ALL
        .into_iter()
        .find(|target| target.name() == name)
        .context("native material catalog has an unknown target")
}

pub(super) const fn triple(target: Target) -> &'static str {
    match target {
        Target::LinuxX64 => "x86_64-unknown-linux-gnu",
        Target::LinuxArm64 => "aarch64-unknown-linux-gnu",
        Target::MacosX64 => "x86_64-apple-darwin",
        Target::MacosArm64 => "aarch64-apple-darwin",
        Target::WindowsX64 => "x86_64-pc-windows-msvc",
    }
}

fn text(path: &Path) -> Result<String> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= INPUT_BYTES,
        "native source input is linked, missing or oversized"
    );
    Ok(fs::read_to_string(path)?)
}

/// Source-bound Rust and Kani materials.
/// Other native providers keep their own obligations.
#[derive(Debug)]
pub struct RustKaniWorkload {
    root: PathBuf,
    target: Target,
    inputs: BTreeMap<String, String>,
    catalog: NativeRustCatalog,
    stable_components: Vec<String>,
}

type PendingLauncher = (Artifact, VerifiedArtifact);

/// Fetch selected native compiler and verifier material through the maintained transport.
///
/// # Errors
///
/// Rejects incomplete source pins, unresolved materials, and acquisition or capture failures.
pub fn prepare_source_rust_kani(
    root: &Path,
    home: &Path,
    parent: &Path,
) -> Result<PreparedRustKani> {
    RustKaniWorkload::capture(root, current_target()?)?.prepare(
        &Cache::new(home.join(super::consume::ACQUISITION_CACHE)),
        &super::transport::Curl,
        parent,
    )
}

impl RustKaniWorkload {
    /// Capture the actual project pins and approved material catalog before any installation.
    ///
    /// # Errors
    ///
    /// Refuses missing source, catalog drift, mismatched provider/version requests and malformed components.
    pub fn capture(root: &Path, target: Target) -> Result<Self> {
        let root = physical(root)?;
        let inputs = SOURCE_INPUTS
            .into_iter()
            .map(|name| Ok((name.into(), text(&root.join(name))?)))
            .collect::<Result<BTreeMap<String, String>>>()?;
        let catalog = NativeRustCatalog::parse(&inputs[CATALOG])?;
        let project: toml::Table = toml::from_str(&inputs["mise.toml"])?;
        let rust = project
            .get("tools")
            .and_then(|tools| tools.get("rust"))
            .context("native Rust preparation requires the project Rust pin")?;
        selected_options(rust, target, &["version", "components", "os"])?;
        ensure!(
            rust.get("version").and_then(toml::Value::as_str)
                == Some(catalog.stable.channel.as_str()),
            "native Rust catalog differs from the project compiler pin"
        );
        let mut stable_components: Vec<String> = rust
            .get("components")
            .cloned()
            .map(toml::Value::try_into)
            .transpose()?
            .unwrap_or_default();
        let requested_components = stable_components.join(",");
        stable_components.extend(CORE_COMPONENTS.map(str::to_owned));
        stable_components.sort();
        stable_components.dedup();
        ensure!(
            stable_components.len() <= COMPONENTS
                && stable_components
                    .iter()
                    .all(|name| rules::artifact_rules::portable_component(name.as_bytes())),
            "native Rust components are malformed or exceed their bound"
        );
        let lock: toml::Table = toml::from_str(&inputs["mise.lock"])?;
        let rust_lock = require_locked(&lock, "rust", &catalog.stable.channel, "core:rust")?;
        let options = rust_lock
            .get("options")
            .map(|options| {
                options
                    .as_table()
                    .context("Rust lock options are malformed")
            })
            .transpose()?;
        ensure!(
            options.is_none_or(|options| options.keys().all(|name| name == "components"))
                && if requested_components.is_empty() {
                    options.is_none_or(toml::Table::is_empty)
                } else {
                    options
                        .and_then(|options| options.get("components"))
                        .and_then(toml::Value::as_str)
                        == Some(requested_components.as_str())
                },
            "Rust component lock differs from the source request"
        );
        if target != Target::WindowsX64 {
            let kani = project
                .get("tools")
                .and_then(|tools| tools.get(KANI_TOOL))
                .context("native verifier preparation requires its project launcher pin")?;
            selected_options(kani, target, &["version", "os"])?;
            ensure!(
                kani.get("version").and_then(toml::Value::as_str)
                    == Some(catalog.kani.version.as_str()),
                "native Kani catalog differs from the project verifier pin"
            );
            let kani_lock = require_locked(&lock, KANI_TOOL, &catalog.kani.version, KANI_TOOL)?;
            ensure!(
                kani_lock
                    .get("options")
                    .is_none_or(|options| options.as_table().is_some_and(toml::Table::is_empty)),
                "Kani launcher options require an unsupported material scope"
            );
        }
        Ok(Self {
            root,
            target,
            inputs,
            catalog,
            stable_components,
        })
    }

    fn current(&self) -> Result<()> {
        ensure!(
            physical(&self.root)? == self.root,
            "native source root changed"
        );
        for (name, expected) in &self.inputs {
            ensure!(
                text(&self.root.join(name))? == *expected,
                "native source input changed: {name}"
            );
        }
        Ok(())
    }

    fn artifact(
        &self,
        subject: &str,
        version: &str,
        url: &str,
        sha256: Digest,
    ) -> Result<Artifact> {
        let provenance = Digest::of(&serde_json::to_vec(&(
            &self.inputs,
            self.target.name(),
            subject,
            version,
            url,
            sha256,
        ))?);
        Ok(Artifact {
            subject: subject.into(),
            target: Some(self.target),
            version: version.into(),
            source: url.into(),
            expected: sha256,
            provenance,
        })
    }

    fn plan(&self, artifacts: Vec<Artifact>) -> Result<Plan> {
        ensure!(
            artifacts.len() <= MATERIALS,
            "native material inventory exceeds its bound"
        );
        Plan::native_materials(
            "source-bound-rust-kani-materials",
            self.inputs
                .iter()
                .map(|(name, text)| (name.clone(), Digest::of(text.as_bytes())))
                .collect(),
            artifacts,
        )
    }

    fn initial(&self) -> Result<Plan> {
        let bootstrap = self
            .catalog
            .rustup
            .targets
            .get(self.target.name())
            .context("Rustup bootstrap is missing for this target")?;
        let mut assets = vec![
            self.artifact(
                "rustup-bootstrap",
                &self.catalog.rustup.version,
                &bootstrap.url,
                bootstrap.sha256,
            )?,
            self.artifact(
                "rust-manifest:stable",
                &self.catalog.stable.channel,
                &self.catalog.stable.url,
                self.catalog.stable.sha256,
            )?,
        ];
        if self.target != Target::WindowsX64 {
            let bundle = self
                .catalog
                .kani
                .targets
                .get(self.target.name())
                .context("Kani bundle is missing for this target")?;
            assets.extend([
                self.artifact(
                    "rust-manifest:nightly",
                    &self.catalog.nightly.channel,
                    &self.catalog.nightly.url,
                    self.catalog.nightly.sha256,
                )?,
                self.artifact(
                    "kani-bundle",
                    &self.catalog.kani.version,
                    &bundle.url,
                    bundle.sha256,
                )?,
                self.artifact(
                    &format!("crate:kani-verifier@{}", self.catalog.kani.version),
                    &self.catalog.kani.version,
                    &format!(
                        "{CRATE_SERVER}/kani-verifier/kani-verifier-{}.crate",
                        self.catalog.kani.version
                    ),
                    self.catalog.kani.launcher_sha256,
                )?,
            ]);
        }
        self.plan(assets)
    }

    /// Fetch and use all selected materials before the native installer runs.
    ///
    /// # Errors
    ///
    /// Rejects missing, poisoned, mismatched, or incomplete material and changed source.
    pub fn prepare(
        self,
        cache: &Cache,
        fetch: &impl Fetch,
        parent: &Path,
    ) -> Result<PreparedRustKani> {
        self.current()?;
        let parent = physical(parent)?;
        ensure!(
            !parent.starts_with(&self.root),
            "native prepared materials must stay outside source"
        );
        let owned = tempfile::Builder::new()
            .prefix("verified-rust-kani-")
            .tempdir_in(parent)?;
        super::cargo_tool_consume::prepare_output_directory(owned.path())?;
        let (mut captured, pending_launcher) = self.capture_inputs(cache, fetch, owned.path())?;
        let input = owned.path().join("inputs");
        let stable = RustDistributionMetadata::parse(
            &text(&captured["rust-manifest:stable"])?,
            triple(self.target),
            &self.stable_components,
        )?;
        ensure!(
            stable.rust_version.split_ascii_whitespace().next()
                == Some(self.catalog.stable.channel.as_str()),
            "Rust manifest version differs from the pinned compiler"
        );
        let mut components = distribution_artifacts(&self, &self.catalog.stable, &stable)?;
        let nightly = if self.target == Target::WindowsX64 {
            None
        } else {
            let requested: Vec<String> = CORE_COMPONENTS
                .into_iter()
                .chain(NIGHTLY_COMPONENTS)
                .map(str::to_owned)
                .collect();
            let nightly = RustDistributionMetadata::parse(
                &text(&captured["rust-manifest:nightly"])?,
                triple(self.target),
                &requested,
            )?;
            ensure!(
                nightly.date == self.catalog.nightly.channel.trim_start_matches("nightly-"),
                "nightly manifest date differs from the source pin"
            );
            let metadata = bundle_metadata(&captured["kani-bundle"])?;
            self.require_bundle_distribution(&metadata, &nightly)?;
            components.extend(distribution_artifacts(
                &self,
                &self.catalog.nightly,
                &nightly,
            )?);
            Some(nightly)
        };
        let launcher = pending_launcher
            .map(|(identity, acquired)| {
                PreparedCargoTool::prepare(owned.path(), &identity, acquired, cache, fetch)
            })
            .transpose()?;
        let components = self.plan(components)?;
        let verified = cache.acquire(&components, fetch)?;
        ensure!(
            verified.digest() == components.digest(),
            "Rust component acquisition differs"
        );
        let (mirror, stdlib) = self.capture_component_mirror(
            &components,
            verified,
            nightly.as_ref(),
            &mut captured,
            owned.path(),
        )?;
        executable(&captured["rustup-bootstrap"])?;
        let input_files = super::cargo_tool_consume::tree(&input)?;
        let mirror_files = super::cargo_tool_consume::tree(&mirror)?;
        self.current()?;
        Ok(PreparedRustKani {
            workload: self,
            owned,
            captured,
            mirror,
            input_files,
            mirror_files,
            launcher,
            stable,
            nightly,
            stdlib,
        })
    }

    fn capture_component_mirror(
        &self,
        components: &Plan,
        verified: super::VerifiedPlan,
        nightly: Option<&RustDistributionMetadata>,
        captured: &mut BTreeMap<String, PathBuf>,
        owned: &Path,
    ) -> Result<(PathBuf, Option<AuthenticatedStdlibLock>)> {
        let mirror = owned.join("mirror");
        fs::create_dir(&mirror)?;
        put_manifest(
            &mirror,
            &self.catalog.stable,
            &captured["rust-manifest:stable"],
        )?;
        if nightly.is_some() {
            put_manifest(
                &mirror,
                &self.catalog.nightly,
                &captured["rust-manifest:nightly"],
            )?;
        }
        let mut stdlib = None;
        for (identity, acquired) in components.artifacts().iter().zip(verified.into_artifacts()) {
            let path = mirror.join(distribution_path(&identity.source)?);
            fs::create_dir_all(
                path.parent()
                    .context("Rust component has no mirror parent")?,
            )?;
            capture(identity, acquired, &path)?;
            if let Some(metadata) = nightly
                && metadata.assets.iter().any(|asset| {
                    asset.component == RUST_SRC_COMPONENT
                        && asset.url.ends_with(".tar.gz")
                        && asset.url == identity.source
                })
            {
                ensure!(stdlib.is_none(), "nightly rust-src gzip input is repeated");
                let selected = AuthenticatedStdlibLock::capture(
                    self,
                    metadata,
                    identity,
                    path,
                    &captured["rust-manifest:nightly"],
                    owned.join("inputs").join(STDLIB_CAPTURE_FILE),
                )?;
                ensure!(
                    captured
                        .insert(STDLIB_CAPTURE.into(), selected.lock_path.clone())
                        .is_none(),
                    "selected stdlib input is repeated"
                );
                stdlib = Some(selected);
            }
        }
        ensure!(
            nightly.is_none() || stdlib.is_some(),
            "nightly rust-src has no captured Cargo lock"
        );
        Ok((mirror, stdlib))
    }

    fn capture_inputs(
        &self,
        cache: &Cache,
        fetch: &impl Fetch,
        owned: &Path,
    ) -> Result<(BTreeMap<String, PathBuf>, Option<PendingLauncher>)> {
        let plan = self.initial()?;
        let verified = cache.acquire(&plan, fetch)?;
        ensure!(
            verified.digest() == plan.digest(),
            "native acquisition belongs to a different workload"
        );
        let input = owned.join("inputs");
        fs::create_dir(&input)?;
        let mut captured = BTreeMap::new();
        let mut launcher = None;
        for (identity, acquired) in plan.artifacts().iter().zip(verified.into_artifacts()) {
            if identity.subject.starts_with("crate:") {
                launcher = Some((identity.clone(), acquired));
            } else {
                let name = if identity.subject == "rustup-bootstrap" {
                    if self.target == Target::WindowsX64 {
                        "rustup-init.exe".into()
                    } else {
                        "rustup-init".into()
                    }
                } else {
                    identity.subject.replace(':', "-")
                };
                let path = input.join(name);
                capture(identity, acquired, &path)?;
                captured.insert(identity.subject.clone(), path);
            }
        }
        Ok((captured, launcher))
    }

    fn require_bundle_distribution(
        &self,
        metadata: &KaniBundleMetadata,
        nightly: &RustDistributionMetadata,
    ) -> Result<()> {
        ensure!(
            metadata.channel == format!("{}-{}", self.catalog.nightly.channel, triple(self.target))
                && metadata.rustc == format!("rustc {}", nightly.rustc_version),
            "Kani bundle and pinned compiler distribution differ"
        );
        Ok(())
    }
}

fn require_locked<'a>(
    lock: &'a toml::Table,
    name: &str,
    version: &str,
    backend: &str,
) -> Result<&'a toml::Value> {
    let versions = lock
        .get("tools")
        .and_then(|tools| tools.get(name))
        .and_then(toml::Value::as_array)
        .context("native compiler or verifier is not locked")?;
    let matching: Vec<_> = versions
        .iter()
        .filter(|tool| tool.get("version").and_then(toml::Value::as_str) == Some(version))
        .collect();
    let [tool] = matching.as_slice() else {
        anyhow::bail!("native tool lock is missing or ambiguous");
    };
    ensure!(
        tool.get("backend").and_then(toml::Value::as_str) == Some(backend)
            && tool
                .get("specifiers")
                .and_then(toml::Value::as_array)
                .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(version))),
        "native tool lock differs from its pinned backend/request"
    );
    Ok(tool)
}

fn selected_options(pin: &toml::Value, target: Target, supported: &[&str]) -> Result<()> {
    let options = pin
        .as_table()
        .context("native tool pin must retain its exact option table")?;
    ensure!(
        options
            .keys()
            .all(|name| supported.contains(&name.as_str())),
        "native tool has unsupported installation options"
    );
    if let Some(operating_systems) = options.get("os") {
        let operating_systems = operating_systems
            .as_array()
            .context("native tool OS scope must be a string array")?;
        let current = match target {
            Target::WindowsX64 => "windows",
            Target::LinuxX64 | Target::LinuxArm64 => "linux",
            Target::MacosX64 | Target::MacosArm64 => "macos",
        };
        ensure!(
            operating_systems
                .iter()
                .all(|value| matches!(value.as_str(), Some("windows" | "linux" | "macos")))
                && operating_systems
                    .iter()
                    .any(|value| value.as_str() == Some(current)),
            "native tool is malformed or outside the selected OS scope"
        );
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
struct ComponentAsset {
    component: String,
    url: String,
    sha256: Digest,
}

/// Data-only decoding of the original Rust release manifest.
#[derive(Debug, Serialize)]
pub struct RustDistributionMetadata {
    date: String,
    rust_version: String,
    rustc_version: String,
    versions: BTreeMap<String, String>,
    assets: Vec<ComponentAsset>,
}

impl RustDistributionMetadata {
    /// Select every supported compression variant of each requested component.
    ///
    /// # Errors
    ///
    /// Rejects unavailable components, unknown hosts, malformed manifests, and unsafe URLs.
    pub fn parse(text: &str, host: &str, components: &[String]) -> Result<Self> {
        ensure!(
            text.len() as u64 <= INPUT_BYTES
                && !components.is_empty()
                && components.len() <= COMPONENTS
                && Target::ALL.iter().any(|target| triple(*target) == host),
            "Rust distribution input or component scope is invalid"
        );
        let manifest: toml::Table = toml::from_str(text)?;
        ensure!(
            manifest
                .get("manifest-version")
                .and_then(toml::Value::as_str)
                == Some("2"),
            "Rust manifest version is unsupported"
        );
        let date = manifest
            .get("date")
            .and_then(toml::Value::as_str)
            .context("Rust manifest has no date")?;
        ensure!(valid_date(date), "Rust manifest has an invalid date");
        let packages = manifest
            .get("pkg")
            .and_then(toml::Value::as_table)
            .context("Rust manifest has no package inventory")?;
        let release = package_version(packages, "rust")?;
        let compiler = package_version(packages, "rustc")?;
        let mut assets = Vec::new();
        let mut selected = BTreeSet::new();
        let mut versions = BTreeMap::new();
        for component in components {
            ensure!(
                rules::artifact_rules::portable_component(component.as_bytes())
                    && selected.insert(component),
                "Rust component names are invalid or repeated"
            );
            let renamed = manifest
                .get("renames")
                .and_then(|renames| renames.get(component))
                .and_then(|rename| rename.get("to"))
                .and_then(toml::Value::as_str)
                .unwrap_or(component);
            versions.insert(component.clone(), package_version(packages, renamed)?);
            let targets = packages
                .get(renamed)
                .and_then(|package| package.get("target"))
                .and_then(toml::Value::as_table)
                .context("requested Rust component is missing")?;
            let target = targets
                .get(host)
                .or_else(|| targets.get("*"))
                .context("Rust component is unavailable for this target")?;
            ensure!(
                target.get("available").and_then(toml::Value::as_bool) == Some(true),
                "requested Rust component is unavailable"
            );
            let before = assets.len();
            for (url, hash) in [
                ("url", "hash"),
                ("xz_url", "xz_hash"),
                ("zst_url", "zst_hash"),
            ] {
                match (target.get(url), target.get(hash)) {
                    (None, None) => {}
                    (Some(url), Some(hash)) => {
                        let url = url
                            .as_str()
                            .context("Rust component URL must be a string")?;
                        let sha256 = Digest::try_from(
                            hash.as_str()
                                .context("Rust component digest must be a string")?
                                .to_owned(),
                        )
                        .map_err(anyhow::Error::msg)?;
                        distribution_path(url)?;
                        assets.push(ComponentAsset {
                            component: component.clone(),
                            url: url.into(),
                            sha256,
                        });
                    }
                    _ => anyhow::bail!("Rust component archive lacks a URL/digest pair"),
                }
            }
            ensure!(
                assets.len() != before,
                "Rust component has no pinned archive variant"
            );
        }
        Ok(Self {
            date: date.into(),
            rust_version: release,
            rustc_version: compiler,
            versions,
            assets,
        })
    }
}

fn package_version(packages: &toml::Table, name: &str) -> Result<String> {
    Ok(packages
        .get(name)
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .context("Rust manifest lacks a compiler identity")?
        .into())
}

fn distribution_path(url: &str) -> Result<PathBuf> {
    let parsed = url::Url::parse(url)?;
    ensure!(
        parsed.scheme() == "https"
            && parsed.host_str() == Some("static.rust-lang.org")
            && parsed.port().is_none()
            && parsed.username().is_empty()
            && parsed.password().is_none()
            && parsed.query().is_none()
            && parsed.fragment().is_none(),
        "Rust distribution URL is outside the official immutable origin"
    );
    let relative = parsed
        .path()
        .strip_prefix('/')
        .context("Rust distribution URL has no absolute path")?;
    ensure!(
        relative.starts_with("dist/")
            && !relative.contains('%')
            && relative
                .split('/')
                .all(|component| rules::artifact_rules::portable_component(component.as_bytes())),
        "Rust distribution path is not portable"
    );
    Ok(PathBuf::from(relative))
}

fn distribution_artifacts(
    workload: &RustKaniWorkload,
    pin: &DistributionPin,
    metadata: &RustDistributionMetadata,
) -> Result<Vec<Artifact>> {
    metadata
        .assets
        .iter()
        .map(|asset| {
            workload.artifact(
                &format!(
                    "rust-component:{}:{}:{}",
                    pin.channel,
                    asset.component,
                    distribution_path(&asset.url)?
                        .file_name()
                        .context("Rust archive has no filename")?
                        .to_string_lossy()
                ),
                &pin.channel,
                &asset.url,
                asset.sha256,
            )
        })
        .collect()
}

fn put_manifest(root: &Path, pin: &DistributionPin, captured: &Path) -> Result<()> {
    let destination = root.join(distribution_path(&pin.url)?);
    fs::create_dir_all(
        destination
            .parent()
            .context("Rust manifest has no mirror parent")?,
    )?;
    let bytes = fs::read(captured)?;
    ensure!(
        Digest::of(&bytes) == pin.sha256,
        "Rust manifest changed before mirror consumption"
    );
    fs::write(&destination, bytes)?;
    fs::write(
        destination.with_extension("toml.sha256"),
        format!("{}\n", pin.sha256.hex()),
    )?;
    Ok(())
}

#[derive(Debug)]
pub struct KaniBundleMetadata {
    channel: String,
    rustc: String,
}

impl KaniBundleMetadata {
    /// Decode original bundle metadata without authenticating or installing its bytes.
    ///
    /// # Errors
    ///
    /// Rejects linked, special, unsafe, repeated, oversize, and incomplete archive inputs.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() as u64 <= super::cache::ARTIFACT_BYTE_LIMIT,
            "Kani bundle exceeds its compressed byte bound"
        );
        parse_bundle(std::io::Cursor::new(bytes), BUNDLE_BYTES, BUNDLE_ENTRIES)
    }

    /// Replay the same decoder under finite fuzz bounds without granting native readiness.
    ///
    /// # Errors
    ///
    /// Refuses the same invalid archive shapes and inputs beyond the finite replay budget.
    pub fn parse_replay(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= REPLAY_COMPRESSED_BYTES,
            "Kani replay exceeds its compressed byte bound"
        );
        parse_bundle(
            std::io::Cursor::new(bytes),
            REPLAY_BUNDLE_BYTES,
            REPLAY_BUNDLE_ENTRIES,
        )
    }
}

fn bundle_metadata(path: &Path) -> Result<KaniBundleMetadata> {
    parse_bundle(
        BufReader::new(File::open(path)?),
        BUNDLE_BYTES,
        BUNDLE_ENTRIES,
    )
}

fn parse_bundle(
    reader: impl BufRead,
    byte_limit: u64,
    entry_limit: usize,
) -> Result<KaniBundleMetadata> {
    let mut gzip = flate2::bufread::GzDecoder::new(reader);
    let mut decoded = (&mut gzip).take(byte_limit + 1);
    let mut archive = tar::Archive::new(&mut decoded);
    let mut seen = BTreeSet::new();
    let mut total = 0_u64;
    let mut channel = None;
    let mut rustc = None;
    let mut archive_root = None;
    for entry in archive.entries()? {
        let mut entry = entry?;
        ensure!(
            seen.len() < entry_limit,
            "Kani bundle exceeds its entry bound"
        );
        let path = entry
            .path()?
            .to_str()
            .context("Kani bundle path requires Unicode")?
            .trim_end_matches('/')
            .to_owned();
        ensure!(
            path.split('/')
                .all(|part| rules::artifact_rules::portable_component(part.as_bytes()))
                && seen.insert(path.to_ascii_lowercase()),
            "Kani bundle has unsafe or duplicate paths"
        );
        let kind = entry.header().entry_type();
        ensure!(
            kind.is_file() || kind.is_dir(),
            "Kani bundle contains a linked or special entry"
        );
        total = total
            .checked_add(entry.size())
            .context("Kani bundle size overflows")?;
        ensure!(
            total <= byte_limit,
            "Kani bundle exceeds its decoded byte bound"
        );
        let mut parts = path.split('/');
        let root = parts.next().context("Kani bundle lacks its archive root")?;
        ensure!(
            archive_root
                .as_deref()
                .is_none_or(|expected| expected == root),
            "Kani bundle has multiple archive roots"
        );
        archive_root = Some(root.to_owned());
        if let (Some(name), None) = (parts.next(), parts.next())
            && matches!(name, "rust-toolchain-version" | "rustc-version")
        {
            ensure!(
                kind.is_file() && entry.size() <= METADATA_BYTES,
                "Kani compiler metadata is not a bounded regular file"
            );
            let mut value = String::new();
            entry.read_to_string(&mut value)?;
            if name == "rust-toolchain-version" {
                channel = Some(value.trim().into());
            } else {
                rustc = Some(value.trim().into());
            }
        }
    }
    let reader = archive.into_inner();
    let mut trailing = [0; ARCHIVE_BUFFER];
    loop {
        let count = reader.read(&mut trailing)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .context("Kani bundle padding size overflows")?;
        ensure!(
            total <= byte_limit && trailing.iter().take(count).all(|byte| *byte == 0),
            "Kani bundle contains excessive or nonzero tar trailing data"
        );
    }
    ensure!(
        reader.limit() > 0,
        "Kani bundle exceeds its decoded stream bound"
    );
    let mut tail = gzip.into_inner();
    ensure!(
        tail.fill_buf()?.is_empty(),
        "Kani bundle contains gzip trailing data"
    );
    Ok(KaniBundleMetadata {
        channel: channel.context("Kani bundle lacks its compiler channel")?,
        rustc: rustc.context("Kani bundle lacks its compiler identity")?,
    })
}

#[derive(Clone, Debug)]
pub(super) struct AuthenticatedStdlibLock {
    source_root: PathBuf,
    source_inputs: BTreeMap<String, String>,
    target: Target,
    artifact: Artifact,
    archive: PathBuf,
    manifest: PathBuf,
    manifest_digest: Digest,
    lock_path: PathBuf,
    text: String,
    name: String,
    origin: Digest,
}

impl AuthenticatedStdlibLock {
    fn capture(
        workload: &RustKaniWorkload,
        metadata: &RustDistributionMetadata,
        artifact: &Artifact,
        archive: PathBuf,
        manifest: &Path,
        lock_path: PathBuf,
    ) -> Result<Self> {
        workload.current()?;
        ensure!(
            Cache::content(&mut File::open(physical(&archive)?)?)? == artifact.expected,
            "rust-src archive differs from its verified identity"
        );
        let bytes = parse_stdlib_lock(
            BufReader::new(File::open(&archive)?),
            BUNDLE_BYTES,
            BUNDLE_ENTRIES,
        )?;
        let text = String::from_utf8(bytes).context("stdlib Cargo lock requires UTF-8")?;
        let manifest_digest = Digest::of(&read_stdlib_file(manifest)?);
        ensure!(
            manifest_digest == workload.catalog.nightly.sha256,
            "nightly manifest changed before stdlib capture"
        );
        let origin = Digest::of(&serde_json::to_vec(&(
            STDLIB_ORIGIN,
            &workload.root,
            workload.target.name(),
            &workload.catalog.nightly.channel,
            &metadata.date,
            &metadata.rust_version,
            &metadata.rustc_version,
            &artifact,
            manifest_digest,
            STDLIB_MEMBER,
            Digest::of(text.as_bytes()),
        ))?);
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)?
            .write_all(text.as_bytes())?;
        let selected = Self {
            source_root: workload.root.clone(),
            source_inputs: workload.inputs.clone(),
            target: workload.target,
            artifact: artifact.clone(),
            archive,
            manifest: manifest.to_owned(),
            manifest_digest,
            lock_path,
            text,
            name: format!(
                "authenticated-rust-src:{}:{}",
                workload.catalog.nightly.channel,
                workload.target.name()
            ),
            origin,
        };
        Plan::cargo_closure_with(
            &BTreeMap::new(),
            &[super::plan::AdditionalCargoLock {
                name: selected.name(),
                text: selected.text(),
                origin: selected.origin(),
            }],
        )?
        .complete()?;
        selected.require_current()?;
        Ok(selected)
    }

    pub(super) fn require_current(&self) -> Result<()> {
        let current = RustKaniWorkload::capture(&self.source_root, self.target)?;
        ensure!(
            current.root == self.source_root && current.inputs == self.source_inputs,
            "authenticated stdlib Source inputs changed"
        );
        ensure!(
            Cache::content(&mut File::open(physical(&self.archive)?)?)? == self.artifact.expected
                && Digest::of(&read_stdlib_file(&self.manifest)?) == self.manifest_digest
                && read_stdlib_file(&self.lock_path)? == self.text.as_bytes(),
            "authenticated stdlib materials changed"
        );
        Ok(())
    }

    pub(super) fn name(&self) -> &str {
        &self.name
    }
    pub(super) fn text(&self) -> &str {
        &self.text
    }
    pub(super) const fn origin(&self) -> Digest {
        self.origin
    }
}

fn read_stdlib_file(path: &Path) -> Result<Vec<u8>> {
    let path = physical(path)?;
    let metadata = fs::symlink_metadata(&path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= INPUT_BYTES,
        "stdlib input is not a bounded regular file"
    );
    let mut bytes = Vec::new();
    File::open(&path)?
        .take(INPUT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= INPUT_BYTES,
        "stdlib input grew beyond its bound"
    );
    Ok(bytes)
}

fn stdlib_tar_tail(reader: &mut impl Read, byte_limit: u64, mut total: u64) -> Result<()> {
    let mut trailing = [0; ARCHIVE_BUFFER];
    loop {
        let count = reader.read(&mut trailing)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .context("rust-src padding size overflows")?;
        ensure!(
            total <= byte_limit && trailing.iter().take(count).all(|byte| *byte == 0),
            "rust-src has excessive or nonzero tar trailing data"
        );
    }
    Ok(())
}

/// Replay untrusted stdlib bytes through the production archive and Cargo-lock decoders.
///
/// # Errors
/// Refuses malformed or unresolved contents without granting Source or installation authority.
pub fn replay_stdlib_archive(bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() <= REPLAY_COMPRESSED_BYTES,
        "stdlib replay exceeds its compressed byte bound"
    );
    let lock = parse_stdlib_lock(
        std::io::Cursor::new(bytes),
        REPLAY_BUNDLE_BYTES,
        REPLAY_BUNDLE_ENTRIES,
    )?;
    Plan::cargo_closure_with(
        &BTreeMap::new(),
        &[super::plan::AdditionalCargoLock {
            name: STDLIB_REPLAY_INPUT,
            text: std::str::from_utf8(&lock).context("stdlib Cargo lock requires UTF-8")?,
            origin: Digest::of(bytes),
        }],
    )?
    .complete()
}

fn parse_stdlib_lock(reader: impl BufRead, byte_limit: u64, entry_limit: usize) -> Result<Vec<u8>> {
    let mut gzip = flate2::bufread::GzDecoder::new(reader);
    let mut decoded = (&mut gzip).take(byte_limit + 1);
    let mut archive = tar::Archive::new(&mut decoded);
    let mut seen = BTreeSet::new();
    let mut total = 0_u64;
    let mut selected = None;
    for entry in archive.entries()? {
        let mut entry = entry?;
        ensure!(seen.len() < entry_limit, "rust-src exceeds its entry bound");
        let kind = entry.header().entry_type();
        let path = entry
            .path()?
            .to_str()
            .context("rust-src path requires Unicode")?
            .to_owned();
        let path = if kind.is_dir() {
            path.trim_end_matches('/')
        } else {
            path.as_str()
        };
        ensure!(
            path.split('/')
                .all(|part| rules::artifact_rules::portable_component(part.as_bytes()))
                && seen.insert(path.to_ascii_lowercase()),
            "rust-src has unsafe or duplicate paths"
        );
        ensure!(
            kind.is_file() || kind.is_dir(),
            "rust-src contains a linked or special entry"
        );
        total = total
            .checked_add(entry.size())
            .context("rust-src size overflows")?;
        ensure!(
            total <= byte_limit,
            "rust-src exceeds its decoded byte bound"
        );
        if path == STDLIB_MEMBER {
            ensure!(
                kind.is_file() && entry.size() <= INPUT_BYTES && selected.is_none(),
                "stdlib lock is not one bounded regular member"
            );
            let mut bytes = Vec::new();
            (&mut entry).take(INPUT_BYTES + 1).read_to_end(&mut bytes)?;
            ensure!(
                bytes.len() as u64 == entry.size() && bytes.len() as u64 <= INPUT_BYTES,
                "stdlib lock is truncated or oversized"
            );
            selected = Some(bytes);
        }
    }
    let reader = archive.into_inner();
    stdlib_tar_tail(reader, byte_limit, total)?;
    ensure!(
        reader.limit() > 0,
        "rust-src exceeds its decoded stream bound"
    );
    let mut tail = gzip.into_inner();
    ensure!(
        tail.fill_buf()?.is_empty(),
        "rust-src contains gzip trailing data"
    );
    selected.context("rust-src lacks its exact stdlib Cargo lock")
}

/// Complete captured materials for the selected Rust/Kani scope, not whole-native readiness.
#[derive(Debug)]
pub struct PreparedRustKani {
    workload: RustKaniWorkload,
    owned: tempfile::TempDir,
    captured: BTreeMap<String, PathBuf>,
    mirror: PathBuf,
    input_files: BTreeMap<String, Digest>,
    mirror_files: BTreeMap<String, Digest>,
    launcher: Option<PreparedCargoTool>,
    stable: RustDistributionMetadata,
    nightly: Option<RustDistributionMetadata>,
    stdlib: Option<AuthenticatedStdlibLock>,
}

impl PreparedRustKani {
    fn current(&self) -> Result<()> {
        self.workload.current()?;
        ensure!(
            super::cargo_tool_consume::tree(&self.owned.path().join("inputs"))? == self.input_files
                && super::cargo_tool_consume::tree(&self.mirror)? == self.mirror_files,
            "native consumption materials changed after capture"
        );
        if let Some(launcher) = &self.launcher {
            launcher.current()?;
        }
        if let Some(lock) = &self.stdlib {
            lock.require_current()?;
        }
        Ok(())
    }

    pub(super) fn stdlib_input(&self, root: &Path) -> Result<Option<AuthenticatedStdlibLock>> {
        self.current()?;
        ensure!(
            root == self.workload.root,
            "stdlib input belongs to another Source"
        );
        ensure!(
            self.nightly.is_none() || self.stdlib.is_some(),
            "nightly stdlib input is missing"
        );
        Ok(self.stdlib.clone())
    }

    fn require_installed_stdlib(&self) -> Result<()> {
        if let Some(lock) = &self.stdlib {
            let path = self
                .owned
                .path()
                .join("home/.rustup/toolchains")
                .join(format!(
                    "{}-{}",
                    self.workload.catalog.nightly.channel,
                    triple(self.workload.target)
                ))
                .join(INSTALLED_STDLIB_LOCK);
            ensure!(
                read_stdlib_file(&path)? == lock.text.as_bytes(),
                "installed stdlib Cargo lock differs from the authenticated original"
            );
        }
        Ok(())
    }

    /// Materials to protect while writable installation and compilation outputs stay separate.
    #[must_use]
    pub fn immutable_paths(&self) -> Vec<PathBuf> {
        let mut paths = vec![self.owned.path().join("inputs"), self.mirror.clone()];
        if let Some(launcher) = &self.launcher {
            paths.extend(launcher.immutable_paths());
        }
        paths
    }

    fn command(&self, program: &Path) -> Result<Command> {
        native_command(
            self.owned.path(),
            program,
            &self.workload.catalog.stable.channel,
        )
    }

    /// Install into new owned stores using only the already captured materials.
    ///
    /// # Errors
    ///
    /// Rejects changed bytes, a different native target, existing output stores, and failed original installers.
    pub fn install(self) -> Result<InstalledRustKani> {
        self.current()?;
        ensure!(
            current_target()? == self.workload.target,
            "native material target differs from this machine"
        );
        let root = self.owned.path();
        let home = root.join("home");
        ensure!(
            !home.exists(),
            "native installation requires a fresh owned home"
        );
        fs::create_dir(&home)?;
        fs::create_dir_all(root.join("tmp"))?;
        let bootstrap = &self.captured["rustup-bootstrap"];
        let mut version = self.command(bootstrap)?;
        version.arg("--version");
        let mut operations = vec![operation(
            version,
            root,
            "verify source-authenticated Rustup installer version",
        )?];
        require_version(
            &operations[0],
            "rustup-init",
            &self.workload.catalog.rustup.version,
        )?;
        let mut install = self.command(bootstrap)?;
        install.args(["-y", "--no-modify-path", "--default-toolchain", "none"]);
        operations.push(operation(install, root, "initialize fresh Rustup stores")?);
        self.current()?;
        let rustup = home.join(".cargo/bin").join(program_name("rustup"));
        let rustup = physical(&rustup)?;
        ensure!(
            Cache::content(&mut File::open(&rustup)?)?
                == self.workload.catalog.rustup.targets[self.workload.target.name()].sha256,
            "installed Rustup is not derived from the verified bootstrap"
        );
        self.install_distributions(&rustup, &mut operations)?;
        self.require_installed_stdlib()?;
        let stable = physical(&home.join(".rustup/toolchains").join(format!(
            "{}-{}",
            self.workload.catalog.stable.channel,
            triple(self.workload.target)
        )))?;
        let launcher_root = root.join("launcher-install");
        let kani_home = home.join(".kani");
        if let Some(launcher) = &self.launcher {
            let cargo = physical(&stable.join("bin").join(program_name("cargo")))?;
            operations.push(launcher.install(self.command(&cargo)?, root, &launcher_root)?);
            let nightly = physical(&home.join(".rustup/toolchains").join(format!(
                "{}-{}",
                self.workload.catalog.nightly.channel,
                triple(self.workload.target)
            )))?;
            let program = physical(&launcher_root.join("bin/kani"))?;
            let mut setup = self.command(&program)?;
            setup
                .args(["setup", "--use-local-bundle"])
                .arg(&self.captured["kani-bundle"])
                .arg("--use-local-toolchain")
                .arg(&nightly);
            operations.push(operation(
                setup,
                root,
                "prepare original Kani distribution from local verified inputs",
            )?);
            ensure!(
                platform::paths::canonical(
                    &kani_home.join(format!("kani-{}/toolchain", launcher.version()))
                )? == nightly,
                "Kani setup did not retain the fresh pinned compiler"
            );
        }
        self.current()?;
        let programs = installed_programs(root, &self.workload)?;
        let digests = programs
            .iter()
            .map(|(name, path)| Ok((name.clone(), Cache::content(&mut File::open(path)?)?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let installed_files = installed_tree(root, &self.workload.catalog.kani.version)?;
        Ok(InstalledRustKani {
            prepared: self,
            programs,
            digests,
            installed_files,
            operations,
            view: NativeDataView::Unbound,
        })
    }

    fn install_distributions(
        &self,
        rustup: &Path,
        operations: &mut Vec<NativeMaterialOperation>,
    ) -> Result<()> {
        let mirror = url::Url::from_directory_path(&self.mirror)
            .map_err(|()| anyhow::anyhow!("Rust distribution mirror has no native file URL"))?;
        for (pin, components) in [
            (
                &self.workload.catalog.stable,
                self.workload.stable_components.clone(),
            ),
            (
                &self.workload.catalog.nightly,
                CORE_COMPONENTS
                    .into_iter()
                    .chain(NIGHTLY_COMPONENTS)
                    .map(str::to_owned)
                    .collect(),
            ),
        ] {
            if pin.channel == self.workload.catalog.nightly.channel && self.nightly.is_none() {
                continue;
            }
            let mut command = self.command(rustup)?;
            command.env("RUSTUP_DIST_SERVER", mirror.as_str()).args([
                "toolchain",
                "install",
                &pin.channel,
                "--profile",
                "minimal",
                "--no-self-update",
            ]);
            for component in components {
                command.arg("--component").arg(component);
            }
            operations.push(operation(
                command,
                self.owned.path(),
                "install pinned Rust distribution from the owned mirror",
            )?);
            self.current()?;
        }
        Ok(())
    }
}

fn program_name(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

fn executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(BOOTSTRAP_EXECUTABLE_MODE))?;
    }
    #[cfg(windows)]
    {
        ensure!(path.is_file(), "bootstrap executable is missing");
    }
    Ok(())
}

fn current_target() -> Result<Target> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok(Target::LinuxX64),
        ("linux", "aarch64") => Ok(Target::LinuxArm64),
        ("macos", "x86_64") => Ok(Target::MacosX64),
        ("macos", "aarch64") => Ok(Target::MacosArm64),
        ("windows", "x86_64") => Ok(Target::WindowsX64),
        _ => anyhow::bail!("native Rust material installation has no supported target"),
    }
}

fn native_command(root: &Path, program: &Path, channel: &str) -> Result<Command> {
    let mut command = platform::tool::external(program);
    for (name, _) in std::env::vars_os() {
        let name_text = name.to_string_lossy().to_ascii_uppercase();
        if [
            "RUSTUP_", "CARGO_", "MISE_", "__MISE_", "UV_", "DOPPLER_", "LD_", "DYLD_",
        ]
        .iter()
        .any(|prefix| name_text.starts_with(prefix))
            || matches!(
                name_text.as_str(),
                "GITHUB_TOKEN"
                    | "GH_TOKEN"
                    | "GH_CONFIG_DIR"
                    | "RUSTC"
                    | "RUSTC_WRAPPER"
                    | "RUSTC_WORKSPACE_WRAPPER"
                    | "RUSTFLAGS"
                    | "RUSTDOC"
                    | "RUSTDOCFLAGS"
            )
        {
            command.env_remove(name);
        }
    }
    let home = root.join("home");
    let mirror = url::Url::from_directory_path(root.join("mirror"))
        .map_err(|()| anyhow::anyhow!("owned Rust mirror has no native file URL"))?;
    let update = url::Url::from_directory_path(root.join("mirror/rustup"))
        .map_err(|()| anyhow::anyhow!("owned Rustup update root has no native file URL"))?;
    let mut path = vec![home.join(".cargo/bin"), root.join("launcher-install/bin")];
    if cfg!(unix) {
        path.push(
            PathBuf::from(TAR)
                .parent()
                .context("native Tar has no directory")?
                .into(),
        );
    }
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    command
        .current_dir(root)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("CARGO_HOME", home.join(".cargo"))
        .env("RUSTUP_HOME", home.join(".rustup"))
        .env("RUSTUP_DIST_SERVER", mirror.as_str())
        .env("RUSTUP_UPDATE_ROOT", update.as_str())
        .env("KANI_HOME", home.join(".kani"))
        .env("RUSTUP_TOOLCHAIN", channel)
        .env("RUSTUP_AUTO_INSTALL", "0")
        .env("CARGO_NET_OFFLINE", "true")
        .env("CARGO_BUILD_JOBS", "2")
        .env("CARGO_INCREMENTAL", "0")
        .env("MISE_OFFLINE", "true")
        .env("MISE_LOCKED", "true")
        .env("MISE_PARANOID", "true")
        .env("MISE_AUTO_INSTALL", "false")
        .env("MISE_AUTO_PRUNE", "false")
        .env("TEMP", root.join("tmp"))
        .env("TMP", root.join("tmp"))
        .env("TMPDIR", root.join("tmp"))
        .env("PATH", std::env::join_paths(path)?);
    Ok(command)
}

fn installed_programs(
    root: &Path,
    workload: &RustKaniWorkload,
) -> Result<BTreeMap<String, PathBuf>> {
    let mut programs = BTreeMap::new();
    let toolchains = root.join("home/.rustup/toolchains");
    let stable = toolchains.join(format!(
        "{}-{}",
        workload.catalog.stable.channel,
        triple(workload.target)
    ));
    for name in ["cargo", "rustc"] {
        programs.insert(
            name.into(),
            physical(&stable.join("bin").join(program_name(name)))?,
        );
    }
    programs.insert(
        "rustup".into(),
        physical(&root.join("home/.cargo/bin").join(program_name("rustup")))?,
    );
    if workload.target != Target::WindowsX64 {
        programs.insert(
            "kani".into(),
            physical(&root.join("launcher-install/bin/kani"))?,
        );
        programs.insert(
            "kani-driver".into(),
            physical(&root.join(format!(
                "home/.kani/kani-{}/bin/kani-driver",
                workload.catalog.kani.version
            )))?,
        );
    }
    Ok(programs)
}

fn require_version(
    operation: &NativeMaterialOperation,
    program: &str,
    version: &str,
) -> Result<()> {
    decode_health(operation, |operation| {
        let stdout = std::str::from_utf8(&operation.stdout)?;
        let mut words = stdout.split_ascii_whitespace();
        ensure!(
            words.next() == Some(program) && words.next() == Some(version),
            "native program identity differs from its source pin"
        );
        Ok(())
    })
}

fn decode_health<T>(
    operation: &NativeMaterialOperation,
    decode: impl FnOnce(&NativeMaterialOperation) -> Result<T>,
) -> Result<T> {
    decode(operation).map_err(|error| {
        anyhow::Error::new(NativeMaterialFailure {
            operation: operation.clone(),
        })
        .context(error)
    })
}

#[derive(Debug)]
enum NativeDataView {
    Unbound,
    Bound {
        data: PathBuf,
        launcher: Option<PathBuf>,
    },
}

/// Owner of a fresh installation before the original verifier health gate passes.
#[derive(Debug)]
pub struct InstalledRustKani {
    prepared: PreparedRustKani,
    programs: BTreeMap<String, PathBuf>,
    digests: BTreeMap<String, Digest>,
    installed_files: BTreeMap<String, Digest>,
    operations: Vec<NativeMaterialOperation>,
    view: NativeDataView,
}

impl InstalledRustKani {
    #[must_use]
    pub fn cargo_home(&self) -> PathBuf {
        self.prepared.owned.path().join("home/.cargo")
    }
    #[must_use]
    pub fn rustup_home(&self) -> PathBuf {
        self.prepared.owned.path().join("home/.rustup")
    }
    #[must_use]
    pub fn kani_home(&self) -> PathBuf {
        self.prepared.owned.path().join("home/.kani")
    }
    #[must_use]
    pub fn launcher_root(&self) -> Option<PathBuf> {
        self.prepared
            .launcher
            .as_ref()
            .map(|_| self.prepared.owned.path().join("launcher-install"))
    }
    #[must_use]
    pub fn launcher_version(&self) -> Option<&str> {
        self.prepared
            .launcher
            .as_ref()
            .map(PreparedCargoTool::version)
    }
    #[must_use]
    pub const fn target(&self) -> Target {
        self.prepared.workload.target
    }
    #[must_use]
    pub fn source_root(&self) -> &Path {
        &self.prepared.workload.root
    }
    #[must_use]
    pub fn owned_namespace(&self) -> &Path {
        self.prepared.owned.path()
    }
    #[must_use]
    pub fn operations(&self) -> &[NativeMaterialOperation] {
        &self.operations
    }
    #[must_use]
    pub fn cargo_program(&self) -> &Path {
        &self.programs["cargo"]
    }
    #[must_use]
    pub fn stable_toolchain(&self) -> PathBuf {
        self.rustup_home().join("toolchains").join(format!(
            "{}-{}",
            self.prepared.workload.catalog.stable.channel,
            triple(self.target())
        ))
    }
    #[must_use]
    pub fn immutable_paths(&self) -> Vec<PathBuf> {
        let mut paths = self.prepared.immutable_paths();
        paths.push(self.rustup_home().join("toolchains"));
        paths.push(self.cargo_home().join("bin"));
        if let Some(launcher) = self.launcher_root() {
            paths.extend([
                launcher,
                self.kani_home().join(format!(
                    "kani-{}",
                    self.prepared.workload.catalog.kani.version
                )),
            ]);
        }
        paths
    }

    /// Recheck source and installed program bytes before any descendant command.
    ///
    /// # Errors
    ///
    /// Refuses source, material, program, installation or linked data-view replacement.
    pub fn require_current(&self) -> Result<()> {
        self.prepared.current()?;
        self.prepared.require_installed_stdlib()?;
        ensure!(
            installed_tree(
                self.prepared.owned.path(),
                &self.prepared.workload.catalog.kani.version
            )? == self.installed_files,
            "fresh native distribution bytes or internal links changed"
        );
        for (name, path) in &self.programs {
            ensure!(
                physical(path)? == *path
                    && Cache::content(&mut File::open(path)?)? == self.digests[name],
                "fresh native program changed: {name}"
            );
        }
        if let NativeDataView::Bound { data, launcher } = &self.view {
            physical(data)?;
            if let Some(path) = launcher {
                ensure!(
                    platform::paths::canonical(path)?
                        == self
                            .launcher_root()
                            .context("bound data view lacks its launcher")?,
                    "Mise data view no longer selects the fresh Kani launcher"
                );
            }
        }
        Ok(())
    }

    /// Bind actual original-mise resolver paths after its public link operation.
    ///
    /// # Errors
    ///
    /// Refuses a caller's old Rust store, a logical path outside the data view or a different launcher.
    pub fn bind_data_view(
        &mut self,
        data: &Path,
        rust_install: &Path,
        launcher_install: Option<&Path>,
    ) -> Result<()> {
        self.require_current()?;
        let data = physical(data)?;
        ensure!(
            !data.starts_with(&self.prepared.workload.root),
            "native data view must stay outside captured source"
        );
        let stable = self.rustup_home().join("toolchains").join(format!(
            "{}-{}",
            self.prepared.workload.catalog.stable.channel,
            triple(self.target())
        ));
        ensure!(
            platform::paths::canonical(rust_install)? == stable,
            "Mise did not select the fresh pinned compiler"
        );
        let launcher = match (self.launcher_root(), launcher_install) {
            (Some(root), Some(logical)) => {
                ensure!(
                    logical.strip_prefix(data.join("installs")).is_ok()
                        && platform::paths::canonical(logical)? == root,
                    "Mise did not select the fresh source-built launcher in its logical data view"
                );
                Some(logical.to_owned())
            }
            (None, None) => None,
            _ => anyhow::bail!("native verifier data-view inventory is incomplete"),
        };
        self.view = NativeDataView::Bound { data, launcher };
        Ok(())
    }

    /// Configure the original health command without requiring the full Code environment.
    ///
    /// # Errors
    ///
    /// Refuses an unbound data view or changed installation and source.
    pub fn configure_health(&self, command: &mut Command) -> Result<()> {
        self.require_current()?;
        let NativeDataView::Bound { data, .. } = &self.view else {
            anyhow::bail!("native health requires the original Mise data-view binding");
        };
        let environment = native_command(
            self.prepared.owned.path(),
            &self.programs["cargo"],
            &self.prepared.workload.catalog.stable.channel,
        )?;
        for (name, value) in environment.get_envs() {
            if let Some(value) = value {
                command.env(name, value);
            } else {
                command.env_remove(name);
            }
        }
        command
            .env("MISE_DATA_DIR", data)
            .env("MISE_RUSTUP_HOME", self.rustup_home())
            .env("MISE_CARGO_HOME", self.cargo_home());
        Ok(())
    }

    /// Observe a fully configured original health command without creating verification credit.
    ///
    /// # Errors
    ///
    /// Rejects missing resolver binding, changed inputs, and failed or unbounded operations.
    pub fn observe_health(&self, command: Command, name: &str) -> Result<NativeMaterialOperation> {
        self.require_current()?;
        ensure!(
            matches!(self.view, NativeDataView::Bound { .. }),
            "native health observation requires the original resolver binding"
        );
        let observed = operation(command, self.prepared.owned.path(), name);
        let current = self.require_current();
        match observed {
            Ok(operation) => {
                current?;
                Ok(operation)
            }
            Err(error) => {
                platform::cleanup::report(current, "recheck native stores after failed health");
                Err(error)
            }
        }
    }

    fn compiler_health(&self) -> Result<()> {
        let root = self.prepared.owned.path();
        let mut rustc = self.prepared.command(&self.programs["rustc"])?;
        rustc.arg("--version");
        let rustc = operation(rustc, root, "live fresh Rust compiler")?;
        decode_health(&rustc, |output| {
            ensure!(
                std::str::from_utf8(&output.stdout)?.trim()
                    == format!("rustc {}", self.prepared.stable.rustc_version),
                "fresh compiler identity differs from the original manifest"
            );
            Ok(())
        })?;
        let stable_path = self.stable_toolchain();
        let mut sysroot = self.prepared.command(&self.programs["rustc"])?;
        sysroot.args(["--print", "sysroot"]);
        let sysroot = operation(sysroot, root, "live fresh Rust sysroot binding")?;
        decode_health(&sysroot, |output| {
            ensure!(
                physical(Path::new(std::str::from_utf8(&output.stdout)?.trim()))? == stable_path,
                "fresh compiler selected a different sysroot"
            );
            Ok(())
        })?;
        for (pin, metadata) in [
            (
                &self.prepared.workload.catalog.stable,
                Some(&self.prepared.stable),
            ),
            (
                &self.prepared.workload.catalog.nightly,
                self.prepared.nightly.as_ref(),
            ),
        ] {
            if pin.channel == self.prepared.workload.catalog.nightly.channel
                && self.prepared.nightly.is_none()
            {
                continue;
            }
            self.distribution_health(pin, metadata)?;
        }
        self.require_current()
    }

    fn distribution_health(
        &self,
        pin: &DistributionPin,
        metadata: Option<&RustDistributionMetadata>,
    ) -> Result<()> {
        let root = self.prepared.owned.path();
        let mut command = self.prepared.command(&self.programs["rustup"])?;
        command.args([
            "component",
            "list",
            "--installed",
            "--toolchain",
            &pin.channel,
        ]);
        let output = operation(command, root, "live fresh Rust component inventory")?;
        let requested = if pin.channel == self.prepared.workload.catalog.stable.channel {
            self.prepared.workload.stable_components.clone()
        } else {
            CORE_COMPONENTS
                .into_iter()
                .chain(NIGHTLY_COMPONENTS)
                .map(str::to_owned)
                .collect()
        };
        decode_health(&output, |output| {
            let inventory: BTreeSet<_> = std::str::from_utf8(&output.stdout)?.lines().collect();
            for name in requested {
                ensure!(
                    inventory.contains(name.as_str())
                        || inventory.contains(format!("{name}-{}", triple(self.target())).as_str()),
                    "fresh distribution lacks an installed requested component: {name}"
                );
            }
            Ok(())
        })?;
        let metadata = metadata.context("requested compiler has no captured manifest")?;
        let toolchain = self.rustup_home().join("toolchains").join(format!(
            "{}-{}",
            pin.channel,
            triple(self.target())
        ));
        let standard_library = toolchain
            .join("lib/rustlib")
            .join(triple(self.target()))
            .join("lib");
        physical(&standard_library)?;
        ensure!(
            fs::read_dir(&standard_library)?.next().is_some(),
            "fresh Rust standard library is empty"
        );
        for (component, program) in [("clippy", "clippy-driver"), ("rustfmt", "rustfmt")] {
            if let Some(expected) = metadata.versions.get(component) {
                let program_path = physical(&toolchain.join("bin").join(program_name(program)))?;
                let mut command = self.prepared.command(&program_path)?;
                command.arg("--version");
                let output = operation(command, root, "live fresh Rust component")?;
                let version = expected
                    .split_ascii_whitespace()
                    .next()
                    .context("Rust component lacks its version")?;
                let version = if component == "rustfmt"
                    && pin.channel == self.prepared.workload.catalog.stable.channel
                {
                    format!("{version}-stable")
                } else {
                    version.to_owned()
                };
                require_version(&output, component, &version)?;
            }
        }
        if metadata.versions.contains_key("rust-src") {
            physical(&toolchain.join("lib/rustlib/src/rust/library/std/src/lib.rs"))?;
        }
        Ok(())
    }

    /// Run real compiler health and the original verifier decoder before invoking the body.
    /// The trusted production composition root supplies the verifier callback.
    ///
    /// # Errors
    ///
    /// Refuses missing binding, failed health and changed inputs before the body can run.
    pub fn with_health<T>(
        &self,
        verifier: impl FnOnce(&Self) -> Result<()>,
        body: impl FnOnce(CheckedRustKaniStores<'_>) -> Result<T>,
    ) -> Result<T> {
        self.require_current()?;
        let NativeDataView::Bound { data, .. } = &self.view else {
            anyhow::bail!("native health has no original resolver binding");
        };
        self.compiler_health()?;
        health_before_body(
            || verifier(self),
            || {
                self.require_current()?;
                body(CheckedRustKaniStores {
                    installed: self,
                    data,
                })
            },
        )
    }
}

pub(super) fn health_before_body<T>(
    health: impl FnOnce() -> Result<()>,
    body: impl FnOnce() -> Result<T>,
) -> Result<T> {
    health()?;
    body()
}

/// Borrowed source/catalog/target/path binding after actual original health.
/// Saved success records can't restore this callback or grant whole-native readiness.
#[derive(Debug)]
pub struct CheckedRustKaniStores<'a> {
    installed: &'a InstalledRustKani,
    data: &'a Path,
}

impl CheckedRustKaniStores<'_> {
    #[must_use]
    pub fn installed(&self) -> &InstalledRustKani {
        self.installed
    }
    #[must_use]
    pub const fn data(&self) -> &Path {
        self.data
    }
}

fn installed_tree(root: &Path, kani_version: &str) -> Result<BTreeMap<String, Digest>> {
    fn visit(
        boundary: &Path,
        path: &Path,
        depth: usize,
        bytes: &mut u64,
        seen: &mut BTreeSet<PathBuf>,
        entries: &mut BTreeMap<String, Digest>,
    ) -> Result<()> {
        ensure!(
            depth <= INVENTORY_DEPTH && entries.len() < BUNDLE_ENTRIES,
            "native installed inventory exceeds its bound"
        );
        let metadata = fs::symlink_metadata(path)?;
        let relative = path
            .strip_prefix(boundary)?
            .to_str()
            .context("installed path requires Unicode")?
            .replace('\\', "/");
        let canonical = platform::paths::canonical(path)?;
        ensure!(
            canonical.starts_with(boundary),
            "native installed link escapes its owned namespace"
        );
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(path)?;
            entries.insert(
                relative,
                Digest::of(&serde_json::to_vec(&(
                    "link",
                    target
                        .to_str()
                        .context("native installed link requires Unicode")?,
                ))?),
            );
        }
        if !seen.insert(canonical.clone()) {
            return Ok(());
        }
        let metadata = fs::metadata(&canonical)?;
        if metadata.is_dir() {
            for entry in fs::read_dir(canonical)? {
                visit(boundary, &entry?.path(), depth + 1, bytes, seen, entries)?;
            }
        } else {
            ensure!(
                metadata.is_file(),
                "native installed inventory has a special object"
            );
            *bytes = bytes
                .checked_add(metadata.len())
                .context("native installed inventory size overflows")?;
            ensure!(
                *bytes <= BUNDLE_BYTES,
                "native installed inventory exceeds its byte bound"
            );
            let name = canonical
                .strip_prefix(boundary)?
                .to_str()
                .context("native installed path requires Unicode")?
                .replace('\\', "/");
            entries.insert(name, Cache::content(&mut File::open(canonical)?)?);
        }
        Ok(())
    }
    let boundary = physical(root)?;
    let mut entries = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut bytes = 0;
    for directory in [
        "home/.rustup/toolchains".into(),
        "home/.cargo/bin".into(),
        "launcher-install/bin".into(),
        format!("home/.kani/kani-{kani_version}"),
    ] {
        let path = boundary.join(directory);
        if path.exists() {
            visit(&boundary, &path, 0, &mut bytes, &mut seen, &mut entries)?;
        }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const MODE: &str = "DOTFILES_NATIVE_RUST_ENV_TEST_MODE";
    const SCOPE: &str = "DOTFILES_NATIVE_RUST_ENV_TEST_SCOPE";
    const ENVIRONMENT_TEST: &str = "artifacts::rustup_consume::tests::owned_native_commands_remove_ambient_store_and_compiler_overrides";
    const OVERRIDES: [&str; 10] = [
        "mise_config_file",
        "rustup_home",
        "uv_cache_dir",
        "gh_config_dir",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "CARGO_ENCODED_RUSTFLAGS",
        "LD_LIBRARY_PATH",
        "DYLD_LIBRARY_PATH",
        "DYLD_FALLBACK_LIBRARY_PATH",
    ];
    const KANI_VERSION: &str = "0.68.0";
    const RUST_VERSION: &str = "1.99.1";
    const BUNDLE_CHANNEL: &str = "nightly-2026-08-21";
    const BUNDLE_RUSTC: &str = "rustc 1.100.0-nightly (8925ea358 2026-08-20)";
    const UNIX_TARGETS: [Target; 4] = [
        Target::MacosArm64,
        Target::MacosX64,
        Target::LinuxArm64,
        Target::LinuxX64,
    ];

    struct PreflightMaterials {
        bytes: BTreeMap<String, Vec<u8>>,
        calls: Cell<usize>,
    }

    impl Fetch for PreflightMaterials {
        fn fetch(
            &self,
            source: &str,
            destination: &Path,
        ) -> std::result::Result<(), super::super::FetchFailure> {
            let bytes = self.bytes.get(source).ok_or_else(|| {
                super::super::FetchFailure::Refused("unexpected preflight fixture fetch".into())
            })?;
            self.calls.set(self.calls.get() + 1);
            fs::write(destination, bytes)
                .map_err(|error| super::super::FetchFailure::Refused(error.to_string()))
        }
    }

    fn preflight_manifest(date: &str, compiler: &str, components: &[String]) -> Vec<u8> {
        use std::fmt::Write as _;

        let mut manifest =
            format!("manifest-version='2'\ndate={date:?}\n[pkg.rust]\nversion={compiler:?}\n");
        for component in components {
            let url = format!("{RUSTUP_SERVER}/dist/{date}/{component}.tar.gz");
            writeln!(
                manifest,
                "[pkg.{component}]\nversion={compiler:?}\n[pkg.{component}.target.{}]\navailable=true\nurl={url:?}\nhash={:?}",
                triple(Target::MacosArm64),
                Digest::of(b"unconsumed component fixture").hex()
            )
            .expect("write the bounded fixture manifest");
        }
        manifest.into_bytes()
    }

    fn preflight_bundle() -> Result<Vec<u8>> {
        let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut archive = tar::Builder::new(gzip);
        let channel = format!("{BUNDLE_CHANNEL}-{}", triple(Target::MacosArm64));
        for (name, contents) in [
            ("rust-toolchain-version", channel.as_bytes()),
            (
                "rustc-version",
                b"rustc 1.100.0-nightly (000000000 2026-08-20)".as_slice(),
            ),
        ] {
            let mut header = tar::Header::new_ustar();
            header.set_size(u64::try_from(contents.len())?);
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_cksum();
            archive.append_data(&mut header, format!("kani-fixture/{name}"), contents)?;
        }
        Ok(archive.into_inner()?.finish()?)
    }

    fn preflight_fixture(root: &Path) -> Result<PreflightMaterials> {
        fs::create_dir_all(root.join("policy"))?;
        for name in ["mise.toml", "mise.lock"] {
            fs::write(
                root.join(name),
                fs::read(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../..")
                        .join(name),
                )?,
            )?;
        }
        let mut policy: toml::Value = toml::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../policy/native-materials.toml"
        )))?;
        let catalog = NativeRustCatalog::parse(&toml::to_string(&policy)?)?;
        let mut components: Vec<_> = CORE_COMPONENTS.into_iter().map(str::to_owned).collect();
        components.extend(["clippy".into(), "rustfmt".into()]);
        let stable = preflight_manifest("2026-10-15", "1.99.1 (58cf8f9c3 2026-10-13)", &components);
        let nightly_components: Vec<_> = CORE_COMPONENTS
            .into_iter()
            .chain(NIGHTLY_COMPONENTS)
            .map(str::to_owned)
            .collect();
        let nightly = preflight_manifest(
            "2026-08-21",
            BUNDLE_RUSTC.trim_start_matches("rustc "),
            &nightly_components,
        );
        let bootstrap = b"unexecuted bootstrap fixture".to_vec();
        let bundle = preflight_bundle()?;
        let launcher = b"verified bytes that deliberately are not a Cargo archive".to_vec();
        let target = Target::MacosArm64.name();
        for (table, bytes) in [("stable", &stable), ("nightly", &nightly)] {
            policy[table]["sha256"] = toml::Value::String(Digest::of(bytes).hex());
        }
        policy["rustup"]["targets"][target]["sha256"] =
            toml::Value::String(Digest::of(&bootstrap).hex());
        policy["kani"]["targets"][target]["sha256"] =
            toml::Value::String(Digest::of(&bundle).hex());
        policy["kani"]["launcher_sha256"] = toml::Value::String(Digest::of(&launcher).hex());
        fs::write(root.join(CATALOG), toml::to_string(&policy)?)?;
        Ok(PreflightMaterials {
            bytes: BTreeMap::from([
                (catalog.stable.url, stable),
                (catalog.nightly.url, nightly),
                (catalog.rustup.targets[target].url.clone(), bootstrap),
                (catalog.kani.targets[target].url.clone(), bundle),
                (
                    format!(
                        "{CRATE_SERVER}/kani-verifier/kani-verifier-{}.crate",
                        catalog.kani.version
                    ),
                    launcher,
                ),
            ]),
            calls: Cell::new(0),
        })
    }

    const STDLIB_LOCK_INPUT: &str = "cargo-lock:rust-src";
    const STDLIB_ARCHIVE_ROOT: &str = "rust-src-nightly";
    const STDLIB_ARCHIVE_MEMBER: &str = "rust-src/lib/rustlib/src/rust/library/Cargo.lock";
    const STDLIB_LOCK_SHA256: &str =
        "34656569ab979fdf259efffc99d2b68e253e73e0e6fba6762dc51e83df71aa76";
    const STDLIB_LOCK: &[u8] =
        include_bytes!("../../tests/fixtures/rust-src-nightly-library-cargo-lock.txt");
    const STDLIB_REGISTRY_PACKAGES: usize = 30;
    const PROJECT_REGISTRY_PACKAGES: usize = 229;
    const STDLIB_ADDITIONAL_PACKAGES: usize = 21;
    const STDLIB_UNION_PACKAGES: usize = 250;
    const MATERIAL_FIXTURE_NAME: &str = "example";
    const MATERIAL_FIXTURE_VERSION: &str = "1.2.3";
    const MATERIAL_FIXTURE_REGISTRY: &str = "registry+https://github.com/rust-lang/crates.io-index";
    const UNEXECUTED_COMPONENT: &[u8] = b"unexecuted source-bound component fixture";

    fn stdlib_material_archive(root: &str, entries: &[(&str, &[u8])]) -> Result<Vec<u8>> {
        let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut archive = tar::Builder::new(gzip);
        for (name, contents) in entries {
            let mut header = tar::Header::new_ustar();
            header.set_size(u64::try_from(contents.len())?);
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_cksum();
            archive.append_data(&mut header, format!("{root}/{name}"), *contents)?;
        }
        Ok(archive.into_inner()?.finish()?)
    }

    fn stdlib_launcher_archive(version: &str, dependency: Digest) -> Result<Vec<u8>> {
        let manifest = format!(
            "[package]\nname='kani-verifier'\nversion={version:?}\nedition='2024'\n[workspace]\n[dependencies]\n{MATERIAL_FIXTURE_NAME}='={MATERIAL_FIXTURE_VERSION}'\n"
        );
        let lock = format!(
            "version=4\n[[package]]\nname='kani-verifier'\nversion={version:?}\ndependencies=[{MATERIAL_FIXTURE_NAME:?}]\n[[package]]\nname={MATERIAL_FIXTURE_NAME:?}\nversion={MATERIAL_FIXTURE_VERSION:?}\nsource={MATERIAL_FIXTURE_REGISTRY:?}\nchecksum={:?}\n",
            dependency.hex()
        );
        stdlib_material_archive(
            &format!("kani-verifier-{version}"),
            &[
                ("Cargo.toml", manifest.as_bytes()),
                ("Cargo.lock", lock.as_bytes()),
                ("src/main.rs", b"fn main() {}\n"),
            ],
        )
    }

    fn stdlib_source_fixture(root: &Path) -> Result<PreflightMaterials> {
        let source =
            stdlib_material_archive(STDLIB_ARCHIVE_ROOT, &[(STDLIB_ARCHIVE_MEMBER, STDLIB_LOCK)])?;
        stdlib_source_fixture_with_archive(root, &source)
    }

    fn stdlib_source_fixture_with_archive(
        root: &Path,
        source: &[u8],
    ) -> Result<PreflightMaterials> {
        let mut fetch = preflight_fixture(root)?;
        let mut policy: toml::Value = toml::from_str(&fs::read_to_string(root.join(CATALOG))?)?;
        let catalog = NativeRustCatalog::parse(&toml::to_string(&policy)?)?;
        let channel = format!("{BUNDLE_CHANNEL}-{}", triple(Target::MacosArm64));
        let bundle = stdlib_material_archive(
            "kani-fixture",
            &[
                ("rust-toolchain-version", channel.as_bytes()),
                ("rustc-version", BUNDLE_RUSTC.as_bytes()),
            ],
        )?;
        let dependency = stdlib_material_archive(
            &format!("{MATERIAL_FIXTURE_NAME}-{MATERIAL_FIXTURE_VERSION}"),
            &[
                (
                    "Cargo.toml",
                    b"[package]\nname='example'\nversion='1.2.3'\nedition='2024'\n",
                ),
                ("src/lib.rs", b"pub fn value() -> u32 { 37 }\n"),
            ],
        )?;
        let launcher = stdlib_launcher_archive(&catalog.kani.version, Digest::of(&dependency))?;
        for name in ["stable", "nightly"] {
            let url = match name {
                "stable" => &catalog.stable.url,
                _ => &catalog.nightly.url,
            };
            let mut manifest: toml::Value =
                toml::from_str(std::str::from_utf8(&fetch.bytes[url])?)?;
            let packages = manifest["pkg"]
                .as_table_mut()
                .context("fixture component table is missing")?;
            for (component, package) in packages
                .iter_mut()
                .filter(|(name, _)| name.as_str() != "rust")
            {
                let asset = &mut package["target"][triple(Target::MacosArm64)];
                let url = asset["url"]
                    .as_str()
                    .context("fixture asset URL is missing")?
                    .to_owned();
                let bytes = if component == "rust-src" {
                    source.to_vec()
                } else {
                    UNEXECUTED_COMPONENT.to_vec()
                };
                asset["hash"] = toml::Value::String(Digest::of(&bytes).hex());
                fetch.bytes.insert(url, bytes);
            }
            let bytes = toml::to_string(&manifest)?.into_bytes();
            policy[name]["sha256"] = toml::Value::String(Digest::of(&bytes).hex());
            fetch.bytes.insert(url.clone(), bytes);
        }
        let target = Target::MacosArm64.name();
        policy["kani"]["targets"][target]["sha256"] =
            toml::Value::String(Digest::of(&bundle).hex());
        policy["kani"]["launcher_sha256"] = toml::Value::String(Digest::of(&launcher).hex());
        fetch
            .bytes
            .insert(catalog.kani.targets[target].url.clone(), bundle);
        fetch.bytes.insert(
            format!(
                "{CRATE_SERVER}/kani-verifier/kani-verifier-{}.crate",
                catalog.kani.version
            ),
            launcher,
        );
        fetch.bytes.insert(
            format!("{CRATE_SERVER}/{MATERIAL_FIXTURE_NAME}/{MATERIAL_FIXTURE_NAME}-{MATERIAL_FIXTURE_VERSION}.crate"),
            dependency,
        );
        fs::write(root.join(CATALOG), toml::to_string(&policy)?)?;
        Ok(fetch)
    }

    const STDLIB_ONLY_PACKAGE: &str = "stdlib-only";
    const OFFLINE_STDLIB_CONSUMER: &str = "stdlib-consumer";
    const TAR_HEADER_NAME_BYTES: usize = 100;

    fn stdlib_fixture_crate(name: &str) -> Result<Vec<u8>> {
        let manifest = format!(
            "[package]\nname={name:?}\nversion={MATERIAL_FIXTURE_VERSION:?}\nedition='2024'\n"
        );
        stdlib_material_archive(
            &format!("{name}-{MATERIAL_FIXTURE_VERSION}"),
            &[
                ("Cargo.toml", manifest.as_bytes()),
                ("src/lib.rs", b"pub fn value() -> u32 { 37 }\n"),
            ],
        )
    }

    fn stdlib_fixture_lock(name: &str, digest: Digest) -> String {
        format!(
            "version=4\n[[package]]\nname={name:?}\nversion={MATERIAL_FIXTURE_VERSION:?}\nsource={MATERIAL_FIXTURE_REGISTRY:?}\nchecksum={:?}\n",
            digest.hex()
        )
    }

    fn stdlib_fixture_project(root: &Path, dependency: Digest) -> Result<()> {
        fs::write(root.join("Cargo.toml"), "[workspace]\n")?;
        fs::write(
            root.join("Cargo.lock"),
            stdlib_fixture_lock(MATERIAL_FIXTURE_NAME, dependency),
        )?;
        Ok(())
    }

    fn prepared_stdlib_archive_fixture(
        namespace: &Path,
        archive: &[u8],
    ) -> Result<(PathBuf, PreflightMaterials, PreparedRustKani)> {
        let root = namespace.join("source");
        let output = namespace.join("output");
        fs::create_dir(&output)?;
        let fetch = stdlib_source_fixture_with_archive(&root, archive)?;
        let dependency = &fetch.bytes[&format!(
            "{CRATE_SERVER}/{MATERIAL_FIXTURE_NAME}/{MATERIAL_FIXTURE_NAME}-{MATERIAL_FIXTURE_VERSION}.crate"
        )];
        stdlib_fixture_project(&root, Digest::of(dependency))?;
        let prepared = RustKaniWorkload::capture(&root, Target::MacosArm64)?.prepare(
            &Cache::new(namespace.join("cache")),
            &fetch,
            &output,
        )?;
        assert!(!prepared.owned.path().join("home").exists());
        Ok((root, fetch, prepared))
    }

    fn stdlib_fixture_consumer(root: &Path, digest: Digest) -> Result<()> {
        fs::create_dir_all(root.join("src"))?;
        fs::write(
            root.join("Cargo.toml"),
            format!(
                "[package]\nname={OFFLINE_STDLIB_CONSUMER:?}\nversion='1.0.0'\nedition='2024'\n[workspace]\n[dependencies]\n{STDLIB_ONLY_PACKAGE}='={MATERIAL_FIXTURE_VERSION}'\n"
            ),
        )?;
        let lock = format!(
            "version=4\n[[package]]\nname={OFFLINE_STDLIB_CONSUMER:?}\nversion='1.0.0'\ndependencies=[{STDLIB_ONLY_PACKAGE:?}]\n{}",
            stdlib_fixture_lock(STDLIB_ONLY_PACKAGE, digest).trim_start_matches("version=4\n")
        );
        fs::write(root.join("Cargo.lock"), lock)?;
        fs::write(
            root.join("src/main.rs"),
            b"fn main() { println!(\"{}\", stdlib_only::value()); }\n",
        )?;
        Ok(())
    }

    fn stdlib_fixture_cargo(
        consumer: &Path,
        target: &Path,
        prepared: &super::super::PreparedCargo,
        workload: &super::super::CargoWorkload,
    ) -> Result<std::process::Output> {
        let mut command = platform::tool::Tool::Cargo.command();
        command
            .current_dir(consumer)
            .args(["build", "--locked", "--offline", "--jobs", "2"])
            .env("CARGO_TARGET_DIR", target)
            .env("CARGO_INCREMENTAL", "0")
            .env("CARGO_PROFILE_DEV_DEBUG", "0");
        prepared.configure(workload, &mut command)?;
        Ok(command.output()?)
    }

    #[test]
    fn authenticated_stdlib_union_closes_a_fresh_offline_cargo_consumer() -> Result<()> {
        let namespace = tempfile::tempdir_in(platform::paths::canonical(&std::env::temp_dir())?)?;
        let dependency = stdlib_fixture_crate(STDLIB_ONLY_PACKAGE)?;
        let lock = stdlib_fixture_lock(STDLIB_ONLY_PACKAGE, Digest::of(&dependency));
        let archive = stdlib_material_archive(
            STDLIB_ARCHIVE_ROOT,
            &[(STDLIB_ARCHIVE_MEMBER, lock.as_bytes())],
        )?;
        let (root, mut fetch, rust) = prepared_stdlib_archive_fixture(namespace.path(), &archive)?;
        let consumer = namespace.path().join("consumer");
        stdlib_fixture_consumer(&consumer, Digest::of(&dependency))?;
        let cache = Cache::new(namespace.path().join("cache"));
        let project = super::super::CargoWorkload::capture(&root)?;
        let original = cache
            .acquire(&project.plan()?, &fetch)?
            .prepare_cargo(&project, &namespace.path().join("output"))?;
        let missing = stdlib_fixture_cargo(
            &consumer,
            &namespace.path().join("project-only-target"),
            &original,
            &project,
        )?;
        assert!(
            !missing.status.success(),
            "project-only vendor must miss the selected stdlib crate"
        );
        assert!(
            String::from_utf8_lossy(&missing.stderr)
                .contains("no matching package named `stdlib-only` found"),
            "unexpected project-only Cargo failure: {}",
            String::from_utf8_lossy(&missing.stderr)
        );
        let union = project.with_stdlib(&rust)?;
        let plan = union.plan()?;
        assert_eq!(plan.artifacts().len(), 2);
        fetch.bytes.insert(
            format!("{CRATE_SERVER}/{STDLIB_ONLY_PACKAGE}/{STDLIB_ONLY_PACKAGE}-{MATERIAL_FIXTURE_VERSION}.crate"),
            dependency,
        );
        let materials = cache
            .acquire(&plan, &fetch)?
            .prepare_cargo(&union, &namespace.path().join("output"))?;
        let target = namespace.path().join("union-target");
        let output = stdlib_fixture_cargo(&consumer, &target, &materials, &union)?;
        assert!(
            output.status.success(),
            "fresh offline stdlib consumer failed: {:?}\nstdout={}\nstderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let executed = platform::process::command(target.join(format!(
            "debug/{OFFLINE_STDLIB_CONSUMER}{}",
            std::env::consts::EXE_SUFFIX
        )))
        .output()?;
        assert!(executed.status.success());
        assert_eq!(executed.stdout, b"37\n");
        assert!(
            !materials
                .directory()
                .join("cargo-home/registry/index")
                .exists()
        );
        assert!(!rust.owned.path().join("home").exists());
        drop(materials);
        drop(original);
        drop(rust);
        platform::cleanup::report(
            namespace.close(),
            "remove the authenticated offline stdlib fixture",
        );
        Ok(())
    }

    #[test]
    fn authenticated_stdlib_union_refuses_missing_or_poisoned_packages() -> Result<()> {
        let namespace = tempfile::tempdir_in(platform::paths::canonical(&std::env::temp_dir())?)?;
        let dependency = stdlib_fixture_crate(STDLIB_ONLY_PACKAGE)?;
        let lock = stdlib_fixture_lock(STDLIB_ONLY_PACKAGE, Digest::of(&dependency));
        let archive = stdlib_material_archive(
            STDLIB_ARCHIVE_ROOT,
            &[(STDLIB_ARCHIVE_MEMBER, lock.as_bytes())],
        )?;
        let (root, mut fetch, rust) = prepared_stdlib_archive_fixture(namespace.path(), &archive)?;
        let union = super::super::CargoWorkload::capture(&root)?.with_stdlib(&rust)?;
        let plan = union.plan()?;
        let cache = Cache::new(namespace.path().join("cache"));
        let retained_outputs = fs::read_dir(namespace.path().join("output"))?.count();
        let missing = cache
            .acquire(&plan, &fetch)
            .expect_err("missing stdlib crate must refuse");
        assert!(format!("{missing:#}").contains("unexpected preflight fixture fetch"));
        let artifact = plan
            .artifacts()
            .iter()
            .find(|asset| asset.source.contains(&format!("/{STDLIB_ONLY_PACKAGE}/")))
            .expect("the authenticated additional package must be selected");
        fetch
            .bytes
            .insert(artifact.source.clone(), b"poisoned package".to_vec());
        let poison = cache
            .acquire(&plan, &fetch)
            .expect_err("poisoned stdlib crate must refuse");
        assert!(format!("{poison:#}").contains("fetched artifact SHA-256 mismatch"));
        assert!(cache.verify(&plan).is_err());
        assert!(!rust.owned.path().join("home").exists());
        assert_eq!(
            fs::read_dir(namespace.path().join("output"))?.count(),
            retained_outputs
        );
        drop(rust);
        platform::cleanup::report(
            namespace.close(),
            "remove the refused stdlib package fixture",
        );
        Ok(())
    }

    #[test]
    fn authenticated_stdlib_union_refuses_conflicts_and_foreign_source() -> Result<()> {
        let namespace = tempfile::tempdir_in(platform::paths::canonical(&std::env::temp_dir())?)?;
        let archive =
            stdlib_material_archive(STDLIB_ARCHIVE_ROOT, &[(STDLIB_ARCHIVE_MEMBER, STDLIB_LOCK)])?;
        let (root, _, rust) = prepared_stdlib_archive_fixture(namespace.path(), &archive)?;
        let lock: toml::Value = toml::from_str(std::str::from_utf8(STDLIB_LOCK)?)?;
        let selected = lock["package"]
            .as_array()
            .context("original stdlib package set")?
            .iter()
            .find(|package| package.get("source").is_some())
            .context("original stdlib requires a registry package")?;
        let name = selected["name"].as_str().context("selected package name")?;
        let version = selected["version"]
            .as_str()
            .context("selected package version")?;
        fs::write(
            root.join("Cargo.lock"),
            format!(
                "version=4\n[[package]]\nname={name:?}\nversion={version:?}\nsource={MATERIAL_FIXTURE_REGISTRY:?}\nchecksum={:?}\n",
                Digest::of(b"conflicting source package bytes").hex()
            ),
        )?;
        let conflict = super::super::CargoWorkload::capture(&root)?
            .with_stdlib(&rust)
            .expect_err("same package identity with different checksum must refuse");
        assert!(format!("{conflict:#}").contains("Cargo locks disagree about the package bytes"));
        let foreign = namespace.path().join("foreign-source");
        fs::create_dir(&foreign)?;
        stdlib_fixture_project(&foreign, Digest::of(b"foreign project bytes"))?;
        let wrong_root = super::super::CargoWorkload::capture(&foreign)?
            .with_stdlib(&rust)
            .expect_err("an authenticated stdlib input must not transfer to another Source root");
        assert!(format!("{wrong_root:#}").contains("stdlib input belongs to another Source"));
        drop(rust);
        platform::cleanup::report(
            namespace.close(),
            "remove the unrelated stdlib Source fixture",
        );
        Ok(())
    }

    #[test]
    fn authenticated_stdlib_union_rechecks_original_source_and_owned_bytes() -> Result<()> {
        let namespace = tempfile::tempdir_in(platform::paths::canonical(&std::env::temp_dir())?)?;
        let archive =
            stdlib_material_archive(STDLIB_ARCHIVE_ROOT, &[(STDLIB_ARCHIVE_MEMBER, STDLIB_LOCK)])?;
        let (root, _, rust) = prepared_stdlib_archive_fixture(namespace.path(), &archive)?;
        let union = super::super::CargoWorkload::capture(&root)?.with_stdlib(&rust)?;
        let digest = union.plan()?.digest();
        let selected = rust
            .stdlib
            .as_ref()
            .expect("prepared nightly must retain its selected lock");
        for path in [&selected.lock_path, &selected.archive, &selected.manifest] {
            let original = fs::read(path)?;
            fs::write(path, b"changed authenticated input")?;
            assert!(
                union.plan().is_err(),
                "changed {} must refuse",
                path.display()
            );
            fs::write(path, original)?;
            assert_eq!(union.plan()?.digest(), digest);
        }
        let config = root.join("mise.toml");
        let original = fs::read(&config)?;
        fs::write(&config, b"[tools]\nrust='1.99.1'\n")?;
        assert!(union.plan().is_err());
        fs::write(config, original)?;
        assert_eq!(union.plan()?.digest(), digest);
        assert!(!selected.lock_path.starts_with(&root));
        drop(rust);
        assert!(
            union.plan().is_err(),
            "dropping the original owner must not grant continuing input authority"
        );
        platform::cleanup::report(namespace.close(), "remove the changed stdlib input fixture");
        Ok(())
    }

    fn stdlib_linked_archive() -> Result<Vec<u8>> {
        let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut archive = tar::Builder::new(gzip);
        let mut header = tar::Header::new_ustar();
        header.set_size(0);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_link_name("outside-lock")?;
        header.set_cksum();
        archive.append_data(&mut header, STDLIB_MEMBER, std::io::empty())?;
        Ok(archive.into_inner()?.finish()?)
    }

    #[test]
    fn stdlib_archive_refuses_a_regular_trailing_slash_alias() -> Result<()> {
        let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut archive = tar::Builder::new(gzip);
        let mut header = tar::Header::new_ustar();
        let alias = format!("{STDLIB_MEMBER}/");
        assert!(alias.len() < TAR_HEADER_NAME_BYTES);
        header.set_size(u64::try_from(STDLIB_LOCK.len())?);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::Regular);
        header.as_mut_bytes()[..TAR_HEADER_NAME_BYTES].fill(0);
        header.as_mut_bytes()[..alias.len()].copy_from_slice(alias.as_bytes());
        header.set_cksum();
        archive.append(&header, STDLIB_LOCK)?;
        let bytes = archive.into_inner()?.finish()?;
        assert!(
            parse_stdlib_lock(std::io::Cursor::new(bytes), BUNDLE_BYTES, BUNDLE_ENTRIES).is_err(),
            "a regular archive member must retain its exact raw path"
        );
        let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut archive = tar::Builder::new(gzip);
        let mut directory = tar::Header::new_ustar();
        let directory_path = format!("{STDLIB_ARCHIVE_ROOT}/");
        directory.set_size(0);
        directory.set_entry_type(tar::EntryType::Directory);
        directory.as_mut_bytes()[..TAR_HEADER_NAME_BYTES].fill(0);
        directory.as_mut_bytes()[..directory_path.len()].copy_from_slice(directory_path.as_bytes());
        directory.set_cksum();
        archive.append(&directory, std::io::empty())?;
        let mut regular = tar::Header::new_ustar();
        regular.set_size(u64::try_from(STDLIB_LOCK.len())?);
        regular.set_entry_type(tar::EntryType::Regular);
        regular.set_cksum();
        archive.append_data(&mut regular, STDLIB_MEMBER, STDLIB_LOCK)?;
        let bytes = archive.into_inner()?.finish()?;
        assert_eq!(
            parse_stdlib_lock(std::io::Cursor::new(bytes), BUNDLE_BYTES, BUNDLE_ENTRIES)?,
            STDLIB_LOCK,
            "a directory's conventional trailing slash must remain supported"
        );
        Ok(())
    }

    #[test]
    fn verified_rust_src_refuses_missing_duplicate_linked_and_malformed_locks() -> Result<()> {
        let cases = [
            (
                stdlib_material_archive(STDLIB_ARCHIVE_ROOT, &[("README", b"no lock")])?,
                "lacks its exact stdlib Cargo lock",
            ),
            (
                stdlib_material_archive(
                    STDLIB_ARCHIVE_ROOT,
                    &[
                        (STDLIB_ARCHIVE_MEMBER, STDLIB_LOCK),
                        (STDLIB_ARCHIVE_MEMBER, STDLIB_LOCK),
                    ],
                )?,
                "unsafe or duplicate paths",
            ),
            (stdlib_linked_archive()?, "linked or special entry"),
            (
                stdlib_material_archive(
                    STDLIB_ARCHIVE_ROOT,
                    &[(STDLIB_ARCHIVE_MEMBER, b"not a Cargo lock")],
                )?,
                "parse workspace Cargo acquisition lock",
            ),
        ];
        for (archive, expected) in cases {
            let namespace =
                tempfile::tempdir_in(platform::paths::canonical(&std::env::temp_dir())?)?;
            let root = namespace.path().join("source");
            let output = namespace.path().join("output");
            fs::create_dir(&output)?;
            let fetch = stdlib_source_fixture_with_archive(&root, &archive)?;
            let failure = RustKaniWorkload::capture(&root, Target::MacosArm64)?
                .prepare(&Cache::new(namespace.path().join("cache")), &fetch, &output)
                .expect_err(
                    "invalid authenticated rust-src contents must refuse before installation",
                );
            assert!(format!("{failure:#}").contains(expected), "{failure:#}");
            assert_eq!(fs::read_dir(&output)?.count(), 0);
            platform::cleanup::report(
                namespace.close(),
                "remove the malformed stdlib archive fixture",
            );
        }
        Ok(())
    }

    #[test]
    fn installed_stdlib_lock_requires_the_authenticated_regular_original() -> Result<()> {
        let namespace = tempfile::tempdir_in(platform::paths::canonical(&std::env::temp_dir())?)?;
        let archive =
            stdlib_material_archive(STDLIB_ARCHIVE_ROOT, &[(STDLIB_ARCHIVE_MEMBER, STDLIB_LOCK)])?;
        let (_, _, rust) = prepared_stdlib_archive_fixture(namespace.path(), &archive)?;
        assert!(rust.require_installed_stdlib().is_err());
        let path = rust
            .owned
            .path()
            .join("home/.rustup/toolchains")
            .join(format!(
                "{}-{}",
                rust.workload.catalog.nightly.channel,
                triple(rust.workload.target)
            ))
            .join(INSTALLED_STDLIB_LOCK);
        fs::create_dir_all(path.parent().context("owned installed stdlib path")?)?;
        fs::write(&path, STDLIB_LOCK)?;
        rust.require_installed_stdlib()?;
        fs::write(&path, b"changed installed Cargo lock")?;
        let changed = rust
            .require_installed_stdlib()
            .expect_err("installed lock drift must refuse");
        assert!(format!("{changed:#}").contains("installed stdlib Cargo lock differs"));
        fs::remove_file(&path)?;
        fs::create_dir(&path)?;
        assert!(rust.require_installed_stdlib().is_err());
        drop(rust);
        platform::cleanup::report(
            namespace.close(),
            "remove the unexecuted installed-lock fixture",
        );
        Ok(())
    }

    #[test]
    fn verified_rust_src_captures_its_original_cargo_lock_before_installation() -> Result<()> {
        assert_eq!(Digest::of(STDLIB_LOCK).hex(), STDLIB_LOCK_SHA256);
        let namespace = tempfile::tempdir_in(platform::paths::canonical(&std::env::temp_dir())?)?;
        let root = namespace.path().join("source");
        let output = namespace.path().join("output");
        fs::create_dir(&output)?;
        let fetch = stdlib_source_fixture(&root)?;
        let prepared = RustKaniWorkload::capture(&root, Target::MacosArm64)?.prepare(
            &Cache::new(namespace.path().join("cache")),
            &fetch,
            &output,
        )?;
        prepared.current()?;
        assert!(!prepared.owned.path().join("home").exists());
        let selected = prepared
            .captured
            .get(STDLIB_LOCK_INPUT)
            .expect("verified rust-src must retain its authenticated original Cargo lock before installation");
        assert!(selected.starts_with(prepared.owned.path().join("inputs")));
        assert_eq!(fs::read(selected)?, STDLIB_LOCK);
        prepared.current()?;
        drop(prepared);
        platform::cleanup::report(
            namespace.close(),
            "remove the unexecuted stdlib material fixture",
        );
        Ok(())
    }

    #[test]
    fn published_stdlib_lock_has_the_exact_source_bound_package_union() -> Result<()> {
        assert_eq!(Digest::of(STDLIB_LOCK).hex(), STDLIB_LOCK_SHA256);
        let namespace = tempfile::tempdir()?;
        fs::write(namespace.path().join("Cargo.toml"), "[workspace]\n")?;
        fs::write(namespace.path().join("Cargo.lock"), STDLIB_LOCK)?;
        let stdlib = super::super::CargoWorkload::capture(namespace.path())?.plan()?;
        stdlib.complete()?;
        assert_eq!(stdlib.artifacts().len(), STDLIB_REGISTRY_PACKAGES);
        let project = super::super::CargoWorkload::capture(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
        )?
        .plan()?;
        project.complete()?;
        assert_eq!(project.artifacts().len(), PROJECT_REGISTRY_PACKAGES);
        let mut identities: BTreeSet<_> =
            project.artifacts().iter().map(Artifact::subject).collect();
        let prior = identities.len();
        for artifact in stdlib.artifacts() {
            if let Some(shared) = project
                .artifacts()
                .iter()
                .find(|entry| entry.subject() == artifact.subject())
            {
                assert_eq!(shared.expected(), artifact.expected());
            }
            identities.insert(artifact.subject());
        }
        assert_eq!(identities.len() - prior, STDLIB_ADDITIONAL_PACKAGES);
        assert_eq!(identities.len(), STDLIB_UNION_PACKAGES);
        platform::cleanup::report(
            namespace.close(),
            "remove the data-only stdlib lock fixture",
        );
        Ok(())
    }

    #[test]
    fn native_preflight_refuses_an_incompatible_bundle_before_launcher_preparation() -> Result<()> {
        let namespace = tempfile::tempdir_in(platform::paths::canonical(&std::env::temp_dir())?)?;
        let root = namespace.path().join("source");
        let output = namespace.path().join("output");
        fs::create_dir(&output)?;
        let fetch = preflight_fixture(&root)?;
        let failure = RustKaniWorkload::capture(&root, Target::MacosArm64)?
            .prepare(&Cache::new(namespace.path().join("cache")), &fetch, &output)
            .expect_err("an incompatible bundle must stop material preparation");
        assert_eq!(fetch.calls.get(), 5);
        assert_eq!(
            failure.to_string(),
            "Kani bundle and pinned compiler distribution differ"
        );
        platform::cleanup::report(namespace.close(), "remove the native preflight fixture");
        Ok(())
    }

    fn bundle_protocol_fixture(
        target: Target,
    ) -> Result<(RustKaniWorkload, RustDistributionMetadata)> {
        let catalog = NativeRustCatalog::parse(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../policy/native-materials.toml"
        )))?;
        assert_eq!(catalog.nightly.channel, BUNDLE_CHANNEL);
        let nightly = RustDistributionMetadata {
            date: "2026-08-21".into(),
            rust_version: BUNDLE_RUSTC.trim_start_matches("rustc ").into(),
            rustc_version: BUNDLE_RUSTC.trim_start_matches("rustc ").into(),
            versions: BTreeMap::new(),
            assets: Vec::new(),
        };
        Ok((
            RustKaniWorkload {
                root: PathBuf::new(),
                target,
                inputs: BTreeMap::new(),
                catalog,
                stable_components: Vec::new(),
            },
            nightly,
        ))
    }

    #[test]
    fn kani_bundle_accepts_the_exact_target_qualified_distribution() -> Result<()> {
        let accepted = UNIX_TARGETS
            .into_iter()
            .map(|target| {
                let (workload, nightly) = bundle_protocol_fixture(target)?;
                let metadata = KaniBundleMetadata {
                    channel: format!("{BUNDLE_CHANNEL}-{}", triple(target)),
                    rustc: BUNDLE_RUSTC.into(),
                };
                Ok((
                    target,
                    workload
                        .require_bundle_distribution(&metadata, &nightly)
                        .is_ok(),
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        assert!(
            accepted.iter().all(|(_, accepted)| *accepted),
            "{accepted:?}"
        );
        Ok(())
    }

    #[test]
    fn kani_bundle_refuses_missing_or_different_native_identity() -> Result<()> {
        let mut refused = Vec::new();
        for target in UNIX_TARGETS {
            let (workload, nightly) = bundle_protocol_fixture(target)?;
            for foreign in UNIX_TARGETS
                .into_iter()
                .filter(|foreign| *foreign != target)
            {
                let metadata = KaniBundleMetadata {
                    channel: format!("{BUNDLE_CHANNEL}-{}", triple(foreign)),
                    rustc: BUNDLE_RUSTC.into(),
                };
                refused.push(
                    workload
                        .require_bundle_distribution(&metadata, &nightly)
                        .is_err(),
                );
            }
            for (channel, rustc) in [
                (BUNDLE_CHANNEL.into(), BUNDLE_RUSTC.into()),
                (
                    format!("nightly-2026-08-20-{}", triple(target)),
                    BUNDLE_RUSTC.into(),
                ),
                (
                    format!("{BUNDLE_CHANNEL}-{}", triple(Target::WindowsX64)),
                    BUNDLE_RUSTC.into(),
                ),
                (
                    format!("{BUNDLE_CHANNEL}-{}", triple(target)),
                    "rustc 1.99.1 (58cf8f9c3 2026-10-13)".into(),
                ),
                (
                    format!("{BUNDLE_CHANNEL}-{}", triple(target)),
                    "rustc 1.100.0-nightly (000000000 2026-08-20)".into(),
                ),
            ] {
                let metadata = KaniBundleMetadata { channel, rustc };
                refused.push(
                    workload
                        .require_bundle_distribution(&metadata, &nightly)
                        .is_err(),
                );
            }
        }
        assert!(refused.iter().all(|refused| *refused), "{refused:?}");
        Ok(())
    }

    #[test]
    fn original_health_failure_preserves_diagnostics_and_does_not_call_the_body() {
        let calls = Cell::new(0);
        let error = health_before_body(
            || {
                Err(anyhow::Error::new(NativeMaterialFailure {
                    operation: NativeMaterialOperation {
                        name: "original controlled health refusal".into(),
                        status: Some(17),
                        stdout: b"original stdout".to_vec(),
                        stderr: b"original stderr".to_vec(),
                        stdout_complete: true,
                        stderr_complete: true,
                    },
                }))
            },
            || {
                calls.set(calls.get() + 1);
                Ok(())
            },
        )
        .unwrap_err();
        assert_eq!(calls.get(), 0);
        let original = error
            .downcast_ref::<NativeMaterialFailure>()
            .expect("the original health failure must remain available");
        assert_eq!(original.operation.status, Some(17));
        assert_eq!(original.operation.stdout, b"original stdout");
        assert_eq!(original.operation.stderr, b"original stderr");
    }

    #[test]
    fn original_health_must_run_once_before_the_body() -> Result<()> {
        let calls = Cell::new(0);
        health_before_body(
            || {
                calls.set(calls.get() + 1);
                Ok(())
            },
            || {
                assert_eq!(calls.get(), 1);
                calls.set(calls.get() + 1);
                Ok(())
            },
        )?;
        assert_eq!(calls.get(), 2);
        Ok(())
    }

    #[test]
    fn distribution_inventory_excludes_mutable_outputs_and_detects_program_changes() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let root = platform::paths::canonical(scope.path())?;
        for file in [
            "home/.rustup/toolchains/test/bin/rustc",
            "home/.cargo/bin/rustup",
            "launcher-install/bin/kani",
            "home/.kani/kani-0.68.0/bin/kani-driver",
        ] {
            let path = root.join(file);
            fs::create_dir_all(path.parent().context("owned fixture program parent")?)?;
            fs::write(path, b"controlled immutable distribution bytes")?;
        }
        let before = installed_tree(&root, KANI_VERSION)?;
        assert_eq!(before.len(), 4);
        for file in [
            "tmp/health.stdout",
            "home/.rustup/tmp/lock",
            "home/.cargo/registry/cache",
            "home/.kani/diagnostics/runtime",
            "launcher-install/.crates.toml",
        ] {
            let path = root.join(file);
            fs::create_dir_all(path.parent().context("owned fixture output parent")?)?;
            fs::write(path, b"controlled mutable operation output")?;
        }
        assert_eq!(installed_tree(&root, KANI_VERSION)?, before);
        fs::write(
            root.join("home/.kani/kani-0.68.0/bin/kani-driver"),
            b"changed distribution program",
        )?;
        assert_ne!(installed_tree(&root, KANI_VERSION)?, before);
        platform::cleanup::report(
            scope.close(),
            "remove the selected native inventory fixture",
        );
        Ok(())
    }

    #[test]
    fn distribution_inventory_rejects_a_link_outside_its_owned_namespace() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let root = platform::paths::canonical(scope.path())?;
        let distribution = root.join("home/.rustup/toolchains");
        fs::create_dir_all(&distribution)?;
        let outside = tempfile::tempdir()?;
        let outside_path = platform::paths::canonical(outside.path())?;
        fs::write(
            outside_path.join("foreign-rustc"),
            b"controlled foreign program",
        )?;
        let link = distribution.join("redirected-toolchain");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside_path, &link)?;
        #[cfg(windows)]
        {
            let mut command = platform::tool::Tool::Pwsh.command();
            command.args(["-NoLogo", "-NoProfile", "-Command", "& { param([string]$Link,[string]$Destination) New-Item -ItemType Junction -Path $Link -Value $Destination -ErrorAction Stop | Out-Null }"]).arg(&link).arg(&outside_path);
            operation(
                command,
                &root,
                "create the controlled escaped installation junction",
            )?;
        }
        let failure = installed_tree(&root, KANI_VERSION)
            .expect_err("a real directory link cannot leave the owned native namespace");
        assert!(format!("{failure:#}").contains("escapes its owned namespace"));
        platform::cleanup::report(
            scope.close(),
            "remove the controlled escaped native namespace",
        );
        platform::cleanup::report(
            outside.close(),
            "remove the controlled foreign native directory",
        );
        Ok(())
    }

    fn run_environment_child(mode: &str) -> Result<()> {
        let scope = tempfile::tempdir()?;
        let root = platform::paths::canonical(scope.path())?;
        for directory in ["home", "mirror", "mirror/rustup", "tmp"] {
            fs::create_dir_all(root.join(directory))?;
        }
        let mut command = native_command(&root, &std::env::current_exe()?, RUST_VERSION)?;
        command
            .args(["--exact", ENVIRONMENT_TEST, "--nocapture"])
            .env(MODE, mode)
            .env(SCOPE, &root);
        let observed = operation(command, &root, "controlled native child environment")?;
        assert_eq!(observed.status, Some(0));
        platform::cleanup::report(scope.close(), "remove the owned native environment fixture");
        Ok(())
    }

    #[test]
    fn owned_native_commands_remove_ambient_store_and_compiler_overrides() -> Result<()> {
        match std::env::var(MODE).ok().as_deref() {
            Some("inspect") => {
                let root = PathBuf::from(
                    std::env::var_os(SCOPE).context("owned native fixture scope is missing")?,
                );
                for name in OVERRIDES {
                    let expected = if cfg!(windows) && name == "rustup_home" {
                        Some(root.join("home/.rustup"))
                    } else {
                        None
                    };
                    assert_eq!(
                        std::env::var_os(name).map(PathBuf::from),
                        expected,
                        "ambient override {name}"
                    );
                }
                assert_eq!(std::env::var_os("HOME"), Some(root.join("home").into()));
                assert_eq!(
                    std::env::var_os("RUSTUP_HOME").map(PathBuf::from),
                    Some(root.join("home/.rustup"))
                );
                assert_eq!(
                    std::env::var("RUSTUP_TOOLCHAIN").as_deref(),
                    Ok(RUST_VERSION)
                );
                for (name, value) in [
                    ("RUSTUP_AUTO_INSTALL", "0"),
                    ("CARGO_NET_OFFLINE", "true"),
                    ("MISE_OFFLINE", "true"),
                    ("MISE_PARANOID", "true"),
                    ("MISE_AUTO_INSTALL", "false"),
                    ("MISE_AUTO_PRUNE", "false"),
                ] {
                    assert_eq!(std::env::var(name).as_deref(), Ok(value));
                }
                assert_eq!(
                    std::env::var("RUSTUP_DIST_SERVER")?,
                    url::Url::from_directory_path(root.join("mirror"))
                        .map_err(|()| anyhow::anyhow!(
                            "fixture mirror is not an absolute directory"
                        ))?
                        .to_string()
                );
            }
            Some("prepare") => run_environment_child("inspect")?,
            None => {
                let mut command = platform::process::command(std::env::current_exe()?);
                command
                    .args(["--exact", ENVIRONMENT_TEST, "--nocapture"])
                    .env(MODE, "prepare");
                for name in OVERRIDES {
                    command.env(name, "controlled inherited override");
                }
                let output = command.output()?;
                ensure!(
                    output.status.success(),
                    "controlled inherited native environment failed: {}; stderr={}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            Some(_) => anyhow::bail!("owned native environment fixture mode is unknown"),
        }
        Ok(())
    }
}

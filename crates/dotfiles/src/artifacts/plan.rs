use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const MISE_LOCK_VERSION: u32 = 3;
const CARGO_LOCK_VERSION: u32 = 4;
const PLAN_SCHEMA: u32 = 1;
const MISE_RELEASES: &str = "https://github.com/jdx/mise/releases/download";
const CRATE_RELEASES: &str = "https://static.crates.io/crates";
const CRATES_IO: &str = "registry+https://github.com/rust-lang/crates.io-index";
const MISE_CONFIG: &str = "mise.toml";
const MISE_LOCK: &str = "mise.lock";
const CARGO_LOCK: &str = "Cargo.lock";
const RELEASE_PINS: &str = "home/.chezmoidata/versions.toml";
const WORKSPACE_CARGO: &str = "workspace-cargo";
const BOOTSTRAP: &str = "mise-bootstrap";
const HTTP_BACKEND: &str = "http:";
const HTTP_LOCK_OPTIONS: [&str; 3] = ["format", "strip_components", "rename_exe"];
const NATIVE_IDENTITY_PAIR_LENGTH: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Digest([u8; 32]);

impl TryFrom<String> for Digest {
    type Error = String;

    fn try_from(text: String) -> std::result::Result<Self, Self::Error> {
        rules::artifact_rules::sha256(text.as_bytes())
            .map(Self)
            .ok_or_else(|| "digest requires 64 lowercase SHA-256 hex digits".into())
    }
}

impl From<Digest> for String {
    fn from(digest: Digest) -> Self {
        digest.hex()
    }
}

impl Digest {
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    #[must_use]
    pub fn hex(self) -> String {
        platform::build_inputs::hex(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    LinuxArm64,
    LinuxX64,
    MacosArm64,
    MacosX64,
    WindowsX64,
}

impl Target {
    pub const ALL: [Self; 5] = [
        Self::LinuxArm64,
        Self::LinuxX64,
        Self::MacosArm64,
        Self::MacosX64,
        Self::WindowsX64,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::LinuxArm64 => "linux-arm64",
            Self::LinuxX64 => "linux-x64",
            Self::MacosArm64 => "macos-arm64",
            Self::MacosX64 => "macos-x64",
            Self::WindowsX64 => "windows-x64",
        }
    }

    const fn os(self) -> &'static str {
        match self {
            Self::LinuxArm64 | Self::LinuxX64 => "linux",
            Self::MacosArm64 | Self::MacosX64 => "macos",
            Self::WindowsX64 => "windows",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Request {
    pub targets: Vec<Target>,
    pub tools: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Artifact {
    pub(super) subject: String,
    pub(super) target: Option<Target>,
    pub(super) version: String,
    pub(super) source: String,
    pub(super) expected: Digest,
    pub(super) provenance: Digest,
}

impl Artifact {
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    #[must_use]
    pub const fn expected(&self) -> Digest {
        self.expected
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    MissingTool,
    UnpinnedVersion,
    VersionMismatch,
    MissingPlatform,
    MissingDigest,
    UnsupportedChecksum,
    UnsupportedProvenance,
    UnsupportedBackend,
    InvalidSource,
    MissingDependency,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub subject: String,
    pub reason: Reason,
}

#[derive(Clone, Copy, Debug)]
pub struct Inputs<'a> {
    pub project: &'a str,
    pub mise_lock: &'a str,
    pub cargo_lock: &'a str,
    pub versions: &'a str,
}

pub(super) struct AdditionalCargoLock<'a> {
    pub name: &'a str,
    pub text: &'a str,
    pub origin: Digest,
}

#[derive(Debug, Serialize)]
pub struct Plan {
    schema: u32,
    scope: &'static str,
    request: Request,
    inputs: BTreeMap<String, Digest>,
    artifacts: Vec<Artifact>,
    diagnostics: Vec<Diagnostic>,
    digest: Digest,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Lockfile {
    #[serde(rename = "lockfile_version")]
    version: u32,
    tools: BTreeMap<String, Vec<LockedTool>>,
    #[serde(default, rename = "conda-packages")]
    conda_packages: BTreeMap<String, BTreeMap<String, LockedArtifact>>,
}

#[derive(Debug, Deserialize)]
struct LockedTool {
    version: String,
    backend: String,
    specifiers: Vec<String>,
    #[serde(default)]
    options: BTreeMap<String, toml::Value>,
    uv: Option<NestedLock>,
    #[serde(flatten)]
    platforms: BTreeMap<String, LockedArtifact>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NestedLock {
    path: String,
    digest: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LockedArtifact {
    url: Option<String>,
    checksum: Option<String>,
    url_api: Option<String>,
    provenance: Option<String>,
    #[serde(default)]
    conda_deps: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CargoLock {
    version: u32,
    #[serde(default)]
    package: Vec<CargoPackage>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CargoPackage {
    name: String,
    version: String,
    source: Option<String>,
    checksum: Option<String>,
    #[serde(default)]
    dependencies: Vec<String>,
}

impl Plan {
    pub(super) fn native_materials(
        scope: &'static str,
        inputs: BTreeMap<String, Digest>,
        mut artifacts: Vec<Artifact>,
    ) -> Result<Self> {
        artifacts.sort_by(|left, right| left.subject.cmp(&right.subject));
        ensure!(
            !artifacts.is_empty()
                && artifacts
                    .windows(NATIVE_IDENTITY_PAIR_LENGTH)
                    .all(|pair| pair[0].subject != pair[1].subject),
            "native materials require distinct nonempty identities"
        );
        let request = Request {
            targets: artifacts
                .iter()
                .filter_map(|asset| asset.target)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            tools: artifacts
                .iter()
                .map(|asset| asset.subject.clone())
                .collect(),
        };
        let diagnostics = Vec::new();
        let digest = Digest::of(&serde_json::to_vec(&(
            PLAN_SCHEMA,
            scope,
            &request,
            &inputs,
            &artifacts,
            &diagnostics,
        ))?);
        Ok(Self {
            schema: PLAN_SCHEMA,
            scope,
            request,
            inputs,
            artifacts,
            diagnostics,
            digest,
        })
    }

    pub(super) fn cargo_closure(inputs: &BTreeMap<String, String>) -> Result<Self> {
        Self::cargo_closure_with(inputs, &[])
    }

    pub(super) fn cargo_closure_with(
        inputs: &BTreeMap<String, String>,
        additional: &[AdditionalCargoLock<'_>],
    ) -> Result<Self> {
        let mut plan = Self {
            schema: PLAN_SCHEMA,
            scope: "source-cargo-closure",
            request: Request::default(),
            inputs: BTreeMap::new(),
            artifacts: Vec::new(),
            diagnostics: Vec::new(),
            digest: Digest::of(b""),
        };
        for (path, text) in inputs {
            plan.inputs
                .insert(path.clone(), Digest::of(text.as_bytes()));
            if path.rsplit('/').next() == Some(CARGO_LOCK) {
                plan.cargo_named(path, text)?;
            }
        }
        for lock in additional {
            ensure!(
                plan.inputs
                    .insert(
                        format!("{}:lock", lock.name),
                        Digest::of(lock.text.as_bytes())
                    )
                    .is_none()
                    && plan
                        .inputs
                        .insert(format!("{}:origin", lock.name), lock.origin)
                        .is_none(),
                "authenticated Cargo input namespace collides with Source"
            );
            plan.cargo_named(lock.name, lock.text)?;
        }
        plan.artifacts
            .sort_by(|left, right| left.subject.cmp(&right.subject));
        let mut unique: Vec<Artifact> = Vec::new();
        for artifact in plan.artifacts {
            if let Some(previous) = unique
                .last()
                .filter(|item| item.subject == artifact.subject)
            {
                ensure!(
                    previous.expected == artifact.expected && previous.source == artifact.source,
                    "{}: Cargo locks disagree about the package bytes",
                    artifact.subject
                );
            } else {
                unique.push(artifact);
            }
        }
        plan.artifacts = unique;
        plan.diagnostics
            .sort_by(|left, right| left.subject.cmp(&right.subject));
        plan.digest = Digest::of(&serde_json::to_vec(&(
            plan.schema,
            plan.scope,
            &plan.inputs,
            &plan.artifacts,
            &plan.diagnostics,
        ))?);
        Ok(plan)
    }

    /// Read the declared project scope without granting native installation credit.
    ///
    /// # Errors
    ///
    /// Rejects malformed lockfiles and ambiguous pinned identities.
    pub fn parse(inputs: Inputs<'_>, mut request: Request) -> Result<Self> {
        if request.targets.is_empty() {
            request.targets.extend(Target::ALL);
        }
        request.targets.sort();
        request.targets.dedup();
        request.tools.sort();
        request.tools.dedup();
        let mut plan = Self {
            schema: PLAN_SCHEMA,
            scope: "project-pins-and-workspace-cargo",
            request,
            inputs: BTreeMap::new(),
            artifacts: Vec::new(),
            diagnostics: Vec::new(),
            digest: Digest::of(b""),
        };
        for (name, bytes) in [
            (MISE_CONFIG, inputs.project),
            (MISE_LOCK, inputs.mise_lock),
            (CARGO_LOCK, inputs.cargo_lock),
            (RELEASE_PINS, inputs.versions),
        ] {
            plan.inputs
                .insert(name.into(), Digest::of(bytes.as_bytes()));
        }
        let project: toml::Table =
            toml::from_str(inputs.project).context("parse project mise pins")?;
        let tools = project
            .get("tools")
            .and_then(toml::Value::as_table)
            .context("mise.toml requires a tools table")?;
        let lock: Lockfile =
            toml::from_str(inputs.mise_lock).context("parse mise acquisition lock")?;
        ensure!(
            lock.version == MISE_LOCK_VERSION,
            "unsupported mise lockfile version"
        );
        plan.mise(tools, &lock)?;
        if plan.request.tools.is_empty() || plan.request.tools.iter().any(|tool| tool == BOOTSTRAP)
        {
            plan.bootstrap(inputs.versions)?;
        }
        if plan.request.tools.is_empty()
            || plan
                .request
                .tools
                .iter()
                .any(|tool| tool == WORKSPACE_CARGO)
        {
            plan.cargo(inputs.cargo_lock)?;
        }
        for name in plan.request.tools.clone() {
            if !tools.contains_key(&name) && name != BOOTSTRAP && name != WORKSPACE_CARGO {
                plan.refuse(name, Reason::MissingTool);
            }
        }
        plan.artifacts
            .sort_by(|left, right| left.subject.cmp(&right.subject));
        plan.diagnostics
            .sort_by(|left, right| left.subject.cmp(&right.subject));
        plan.digest = Digest::of(&serde_json::to_vec(&(
            &plan.schema,
            &plan.scope,
            &plan.request,
            &plan.inputs,
            &plan.artifacts,
            &plan.diagnostics,
        ))?);
        Ok(plan)
    }

    #[must_use]
    pub fn artifacts(&self) -> &[Artifact] {
        &self.artifacts
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    #[must_use]
    pub const fn digest(&self) -> Digest {
        self.digest
    }

    /// Require complete acquisition metadata for this command's declared scope.
    ///
    /// # Errors
    ///
    /// Refuses unresolved and empty scopes without changing native stage evidence.
    pub fn complete(&self) -> Result<()> {
        ensure!(
            rules::artifact_rules::admission(
                self.artifacts.len(),
                self.artifacts.len(),
                self.diagnostics.len()
            ) == rules::artifact_rules::Admission::Ready,
            "acquisition plan is empty or unresolved; inspect its diagnostics"
        );
        Ok(())
    }

    fn refuse(&mut self, subject: String, reason: Reason) {
        self.diagnostics.push(Diagnostic { subject, reason });
    }

    fn mise(&mut self, tools: &toml::Table, lock: &Lockfile) -> Result<()> {
        for (name, pin) in tools {
            if !self.request.tools.is_empty() && !self.request.tools.contains(name) {
                continue;
            }
            let os: Option<Vec<String>> = pin
                .get("os")
                .map(|os| os.clone().try_into())
                .transpose()
                .context("tool os filters require strings")?;
            let targets: Vec<_> = self
                .request
                .targets
                .iter()
                .copied()
                .filter(|target| {
                    os.as_ref()
                        .is_none_or(|os| os.iter().any(|name| name == target.os()))
                })
                .collect();
            match rules::artifact_rules::target_selection(
                !targets.is_empty(),
                self.request.tools.contains(name),
            ) {
                rules::artifact_rules::TargetSelection::Required => {}
                rules::artifact_rules::TargetSelection::Excluded => continue,
                rules::artifact_rules::TargetSelection::Refused => {
                    self.refuse(name.clone(), Reason::MissingPlatform);
                    continue;
                }
            }
            let requested = pin
                .as_str()
                .or_else(|| pin.get("version").and_then(toml::Value::as_str));
            let Some(requested) =
                requested.filter(|version| rules::artifact_rules::pinned(version.as_bytes()))
            else {
                self.refuse(name.clone(), Reason::UnpinnedVersion);
                continue;
            };
            let Some(versions) = lock.tools.get(name) else {
                self.refuse(name.clone(), Reason::MissingTool);
                continue;
            };
            let matched: Vec<_> = versions
                .iter()
                .filter(|version| {
                    version
                        .specifiers
                        .iter()
                        .any(|specifier| specifier == requested)
                })
                .collect();
            let [version] = matched.as_slice() else {
                self.refuse(name.clone(), Reason::VersionMismatch);
                continue;
            };
            if version.version != requested || version.backend.is_empty() {
                self.refuse(name.clone(), Reason::VersionMismatch);
                continue;
            }
            if name.contains(':') && version.backend != *name {
                self.refuse(name.clone(), Reason::VersionMismatch);
                continue;
            }
            if !Self::options_match(pin, version, &targets) {
                self.refuse(name.clone(), Reason::VersionMismatch);
                continue;
            }
            ensure!(
                version
                    .platforms
                    .keys()
                    .all(|key| key.starts_with("platforms.")),
                "{name}: unknown acquisition lock fields"
            );
            if let Some(uv) = &version.uv {
                self.refuse(
                    format!("{name}: nested lock {} ({})", uv.path, uv.digest),
                    Reason::UnsupportedBackend,
                );
                continue;
            }
            self.materials(name, requested, version, &targets, lock)?;
        }
        Ok(())
    }

    fn options_match(pin: &toml::Value, version: &LockedTool, targets: &[Target]) -> bool {
        if version.backend.starts_with(HTTP_BACKEND) {
            return Self::http_options_match(pin, version, targets);
        }
        pin.as_table().is_none_or(|options| {
            options
                .iter()
                .filter(|(key, _)| !matches!(key.as_str(), "version" | "os"))
                .all(|(key, value)| {
                    let normalized = match value {
                        toml::Value::String(value) => Some(value.clone()),
                        toml::Value::Boolean(value) => Some(value.to_string()),
                        toml::Value::Integer(value) => Some(value.to_string()),
                        toml::Value::Array(values) => values
                            .iter()
                            .map(toml::Value::as_str)
                            .collect::<Option<Vec<_>>>()
                            .map(|values| values.join(",")),
                        toml::Value::Float(_)
                        | toml::Value::Datetime(_)
                        | toml::Value::Table(_) => None,
                    };
                    normalized.as_ref().is_some_and(|value| {
                        version.options.get(key).and_then(toml::Value::as_str)
                            == Some(value.as_str())
                    })
                })
        })
    }

    fn http_options_match(pin: &toml::Value, version: &LockedTool, targets: &[Target]) -> bool {
        let mut normalized = BTreeMap::new();
        if let Some(options) = pin.as_table() {
            for (name, value) in options {
                if matches!(name.as_str(), "version" | "os") {
                    continue;
                }
                if matches!(name.as_str(), "url" | "checksum") {
                    if value.as_str().is_none() {
                        return false;
                    }
                    continue;
                }
                if !HTTP_LOCK_OPTIONS.contains(&name.as_str()) {
                    return false;
                }
                let value = if name == "strip_components" {
                    match value {
                        toml::Value::String(value) => {
                            value.parse::<usize>().ok().map(|_| value.clone())
                        }
                        toml::Value::Integer(value) => {
                            usize::try_from(*value).ok().map(|value| value.to_string())
                        }
                        _ => None,
                    }
                } else {
                    value.as_str().map(str::to_owned)
                };
                let Some(value) = value else {
                    return false;
                };
                normalized.insert(name, value);
            }
        }
        let options_match = version.options.len() == normalized.len()
            && normalized.iter().all(|(name, value)| {
                version.options.get(*name).and_then(toml::Value::as_str) == Some(value.as_str())
            });
        let matches = |name| {
            targets.iter().all(|target| {
                version
                    .platforms
                    .get(&format!("platforms.{}", target.name()))
                    .is_none_or(|artifact| {
                        let locked = match name {
                            "url" => artifact.url.as_deref(),
                            "checksum" => artifact.checksum.as_deref(),
                            _ => None,
                        };
                        pin.get(name).is_none_or(|value| {
                            value.as_str().is_some_and(|value| Some(value) == locked)
                        })
                    })
            })
        };
        rules::artifact_rules::http_binding(options_match, matches("url"), matches("checksum"))
            == rules::artifact_rules::HttpBinding::Matched
    }

    fn materials(
        &mut self,
        name: &str,
        requested: &str,
        version: &LockedTool,
        targets: &[Target],
        lock: &Lockfile,
    ) -> Result<()> {
        let identity = serde_json::to_vec(&(
            name,
            requested,
            &version.version,
            &version.backend,
            &version.options,
        ))?;
        for &target in targets {
            let key = format!("platforms.{}", target.name());
            let subject = format!("{name}@{}:{}", version.version, target.name());
            let Some(asset) = version.platforms.get(&key) else {
                self.refuse(
                    subject,
                    if version.platforms.is_empty() {
                        Reason::UnsupportedBackend
                    } else {
                        Reason::MissingPlatform
                    },
                );
                continue;
            };
            self.asset(
                subject,
                Some(target),
                &version.version,
                asset,
                MISE_LOCK,
                &identity,
            )?;
            for dependency in &asset.conda_deps {
                let subject = format!("{name}:{dependency}:{}", target.name());
                if let Some(asset) = lock
                    .conda_packages
                    .get(target.name())
                    .and_then(|packages| packages.get(dependency))
                {
                    self.asset(
                        subject.clone(),
                        Some(target),
                        dependency,
                        asset,
                        MISE_LOCK,
                        &identity,
                    )?;
                    for nested in &asset.conda_deps {
                        self.refuse(format!("{subject}:{nested}"), Reason::UnsupportedBackend);
                    }
                } else {
                    self.refuse(subject, Reason::MissingDependency);
                }
            }
        }
        Ok(())
    }

    fn asset(
        &mut self,
        subject: String,
        target: Option<Target>,
        version: &str,
        asset: &LockedArtifact,
        input: &str,
        identity: &[u8],
    ) -> Result<()> {
        let Some(checksum) = &asset.checksum else {
            self.refuse(subject, Reason::MissingDigest);
            return Ok(());
        };
        let Some(hex) = checksum.strip_prefix("sha256:") else {
            self.refuse(subject, Reason::UnsupportedChecksum);
            return Ok(());
        };
        let Ok(expected) = Digest::try_from(hex.to_owned()) else {
            self.refuse(subject, Reason::MissingDigest);
            return Ok(());
        };
        if asset.provenance.is_some() {
            self.refuse(subject, Reason::UnsupportedProvenance);
            return Ok(());
        }
        let Some(source) = &asset.url else {
            self.refuse(subject, Reason::InvalidSource);
            return Ok(());
        };
        let Ok(url) = url::Url::parse(source) else {
            self.refuse(subject, Reason::InvalidSource);
            return Ok(());
        };
        if url.scheme() != "https"
            || !url.has_host()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || url.query().is_some()
        {
            self.refuse(subject, Reason::InvalidSource);
            return Ok(());
        }
        let source = String::from(url);
        let provenance = Digest::of(&serde_json::to_vec(&(
            input,
            &subject,
            target,
            version,
            &source,
            expected,
            identity,
            &asset.url_api,
        ))?);
        self.artifacts.push(Artifact {
            subject,
            target,
            version: version.into(),
            source,
            expected,
            provenance,
        });
        Ok(())
    }

    fn bootstrap(&mut self, versions: &str) -> Result<()> {
        let data = model::data::parse(RELEASE_PINS, versions).map_err(anyhow::Error::msg)?;
        let Some(release) = data.mise else {
            self.refuse(BOOTSTRAP.into(), Reason::MissingTool);
            return Ok(());
        };
        let Some(version) = release
            .version
            .filter(|version| rules::artifact_rules::pinned(version.as_bytes()))
        else {
            self.refuse(BOOTSTRAP.into(), Reason::UnpinnedVersion);
            return Ok(());
        };
        for target in self.request.targets.clone() {
            let subject = format!("{BOOTSTRAP}@{version}:{}", target.name());
            let checksum = release
                .sha256
                .as_ref()
                .and_then(|checksums| checksums.get(target.name()))
                .map(|hex| format!("sha256:{hex}"));
            let suffix = if target == Target::WindowsX64 {
                ".zip"
            } else {
                ""
            };
            let asset = LockedArtifact {
                url: Some(format!(
                    "{MISE_RELEASES}/v{version}/mise-v{version}-{}{suffix}",
                    target.name()
                )),
                checksum,
                url_api: None,
                provenance: None,
                conda_deps: Vec::new(),
            };
            self.asset(
                subject,
                Some(target),
                &version,
                &asset,
                RELEASE_PINS,
                version.as_bytes(),
            )?;
        }
        Ok(())
    }

    fn cargo(&mut self, text: &str) -> Result<()> {
        self.cargo_named(CARGO_LOCK, text)
    }

    fn cargo_named(&mut self, input: &str, text: &str) -> Result<()> {
        let lock: CargoLock =
            toml::from_str(text).context("parse workspace Cargo acquisition lock")?;
        ensure!(
            lock.version == CARGO_LOCK_VERSION,
            "unsupported Cargo lockfile version"
        );
        let mut subjects = BTreeSet::new();
        for package in lock.package {
            let Some(source) = &package.source else {
                continue;
            };
            let subject = format!("crate:{}@{}", package.name, package.version);
            ensure!(
                subjects.insert(subject.clone()),
                "duplicate Cargo acquisition identity"
            );
            if source != CRATES_IO {
                self.refuse(subject, Reason::UnsupportedBackend);
                continue;
            }
            if package.name.is_empty()
                || package.version.is_empty()
                || !package
                    .name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
                || !package
                    .version
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'-'))
            {
                self.refuse(subject, Reason::UnpinnedVersion);
                continue;
            }
            let asset = LockedArtifact {
                url: Some(format!(
                    "{CRATE_RELEASES}/{}/{}-{}.crate",
                    package.name, package.name, package.version
                )),
                checksum: package.checksum.map(|hex| format!("sha256:{hex}")),
                url_api: None,
                provenance: None,
                conda_deps: Vec::new(),
            };
            self.asset(
                subject,
                None,
                &package.version,
                &asset,
                input,
                &serde_json::to_vec(&(source, &package.dependencies))?,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROJECT: &str = "[tools]\nexample = \"1.2.3\"\n";
    const EMPTY_CARGO: &str = "version = 4\n";
    const EMPTY_VERSIONS: &str = "";

    fn fixture(checksum: &str, extra: &str) -> String {
        format!(
            "lockfile_version = 3\n[[tools.example]]\nversion = \"1.2.3\"\nbackend = \"aqua:owner/example\"\nspecifiers = [\"1.2.3\"]\n[tools.example.\"platforms.windows-x64\"]\nurl = \"https://example.com/example-1.2.3.zip\"\nchecksum = {checksum:?}\n{extra}"
        )
    }

    fn parse(lock: &str) -> Result<Plan> {
        Plan::parse(
            Inputs {
                project: PROJECT,
                mise_lock: lock,
                cargo_lock: EMPTY_CARGO,
                versions: EMPTY_VERSIONS,
            },
            Request {
                targets: vec![Target::WindowsX64],
                tools: vec!["example".into()],
            },
        )
    }

    #[test]
    fn pinned_artifacts_keep_platform_source_and_expected_digest() -> Result<()> {
        let expected = Digest::of(b"trusted archive");
        let plan = parse(&fixture(&format!("sha256:{}", expected.hex()), ""))?;
        assert!(plan.diagnostics.is_empty(), "{:?}", plan.diagnostics);
        let artifact = plan
            .artifacts
            .first()
            .expect("the selected artifact is present");
        assert_eq!(artifact.target, Some(Target::WindowsX64));
        assert_eq!(artifact.version, "1.2.3");
        assert_eq!(artifact.source, "https://example.com/example-1.2.3.zip");
        assert_eq!(artifact.expected, expected);
        Ok(())
    }

    fn filtered_plan(request: Request) -> Result<Plan> {
        let checksum = format!("sha256:{}", Digest::of(b"archive").hex());
        let versions = format!(
            "[mise]\nversion = \"2026.10.0\"\n[mise.sha256]\nwindows-x64 = {:?}\n",
            Digest::of(b"bootstrap").hex()
        );
        Plan::parse(
            Inputs {
                project: "[tools]\nexample = \"1.2.3\"\nlinux_only = {version = \"1.2.3\", os = [\"linux\"]}\n",
                mise_lock: &fixture(&checksum, ""),
                cargo_lock: EMPTY_CARGO,
                versions: &versions,
            },
            request,
        )
    }

    #[test]
    fn an_ordinary_target_plan_excludes_tools_for_other_operating_systems() -> Result<()> {
        let plan = filtered_plan(Request {
            targets: vec![Target::WindowsX64],
            tools: Vec::new(),
        })?;
        plan.complete()?;
        assert_eq!(plan.artifacts.len(), 2);
        assert!(plan.artifacts.iter().all(|artifact| {
            matches!(
                artifact.subject.as_str(),
                "example@1.2.3:windows-x64" | "mise-bootstrap@2026.10.0:windows-x64"
            )
        }));
        Ok(())
    }

    #[test]
    fn explicitly_selected_tools_for_another_target_refuse_before_lock_lookup() -> Result<()> {
        let plan = filtered_plan(Request {
            targets: vec![Target::WindowsX64],
            tools: vec!["linux_only".into()],
        })?;
        assert_eq!(
            plan.diagnostics,
            [Diagnostic {
                subject: "linux_only".into(),
                reason: Reason::MissingPlatform,
            }]
        );
        assert!(plan.complete().is_err());
        Ok(())
    }

    #[test]
    fn a_required_target_tool_still_needs_its_locked_material() -> Result<()> {
        let plan = filtered_plan(Request {
            targets: vec![Target::LinuxX64],
            tools: vec!["linux_only".into()],
        })?;
        assert_eq!(
            plan.diagnostics,
            [Diagnostic {
                subject: "linux_only".into(),
                reason: Reason::MissingTool,
            }]
        );
        assert!(plan.complete().is_err());
        Ok(())
    }

    #[test]
    fn unsupported_checksum_and_provenance_supply_no_artifact() -> Result<()> {
        for lock in [
            fixture("blake3:unknown", ""),
            fixture(
                &format!("sha256:{}", Digest::of(b"trusted").hex()),
                "provenance = \"cosign\"\n",
            ),
        ] {
            let plan = parse(&lock)?;
            assert!(plan.artifacts.is_empty());
            assert_ne!(plan.diagnostics, Vec::new());
            assert!(
                plan.diagnostics
                    .iter()
                    .all(|diagnostic| diagnostic.subject.contains("example"))
            );
        }
        Ok(())
    }

    #[test]
    fn windows_bootstrap_uses_the_zip_that_its_release_pin_hashes() -> Result<()> {
        let digest = Digest::of(b"mise release zip").hex();
        let versions =
            format!("[mise]\nversion = \"2026.10.0\"\n[mise.sha256]\nwindows-x64 = {digest:?}\n");
        let plan = Plan::parse(
            Inputs {
                project: "[tools]\n",
                mise_lock: "lockfile_version = 3\n[tools]\n",
                cargo_lock: EMPTY_CARGO,
                versions: &versions,
            },
            Request {
                targets: vec![Target::WindowsX64],
                tools: vec![BOOTSTRAP.into()],
            },
        )?;
        plan.complete()?;
        assert_eq!(
            plan.artifacts.first().expect("bootstrap is present").source,
            "https://github.com/jdx/mise/releases/download/v2026.10.0/mise-v2026.10.0-windows-x64.zip"
        );
        Ok(())
    }

    #[test]
    fn changed_install_options_and_explicit_backends_refuse_the_locked_identity() -> Result<()> {
        let checksum = format!("sha256:{}", Digest::of(b"archive").hex());
        let lock = fixture(&checksum, "").replace(
            "[tools.example.\"platforms.windows-x64\"]",
            "[tools.example.options]\nlibc = \"gnu\"\n[tools.example.\"platforms.windows-x64\"]",
        );
        let options = Plan::parse(
            Inputs {
                project: "[tools]\nexample = {version = \"1.2.3\", libc = \"musl\"}\n",
                mise_lock: &lock,
                cargo_lock: EMPTY_CARGO,
                versions: EMPTY_VERSIONS,
            },
            Request {
                targets: vec![Target::WindowsX64],
                tools: vec!["example".into()],
            },
        )?;
        assert!(
            options.complete().is_err(),
            "options mismatch cannot identify the requested installation"
        );
        let lock = fixture(&checksum, "").replace("tools.example", "tools.\"cargo:example\"");
        let backend = Plan::parse(
            Inputs {
                project: "[tools]\n\"cargo:example\" = \"1.2.3\"\n",
                mise_lock: &lock,
                cargo_lock: EMPTY_CARGO,
                versions: EMPTY_VERSIONS,
            },
            Request {
                targets: vec![Target::WindowsX64],
                tools: vec!["cargo:example".into()],
            },
        )?;
        assert!(
            backend.complete().is_err(),
            "explicit Cargo pin cannot use an Aqua backend record"
        );
        Ok(())
    }

    #[test]
    fn nested_conda_materials_cannot_silently_disappear_from_a_complete_plan() -> Result<()> {
        let checksum = format!("sha256:{}", Digest::of(b"archive").hex());
        let lock = fixture(
            &checksum,
            &format!(
                "conda_deps = [\"dependency\"]\n[conda-packages.windows-x64.dependency]\nurl = \"https://example.com/dependency.conda\"\nchecksum = {checksum:?}\nconda_deps = [\"nested\"]\n"
            ),
        );
        let plan = parse(&lock)?;
        assert!(
            plan.complete().is_err(),
            "declared nested material lacks acquisition identity"
        );
        Ok(())
    }

    #[test]
    fn every_selected_target_requires_its_material_and_unknown_tools_fail_closed() -> Result<()> {
        let lock = fixture(&format!("sha256:{}", Digest::of(b"archive").hex()), "");
        let plan = Plan::parse(
            Inputs {
                project: PROJECT,
                mise_lock: &lock,
                cargo_lock: EMPTY_CARGO,
                versions: EMPTY_VERSIONS,
            },
            Request {
                targets: vec![Target::LinuxArm64, Target::WindowsX64],
                tools: vec!["example".into(), "unknown".into()],
            },
        )?;
        assert!(
            plan.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.reason == Reason::MissingPlatform)
        );
        assert!(
            plan.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.reason == Reason::MissingTool)
        );
        assert!(plan.complete().is_err());
        Ok(())
    }
}

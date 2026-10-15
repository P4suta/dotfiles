use super::{CargoWorkload, Diagnostic, Digest, Inputs, Plan, Reason, Request, Target};
use crate::profile_rules::Profile;
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SCHEMA: u32 = 1;
const SCOPE: &str = "rendered-native-mise-and-source-cargo-acquisition";
const PROJECT_CONFIG: &str = "mise.toml";
const PROJECT_ENVIRONMENT_PREFIX: &str = "mise.";
const CONFIGURATION_EXTENSION: &str = "toml";
const PROJECT_LOCK: &str = "mise.lock";
const CARGO_LOCK: &str = "Cargo.lock";
const RELEASE_PINS: &str = "home/.chezmoidata/versions.toml";
const NATIVE_TEMPLATE: &str = "home/dot_config/mise/config.toml.tmpl";
const INPUT_BYTE_LIMIT: u64 = 8 * 1024 * 1024;
const SOURCE_BYTE_LIMIT: u64 = 64 * 1024 * 1024;
const SOURCE_FILE_LIMIT: usize = 16_384;
const SOURCE_DEPTH_LIMIT: usize = 64;
const SOURCE_DIRECTORY_LIMIT: usize = 65_536;
const RENDER_LIMIT: Duration = Duration::from_secs(60);
const RENDER_POLL: Duration = Duration::from_millis(20);
const PRE_RENDER_PROVENANCE_FIELDS: usize = 3;
const POST_RENDER_PROVENANCE_FIELDS: usize = PRE_RENDER_PROVENANCE_FIELDS + 1;

/// Inputs for an actual Source rendering, with an independently captured native lock.
#[derive(Clone, Copy)]
pub struct NativeInputs<'a> {
    pub configuration: &'a Path,
    pub lock: &'a Path,
    pub renderer: &'a Path,
    pub destination: &'a Path,
}

impl std::fmt::Debug for NativeInputs<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeInputs").finish_non_exhaustive()
    }
}

/// Captured project and actual rendered native tool requirements, without installation credit.
pub struct NativeWorkload {
    root: PathBuf,
    target: Target,
    profile: Profile,
    source: BTreeMap<String, Digest>,
    project: BTreeMap<String, String>,
    global: String,
    global_lock: String,
    cargo: CargoWorkload,
}

impl std::fmt::Debug for NativeWorkload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeWorkload")
            .field("target", &self.target)
            .field("profile", &self.profile)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Consumer {
    NativeToolInstallation,
    ToolDependencyMaterials,
    VerificationEnvironmentMaterials,
    IndependentLiveHealth,
}

/// Named acquisition members don't complete a native stage.
#[derive(Debug, Serialize)]
pub struct NativePlan {
    schema: u32,
    scope: &'static str,
    profile: &'static str,
    target: Target,
    inputs: BTreeMap<String, Digest>,
    project: Plan,
    global: Plan,
    cargo: Plan,
    diagnostics: Vec<Diagnostic>,
    pending_consumers: [Consumer; 4],
    digest: Digest,
}

impl NativePlan {
    #[must_use]
    pub fn members(&self) -> [&Plan; 3] {
        [&self.project, &self.global, &self.cargo]
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    #[must_use]
    pub const fn digest(&self) -> Digest {
        self.digest
    }

    /// Require every named configuration acquisition member, without native readiness credit.
    ///
    /// # Errors
    ///
    /// Refuses unresolved providers or any incomplete project, global, or Cargo member.
    pub fn complete_acquisition(&self) -> Result<()> {
        let mut failures = Vec::new();
        for (name, member) in [
            ("project material", &self.project),
            ("native global material", &self.global),
            ("source Cargo material", &self.cargo),
        ] {
            if let Err(error) = member.complete() {
                failures.push(format!("{name}: {error:#}"));
            }
        }
        ensure!(
            failures.is_empty() && self.diagnostics.is_empty(),
            "native acquisition is unresolved: {}",
            failures.join("; ")
        );
        Ok(())
    }
}

/// Untrusted configuration parsing, separate from an actual Source rendering.
#[derive(Debug, Serialize)]
pub struct NativeConfiguration {
    materials: Plan,
    diagnostics: Vec<Diagnostic>,
}

impl NativeConfiguration {
    /// Use the acquisition parser to decode materials and keep unresolved providers.
    ///
    /// # Errors
    ///
    /// Rejects oversize, malformed, or empty configuration and lock inputs.
    pub fn parse(inputs: Inputs<'_>, target: Target) -> Result<Self> {
        for input in [
            inputs.project,
            inputs.mise_lock,
            inputs.cargo_lock,
            inputs.versions,
        ] {
            ensure!(
                input.len() as u64 <= INPUT_BYTE_LIMIT,
                "native configuration exceeds its byte bound"
            );
        }
        let materials = Plan::parse(
            inputs,
            Request {
                targets: vec![target],
                tools: Vec::new(),
            },
        )?;
        let configuration: toml::Table = toml::from_str(inputs.project)?;
        let tools = configuration
            .get("tools")
            .and_then(toml::Value::as_table)
            .context("native acquisition configuration requires tools")?;
        ensure!(
            !tools.is_empty(),
            "native acquisition configuration has no tools"
        );
        let diagnostics = providers(target, &configuration, inputs.mise_lock)?;
        Ok(Self {
            materials,
            diagnostics,
        })
    }

    /// Validate native answers without granting rendering or installation provenance.
    ///
    /// # Errors
    ///
    /// Refuses malformed answers or a profile that differs from the requested target.
    pub fn profile(configuration: &str, target: Target) -> Result<Profile> {
        ensure!(
            configuration.len() as u64 <= INPUT_BYTE_LIMIT,
            "native answers exceed their byte bound"
        );
        let table: toml::Table = toml::from_str(configuration)?;
        let profile = crate::profiles::configured_profile(&serde_json::to_value(
            table.get("data").context("native answers require data")?,
        )?)?;
        ensure!(
            matches!(
                (profile, target),
                (Profile::Mac, Target::MacosArm64 | Target::MacosX64)
                    | (
                        Profile::Linux | Profile::Wsl,
                        Target::LinuxArm64 | Target::LinuxX64
                    )
                    | (Profile::Windows, Target::WindowsX64)
            ),
            "native profile differs from the requested target"
        );
        Ok(profile)
    }

    /// Require complete acquisition metadata, without authenticating caller-supplied bytes.
    ///
    /// # Errors
    ///
    /// Refuses missing materials and unresolved provider implementations.
    pub fn complete(&self) -> Result<()> {
        self.materials.complete()?;
        ensure!(
            self.diagnostics.is_empty(),
            "native provider materials remain unresolved"
        );
        Ok(())
    }
}

impl NativeWorkload {
    /// Render the captured Source through the supplied pinned, owner-resolved renderer.
    ///
    /// # Errors
    ///
    /// Refuses changed Source, unsafe paths, mismatched profiles or renderers, and failed rendering.
    pub fn capture(root: &Path, target: Target, native: NativeInputs<'_>) -> Result<Self> {
        let root = real_directory(root)?;
        let destination = real_directory(native.destination)?;
        ensure!(
            !destination.starts_with(&root),
            "native destination must be outside Source"
        );
        for path in [native.configuration, native.lock, native.renderer] {
            ensure!(
                path.is_absolute(),
                "native acquisition paths require an absolute path"
            );
            file_digest(path)?;
        }
        ensure!(
            !platform::paths::canonical(native.configuration)?.starts_with(&root)
                && !platform::paths::canonical(native.lock)?.starts_with(&root),
            "native configuration and lock must remain outside Source"
        );
        let mut project = BTreeMap::new();
        for name in [PROJECT_CONFIG, PROJECT_LOCK, CARGO_LOCK, RELEASE_PINS] {
            project.insert(name.into(), text(&root.join(name))?);
        }
        let configuration = text(native.configuration)?;
        let profile = NativeConfiguration::profile(&configuration, target)?;
        let mut source = source_inputs(&root)?;
        ensure!(
            project
                .iter()
                .all(|(name, text)| source.get(name) == Some(&Digest::of(text.as_bytes()))),
            "project acquisition inputs changed during Source capture"
        );
        source.insert(
            "native/answers".into(),
            Digest::of(configuration.as_bytes()),
        );
        source.insert("native/renderer".into(), file_digest(native.renderer)?);
        let global_lock = text(native.lock)?;
        source.insert("native/lock".into(), Digest::of(global_lock.as_bytes()));
        require_renderer(&project[PROJECT_CONFIG], native.renderer)?;
        let global = render(&root, &destination, &native)?;
        let current = source_inputs(&root)?;
        ensure!(
            current
                .iter()
                .all(|(name, digest)| source.get(name) == Some(digest))
                && current.len() + PRE_RENDER_PROVENANCE_FIELDS == source.len()
                && text(native.configuration)? == configuration
                && text(native.lock)? == global_lock
                && file_digest(native.renderer)? == source["native/renderer"],
            "native acquisition inputs changed during rendering"
        );
        source.insert("native/rendered-mise".into(), Digest::of(global.as_bytes()));
        Ok(Self {
            cargo: CargoWorkload::capture(&root)?,
            root,
            target,
            profile,
            source,
            project,
            global,
            global_lock,
        })
    }

    /// Plan all configured tool members from the actual rendered file and its exact lock.
    ///
    /// # Errors
    ///
    /// Refuses changed Source or malformed configuration and material locks.
    pub fn plan(&self) -> Result<NativePlan> {
        self.cargo.require_current(&self.root)?;
        let source = source_inputs(&self.root)?;
        ensure!(
            source
                .iter()
                .all(|(name, digest)| self.source.get(name) == Some(digest))
                && source.len() + POST_RENDER_PROVENANCE_FIELDS == self.source.len(),
            "native acquisition Source changed after rendering"
        );
        let project = self.mise_plan(&self.project[PROJECT_CONFIG], &self.project[PROJECT_LOCK])?;
        let global = self.mise_plan(&self.global, &self.global_lock)?;
        let cargo = self.cargo.plan()?;
        let mut diagnostics = Vec::new();
        for name in source.keys().filter(|name| optional_configuration(name)) {
            diagnostics.push(Diagnostic {
                subject: format!("project-environment:{name}"),
                reason: Reason::MissingDependency,
            });
        }
        for (scope, member) in [("project", &project), ("global", &global)] {
            diagnostics.extend(member.diagnostics.iter().map(|diagnostic| Diagnostic {
                subject: format!("{scope}:{}", diagnostic.subject),
                reason: diagnostic.reason.clone(),
            }));
        }
        let project = project.materials;
        let global = global.materials;
        let digest = Digest::of(&serde_json::to_vec(&(
            SCHEMA,
            SCOPE,
            self.profile.name(),
            self.target,
            &self.source,
            project.digest(),
            global.digest(),
            cargo.digest(),
            &diagnostics,
        ))?);
        Ok(NativePlan {
            schema: SCHEMA,
            scope: SCOPE,
            profile: self.profile.name(),
            target: self.target,
            inputs: self.source.clone(),
            project,
            global,
            cargo,
            diagnostics,
            pending_consumers: [
                Consumer::NativeToolInstallation,
                Consumer::ToolDependencyMaterials,
                Consumer::VerificationEnvironmentMaterials,
                Consumer::IndependentLiveHealth,
            ],
            digest,
        })
    }

    fn mise_plan(&self, configuration: &str, lock: &str) -> Result<NativeConfiguration> {
        NativeConfiguration::parse(
            Inputs {
                project: configuration,
                mise_lock: lock,
                cargo_lock: &self.project[CARGO_LOCK],
                versions: &self.project[RELEASE_PINS],
            },
            self.target,
        )
    }
}

fn providers(target: Target, configuration: &toml::Table, lock: &str) -> Result<Vec<Diagnostic>> {
    let lock: toml::Table = toml::from_str(lock)?;
    let mut diagnostics = Vec::new();
    if let Some(aliases) = configuration.get("tool_alias") {
        let aliases = aliases
            .as_table()
            .context("native tool aliases require a table")?;
        for (name, provider) in aliases {
            provider
                .as_str()
                .context("native tool provider requires a string")?;
            diagnostics.push(Diagnostic {
                subject: format!("{name}:provider-revision"),
                reason: Reason::MissingDependency,
            });
        }
    }
    if let Some(tools) = lock.get("tools").and_then(toml::Value::as_table) {
        for (name, versions) in tools {
            let Some(pin) = configuration.get("tools").and_then(|tools| tools.get(name)) else {
                continue;
            };
            if let Some(os) = pin.get("os") {
                let os: Vec<String> = os
                    .clone()
                    .try_into()
                    .context("native OS filters require strings")?;
                let expected = match target {
                    Target::LinuxArm64 | Target::LinuxX64 => "linux",
                    Target::MacosArm64 | Target::MacosX64 => "macos",
                    Target::WindowsX64 => "windows",
                };
                if !os.iter().any(|os| os == expected) {
                    continue;
                }
            }
            let versions = versions
                .as_array()
                .context("native locked tools require arrays")?;
            for version in versions {
                let backend = version
                    .get("backend")
                    .and_then(toml::Value::as_str)
                    .context("native locked tool requires its provider")?;
                if backend == "core:rust" || backend.starts_with("cargo:") {
                    diagnostics.push(Diagnostic {
                        subject: format!("{name}:native-installation-materials"),
                        reason: Reason::MissingDependency,
                    });
                }
                if !matches!(
                    backend.split_once(':').map(|(kind, _)| kind),
                    Some("core" | "aqua" | "github" | "http" | "cargo" | "pipx" | "pypi" | "conda")
                ) {
                    diagnostics.push(Diagnostic {
                        subject: format!("{name}:provider"),
                        reason: Reason::UnsupportedBackend,
                    });
                }
            }
        }
    }
    Ok(diagnostics)
}

fn require_renderer(configuration: &str, renderer: &Path) -> Result<()> {
    let configuration: toml::Table = toml::from_str(configuration)?;
    let pin = configuration
        .get("tools")
        .and_then(|tools| tools.get("chezmoi"))
        .and_then(|pin| {
            pin.as_str()
                .or_else(|| pin.get("version").and_then(toml::Value::as_str))
        })
        .context("native acquisition needs the project ChezMoi pin")?;
    ensure!(
        rules::artifact_rules::pinned(pin.as_bytes()),
        "native renderer requires an exact project pin"
    );
    let scope = tempfile::tempdir()?;
    let result = output(
        platform::tool::external(renderer).arg("--version"),
        scope.path(),
        "renderer",
    );
    platform::cleanup::report(scope.close(), "remove the native renderer probe");
    let version = result?;
    let mut words = version.split_ascii_whitespace();
    ensure!(
        words.next() == Some("chezmoi")
            && words.next() == Some("version")
            && words
                .next()
                .is_some_and(
                    |version| version.trim_end_matches(',').trim_start_matches('v') == pin
                ),
        "native renderer differs from the exact project pin"
    );
    Ok(())
}

pub(super) fn renderer(root: &Path) -> Result<PathBuf> {
    let scope = tempfile::tempdir()?;
    let result = output(
        platform::tool::Tool::Mise
            .command()
            .current_dir(root)
            .args(["which", "chezmoi"])
            .env("MISE_AUTO_INSTALL", "false")
            .env("MISE_AUTO_PRUNE", "false"),
        scope.path(),
        "renderer-path",
    );
    platform::cleanup::report(scope.close(), "remove the native renderer resolution");
    let path = PathBuf::from(result?.trim());
    ensure!(
        path.is_absolute(),
        "native renderer resolution requires an absolute path"
    );
    file_digest(&path)?;
    Ok(platform::paths::canonical(&path)?)
}

fn render(root: &Path, destination: &Path, native: &NativeInputs<'_>) -> Result<String> {
    let scope = tempfile::tempdir()?;
    let mut command = platform::tool::external(native.renderer);
    command
        .arg("--source")
        .arg(root)
        .arg("--config")
        .arg(native.configuration)
        .arg("--destination")
        .arg(destination)
        .arg("--persistent-state")
        .arg(scope.path().join("state.boltdb"))
        .arg("--cache")
        .arg(scope.path().join("cache"))
        .args([
            "--refresh-externals=never",
            "--no-tty",
            "execute-template",
            "--file",
        ])
        .arg(root.join(NATIVE_TEMPLATE));
    command
        .env("HOME", destination)
        .env("USERPROFILE", destination)
        .env("MISE_AUTO_INSTALL", "false")
        .env("MISE_AUTO_PRUNE", "false");
    let result = output(&mut command, scope.path(), "configuration");
    platform::cleanup::report(scope.close(), "remove the native configuration rendering");
    result
}

fn real_directory(path: &Path) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "native acquisition needs a real directory"
    );
    Ok(platform::paths::canonical(path)?)
}

fn text(path: &Path) -> Result<String> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= INPUT_BYTE_LIMIT,
        "native acquisition input is not a bounded regular file"
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take(INPUT_BYTE_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= INPUT_BYTE_LIMIT,
        "native acquisition input grew beyond its bound"
    );
    String::from_utf8(bytes).context("native acquisition input requires UTF-8")
}

fn file_digest(path: &Path) -> Result<Digest> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file(),
        "native acquisition input must be a regular file"
    );
    super::Cache::content(&mut File::open(path)?)
}

fn source_inputs(root: &Path) -> Result<BTreeMap<String, Digest>> {
    let mut inputs = BTreeMap::new();
    let mut bytes = 0;
    let mut directories = 0;
    source_directory(
        root,
        &root.join("home"),
        0,
        &mut bytes,
        &mut directories,
        &mut inputs,
    )?;
    for name in [PROJECT_CONFIG, PROJECT_LOCK, CARGO_LOCK, ".chezmoiroot"] {
        inputs.insert(name.into(), file_digest(&root.join(name))?);
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("native Source path requires Unicode"))?;
        if optional_configuration(&name) {
            ensure!(
                inputs.len() < SOURCE_FILE_LIMIT,
                "native Source exceeds its file bound"
            );
            inputs.insert(name, Digest::of(text(&entry.path())?.as_bytes()));
        }
    }
    Ok(inputs)
}

fn optional_configuration(name: &str) -> bool {
    name.starts_with(PROJECT_ENVIRONMENT_PREFIX)
        && !name.eq_ignore_ascii_case(PROJECT_CONFIG)
        && Path::new(name)
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|extension| extension.eq_ignore_ascii_case(CONFIGURATION_EXTENSION))
}

fn source_directory(
    root: &Path,
    directory: &Path,
    depth: usize,
    bytes: &mut u64,
    directories: &mut usize,
    inputs: &mut BTreeMap<String, Digest>,
) -> Result<()> {
    ensure!(
        depth <= SOURCE_DEPTH_LIMIT,
        "native Source exceeds its path-depth bound"
    );
    *directories += 1;
    ensure!(
        *directories <= SOURCE_DIRECTORY_LIMIT,
        "native Source exceeds its directory bound"
    );
    real_directory(directory)?;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "native Source cannot contain symlinks"
        );
        if metadata.is_dir() {
            source_directory(root, &entry.path(), depth + 1, bytes, directories, inputs)?;
        } else {
            ensure!(
                metadata.is_file() && inputs.len() < SOURCE_FILE_LIMIT,
                "native Source exceeds its file bound"
            );
            *bytes = bytes
                .checked_add(metadata.len())
                .context("native Source size overflows")?;
            ensure!(
                *bytes <= SOURCE_BYTE_LIMIT,
                "native Source exceeds its byte bound"
            );
            let path = entry.path();
            let relative = path
                .strip_prefix(root)?
                .to_str()
                .context("native Source path requires Unicode")?
                .replace('\\', "/");
            inputs.insert(relative, file_digest(&path)?);
        }
    }
    Ok(())
}

fn output(command: &mut Command, scope: &Path, name: &str) -> Result<String> {
    let stdout = scope.join(format!("{name}.stdout"));
    let stderr = scope.join(format!("{name}.stderr"));
    command
        .stdin(Stdio::null())
        .stdout(File::create(&stdout)?)
        .stderr(File::create(&stderr)?);
    let mut child = command.spawn()?;
    let start = Instant::now();
    let waited: Result<_> = (|| loop {
        if fs::metadata(&stdout)?.len() > INPUT_BYTE_LIMIT
            || fs::metadata(&stderr)?.len() > INPUT_BYTE_LIMIT
            || start.elapsed() > RENDER_LIMIT
        {
            anyhow::bail!("native renderer exceeded its output or time bound");
        }
        if let Some(status) = child.try_wait()? {
            break Ok(status);
        }
        std::thread::sleep(RENDER_POLL);
    })();
    if waited.is_err() {
        platform::cleanup::report(child.kill(), "stop the bounded native renderer");
        platform::cleanup::report(child.wait(), "reap the bounded native renderer");
    }
    let status = waited?;
    ensure!(
        status.success(),
        "native renderer refused {name}: {status}: {}",
        text(&stderr)?
    );
    text(&stdout)
}

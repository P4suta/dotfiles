//! Code gates build their environment from resolved project installations.

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::paths::canonical;

const UNIX_BASE: [&str; 4] = ["/usr/bin", "/bin", "/usr/sbin", "/sbin"];
const WINDOWS_GIT: &str = "Git";
const MISE_CONFIG: &str = "mise.toml";
const MISE_LOCK: &str = "mise.lock";
const PROJECT_INPUTS: [&str; 2] = [MISE_CONFIG, MISE_LOCK];
const LOCK_SIDECARS: &str = ".mise/locks";
const PROJECT_INPUT_LIMIT: u64 = 4 * 1024 * 1024;
const PROJECT_INPUT_COUNT: usize = 128;
const PROJECT_INPUT_DEPTH: usize = 8;
const PRIVATE_PROJECT: &str = "project";
const LINK_READBACK: &str = "linked-tools.json";
const NATIVE_READBACK: &str = "inherited-native-tools.json";
const STAGED_READBACK: &str = "inherited-staged-tools.json";
const PRIVATE_DATA: &str = ".local/share/mise";
const DOCKER_USER_DIRECTORY: &str = ".docker";
const DOCKER_PLUGIN_DIRECTORY: &str = "cli-plugins";
const BUILDX_STATE_DIRECTORY: &str = "buildx";
const BUILDX_TOOL: &str = "aqua:docker/buildx";
const BUILDX_PROGRAM: &str = "docker-cli-plugin-docker-buildx";
const BUILDX_PLUGIN: &str = "docker-buildx";
#[cfg(unix)]
const POWERSHELL_USER_DIRECTORIES: [&str; 2] = [".local/share/powershell", ".config/powershell"];
const ABSENT_SYSTEM_INSTALLS: &str = "absent-system-installs";
const TOOL_INVENTORY_LIMIT: usize = 128;
const TOOL_DIAGNOSTIC_LIMIT: usize = 4096;
const CARGO_BIN: &str = "bin";
const RUSTUP_COMPONENT_PROXIES: [&str; 2] = ["cargo-miri", "rustdoc"];
const KANI_HOME: &str = ".kani";
const KANI_TOOL: &str = "cargo:kani-verifier";
const RELEASE_PINS: &str = "home/.chezmoidata/versions.toml";
const GITHUB_HOSTS: &str = "github.com: {}\n";
const BACKEND_METADATA: &str = ".mise.backend.toml";
const MISE_DIFF: &str = "__MISE_DIFF";
const NATIVE_QUERY_CONFIG: &str = "empty.toml";
pub const NATIVE_PATH_DIFF: &str = "DOTFILES_HOST_MISE_DIFF";
pub const NATIVE_PATH_DIFF_PRESENT: &str = "DOTFILES_HOST_MISE_DIFF_PRESENT";
pub const HOST_CACHE: &str = ".cache/dotfiles/hosts";
pub const MIRI_TARGET_VARIABLE: &str = "DOTFILES_HOST_MIRI_TARGET";
const MIRI_TARGET_DIRECTORY: &str = ".cache/miri-target";
#[cfg(unix)]
const BOOTSTRAP_COPY: &str = "/bin/cp";
#[cfg(windows)]
const REGISTERED_COMPILER: &str = "DOTFILES_HOST_COMPILER";
#[cfg(windows)]
const MSVC_COMPILER: &str = "cl.exe";

/// The native PATH recovered before candidate configuration replaces its origin.
#[derive(Debug)]
pub struct NativePath {
    path: OsString,
    mise: Option<PathBuf>,
}

/// A checked environment response retaining only its native PATH.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct NativePathResponse {
    #[serde(rename = "PATH")]
    path: String,
}

impl NativePathResponse {
    /// Parse a response without retaining unrelated environment values.
    ///
    /// # Errors
    ///
    /// Reports only the JSON error category for malformed or ambiguous responses.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(|error| {
            anyhow::anyhow!("native PATH response is invalid ({:?})", error.classify())
        })
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.path
    }

    fn read(output: &std::process::Output) -> Result<Self> {
        ensure!(
            output.status.success(),
            "native PATH query failed with {}",
            output.status
        );
        ensure!(
            output.stderr.is_empty(),
            "native PATH query produced diagnostics"
        );
        Self::parse(&output.stdout)
    }
}

impl<'de> Deserialize<'de> for NativePathResponse {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct Response;

        impl<'de> serde::de::Visitor<'de> for Response {
            type Value = NativePathResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an environment with one native PATH")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut values: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                use serde::de::Error as _;
                let mut path = None;
                while let Some(name) = values.next_key::<String>()? {
                    if name == "PATH" || (cfg!(windows) && name.eq_ignore_ascii_case("PATH")) {
                        if path.is_some() {
                            return Err(A::Error::duplicate_field("PATH"));
                        }
                        path = Some(values.next_value::<String>()?);
                    } else {
                        values.next_value::<serde::de::IgnoredAny>()?;
                    }
                }
                path.map(|path| NativePathResponse { path })
                    .ok_or_else(|| A::Error::missing_field("PATH"))
            }
        }
        deserializer.deserialize_map(Response)
    }
}

impl NativePath {
    /// Recover parent activation through mise while keeping every query write in owned storage.
    ///
    /// # Errors
    ///
    /// Refuses incomplete provenance, unavailable mise, and any unsuccessful or malformed query.
    pub fn capture(home: &Path) -> Result<Self> {
        let explicit = std::env::var_os("DOTFILES_HOST_PATH");
        let inherited = std::env::var_os("PATH").unwrap_or_default();
        let path = explicit.clone().unwrap_or_else(|| inherited.clone());
        let diff = match (
            std::env::var_os(NATIVE_PATH_DIFF_PRESENT),
            std::env::var_os(NATIVE_PATH_DIFF),
        ) {
            (Some(present), diff) => {
                ensure!(
                    explicit.is_some(),
                    "native PATH metadata has no captured PATH"
                );
                if present == "1" {
                    Some(diff.context("native PATH metadata is missing")?)
                } else {
                    ensure!(
                        present == "0" && diff.as_ref().is_none_or(|value| value.is_empty()),
                        "native PATH metadata presence is invalid"
                    );
                    None
                }
            }
            (None, None) => {
                let diff = std::env::var_os(MISE_DIFF);
                ensure!(
                    diff.is_none() || explicit.is_none() || path == inherited,
                    "native PATH and activation metadata have different origins"
                );
                diff
            }
            _ => anyhow::bail!("native PATH metadata is incomplete"),
        };
        let Some(diff) = diff else {
            return Ok(Self { path, mise: None });
        };
        let mise = mise_executable(home, &path)?;
        let capability = match crate::github::executable(&path) {
            Ok(executable) => Some(executable),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error).context("resolve the native GitHub CLI"),
        };
        let scope = tempfile::tempdir()?;
        let result = Self::query(scope.path(), &mise, &path, &diff, capability.as_deref());
        crate::cleanup::report(scope.close(), "remove the native PATH query");
        result
    }

    fn query(
        scope: &Path,
        mise: &Path,
        path: &OsStr,
        diff: &OsStr,
        capability: Option<&Path>,
    ) -> Result<Self> {
        let scope = canonical(scope)?;
        let empty = scope.join(NATIVE_QUERY_CONFIG);
        fs::write(&empty, "")?;
        let mut command = crate::process::command(mise);
        for (name, _) in std::env::vars_os() {
            let text = name.to_string_lossy().to_ascii_uppercase();
            if text.starts_with("MISE_") || text.starts_with("__MISE_") {
                command.env_remove(name);
            }
        }
        for (name, directory) in [
            ("MISE_CONFIG_FILE", empty.clone()),
            ("MISE_GLOBAL_CONFIG_FILE", empty.clone()),
            ("MISE_SYSTEM_CONFIG_FILE", empty),
            ("MISE_CONFIG_DIR", scope.join("config")),
            ("MISE_SYSTEM_CONFIG_DIR", scope.join("system-config")),
            ("MISE_DATA_DIR", scope.join("data")),
            ("MISE_SYSTEM_DATA_DIR", scope.join("system-data")),
            ("MISE_STATE_DIR", scope.join("state")),
            ("MISE_CACHE_DIR", scope.join("cache")),
            ("MISE_CEILING_PATHS", scope.clone()),
            ("MISE_TRUSTED_CONFIG_PATHS", scope.clone()),
            ("MISE_CD", scope.clone()),
            ("TMPDIR", scope.clone()),
            ("TEMP", scope.clone()),
            ("TMP", scope.clone()),
        ] {
            command.env(name, directory);
        }
        let output = command
            .current_dir(&scope)
            .args(["env", "--json"])
            .env("PATH", path)
            .env(MISE_DIFF, diff)
            .env_remove("DOTFILES_HOST_PATH")
            .env_remove(NATIVE_PATH_DIFF)
            .env_remove(NATIVE_PATH_DIFF_PRESENT)
            .env("MISE_AUTO_INSTALL", "false")
            .env("MISE_ENV_CACHE", "false")
            .env("MISE_AUTO_ENV", "false")
            .env("MISE_OVERRIDE_TOOL_VERSIONS_FILENAMES", "none")
            .stdin(Stdio::null())
            .output()
            .context("query the native PATH")?;
        let response = NativePathResponse::read(&output)?;
        let mut directories: Vec<_> =
            std::env::split_paths(OsStr::new(response.as_str())).collect();
        if let Some(capability) = capability {
            let directory = capability.parent().context("native GitHub CLI directory")?;
            if !directories.iter().any(|candidate| candidate == directory) {
                directories.push(directory.to_path_buf());
            }
        }
        Ok(Self {
            path: std::env::join_paths(directories)?,
            mise: Some(mise.to_path_buf()),
        })
    }

    #[must_use]
    pub fn as_os_str(&self) -> &OsStr {
        &self.path
    }
}

fn mise_executable(home: &Path, path: &OsStr) -> Result<PathBuf> {
    let path = crate::home::path_with(home, path)?;
    let mise = std::env::split_paths(&path)
        .map(|directory| directory.join(format!("mise{}", std::env::consts::EXE_SUFFIX)))
        .find(|path| path.is_file())
        .context("mise is unavailable; apply the host profile")?;
    Ok(canonical(&mise)?)
}

/// Installed tool storage without host configuration or credentials.
#[derive(Debug)]
pub struct Installations {
    pub mise: PathBuf,
    pub data: PathBuf,
    pub cargo: PathBuf,
    pub rustup: PathBuf,
    pub kani: PathBuf,
    pub cache: PathBuf,
}

impl Installations {
    /// Locate the existing mise executable and its storage without running an installer.
    ///
    /// # Errors
    ///
    /// Returns an error when the managed tool path contains no mise executable.
    pub fn discover(home: &Path) -> Result<Self> {
        let native = NativePath::capture(home)?;
        let mise = native
            .mise
            .map_or_else(|| mise_executable(home, &native.path), Ok)?;
        let data = std::env::var_os("MISE_DATA_DIR").map_or_else(
            || {
                if cfg!(windows) {
                    crate::home::local_app_data(home).join("mise")
                } else {
                    home.join(".local/share/mise")
                }
            },
            PathBuf::from,
        );
        Ok(Self {
            mise: canonical(&mise)?,
            data,
            cargo: std::env::var_os("DOTFILES_HOST_CARGO_HOME")
                .or_else(|| std::env::var_os("CARGO_HOME"))
                .map_or_else(|| home.join(".cargo"), PathBuf::from),
            rustup: std::env::var_os("RUSTUP_HOME")
                .map_or_else(|| home.join(".rustup"), PathBuf::from),
            kani: home.join(KANI_HOME),
            cache: std::env::var_os("DOTFILES_HOST_CACHE")
                .map_or_else(|| home.join(HOST_CACHE), PathBuf::from),
        })
    }

    fn shared_lookups(
        &self,
        origin: Option<&OsStr>,
        shared: Option<&OsStr>,
    ) -> Result<Vec<InstallationLookup>> {
        let Some(shared) = shared else {
            return Ok(Vec::new());
        };
        let Some(origin) = origin
            .map(Path::new)
            .filter(|origin| *origin == self.data.as_path())
        else {
            return Ok(Vec::new());
        };
        ensure!(
            origin.is_absolute()
                && self.data.is_absolute()
                && canonical(origin)? == canonical(&self.data)?,
            "shared installs belong to another native data origin"
        );
        let mut lookups = Vec::new();
        for path in std::env::split_paths(shared) {
            ensure!(
                lookups.len() < TOOL_INVENTORY_LIMIT,
                "shared install lookup inventory exceeds its member limit"
            );
            let lookup = InstallationLookup::capture(&path)?;
            if !lookups.contains(&lookup) {
                lookups.push(lookup);
            }
        }
        Ok(lookups)
    }
}

#[derive(Debug, PartialEq, Eq)]
struct InstallationLookup {
    installs: PathBuf,
    storage: PathBuf,
}

impl InstallationLookup {
    fn capture(path: &Path) -> Result<Self> {
        const INSTALLS: &str = "installs";
        ensure!(
            path.is_absolute() && path.file_name() == Some(OsStr::new(INSTALLS)),
            "native install lookup must name an absolute installs directory"
        );
        let storage = canonical(path.parent().context("native install lookup has no root")?)?;
        let installs = canonical(path)?;
        ensure!(
            installs == storage.join(INSTALLS) && installs.is_dir(),
            "native install lookup redirects outside its captured storage root"
        );
        Ok(Self { installs, storage })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    r#type: String,
    path: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Installation {
    version: String,
    requested_version: String,
    install_path: PathBuf,
    source: Source,
    installed: bool,
    active: bool,
    symlinked_to: Option<PathBuf>,
}

#[cfg(windows)]
#[derive(Debug, serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Compiler {
    executable: PathBuf,
    directories: Vec<PathBuf>,
    libraries: Vec<PathBuf>,
    includes: Vec<PathBuf>,
    library_paths: Vec<PathBuf>,
}

#[cfg(windows)]
impl Compiler {
    fn discover(environment: &CodeEnvironment) -> Result<Self> {
        let base: Vec<_> = std::env::split_paths(
            environment
                .variables
                .get(OsStr::new("PATH"))
                .context("code PATH is missing")?,
        )
        .collect();
        let tool =
            find_msvc_tools::find_tool_with_env(std::env::consts::ARCH, MSVC_COMPILER, environment)
                .context("the Windows code stage requires an installed MSVC toolchain and SDK")?;
        let mut compiler = Self {
            executable: tool.path().to_path_buf(),
            directories: vec![
                tool.path()
                    .parent()
                    .context("MSVC compiler directory")?
                    .into(),
            ],
            libraries: Vec::new(),
            includes: Vec::new(),
            library_paths: Vec::new(),
        };
        for (name, value) in tool.env() {
            let directories = match name.to_str() {
                Some("PATH") => &mut compiler.directories,
                Some("LIB") => &mut compiler.libraries,
                Some("INCLUDE") => &mut compiler.includes,
                Some("LIBPATH") => &mut compiler.library_paths,
                _ => anyhow::bail!(
                    "MSVC returned an unexpected environment key: {}",
                    name.display()
                ),
            };
            directories.extend(std::env::split_paths(value).filter(|path| {
                !path.as_os_str().is_empty() && (name != "PATH" || !base.contains(path))
            }));
        }
        Ok(compiler)
    }
}

/// Owns the scratch home for as long as a gate or one of its child processes runs.
#[derive(Debug)]
pub struct CodeEnvironment {
    scope: Option<tempfile::TempDir>,
    root: PathBuf,
    configuration: PathBuf,
    inputs: BTreeMap<PathBuf, Vec<u8>>,
    variables: BTreeMap<OsString, OsString>,
    docker_plugin: Option<(PathBuf, [u8; 32])>,
}

#[derive(Default)]
struct ProjectInputBudget {
    bytes: u64,
    nodes: usize,
}

fn regular_project_input(path: &Path, budget: &mut ProjectInputBudget) -> Result<Vec<u8>> {
    use std::io::Read as _;

    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "project input is not a regular file: {}",
        path.display()
    );
    ensure!(
        metadata.len() <= PROJECT_INPUT_LIMIT,
        "project input exceeds its byte limit"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(PROJECT_INPUT_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    budget.bytes = budget
        .bytes
        .checked_add(u64::try_from(bytes.len())?)
        .context("project input byte count overflow")?;
    budget.nodes += 1;
    ensure!(
        budget.bytes <= PROJECT_INPUT_LIMIT && budget.nodes <= PROJECT_INPUT_COUNT,
        "project inputs exceed their bounded capture"
    );
    ensure!(
        !fs::symlink_metadata(path)?.file_type().is_symlink(),
        "project input became a filesystem link"
    );
    Ok(bytes)
}

fn collect_sidecars(
    root: &Path,
    relative: &Path,
    depth: usize,
    budget: &mut ProjectInputBudget,
    inputs: &mut BTreeMap<PathBuf, Vec<u8>>,
) -> Result<()> {
    ensure!(
        depth <= PROJECT_INPUT_DEPTH,
        "project lock sidecars exceed their depth limit"
    );
    let path = root.join(relative);
    let metadata = fs::symlink_metadata(&path)?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "project lock sidecar is a filesystem link"
    );
    if metadata.is_file() {
        inputs.insert(relative.to_owned(), regular_project_input(&path, budget)?);
        return Ok(());
    }
    ensure!(metadata.is_dir(), "project lock sidecar has a special type");
    budget.nodes += 1;
    ensure!(
        budget.nodes <= PROJECT_INPUT_COUNT,
        "project lock sidecars exceed their entry limit"
    );
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name();
        let text = name
            .to_str()
            .context("project lock sidecar name is not Unicode")?;
        ensure!(
            rules::artifact_rules::portable_component(text.as_bytes()),
            "project lock sidecar name is not portable"
        );
        collect_sidecars(root, &relative.join(name), depth + 1, budget, inputs)?;
    }
    Ok(())
}

fn project_inputs(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    let mut budget = ProjectInputBudget::default();
    let mut inputs = BTreeMap::new();
    for name in PROJECT_INPUTS {
        inputs.insert(
            PathBuf::from(name),
            regular_project_input(&root.join(name), &mut budget)?,
        );
    }
    let sidecars = Path::new(LOCK_SIDECARS);
    let parent = sidecars
        .parent()
        .context("project lock namespace has no parent")?;
    match fs::symlink_metadata(root.join(parent)) {
        Ok(metadata) => ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "project lock namespace parent is not a regular directory"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(inputs),
        Err(error) => return Err(error.into()),
    }
    match fs::symlink_metadata(root.join(sidecars)) {
        Ok(_) => collect_sidecars(root, sidecars, 0, &mut budget, &mut inputs)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(inputs)
}

impl CodeEnvironment {
    /// Resolve only this checkout's pins in a clean environment, then build its gate environment.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing or inactive installation, a mismatched version or source,
    /// a nonabsolute tool directory, or an unsuccessful resolution command.
    pub fn prepare(root: &Path, installed: &Installations) -> Result<CodePreparation> {
        Self::prepare_scope(root, installed, tempfile::tempdir()?)
    }

    /// Stage the private data view beside the caller's fresh native materials.
    ///
    /// # Errors
    /// Refuses storage inside Source and invalid project or native pins.
    pub fn prepare_in(
        root: &Path,
        installed: &Installations,
        parent: &Path,
    ) -> Result<CodePreparation> {
        let parent = canonical(parent)?;
        ensure!(
            !parent.starts_with(canonical(root)?),
            "private native data view must stay outside Source"
        );
        Self::prepare_scope(root, installed, tempfile::tempdir_in(parent)?)
    }

    fn prepare_scope(
        root: &Path,
        installed: &Installations,
        scope: tempfile::TempDir,
    ) -> Result<CodePreparation> {
        let directory = canonical(scope.path())?;
        let root = canonical(root)?;
        let home = directory.join("home");
        let temporary = directory.join("tmp");
        let bootstrap = directory.join("bootstrap");
        for path in [&home, &temporary, &bootstrap] {
            fs::create_dir_all(path)?;
        }
        let empty = directory.join("empty-config.toml");
        fs::write(&empty, "")?;
        let mise = bootstrap.join(format!("mise{}", std::env::consts::EXE_SUFFIX));
        copy_bootstrap(&installed.mise, &mise).context("copy the mise bootstrap")?;
        let project = directory.join(PRIVATE_PROJECT);
        fs::create_dir(&project)?;
        let inputs = project_inputs(&root)?;
        for (name, bytes) in &inputs {
            let path = project.join(name);
            fs::create_dir_all(path.parent().context("private input has no parent")?)?;
            fs::write(path, bytes)?;
        }
        let github = home.join(".config/gh");
        fs::create_dir_all(&github)?;
        fs::write(github.join("hosts.yml"), GITHUB_HOSTS)?;
        let mut base = base_directories();
        let mut environment = Self {
            scope: Some(scope),
            root,
            configuration: project.join(MISE_CONFIG),
            inputs,
            variables: BTreeMap::new(),
            docker_plugin: None,
        };
        environment.seed(installed, &home, &temporary, &empty)?;
        for name in [
            "MISE_CONFIG_DIR",
            "MISE_CACHE_DIR",
            "MISE_STATE_DIR",
            "XDG_CACHE_HOME",
            "XDG_DATA_HOME",
            "XDG_STATE_HOME",
            #[cfg(windows)]
            "APPDATA",
            #[cfg(windows)]
            "LOCALAPPDATA",
            #[cfg(windows)]
            "ProgramData",
        ] {
            fs::create_dir_all(Path::new(
                environment
                    .variables
                    .get(OsStr::new(name))
                    .context("native mutable directory is missing")?,
            ))?;
        }
        #[cfg(unix)]
        for relative in POWERSHELL_USER_DIRECTORIES {
            fs::create_dir_all(home.join(relative))?;
        }
        fs::create_dir_all(home.join(DOCKER_USER_DIRECTORY))?;
        base.insert(0, bootstrap.clone());
        environment.set("PATH", std::env::join_paths(&base)?);
        #[cfg(windows)]
        {
            base = environment.compiler_directories()?;
            environment.set("PATH", std::env::join_paths(&base)?);
        }
        let releases = model::data::parse(
            RELEASE_PINS,
            &fs::read_to_string(environment.root.join(RELEASE_PINS))?,
        )
        .map_err(anyhow::Error::msg)?;
        let pin = releases
            .mise
            .and_then(|mise| mise.version)
            .context("mise has no release pin")?;
        let version = environment.output(&mise, &["--version"])?;
        ensure!(
            std::str::from_utf8(&version)?.split_whitespace().next() == Some(pin.as_str()),
            "the mise bootstrap does not match release pin {pin}"
        );
        Ok(CodePreparation {
            environment,
            mise,
            base,
            links: None,
            inherited: None,
        })
    }

    /// Refuse a missing native compiler before any material installer runs.
    ///
    /// # Errors
    /// Refuses invalid source or bootstrap pins and an unavailable Windows Microsoft Visual C++ toolchain.
    pub fn compiler_preflight(root: &Path, installed: &Installations) -> Result<()> {
        let mut preparation = Self::prepare(root, installed)?;
        preparation.admit_inherited(installed)
    }

    /// Resolve the existing native installations without granting material health.
    ///
    /// # Errors
    /// Refuses invalid project pins, bootstrap identity, and unresolved installations.
    pub fn build(root: &Path, installed: &Installations) -> Result<Self> {
        let mut preparation = Self::prepare(root, installed)?;
        let tools: BTreeMap<String, Vec<Installation>> = serde_json::from_slice(
            &preparation
                .environment
                .output(&preparation.mise, &["ls", "--local", "--json"])?,
        )?;
        if tools.contains_key(BUILDX_TOOL) {
            preparation.admit_inherited(installed)?;
        }
        preparation.finish(installed, false)?.into_environment()
    }
    fn seed(
        &mut self,
        installed: &Installations,
        home: &Path,
        temporary: &Path,
        empty: &Path,
    ) -> Result<()> {
        for (name, value) in [
            ("HOME", home.to_path_buf()),
            ("USERPROFILE", home.to_path_buf()),
            ("CARGO_HOME", installed.cache.join("cargo")),
            ("DOTFILES_HOST_CARGO_HOME", installed.cargo.clone()),
            ("RUSTUP_HOME", installed.rustup.clone()),
            ("MISE_RUSTUP_HOME", installed.rustup.clone()),
            ("MISE_CARGO_HOME", installed.cargo.clone()),
            ("MISE_DATA_DIR", installed.data.clone()),
            ("MISE_CONFIG_DIR", home.join(".config/mise")),
            ("MISE_CACHE_DIR", home.join(".cache/mise")),
            ("MISE_STATE_DIR", home.join(".local/state/mise")),
            ("MISE_GLOBAL_CONFIG_FILE", empty.to_path_buf()),
            ("MISE_SYSTEM_CONFIG_FILE", empty.to_path_buf()),
            (
                "MISE_TRUSTED_CONFIG_PATHS",
                self.configuration
                    .parent()
                    .context("private project has no parent")?
                    .to_path_buf(),
            ),
            (
                "MISE_CEILING_PATHS",
                self.root
                    .parent()
                    .context("checkout has no parent")?
                    .to_path_buf(),
            ),
            ("GIT_CONFIG_GLOBAL", empty.to_path_buf()),
            ("GIT_CONFIG_SYSTEM", empty.to_path_buf()),
            ("TMPDIR", temporary.to_path_buf()),
            ("TEMP", temporary.to_path_buf()),
            ("TMP", temporary.to_path_buf()),
            ("XDG_CONFIG_HOME", home.join(".config")),
            ("XDG_CACHE_HOME", home.join(".cache")),
            ("XDG_DATA_HOME", home.join(".local/share")),
            ("XDG_STATE_HOME", home.join(".local/state")),
            ("SKILL_OPS_STATE", home.join(".local/state/skill-ops")),
            ("GH_CONFIG_DIR", home.join(".config/gh")),
            ("DOTFILES_ONEPASSWORD_APP", home.join("absent-app")),
            (
                "CARGO_TARGET_DIR",
                crate::cargo_target::code(&self.root, &installed.cache)?,
            ),
            ("DOTFILES_HOST_CACHE", installed.cache.clone()),
            (MIRI_TARGET_VARIABLE, home.join(MIRI_TARGET_DIRECTORY)),
        ] {
            self.set(name, value.as_os_str());
        }
        self.set("MISE_CONFIG_FILE", self.configuration.clone());
        self.set("KANI_HOME", installed.kani.clone());
        for (name, value) in [
            ("GIT_CONFIG_NOSYSTEM", "1"),
            ("GIT_TERMINAL_PROMPT", "0"),
            ("MISE_AUTO_INSTALL", "false"),
            ("MISE_AUTO_PRUNE", "false"),
            ("MISE_GITHUB_GH_CLI_TOKENS", "false"),
            ("LC_ALL", "C"),
            ("LANG", "C"),
            ("TZ", "UTC"),
            ("TERM", "xterm-256color"),
        ] {
            self.set(name, value);
        }
        if cfg!(windows) {
            let system = system_root();
            self.set("SystemRoot", &system);
            self.set("WINDIR", &system);
            self.set("ComSpec", system.join("System32").join("cmd.exe"));
            self.set("PATHEXT", ".COM;.EXE;.BAT;.CMD");
            self.set("APPDATA", home.join("AppData/Roaming"));
            self.set("LOCALAPPDATA", home.join("AppData/Local"));
            self.set("ProgramData", home.join("ProgramData"));
            self.set("ALLUSERSPROFILE", home.join("ProgramData"));
            self.set("ProgramFiles", program_files());
        }
        let shared = installed.shared_lookups(
            std::env::var_os("MISE_DATA_DIR").as_deref(),
            std::env::var_os("MISE_SHARED_INSTALL_DIRS").as_deref(),
        )?;
        if !shared.is_empty() {
            self.set(
                "MISE_SHARED_INSTALL_DIRS",
                std::env::join_paths(shared.iter().map(|lookup| &lookup.installs))?,
            );
        }
        Ok(())
    }

    fn resolve(
        &mut self,
        mise: &Path,
        installed: &Installations,
        prepared: bool,
    ) -> Result<Vec<PathBuf>> {
        let listing = self.output(mise, &["ls", "--local", "--json"])?;
        let mut tools: BTreeMap<String, Vec<Installation>> = serde_json::from_slice(&listing)?;
        ensure!(!tools.is_empty(), "the checkout has no pinned tools");
        for (name, versions) in &tools {
            ensure!(
                versions.len() == 1,
                "{name} must resolve to exactly one pinned version"
            );
            for tool in versions {
                ensure!(
                    (tool.active || !tool.installed)
                        && tool.source.r#type == MISE_CONFIG
                        && canonical(&tool.source.path)? == self.configuration
                        && tool.version == tool.requested_version,
                    "{name} does not resolve to its exact active project pin"
                );
            }
        }
        if !prepared {
            self.prepare_tools(mise, installed, &tools)?;
        }
        {
            let resolved: BTreeMap<String, Vec<Installation>> =
                serde_json::from_slice(&self.output(mise, &["ls", "--local", "--json"])?)?;
            ensure!(
                tools.keys().eq(resolved.keys())
                    && tools.iter().all(|(name, versions)| versions
                        .iter()
                        .map(|tool| &tool.requested_version)
                        .eq(resolved[name].iter().map(|tool| &tool.requested_version))),
                "installation changed the project pin inventory"
            );
            tools = resolved;
        }
        let mut requests = Vec::new();
        let mut rust = None;
        for (name, versions) in tools {
            ensure!(
                versions.len() == 1,
                "{name} must resolve to exactly one pinned version"
            );
            for tool in versions {
                ensure!(
                    rules::hosts_rules::pinned_tool([
                        tool.installed,
                        tool.active,
                        tool.source.r#type == MISE_CONFIG
                            && canonical(&tool.source.path)? == self.configuration,
                        tool.version == tool.requested_version,
                    ]),
                    "{name} does not resolve to its installed project pin: {tool:?}"
                );
                ensure!(
                    tool.install_path.is_absolute(),
                    "{name} has a relative installation"
                );
                if let Some(link) = tool.symlinked_to {
                    ensure!(
                        link.is_absolute(),
                        "{name} has a relative installation link"
                    );
                }
                if name == "rust" {
                    rust = Some(tool.version);
                } else {
                    requests.push(format!("{name}@{}", tool.version));
                }
            }
        }
        let mut directories = Vec::new();
        if let Some(version) = rust {
            self.set("RUSTUP_TOOLCHAIN", &version);
            let rustc = installed
                .cargo
                .join(CARGO_BIN)
                .join(format!("rustc{}", std::env::consts::EXE_SUFFIX));
            let actual = self.output(&rustc, &["--version"])?;
            ensure!(
                std::str::from_utf8(&actual)?.split_whitespace().nth(1) == Some(version.as_str()),
                "rustc does not match project pin {version}"
            );
            let sysroot = self.output(&rustc, &["--print", "sysroot"])?;
            directories
                .push(canonical(Path::new(std::str::from_utf8(&sysroot)?.trim()))?.join(CARGO_BIN));
        }
        if !requests.is_empty() {
            let arguments: Vec<_> = std::iter::once("bin-paths")
                .chain(requests.iter().map(String::as_str))
                .collect();
            let resolved = self.output(mise, &arguments)?;
            for directory in std::str::from_utf8(&resolved)?.lines() {
                let directory = Path::new(directory);
                ensure!(
                    directory.is_absolute(),
                    "mise returned a relative bin directory"
                );
                directories.push(canonical(directory)?);
            }
        }
        Ok(directories)
    }

    #[cfg(windows)]
    fn compiler_directories(&mut self) -> Result<Vec<PathBuf>> {
        let mut compiler: Compiler = match std::env::var_os(REGISTERED_COMPILER) {
            Some(value) => serde_json::from_str(
                value
                    .to_str()
                    .context("registered compiler environment is not Unicode")?,
            )
            .context("invalid registered compiler environment")?,
            None => Compiler::discover(self)?,
        };
        ensure!(
            compiler.executable.is_absolute()
                && compiler.executable.file_name() == Some(OsStr::new(MSVC_COMPILER))
                && compiler.executable.is_file(),
            "registered compiler must name an installed MSVC compiler"
        );
        ensure!(
            !compiler.directories.is_empty()
                && !compiler.libraries.is_empty()
                && !compiler.includes.is_empty(),
            "registered compiler requires tool, library, and include directories"
        );
        compiler.executable = canonical(&compiler.executable)?;
        for directory in &mut compiler.directories {
            ensure!(
                directory.is_absolute() && directory.is_dir(),
                "registered compiler tool directory must be absolute and installed"
            );
            *directory = canonical(directory)?;
        }
        for directories in [
            &compiler.libraries,
            &compiler.includes,
            &compiler.library_paths,
        ] {
            for directory in directories {
                ensure!(
                    directory.is_absolute(),
                    "registered compiler library and include directories must be absolute"
                );
            }
        }
        ensure!(
            compiler.directories.first().map(PathBuf::as_path) == compiler.executable.parent(),
            "registered compiler must lead its tool directories"
        );
        for (name, directories) in [
            ("LIB", &compiler.libraries),
            ("INCLUDE", &compiler.includes),
            ("LIBPATH", &compiler.library_paths),
        ] {
            self.set(name, std::env::join_paths(directories)?);
        }
        self.set(REGISTERED_COMPILER, serde_json::to_string(&compiler)?);
        compiler.directories.extend(std::env::split_paths(
            self.variables
                .get(OsStr::new("PATH"))
                .context("code PATH is missing")?,
        ));
        Ok(compiler.directories)
    }

    fn prepare_tools(
        &mut self,
        mise: &Path,
        installed: &Installations,
        tools: &BTreeMap<String, Vec<Installation>>,
    ) -> Result<()> {
        let home = PathBuf::from(
            self.variables
                .get(OsStr::new("HOME"))
                .context("code HOME is missing")?,
        );
        let data = home.join(".local/share/mise");
        link_installations(&data, installed, tools, false)?;
        if let Some(kani) = tools.get(KANI_TOOL).and_then(|versions| versions.first()) {
            let bundle = format!("kani-{}", kani.version);
            let source = installed.kani.join(&bundle);
            if source.is_dir() {
                fs::create_dir_all(home.join(KANI_HOME))?;
                link_entry(&source, &home.join(KANI_HOME).join(bundle))?;
            }
        }
        let mut bootstrap = Vec::new();
        if let Some(rust) = tools.get("rust").and_then(|versions| versions.first()) {
            link_rust_proxies(mise, installed)?;
            self.set("RUSTUP_TOOLCHAIN", &rust.version);
            if rust.installed {
                let rustc = installed
                    .cargo
                    .join(CARGO_BIN)
                    .join(format!("rustc{}", std::env::consts::EXE_SUFFIX));
                let output = self.output(&rustc, &["--print", "sysroot"])?;
                bootstrap.push(
                    canonical(Path::new(std::str::from_utf8(&output)?.trim()))?.join(CARGO_BIN),
                );
            }
        }
        for (name, versions) in tools {
            if name == "rust" {
                continue;
            }
            for tool in versions.iter().filter(|tool| tool.installed) {
                let request = format!("{name}@{}", tool.version);
                let output = self.output(mise, &["bin-paths", &request])?;
                for directory in std::str::from_utf8(&output)?.lines() {
                    ensure!(
                        Path::new(directory).is_absolute(),
                        "mise returned a relative bootstrap directory"
                    );
                    bootstrap.push(canonical(Path::new(directory))?);
                }
            }
        }
        bootstrap.extend(std::env::split_paths(
            self.variables
                .get(OsStr::new("PATH"))
                .context("code PATH is missing")?,
        ));
        self.set("PATH", std::env::join_paths(bootstrap)?);
        self.set("MISE_DATA_DIR", &data);
        self.set("MISE_CARGO_HOME", home.join(".cargo"));
        if tools
            .get("rust")
            .is_some_and(|tools| tools.iter().any(|tool| !tool.installed))
        {
            self.set("RUSTUP_HOME", home.join(".rustup"));
            self.set("MISE_RUSTUP_HOME", home.join(".rustup"));
        }
        let staged: BTreeMap<String, Vec<Installation>> =
            serde_json::from_slice(&self.output(mise, &["ls", "--local", "--json"])?)?;
        ensure!(
            tools.keys().eq(staged.keys())
                && tools.iter().all(|(name, versions)| versions
                    .iter()
                    .map(|tool| &tool.requested_version)
                    .eq(staged[name].iter().map(|tool| &tool.requested_version))),
            "scratch storage changed the project pin inventory"
        );
        let mut requests = Vec::new();
        for (name, versions) in tools {
            for (tool, staged) in versions.iter().zip(&staged[name]) {
                if !staged.installed {
                    if tool.installed {
                        let relative = tool
                            .install_path
                            .strip_prefix(installed.data.join("installs"))?;
                        let link = data.join("installs").join(relative);
                        ensure!(
                            canonical(&link)?.starts_with(&data),
                            "an invalid scratch installation must belong to scratch storage"
                        );
                        fs::remove_dir_all(&link)?;
                    }
                    requests.push(format!("{name}@{}", tool.requested_version));
                }
            }
        }
        let arguments: Vec<_> = ["install", "--yes"]
            .into_iter()
            .chain(requests.iter().map(String::as_str))
            .collect();
        if !requests.is_empty() {
            self.output(mise, &arguments)?;
        }
        Ok(())
    }

    fn set(&mut self, name: &str, value: impl AsRef<OsStr>) {
        self.variables
            .insert(name.into(), value.as_ref().to_owned());
    }

    /// Recheck the exact original project inputs and their private copies.
    ///
    /// # Errors
    /// Refuses a moved configuration or changed source and copied bytes.
    pub fn require_current(&self) -> Result<()> {
        ensure!(
            self.scope.is_some(),
            "code environment ownership was transferred"
        );
        ensure!(
            canonical(&self.configuration)? == self.configuration,
            "private project configuration changed"
        );
        let project = self
            .configuration
            .parent()
            .context("private project has no parent")?;
        ensure!(
            project_inputs(&self.root)? == self.inputs,
            "Source project tool inputs changed"
        );
        ensure!(
            project_inputs(project)? == self.inputs,
            "private project tool inputs changed"
        );
        if let Some((path, digest)) = &self.docker_plugin {
            ensure!(
                fs::symlink_metadata(path)?.file_type().is_file()
                    && canonical(path)? == *path
                    && Sha256::digest(fs::read(path)?)[..] == digest[..],
                "owned Docker Buildx plugin changed"
            );
        }
        Ok(())
    }

    /// A gate command and all its descendants inherit this environment.
    #[must_use]
    pub fn command(&self, program: impl AsRef<OsStr>) -> Command {
        let mut command = crate::process::isolated_command(program, &self.variables);
        command.current_dir(&self.root);
        command
    }

    fn output(&self, program: &Path, arguments: &[&str]) -> Result<Vec<u8>> {
        let mut command = self.command(program);
        command.current_dir(
            self.configuration
                .parent()
                .context("private project has no parent")?,
        );
        command.args(arguments);
        let output = if arguments.first() == Some(&"install") {
            crate::process::mise_install(command)?.output()
        } else {
            command.output()
        }
        .with_context(|| format!("start {} {arguments:?}", program.display()))?;
        ensure!(
            output.status.success(),
            "{} {arguments:?}: {}",
            program.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(output.stdout)
    }
}

/// An owned private project view before its native health gate.
#[derive(Debug)]
pub struct CodePreparation {
    environment: CodeEnvironment,
    mise: PathBuf,
    base: Vec<PathBuf>,
    links: Option<RustKaniLinks>,
    inherited: Option<InheritedTools>,
}

#[derive(Debug)]
struct InheritedTools {
    tools: BTreeMap<String, Vec<Installation>>,
    lookups: Vec<InstallationLookup>,
}

/// A borrowing resolver result which leaves ownership with its preparation on failure.
#[derive(Debug)]
pub struct ResolvedCode<'a> {
    environment: &'a mut CodeEnvironment,
}

impl ResolvedCode<'_> {
    /// Transfer the validated environment after the caller's complete health operation succeeds.
    ///
    /// # Errors
    /// Refuses an already transferred environment without consuming its preparation owner.
    pub fn into_environment(self) -> Result<CodeEnvironment> {
        let scope = self
            .environment
            .scope
            .take()
            .context("code environment ownership was transferred")?;
        Ok(CodeEnvironment {
            scope: Some(scope),
            root: std::mem::take(&mut self.environment.root),
            configuration: std::mem::take(&mut self.environment.configuration),
            inputs: std::mem::take(&mut self.environment.inputs),
            variables: std::mem::take(&mut self.environment.variables),
            docker_plugin: self.environment.docker_plugin.take(),
        })
    }
}

/// The original resolver's checked logical paths and their actual fresh stores.
#[derive(Debug)]
pub struct RustKaniLinks {
    data: PathBuf,
    rust: PathBuf,
    launcher: Option<PathBuf>,
}

impl RustKaniLinks {
    #[must_use]
    pub fn data(&self) -> &Path {
        &self.data
    }
    #[must_use]
    pub fn rust(&self) -> &Path {
        &self.rust
    }
    #[must_use]
    pub fn launcher(&self) -> Option<&Path> {
        self.launcher.as_deref()
    }
}

impl CodePreparation {
    /// Capture and check every inherited pin before native material preparation.
    ///
    /// # Errors
    /// Refuses incomplete native tools, changed variants, unrelated storage, and changed inputs.
    pub fn admit_inherited(&mut self, installed: &Installations) -> Result<()> {
        self.environment.require_current()?;
        self.environment.set("MISE_OFFLINE", "true");
        self.environment.set("MISE_LOCKED", "true");
        let scope = self
            .environment
            .scope
            .as_ref()
            .context("code environment ownership was transferred")?
            .path()
            .to_owned();
        self.environment.set(
            "MISE_SYSTEM_INSTALLS_DIR",
            scope.join(ABSENT_SYSTEM_INSTALLS),
        );
        if let Some(inherited) = &self.inherited {
            let listing = self
                .environment
                .output(&self.mise, &["ls", "--local", "--json"])?;
            return inherited.require_matching(
                &serde_json::from_slice(&listing)?,
                &self.environment.configuration,
            );
        }
        let original = self
            .environment
            .output(&self.mise, &["ls", "--local", "--json"])?;
        fs::write(scope.join(NATIVE_READBACK), &original)?;
        let tools: BTreeMap<String, Vec<Installation>> = serde_json::from_slice(&original)?;
        require_inherited_pins(&tools, &self.environment.configuration)?;
        let mut lookups = vec![InstallationLookup::capture(
            &installed.data.join("installs"),
        )?];
        for lookup in installed.shared_lookups(
            self.environment
                .variables
                .get(OsStr::new("MISE_DATA_DIR"))
                .map(OsString::as_os_str),
            self.environment
                .variables
                .get(OsStr::new("MISE_SHARED_INSTALL_DIRS"))
                .map(OsString::as_os_str),
        )? {
            if !lookups.contains(&lookup) {
                lookups.push(lookup);
            }
        }
        let inherited = InheritedTools { tools, lookups };
        inherited.require_native_storage()?;
        let staged = self.stage_inherited(&inherited)?;
        fs::write(scope.join(STAGED_READBACK), &staged)?;
        inherited.require_matching(
            &serde_json::from_slice(&staged)?,
            &self.environment.configuration,
        )?;
        if let Some(versions) = inherited.tools.get(BUILDX_TOOL) {
            let request = format!("{BUILDX_TOOL}@{}", versions[0].version);
            let paths = self
                .environment
                .output(&self.mise, &["bin-paths", &request])?;
            let directories = std::str::from_utf8(&paths)?
                .lines()
                .map(PathBuf::from)
                .collect::<Vec<_>>();
            self.stage_docker_plugin(&inherited, &directories)?;
        }
        self.environment.require_current()?;
        self.inherited = Some(inherited);
        Ok(())
    }

    fn stage_docker_plugin(
        &mut self,
        inherited: &InheritedTools,
        directories: &[PathBuf],
    ) -> Result<()> {
        require_inherited_pins(&inherited.tools, &self.environment.configuration)?;
        inherited.require_native_storage()?;
        let [tool] = inherited
            .tools
            .get(BUILDX_TOOL)
            .context("missing Buildx admission")?
            .as_slice()
        else {
            anyhow::bail!("ambiguous Buildx admission");
        };
        let installation = canonical(&tool.install_path)?;
        let program = format!("{BUILDX_PROGRAM}{}", std::env::consts::EXE_SUFFIX);
        let mut sources = Vec::new();
        for directory in directories {
            ensure!(directory.is_absolute(), "Buildx bin directory is relative");
            let directory = canonical(directory)?;
            ensure!(
                directory.starts_with(&installation),
                "Buildx bin directory escaped its admitted installation"
            );
            let source = directory.join(&program);
            if source.try_exists()? {
                ensure!(
                    fs::symlink_metadata(&source)?.file_type().is_file(),
                    "Buildx plugin is not a regular file"
                );
                sources.push(source);
            }
        }
        sources.sort();
        sources.dedup();
        let [source] = sources.as_slice() else {
            anyhow::bail!("Buildx must resolve to one admitted executable");
        };
        let before = Sha256::digest(fs::read(source)?);
        let home = self
            .environment
            .scope
            .as_ref()
            .context("code environment ownership was transferred")?
            .path()
            .join("home");
        let directory = home
            .join(DOCKER_USER_DIRECTORY)
            .join(DOCKER_PLUGIN_DIRECTORY);
        fs::create_dir(&directory)?;
        fs::create_dir(
            home.join(DOCKER_USER_DIRECTORY)
                .join(BUILDX_STATE_DIRECTORY),
        )?;
        let destination =
            directory.join(format!("{BUILDX_PLUGIN}{}", std::env::consts::EXE_SUFFIX));
        copy_bootstrap(source, &destination)?;
        ensure!(
            Sha256::digest(fs::read(&destination)?) == before
                && Sha256::digest(fs::read(source)?) == before,
            "Buildx bytes changed during staging"
        );
        self.environment.docker_plugin = Some((canonical(&destination)?, before.into()));
        Ok(())
    }

    fn stage_inherited(&mut self, inherited: &InheritedTools) -> Result<Vec<u8>> {
        let home = PathBuf::from(
            self.environment
                .variables
                .get(OsStr::new("HOME"))
                .context("code HOME is missing")?,
        );
        let data = home.join(PRIVATE_DATA);
        fs::create_dir_all(&data)?;
        self.environment.set("MISE_DATA_DIR", &data);
        self.environment.set(
            "MISE_SHARED_INSTALL_DIRS",
            std::env::join_paths(inherited.lookups.iter().map(|lookup| &lookup.installs))?,
        );
        self.environment
            .output(&self.mise, &["ls", "--local", "--json"])
    }

    /// Register fresh stores with the public resolver, retaining its original readback.
    ///
    /// # Errors
    /// Refuses changed project inputs, missing stores, wrong logical paths, and unrelated targets.
    pub fn link_rust_kani(
        &mut self,
        installed: &Installations,
        rust: &Path,
        launcher: Option<&Path>,
    ) -> Result<&RustKaniLinks> {
        self.admit_inherited(installed)?;
        self.environment.require_current()?;
        let tools: BTreeMap<String, Vec<Installation>> = serde_json::from_slice(
            &self
                .environment
                .output(&self.mise, &["ls", "--local", "--json"])?,
        )?;
        let home = PathBuf::from(
            self.environment
                .variables
                .get(OsStr::new("HOME"))
                .context("code HOME is missing")?,
        );
        let data = home.join(PRIVATE_DATA);
        fs::create_dir_all(&data)?;
        self.environment.set("MISE_DATA_DIR", &data);
        self.environment.set("MISE_OFFLINE", "true");
        self.environment.set("MISE_LOCKED", "true");
        self.link_fresh_stores(&tools, installed, rust, launcher)?;
        link_rust_proxies(&self.mise, installed)?;
        if let Some(launcher) = launcher {
            self.base.insert(0, canonical(&launcher.join(CARGO_BIN))?);
        }
        self.environment
            .set("PATH", std::env::join_paths(&self.base)?);
        let original = self
            .environment
            .output(&self.mise, &["ls", "--local", "--json"])?;
        fs::write(
            self.environment
                .scope
                .as_ref()
                .context("code environment ownership was transferred")?
                .path()
                .join(LINK_READBACK),
            &original,
        )?;
        let resolved: BTreeMap<String, Vec<Installation>> = serde_json::from_slice(&original)?;
        self.inherited
            .as_ref()
            .context("inherited inventory was not captured")?
            .require_matching(&resolved, &self.environment.configuration)?;
        let rust = checked_store(
            &resolved,
            "rust",
            &self.environment.configuration,
            &data,
            rust,
        )?;
        let launcher = launcher
            .map(|path| {
                checked_store(
                    &resolved,
                    KANI_TOOL,
                    &self.environment.configuration,
                    &data,
                    path,
                )
            })
            .transpose()?;
        self.environment.require_current()?;
        self.links = Some(RustKaniLinks {
            data,
            rust,
            launcher,
        });
        self.links
            .as_ref()
            .context("native material links were not recorded")
    }

    fn link_fresh_stores(
        &mut self,
        tools: &BTreeMap<String, Vec<Installation>>,
        installed: &Installations,
        rust: &Path,
        launcher: Option<&Path>,
    ) -> Result<()> {
        for (name, path) in [
            ("RUSTUP_HOME", &installed.rustup),
            ("MISE_RUSTUP_HOME", &installed.rustup),
            ("MISE_CARGO_HOME", &installed.cargo),
            ("DOTFILES_HOST_CARGO_HOME", &installed.cargo),
            ("KANI_HOME", &installed.kani),
        ] {
            self.environment.set(name, path);
        }
        let rust_pin = exact_project_pin(tools, "rust", &self.environment.configuration)?;
        self.environment.output(
            &self.mise,
            &[
                "link",
                &format!("rust@{rust_pin}"),
                rust.to_str().context("fresh Rust path is not Unicode")?,
            ],
        )?;
        match (tools.get(KANI_TOOL), launcher) {
            (Some(_), Some(path)) => {
                let pin = exact_project_pin(tools, KANI_TOOL, &self.environment.configuration)?;
                self.environment.output(
                    &self.mise,
                    &[
                        "link",
                        &format!("{KANI_TOOL}@{pin}"),
                        path.to_str()
                            .context("fresh Kani launcher path is not Unicode")?,
                    ],
                )?;
            }
            (None, None) => {}
            _ => anyhow::bail!("native verifier project and material inventories differ"),
        }
        Ok(())
    }

    /// Add only the private resolver and compiler paths to an already isolated health command.
    ///
    /// # Errors
    /// Refuses an unlinked data view or changed original and copied project inputs.
    pub fn configure_health(&self, command: &mut Command) -> Result<()> {
        self.links
            .as_ref()
            .context("native health has no original resolver links")?;
        self.environment.require_current()?;
        for (name, value) in &self.environment.variables {
            let key = name.to_string_lossy();
            if key.starts_with("MISE_")
                || key == "PATH"
                || key == "CARGO_TARGET_DIR"
                || key == "LIB"
                || key == "INCLUDE"
                || key == "LIBPATH"
                || key == "DOTFILES_HOST_COMPILER"
            {
                command.env(name, value);
            }
        }
        command.current_dir(&self.environment.root);
        Ok(())
    }

    #[must_use]
    pub fn immutable_paths(&self) -> Vec<PathBuf> {
        let project = self.environment.configuration.with_file_name("");
        let mut paths = self
            .environment
            .inputs
            .keys()
            .map(|name| project.join(name))
            .collect::<Vec<_>>();
        if let Some(bootstrap) = self.mise.parent() {
            paths.push(bootstrap.to_owned());
        }
        if let Some((path, _)) = &self.environment.docker_plugin {
            paths.push(path.clone());
        }
        if let Some(links) = &self.links {
            paths.push(links.data.join("installs"));
            if let Some(path) = &links.launcher {
                paths.push(path.clone());
            }
        }
        paths
    }

    /// Build only after the trusted caller has completed original native health.
    ///
    /// # Errors
    /// Refuses a missing original resolver binding and any unresolved project tool.
    pub fn build(&mut self, installed: &Installations) -> Result<ResolvedCode<'_>> {
        self.links
            .as_ref()
            .context("prepared code environment has no resolver binding")?;
        self.finish(installed, true)
    }

    fn finish(&mut self, installed: &Installations, prepared: bool) -> Result<ResolvedCode<'_>> {
        self.environment.require_current()?;
        if let Some(inherited) = &self.inherited {
            let original = self
                .environment
                .output(&self.mise, &["ls", "--local", "--json"])?;
            inherited.require_matching(
                &serde_json::from_slice(&original)?,
                &self.environment.configuration,
            )?;
        }
        let mut directories = self.environment.resolve(&self.mise, installed, prepared)?;
        directories.extend(self.base.iter().cloned());
        let bootstrap = self
            .mise
            .parent()
            .context("mise bootstrap has no directory")?;
        directories.retain(|directory| directory != bootstrap);
        directories.insert(0, bootstrap.into());
        self.environment
            .set("PATH", std::env::join_paths(directories)?);
        self.environment.require_current()?;
        Ok(ResolvedCode {
            environment: &mut self.environment,
        })
    }
}

fn exact_project_pin<'a>(
    tools: &'a BTreeMap<String, Vec<Installation>>,
    name: &str,
    configuration: &Path,
) -> Result<&'a str> {
    let versions = tools
        .get(name)
        .with_context(|| format!("project has no {name} pin"))?;
    let [tool] = versions.as_slice() else {
        anyhow::bail!("{name} project pin is missing or ambiguous");
    };
    ensure!(
        tool.source.r#type == MISE_CONFIG
            && canonical(&tool.source.path)? == configuration
            && tool.version == tool.requested_version,
        "{name} does not resolve to the exact private project pin"
    );
    Ok(&tool.requested_version)
}

fn fresh_native_tool(name: &str) -> bool {
    name == "rust" || name == KANI_TOOL
}

fn require_inherited_pins(
    tools: &BTreeMap<String, Vec<Installation>>,
    configuration: &Path,
) -> Result<()> {
    ensure!(!tools.is_empty(), "the checkout has no pinned tools");
    ensure!(
        tools.len() <= TOOL_INVENTORY_LIMIT,
        "project tool inventory exceeds its member limit"
    );
    let mut failures = Vec::new();
    for (name, versions) in tools {
        let checked = (|| {
            exact_project_pin(tools, name, configuration)?;
            if fresh_native_tool(name) {
                return Ok(());
            }
            let [tool] = versions.as_slice() else {
                anyhow::bail!("{name} must resolve to exactly one pinned version");
            };
            ensure!(
                rules::hosts_rules::pinned_tool([
                    tool.installed,
                    tool.active,
                    tool.source.r#type == MISE_CONFIG
                        && canonical(&tool.source.path)? == configuration,
                    tool.version == tool.requested_version,
                ]),
                "{name} does not resolve to its installed project pin: {tool:?}"
            );
            ensure!(
                tool.install_path.is_absolute(),
                "{name} has a relative installation"
            );
            Ok(())
        })();
        if let Err(error) = checked {
            failures.push(tool_failure(name, &error));
        }
    }
    require_tools(&failures)
}

fn tool_failure(name: &str, error: &anyhow::Error) -> String {
    format!("{name}: {error:#}")
        .chars()
        .take(TOOL_DIAGNOSTIC_LIMIT)
        .collect()
}

fn require_tools(failures: &[String]) -> Result<()> {
    ensure!(
        failures.is_empty(),
        "inherited tool admission refused {} member(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    Ok(())
}

impl InheritedTools {
    fn require_native_storage(&self) -> Result<()> {
        let mut failures = Vec::new();
        for (name, versions) in &self.tools {
            if fresh_native_tool(name) {
                continue;
            }
            let checked = (|| {
                let [tool] = versions.as_slice() else {
                    anyhow::bail!("{name} native installation is ambiguous");
                };
                let endpoint = canonical(&tool.install_path)?;
                ensure!(
                    self.lookups
                        .iter()
                        .any(|lookup| tool.install_path.starts_with(&lookup.installs)
                            && endpoint.starts_with(&lookup.storage)),
                    "{name} native installation is outside its captured installs lookup or storage"
                );
                Ok(())
            })();
            if let Err(error) = checked {
                failures.push(tool_failure(name, &error));
            }
        }
        require_tools(&failures)
    }

    fn require_matching(
        &self,
        staged: &BTreeMap<String, Vec<Installation>>,
        configuration: &Path,
    ) -> Result<()> {
        require_inherited_pins(staged, configuration)?;
        ensure!(
            self.tools.keys().eq(staged.keys()),
            "staging changed the complete project pin inventory: native={:?}, staged={:?}",
            self.tools.keys().collect::<Vec<_>>(),
            staged.keys().collect::<Vec<_>>()
        );
        self.require_native_storage()?;
        let mut failures = Vec::new();
        for (name, originals) in &self.tools {
            let checked = (|| {
                let [original] = originals.as_slice() else {
                    anyhow::bail!("{name} native installation is ambiguous");
                };
                let [current] = staged
                    .get(name)
                    .context("staged tool is missing")?
                    .as_slice()
                else {
                    anyhow::bail!("{name} staged installation is ambiguous");
                };
                ensure!(
                    original.requested_version == current.requested_version
                        && original.version == current.version,
                    "{name} staging changed its exact project pin"
                );
                if fresh_native_tool(name) {
                    return Ok(());
                }
                ensure!(
                    original.install_path.file_name() == current.install_path.file_name()
                        && canonical(&original.install_path)? == canonical(&current.install_path)?,
                    "{name} staging changed its native installation variant or endpoint"
                );
                Ok(())
            })();
            if let Err(error) = checked {
                failures.push(tool_failure(name, &error));
            }
        }
        require_tools(&failures)
    }
}

fn checked_store(
    tools: &BTreeMap<String, Vec<Installation>>,
    name: &str,
    configuration: &Path,
    data: &Path,
    expected: &Path,
) -> Result<PathBuf> {
    exact_project_pin(tools, name, configuration)?;
    let tool = &tools[name][0];
    ensure!(
        tool.installed && tool.active && tool.install_path.is_absolute(),
        "{name} resolver path is not an active absolute installation"
    );
    if name == KANI_TOOL {
        ensure!(
            tool.install_path.starts_with(data.join("installs")),
            "native verifier resolver path is outside its owned logical installation namespace"
        );
    }
    if let Some(link) = &tool.symlinked_to {
        ensure!(
            link.is_absolute() && canonical(link)? == canonical(expected)?,
            "{name} resolver names an unrelated linked store"
        );
    }
    ensure!(
        canonical(&tool.install_path)? == canonical(expected)?,
        "{name} resolver did not select the fresh physical store"
    );
    Ok(tool.install_path.clone())
}

#[cfg(windows)]
impl find_msvc_tools::EnvGetter for CodeEnvironment {
    fn get_env(&self, name: &'static str) -> Option<find_msvc_tools::Env> {
        self.variables
            .get(OsStr::new(name))
            .cloned()
            .map(find_msvc_tools::Env::Owned)
    }
}

fn link_rust_proxies(mise: &Path, installed: &Installations) -> Result<()> {
    use crate::tool::Tool;

    let source = installed.cargo.join(CARGO_BIN).join(format!(
        "{}{}",
        Tool::Rustup.program(),
        std::env::consts::EXE_SUFFIX
    ));
    for program in [
        Tool::Rustup.program(),
        Tool::Cargo.program(),
        Tool::Rustc.program(),
    ]
    .into_iter()
    .chain(RUSTUP_COMPONENT_PROXIES)
    {
        link_executable(
            &source,
            &mise.with_file_name(format!("{}{}", program, std::env::consts::EXE_SUFFIX)),
        )?;
    }
    Ok(())
}

fn copy_bootstrap(source: &Path, destination: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        // Only the copy worker opens the executable for writing, so concurrent forks can't inherit its writer.
        let output = crate::process::command(canonical(Path::new(BOOTSTRAP_COPY))?)
            .env_clear()
            .arg("--")
            .arg(source)
            .arg(destination)
            .output()?;
        ensure!(
            output.status.success(),
            "{BOOTSTRAP_COPY}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::set_permissions(destination, fs::metadata(source)?.permissions())?;
    }
    #[cfg(windows)]
    fs::copy(source, destination)?;
    Ok(())
}

fn link_installations(
    data: &Path,
    installed: &Installations,
    tools: &BTreeMap<String, Vec<Installation>>,
    prepared_rust: bool,
) -> Result<()> {
    for (name, versions) in tools {
        if prepared_rust && matches!(name.as_str(), "rust" | KANI_TOOL) {
            continue;
        }
        for tool in versions.iter().filter(|tool| tool.installed) {
            let relative = tool
                .install_path
                .strip_prefix(installed.data.join("installs"))
                .with_context(|| format!("{name} installation belongs outside the tool store"))?;
            let link = data.join("installs").join(relative);
            let directory = link.parent().context("installation link has no parent")?;
            fs::create_dir_all(directory)?;
            let metadata = tool
                .install_path
                .parent()
                .context("installation has no parent")?
                .join(BACKEND_METADATA);
            if metadata.try_exists()? {
                fs::copy(metadata, directory.join(BACKEND_METADATA))?;
            }
            fs::create_dir(&link)?;
            for entry in fs::read_dir(&tool.install_path)? {
                let entry = entry?;
                let destination = link.join(entry.file_name());
                link_entry(&entry.path(), &destination)?;
            }
        }
    }
    Ok(())
}

/// Borrow an existing executable without opening a new executable for writing.
///
/// # Errors
///
/// Returns an error when resolving the source or creating the native link fails.
pub fn link_executable(source: &Path, destination: &Path) -> Result<()> {
    link_entry(&canonical(source)?, destination)
}

fn link_entry(source: &Path, destination: &Path) -> Result<()> {
    #[cfg(unix)]
    std::os::unix::fs::symlink(source, destination)?;
    #[cfg(windows)]
    if source.is_dir() {
        std::os::windows::fs::symlink_dir(source, destination)?;
    } else {
        std::os::windows::fs::symlink_file(source, destination)?;
    }
    Ok(())
}

impl Drop for CodeEnvironment {
    fn drop(&mut self) {
        if let Some(scope) = &self.scope {
            crate::cleanup::report(
                fs::remove_dir_all(scope.path()),
                "remove the code gate environment",
            );
        }
    }
}

fn system_root() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map_or_else(|| crate::home::DEFAULT_SYSTEM_ROOT.into(), PathBuf::from)
}

fn program_files() -> PathBuf {
    const DEFAULT: &str = "C:/Program Files";
    std::env::var_os("ProgramFiles").map_or_else(|| DEFAULT.into(), PathBuf::from)
}

fn base_directories() -> Vec<PathBuf> {
    if cfg!(windows) {
        let system = system_root();
        let git = program_files().join(WINDOWS_GIT);
        vec![
            system.join("System32"),
            system.clone(),
            system.join("System32/Wbem"),
            system.join("System32/WindowsPowerShell/v1.0"),
            system.join("System32/OpenSSH"),
            git.join("cmd"),
            git.join("usr/bin"),
        ]
    } else {
        UNIX_BASE.map(PathBuf::from).into()
    }
}

#[cfg(test)]
mod native_path_tests {
    use super::*;

    const PINNED_MISE_FIXTURE: &str = "DOTFILES_VERIFIED_MISE_BOOTSTRAP";
    const PINNED_MISE_VERSION: &str = "2026.10.7";
    const NATIVE_INVENTORY_SOURCE: &str = "DOTFILES_NATIVE_INVENTORY_SOURCE";
    const NATIVE_INVENTORY_PARENT: &str = "DOTFILES_NATIVE_INVENTORY_PARENT";
    const NATIVE_INVENTORY_RESULTS: &str = "DOTFILES_NATIVE_INVENTORY_RESULTS";
    const NATIVE_INVENTORY_METADATA: &str = "native-inventory.json";
    const NATIVE_INVENTORY_FAILURE: &str = "native-inventory.failure.txt";
    const NATIVE_INVENTORY_BYTES: usize = 1024 * 1024;
    const RESOLVER_TOOL: &str = "http:choose";
    const RESOLVER_PROJECT: &str = "[tools]\n\"http:choose\"={version=\"1.3.7\",url=\"https://github.com/theryangeary/choose/releases/download/v1.3.7/choose-x86_64-pc-windows-gnu.exe\",checksum=\"sha256:9233beade020c3e74854911a15aa973f7dc9f4253abe8856b8c273579a555903\"}\n[settings]\nparanoid=true\n";
    const LOCKED_SIDECARS: [(&str, &[u8]); 4] = [
        (
            ".mise/locks/yamllint/1.38.0/pyproject.toml",
            include_bytes!("../../../.mise/locks/yamllint/1.38.0/pyproject.toml"),
        ),
        (
            ".mise/locks/yamllint/1.38.0/uv.lock",
            include_bytes!("../../../.mise/locks/yamllint/1.38.0/uv.lock"),
        ),
        (
            ".mise/locks/pypi-ansible-core/2.21.4/pyproject.toml",
            include_bytes!("../../../.mise/locks/pypi-ansible-core/2.21.4/pyproject.toml"),
        ),
        (
            ".mise/locks/pypi-ansible-core/2.21.4/uv.lock",
            include_bytes!("../../../.mise/locks/pypi-ansible-core/2.21.4/uv.lock"),
        ),
    ];

    fn write_resolver_fixture(root: &Path) -> Result<()> {
        for (name, bytes) in [
            (MISE_CONFIG, include_bytes!("../../../mise.toml").as_slice()),
            (MISE_LOCK, include_bytes!("../../../mise.lock").as_slice()),
            (
                RELEASE_PINS,
                include_bytes!("../../../home/.chezmoidata/versions.toml").as_slice(),
            ),
        ]
        .into_iter()
        .chain(LOCKED_SIDECARS)
        {
            let path = root.join(name);
            fs::create_dir_all(path.parent().context("resolver fixture input parent")?)?;
            fs::write(path, bytes)?;
        }
        Ok(())
    }

    fn native_inventory_directory(variable: &str) -> Result<PathBuf> {
        let path = PathBuf::from(
            std::env::var_os(variable)
                .with_context(|| format!("provide the owned native inventory {variable}"))?,
        );
        ensure!(
            path.is_absolute(),
            "native inventory directory must be absolute"
        );
        let physical = canonical(&path)?;
        ensure!(
            path == physical
                && fs::symlink_metadata(&path)?.is_dir()
                && !fs::symlink_metadata(&path)?.file_type().is_symlink(),
            "native inventory directory must be physical and regular"
        );
        Ok(physical)
    }

    fn write_native_inventory(
        results: &Path,
        name: &str,
        bytes: &[u8],
    ) -> Result<serde_json::Value> {
        use sha2::{Digest as _, Sha256};
        use std::io::Write as _;

        ensure!(
            bytes.len() <= NATIVE_INVENTORY_BYTES,
            "native inventory output exceeds its retention bound"
        );
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(results.join(name))?
            .write_all(bytes)?;
        Ok(
            serde_json::json!({"file":name,"bytes":bytes.len(),"sha256":crate::build_inputs::hex(&Sha256::digest(bytes))}),
        )
    }

    fn retain_native_inventory(
        preparation: &CodePreparation,
        results: &Path,
    ) -> Result<serde_json::Value> {
        let scope = preparation
            .environment
            .scope
            .as_ref()
            .context("native inventory owner is missing")?;
        let mut files = Vec::new();
        for name in [NATIVE_READBACK, STAGED_READBACK] {
            match fs::read(scope.path().join(name)) {
                Ok(bytes) => {
                    files.push(write_native_inventory(results, name, &bytes)?);
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    files.push(serde_json::json!({"file":name,"status":"NOT_RUN"}));
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(serde_json::Value::Array(files))
    }

    #[test]
    #[ignore = "requires the source-authenticated pinned mise bootstrap and complete native stores"]
    fn inherited_native_inventory_preserves_all_original_variants() -> Result<()> {
        let source = native_inventory_directory(NATIVE_INVENTORY_SOURCE)?;
        let parent = native_inventory_directory(NATIVE_INVENTORY_PARENT)?;
        let results = native_inventory_directory(NATIVE_INVENTORY_RESULTS)?;
        ensure!(
            !parent.starts_with(&source) && results.starts_with(&parent) && results != parent,
            "native inventory outputs must remain inside their owned namespace outside Source"
        );
        let producer = crate::freshness::embedded();
        ensure!(
            canonical(&producer.root)? == source
                && crate::build_inputs::content_hash(&source)? == producer.hash,
            "native inventory test binary does not bind the exact current Source"
        );
        for name in [
            NATIVE_READBACK,
            STAGED_READBACK,
            NATIVE_INVENTORY_METADATA,
            NATIVE_INVENTORY_FAILURE,
        ] {
            ensure!(
                fs::symlink_metadata(results.join(name))
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
                "native inventory output already exists or cannot be inspected"
            );
        }
        let home = crate::home::home().context("native home is unavailable")?;
        let mut installed = Installations::discover(&home)?;
        installed.mise = canonical(Path::new(
            &std::env::var_os(PINNED_MISE_FIXTURE)
                .context("provide the verified pinned mise bootstrap")?,
        ))?;
        let mut preparation = CodeEnvironment::prepare_in(&source, &installed, &parent)?;
        let outcome = preparation.admit_inherited(&installed);
        let files = retain_native_inventory(&preparation, &results)?;
        let failure = outcome
            .as_ref()
            .err()
            .map(|error| {
                write_native_inventory(
                    &results,
                    NATIVE_INVENTORY_FAILURE,
                    format!("{error:#}").as_bytes(),
                )
            })
            .transpose()?;
        write_native_inventory(
            &results,
            NATIVE_INVENTORY_METADATA,
            &serde_json::to_vec_pretty(&serde_json::json!({
                "source":source,"producer_root":producer.root,"producer_hash":producer.hash,
                "mise":installed.mise,"native_data":installed.data,"files":files,
                "admitted":outcome.is_ok(),"failure":failure,
                "native_installation":"NOT_RUN","native_health":"NOT_RUN"
            }))?,
        )?;
        outcome?;
        preparation.environment.require_current()?;
        ensure!(
            crate::build_inputs::content_hash(&source)? == producer.hash,
            "native inventory operation changed Source"
        );
        Ok(())
    }

    #[test]
    #[ignore = "requires the source-authenticated pinned mise bootstrap"]
    fn private_project_preserves_locked_sidecars_and_refuses_both_input_drifts() -> Result<()> {
        let source = tempfile::tempdir()?;
        let materials = tempfile::tempdir()?;
        write_resolver_fixture(source.path())?;
        let installed = Installations {
            mise: canonical(Path::new(
                &std::env::var_os(PINNED_MISE_FIXTURE)
                    .context("provide the verified pinned mise bootstrap")?,
            ))?,
            data: materials.path().join("native-data"),
            cargo: materials.path().join("native-cargo"),
            rustup: materials.path().join("native-rustup"),
            kani: materials.path().join("native-kani"),
            cache: materials.path().join("cache"),
        };
        let mut preparation =
            CodeEnvironment::prepare_in(source.path(), &installed, materials.path())?;
        preparation.environment.set("MISE_OFFLINE", "true");
        preparation.environment.set("MISE_LOCKED", "true");
        preparation.environment.set("MISE_PARANOID", "true");
        let project = preparation
            .environment
            .configuration
            .parent()
            .context("captured project parent")?
            .to_owned();
        let mut copied = Vec::new();
        for (name, bytes) in LOCKED_SIDECARS {
            let path = project.join(name);
            copied.push((name, fs::read(&path).is_ok_and(|actual| actual == bytes)));
            fs::create_dir_all(path.parent().context("private sidecar parent")?)?;
            fs::write(path, bytes)?;
        }
        let before = preparation
            .environment
            .output(&preparation.mise, &["ls", "--local", "--json"])?;
        eprintln!(
            "inherited inventory before={}",
            String::from_utf8_lossy(&before)
        );
        let inventory: BTreeMap<String, Vec<Installation>> = serde_json::from_slice(&before)?;
        assert!(!inventory.is_empty());
        assert!(inventory.contains_key("yamllint"));
        for versions in inventory.values() {
            for tool in versions {
                assert_eq!(
                    canonical(&tool.source.path)?,
                    preparation.environment.configuration
                );
                assert_eq!(tool.version, tool.requested_version);
            }
        }
        let mut refused = Vec::new();
        for (name, bytes) in LOCKED_SIDECARS {
            for base in [source.path(), project.as_path()] {
                let path = base.join(name);
                let mut changed = bytes.to_vec();
                changed.push(b'\n');
                fs::write(&path, changed)?;
                refused.push((
                    path.clone(),
                    preparation.environment.require_current().is_err(),
                ));
                fs::write(path, bytes)?;
                preparation.environment.require_current()?;
            }
        }
        let after = preparation
            .environment
            .output(&preparation.mise, &["ls", "--local", "--json"])?;
        eprintln!(
            "inherited inventory after={}",
            String::from_utf8_lossy(&after)
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&before)?,
            serde_json::from_slice::<serde_json::Value>(&after)?
        );
        assert!(
            copied.iter().all(|(_, retained)| *retained),
            "all tracked sidecars must be captured: {copied:?}"
        );
        assert!(
            refused.iter().all(|(_, rejected)| *rejected),
            "both captured and Source drift must refuse: {refused:?}"
        );
        Ok(())
    }

    #[test]
    #[ignore = "requires the source-authenticated pinned mise bootstrap"]
    fn internal_mise_resolution_uses_the_exact_private_project() -> Result<()> {
        let mise = canonical(Path::new(
            &std::env::var_os(PINNED_MISE_FIXTURE)
                .context("provide the verified pinned mise bootstrap")?,
        ))?;
        let root = tempfile::tempdir()?;
        let scope = tempfile::tempdir()?;
        let project = scope.path().join(PRIVATE_PROJECT);
        let home = scope.path().join("home");
        let temporary = scope.path().join("tmp");
        let empty = scope.path().join("empty-config.toml");
        for directory in [&project, &home, &temporary] {
            fs::create_dir(directory)?;
        }
        fs::write(&empty, "")?;
        let mut inputs = BTreeMap::new();
        for (name, bytes) in [
            (MISE_CONFIG, RESOLVER_PROJECT.as_bytes()),
            (MISE_LOCK, b"lockfile_version=3\n".as_slice()),
        ] {
            fs::write(root.path().join(name), bytes)?;
            fs::write(project.join(name), bytes)?;
            inputs.insert(PathBuf::from(name), bytes.to_vec());
        }
        let installed = Installations {
            mise: mise.clone(),
            data: home.join("data"),
            cargo: home.join("cargo"),
            rustup: home.join("rustup"),
            kani: home.join("kani"),
            cache: home.join("cache"),
        };
        let mut environment = CodeEnvironment {
            root: canonical(root.path())?,
            configuration: canonical(&project.join(MISE_CONFIG))?,
            inputs,
            variables: BTreeMap::new(),
            docker_plugin: None,
            scope: Some(scope),
        };
        environment.seed(&installed, &home, &temporary, &empty)?;
        environment.set("PATH", std::env::join_paths(base_directories())?);
        environment.set("MISE_OFFLINE", "true");
        environment.set("MISE_PARANOID", "true");
        environment.require_current()?;
        let version = environment.output(&mise, &["--version"])?;
        assert_eq!(
            std::str::from_utf8(&version)?.split_whitespace().next(),
            Some(PINNED_MISE_VERSION)
        );
        let listing = environment.output(&mise, &["ls", "--local", "--json"])?;
        let tools: BTreeMap<String, Vec<Installation>> = serde_json::from_slice(&listing)?;
        let [tool] = tools
            .get(RESOLVER_TOOL)
            .context("the exact private project tool must be listed")?
            .as_slice()
        else {
            anyhow::bail!("the private project tool must resolve once");
        };
        assert_eq!(canonical(&tool.source.path)?, environment.configuration);
        assert_eq!(tool.source.r#type, MISE_CONFIG);
        assert_eq!(tool.requested_version, "1.3.7");
        assert_eq!(
            environment.command(&mise).get_current_dir(),
            Some(environment.root.as_path())
        );
        environment.require_current()
    }

    #[test]
    fn exact_project_copies_refuse_source_and_private_input_drift() -> Result<()> {
        let root = tempfile::tempdir()?;
        let scope = tempfile::tempdir()?;
        let project = scope.path().join(PRIVATE_PROJECT);
        fs::create_dir(&project)?;
        let mut inputs = BTreeMap::new();
        for name in PROJECT_INPUTS {
            let bytes = name.as_bytes().to_vec();
            fs::write(root.path().join(name), &bytes)?;
            fs::write(project.join(name), &bytes)?;
            inputs.insert(PathBuf::from(name), bytes);
        }
        let sidecar = Path::new(LOCK_SIDECARS).join("tool/uv.lock");
        for base in [root.path(), project.as_path()] {
            let path = base.join(&sidecar);
            fs::create_dir_all(path.parent().context("test sidecar parent")?)?;
            fs::write(path, b"captured graph")?;
        }
        inputs.insert(sidecar.clone(), b"captured graph".to_vec());
        let environment = CodeEnvironment {
            root: canonical(root.path())?,
            configuration: canonical(&project.join(MISE_CONFIG))?,
            inputs,
            variables: BTreeMap::new(),
            docker_plugin: None,
            scope: Some(scope),
        };
        environment.require_current()?;
        fs::write(root.path().join(MISE_LOCK), "changed source")?;
        assert!(environment.require_current().is_err());
        fs::write(root.path().join(MISE_LOCK), MISE_LOCK)?;
        environment.require_current()?;
        fs::write(project.join(MISE_CONFIG), "changed private copy")?;
        assert!(environment.require_current().is_err());
        fs::write(project.join(MISE_CONFIG), MISE_CONFIG)?;
        environment.require_current()?;
        for base in [root.path(), project.as_path()] {
            fs::remove_file(base.join(&sidecar))?;
            assert!(environment.require_current().is_err());
            fs::write(base.join(&sidecar), b"captured graph")?;
            environment.require_current()?;
            let added = base.join(LOCK_SIDECARS).join("unexpected.lock");
            fs::write(&added, b"uncaptured graph")?;
            assert!(environment.require_current().is_err());
            fs::remove_file(added)?;
            environment.require_current()?;
        }
        Ok(())
    }

    #[test]
    fn project_sidecar_capture_refuses_links_and_excessive_inputs() -> Result<()> {
        let root = tempfile::tempdir()?;
        let foreign = tempfile::tempdir()?;
        for name in PROJECT_INPUTS {
            fs::write(root.path().join(name), name)?;
        }
        let sidecars = root.path().join(LOCK_SIDECARS);
        fs::create_dir_all(&sidecars)?;
        let input = sidecars.join("graph.lock");
        fs::write(&input, b"captured graph")?;
        project_inputs(root.path())?;
        fs::File::create(&input)?.set_len(PROJECT_INPUT_LIMIT + 1)?;
        assert!(project_inputs(root.path()).is_err());
        fs::write(&input, b"captured graph")?;
        let link = sidecars.join("linked");
        #[cfg(unix)]
        std::os::unix::fs::symlink(foreign.path(), &link)?;
        #[cfg(windows)]
        {
            let output = crate::tool::Tool::Pwsh.command()
                .args(["-NoLogo", "-NoProfile", "-Command", "& { param([string]$Link,[string]$Target) New-Item -ItemType Junction -Path $Link -Value $Target -ErrorAction Stop | Out-Null }"])
                .arg(&link).arg(foreign.path()).output()?;
            ensure!(
                output.status.success(),
                "create the linked sidecar control: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let failure = project_inputs(root.path()).expect_err("linked graph inputs must refuse");
        assert_eq!(
            failure.to_string(),
            "project lock sidecar is a filesystem link"
        );
        #[cfg(unix)]
        fs::remove_file(link)?;
        #[cfg(windows)]
        fs::remove_dir(link)?;
        for index in 0..PROJECT_INPUT_COUNT {
            fs::write(sidecars.join(format!("member{index}.lock")), b"graph")?;
        }
        assert!(project_inputs(root.path()).is_err());
        Ok(())
    }

    fn copied_project_inputs(root: &Path, project: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
        let mut inputs = BTreeMap::new();
        for name in PROJECT_INPUTS {
            let bytes = name.as_bytes().to_vec();
            fs::write(root.join(name), &bytes)?;
            fs::write(project.join(name), &bytes)?;
            inputs.insert(PathBuf::from(name), bytes);
        }
        Ok(inputs)
    }

    #[test]
    fn docker_plugin_staging_preserves_isolation_and_refuses_changed_material() -> Result<()> {
        let root = tempfile::tempdir()?;
        let scope = tempfile::tempdir()?;
        let project = scope.path().join(PRIVATE_PROJECT);
        let home = scope.path().join("home");
        fs::create_dir_all(home.join(DOCKER_USER_DIRECTORY))?;
        fs::create_dir(&project)?;
        let inputs = copied_project_inputs(root.path(), &project)?;
        let installed = nested_installations_fixture(root.path())?;
        let installation = installed.data.join("installs/buildx/0.38.0");
        fs::create_dir_all(&installation)?;
        let source = installation.join(format!("{BUILDX_PROGRAM}{}", std::env::consts::EXE_SUFFIX));
        fs::write(&source, b"exact admitted plugin")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&source, fs::Permissions::from_mode(0o755))?;
        }
        fs::write(
            installation.join("config.json"),
            b"native credential sentinel",
        )?;
        let configuration = canonical(&project.join(MISE_CONFIG))?;
        let mut inherited = InheritedTools {
            tools: BTreeMap::from([(
                BUILDX_TOOL.into(),
                vec![Installation {
                    version: "0.38.0".into(),
                    requested_version: "0.38.0".into(),
                    install_path: canonical(&installation)?,
                    source: Source {
                        r#type: MISE_CONFIG.into(),
                        path: configuration.clone(),
                    },
                    installed: true,
                    active: true,
                    symlinked_to: None,
                }],
            )]),
            lookups: vec![InstallationLookup::capture(
                &installed.data.join("installs"),
            )?],
        };
        let mut preparation = CodePreparation {
            mise: scope.path().join("bootstrap/mise"),
            base: Vec::new(),
            links: None,
            inherited: None,
            environment: CodeEnvironment {
                scope: Some(scope),
                root: canonical(root.path())?,
                configuration,
                inputs,
                variables: BTreeMap::new(),
                docker_plugin: None,
            },
        };
        inherited.tools.get_mut(BUILDX_TOOL).unwrap()[0].requested_version = "wrong".into();
        assert!(
            preparation
                .stage_docker_plugin(&inherited, std::slice::from_ref(&installation))
                .is_err()
        );
        inherited.tools.get_mut(BUILDX_TOOL).unwrap()[0].requested_version = "0.38.0".into();
        assert!(
            preparation
                .stage_docker_plugin(&inherited, &[root.path().to_owned()])
                .is_err()
        );
        preparation.stage_docker_plugin(&inherited, &[installation])?;
        let docker = home.join(DOCKER_USER_DIRECTORY);
        let plugin = canonical(
            &docker
                .join(DOCKER_PLUGIN_DIRECTORY)
                .join(format!("{BUILDX_PLUGIN}{}", std::env::consts::EXE_SUFFIX)),
        )?;
        assert_eq!(fs::read(&plugin)?, b"exact admitted plugin");
        assert!(preparation.immutable_paths().contains(&plugin));
        assert!(!preparation.immutable_paths().contains(&docker));
        assert!(!docker.join("config.json").exists());
        let state = docker.join(BUILDX_STATE_DIRECTORY);
        assert!(state.is_dir() && !preparation.immutable_paths().contains(&state));
        fs::write(state.join("state"), b"writable plugin state")?;
        preparation.environment.require_current()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(fs::metadata(&plugin)?.permissions().mode() & 0o777, 0o755);
        }
        fs::write(&plugin, b"modified owned plugin")?;
        assert!(preparation.environment.require_current().is_err());
        assert_eq!(fs::read(&source)?, b"exact admitted plugin");
        fs::write(&plugin, b"exact admitted plugin")?;
        let environment = ResolvedCode {
            environment: &mut preparation.environment,
        }
        .into_environment()?;
        environment.require_current()?;
        fs::write(&plugin, b"changed after ownership transfer")?;
        assert!(environment.require_current().is_err());
        Ok(())
    }

    #[test]
    fn native_health_and_code_share_the_existing_cargo_target() -> Result<()> {
        let root = tempfile::tempdir()?;
        let scope = tempfile::tempdir()?;
        let project = scope.path().join(PRIVATE_PROJECT);
        let target = scope.path().join("target");
        fs::create_dir(&project)?;
        fs::create_dir(&target)?;
        let mut inputs = BTreeMap::new();
        for name in PROJECT_INPUTS {
            let bytes = name.as_bytes().to_vec();
            fs::write(root.path().join(name), &bytes)?;
            fs::write(project.join(name), &bytes)?;
            inputs.insert(PathBuf::from(name), bytes);
        }
        let preparation = CodePreparation {
            mise: scope.path().join("mise"),
            base: Vec::new(),
            links: Some(RustKaniLinks {
                data: scope.path().join("data"),
                rust: scope.path().join("fresh-rust"),
                launcher: None,
            }),
            inherited: None,
            environment: CodeEnvironment {
                docker_plugin: None,
                root: canonical(root.path())?,
                configuration: canonical(&project.join(MISE_CONFIG))?,
                inputs,
                variables: BTreeMap::from([(
                    "CARGO_TARGET_DIR".into(),
                    target.clone().into_os_string(),
                )]),
                scope: Some(scope),
            },
        };
        let code = preparation.environment.command("fixture-cargo");
        assert_eq!(
            code.get_envs()
                .find(|(key, _)| *key == "CARGO_TARGET_DIR")
                .and_then(|(_, value)| value),
            Some(target.as_os_str())
        );
        let mut health = crate::process::command("fixture-cargo");
        health.env_remove("CARGO_TARGET_DIR");
        preparation.configure_health(&mut health)?;
        assert_eq!(
            health
                .get_envs()
                .find(|(key, _)| *key == "CARGO_TARGET_DIR")
                .and_then(|(_, value)| value),
            Some(target.as_os_str())
        );
        assert_eq!(
            health.get_current_dir(),
            Some(preparation.environment.root.as_path())
        );
        Ok(())
    }

    fn nested_installations_fixture(scope: &Path) -> Result<Installations> {
        let data = scope.join("local-data");
        fs::create_dir_all(data.join("installs"))?;
        Ok(Installations {
            mise: scope.join("mise"),
            data,
            cargo: scope.join("cargo"),
            rustup: scope.join("rustup"),
            kani: scope.join("kani"),
            cache: scope.join("cache"),
        })
    }

    #[test]
    fn nested_installation_lookups_bind_the_current_data_origin_and_canonical_roots() -> Result<()>
    {
        let scope = tempfile::tempdir()?;
        let installed = nested_installations_fixture(scope.path())?;
        let shared = scope.path().join("shared-data/installs");
        fs::create_dir_all(&shared)?;
        let paths = std::env::join_paths([&shared, &shared])?;
        let lookups = installed.shared_lookups(Some(installed.data.as_os_str()), Some(&paths))?;
        assert_eq!(lookups.len(), 1);
        assert_eq!(lookups[0].installs, canonical(&shared)?);
        assert_eq!(lookups[0].storage, canonical(shared.parent().unwrap())?);
        assert_eq!(
            installed.shared_lookups(None, Some(&paths))?,
            Vec::<InstallationLookup>::new()
        );
        assert_eq!(
            installed.shared_lookups(Some(scope.path().as_os_str()), Some(&paths))?,
            Vec::<InstallationLookup>::new()
        );
        assert_eq!(
            installed.shared_lookups(Some(installed.data.as_os_str()), None)?,
            Vec::<InstallationLookup>::new()
        );
        let file_lookup = scope.path().join("file-data/installs");
        fs::create_dir_all(file_lookup.parent().unwrap())?;
        fs::write(&file_lookup, b"not a directory")?;
        for path in [
            PathBuf::from("relative/installs"),
            scope.path().to_path_buf(),
            scope.path().join("missing/installs"),
            file_lookup,
        ] {
            let paths = std::env::join_paths([path])?;
            assert!(
                installed
                    .shared_lookups(Some(installed.data.as_os_str()), Some(&paths))
                    .is_err()
            );
        }
        Ok(())
    }

    #[test]
    fn nested_installation_inventory_preserves_shared_endpoints_and_all_pin_checks() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let installed = nested_installations_fixture(scope.path())?;
        let shared = scope.path().join("shared-data/installs");
        let endpoint = shared.join("python/3.14.8");
        let changed = shared.join("python/3.14.8-other-variant");
        let unrelated = scope.path().join("unrelated/python/3.14.8");
        for path in [&endpoint, &changed, &unrelated] {
            fs::create_dir_all(path)?;
        }
        let configuration = scope.path().join(MISE_CONFIG);
        fs::write(&configuration, "project")?;
        let configuration = canonical(&configuration)?;
        let tools = BTreeMap::from([(
            "python".into(),
            vec![Installation {
                version: "3.14.8".into(),
                requested_version: "3.14.8".into(),
                install_path: canonical(&endpoint)?,
                source: Source {
                    r#type: MISE_CONFIG.into(),
                    path: configuration.clone(),
                },
                installed: true,
                active: true,
                symlinked_to: None,
            }],
        )]);
        let paths = std::env::join_paths([&shared])?;
        let mut lookups = vec![InstallationLookup::capture(
            &installed.data.join("installs"),
        )?];
        lookups.extend(installed.shared_lookups(Some(installed.data.as_os_str()), Some(&paths))?);
        let mut inherited = InheritedTools {
            tools: tools.clone(),
            lookups,
        };
        inherited.require_matching(&tools, &configuration)?;
        let shared_lookup = inherited.lookups.pop().unwrap();
        assert!(inherited.require_native_storage().is_err());
        inherited.lookups.push(shared_lookup);
        inherited.require_native_storage()?;
        for tool in [
            Installation {
                installed: false,
                ..tools["python"][0].clone()
            },
            Installation {
                active: false,
                ..tools["python"][0].clone()
            },
            Installation {
                version: "changed".into(),
                ..tools["python"][0].clone()
            },
            Installation {
                install_path: canonical(&changed)?,
                ..tools["python"][0].clone()
            },
            Installation {
                install_path: canonical(&unrelated)?,
                ..tools["python"][0].clone()
            },
            Installation {
                source: Source {
                    r#type: "global".into(),
                    path: configuration.clone(),
                },
                ..tools["python"][0].clone()
            },
        ] {
            let changed = BTreeMap::from([("python".into(), vec![tool])]);
            assert!(
                inherited
                    .require_matching(&changed, &configuration)
                    .is_err()
            );
        }
        inherited.lookups[1].storage = canonical(&installed.data)?;
        assert!(inherited.require_native_storage().is_err());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn nested_installation_lookups_refuse_redirected_storage_roots() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let installed = nested_installations_fixture(scope.path())?;
        let storage = scope.path().join("shared-data");
        let unrelated = scope.path().join("unrelated");
        fs::create_dir(&storage)?;
        fs::create_dir(&unrelated)?;
        let shared = storage.join("installs");
        std::os::unix::fs::symlink(&unrelated, &shared)?;
        let paths = std::env::join_paths([&shared])?;
        assert!(
            installed
                .shared_lookups(Some(installed.data.as_os_str()), Some(&paths))
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn original_resolver_refuses_missing_uninstalled_and_unrelated_fresh_stores() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let configuration = scope.path().join(MISE_CONFIG);
        fs::write(&configuration, "project")?;
        let configuration = canonical(&configuration)?;
        let data = scope.path().join("data");
        let selected = data.join("installs/kani/verifier");
        let other = scope.path().join("other");
        fs::create_dir_all(&selected)?;
        fs::create_dir(&other)?;
        let tool = Installation {
            version: "0.68.0".into(),
            requested_version: "0.68.0".into(),
            install_path: selected.clone(),
            source: Source {
                r#type: MISE_CONFIG.into(),
                path: configuration.clone(),
            },
            installed: true,
            active: true,
            symlinked_to: None,
        };
        let mut tools = BTreeMap::from([(KANI_TOOL.to_owned(), vec![tool])]);
        assert_eq!(
            checked_store(&tools, KANI_TOOL, &configuration, &data, &selected)?,
            selected
        );
        assert!(checked_store(&tools, KANI_TOOL, &configuration, &data, &other).is_err());
        assert!(
            checked_store(
                &tools,
                KANI_TOOL,
                &configuration,
                &data,
                &scope.path().join("missing")
            )
            .is_err()
        );
        let tool = tools
            .get_mut(KANI_TOOL)
            .context("test resolver tool")?
            .first_mut()
            .context("test resolver version")?;
        tool.installed = false;
        assert!(checked_store(&tools, KANI_TOOL, &configuration, &data, &selected).is_err());
        Ok(())
    }

    fn response(stdout: &str, stderr: &str, status: i32) -> std::process::Output {
        #[cfg(windows)]
        let status = {
            use std::os::windows::process::ExitStatusExt as _;
            std::process::ExitStatus::from_raw(status.unsigned_abs())
        };
        #[cfg(unix)]
        let status = {
            use std::os::unix::process::ExitStatusExt as _;
            std::process::ExitStatus::from_raw(status)
        };
        std::process::Output {
            status,
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[test]
    fn native_response_retains_only_the_platform_path() -> Result<()> {
        let path = NativePathResponse::read(&response(
            r#"{"PATH":"native","private":{"token":[1,2,null]}}"#,
            "",
            0,
        ))?;
        assert_eq!(path.as_str(), "native");
        let mixed = response(r#"{"Path":"native"}"#, "", 0);
        assert_eq!(NativePathResponse::read(&mixed).is_ok(), cfg!(windows));
        #[cfg(unix)]
        {
            let path = NativePathResponse::read(&response(
                r#"{"Path":"ignored","PATH":"native"}"#,
                "",
                0,
            ))?;
            assert_eq!(path.as_str(), "native");
        }
        Ok(())
    }

    #[test]
    fn native_response_refuses_missing_nonstring_and_duplicate_paths() {
        for body in [
            r#"{"private":"private-response-sentinel"}"#,
            r#"{"PATH":null}"#,
            r#"{"PATH":3}"#,
            r#"{"PATH":["private-response-sentinel"]}"#,
            r#"{"PATH":"first","PATH":"second"}"#,
            "[\"private-response-sentinel\"]",
            "private-response-sentinel",
        ] {
            let error = NativePathResponse::read(&response(body, "", 0))
                .expect_err("an invalid native PATH response must be refused");
            let diagnostic = error.to_string();
            assert!(diagnostic.starts_with("native PATH response is invalid ("));
            assert!(!diagnostic.contains("private-response-sentinel"));
        }
        let ambiguous = response(r#"{"PATH":"first","Path":"second"}"#, "", 0);
        assert_eq!(NativePathResponse::read(&ambiguous).is_err(), cfg!(windows));
    }

    #[test]
    fn native_response_refuses_failure_or_diagnostics_without_payloads() {
        for (stderr, status, expected) in [
            ("private-diagnostic-sentinel", 0, "produced diagnostics"),
            ("", 1, "failed with"),
        ] {
            let error = NativePathResponse::read(&response(
                r#"{"PATH":"private-response-sentinel"}"#,
                stderr,
                status,
            ))
            .expect_err("only a silent successful query is authoritative");
            let diagnostic = error.to_string();
            assert!(diagnostic.contains(expected));
            assert!(!diagnostic.contains("private-response-sentinel"));
            assert!(!diagnostic.contains("private-diagnostic-sentinel"));
        }
    }
}

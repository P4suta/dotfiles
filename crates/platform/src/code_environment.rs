//! Code gates build their environment from resolved project installations.

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::paths::canonical;

const UNIX_BASE: [&str; 4] = ["/usr/bin", "/bin", "/usr/sbin", "/sbin"];
const WINDOWS_GIT: &str = "Git";
const MISE_CONFIG: &str = "mise.toml";
const CARGO_BIN: &str = "bin";
const RELEASE_PINS: &str = "home/.chezmoidata/versions.toml";

/// Installed tool storage without host configuration or credentials.
#[derive(Debug)]
pub struct Installations {
    pub mise: PathBuf,
    pub data: PathBuf,
    pub cargo: PathBuf,
    pub rustup: PathBuf,
}

impl Installations {
    /// Locate the existing mise executable and its storage without running an installer.
    ///
    /// # Errors
    ///
    /// Returns an error when the managed tool path contains no mise executable.
    pub fn discover(home: &Path) -> Result<Self> {
        let path = crate::home::managed_path(home)?;
        let mise = std::env::split_paths(&path)
            .map(|directory| directory.join(format!("mise{}", std::env::consts::EXE_SUFFIX)))
            .find(|path| path.is_file())
            .context("mise is unavailable; apply the host profile")?;
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
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    r#type: String,
    path: PathBuf,
}

#[derive(Debug, Deserialize)]
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

/// Owns the scratch home for as long as a gate or one of its child processes runs.
#[derive(Debug)]
pub struct CodeEnvironment {
    scope: tempfile::TempDir,
    root: PathBuf,
    variables: BTreeMap<OsString, OsString>,
}

impl CodeEnvironment {
    /// Resolve only this checkout's pins in a clean environment, then build its gate environment.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing or inactive installation, a mismatched version or source,
    /// a nonabsolute tool directory, or an unsuccessful resolution command.
    pub fn build(root: &Path, installed: &Installations) -> Result<Self> {
        let scope = tempfile::tempdir()?;
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
        fs::copy(&installed.mise, &mise)?;
        let mut base = base_directories();
        let mut environment = Self {
            scope,
            root,
            variables: BTreeMap::new(),
        };
        environment.seed(installed, &home, &temporary, &empty)?;
        base.insert(0, bootstrap);
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
        let mut directories = environment.resolve(&mise, installed)?;
        directories.extend(base);
        environment.set("PATH", std::env::join_paths(directories)?);
        environment.set("MISE_CARGO_HOME", home.join(".cargo"));
        Ok(environment)
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
            ("CARGO_HOME", home.join(".cargo")),
            ("RUSTUP_HOME", installed.rustup.clone()),
            ("MISE_RUSTUP_HOME", installed.rustup.clone()),
            ("MISE_CARGO_HOME", installed.cargo.clone()),
            ("MISE_DATA_DIR", installed.data.clone()),
            ("MISE_CONFIG_DIR", home.join(".config/mise")),
            ("MISE_CACHE_DIR", home.join(".cache/mise")),
            ("MISE_STATE_DIR", home.join(".local/state/mise")),
            ("MISE_GLOBAL_CONFIG_FILE", empty.to_path_buf()),
            ("MISE_SYSTEM_CONFIG_FILE", empty.to_path_buf()),
            ("MISE_TRUSTED_CONFIG_PATHS", self.root.clone()),
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
            ("DOTFILES_ONEPASSWORD_APP", home.join("absent-app")),
            ("CARGO_TARGET_DIR", self.root.join("target/hosts")),
        ] {
            self.set(name, value.as_os_str());
        }
        for (name, value) in [
            ("GIT_CONFIG_NOSYSTEM", "1"),
            ("GIT_TERMINAL_PROMPT", "0"),
            ("MISE_AUTO_INSTALL", "false"),
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
            self.set("ComSpec", system.join("System32/cmd.exe"));
            self.set("PATHEXT", ".COM;.EXE;.BAT;.CMD");
            self.set("APPDATA", home.join("AppData/Roaming"));
            self.set("LOCALAPPDATA", home.join("AppData/Local"));
            self.set("ProgramData", home.join("ProgramData"));
            self.set("ALLUSERSPROFILE", home.join("ProgramData"));
            self.set("ProgramFiles", program_files());
        }
        Ok(())
    }

    fn resolve(&mut self, mise: &Path, installed: &Installations) -> Result<Vec<PathBuf>> {
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
                    tool.active
                        && tool.source.r#type == MISE_CONFIG
                        && canonical(&tool.source.path)? == self.root.join(MISE_CONFIG)
                        && tool.version == tool.requested_version,
                    "{name} does not resolve to its exact active project pin"
                );
            }
        }
        if tools.values().flatten().any(|tool| !tool.installed) {
            self.install_missing(mise, installed, &tools)?;
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
                            && canonical(&tool.source.path)? == self.root.join(MISE_CONFIG),
                        tool.version == tool.requested_version,
                    ]),
                    "{name} does not resolve to its installed project pin"
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
        let compiler = find_msvc_tools::find_tool_with_env(std::env::consts::ARCH, "cl.exe", self)
            .context("the Windows code stage requires an installed MSVC toolchain and SDK")?;
        ensure!(
            compiler.path().is_absolute(),
            "MSVC returned a relative compiler"
        );
        let mut directories = vec![canonical(
            compiler
                .path()
                .parent()
                .context("MSVC compiler directory")?,
        )?];
        for (name, value) in compiler.env() {
            match name.to_str() {
                Some("PATH") => {
                    for directory in std::env::split_paths(value) {
                        ensure!(
                            directory.is_absolute(),
                            "MSVC returned a relative tool directory"
                        );
                        let directory = canonical(&directory)?;
                        if !directories.contains(&directory) {
                            directories.push(directory);
                        }
                    }
                }
                Some(name @ ("LIB" | "LIBPATH" | "INCLUDE")) => self.set(name, value),
                _ => anyhow::bail!(
                    "MSVC returned an unexpected environment key: {}",
                    name.display()
                ),
            }
        }
        Ok(directories)
    }

    fn install_missing(
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
        let mut requests = Vec::new();
        for (name, versions) in tools {
            for tool in versions {
                if tool.installed {
                    let relative = tool
                        .install_path
                        .strip_prefix(installed.data.join("installs"))
                        .with_context(|| {
                            format!("{name} installation belongs outside the tool store")
                        })?;
                    let link = data.join("installs").join(relative);
                    fs::create_dir_all(link.parent().context("installation link has no parent")?)?;
                    link_directory(&tool.install_path, &link)?;
                } else {
                    requests.push(format!("{name}@{}", tool.version));
                }
            }
        }
        let mut bootstrap = Vec::new();
        if let Some(rust) = tools.get("rust").and_then(|versions| versions.first()) {
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
        let arguments: Vec<_> = ["install", "--yes"]
            .into_iter()
            .chain(requests.iter().map(String::as_str))
            .collect();
        self.output(mise, &arguments)?;
        Ok(())
    }

    fn set(&mut self, name: &str, value: impl AsRef<OsStr>) {
        self.variables
            .insert(name.into(), value.as_ref().to_owned());
    }

    /// A gate command and all its descendants inherit this environment.
    #[must_use]
    pub fn command(&self, program: impl AsRef<OsStr>) -> Command {
        let mut command = crate::process::isolated_command(program, &self.variables);
        command.current_dir(&self.root);
        command
    }

    fn output(&self, program: &Path, arguments: &[&str]) -> Result<Vec<u8>> {
        let output = self.command(program).args(arguments).output()?;
        ensure!(
            output.status.success(),
            "{} {arguments:?}: {}",
            program.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(output.stdout)
    }
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

fn link_directory(source: &Path, destination: &Path) -> Result<()> {
    #[cfg(unix)]
    std::os::unix::fs::symlink(source, destination)?;
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(source, destination)?;
    Ok(())
}

impl Drop for CodeEnvironment {
    fn drop(&mut self) {
        crate::cleanup::report(
            fs::remove_dir_all(self.scope.path()),
            "remove the code gate environment",
        );
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

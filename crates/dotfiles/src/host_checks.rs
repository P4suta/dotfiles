//! Candidate checks provision scratch files and inspect the applied host profile.

use anyhow::{Context, Result, ensure};
use platform::checks::Checks;
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const CONFIG: &str = ".config/chezmoi/chezmoi.toml";
const GIT_CONFIG: &str = ".config/git/config";
const MISE_CONFIG: &str = ".config/mise/config.toml";
const MISE_DATA: &str = ".local/share/mise";
const GH_CONFIG: &str = ".config/gh";
const GH_DIRECTORY: &str = "gh";
const WINDOWS_GH_DIRECTORY: &str = "GitHub CLI";
const GH_LOGIN_FILES: [&str; 2] = ["hosts.yml", "config.yml"];
const CHEZMOI_INSTALLS: &str = "installs/chezmoi";
const PROJECT_CONFIG: &str = "mise.toml";
const EMPTY_CONFIG: &str = "empty-config.toml";
const RUNTIME_BIN: &str = ".local/bin";
const RUNTIME_BUILD: &str = "runtime";
const GO_PATH: &str = "go";
const GO_MODULE_CACHE: &str = "go/pkg/mod";
const GO_BUILD_CACHE: &str = ".cache/go-build";
const GO_ENVIRONMENT_PREFIX: &str = "GO";
#[cfg(unix)]
const PRIVATE_SCOPE_MODE: u32 = 0o700;
#[cfg(unix)]
const DIRECTORY_FLAGS: rustix::fs::OFlags = rustix::fs::OFlags::RDONLY
    .union(rustix::fs::OFlags::DIRECTORY)
    .union(rustix::fs::OFlags::NOFOLLOW)
    .union(rustix::fs::OFlags::CLOEXEC);
const DEBUG_OUTPUT: &str = "debug";
const MAC_SHELL: &str = "/bin/zsh";
const UNIX_SHELL: &str = "/bin/bash";
const PUBLIC_SOURCES: [&str; 3] = [
    "https://github.com/eza-community/eza.git",
    "https://github.com/RustSec/advisory-db.git",
    "https://github.com/embarkstudios/std-replacement-data.git",
];
const UNATTENDED_SSH: &str = "ssh -o BatchMode=yes -o IdentityAgent=none -o PasswordAuthentication=no -o KbdInteractiveAuthentication=no";

/// Read-only observations of the installed profile.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Applied {
    pub drifted_targets: Vec<String>,
    pub verification_error: Option<String>,
}

impl Applied {
    pub fn report(&self, prefix: &str) {
        for target in &self.drifted_targets {
            println!("{prefix} drift: {target}");
        }
        if let Some(error) = &self.verification_error {
            println!("{prefix} verification error: {error}");
        } else if self.drifted_targets.is_empty() {
            println!("{prefix}: matched");
        }
    }
}

#[derive(Debug)]
pub struct Report {
    pub candidate: Result<()>,
    pub applied: Applied,
}

#[must_use]
pub fn unattended(program: impl AsRef<OsStr>) -> Command {
    let mut command = platform::process::command(program);
    command
        .env_remove("SSH_AUTH_SOCK")
        .env_remove("SSH_AGENT_PID")
        .env("GIT_SSH_COMMAND", UNATTENDED_SSH)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .stdin(Stdio::null());
    command
}

/// Owned destinations for candidate files and every installer write.
#[derive(Debug)]
pub struct Scratch {
    scope: tempfile::TempDir,
    #[cfg(unix)]
    directory: fs::File,
    home: PathBuf,
    config: PathBuf,
    download_cache: Option<PathBuf>,
    native_path: platform::code_environment::NativePath,
}

impl Scratch {
    /// Copy the host's answers while relocating its paths into an owned destination.
    ///
    /// # Errors
    ///
    /// Returns an error for missing or malformed configuration or unavailable scratch storage.
    pub fn new(config: &Path) -> Result<Self> {
        let native_path = platform::code_environment::NativePath::capture(
            &platform::home::home().context("native home is unavailable")?,
        )?;
        #[cfg(unix)]
        let scope = {
            use std::os::unix::fs::PermissionsExt as _;
            tempfile::Builder::new()
                .permissions(fs::Permissions::from_mode(PRIVATE_SCOPE_MODE))
                .tempdir()?
        };
        #[cfg(not(unix))]
        let scope = tempfile::tempdir()?;
        let directory = platform::paths::canonical(scope.path())?;
        #[cfg(unix)]
        let descriptor = fs::File::from(rustix::fs::open(
            &directory,
            DIRECTORY_FLAGS,
            rustix::fs::Mode::empty(),
        )?);
        let home = directory.join("home");
        fs::create_dir_all(&home)?;
        fs::write(directory.join(EMPTY_CONFIG), "")?;
        let mut answers: toml::Table = toml::from_str(&fs::read_to_string(config)?)?;
        let data = answers
            .get_mut("data")
            .context("host configuration requires data")?;
        crate::profiles::configured_profile(&serde_json::to_value(&data)?)?;
        if let Some(paths) = data.get_mut("paths").and_then(toml::Value::as_table_mut) {
            paths.insert(
                "projects".into(),
                toml::Value::String(home.join("projects").to_string_lossy().into_owned()),
            );
        }
        let config = directory.join("chezmoi.toml");
        fs::write(&config, toml::to_string(&answers)?)?;
        Ok(Self {
            scope,
            #[cfg(unix)]
            directory: descriptor,
            home,
            config,
            download_cache: None,
            native_path,
        })
    }

    /// Keep downloads on this host while configurations and installations stay in scratch storage.
    #[must_use]
    pub fn cache_downloads(mut self, native_home: &Path) -> Self {
        self.download_cache = Some(
            native_home
                .join(platform::code_environment::HOST_CACHE)
                .join("real-mise"),
        );
        self
    }

    fn github(&self, source: &Path) -> Result<()> {
        let destination = self.home.join(GH_CONFIG);
        fs::create_dir_all(&destination)?;
        for name in GH_LOGIN_FILES {
            match fs::copy(source.join(name), destination.join(name)) {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error).context("snapshot GitHub CLI login"),
            }
        }
        Ok(())
    }

    /// Keep native executable resolution while redirecting writable user state.
    #[must_use]
    pub fn command(&self, program: impl AsRef<OsStr>) -> Command {
        let mut command = unattended(program);
        command.env("PATH", self.native_path.as_os_str());
        for (name, _) in std::env::vars_os() {
            let text = name.to_string_lossy();
            if text.starts_with("MISE_")
                || text.starts_with("__MISE_")
                || text.starts_with("CARGO_")
                || text.starts_with("RUSTUP_")
                || text
                    .get(..GO_ENVIRONMENT_PREFIX.len())
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case(GO_ENVIRONMENT_PREFIX))
                || text.starts_with("GIT_CONFIG_")
                || platform::tool::GIT_LOCAL_ENVIRONMENT.contains(&text.as_ref())
            {
                command.env_remove(&name);
            }
        }
        for name in [
            "GH_TOKEN",
            "GITHUB_TOKEN",
            "GITHUB_API_TOKEN",
            "GH_ENTERPRISE_TOKEN",
            "GITHUB_ENTERPRISE_TOKEN",
            "GIT_ASKPASS",
            "SSH_ASKPASS",
        ] {
            command.env_remove(name);
        }
        for (name, value) in [
            ("HOME", self.home.clone()),
            ("USERPROFILE", self.home.clone()),
            ("XDG_CONFIG_HOME", self.home.join(".config")),
            ("XDG_CACHE_HOME", self.home.join(".cache")),
            ("XDG_DATA_HOME", self.home.join(".local/share")),
            ("XDG_STATE_HOME", self.home.join(".local/state")),
            ("APPDATA", self.home.join("AppData/Roaming")),
            ("LOCALAPPDATA", self.home.join("AppData/Local")),
            ("ProgramData", self.home.join("ProgramData")),
            ("ALLUSERSPROFILE", self.home.join("ProgramData")),
            ("CARGO_HOME", self.home.join(".cargo")),
            ("RUSTUP_HOME", self.home.join(".rustup")),
            ("CARGO_TARGET_DIR", self.home.join("target")),
            ("GOPATH", self.home.join(GO_PATH)),
            ("GOMODCACHE", self.home.join(GO_MODULE_CACHE)),
            ("GOCACHE", self.home.join(GO_BUILD_CACHE)),
            ("GOBIN", self.home.join(RUNTIME_BIN)),
            ("GOTMPDIR", self.scope.path().to_path_buf()),
            ("MISE_DATA_DIR", self.home.join(MISE_DATA)),
            ("MISE_CONFIG_DIR", self.home.join(".config/mise")),
            ("MISE_GLOBAL_CONFIG_FILE", self.home.join(MISE_CONFIG)),
            (
                "MISE_SYSTEM_CONFIG_FILE",
                self.scope.path().join(EMPTY_CONFIG),
            ),
            ("MISE_CACHE_DIR", self.home.join(".cache/mise")),
            ("MISE_STATE_DIR", self.home.join(".local/state/mise")),
            ("GIT_CONFIG_GLOBAL", self.home.join(GIT_CONFIG)),
            ("GH_CONFIG_DIR", self.home.join(GH_CONFIG)),
            ("SKILL_OPS_STATE", self.home.join(".local/state/skill-ops")),
            ("TMPDIR", self.scope.path().to_path_buf()),
            ("TMP", self.scope.path().to_path_buf()),
            ("TEMP", self.scope.path().to_path_buf()),
        ] {
            command.env(name, value);
        }
        command.env("GOENV", "off").env("GOWORK", "off");
        if let Some(cache) = &self.download_cache {
            command
                .env("MISE_CACHE_DIR", cache)
                .env("MISE_DOWNLOADS_DIR", cache.join("downloads"))
                .env("MISE_ALWAYS_KEEP_DOWNLOAD", "true");
        }
        command.current_dir(&self.home);
        command
    }

    fn configure(&self, command: &mut Command) {
        let environment = self.command(command.get_program());
        for (name, value) in environment.get_envs() {
            if let Some(value) = value {
                command.env(name, value);
            } else {
                command.env_remove(name);
            }
        }
    }

    /// Select the project's exact renderer, installing a missing pin only in scratch storage.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid pin, failed installation, or mismatched executable.
    pub fn renderer(
        &self,
        root: &Path,
        installed: &platform::code_environment::Installations,
    ) -> Result<PathBuf> {
        let table: toml::Table = toml::from_str(&fs::read_to_string(root.join(PROJECT_CONFIG))?)?;
        let tool = table
            .get("tools")
            .and_then(|tools| tools.get("chezmoi"))
            .context("mise.toml pins no chezmoi version")?;
        let pin = tool
            .as_str()
            .or_else(|| tool.get("version").and_then(toml::Value::as_str))
            .context("chezmoi requires an exact version string")?;
        ensure!(
            Path::new(pin)
                .components()
                .eq(std::iter::once(std::path::Component::Normal(OsStr::new(
                    pin
                )))),
            "chezmoi pin must name one installation directory"
        );
        let relative = Path::new(CHEZMOI_INSTALLS)
            .join(pin)
            .join(format!("chezmoi{}", std::env::consts::EXE_SUFFIX));
        let mut renderer = installed.data.join(&relative);
        if !renderer.is_file() {
            let mut command = self.command(&installed.mise);
            command
                .env(
                    "MISE_GLOBAL_CONFIG_FILE",
                    self.scope.path().join(EMPTY_CONFIG),
                )
                .env("MISE_TRUSTED_CONFIG_PATHS", &self.home)
                .env("MISE_CEILING_PATHS", self.scope.path())
                .args(["install", "--yes", &format!("chezmoi@{pin}")]);
            let status = platform::process::mise_install(command)?.status()?;
            ensure!(status.success(), "install chezmoi pin {pin}: {status}");
            renderer = self.home.join(MISE_DATA).join(relative);
        }
        let renderer = platform::paths::canonical(&renderer)?;
        let version = self.command(&renderer).arg("--version").output()?;
        ensure!(
            version.status.success()
                && std::str::from_utf8(&version.stdout)?
                    .starts_with(&format!("chezmoi version v{pin},")),
            "the renderer does not match chezmoi pin {pin}: {}{}",
            String::from_utf8_lossy(&version.stdout),
            String::from_utf8_lossy(&version.stderr)
        );
        Ok(renderer)
    }

    fn chezmoi(&self, root: &Path, renderer: &Path) -> Command {
        let configured =
            crate::profiles::chezmoi(root, self.scope.path(), &self.config, &self.home);
        let mut command = self.command(renderer);
        command.args(configured.get_args());
        command
    }

    /// Install this worker and build the Git guard its candidate hooks and launcher require.
    ///
    /// # Errors
    ///
    /// Returns an error when building the pinned guard or installing a runtime entry point fails.
    pub fn provision_runtime(&self, root: &Path, binary: &Path, native_home: &Path) -> Result<()> {
        let directory = self.home.join(RUNTIME_BIN);
        fs::create_dir_all(&directory)?;
        platform::code_environment::link_executable(
            binary,
            &directory.join(format!("dotfiles{}", std::env::consts::EXE_SUFFIX)),
        )?;
        let guard = platform::tool::Tool::Dotguard.program();
        let environment = crate::hosts::code_environment(root, native_home)?;
        let build = native_home
            .join(platform::code_environment::HOST_CACHE)
            .join(RUNTIME_BUILD);
        let status = environment
            .command(platform::tool::Tool::Cargo.program())
            .args([
                "build",
                "--locked",
                "--package",
                guard,
                "--bin",
                guard,
                "--target-dir",
            ])
            .arg(&build)
            .status()?;
        ensure!(status.success(), "build candidate Git guard: {status}");
        let executable = format!("{guard}{}", std::env::consts::EXE_SUFFIX);
        platform::code_environment::link_executable(
            &build.join(DEBUG_OUTPUT).join(&executable),
            &directory.join(executable),
        )?;
        if cfg!(windows) {
            crate::setup::git_wrapper(&directory)?;
        }
        Ok(())
    }

    /// Run the source's checksum-verified mise launcher in the owned home.
    ///
    /// # Errors
    ///
    /// Returns an error when rendering or running the pinned launcher fails.
    pub fn provision_mise(&self, root: &Path, renderer: &Path) -> Result<PathBuf> {
        const PARTIALS: &str = "home/.chezmoitemplates/partials";
        let (partial, script, program, arguments): (_, _, _, &[&str]) = if cfg!(windows) {
            (
                "install-mise-ps1",
                "install-mise.ps1",
                platform::tool::Tool::Pwsh,
                &["-NoProfile", "-File"],
            )
        } else {
            (
                "install-mise-sh",
                "install-mise.sh",
                platform::tool::Tool::Sh,
                &[],
            )
        };
        let output = self
            .chezmoi(root, renderer)
            .args(["execute-template", "--file"])
            .arg(root.join(PARTIALS).join(partial))
            .output()?;
        ensure!(
            output.status.success(),
            "mise launcher render failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let script = self.scope.path().join(script);
        fs::write(&script, output.stdout)?;
        let status = self
            .command(program.program())
            .args(arguments)
            .arg(&script)
            .status()?;
        ensure!(status.success(), "mise launcher failed with {status}");
        Ok(self
            .home
            .join(RUNTIME_BIN)
            .join(format!("mise{}", std::env::consts::EXE_SUFFIX)))
    }

    fn profile(&self, root: &Path, action: &str, renderer: &Path) -> Result<()> {
        let status = self
            .chezmoi(root, renderer)
            .args([action, "--exclude", "scripts,externals"])
            .status()?;
        ensure!(
            status.success(),
            "candidate profile {action} failed with {status}"
        );
        Ok(())
    }

    fn install(&self, directory: &Path) -> Result<()> {
        let invocation = if cfg!(windows) {
            "& (Join-Path $env:USERPROFILE '.local/bin/dotfiles.exe') mise-install -y"
        } else {
            "\"$HOME/.local/bin/dotfiles\" mise-install -y"
        };
        let command = self.toolchain_command(directory, invocation)?;
        let status = platform::process::mise_install(command)?.status()?;
        ensure!(status.success(), "mise install failed with {status}");
        Ok(())
    }

    fn toolchain_command(&self, directory: &Path, invocation: &str) -> Result<Command> {
        let directory = platform::paths::canonical(directory)?;
        let mut command = shell(invocation)?;
        self.configure(&mut command);
        command
            .current_dir(&directory)
            .env("MISE_TRUSTED_CONFIG_PATHS", &directory)
            .env(
                "MISE_CEILING_PATHS",
                directory
                    .parent()
                    .context("toolchain directory has no parent")?,
            );
        Ok(command)
    }

    fn fetch(&self, url: &str) -> Result<()> {
        let directory = tempfile::tempdir_in(self.scope.path())?;
        let git = system_git();
        let status = self
            .command(&git)
            .current_dir(directory.path())
            .args(["init", "--bare", "--quiet"])
            .status()?;
        ensure!(status.success(), "git init failed with {status}");
        let status = self.fetch_command(directory.path(), url).status()?;
        ensure!(status.success(), "git fetch {url} failed with {status}");
        Ok(())
    }

    #[must_use]
    pub fn fetch_command(&self, directory: &Path, url: &str) -> Command {
        let mut command = self.command(system_git());
        command
            .current_dir(directory)
            .args(["fetch", "--depth=1", url, "HEAD"]);
        command
    }
}

fn github_config(home: &Path) -> PathBuf {
    for (variable, suffix) in [("GH_CONFIG_DIR", ""), ("XDG_CONFIG_HOME", GH_DIRECTORY)] {
        if let Some(directory) = std::env::var_os(variable).filter(|value| !value.is_empty()) {
            return PathBuf::from(directory).join(suffix);
        }
    }
    if cfg!(windows)
        && let Some(directory) = std::env::var_os("APPDATA").filter(|value| !value.is_empty())
    {
        return PathBuf::from(directory).join(WINDOWS_GH_DIRECTORY);
    }
    home.join(GH_CONFIG)
}

impl Drop for Scratch {
    fn drop(&mut self) {
        #[cfg(unix)]
        platform::cleanup::report(
            writable_directories(&self.directory, self.scope.path())
                .map_err(|error| format!("{error:#}")),
            format_args!(
                "prepare candidate host environment {}",
                self.scope.path().display()
            ),
        );
        platform::cleanup::report(
            platform::cleanup::absent_ok(fs::remove_dir_all(self.scope.path())),
            format_args!(
                "remove candidate host environment {}",
                self.scope.path().display()
            ),
        );
    }
}

#[cfg(unix)]
fn writable_directories(directory: &fs::File, path: &Path) -> Result<()> {
    use std::os::unix::ffi::OsStrExt as _;
    use std::os::unix::fs::PermissionsExt as _;
    let mode = directory
        .metadata()
        .with_context(|| format!("read directory mode {}", path.display()))?
        .permissions()
        .mode();
    let writable = rules::hosts_rules::cleanup_directory_mode(mode);
    if writable != mode {
        directory
            .set_permissions(fs::Permissions::from_mode(writable))
            .with_context(|| format!("prepare directory {}", path.display()))?;
    }
    let entries = match rustix::fs::Dir::read_from(directory) {
        Ok(entries) => entries,
        Err(rustix::io::Errno::NOENT) => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("read directory {}", path.display()));
        }
    };
    for entry in entries {
        let entry = entry.with_context(|| format!("read entry in {}", path.display()))?;
        let name = entry.file_name();
        if matches!(name.to_bytes(), b"." | b"..") {
            continue;
        }
        let child_path = path.join(OsStr::from_bytes(name.to_bytes()));
        match rustix::fs::openat(directory, name, DIRECTORY_FLAGS, rustix::fs::Mode::empty()) {
            Ok(descriptor) => writable_directories(&fs::File::from(descriptor), &child_path)?,
            Err(rustix::io::Errno::NOENT | rustix::io::Errno::NOTDIR | rustix::io::Errno::LOOP) => {
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("open directory {}", child_path.display()));
            }
        }
    }
    Ok(())
}

fn system_git() -> PathBuf {
    const UNIX_GIT: &str = "/usr/bin/git";
    const WINDOWS_GIT: &str = "C:/Program Files/Git/cmd/git.exe";
    if cfg!(windows) {
        WINDOWS_GIT.into()
    } else {
        UNIX_GIT.into()
    }
}

fn applied_command(config: &Path, home: &Path, scope: &Path, renderer: &Path) -> Command {
    let store = crate::profiles::Store::scoped(scope);
    let mut command = unattended(renderer);
    command
        .arg("--config")
        .arg(config)
        .arg("--destination")
        .arg(home)
        .arg("--persistent-state")
        .arg(store.database)
        .args(["--refresh-externals=never", "--no-tty"])
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", scope)
        .env("XDG_CACHE_HOME", scope)
        .env("LOCALAPPDATA", home.join("AppData/Local"))
        .env("APPDATA", home.join("AppData/Roaming"))
        .current_dir(home);
    if let Some(cache) = store.cache {
        command.arg("--cache").arg(cache);
    }
    if let Some(path) = std::env::var_os("DOTFILES_HOST_PATH") {
        command.env("PATH", path);
    }
    command
}

fn verify_applied(config: &Path, home: &Path, renderer: &Path) -> Result<Applied> {
    let scope = tempfile::tempdir()?;
    let output = crate::profiles::output(
        applied_command(config, home, scope.path(), renderer).arg("source-path"),
        "find applied profile source",
    )?;
    let source = String::from_utf8(output.stdout)?;
    let source = Path::new(source.trim());
    ensure!(source.is_absolute(), "applied source must be absolute");
    let output = applied_command(config, home, scope.path(), renderer)
        .arg("--source")
        .arg(source)
        .args(["status", "--exclude", "scripts,externals", "--keep-going"])
        .output()?;
    Ok(Applied {
        drifted_targets: crate::profiles::target_changes(std::str::from_utf8(&output.stdout)?),
        verification_error: (!output.status.success()).then(|| {
            format!(
                "applied profile verification failed with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )
        }),
    })
}

/// Apply the candidate in scratch storage and inspect the applied profile without changing host files.
#[must_use]
pub fn check(root: &Path, home: &Path) -> Report {
    let config = home.join(CONFIG);
    let mut applied = None;
    let candidate = (|| {
        let scratch = Scratch::new(&config)?.cache_downloads(home);
        scratch.github(&github_config(home))?;
        let mut candidate = Checks::default();
        let mut renderer = None;
        candidate.run("host.renderer", || {
            renderer = Some(scratch.renderer(
                root,
                &platform::code_environment::Installations::discover(home)?,
            )?);
            Ok(())
        });
        let inspection =
            || verify_applied(&config, home, renderer.as_deref().context("host renderer")?);
        applied = Some(inspection().unwrap_or_else(|error| Applied {
            drifted_targets: Vec::new(),
            verification_error: Some(format!("{error:#}")),
        }));
        candidate.run("host.profile-apply", || {
            scratch.profile(root, "apply", renderer.as_deref().context("host renderer")?)
        });
        candidate.run("host.runtime", || {
            scratch.provision_runtime(root, &std::env::current_exe()?, home)
        });
        candidate.run("host.scratch-verify", || {
            scratch.profile(
                root,
                "verify",
                renderer.as_deref().context("host renderer")?,
            )
        });
        candidate.run("host.toolchain", || scratch.install(&scratch.home));
        candidate.run("host.project-tools", || scratch.install(root));
        for url in PUBLIC_SOURCES {
            candidate.run(url, || scratch.fetch(url));
        }
        candidate.finish()
    })();
    let applied = applied.unwrap_or_else(|| Applied {
        drifted_targets: Vec::new(),
        verification_error: Some(format!("applied inspection unavailable: {candidate:?}")),
    });
    applied.report("host.applied");
    Report { candidate, applied }
}

/// Run a command in the user's native login shell.
///
/// # Errors
///
/// Returns an error when the native shell can't start.
pub fn shell(command: &str) -> Result<Command> {
    let mut process = if cfg!(windows) {
        let mut process = unattended("pwsh");
        process.args(["-NoLogo", "-NoProfile", "-Command", command]);
        process
    } else {
        let shell = std::env::var_os("SHELL").unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                MAC_SHELL.into()
            } else {
                UNIX_SHELL.into()
            }
        });
        ensure!(
            Path::new(&shell).is_absolute(),
            "the native shell must have an absolute path"
        );
        let mut process = unattended(shell);
        process.args(["-lc", command]);
        process
    };
    process.current_dir(std::env::current_dir().context("native working directory")?);
    Ok(process)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn an_already_removed_scratch_needs_no_permission_preparation() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let config = fixture.path().join("chezmoi.toml");
        fs::write(
            &config,
            format!(
                "[data]\nprofile = {:?}\n",
                crate::profiles::native_profile()?.name()
            ),
        )?;
        let scratch = Scratch::new(&config)?;
        fs::remove_dir_all(scratch.scope.path())?;
        writable_directories(&scratch.directory, scratch.scope.path())?;
        drop(scratch);
        Ok(())
    }

    #[test]
    #[cfg(unix)]
    fn directory_permissions_stay_bound_to_the_open_descriptor_after_replacement() -> Result<()> {
        use std::os::unix::fs::{PermissionsExt as _, symlink};
        const READ_ONLY_MODE: u32 = 0o555;
        const WRITABLE_MODE: u32 = 0o755;
        let fixture = tempfile::tempdir()?;
        let owned = fixture.path().join("owned");
        let parked = fixture.path().join("parked");
        let outside = fixture.path().join("outside");
        fs::create_dir(&owned)?;
        fs::create_dir(&outside)?;
        fs::write(outside.join("sentinel"), "outside bytes")?;
        let descriptor = fs::File::from(rustix::fs::open(
            &owned,
            DIRECTORY_FLAGS,
            rustix::fs::Mode::empty(),
        )?);
        let outside_descriptor = fs::File::from(rustix::fs::open(
            &outside,
            DIRECTORY_FLAGS,
            rustix::fs::Mode::empty(),
        )?);
        descriptor.set_permissions(fs::Permissions::from_mode(READ_ONLY_MODE))?;
        outside_descriptor.set_permissions(fs::Permissions::from_mode(READ_ONLY_MODE))?;
        let outside_mode = outside_descriptor.metadata()?.permissions().mode();
        fs::rename(&owned, &parked)?;
        symlink(&outside, &owned)?;
        writable_directories(&descriptor, &owned)?;
        let preserved_mode = outside_descriptor.metadata()?.permissions().mode();
        let parked_mode = descriptor.metadata()?.permissions().mode();
        outside_descriptor.set_permissions(fs::Permissions::from_mode(WRITABLE_MODE))?;
        assert_eq!(preserved_mode, outside_mode);
        assert_eq!(
            parked_mode,
            rules::hosts_rules::cleanup_directory_mode(outside_mode)
        );
        assert_eq!(fs::read(outside.join("sentinel"))?, b"outside bytes");
        Ok(())
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn an_unreadable_directory_reports_the_actual_entry_without_changing_other_permissions()
    -> Result<()> {
        use std::os::unix::fs::PermissionsExt as _;
        const INACCESSIBLE_MODE: u32 = 0;
        const WRITABLE_MODE: u32 = 0o755;
        let fixture = tempfile::tempdir()?;
        let unreadable = fixture.path().join("unreadable");
        fs::create_dir(&unreadable)?;
        let descriptor = fs::File::from(rustix::fs::open(
            fixture.path(),
            DIRECTORY_FLAGS,
            rustix::fs::Mode::empty(),
        )?);
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(INACCESSIBLE_MODE))?;
        let original_mode = fs::metadata(&unreadable)?.permissions().mode();
        let result = writable_directories(&descriptor, fixture.path());
        let unchanged = fs::metadata(&unreadable)?.permissions().mode();
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(WRITABLE_MODE))?;
        let error =
            result.expect_err("an inaccessible child must report the operating-system failure");
        let diagnosis = format!("{error:#}");
        assert!(diagnosis.contains(&unreadable.display().to_string()));
        assert!(diagnosis.contains("Permission denied"));
        assert_eq!(unchanged, original_mode);
        Ok(())
    }

    #[test]
    fn real_downloads_survive_scratch_and_installations_remain_owned() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let host = scope.path().join("host");
        let config = scope.path().join("chezmoi.toml");
        fs::write(
            &config,
            format!(
                "[data]\nprofile = {:?}\n",
                crate::profiles::native_profile()?.name()
            ),
        )?;
        let scratch = Scratch::new(&config)?.cache_downloads(&host);
        let command = scratch.command("mise");
        let value = |name: &str| -> Result<PathBuf> {
            Ok(command
                .get_envs()
                .find_map(|(key, value)| (key == name).then_some(value).flatten())
                .context("mise directory")?
                .into())
        };
        let cache = value("MISE_CACHE_DIR")?;
        let downloads = value("MISE_DOWNLOADS_DIR")?;
        assert!(cache.starts_with(host.join(".cache")));
        assert!(downloads.starts_with(host.join(".cache")));
        assert!(value("MISE_DATA_DIR")?.starts_with(&scratch.home));
        assert!(value("MISE_CONFIG_DIR")?.starts_with(&scratch.home));
        fs::create_dir_all(&downloads)?;
        fs::write(downloads.join("archive"), "downloaded")?;
        drop(scratch);
        let next = Scratch::new(&config)?.cache_downloads(&host);
        assert_eq!(fs::read(downloads.join("archive"))?, b"downloaded");
        assert_eq!(next.download_cache.as_ref(), Some(&cache));
        Ok(())
    }

    #[test]
    fn candidate_cli_configuration_is_explicitly_confined_to_scratch() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let config = scope.path().join("chezmoi.toml");
        fs::write(
            &config,
            format!(
                "[data]\nprofile = {:?}\n",
                crate::profiles::native_profile()?.name()
            ),
        )?;
        let scratch = Scratch::new(&config)?;
        let command = scratch.command("gh");
        let directory = command
            .get_envs()
            .find_map(|(name, value)| (name == "GH_CONFIG_DIR").then_some(value).flatten());
        assert_eq!(directory, Some(scratch.home.join(".config/gh").as_os_str()));
        Ok(())
    }

    #[test]
    fn candidate_reads_the_cli_login_without_a_writable_path_to_host_files() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let host = scope.path().join("host");
        let github = host.join(GH_CONFIG);
        fs::create_dir_all(&github)?;
        fs::create_dir_all(host.join(".ssh"))?;
        fs::write(host.join(".ssh/id_ed25519"), "fixture SSH key")?;
        fs::write(host.join(".git-credentials"), "fixture Git credential")?;
        let login = "github.com:\n  user: fixture-gh-auth\n  oauth_token: fixture-cli-token\n  git_protocol: https\n";
        let settings = "prompt: enabled\n";
        fs::write(github.join(GH_LOGIN_FILES[0]), login)?;
        fs::write(github.join(GH_LOGIN_FILES[1]), settings)?;
        fs::write(github.join("unrelated"), "must not reach the candidate")?;
        let config = host.join("chezmoi.toml");
        fs::write(
            &config,
            format!(
                "[data]\nprofile = {:?}\n",
                crate::profiles::native_profile()?.name()
            ),
        )?;
        let scratch = Scratch::new(&config)?;
        scratch.github(&github)?;
        let output = scratch
            .command("gh")
            .env("GH_NO_UPDATE_NOTIFIER", "1")
            .args(["auth", "token", "--hostname", "github.com"])
            .output()?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            std::str::from_utf8(&output.stdout)?.trim(),
            "fixture-cli-token"
        );
        assert!(!scratch.home.join(".ssh").exists());
        assert!(!scratch.home.join(".git-credentials").exists());
        assert!(!scratch.home.join(GH_CONFIG).join("unrelated").exists());
        let status = scratch
            .command("gh")
            .env("GH_NO_UPDATE_NOTIFIER", "1")
            .args(["config", "set", "prompt", "disabled"])
            .status()?;
        ensure!(status.success(), "change the copied CLI configuration");
        assert_eq!(fs::read_to_string(github.join(GH_LOGIN_FILES[0]))?, login);
        assert_eq!(
            fs::read_to_string(github.join(GH_LOGIN_FILES[1]))?,
            settings
        );
        assert!(
            fs::read_to_string(scratch.home.join(GH_CONFIG).join(GH_LOGIN_FILES[1]))?
                .contains("disabled")
        );
        Ok(())
    }

    #[test]
    fn a_missing_cli_login_is_optional_but_a_failed_snapshot_is_reported() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let config = scope.path().join("chezmoi.toml");
        fs::write(
            &config,
            format!(
                "[data]\nprofile = {:?}\n",
                crate::profiles::native_profile()?.name()
            ),
        )?;
        let scratch = Scratch::new(&config)?;
        scratch.github(&scope.path().join("absent"))?;
        assert!(fs::read_dir(scratch.home.join(GH_CONFIG))?.next().is_none());
        let invalid = scope.path().join("invalid");
        fs::create_dir_all(invalid.join(GH_LOGIN_FILES[0]))?;
        assert!(scratch.github(&invalid).is_err());
        Ok(())
    }

    #[test]
    fn applied_verification_finds_the_native_default_source_read_only() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let home = scope.path().join("host");
        let source = home.join(".local/share/chezmoi");
        let config = home.join(CONFIG);
        fs::create_dir_all(source.join("home"))?;
        fs::create_dir_all(config.parent().context("configuration directory")?)?;
        fs::write(source.join(".chezmoiroot"), "home\n")?;
        fs::write(source.join("home/dot_existing"), "source\n")?;
        fs::write(source.join("home/dot_missing"), "source\n")?;
        fs::write(home.join(".existing"), "host drift\n")?;
        let answers = format!(
            "[data]\nprofile = {:?}\n",
            crate::profiles::native_profile()?.name()
        );
        fs::write(&config, &answers)?;
        let applied = verify_applied(
            &config,
            &home,
            Path::new(platform::tool::Tool::Chezmoi.program()),
        )?;
        assert_eq!(applied.drifted_targets, [".existing", ".missing"]);
        assert!(applied.verification_error.is_none());
        assert_eq!(fs::read_to_string(&config)?, answers);
        assert_eq!(fs::read_to_string(home.join(".existing"))?, "host drift\n");
        assert!(!home.join(".missing").exists());
        assert!(!config.with_file_name("chezmoistate.boltdb").exists());
        assert!(!home.join(".cache/chezmoi").exists());
        Ok(())
    }

    #[test]
    fn candidate_mise_reads_only_the_rendered_global_and_current_project() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let config = scope.path().join("chezmoi.toml");
        fs::write(
            &config,
            format!(
                "[data]\nprofile = {:?}\n",
                crate::profiles::native_profile()?.name()
            ),
        )?;
        let source = scope.path().join("candidate");
        fs::create_dir_all(source.join("dot_config/mise"))?;
        fs::write(
            source.join("dot_config/mise/config.toml"),
            "[tools]\npython = '3.14.8'\n",
        )?;
        fs::create_dir_all(scope.path().join(".config/mise"))?;
        fs::write(
            scope.path().join(".config/mise/config.toml"),
            "[tools]\npython = 'invalid-ancestor-version'\n",
        )?;
        let project = scope.path().join("project");
        fs::create_dir(&project)?;
        fs::write(project.join("mise.toml"), "[tools]\nuv = '0.12.22'\n")?;
        let scratch = Scratch::new(&config)?;
        scratch.profile(
            &source,
            "apply",
            Path::new(platform::tool::Tool::Chezmoi.program()),
        )?;
        let output = scratch
            .toolchain_command(&project, "mise ls --json")?
            .output()?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let tools: serde_json::Map<String, serde_json::Value> =
            serde_json::from_slice(&output.stdout)?;
        assert_eq!(tools.len(), 2);
        assert_eq!(tools["python"][0]["requested_version"], "3.14.8");
        assert_eq!(
            Path::new(
                tools["python"][0]["source"]["path"]
                    .as_str()
                    .context("mise source")?
            ),
            scratch.home.join(".config/mise/config.toml")
        );
        assert_eq!(tools["uv"][0]["requested_version"], "0.12.22");
        assert_eq!(
            Path::new(
                tools["uv"][0]["source"]["path"]
                    .as_str()
                    .context("project mise source")?
            ),
            project.join("mise.toml")
        );
        Ok(())
    }

    #[test]
    fn candidate_application_and_applied_verification_preserve_the_host_home() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let host = scope.path().join("host");
        let applied = scope.path().join("applied");
        let candidate = scope.path().join("candidate");
        for path in [&host, &applied, &candidate] {
            fs::create_dir_all(path)?;
        }
        let profile = crate::profiles::native_profile()?.name();
        let config = host.join("chezmoi.toml");
        let contents = format!(
            "sourceDir = {}\n[data]\nprofile = {profile:?}\n[data.paths]\nprojects = {}\n",
            toml::Value::String(applied.to_string_lossy().into_owned()),
            toml::Value::String(host.join("projects").to_string_lossy().into_owned())
        );
        fs::write(&config, &contents)?;
        fs::write(applied.join("dot_existing"), "installed source\n")?;
        fs::write(host.join(".existing"), "installed source\n")?;
        fs::write(
            candidate.join("dot_candidate.tmpl"),
            "{{ .chezmoi.homeDir }}\n{{ env \"HOME\" }}\n{{ .paths.projects }}\n",
        )?;
        let marker = host.join("forbidden");
        let (launcher, script) = if cfg!(windows) {
            (
                "run_once_before_escape.ps1",
                format!(
                    "Set-Content -LiteralPath '{}' -Value escaped",
                    marker.display()
                ),
            )
        } else {
            (
                "run_once_before_escape.sh",
                format!("#!/bin/sh\nprintf escaped > '{}'\n", marker.display()),
            )
        };
        fs::write(candidate.join(launcher), script)?;
        let scratch = Scratch::new(&config)?;
        let program = Path::new(platform::tool::Tool::Chezmoi.program());
        scratch.profile(&candidate, "apply", program)?;
        scratch.profile(&candidate, "verify", program)?;
        let rendered = fs::read_to_string(scratch.home.join(".candidate"))?;
        assert!(!rendered.contains(&host.to_string_lossy().replace('\\', "/")));
        assert_eq!(rendered.lines().count(), 3);
        assert_eq!(
            Path::new(rendered.lines().next().context("rendered destination")?),
            Path::new(rendered.lines().nth(1).context("rendered HOME")?)
        );
        let applied_report = verify_applied(&config, &host, program)?;
        assert_eq!(applied_report.drifted_targets, Vec::<String>::new());
        assert!(applied_report.verification_error.is_none());
        assert_eq!(fs::read_to_string(&config)?, contents);
        assert_eq!(
            fs::read_to_string(host.join(".existing"))?,
            "installed source\n"
        );
        assert!(!host.join(".candidate").exists());
        assert!(!marker.exists());
        let command = scratch.command("mise");
        for (name, value) in command.get_envs().filter(|(_, value)| value.is_some()) {
            if matches!(
                name.to_str(),
                Some(
                    "HOME"
                        | "USERPROFILE"
                        | "ProgramData"
                        | "ALLUSERSPROFILE"
                        | "CARGO_HOME"
                        | "RUSTUP_HOME"
                        | "MISE_DATA_DIR"
                        | "MISE_CONFIG_DIR"
                        | "MISE_GLOBAL_CONFIG_FILE"
                        | "MISE_SYSTEM_CONFIG_FILE"
                        | "MISE_CACHE_DIR"
                        | "MISE_STATE_DIR"
                        | "GIT_CONFIG_GLOBAL"
                )
            ) {
                assert!(
                    Path::new(value.unwrap())
                        .starts_with(scratch.home.parent().context("scratch root")?)
                );
            }
        }
        fs::write(host.join(".existing"), "host drift\n")?;
        fs::write(applied.join("dot_other"), "expected\n")?;
        fs::write(host.join(".other"), "other drift\n")?;
        let applied_report = verify_applied(&config, &host, program)?;
        assert_eq!(applied_report.drifted_targets, [".existing", ".other"]);
        assert!(applied_report.verification_error.is_none());
        assert_eq!(fs::read_to_string(host.join(".existing"))?, "host drift\n");
        assert_eq!(fs::read_to_string(host.join(".other"))?, "other drift\n");
        Ok(())
    }
}

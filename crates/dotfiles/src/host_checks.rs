//! Candidate checks provision scratch files and inspect the applied host profile.

use anyhow::{Context, Result, ensure};
use platform::checks::Checks;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const CONFIG: &str = ".config/chezmoi/chezmoi.toml";
const GIT_CONFIG: &str = ".config/git/config";
const MAC_SHELL: &str = "/bin/zsh";
const UNIX_SHELL: &str = "/bin/bash";
const PUBLIC_SOURCES: [&str; 3] = [
    "https://github.com/eza-community/eza.git",
    "https://github.com/RustSec/advisory-db.git",
    "https://github.com/embarkstudios/std-replacement-data.git",
];
const UNATTENDED_SSH: &str = "ssh -o BatchMode=yes -o IdentityAgent=none -o PasswordAuthentication=no -o KbdInteractiveAuthentication=no";

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
    home: PathBuf,
    config: PathBuf,
}

impl Scratch {
    /// Copy the host's answers while relocating its paths into an owned destination.
    ///
    /// # Errors
    ///
    /// Returns an error for missing or malformed configuration or unavailable scratch storage.
    pub fn new(config: &Path) -> Result<Self> {
        let scope = tempfile::tempdir()?;
        let directory = platform::paths::canonical(scope.path())?;
        let home = directory.join("home");
        fs::create_dir_all(&home)?;
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
            home,
            config,
        })
    }

    /// Keep native executable resolution while redirecting writable user state.
    #[must_use]
    pub fn command(&self, program: impl AsRef<OsStr>) -> Command {
        let mut command = unattended(program);
        if let Some(path) = std::env::var_os("DOTFILES_HOST_PATH") {
            command.env("PATH", path);
        }
        for (name, _) in std::env::vars_os() {
            let text = name.to_string_lossy();
            if text.starts_with("MISE_")
                || text.starts_with("CARGO_")
                || text.starts_with("RUSTUP_")
                || text.starts_with("GIT_CONFIG_")
                || platform::tool::GIT_LOCAL_ENVIRONMENT.contains(&text.as_ref())
            {
                command.env_remove(&name);
            }
        }
        for name in [
            "GH_TOKEN",
            "GITHUB_TOKEN",
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
            ("MISE_DATA_DIR", self.home.join(".local/share/mise")),
            ("MISE_CONFIG_DIR", self.home.join(".config/mise")),
            ("MISE_CACHE_DIR", self.home.join(".cache/mise")),
            ("MISE_STATE_DIR", self.home.join(".local/state/mise")),
            ("GIT_CONFIG_GLOBAL", self.home.join(GIT_CONFIG)),
            ("SKILL_OPS_STATE", self.home.join(".local/state/skill-ops")),
            ("TMPDIR", self.scope.path().to_path_buf()),
            ("TMP", self.scope.path().to_path_buf()),
            ("TEMP", self.scope.path().to_path_buf()),
        ] {
            command.env(name, value);
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

    fn profile(&self, root: &Path, action: &str) -> Result<()> {
        let mut command =
            crate::profiles::chezmoi(root, self.scope.path(), &self.config, &self.home);
        self.configure(&mut command);
        let status = command
            .args([action, "--exclude", "scripts,externals"])
            .status()?;
        ensure!(
            status.success(),
            "candidate profile {action} failed with {status}"
        );
        Ok(())
    }

    fn install(&self, directory: &Path) -> Result<()> {
        let mut command = shell("mise install -y")?;
        self.configure(&mut command);
        let status = command
            .current_dir(directory)
            .env("MISE_TRUSTED_CONFIG_PATHS", directory)
            .status()?;
        ensure!(status.success(), "mise install failed with {status}");
        Ok(())
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

impl Drop for Scratch {
    fn drop(&mut self) {
        platform::cleanup::report(
            fs::remove_dir_all(self.scope.path()),
            "remove candidate host environment",
        );
    }
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

fn verify_applied(config: &Path, home: &Path) -> Result<()> {
    let answers: toml::Table = toml::from_str(&fs::read_to_string(config)?)?;
    let source = Path::new(
        answers
            .get("sourceDir")
            .and_then(toml::Value::as_str)
            .context("applied configuration requires sourceDir")?,
    );
    ensure!(source.is_absolute(), "applied source must be absolute");
    let scope = tempfile::tempdir()?;
    let mut command = crate::profiles::chezmoi(source, scope.path(), config, home);
    command
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("LOCALAPPDATA", platform::home::local_app_data(home))
        .env("APPDATA", home.join("AppData/Roaming"));
    if let Some(path) = std::env::var_os("DOTFILES_HOST_PATH") {
        command.env("PATH", path);
    }
    let status = command
        .args(["verify", "--exclude", "scripts,externals"])
        .status()?;
    ensure!(
        status.success(),
        "applied profile verification failed with {status}"
    );
    Ok(())
}

/// Apply the candidate in scratch storage and inspect the applied profile without changing host files.
///
/// # Errors
///
/// Returns every failed profile, installation, and network check.
pub fn check(root: &Path, home: &Path) -> Result<()> {
    let config = home.join(CONFIG);
    let mut checks = Checks::default();
    checks.run("host.profile-verify", || verify_applied(&config, home));
    checks.run("host.candidate", || {
        let scratch = Scratch::new(&config)?;
        let mut candidate = Checks::default();
        candidate.run("host.profile-apply", || scratch.profile(root, "apply"));
        candidate.run("host.scratch-verify", || scratch.profile(root, "verify"));
        candidate.run("host.toolchain", || scratch.install(&scratch.home));
        candidate.run("host.project-tools", || scratch.install(root));
        for url in PUBLIC_SOURCES {
            candidate.run(url, || scratch.fetch(url));
        }
        candidate.finish()
    });
    checks.finish()
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
        scratch.profile(&candidate, "apply")?;
        scratch.profile(&candidate, "verify")?;
        let rendered = fs::read_to_string(scratch.home.join(".candidate"))?;
        assert!(!rendered.contains(&host.to_string_lossy().replace('\\', "/")));
        assert_eq!(rendered.lines().count(), 3);
        assert_eq!(
            Path::new(rendered.lines().next().context("rendered destination")?),
            Path::new(rendered.lines().nth(1).context("rendered HOME")?)
        );
        verify_applied(&config, &host)?;
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
        assert!(verify_applied(&config, &host).is_err());
        assert_eq!(fs::read_to_string(host.join(".existing"))?, "host drift\n");
        Ok(())
    }
}

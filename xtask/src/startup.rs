//! The programs each interactive shell starts while it reads its startup files, counted through logging shims for every program on `PATH`.
//! zsh, Nushell, and PowerShell must start none, so a new initialization that forks fails the rehearsal.

use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

/// A shell whose startup the rehearsal counts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Shell {
    Zsh,
    Nu,
    Pwsh,
}

impl Shell {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Zsh => "zsh",
            Self::Nu => "nu",
            Self::Pwsh => "pwsh",
        }
    }
}

/// The program file names on `path`, first occurrence first, each with the file that runs it.
fn programs(path: &OsStr) -> Vec<(String, std::path::PathBuf)> {
    let mut seen = BTreeSet::new();
    let mut found = Vec::new();
    for directory in std::env::split_paths(path) {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let file = entry.path();
            let Some(name) = file
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
            else {
                continue;
            };
            let runnable = if cfg!(windows) {
                file.extension().is_some_and(|extension| {
                    matches!(
                        extension.to_string_lossy().to_ascii_lowercase().as_str(),
                        "exe" | "cmd" | "bat"
                    )
                })
            } else {
                is_executable(&file)
            };
            let stem = if cfg!(windows) {
                file.file_stem().map_or_else(String::new, |stem| {
                    stem.to_string_lossy().to_ascii_lowercase()
                })
            } else {
                name
            };
            if runnable && file.is_file() && seen.insert(stem.clone()) {
                found.push((stem, file));
            }
        }
    }
    found
}

#[cfg(unix)]
fn is_executable(file: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    /// Any of a file's three execute bits.
    const ANY_EXECUTE: u32 = 0o111;
    fs::metadata(file).is_ok_and(|metadata| metadata.permissions().mode() & ANY_EXECUTE != 0)
}

#[cfg(not(unix))]
fn is_executable(_: &Path) -> bool {
    false
}

/// Writes into `shims` one logging shim for every program on `path`, which appends its name to `log` and runs the real program.
///
/// # Errors
///
/// Returns an error when writing a shim fails.
pub fn write_shims(path: &OsStr, shims: &Path, log: &Path) -> Result<usize> {
    fs::create_dir_all(shims)?;
    let quote = |text: &Path| text.to_string_lossy().replace('\'', r"'\''");
    let mut written = 0;
    for (name, real) in programs(path) {
        if cfg!(windows) {
            let shim = shims.join(format!("{name}.cmd"));
            fs::write(
                &shim,
                format!(
                    "@echo {name} %*>>\"{}\"\r\n@\"{}\" %*\r\n",
                    log.display(),
                    real.display()
                ),
            )?;
        } else {
            let shim = shims.join(&name);
            fs::write(
                &shim,
                format!(
                    "#!/bin/sh\nprintf '%s %s\\n' '{name}' \"$*\" >> '{}'\nexec '{}' \"$@\"\n",
                    quote(log),
                    quote(&real)
                ),
            )?;
            #[cfg(unix)]
            make_executable(&shim)?;
        }
        written += 1;
    }
    Ok(written)
}

#[cfg(unix)]
fn make_executable(file: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    /// The mode of a written shim: its owner writes it and everyone runs it.
    const SHIM_MODE: u32 = 0o755;
    fs::set_permissions(file, fs::Permissions::from_mode(SHIM_MODE))?;
    Ok(())
}

/// The file that runs `tool` on `path`, so the shell itself never starts through a shim.
fn located(tool: Tool, path: &OsStr) -> Result<std::path::PathBuf> {
    let program = Path::new(tool.program());
    if program.is_absolute() {
        return Ok(program.to_path_buf());
    }
    programs(path)
        .into_iter()
        .find(|(name, _)| name == tool.program())
        .map(|(_, file)| file)
        .with_context(|| format!("{} is not on PATH", tool.program()))
}

/// The startup files and the caches they read, relative to a home.
const STARTUP_PATHS: [&str; 12] = [
    ".zshrc",
    ".zprofile",
    ".zshenv",
    ".zlogin",
    ".zlogout",
    ".cache/zsh",
    ".config/shell",
    ".config/nushell",
    ".config/fzf",
    "Documents/PowerShell",
    ".cache/pwsh",
    ".config/starship.toml",
];

const STARTUP_COMPLETE: &str = "__DOTFILES_STARTUP_COMPLETE__";
const NU_EXIT: &str = "while not (job list | is-empty) { sleep 1ms }; exit";

fn witness(shell: Shell, home: &Path) -> Result<()> {
    let (file, marker) = match shell {
        Shell::Zsh => (".zshrc", format!("printf '%s\\n' '{STARTUP_COMPLETE}'")),
        Shell::Nu => (
            ".config/nushell/config.nu",
            format!("print '{STARTUP_COMPLETE}'"),
        ),
        Shell::Pwsh => (
            "Documents/PowerShell/Microsoft.PowerShell_profile.ps1",
            format!("Write-Output '{STARTUP_COMPLETE}'"),
        ),
    };
    let file = home.join(file);
    let mut contents = fs::read_to_string(&file)?;
    contents.push_str(&format!("\n{marker}\n"));
    fs::write(file, contents)?;
    Ok(())
}

/// Copies a file or a directory tree, when it exists.
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    if source.is_dir() {
        fs::create_dir_all(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_tree(&entry.path(), &target.join(entry.file_name()))?;
        }
    } else if source.is_file() {
        fs::create_dir_all(target.parent().context("a copied file has a parent")?)?;
        fs::copy(source, target)?;
    }
    Ok(())
}

/// The command that starts `shell` interactively over the startup files under `home` and exits at once.
fn startup(shell: Shell, home: &Path, path: &OsStr) -> Result<Command> {
    Ok(match shell {
        Shell::Zsh => {
            let mut command = crate::tool::external(located(Tool::Zsh, path)?);
            // The host's own /etc/zshrc runs programs of its own, such as macOS calling `locale`, which no file of this repository controls.
            command
                .args(["--no-globalrcs", "-e", "-l", "-i", "-c", "wait; exit"])
                .env("ZDOTDIR", home);
            command
        }
        Shell::Nu => {
            let config = home.join(".config/nushell");
            let mut command = crate::tool::external(located(Tool::Nu, path)?);
            command
                .arg("--config-home")
                .arg(&config)
                .arg("--config")
                .arg(config.join("config.nu"))
                .arg("--env-config")
                .arg(config.join("env.nu"))
                .args(["--interactive", "--execute", NU_EXIT]);
            command
        }
        Shell::Pwsh => {
            let mut command = crate::tool::external(located(Tool::Pwsh, path)?);
            command
                .args(["-NoLogo", "-NoProfile", "-Command"])
                .arg(format!(
                    "$ErrorActionPreference = 'Stop'; $PSNativeCommandUseErrorActionPreference = $true; Import-Module PSReadLine; . '{}'; exit",
                    home.join("Documents/PowerShell/Microsoft.PowerShell_profile.ps1")
                        .to_string_lossy()
                        .replace('\'', "''")
                ));
            command
        }
    })
}

/// The programs `shell` starts while it reads the startup files under `home`, with only logging shims for the programs of `path` on its `PATH`.
///
/// # Errors
///
/// Returns an error when writing shims or starting the shell fails.
pub fn started(shell: Shell, home: &Path, path: &OsStr) -> Result<Vec<String>> {
    let scope = tempfile::tempdir()?;
    // Startup files prepend ~/.local/bin, so the measured home's directory holds every logging shim.
    let measured = scope.path().join("home");
    for startup_path in STARTUP_PATHS {
        copy_tree(&home.join(startup_path), &measured.join(startup_path))?;
    }
    witness(shell, &measured)?;
    for directory in ["AppData/Roaming", "AppData/Local", ".local/share"] {
        fs::create_dir_all(measured.join(directory))?;
    }
    let shims = measured.join(".local/bin");
    let log = scope.path().join("started");
    fs::write(&log, "")?;
    write_shims(path, &shims, &log)?;
    let mut command = startup(shell, &measured, path)?;
    let output = command
        .current_dir(&measured)
        .env("PATH", &shims)
        .env("HOME", &measured)
        .env("USERPROFILE", &measured)
        .env("APPDATA", measured.join("AppData/Roaming"))
        .env("LOCALAPPDATA", measured.join("AppData/Local"))
        .env("XDG_CONFIG_HOME", measured.join(".config"))
        .env("XDG_DATA_HOME", measured.join(".local/share"))
        .env("TERM", "dumb")
        .env_remove("SSH_CONNECTION")
        .env_remove("HERDR_ENV")
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("start {}", shell.name()))?;
    ensure!(
        output.status.success(),
        "{} exited with {} while reading its startup files: {}",
        shell.name(),
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    ensure!(
        shell != Shell::Nu
            || !String::from_utf8_lossy(&output.stderr)
                .lines()
                .any(|line| line.starts_with("Error:")),
        "Nushell reported a startup error: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    ensure!(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.trim() == STARTUP_COMPLETE),
        "{} did not finish reading its measured startup file: {}",
        shell.name(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(fs::read_to_string(&log)?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_shell_logs_a_program_added_to_its_initialization() {
        let path = std::env::var_os("PATH").unwrap();
        let shells = if cfg!(windows) {
            &[Shell::Nu, Shell::Pwsh][..]
        } else {
            &[Shell::Zsh, Shell::Nu, Shell::Pwsh][..]
        };
        for &shell in shells {
            let scope = tempfile::tempdir().unwrap();
            let (file, initialization) = match shell {
                Shell::Zsh => (".zshrc", "git --version >/dev/null\n"),
                Shell::Nu => (".config/nushell/config.nu", "^git --version | ignore\n"),
                Shell::Pwsh => (
                    "Documents/PowerShell/Microsoft.PowerShell_profile.ps1",
                    "& git --version | Out-Null\n",
                ),
            };
            let file = scope.path().join(file);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, initialization).unwrap();
            if shell == Shell::Nu {
                fs::write(file.with_file_name("env.nu"), "").unwrap();
            }
            let programs = started(shell, scope.path(), &path).unwrap();
            assert!(
                programs.iter().any(|line| line.starts_with("git ")),
                "{}: {programs:?}",
                shell.name()
            );
        }
    }

    #[test]
    fn failed_initialization_cannot_report_zero_programs() {
        let home = tempfile::tempdir().unwrap();
        let profile = home
            .path()
            .join("Documents/PowerShell/Microsoft.PowerShell_profile.ps1");
        fs::create_dir_all(profile.parent().unwrap()).unwrap();
        fs::write(profile, "throw 'startup probe'\n").unwrap();
        let path = std::env::var_os("PATH").unwrap();
        assert!(started(Shell::Pwsh, home.path(), &path).is_err());
    }

    #[test]
    fn a_nushell_startup_job_cannot_escape_the_count() {
        let home = tempfile::tempdir().unwrap();
        let config = home.path().join(".config/nushell");
        fs::create_dir_all(&config).unwrap();
        fs::write(config.join("env.nu"), "").unwrap();
        fs::write(
            config.join("config.nu"),
            "job spawn { ^git --version | ignore } | ignore\n",
        )
        .unwrap();
        let path = std::env::var_os("PATH").unwrap();
        let programs = started(Shell::Nu, home.path(), &path).unwrap();
        assert!(
            programs.iter().any(|line| line.starts_with("git ")),
            "{programs:?}"
        );
    }
}

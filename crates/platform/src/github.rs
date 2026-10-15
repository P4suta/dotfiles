//! GitHub credentials from an installed CLI without entering mise's install locks.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

const SHIMS: &str = "shims";
const INSTALLATIONS: &str = "installs/gh";
const LATEST: &str = "latest";
const BIN: &str = "bin";

/// The owner's home used only by the native credential lookup child.
pub const CREDENTIAL_HOME: &str = "DOTFILES_GITHUB_CREDENTIAL_HOME";

fn direct(candidate: &Path, name: &OsStr) -> io::Result<Option<PathBuf>> {
    if !candidate.is_file() {
        return Ok(None);
    }
    let executable = crate::paths::canonical(candidate)?;
    Ok((executable.file_name() == Some(name)
        && executable.parent().and_then(Path::file_name) != Some(OsStr::new(SHIMS)))
    .then_some(executable))
}

fn installation(directory: &Path, name: &OsStr) -> io::Result<Option<PathBuf>> {
    if let Some(executable) = direct(&directory.join(BIN).join(name), name)? {
        return Ok(Some(executable));
    }
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    let mut archives = entries.collect::<io::Result<Vec<_>>>()?;
    archives.sort_by_key(fs::DirEntry::file_name);
    for archive in archives {
        if let Some(executable) = direct(&archive.path().join(BIN).join(name), name)? {
            return Ok(Some(executable));
        }
    }
    Ok(None)
}

fn installed(data: &Path, name: &OsStr) -> io::Result<Option<PathBuf>> {
    let installations = data.join(INSTALLATIONS);
    let alias = installations.join(LATEST);
    if let Some(executable) = installation(&alias, name)? {
        return Ok(Some(executable));
    }
    if alias.is_file() {
        let target = fs::read_to_string(&alias)?;
        if let Some(executable) = installation(&installations.join(target.trim()), name)? {
            return Ok(Some(executable));
        }
    }
    let entries = match fs::read_dir(&installations) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut versions = entries.collect::<io::Result<Vec<_>>>()?;
    versions.sort_by_key(fs::DirEntry::file_name);
    for version in versions.into_iter().rev() {
        if let Some(executable) = installation(&version.path(), name)? {
            return Ok(Some(executable));
        }
    }
    Ok(None)
}

/// Resolve a native CLI, replacing a mise shim with an executable in its installation store.
///
/// # Errors
///
/// Returns an error if installation discovery fails or no installed CLI exists.
pub fn executable(path: &OsStr) -> io::Result<PathBuf> {
    let name = format!("gh{}", std::env::consts::EXE_SUFFIX);
    let name = OsStr::new(&name);
    for directory in std::env::split_paths(path) {
        if !directory.is_dir() {
            continue;
        }
        let directory = crate::paths::canonical(&directory)?;
        if directory.file_name() == Some(OsStr::new(SHIMS)) {
            if let Some(data) = directory.parent()
                && let Some(executable) = installed(data, name)?
            {
                return Ok(executable);
            }
        } else if let Some(executable) = direct(&directory.join(name), name)? {
            return Ok(executable);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "an installed GitHub CLI is unavailable; mise shims cannot supply credentials",
    ))
}

/// Read the requested host's CLI login with no token persistence or mise invocation.
///
/// # Errors
///
/// Returns an error if PATH lacks an installed CLI or reading its installation fails.
pub fn command(host: &OsStr) -> io::Result<Command> {
    command_in_home(host, None)
}

/// Resolve credentials with an optional native home for the lookup child.
///
/// # Errors
///
/// Returns an error if native CLI discovery fails.
pub fn command_in_home(host: &OsStr, home: Option<&Path>) -> io::Result<Command> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let directories =
        std::env::split_paths(&path).chain(crate::process::mise_shims(std::env::var_os));
    let discovery = std::env::join_paths(directories)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    Ok(credential_command(&executable(&discovery)?, host, home))
}

fn credential_command(program: &Path, host: &OsStr, home: Option<&Path>) -> Command {
    let mut command = crate::process::command(program);
    command.args(["auth", "token", "--hostname"]).arg(host);
    command.env_remove(CREDENTIAL_HOME);
    if let Some(home) = home {
        command.env("HOME", home);
    }
    command
}

#[cfg(all(test, feature = "tools"))]
mod tests {
    use super::*;

    #[test]
    fn credential_home_changes_only_the_lookup_child() -> io::Result<()> {
        let scope = tempfile::tempdir()?;
        let native = scope.path().join("native");
        let scratch = scope.path().join("scratch");
        let login = scratch.join("gh");
        let script = scope
            .path()
            .join(if cfg!(windows) { "gh.cmd" } else { "gh" });
        let body = if cfg!(windows) {
            "@echo off\r\nif not \"%HOME%\"==\"%EXPECTED_HOME%\" exit /b 21\r\nif not \"%GH_CONFIG_DIR%\"==\"%EXPECTED_LOGIN%\" exit /b 22\r\necho credential-environment-ok\r\n"
        } else {
            "#!/bin/sh\n[ \"$HOME\" = \"$EXPECTED_HOME\" ] || exit 21\n[ \"$GH_CONFIG_DIR\" = \"$EXPECTED_LOGIN\" ] || exit 22\nprintf 'credential-environment-ok\\n'\n"
        };
        fs::write(&script, body)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&script, fs::Permissions::from_mode(0o700))?;
        }
        for selected in [None, Some(native.as_path())] {
            let mut command = credential_command(&script, OsStr::new("github.com"), selected);
            if selected.is_none() {
                command.env("HOME", &scratch);
            }
            let expected = selected.unwrap_or(&scratch);
            let output = command
                .env("EXPECTED_HOME", expected)
                .env("GH_CONFIG_DIR", &login)
                .env("EXPECTED_LOGIN", &login)
                .output()?;
            assert!(output.status.success(), "credential fixture: {output:?}");
            assert_eq!(
                String::from_utf8_lossy(&output.stdout).trim(),
                "credential-environment-ok"
            );
        }
        Ok(())
    }

    #[test]
    fn installed_cli_discovery_respects_aliases_inside_release_archives() -> io::Result<()> {
        for archive in [
            "gh_2.87.3_macOS_arm64",
            "gh_2.87.3_linux_amd64",
            "gh_2.87.3_windows_amd64",
        ] {
            let scope = tempfile::tempdir()?;
            let data = scope.path().join("mise");
            let shims = data.join(SHIMS);
            let installations = data.join(INSTALLATIONS);
            let name = format!("gh{}", std::env::consts::EXE_SUFFIX);
            let selected = installations.join("2.87.3").join(archive).join(BIN);
            let other = installations.join("2.9.0").join(BIN);
            for directory in [&shims, &selected, &other] {
                fs::create_dir_all(directory)?;
                fs::write(directory.join(&name), "fixture executable")?;
            }
            fs::write(installations.join(LATEST), "./2.87.3")?;
            let discovery = std::env::join_paths([shims]).unwrap();
            assert_eq!(
                executable(&discovery)?,
                crate::paths::canonical(&selected.join(&name))?
            );
        }
        Ok(())
    }
}

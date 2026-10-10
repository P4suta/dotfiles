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

fn direct(candidate: &Path, name: &OsStr) -> io::Result<Option<PathBuf>> {
    if !candidate.is_file() {
        return Ok(None);
    }
    let executable = crate::paths::canonical(candidate)?;
    Ok((executable.file_name() == Some(name)
        && executable.parent().and_then(Path::file_name) != Some(OsStr::new(SHIMS)))
    .then_some(executable))
}

fn installed(data: &Path, name: &OsStr) -> io::Result<Option<PathBuf>> {
    let installations = data.join(INSTALLATIONS);
    let alias = installations.join(LATEST);
    if let Some(executable) = direct(&alias.join(BIN).join(name), name)? {
        return Ok(Some(executable));
    }
    if alias.is_file() {
        let target = fs::read_to_string(&alias)?;
        if let Some(executable) = direct(
            &installations.join(target.trim()).join(BIN).join(name),
            name,
        )? {
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
        if let Some(executable) = direct(&version.path().join(BIN).join(name), name)? {
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
    let path = std::env::var_os("PATH").unwrap_or_default();
    let directories =
        std::env::split_paths(&path).chain(crate::process::mise_shims(std::env::var_os));
    let discovery = std::env::join_paths(directories)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let mut command = crate::process::command(executable(&discovery)?);
    command.args(["auth", "token", "--hostname"]).arg(host);
    Ok(command)
}

use super::{POLICY, Policy};
use anyhow::{Context, Result, ensure};
use rules::bounded_rules::{Route, route};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

const BACKUP: &str = ".dotfiles-native";
const MARKER: &str = ".dotfiles-guard";
const DOCKER_MARKER: &str = "/.dockerenv";
const PODMAN_MARKER: &str = "/run/.containerenv";
#[cfg(unix)]
const ANY_EXECUTE: u32 = 0o111;

fn current_route() -> Route {
    route(
        std::env::var("GITHUB_ACTIONS").is_ok_and(|value| value == "true"),
        Path::new(DOCKER_MARKER).exists() || Path::new(PODMAN_MARKER).exists(),
    )
}

/// Whether this owner-side process must use the bounded container route.
#[must_use]
pub fn requires_container() -> bool {
    current_route() == Route::Refuse
}

fn beside(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    if cfg!(windows) && suffix == BACKUP {
        name.push(".exe");
    }
    path.with_file_name(name)
}

/// Dispatch a protected executable before the ordinary dotfiles command line.
///
/// # Errors
///
/// Returns failures resolving or executing the preserved native tool.
pub fn dispatch() -> Result<Option<std::process::ExitStatus>> {
    let executable = std::env::current_exe()?;
    let backup = beside(&executable, BACKUP);
    if !backup.is_file() {
        ensure!(
            !beside(&executable, MARKER).exists(),
            "protected tool lost its native backup; use `dotfiles bounded run` and repair its installation"
        );
        return Ok(None);
    }
    ensure!(
        current_route() != Route::Refuse,
        "native unbounded tool {} refused on an owner machine; use the project's bounded container entrypoint or `dotfiles bounded run --image <name@sha256:digest> --project . -- <project command>`; policy: {POLICY}",
        executable.display()
    );
    let mut command = platform::process::command(backup);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.arg0(executable);
    }
    Ok(Some(command.args(std::env::args_os().skip(1)).status()?))
}

fn marker(path: &Path) -> Result<String> {
    match fs::read_to_string(beside(path, MARKER)) {
        Ok(value) => Ok(value),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error.into()),
    }
}

fn preserve_native(path: &Path) -> Result<()> {
    let backup = beside(path, BACKUP);
    let current = platform::build_inputs::hex(&Sha256::digest(fs::read(path)?));
    let previous = marker(path)?;
    if previous.lines().any(|digest| digest == current) {
        ensure!(
            backup.is_file(),
            "protected tool {} lost its native backup",
            path.display()
        );
    } else {
        ensure!(
            !previous.is_empty() || !backup.exists(),
            "protected tool {} lost its guard identity; preserve its native backup and repair the installation",
            path.display()
        );
        platform::aside::install(path, &backup)?;
    }
    Ok(())
}

fn publish(path: &Path, runtime: &Path, digest: &str) -> Result<()> {
    let mut identities = marker(path)?;
    if !identities.lines().any(|known| known == digest) {
        if !identities.is_empty() {
            identities.push('\n');
        }
        identities.push_str(digest);
    }
    let mut staged = tempfile::NamedTempFile::new_in(path.parent().context("guard parent")?)?;
    staged.write_all(identities.as_bytes())?;
    staged.as_file().sync_all()?;
    platform::aside::replace(&staged.into_temp_path(), &beside(path, MARKER))?;
    platform::aside::install(runtime, path)?;
    Ok(())
}

fn executable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & ANY_EXECUTE != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn discover(directory: &Path, policy: &Policy, found: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            discover(&path, policy, found)?;
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                policy
                    .tools
                    .iter()
                    .any(|tool| name == tool || cfg!(windows) && name == format!("{tool}.exe"))
            })
            && executable(&path)
        {
            found.push(path);
        }
    }
    Ok(())
}

/// Protect all installed versions, including the executable directories mise prepends for tasks.
///
/// # Errors
///
/// Returns policy, discovery, backup, or atomic installation failures.
pub fn install() -> Result<()> {
    if current_route() != Route::Refuse {
        return Ok(());
    }
    let home = platform::home::home().context("bounded guard needs the native home")?;
    let runtime = std::env::current_exe()?;
    install_in(&home, &runtime, &Policy::load(None)?)
}

fn install_in(home: &Path, runtime: &Path, policy: &Policy) -> Result<()> {
    let mise = std::env::var_os("MISE_DATA_DIR").map_or_else(
        || {
            if cfg!(windows) {
                platform::home::local_app_data(home).join("mise")
            } else {
                home.join(".local/share/mise")
            }
        },
        PathBuf::from,
    );
    let cargo = std::env::var_os("CARGO_HOME").map_or_else(|| home.join(".cargo"), PathBuf::from);
    let mut found = Vec::new();
    for directory in [
        mise.join("installs"),
        cargo.join("bin"),
        home.join(".local/bin"),
        home.join(".kani"),
        home.join(".elan"),
    ] {
        discover(&directory, policy, &mut found)?;
    }
    let rustup =
        std::env::var_os("RUSTUP_HOME").map_or_else(|| home.join(".rustup"), PathBuf::from);
    match fs::read_dir(rustup.join("toolchains")) {
        Ok(toolchains) => {
            for toolchain in toolchains {
                discover(&toolchain?.path().join("bin"), policy, &mut found)?;
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let digest = platform::build_inputs::hex(&Sha256::digest(fs::read(runtime)?));
    for path in &found {
        preserve_native(path)?;
    }
    for path in &found {
        publish(path, runtime, &digest)?;
    }
    eprintln!("Protected {} unbounded tool executable(s)", found.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn protect(path: &Path, runtime: &Path, digest: &str) -> Result<()> {
        preserve_native(path)?;
        publish(path, runtime, digest)
    }

    #[cfg(unix)]
    #[test]
    fn discovery_skips_directory_and_dangling_symlinks_and_nonexecutables() -> Result<()> {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let scope = tempfile::tempdir()?;
        let directory = scope.path().join("tools");
        let package = scope.path().join("package");
        fs::create_dir(&directory)?;
        fs::create_dir(&package)?;
        let tool = directory.join("cargo-kani");
        let package_tool = package.join("kani");
        for executable in [&tool, &package_tool] {
            fs::write(executable, b"native")?;
            fs::set_permissions(executable, fs::Permissions::from_mode(0o755))?;
        }
        let alias = directory.join("kani-driver");
        symlink(&tool, &alias)?;
        symlink(&package, directory.join("quint"))?;
        symlink(scope.path().join("missing"), directory.join("cbmc"))?;
        fs::write(directory.join("z3"), b"not executable")?;

        let policy = Policy::parse(super::super::DEFAULT_POLICY)?;
        let mut found = Vec::new();
        discover(&directory, &policy, &mut found)?;
        found.sort();
        assert_eq!(found, vec![tool, alias]);
        Ok(())
    }

    #[test]
    fn repeated_install_and_tool_upgrades_preserve_the_current_native_binary() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let runtime = scope.path().join("dotfiles");
        let tool = scope.path().join("cargo-kani");
        fs::write(&runtime, b"guard one")?;
        fs::write(&tool, b"native one")?;
        let digest = platform::build_inputs::hex(&Sha256::digest(fs::read(&runtime)?));
        protect(&tool, &runtime, &digest)?;
        protect(&tool, &runtime, &digest)?;
        assert_eq!(fs::read(beside(&tool, BACKUP))?, b"native one");
        fs::write(&tool, b"native two")?;
        protect(&tool, &runtime, &digest)?;
        assert_eq!(fs::read(beside(&tool, BACKUP))?, b"native two");
        assert_eq!(fs::read(&tool)?, b"guard one");
        Ok(())
    }

    #[test]
    fn interrupted_runtime_updates_keep_the_preserved_native_tool() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let runtime = scope.path().join("dotfiles");
        let tool = scope.path().join("cargo-kani");
        fs::write(&runtime, b"guard one")?;
        fs::write(&tool, b"native")?;
        let first = platform::build_inputs::hex(&Sha256::digest(fs::read(&runtime)?));
        protect(&tool, &runtime, &first)?;
        fs::write(&runtime, b"guard two")?;
        let second = platform::build_inputs::hex(&Sha256::digest(fs::read(&runtime)?));
        assert!(publish(&tool, &scope.path().join("missing-runtime"), &second).is_err());
        protect(&tool, &runtime, &second)?;
        assert_eq!(fs::read(beside(&tool, BACKUP))?, b"native");
        assert_eq!(fs::read(tool)?, b"guard two");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn alias_backups_precede_every_guard_replacement() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let runtime = scope.path().join("dotfiles");
        let driver = scope.path().join("kani-driver");
        let alias = scope.path().join("kani");
        fs::write(&runtime, b"guard")?;
        fs::write(&driver, b"native")?;
        std::os::unix::fs::symlink(&driver, &alias)?;
        let digest = platform::build_inputs::hex(&Sha256::digest(fs::read(&runtime)?));
        for path in [&driver, &alias] {
            preserve_native(path)?;
        }
        for path in [&driver, &alias] {
            publish(path, &runtime, &digest)?;
        }
        assert_eq!(fs::read(beside(&driver, BACKUP))?, b"native");
        assert_eq!(fs::read(beside(&alias, BACKUP))?, b"native");
        Ok(())
    }
}

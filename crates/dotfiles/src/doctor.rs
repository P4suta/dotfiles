//! The host drift `dotfiles doctor` reports beside missing tools: hand-installed copies that hide a managed tool, an applied source off the merged main, and an installed runtime built from other sources.

use crate::runtime::{Real, Runner, args};
use crate::tool::Tool;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// The user binary directories, which hold hand-installed copies that can hide a managed tool.
const USER_BINARIES: [&str; 2] = [".local/bin", ".cargo/bin"];
/// Programs a managed wrapper places ahead of the real tool on purpose: the Git guard.
const WRAPPERS: [&str; 1] = ["git"];
/// The extensions Windows resolves a bare command name to.
const WINDOWS_COMMANDS: [&str; 3] = ["exe", "cmd", "bat"];
/// The branch every host applies, and its last fetched remote state.
const MAIN: &str = "main";
const ORIGIN_MAIN: &str = "origin/main";
/// The directory beside the mise shims that holds each installed tool version.
const MISE_INSTALLS: &str = "installs";
/// The installed runtime, which every hook and launcher runs.
const RUNTIME: &str = ".local/bin/dotfiles";
/// The revision digits a report shows, as many as a short Git revision.
const SHORT_REVISION: usize = 12;

/// The command name a binary answers to: its filename, without an executable extension on Windows.
fn command_name(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    if cfg!(windows) {
        let (stem, extension) = name.rsplit_once('.')?;
        WINDOWS_COMMANDS
            .iter()
            .any(|known| extension.eq_ignore_ascii_case(known))
            .then(|| stem.to_ascii_lowercase())
    } else {
        Some(name.to_owned())
    }
}

/// The file in `directory` that the command `name` resolves to, if any.
fn command_in(directory: &Path, name: &str) -> Option<PathBuf> {
    if cfg!(windows) {
        WINDOWS_COMMANDS
            .iter()
            .map(|extension| directory.join(format!("{name}.{extension}")))
            .find(|path| path.is_file())
    } else {
        Some(directory.join(name)).filter(|path| path.is_file())
    }
}

#[cfg(unix)]
fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    /// Any execute bit.
    const EXECUTE: u32 = 0o111;
    std::fs::metadata(path).is_ok_and(|metadata| metadata.permissions().mode() & EXECUTE != 0)
}

#[cfg(not(unix))]
fn executable(path: &Path) -> bool {
    path.is_file()
}

fn same_file(left: &Path, right: &Path) -> bool {
    match (crate::canonical(left), crate::canonical(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

/// The tool a mise shim stands for under the host's global configuration, since a project's pin applies only inside that project.
/// An inactive shim makes mise print an error, which the report replaces by skipping the shim.
fn global_mise_tool(native: &Real, name: &str) -> Result<Option<PathBuf>> {
    let output = native
        .command(Tool::Mise)
        .args(["which", name])
        .current_dir(&native.home)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()?;
    Ok(output
        .status
        .success()
        .then(|| PathBuf::from(String::from_utf8_lossy(&output.stdout).trim())))
}

/// The copy of `name` that `path` hides further along the managed `PATH`.
/// A mise shim stands for the tool `mise which` resolves, and a shim mise leaves inactive hides nothing.
/// The tool directories a mise session adds for one project's pins hide nothing at the host level either.
fn hidden_copy(
    native: &Real,
    later: &[PathBuf],
    path: &Path,
    name: &str,
) -> Result<Option<PathBuf>> {
    let shims = platform::home::mise_shims(&native.home);
    let installs = shims.parent().map(|data| data.join(MISE_INSTALLS));
    for directory in later {
        if installs
            .as_deref()
            .is_some_and(|installs| directory.starts_with(installs))
        {
            continue;
        }
        let Some(candidate) = command_in(directory, name) else {
            continue;
        };
        if *directory != shims {
            if same_file(path, &candidate) {
                continue;
            }
            return Ok(Some(candidate));
        }
        if let Some(resolved) = global_mise_tool(native, name)? {
            return Ok((!same_file(path, &resolved)).then_some(resolved));
        }
    }
    Ok(None)
}

/// Prints and counts the programs in a user binary directory that hide another copy of the same tool on the managed `PATH`.
/// A stale hand-installed chezmoi once hid the pinned one this way.
///
/// # Errors
///
/// Returns an error when mise can't start.
pub fn shadowed(native: &Real) -> Result<usize> {
    let directories: Vec<PathBuf> = std::env::split_paths(&native.path).collect();
    let mut found = 0;
    for user in USER_BINARIES.map(|relative| native.home.join(relative)) {
        let Some(position) = directories.iter().position(|directory| *directory == user) else {
            continue;
        };
        let Ok(entries) = std::fs::read_dir(&user) else {
            continue;
        };
        let mut programs: Vec<(String, PathBuf)> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_file() && executable(path))
            .filter_map(|path| Some((command_name(&path)?, path)))
            .filter(|(name, _)| !WRAPPERS.contains(&name.as_str()))
            .collect();
        programs.sort();
        for (name, path) in programs {
            let later = &directories[position + 1..];
            if let Some(hidden) = hidden_copy(native, later, &path, &name)? {
                println!(
                    "{name}: {} hides {}; move the first aside",
                    path.display(),
                    hidden.display()
                );
                found += 1;
            }
        }
    }
    Ok(found)
}

fn git(native: &mut Real, root: &Path, arguments: &[&str]) -> Result<Option<String>> {
    let mut command = args(&["-C"]);
    command.push(root.into());
    command.extend(args(arguments));
    let reply = native.run(Tool::Git, &command)?;
    Ok(reply
        .success
        .then(|| String::from_utf8_lossy(&reply.bytes).trim().to_owned()))
}

/// Prints and counts how the applied source strays from the last fetched main, and how the installed runtime strays from the applied source.
///
/// # Errors
///
/// Returns an error when chezmoi or Git can't start, or when it can't read the source's build inputs.
pub fn source_drift(native: &mut Real) -> Result<usize> {
    let reply = native.run(Tool::Chezmoi, &args(&["source-path"]))?;
    if !reply.success {
        println!("chezmoi: the applied source is unknown");
        return Ok(1);
    }
    let source = PathBuf::from(String::from_utf8(reply.bytes)?.trim());
    let Some(top) = git(native, &source, &["rev-parse", "--show-toplevel"])? else {
        return Ok(0);
    };
    let top = PathBuf::from(top);
    let mut found = 0;
    let branch = git(
        native,
        &top,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
    )?;
    if branch.as_deref() != Some(MAIN) {
        println!(
            "source: {} is on {} instead of {MAIN}",
            top.display(),
            branch.as_deref().unwrap_or("a detached head")
        );
        found += 1;
    }
    let range = format!("{ORIGIN_MAIN}...HEAD");
    if let Some(counts) = git(
        native,
        &top,
        &["rev-list", "--left-right", "--count", &range],
    )? {
        let mut counts = counts.split_whitespace().map(str::parse::<u64>);
        if let (Some(Ok(behind)), Some(Ok(ahead))) = (counts.next(), counts.next()) {
            if behind > 0 {
                println!(
                    "source: {} is {behind} commits behind {ORIGIN_MAIN}; pull it and apply",
                    top.display()
                );
                found += 1;
            }
            if ahead > 0 {
                println!(
                    "source: {} holds {ahead} commits that {ORIGIN_MAIN} lacks",
                    top.display()
                );
                found += 1;
            }
        }
    }
    if crate::freshness::is_checkout(&top) {
        found += runtime_drift(native, &top)?;
    }
    Ok(found)
}

fn runtime_drift(native: &Real, top: &Path) -> Result<usize> {
    let runtime = native
        .home
        .join(format!("{RUNTIME}{}", std::env::consts::EXE_SUFFIX));
    // A missing runtime, or one older than its build identity, falls to the report below.
    let stamp = native
        .command_installed(&runtime)
        .arg(crate::freshness::BUILD_IDENTITY_ARGUMENT)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            serde_json::from_slice::<crate::freshness::Embedded>(&output.stdout).ok()
        });
    let current = crate::build_inputs::content_hash(top)?;
    match stamp {
        Some(stamp) if stamp.hash == current => Ok(0),
        Some(stamp) => {
            println!(
                "dotfiles: {} was built from revision {}, whose sources differ from {}; apply the source to reinstall it",
                runtime.display(),
                stamp
                    .revision
                    .chars()
                    .take(SHORT_REVISION)
                    .collect::<String>(),
                top.display()
            );
            Ok(1)
        }
        None => {
            println!(
                "dotfiles: {} is missing or reports no build identity; apply the source to reinstall it",
                runtime.display()
            );
            Ok(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `rwxr-xr-x`, as an installed program has.
    #[cfg(unix)]
    const PROGRAM_MODE: u32 = 0o755;

    #[cfg(unix)]
    fn program(directory: &Path, name: &str, content: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(directory).unwrap();
        let path = directory.join(name);
        std::fs::write(&path, content).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(PROGRAM_MODE)).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn a_user_copy_that_hides_a_later_copy_is_reported_and_wrappers_and_lone_copies_are_not() {
        let fixture = tempfile::tempdir().unwrap();
        let home = fixture.path();
        let system = home.join("system");
        program(&home.join(".local/bin"), "gh", "stale");
        program(&system, "gh", "current");
        program(&home.join(".local/bin"), "git", "guard");
        program(&system, "git", "git");
        program(&home.join(".local/bin"), "dotfiles", "runtime");
        let mut native = Real::new(home).unwrap();
        native.path = std::env::join_paths([home.join(".local/bin"), system.clone()]).unwrap();
        assert_eq!(shadowed(&native).unwrap(), 1);
        std::fs::remove_file(home.join(".local/bin/gh")).unwrap();
        assert_eq!(shadowed(&native).unwrap(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn a_repeated_directory_or_link_to_the_same_file_hides_nothing() {
        let fixture = tempfile::tempdir().unwrap();
        let home = fixture.path();
        let user = home.join(".local/bin");
        let tool = program(&home.join("store"), "tool", "tool");
        std::fs::create_dir_all(&user).unwrap();
        std::os::unix::fs::symlink(&tool, user.join("tool")).unwrap();
        let mut native = Real::new(home).unwrap();
        native.path = std::env::join_paths([user.clone(), user, home.join("store")]).unwrap();
        assert_eq!(shadowed(&native).unwrap(), 0);
    }

    #[test]
    fn windows_command_names_drop_the_executable_extension() {
        if cfg!(windows) {
            assert_eq!(command_name(Path::new("Gh.EXE")).as_deref(), Some("gh"));
            assert_eq!(command_name(Path::new("notes.txt")), None);
        } else {
            assert_eq!(command_name(Path::new("gh")).as_deref(), Some("gh"));
        }
    }

    /// A runtime counts as current only when its build identity carries the source's content hash.
    #[cfg(unix)]
    #[test]
    fn the_installed_runtime_matches_only_its_own_sources() {
        let fixture = tempfile::tempdir().unwrap();
        let home = fixture.path().join("home");
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|directory| crate::freshness::is_checkout(directory))
            .unwrap();
        let current = crate::build_inputs::content_hash(source).unwrap();
        let native = Real::new(&home).unwrap();
        assert_eq!(
            runtime_drift(&native, source).unwrap(),
            1,
            "missing runtime"
        );
        for (hash, expected) in [
            (current.as_str(), 0),
            ("0".repeat(current.len()).as_str(), 1),
        ] {
            program(
                &home.join(".local/bin"),
                "dotfiles",
                &format!(
                    "#!/bin/sh\nprintf '%s' '{{\"revision\":\"r\",\"root\":\"/r\",\"hash\":\"{hash}\"}}'\n"
                ),
            );
            assert_eq!(runtime_drift(&native, source).unwrap(), expected, "{hash}");
        }
        program(&home.join(".local/bin"), "dotfiles", "#!/bin/sh\nexit 2\n");
        assert_eq!(runtime_drift(&native, source).unwrap(), 1, "no identity");
    }
}

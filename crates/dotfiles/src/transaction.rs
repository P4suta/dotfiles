use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
struct Entry {
    path: PathBuf,
    existed: bool,
}

pub(crate) fn relative(path: &Path) -> Result<()> {
    ensure!(
        !path.as_os_str().is_empty()
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "backup target must be a nonempty relative path"
    );
    Ok(())
}

/// Windows removes a directory symlink or junction with `remove_dir` and denies `remove_file` on it.
fn is_directory_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTypeExt;
        metadata.file_type().is_symlink_dir()
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

fn remove(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => fs::remove_dir_all(path)?,
        Ok(metadata) if is_directory_link(&metadata) => fs::remove_dir(path)?,
        Ok(_) => fs::remove_file(path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

/// Whether `current` already matches the captured `saved` tree, so restoring it would change nothing.
fn unchanged(saved: &Path, current: &Path) -> Result<bool> {
    let saved_metadata = fs::symlink_metadata(saved)?;
    let current_metadata = match fs::symlink_metadata(current) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if saved_metadata.is_symlink() || current_metadata.is_symlink() {
        return Ok(saved_metadata.is_symlink()
            && current_metadata.is_symlink()
            && fs::read_link(saved)? == fs::read_link(current)?);
    }
    if saved_metadata.permissions() != current_metadata.permissions() {
        return Ok(false);
    }
    if saved_metadata.is_dir() || current_metadata.is_dir() {
        if !(saved_metadata.is_dir() && current_metadata.is_dir()) {
            return Ok(false);
        }
        let mut saved_names = fs::read_dir(saved)?
            .map(|entry| Ok(entry?.file_name()))
            .collect::<Result<Vec<_>>>()?;
        let mut current_names = fs::read_dir(current)?
            .map(|entry| Ok(entry?.file_name()))
            .collect::<Result<Vec<_>>>()?;
        saved_names.sort();
        current_names.sort();
        if saved_names != current_names {
            return Ok(false);
        }
        for name in saved_names {
            if !unchanged(&saved.join(&name), &current.join(&name))? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    Ok(current_metadata.is_file() && fs::read(saved)? == fs::read(current)?)
}

fn copy(source: &Path, destination: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    fs::create_dir_all(destination.parent().context("backup parent is missing")?)?;
    if metadata.is_symlink() {
        let target = fs::read_link(source)?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, destination)?;
        #[cfg(windows)]
        if source.is_dir() {
            std::os::windows::fs::symlink_dir(target, destination)?;
        } else {
            std::os::windows::fs::symlink_file(target, destination)?;
        }
    } else if metadata.is_dir() {
        fs::create_dir(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy(&entry.path(), &destination.join(entry.file_name()))?;
        }
        sync_directory(destination)?;
        fs::set_permissions(destination, metadata.permissions())?;
    } else {
        ensure!(metadata.is_file(), "backup refuses special files");
        fs::copy(source, destination)?;
        // Windows flushes only a handle open for writing, so the copy syncs before it takes a read-only mode.
        fs::File::options()
            .write(true)
            .open(destination)?
            .sync_all()?;
        fs::set_permissions(destination, metadata.permissions())?;
    }
    Ok(())
}

/// Syncs a sibling file and renames it into place.
/// On Unix, syncs the parent directory.
fn write_durably(path: &Path, bytes: &[u8]) -> Result<()> {
    let partial = path.with_extension("partial");
    let mut file = fs::File::create(&partial)?;
    std::io::Write::write_all(&mut file, bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&partial, path)?;
    sync_directory(path.parent().context("a durable file has a parent")?)
}

/// Syncs the directory on Unix after a rename.
#[cfg_attr(
    not(unix),
    expect(
        clippy::unnecessary_wraps,
        reason = "the Unix build syncs the directory, which can fail"
    )
)]
fn sync_directory(directory: &Path) -> Result<()> {
    #[cfg(unix)]
    fs::File::open(directory)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = directory;
    Ok(())
}

pub(crate) fn capture(destination: &Path, backup: &Path, targets: &[PathBuf]) -> Result<()> {
    ensure!(!backup.exists(), "backup must use a fresh directory");
    ensure!(!targets.is_empty(), "backup requires managed targets");
    let destination = crate::canonical(destination)?;
    let backup = crate::canonical(backup.parent().context("backup parent is missing")?)?
        .join(backup.file_name().context("backup name is missing")?);
    let mut targets = targets.to_vec();
    targets.sort();
    targets.dedup();
    for target in &targets {
        relative(target)?;
        ensure!(
            !backup.starts_with(destination.join(target)),
            "backup must be outside every captured target"
        );
        for ancestor in target
            .ancestors()
            .skip(1)
            .filter(|path| !path.as_os_str().is_empty())
        {
            if let Ok(metadata) = fs::symlink_metadata(destination.join(ancestor)) {
                ensure!(!metadata.is_symlink(), "backup refuses a symlink parent");
            }
        }
    }
    let roots: Vec<_> = targets
        .iter()
        .filter(|path| {
            !targets
                .iter()
                .any(|parent| *path != parent && path.starts_with(parent))
        })
        .collect();
    fs::create_dir(&backup)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // `rwx------`: the backup holds the owner's configuration, which can include secrets.
        const OWNER_ONLY: u32 = 0o700;
        fs::set_permissions(&backup, fs::Permissions::from_mode(OWNER_ONLY))?;
    }
    let mut entries = Vec::new();
    for path in roots {
        let source = destination.join(path);
        let existed = match fs::symlink_metadata(&source) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.into()),
        };
        if existed {
            let copied = backup.join("files").join(path);
            copy(&source, &copied)?;
            // Each directory down to the copy records its entry.
            for directory in copied.ancestors().skip(1) {
                sync_directory(directory)?;
                if directory == backup {
                    break;
                }
            }
        }
        entries.push(Entry {
            path: path.clone(),
            existed,
        });
    }
    // Capture syncs the backup before writing the manifest.
    // Targets change only after capture succeeds.
    write_durably(
        &backup.join("destination.txt"),
        destination.as_os_str().as_encoded_bytes(),
    )?;
    write_durably(
        &backup.join("manifest.json"),
        &serde_json::to_vec_pretty(&entries)?,
    )?;
    Ok(())
}

pub(crate) fn restore(destination: &Path, backup: &Path) -> Result<()> {
    let destination = crate::canonical(destination)?;
    ensure!(
        fs::read(backup.join("destination.txt"))? == destination.as_os_str().as_encoded_bytes(),
        "backup destination identity differs"
    );
    let entries: Vec<Entry> = serde_json::from_slice(&fs::read(backup.join("manifest.json"))?)?;
    for entry in &entries {
        relative(&entry.path)?;
        for ancestor in entry
            .path
            .ancestors()
            .skip(1)
            .filter(|path| !path.as_os_str().is_empty())
        {
            if let Ok(metadata) = fs::symlink_metadata(destination.join(ancestor)) {
                ensure!(!metadata.is_symlink(), "restore refuses a symlink parent");
            }
        }
    }
    for entry in entries {
        let target = destination.join(&entry.path);
        let saved = backup.join("files").join(&entry.path);
        // Untouched targets stay in place: rewriting them gains nothing and can fail on a file another process holds open.
        if entry.existed && unchanged(&saved, &target)? {
            continue;
        }
        remove(&target).with_context(|| format!("restore {}", entry.path.display()))?;
        if entry.existed {
            copy(&saved, &target).with_context(|| format!("restore {}", entry.path.display()))?;
        }
    }
    Ok(())
}

pub(crate) fn apply(
    destination: &Path,
    backup: &Path,
    targets: &[PathBuf],
    apply: impl FnOnce() -> Result<()>,
    verify: impl FnOnce() -> Result<()>,
) -> Result<()> {
    capture(destination, backup, targets)?;
    if let Err(failure) = apply().and_then(|()| verify()) {
        restore(destination, backup)
            .with_context(|| format!("application failed ({failure:#}); rollback also failed"))?;
        anyhow::bail!("application failed and managed files were restored: {failure:#}");
    }
    Ok(())
}

pub(crate) fn apply_profile(
    destination: &Path,
    backup: &Path,
    targets: &[PathBuf],
    state_file: &Path,
    apply_files: impl FnOnce() -> Result<()>,
    verify: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let previous = match fs::read(state_file) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let result = apply(
        destination,
        backup,
        targets,
        || {
            if let Some(bytes) = &previous {
                fs::write(backup.join("previous-state.boltdb"), bytes)?;
            }
            apply_files()
        },
        verify,
    );
    if let Err(error) = result {
        if let Some(bytes) = previous {
            crate::runtime::replace(state_file, &bytes, false)?;
        } else if state_file.exists() {
            fs::remove_file(state_file)?;
        }
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[cfg(unix)]
    const MODE_MASK: u32 = 0o7777;

    /// What a path holds: a directory, a file's bytes and Unix mode, or a link's target.
    #[derive(Debug, PartialEq)]
    enum Node {
        Directory(Option<u32>),
        File(Vec<u8>, Option<u32>),
        Link(PathBuf),
    }

    /// Every path under `root` with its bytes, mode, or link target.
    fn tree(root: &Path) -> BTreeMap<PathBuf, Node> {
        fn walk(root: &Path, directory: &Path, found: &mut BTreeMap<PathBuf, Node>) {
            for entry in fs::read_dir(directory).unwrap() {
                let path = entry.unwrap().path();
                let metadata = fs::symlink_metadata(&path).unwrap();
                #[cfg(unix)]
                let mode = {
                    use std::os::unix::fs::PermissionsExt;
                    Some(metadata.permissions().mode() & MODE_MASK)
                };
                #[cfg(not(unix))]
                let mode = None;
                let relative = path.strip_prefix(root).unwrap().to_path_buf();
                if metadata.is_symlink() {
                    found.insert(relative, Node::Link(fs::read_link(&path).unwrap()));
                } else if metadata.is_dir() {
                    found.insert(relative, Node::Directory(mode));
                    walk(root, &path, found);
                } else {
                    found.insert(relative, Node::File(fs::read(&path).unwrap(), mode));
                }
            }
        }
        let mut found = BTreeMap::new();
        walk(root, root, &mut found);
        found
    }

    /// One step of the apply under test, which writes, replaces, removes, links, or changes a mode.
    type Step = fn(&Path) -> std::io::Result<()>;

    #[cfg(unix)]
    fn executable(path: &Path) -> std::io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o750))
    }

    #[cfg(not(unix))]
    #[expect(
        clippy::unnecessary_wraps,
        reason = "each step of the apply returns the result the Unix mode change can fail with"
    )]
    fn executable(_path: &Path) -> std::io::Result<()> {
        Ok(())
    }

    fn link(target: &str, path: &Path) -> std::io::Result<()> {
        #[cfg(unix)]
        return std::os::unix::fs::symlink(target, path);
        #[cfg(windows)]
        return std::os::windows::fs::symlink_file(target, path);
    }

    /// The steps of an apply, each repeatable, so a second apply finds nothing to change.
    const STEPS: [Step; 8] = [
        |home| fs::write(home.join("kept"), "after"),
        |home| executable(&home.join("kept")),
        |home| fs::create_dir_all(home.join("tree/nested")),
        |home| fs::write(home.join("tree/nested/new"), "created"),
        |home| match fs::remove_file(home.join("tree/old")) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            result => result,
        },
        |home| fs::write(home.join("fresh"), "created"),
        |home| match fs::remove_file(home.join("pointer")) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            result => result,
        },
        |home| match link("fresh", &home.join("pointer")) {
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            result => result,
        },
    ];

    const TARGETS: [&str; 5] = ["kept", "tree", "tree/nested/new", "fresh", "pointer"];

    /// A home holding a file, a tree with an old file, a link, and an unmanaged file.
    fn home(scope: &Path) -> PathBuf {
        let home = scope.join("home");
        fs::create_dir_all(home.join("tree")).unwrap();
        fs::write(home.join("kept"), "before").unwrap();
        fs::write(home.join("tree/old"), "old").unwrap();
        fs::write(home.join("unmanaged"), "theirs").unwrap();
        link("kept", &home.join("pointer")).unwrap();
        home
    }

    /// The apply fails after each of its steps in turn, and every rollback restores bytes, modes, and links, removes new targets, and changes nothing when it runs again.
    /// A finished apply changes nothing when it runs a second and a third time.
    #[test]
    fn every_crash_point_of_an_apply_rolls_back_to_the_same_tree() {
        let targets: Vec<PathBuf> = TARGETS.iter().map(PathBuf::from).collect();
        for crash in 0..=STEPS.len() {
            let scope = tempfile::tempdir().unwrap();
            let home = home(scope.path());
            let before = tree(&home);
            let backup = scope.path().join("backup");
            let result = apply(
                &home,
                &backup,
                &targets,
                || {
                    for step in STEPS.iter().take(crash) {
                        step(&home)?;
                    }
                    anyhow::bail!("injected crash after step {crash}")
                },
                || Ok(()),
            );
            assert!(result.unwrap_err().to_string().contains("restored"));
            assert_eq!(tree(&home), before, "crash after step {crash}");
            restore(&home, &backup).unwrap();
            assert_eq!(tree(&home), before, "second restore after step {crash}");
        }
        let scope = tempfile::tempdir().unwrap();
        let home = home(scope.path());
        let mut applied = None;
        for run in 0..3 {
            apply(
                &home,
                &scope.path().join(format!("backup-{run}")),
                &targets,
                || {
                    for step in STEPS {
                        step(&home)?;
                    }
                    Ok(())
                },
                || Ok(()),
            )
            .unwrap();
            let after = tree(&home);
            assert!(
                applied.as_ref().is_none_or(|first| *first == after),
                "run {run}"
            );
            applied = Some(after);
        }
    }

    #[test]
    fn a_capture_leaves_its_manifest_whole_and_no_partial_file() {
        let scope = tempfile::tempdir().unwrap();
        let home = home(scope.path());
        let backup = scope.path().join("backup");
        capture(&home, &backup, &["kept".into(), "tree".into()]).unwrap();
        let entries: Vec<Entry> =
            serde_json::from_slice(&fs::read(backup.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(fs::read_dir(&backup).unwrap().all(|entry| {
            entry
                .unwrap()
                .path()
                .extension()
                .is_none_or(|extension| extension != "partial")
        }));
    }

    #[test]
    fn partial_apply_and_verification_failure_restore_existing_and_remove_new_targets() {
        for apply_fails in [true, false] {
            let scope = tempfile::tempdir().unwrap();
            let destination = scope.path().join("home");
            fs::create_dir(&destination).unwrap();
            fs::write(destination.join("old"), "before").unwrap();
            fs::write(destination.join("auth.json"), "unmanaged").unwrap();
            let backup = scope.path().join("backup");
            let result = apply(
                &destination,
                &backup,
                &["old".into(), "new".into()],
                || {
                    fs::write(destination.join("old"), "after")?;
                    fs::write(destination.join("new"), "created")?;
                    ensure!(!apply_fails, "injected apply failure");
                    Ok(())
                },
                || anyhow::bail!("injected verification failure"),
            );
            assert!(result.is_err());
            assert_eq!(
                fs::read_to_string(destination.join("old")).unwrap(),
                "before"
            );
            assert!(!destination.join("new").exists());
            assert_eq!(
                fs::read_to_string(destination.join("auth.json")).unwrap(),
                "unmanaged"
            );
            assert!(backup.join("manifest.json").is_file());
        }
    }

    #[test]
    fn profile_failure_restores_existing_state_or_removes_new_state() -> Result<()> {
        for existing_state in [false, true] {
            for apply_fails in [false, true] {
                let scope = tempfile::tempdir()?;
                let destination = scope.path().join("home");
                fs::create_dir(&destination)?;
                fs::write(destination.join("config"), "original configuration")?;
                let state = scope.path().join("state.boltdb");
                if existing_state {
                    fs::write(&state, "original state")?;
                }
                let backup = scope.path().join("backup");
                let result = apply_profile(
                    &destination,
                    &backup,
                    &["config".into()],
                    &state,
                    || {
                        fs::write(destination.join("config"), "candidate configuration")?;
                        fs::write(&state, "candidate state")?;
                        ensure!(!apply_fails, "injected apply failure");
                        Ok(())
                    },
                    || anyhow::bail!("injected verification failure"),
                );
                assert!(result.is_err());
                assert_eq!(
                    fs::read_to_string(destination.join("config"))?,
                    "original configuration"
                );
                if existing_state {
                    assert_eq!(fs::read_to_string(&state)?, "original state");
                    assert_eq!(
                        fs::read_to_string(backup.join("previous-state.boltdb"))?,
                        "original state"
                    );
                } else {
                    assert!(!state.exists());
                }
            }
        }
        Ok(())
    }

    #[test]
    fn backup_refuses_path_escape_and_wrong_destination() {
        let scope = tempfile::tempdir().unwrap();
        let home = scope.path().join("home");
        fs::create_dir(&home).unwrap();
        for path in ["../secret", "/etc/passwd", ""] {
            assert!(capture(&home, &scope.path().join("backup"), &[path.into()]).is_err());
        }
        capture(&home, &scope.path().join("backup"), &["new".into()]).unwrap();
        assert!(restore(scope.path(), &scope.path().join("backup")).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn rollback_removes_a_directory_symlink_it_created() {
        let scope = tempfile::tempdir().unwrap();
        let destination = scope.path().join("home");
        fs::create_dir_all(destination.join("skills/shared")).unwrap();
        let backup = scope.path().join("backup");
        let result = apply(
            &destination,
            &backup,
            &["linked".into()],
            || {
                std::os::windows::fs::symlink_dir(
                    destination.join("skills/shared"),
                    destination.join("linked"),
                )?;
                anyhow::bail!("injected apply failure")
            },
            || Ok(()),
        );
        assert!(result.unwrap_err().to_string().contains("restored"));
        assert!(fs::symlink_metadata(destination.join("linked")).is_err());
        assert!(destination.join("skills/shared").is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn rollback_leaves_untouched_targets_that_cannot_be_rewritten() {
        use std::os::unix::fs::PermissionsExt;
        let scope = tempfile::tempdir().unwrap();
        let destination = scope.path().join("home");
        fs::create_dir_all(destination.join("held")).unwrap();
        fs::write(destination.join("held/config"), "kept").unwrap();
        fs::write(destination.join("changed"), "before").unwrap();
        let backup = scope.path().join("backup");
        let result = apply(
            &destination,
            &backup,
            &["held/config".into(), "changed".into()],
            || {
                fs::write(destination.join("changed"), "after")?;
                fs::set_permissions(destination.join("held"), fs::Permissions::from_mode(0o500))?;
                anyhow::bail!("injected apply failure")
            },
            || Ok(()),
        );
        fs::set_permissions(destination.join("held"), fs::Permissions::from_mode(0o700)).unwrap();
        assert!(result.unwrap_err().to_string().contains("restored"));
        assert_eq!(
            fs::read_to_string(destination.join("changed")).unwrap(),
            "before"
        );
        assert_eq!(
            fs::read_to_string(destination.join("held/config")).unwrap(),
            "kept"
        );
    }

    #[test]
    fn backup_inside_home_must_stay_outside_captured_targets() {
        let scope = tempfile::tempdir().unwrap();
        let home = scope.path().join("home");
        fs::create_dir_all(home.join("managed")).unwrap();
        assert!(capture(&home, &home.join("managed/backup"), &["managed".into()]).is_err());
        assert!(capture(&home, &home.join("pending"), &["pending".into()]).is_err());
        capture(&home, &home.join("backup"), &["managed".into()]).unwrap();
        assert!(home.join("backup/manifest.json").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_and_executable_permissions_survive_rollback() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let scope = tempfile::tempdir().unwrap();
        let home = scope.path().join("home");
        fs::create_dir(&home).unwrap();
        fs::write(home.join("hook"), "original").unwrap();
        fs::set_permissions(home.join("hook"), fs::Permissions::from_mode(0o750)).unwrap();
        symlink("hook", home.join("link")).unwrap();
        let backup = scope.path().join("backup");
        capture(&home, &backup, &["hook".into(), "link".into()]).unwrap();
        fs::remove_file(home.join("link")).unwrap();
        restore(&home, &backup).unwrap();
        assert_eq!(fs::read_link(home.join("link")).unwrap(), Path::new("hook"));
        assert_eq!(
            fs::metadata(home.join("hook"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o750
        );
        symlink(scope.path(), home.join("escape")).unwrap();
        assert!(capture(&home, &scope.path().join("bad"), &["escape/secret".into()]).is_err());
    }
}

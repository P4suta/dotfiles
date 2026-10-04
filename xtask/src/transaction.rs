use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
struct Entry {
    path: PathBuf,
    existed: bool,
}

pub fn relative(path: &Path) -> Result<()> {
    ensure!(
        !path.as_os_str().is_empty()
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "backup target must be a nonempty relative path"
    );
    Ok(())
}

fn remove(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => fs::remove_dir_all(path)?,
        Ok(_) => fs::remove_file(path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
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
        fs::set_permissions(destination, metadata.permissions())?;
    } else {
        ensure!(metadata.is_file(), "backup refuses special files");
        fs::copy(source, destination)?;
        fs::set_permissions(destination, metadata.permissions())?;
    }
    Ok(())
}

pub fn capture(destination: &Path, backup: &Path, targets: &[PathBuf]) -> Result<()> {
    ensure!(!backup.exists(), "backup must use a fresh directory");
    ensure!(!targets.is_empty(), "backup requires managed targets");
    let destination = destination.canonicalize()?;
    let parent = backup
        .parent()
        .context("backup parent is missing")?
        .canonicalize()?;
    let mut targets = targets.to_vec();
    targets.sort();
    targets.dedup();
    for target in &targets {
        relative(target)?;
        ensure!(
            !parent.starts_with(destination.join(target)),
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
    fs::create_dir(backup)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(backup, fs::Permissions::from_mode(0o700))?;
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
            copy(&source, &backup.join("files").join(path))?;
        }
        entries.push(Entry {
            path: path.clone(),
            existed,
        });
    }
    fs::write(
        backup.join("manifest.json"),
        serde_json::to_vec_pretty(&entries)?,
    )?;
    fs::write(
        backup.join("destination.txt"),
        destination.as_os_str().as_encoded_bytes(),
    )?;
    Ok(())
}

pub fn restore(destination: &Path, backup: &Path) -> Result<()> {
    let destination = destination.canonicalize()?;
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
        remove(&target)?;
        if entry.existed {
            copy(&backup.join("files").join(entry.path), &target)?;
        }
    }
    Ok(())
}

pub fn apply(
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn backup_inside_home_must_stay_outside_captured_targets() {
        let scope = tempfile::tempdir().unwrap();
        let home = scope.path().join("home");
        fs::create_dir_all(home.join("managed")).unwrap();
        assert!(capture(&home, &home.join("managed/backup"), &["managed".into()]).is_err());
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

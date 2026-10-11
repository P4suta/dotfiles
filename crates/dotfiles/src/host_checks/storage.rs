use anyhow::{Context, Result, ensure};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

const SYSTEM_CONFIG_DIR: &str = "/etc/mise";
pub(super) const DATA_CHILDREN: [(&str, &str); 4] = [
    ("MISE_INSTALLS_DIR", "installs"),
    ("MISE_DOWNLOADS_DIR", "downloads"),
    ("MISE_PLUGINS_DIR", "plugins"),
    ("MISE_SHIMS_DIR", "shims"),
];

#[derive(Debug)]
pub(super) struct Mise {
    pub data: PathBuf,
    pub state: PathBuf,
    pub cache: PathBuf,
    pub config: PathBuf,
    pub global: PathBuf,
    pub system: PathBuf,
    pub children: [PathBuf; 4],
}

impl Mise {
    pub fn discover(home: &Path) -> Self {
        Self::from_environment(home, |name| std::env::var_os(name))
    }

    fn from_environment(home: &Path, environment: impl Fn(&str) -> Option<OsString>) -> Self {
        let directory = |name: &str, fallback: PathBuf| {
            environment(&format!("DOTFILES_HOST_{name}"))
                .or_else(|| environment(name))
                .filter(|value| !value.is_empty())
                .map_or(fallback, PathBuf::from)
        };
        let xdg = |name: &str, fallback: PathBuf| {
            environment(name)
                .filter(|value| !value.is_empty())
                .map_or(fallback, PathBuf::from)
                .join("mise")
        };
        let data = directory(
            "MISE_DATA_DIR",
            xdg(
                "XDG_DATA_HOME",
                if cfg!(windows) {
                    environment("LOCALAPPDATA")
                        .filter(|value| !value.is_empty())
                        .map_or_else(|| home.join("AppData/Local"), PathBuf::from)
                } else {
                    home.join(".local/share")
                },
            ),
        );
        let config = directory(
            "MISE_CONFIG_DIR",
            xdg("XDG_CONFIG_HOME", home.join(".config")),
        );
        Self {
            children: DATA_CHILDREN.map(|(name, relative)| directory(name, data.join(relative))),
            data,
            state: directory(
                "MISE_STATE_DIR",
                xdg("XDG_STATE_HOME", home.join(".local/state")),
            ),
            cache: directory(
                "MISE_CACHE_DIR",
                xdg(
                    "XDG_CACHE_HOME",
                    if cfg!(windows) {
                        environment("TEMP")
                            .filter(|value| !value.is_empty())
                            .map_or_else(std::env::temp_dir, PathBuf::from)
                    } else if cfg!(target_os = "macos") {
                        home.join("Library/Caches")
                    } else {
                        home.join(".cache")
                    },
                ),
            ),
            global: directory(
                "MISE_GLOBAL_CONFIG_FILE",
                directory("MISE_CONFIG_FILE", config.join("config.toml")),
            ),
            system: directory(
                "MISE_SYSTEM_CONFIG_FILE",
                directory(
                    "MISE_SYSTEM_CONFIG_DIR",
                    directory("MISE_SYSTEM_DIR", SYSTEM_CONFIG_DIR.into()),
                )
                .join("config.toml"),
            ),
            config,
        }
    }
}

pub(super) fn copy(source: &Path, destination: &Path) -> Result<()> {
    if !source.try_exists()? {
        return Ok(());
    }
    copy_entry(
        source,
        destination,
        &platform::paths::canonical(source)?,
        destination,
        &mut Vec::new(),
    )
}

fn copy_entry(
    source: &Path,
    destination: &Path,
    root: &Path,
    owned: &Path,
    ancestors: &mut Vec<PathBuf>,
) -> Result<()> {
    let entry = fs::symlink_metadata(source)?;
    if entry.is_symlink() {
        let target = fs::read_link(source)?;
        let target = if target.is_absolute() {
            target
        } else {
            source
                .parent()
                .context("snapshot link parent")?
                .join(target)
        };
        let target = platform::paths::without_verbatim_prefix(&target);
        let mut normalized = PathBuf::new();
        for component in target.components() {
            match component {
                std::path::Component::ParentDir => {
                    normalized.pop();
                }
                std::path::Component::CurDir => {}
                component => normalized.push(component.as_os_str()),
            }
        }
        if let Ok(relative) = normalized.strip_prefix(root) {
            fs::create_dir_all(destination.parent().context("snapshot link destination")?)?;
            let target = owned.join(relative);
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, destination)?;
            #[cfg(windows)]
            {
                use std::os::windows::fs::FileTypeExt;
                if entry.file_type().is_symlink_dir() {
                    std::os::windows::fs::symlink_dir(target, destination)?;
                } else {
                    std::os::windows::fs::symlink_file(target, destination)?;
                }
            }
            return Ok(());
        }
    }
    let metadata = fs::metadata(source)
        .with_context(|| format!("inspect snapshot source {}", source.display()))?;
    if metadata.is_dir() {
        let canonical = platform::paths::canonical(source)?;
        ensure!(
            !ancestors.contains(&canonical),
            "snapshot refuses a directory cycle: {}",
            source.display()
        );
        ancestors.push(canonical);
        fs::create_dir_all(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_entry(
                &entry.path(),
                &destination.join(entry.file_name()),
                root,
                owned,
                ancestors,
            )?;
        }
        ancestors.pop();
    } else {
        ensure!(
            metadata.is_file(),
            "snapshot refuses special files: {}",
            source.display()
        );
        fs::create_dir_all(destination.parent().context("snapshot parent")?)?;
        fs::copy(source, destination).with_context(|| {
            format!(
                "copy snapshot source {} into {}",
                source.display(),
                destination.display()
            )
        })?;
        fs::set_permissions(destination, metadata.permissions())?;
    }
    Ok(())
}

pub(super) fn tracking(
    source: &Path,
    destination: &Path,
    global: &Path,
    candidate: &Path,
) -> Result<()> {
    if !source.try_exists()? {
        return Ok(());
    }
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            tracking(&entry.path(), &target, global, candidate)?;
        } else if entry.file_type()?.is_symlink() {
            let link = fs::read_link(entry.path())?;
            let link = if link == global { candidate } else { &link };
            #[cfg(unix)]
            std::os::unix::fs::symlink(link, target)?;
            #[cfg(windows)]
            if entry.path().is_dir() {
                std::os::windows::fs::symlink_dir(link, target)?;
            } else {
                std::os::windows::fs::symlink_file(link, target)?;
            }
        } else {
            let bytes = fs::read(entry.path())?;
            if std::str::from_utf8(&bytes).is_ok_and(|text| Path::new(text.trim()) == global) {
                fs::write(target, candidate.as_os_str().as_encoded_bytes())?;
            } else {
                fs::write(target, bytes)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_storage_precedes_the_bootstrap_environment() {
        let storage = Mise::from_environment(Path::new("native"), |name| match name {
            "DOTFILES_HOST_MISE_STATE_DIR" => Some("native-state".into()),
            "MISE_STATE_DIR" => Some("bootstrap-state".into()),
            "DOTFILES_HOST_MISE_DATA_DIR" => Some("native-data".into()),
            "MISE_DATA_DIR" => Some("bootstrap-data".into()),
            _ => None,
        });
        assert_eq!(storage.state, Path::new("native-state"));
        assert_eq!(storage.data, Path::new("native-data"));
    }

    #[test]
    fn native_storage_respects_xdg_and_separate_installations() {
        let storage = Mise::from_environment(Path::new("native"), |name| match name {
            "XDG_DATA_HOME" => Some("xdg-data".into()),
            "XDG_CACHE_HOME" => Some("xdg-cache".into()),
            "MISE_INSTALLS_DIR" => Some("separate-installs".into()),
            "MISE_CONFIG_FILE" => Some("custom-global.toml".into()),
            "MISE_SYSTEM_CONFIG_DIR" => Some("custom-system".into()),
            _ => None,
        });
        assert_eq!(storage.data, Path::new("xdg-data/mise"));
        assert_eq!(storage.cache, Path::new("xdg-cache/mise"));
        assert_eq!(storage.children[0], Path::new("separate-installs"));
        assert_eq!(storage.global, Path::new("custom-global.toml"));
        assert_eq!(storage.system, Path::new("custom-system/config.toml"));
    }

    #[test]
    fn tracking_preserves_projects_and_relocates_only_the_global_config() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let state = scope.path().join("state/tracked-configs");
        fs::create_dir_all(&state)?;
        let global = scope.path().join("native/config.toml");
        let candidate = scope.path().join("candidate/config.toml");
        let project = scope.path().join("project/mise.toml");
        for (name, target) in [("global", &global), ("project", &project)] {
            #[cfg(windows)]
            fs::write(state.join(name), target.as_os_str().as_encoded_bytes())?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, state.join(name))?;
        }
        let destination = scope.path().join("candidate-state");
        tracking(
            &scope.path().join("state"),
            &destination,
            &global,
            &candidate,
        )?;
        for (name, expected) in [("global", &candidate), ("project", &project)] {
            let record = destination.join("tracked-configs").join(name);
            #[cfg(windows)]
            assert_eq!(Path::new(&fs::read_to_string(record)?), expected);
            #[cfg(unix)]
            assert_eq!(fs::read_link(record)?, *expected);
        }
        Ok(())
    }

    #[test]
    fn internal_and_dangling_links_refer_only_to_the_owned_snapshot() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let source = scope.path().join("source");
        fs::create_dir(&source)?;
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&source, source.join("cycle"))?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(&source, source.join("cycle"))?;
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(source.join("absent"), source.join("dangling"))?;
        #[cfg(unix)]
        std::os::unix::fs::symlink("nested/../absent", source.join("dangling"))?;
        let target = scope.path().join("target");
        copy(&source, &target)?;
        assert_eq!(fs::read_link(target.join("cycle"))?, target);
        assert_eq!(
            fs::read_link(target.join("dangling"))?,
            target.join("absent")
        );
        fs::write(target.join("created"), "candidate")?;
        assert!(!source.join("created").exists());
        assert!(source.is_dir());
        Ok(())
    }

    #[test]
    fn installations_are_independent_files_including_linked_tools() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let source = scope.path().join("source");
        let target = scope.path().join("target");
        let external = scope.path().join("external");
        fs::create_dir(&source)?;
        fs::write(&external, "original tool")?;
        platform::code_environment::link_executable(&external, &source.join("tool"))?;
        copy(&source, &target)?;
        assert!(!fs::symlink_metadata(target.join("tool"))?.is_symlink());
        fs::write(target.join("tool"), "candidate writes")?;
        assert_eq!(fs::read_to_string(&external)?, "original tool");
        fs::remove_file(target.join("tool"))?;
        assert_eq!(fs::read_to_string(&external)?, "original tool");
        Ok(())
    }
}

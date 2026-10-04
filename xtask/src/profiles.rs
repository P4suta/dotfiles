use crate::profile_rules::{Mode, Profile, permitted};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum NativeAction {
    Diff,
    Apply,
    Verify,
    Doctor,
}

impl NativeAction {
    fn name(self) -> &'static str {
        match self {
            Self::Diff => "diff",
            Self::Apply => "apply",
            Self::Verify => "verify",
            Self::Doctor => "doctor",
        }
    }
}

pub(crate) fn source_files(directory: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if matches!(
            entry.file_name().to_str(),
            Some(".git" | "target" | "node_modules" | ".cache")
        ) {
            continue;
        }
        let kind = entry.file_type()?;
        ensure!(!kind.is_symlink(), "unexpected source filesystem symlink");
        if kind.is_dir() {
            source_files(&entry.path(), output)?;
        } else {
            ensure!(kind.is_file(), "unexpected non-file in source");
            output.push(entry.path());
        }
    }
    Ok(())
}

pub fn check_public(root: &Path) -> Result<()> {
    let mut files = Vec::new();
    source_files(root, &mut files)?;
    ensure!(!files.is_empty(), "public source check has no inputs");
    for file in files {
        let relative = file
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        let components: Vec<_> = relative.split('/').collect();
        ensure!(
            !components.iter().any(|part| matches!(
                *part,
                "dot_doppler" | ".doppler" | "auth.json" | "hosts.yml"
            ) && !relative.starts_with("ops/inventory/"))
                && !relative.ends_with("/id_ed25519")
                && !relative.ends_with("/id_rsa"),
            "runtime credential path in public source: {relative}"
        );
        let bytes = fs::read(&file)?;
        if let Ok(text) = std::str::from_utf8(&bytes) {
            ensure!(
                !text.lines().any(|line| line
                    .trim()
                    .strip_prefix("-----BEGIN ")
                    .and_then(|value| value.strip_suffix("-----"))
                    .is_some_and(|value| matches!(
                        value,
                        "OPENSSH PRIVATE KEY"
                            | "RSA PRIVATE KEY"
                            | "EC PRIVATE KEY"
                            | "PRIVATE KEY"
                    ))),
                "private key material in public source: {relative}"
            );
            if relative.ends_with(".tmpl") || relative.starts_with(".chezmoitemplates/") {
                ensure!(
                    !text.contains("{{ onepasswordRead")
                        && !text.contains("{{- onepasswordRead")
                        && !text.contains("{{ doppler ")
                        && !text.contains("{{- doppler "),
                    "secret retrieval belongs in a consuming process, not a managed template: {relative}"
                );
            }
        }
    }
    Ok(())
}

pub fn audit_private(root: &Path, denylist: &Path) -> Result<()> {
    let values: Vec<String> = serde_json::from_slice(&fs::read(denylist)?)?;
    ensure!(
        !values.is_empty() && values.iter().all(|value| !value.is_empty()),
        "empty private audit inputs"
    );
    let mut files = Vec::new();
    source_files(root, &mut files)?;
    ensure!(!files.is_empty(), "private audit has no source files");
    for file in files {
        let bytes = fs::read(&file)?;
        for value in &values {
            ensure!(
                !bytes
                    .windows(value.len())
                    .any(|window| window == value.as_bytes()),
                "private metadata remains in {}",
                file.strip_prefix(root)?.display()
            );
        }
    }
    Ok(())
}

/// chezmoi reports `.chezmoi.homeDir` and `.chezmoi.sourceDir` with forward slashes on every host, including Windows.
pub fn chezmoi_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if cfg!(windows) {
        text.replace('\\', "/")
    } else {
        text.into_owned()
    }
}

pub fn fixture(profile: Profile, root: &Path, destination: &Path) -> Value {
    json!({
        "profile": profile.name(), "role": "personal",
        "git": {"name": "Example User", "email": "user@example.invalid"},
        "signing": {"enabled": false, "public_key": "", "pubkey": "", "key_comment": ""},
        "herdr_agent": {"public_keys": []},
        "doppler": {"project": "fixture", "config": "fixture"},
        "paths": {"projects": destination.join("projects")},
        "chezmoi": {"os": profile.os(), "homeDir": chezmoi_path(destination), "sourceDir": chezmoi_path(root),
            "username": "fixture", "hostname": "fixture.invalid",
            "kernel": {"osrelease": if profile == Profile::Wsl {"fixture-microsoft"} else {"fixture"}}}
    })
}

fn output(command: &mut Command, phase: &str) -> Result<Output> {
    let result = command.output().with_context(|| format!("start {phase}"))?;
    ensure!(
        result.status.success(),
        "{phase} failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(result)
}

pub fn chezmoi(root: &Path, scope: &Path, config: &Path, destination: &Path) -> Command {
    let mut command = Command::new("chezmoi");
    command
        .arg("--source")
        .arg(root)
        .arg("--config")
        .arg(config)
        .arg("--destination")
        .arg(destination)
        .arg("--cache")
        .arg(scope.join("cache"))
        .arg("--persistent-state")
        .arg(scope.join("state.boltdb"))
        .args(["--refresh-externals=never", "--no-tty"]);
    command
}

/// Scripts are actions rather than state, and an always-run `run_` script would make every verification fail.
pub fn native_action_arguments(name: &str) -> Vec<&str> {
    if name == "verify" {
        vec![name, "--exclude", "scripts"]
    } else {
        vec![name]
    }
}

/// `chezmoi managed --nul-path-separator` output; its `--format` flag does not apply to relative paths, which are always printed as text.
pub fn managed_paths(stdout: &[u8]) -> Result<Vec<String>> {
    stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| Ok(std::str::from_utf8(path)?.to_owned()))
        .collect()
}

/// Managed directories with no managed file or symlink beneath them: another platform's tree leaking into this profile.
pub fn empty_directories(directories: &[String], leaves: &[String]) -> Vec<String> {
    directories
        .iter()
        .filter(|directory| {
            !leaves.iter().any(|leaf| {
                leaf.strip_prefix(directory.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
            })
        })
        .cloned()
        .collect()
}

pub fn check_profiles(root: &Path, report: Option<&Path>) -> Result<()> {
    check_public(root)?;
    if let Some(report) = report {
        fs::create_dir(report).context("profile report requires a fresh output directory")?;
    }
    let scratch = tempfile::tempdir()?;
    for profile in Profile::ALL {
        let scope = scratch.path().join(profile.name());
        let destination = scope.join("home with ' quote");
        fs::create_dir_all(&destination)?;
        let config = scope.join("config.json");
        let data = fixture(profile, root, &destination);
        fs::write(&config, serde_json::to_vec(&json!({"data": data}))?)?;
        let overrides = serde_json::to_string(&fixture(profile, root, &destination))?;
        let mut command = chezmoi(root, &scope, &config, &destination);
        command
            .arg("--override-data")
            .arg(&overrides)
            .env_remove("SSH_AUTH_SOCK")
            .env_remove("DOTFILES_SSH_AUTH_SOCK")
            .env_remove("SSH_CONNECTION")
            .env_remove("SSH_CLIENT")
            .env_remove("SSH_TTY")
            .env("DOTFILES_ONEPASSWORD_APP", scope.join("absent-app"))
            .args([
                "dump",
                "--include",
                "files,symlinks,scripts",
                "--exclude",
                "externals",
                "--format",
                "json",
            ]);
        let result = output(
            &mut command,
            &format!("{} profile rendering", profile.name()),
        )?;
        let dump: Value = serde_json::from_slice(&result.stdout)?;
        ensure!(
            dump.as_object().is_some_and(|targets| !targets.is_empty()),
            "empty profile rendering"
        );
        let contracts: std::collections::BTreeMap<String, u8> =
            serde_json::from_slice(&fs::read(root.join("policy/profile-contracts.json"))?)?;
        for (target, availability) in contracts {
            ensure!(
                dump.get(&target).is_some()
                    == crate::profile_rules::selected(profile, availability),
                "profile target contract failed: {} / {target}",
                profile.name()
            );
        }
        crate::quality::rendered(&dump, &scope.join("syntax"))?;
        let managed = |include: &str| -> Result<Vec<String>> {
            let listed = output(
                chezmoi(root, &scope, &config, &destination)
                    .arg("--override-data")
                    .arg(&overrides)
                    .args([
                        "managed",
                        "--include",
                        include,
                        "--exclude",
                        "externals",
                        "--nul-path-separator",
                    ]),
                &format!("{} managed {include}", profile.name()),
            )?;
            managed_paths(&listed.stdout)
        };
        let empty = empty_directories(&managed("dirs")?, &managed("files,symlinks")?);
        ensure!(
            empty.is_empty(),
            "{} profile would create or change directories it manages nothing in: {}",
            profile.name(),
            empty.join(", ")
        );
        fs::write(scope.join("render.json"), &result.stdout)?;
        if let Some(report) = report {
            fs::write(
                report.join(format!("{}.json", profile.name())),
                &result.stdout,
            )?;
        }
        let retired = destination.join(".config/shell/github-token.sh");
        fs::create_dir_all(retired.parent().context("fixture parent is missing")?)?;
        fs::write(&retired, "obsolete credential exporter fixture")?;
        for action in ["apply", "verify", "apply", "verify"] {
            output(
                chezmoi(root, &scope, &config, &destination)
                    .arg("--override-data")
                    .arg(&overrides)
                    .arg("--force")
                    .args([
                        action,
                        "--include",
                        "dirs,files,symlinks,remove",
                        "--exclude",
                        "externals",
                    ]),
                &format!("{} isolated files-only {action}", profile.name()),
            )?;
        }
        ensure!(
            profile == Profile::Windows || !retired.exists(),
            "obsolete credential exporter was preserved"
        );
        output(
            Command::new("git")
                .args(["config", "--file"])
                .arg(destination.join(if profile == Profile::Windows {
                    ".config/git/config"
                } else {
                    ".gitconfig"
                }))
                .args(["--list"]),
            "parse rendered Git configuration",
        )?;
        println!(
            "Rendered, applied twice, and verified {} in an owned fixture",
            profile.name()
        );
    }
    Ok(())
}

pub fn native_profile() -> Result<Profile> {
    match std::env::consts::OS {
        "macos" => Ok(Profile::Mac),
        "windows" => Ok(Profile::Windows),
        "linux" => Ok(
            if fs::read_to_string("/proc/sys/kernel/osrelease")
                .is_ok_and(|release| release.to_ascii_lowercase().contains("microsoft"))
            {
                Profile::Wsl
            } else {
                Profile::Linux
            },
        ),
        _ => anyhow::bail!("unsupported native operating system"),
    }
}

/// A setup script a machine-local configuration deliberately leaves out, with the reason that makes the omission reviewable.
#[derive(Debug, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkippedScript {
    pub script: String,
    pub reason: String,
}

/// `setup.skip` entries, which `.chezmoiignore` removes from application; every entry must name one script target and say why.
pub fn declared_skips(data: &Value) -> Result<Vec<SkippedScript>> {
    let Some(entries) = data.pointer("/setup/skip") else {
        return Ok(Vec::new());
    };
    let skips: Vec<SkippedScript> = serde_json::from_value(entries.clone())
        .context("setup.skip requires a list of script and reason entries")?;
    for skip in &skips {
        ensure!(
            !skip.script.is_empty()
                && skip
                    .script
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
                && (skip.script.ends_with(".sh") || skip.script.ends_with(".ps1")),
            "setup.skip names a script target such as install-tools.sh: {}",
            skip.script
        );
        ensure!(
            !skip.reason.trim().is_empty(),
            "setup.skip requires a reason for {}",
            skip.script
        );
    }
    Ok(skips)
}

pub fn configured_profile(data: &Value) -> Result<Profile> {
    match data["profile"].as_str() {
        Some("mac") => Ok(Profile::Mac),
        Some("linux") => Ok(Profile::Linux),
        Some("windows") => Ok(Profile::Windows),
        Some("wsl") => Ok(Profile::Wsl),
        _ => anyhow::bail!("machine-local configuration requires a known profile"),
    }
}

pub fn operate(
    root: &Path,
    action: NativeAction,
    config: &Path,
    destination: &Path,
    live: bool,
    state: &Path,
    backup: Option<&Path>,
) -> Result<()> {
    ensure!(config.is_file(), "machine-local config is missing");
    ensure!(
        config.is_absolute() && destination.is_absolute() && state.is_absolute(),
        "native paths must be explicit absolute paths"
    );
    ensure!(
        !crate::canonical(config)?.starts_with(root),
        "machine-local config belongs outside the public source"
    );
    let scope = tempfile::tempdir()?;
    let bytes = output(
        chezmoi(root, scope.path(), config, destination).args(["data", "--format", "json"]),
        "read machine-local profile",
    )?
    .stdout;
    let data: Value = serde_json::from_slice(&bytes)?;
    let profile = configured_profile(&data)?;
    for skip in declared_skips(&data)? {
        println!("Skipping setup script {}: {}", skip.script, skip.reason);
    }
    let mode = if matches!(action, NativeAction::Apply) {
        Mode::Full
    } else {
        Mode::Preview
    };
    ensure!(
        permitted(mode, profile == native_profile()?, false, live),
        "live application requires --live and this host's native profile"
    );
    if matches!(mode, Mode::Full) {
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .context("native home is unavailable")?;
        ensure!(
            destination.canonicalize()? == Path::new(&home).canonicalize()?,
            "setup scripts require this host's own native home destination"
        );
    }
    outside_public(root, state)?;
    fs::create_dir_all(state)?;
    let _lock = if matches!(mode, Mode::Full) {
        Some(StateLock::acquire(state)?)
    } else {
        None
    };
    let execute = |name: &str| -> Result<()> {
        let status = chezmoi(root, state, config, destination)
            .args(native_action_arguments(name))
            .status()?;
        ensure!(status.success(), "chezmoi {name} failed with {status}");
        Ok(())
    };
    if matches!(action, NativeAction::Apply) {
        let backup = backup.context("application requires an explicit fresh --backup directory")?;
        outside_public(root, backup)?;
        let state_file = state.join("state.boltdb");
        let previous_state = match fs::read(&state_file) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let listed = output(
            chezmoi(root, scope.path(), config, destination).args([
                "managed",
                "--include",
                "files,symlinks,remove",
                "--exclude",
                "externals",
                "--nul-path-separator",
            ]),
            "list backup targets",
        )?;
        let targets: Vec<PathBuf> = managed_paths(&listed.stdout)?
            .into_iter()
            .map(PathBuf::from)
            .collect();
        let result = crate::transaction::apply(
            destination,
            backup,
            &targets,
            || {
                if let Some(bytes) = &previous_state {
                    fs::write(backup.join("previous-state.boltdb"), bytes)?;
                }
                execute("apply")
            },
            || {
                execute("verify").map_err(|error| {
                    // Name the diverging targets before the transaction restores them, or the evidence is gone.
                    let pending = chezmoi(root, state, config, destination)
                        .args(["status", "--exclude", "scripts,externals"])
                        .output()
                        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
                        .unwrap_or_default();
                    error.context(format!("targets still differing after apply:\n{pending}"))
                })
            },
        );
        if let Err(error) = result {
            if let Some(bytes) = previous_state {
                crate::runtime::replace(&state_file, &bytes, false)?;
            } else if state_file.exists() {
                fs::remove_file(&state_file)?;
            }
            return Err(error);
        }
        println!(
            "Application verified; managed-file backup retained at {}",
            backup.display()
        );
    } else {
        execute(action.name())?;
    }
    Ok(())
}

fn outside_public(root: &Path, path: &Path) -> Result<()> {
    let root = crate::canonical(root)?;
    ensure!(
        path.is_absolute()
            && !path
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir)),
        "private state paths must be absolute without parent traversal"
    );
    let existing = path
        .ancestors()
        .find(|path| path.exists())
        .context("private path has no existing ancestor")?;
    ensure!(
        !crate::canonical(existing)?.starts_with(&root) && !path.starts_with(&root),
        "private state and backups belong outside the public source"
    );
    Ok(())
}

struct StateLock(PathBuf);
impl StateLock {
    fn acquire(state: &Path) -> Result<Self> {
        let path = state.join("apply.lock");
        fs::OpenOptions::new().write(true).create_new(true).open(&path)
            .context("another application or an interrupted application holds apply.lock; inspect it before retrying")?;
        Ok(Self(path))
    }
}
impl Drop for StateLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_lock_refuses_overlap_and_does_not_steal_an_interrupted_lock() {
        let state = tempfile::tempdir().unwrap();
        let lock = StateLock::acquire(state.path()).unwrap();
        assert!(StateLock::acquire(state.path()).is_err());
        drop(lock);
        drop(StateLock::acquire(state.path()).unwrap());
        fs::write(state.path().join("apply.lock"), "interrupted application").unwrap();
        assert!(StateLock::acquire(state.path()).is_err());
        assert_eq!(
            fs::read(state.path().join("apply.lock")).unwrap(),
            b"interrupted application"
        );
    }

    #[cfg(unix)]
    #[test]
    fn private_state_cannot_escape_the_public_boundary_through_a_symlink() {
        let scope = tempfile::tempdir().unwrap();
        let public = scope.path().join("public");
        fs::create_dir(&public).unwrap();
        std::os::unix::fs::symlink(&public, scope.path().join("alias")).unwrap();
        assert!(outside_public(&public, &scope.path().join("alias/state")).is_err());
        assert!(outside_public(&public, &public.join("../state")).is_err());
        outside_public(&public, &scope.path().join("private/state")).unwrap();
    }
}

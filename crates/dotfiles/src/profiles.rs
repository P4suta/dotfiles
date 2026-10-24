//! The host's own profile: the native chezmoi actions with a machine-local configuration, and the transaction that applies it.

use crate::profile_rules::{Mode, Profile, application_source, host_edit_refused, permitted};
use crate::tool::Tool;
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

/// Runs `command` and returns its output when it succeeds.
///
/// # Errors
///
/// Returns an error naming `phase` when the command can't start or fails, with its standard error.
pub fn output(command: &mut Command, phase: &str) -> Result<Output> {
    let result = command.output().with_context(|| format!("start {phase}"))?;
    ensure!(
        result.status.success(),
        "{phase} failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(result)
}

/// Where chezmoi keeps the state database and the cache of one profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Store {
    /// The directory that holds the database and the apply lock.
    pub directory: PathBuf,
    pub database: PathBuf,
    /// The cache, or `None` for chezmoi's default.
    pub cache: Option<PathBuf>,
}

impl Store {
    /// A database and a cache under `directory`, which keep a gate or a rehearsal apart from the host's own record.
    #[must_use]
    pub fn scoped(directory: &Path) -> Self {
        Self {
            directory: directory.to_path_buf(),
            database: directory.join("state.boltdb"),
            cache: Some(directory.join("cache")),
        }
    }

    /// chezmoi's default database beside the machine-local configuration.
    /// `just apply` and a plain `chezmoi apply` then share one record of the scripts that ran.
    ///
    /// # Errors
    ///
    /// Returns an error when `config` names no directory.
    pub fn beside(config: &Path) -> Result<Self> {
        let directory = config
            .parent()
            .context("the machine-local configuration names no directory")?;
        Ok(Self {
            directory: directory.to_path_buf(),
            database: directory.join("chezmoistate.boltdb"),
            cache: None,
        })
    }
}

#[must_use]
pub fn chezmoi(root: &Path, scope: &Path, config: &Path, destination: &Path) -> Command {
    chezmoi_with(root, &Store::scoped(scope), config, destination)
}

/// chezmoi on `destination` with `config`, reading and writing its state in `store`.
#[must_use]
pub fn chezmoi_with(root: &Path, store: &Store, config: &Path, destination: &Path) -> Command {
    let mut command = Tool::chezmoi_in(root);
    command
        .arg("--config")
        .arg(config)
        .arg("--destination")
        .arg(destination);
    if let Some(cache) = &store.cache {
        command.arg("--cache").arg(cache);
    }
    command
        .arg("--persistent-state")
        .arg(&store.database)
        .args(["--refresh-externals=never", "--no-tty"]);
    command
}

/// Targets that `chezmoi status` reports as changed since chezmoi last wrote them, from its first column.
#[must_use]
pub fn externally_changed(status: &str) -> Vec<String> {
    status
        .lines()
        .filter(|line| !line.starts_with(' '))
        .filter_map(status_target)
        .collect()
}

/// Targets that differ from their rendered source, from the second status column.
#[must_use]
pub fn target_changes(status: &str) -> Vec<String> {
    status
        .lines()
        .filter(|line| line.chars().nth(1).is_some_and(|status| status != ' '))
        .filter_map(status_target)
        .collect()
}

/// `chezmoi status` starts each line with its two status columns and a space.
const STATUS_COLUMNS: usize = 3;

fn status_target(line: &str) -> Option<String> {
    line.get(STATUS_COLUMNS..)
        .filter(|target| !target.is_empty())
        .map(str::to_owned)
}

/// The lines of a chezmoi report that a refusal shows.
const REPORT_LINES: usize = 200;

/// The kernel release, which names Microsoft on WSL.
const OS_RELEASE: &str = "/proc/sys/kernel/osrelease";

/// Targets with a `modify_` template source, from `chezmoi managed --path-style all --format json`.
/// Such a template may merge its keys into whatever the client last wrote, so expect changes made outside chezmoi there.
///
/// # Errors
///
/// Returns an error when the listing holds something other than a JSON object or an entry has no `sourceRelative`.
pub fn merged_targets(listing: &[u8]) -> Result<Vec<String>> {
    let entries: serde_json::Map<String, Value> = serde_json::from_slice(listing)?;
    entries
        .into_iter()
        .filter_map(|(target, entry)| {
            let Some(source) = entry["sourceRelative"].as_str() else {
                return Some(Err(anyhow::anyhow!(
                    "managed entry without a source: {target}"
                )));
            };
            let name = source.rsplit('/').next().unwrap_or_default();
            name.starts_with("modify_").then_some(Ok(target))
        })
        .collect()
}

/// Whether a rendering keeps every key path of the host file, so a merge overwrites only the keys its template sets.
///
/// # Errors
///
/// Returns an error for an unsupported format or an invalid JSON or TOML document.
pub fn keeps_host_keys(target: &str, current: &str, rendered: &str) -> Result<bool> {
    fn kept(current: &Value, rendered: &Value) -> bool {
        match (current, rendered) {
            (Value::Object(current), Value::Object(rendered)) => {
                current.iter().all(|(key, value)| {
                    rendered
                        .get(key)
                        .is_some_and(|rendered| kept(value, rendered))
                })
            }
            (Value::Object(current), _) => current.is_empty(),
            _ => true,
        }
    }
    if matches!(target, ".ssh/config" | ".ssh\\config") {
        return Ok(crate::herdr_rules::ssh_config_preserved(current, rendered));
    }
    let toml = match Path::new(target).extension().and_then(|e| e.to_str()) {
        Some("json") => false,
        Some("toml") => true,
        _ => anyhow::bail!(
            "{target} is a modify_ target in a format whose keys keeps_host_keys in crates/dotfiles/src/profiles.rs cannot read; add the format there, then run `just profiles`"
        ),
    };
    let parse = |contents: &str| -> Result<Value> {
        Ok(if contents.trim().is_empty() {
            json!({})
        } else if toml {
            serde_json::to_value(toml::from_str::<toml::Table>(contents)?)?
        } else {
            serde_json::from_str(contents)?
        })
    };
    let current = parse(current).with_context(|| format!("parse host {target}"))?;
    let rendered = parse(rendered).with_context(|| format!("parse rendered {target}"))?;
    Ok(kept(&current, &rendered))
}

/// Targets changed outside chezmoi since the last apply that the next apply would overwrite.
/// A `modify_` target passes only when its rendering keeps every key the host file has.
///
/// # Errors
///
/// Returns an error when a chezmoi command fails, it can't parse the command's listing, or it can't read a changed host file.
/// Returns an error when it can't compare a changed `modify_` target with its rendering.
pub fn uncommitted_host_edits(
    root: &Path,
    store: &Store,
    config: &Path,
    destination: &Path,
) -> Result<Vec<String>> {
    let status = output(
        chezmoi_with(root, store, config, destination).args(["status", "--exclude", "scripts"]),
        "read target status",
    )?;
    let merged = merged_targets(
        &output(
            chezmoi_with(root, store, config, destination).args([
                "managed",
                "--include",
                "files",
                "--path-style",
                "all",
                "--format",
                "json",
            ]),
            "list merged targets",
        )?
        .stdout,
    )?;
    let mut refused = Vec::new();
    for target in externally_changed(&String::from_utf8_lossy(&status.stdout)) {
        let is_merged = merged.contains(&target);
        let keeps = is_merged && {
            let path = destination.join(&target);
            let rendered = output(
                chezmoi_with(root, store, config, destination)
                    .arg("cat")
                    .arg(&path),
                &format!("render {target}"),
            )?;
            let current = match fs::read_to_string(&path) {
                Ok(contents) => contents,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
                Err(error) => return Err(error).with_context(|| format!("read host {target}")),
            };
            keeps_host_keys(
                &target,
                &current,
                &String::from_utf8_lossy(&rendered.stdout),
            )?
        };
        if host_edit_refused(true, is_merged, keeps) {
            refused.push(target);
        }
    }
    Ok(refused)
}

/// The refusal for targets changed outside chezmoi that the next apply would overwrite.
#[must_use]
pub fn host_edit_refusal(changed: &[String], config: &Path, destination: &Path) -> String {
    let paths = format!("'{}' '{}'", config.display(), destination.display());
    format!(
        "{} managed files changed outside chezmoi since the last application, and nothing has been changed: {changed:?}\nRun `just diff {paths}` to see each change, carry it into the source or undo it on the host, then run `just apply {paths} <fresh backup directory>`",
        changed.len()
    )
}

/// Forgets which `run_onchange_` and `run_once_` scripts already ran, so the next apply reruns them.
///
/// # Errors
///
/// Returns an error when chezmoi fails to dump or delete its state, or the dump doesn't parse as JSON.
pub fn reset_entry_and_script_state(
    root: &Path,
    store: &Store,
    config: &Path,
    destination: &Path,
) -> Result<()> {
    let dump = output(
        chezmoi_with(root, store, config, destination).args(["state", "dump", "--format", "json"]),
        "read chezmoi state",
    )?;
    let buckets: Value = serde_json::from_slice(&dump.stdout)?;
    for bucket in ["entryState", "scriptState"] {
        if buckets.get(bucket).is_some() {
            output(
                chezmoi_with(root, store, config, destination).args([
                    "state",
                    "delete-bucket",
                    &format!("--bucket={bucket}"),
                ]),
                "forget script runs",
            )?;
        }
    }
    Ok(())
}

/// The script targets the next apply would run that rerun only when their inputs change, which a repeated apply must leave none of.
///
/// # Errors
///
/// Returns an error when a chezmoi command fails or its listing doesn't parse.
pub fn pending_onchange_scripts(
    root: &Path,
    store: &Store,
    config: &Path,
    destination: &Path,
) -> Result<Vec<String>> {
    let status = output(
        chezmoi_with(root, store, config, destination).args(["status", "--include", "scripts"]),
        "read script status",
    )?;
    let listing: serde_json::Map<String, Value> = serde_json::from_slice(
        &output(
            chezmoi_with(root, store, config, destination).args([
                "managed",
                "--include",
                "scripts",
                "--path-style",
                "all",
                "--format",
                "json",
            ]),
            "list scripts",
        )?
        .stdout,
    )?;
    Ok(String::from_utf8_lossy(&status.stdout)
        .lines()
        .filter_map(status_target)
        .filter(|target| {
            listing
                .get(target)
                .and_then(|entry| entry["sourceRelative"].as_str())
                .and_then(|source| source.rsplit('/').next())
                .is_some_and(|name| {
                    name.starts_with("run_onchange_") || name.starts_with("run_once_")
                })
        })
        .collect())
}

#[must_use]
pub fn native_action_arguments(name: &str) -> Vec<&str> {
    if name == "verify" {
        vec![name, "--exclude", "scripts"]
    } else {
        vec![name]
    }
}

/// Script targets in the order chezmoi runs them: every `run_before_` script, then every `run_after_` script, each group by target name.
///
/// # Errors
///
/// Returns an error when the listing holds something other than a JSON object or an entry has no `sourceRelative`.
pub fn script_order(listing: &[u8]) -> Result<Vec<String>> {
    let entries: std::collections::BTreeMap<String, Value> = serde_json::from_slice(listing)?;
    let mut before = Vec::new();
    let mut after = Vec::new();
    for (target, entry) in entries {
        let source = entry["sourceRelative"]
            .as_str()
            .context("script listing requires sourceRelative")?;
        let name = source.rsplit('/').next().unwrap_or(source);
        let attributes: Vec<_> = name.split('_').collect();
        if attributes.contains(&"before") {
            before.push(target);
        } else {
            after.push(target);
        }
    }
    before.extend(after);
    Ok(before)
}

/// `chezmoi managed --nul-path-separator` output.
/// Its `--format` flag ignores relative paths and always prints them as text.
///
/// # Errors
///
/// Returns an error when a path contains bytes outside UTF-8.
pub fn managed_paths(stdout: &[u8]) -> Result<Vec<String>> {
    stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| Ok(std::str::from_utf8(path)?.to_owned()))
        .collect()
}

/// The data files of the source state, merged the way chezmoi merges them.
///
/// # Errors
///
/// Returns an error that names the file and the key when a data file holds an unknown or mistyped key, or when reading a file fails.
pub fn source_data(root: &Path) -> Result<Value> {
    model::data::load(&crate::source_state(root).join(model::data::DIRECTORY))
        .map_err(|error| anyhow::anyhow!("{error}"))
}

/// The profile this host runs natively, from its operating system and, on Linux, its kernel release.
///
/// # Errors
///
/// Returns an error on an operating system no profile covers.
pub fn native_profile() -> Result<Profile> {
    use crate::profile_rules::{System, detected, microsoft_kernel};
    let system = match std::env::consts::OS {
        "macos" => System::Mac,
        "windows" => System::Windows,
        "linux" => System::Linux,
        _ => anyhow::bail!("unsupported native operating system"),
    };
    let microsoft = system == System::Linux
        && fs::read(OS_RELEASE).is_ok_and(|release| microsoft_kernel(&release));
    Ok(detected(system, microsoft))
}

/// A setup script a machine-local configuration leaves out, with the reason that makes the omission reviewable.
#[derive(Debug, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkippedScript {
    pub script: String,
    pub reason: String,
}

/// `setup.ignore` entries, which `.chezmoiignore` keeps chezmoi from applying, with `setup.skip` as the spelling before the rename.
///
/// # Errors
///
/// Returns an error when `setup.ignore` holds something other than a list of script and reason entries.
/// Returns an error when an entry names something other than a `.sh` or `.ps1` script target or has a blank reason.
pub fn declared_skips(data: &Value) -> Result<Vec<SkippedScript>> {
    let entries = data
        .pointer("/setup/ignore")
        .or_else(|| data.pointer("/setup/skip"));
    let Some(entries) = entries else {
        return Ok(Vec::new());
    };
    let skips: Vec<SkippedScript> = serde_json::from_value(entries.clone())
        .context("setup.ignore requires a list of script and reason entries")?;
    for skip in &skips {
        ensure!(
            !skip.script.is_empty()
                && skip
                    .script
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
                && (crate::has_extension(&skip.script, "sh")
                    || crate::has_extension(&skip.script, "ps1")),
            "setup.ignore names a setup launcher by its step and extension, such as ocaml.sh: {}",
            skip.script
        );
        ensure!(
            !skip.reason.trim().is_empty(),
            "setup.ignore requires a reason for {}",
            skip.script
        );
    }
    Ok(skips)
}

/// The profile that machine-local configuration data names.
///
/// # Errors
///
/// Returns an error when the data names no profile or an unknown one.
pub fn configured_profile(data: &Value) -> Result<Profile> {
    match data["profile"].as_str() {
        Some("mac") => Ok(Profile::Mac),
        Some("linux") => Ok(Profile::Linux),
        Some("windows") => Ok(Profile::Windows),
        Some("wsl") => Ok(Profile::Wsl),
        _ => anyhow::bail!("machine-local configuration requires a known profile"),
    }
}

fn application_root(root: &Path, config: &Path) -> Result<PathBuf> {
    let configuration: Value = serde_json::from_slice(
        &output(
            &mut Tool::chezmoi_configuration(config),
            "read configured chezmoi source",
        )?
        .stdout,
    )?;
    let configured = Path::new(
        configuration["sourceDir"]
            .as_str()
            .context("chezmoi configuration requires sourceDir")?,
    );
    let configured = crate::canonical(configured).with_context(|| {
        format!(
            "configured chezmoi source {} is unavailable",
            configured.display()
        )
    })?;
    let root = crate::canonical(root)?;
    let state = crate::source_state(&root);
    let checkout = configured
        .parent()
        .filter(|parent| {
            fs::read_to_string(parent.join(".chezmoiroot"))
                .is_ok_and(|name| parent.join(name.trim()) == configured)
        })
        .unwrap_or(&configured);
    ensure!(
        application_source(configured == root, configured == state),
        "apply must run from the configured chezmoi source {}; run this apply from {}",
        configured.display(),
        checkout.display()
    );
    Ok(root)
}

/// Runs a native chezmoi action on this host with a machine-local configuration.
///
/// # Errors
///
/// Returns an error when the configuration doesn't exist or lives inside the public source, or when it receives a relative path.
/// It also fails when the configuration names an unknown profile or invalid skips.
/// An apply from another checkout refuses before rendering or changing any target.
/// Returns an error when a live apply lacks `--live`, this host's native profile, its native home, or a backup directory, or when chezmoi fails.
/// Without `state`, chezmoi keeps its default database beside the configuration.
pub fn operate(
    root: &Path,
    action: NativeAction,
    config: &Path,
    destination: &Path,
    live: bool,
    state: Option<&Path>,
    backup: Option<&Path>,
) -> Result<()> {
    ensure!(config.is_file(), "machine-local config is missing");
    ensure!(
        config.is_absolute() && destination.is_absolute() && state.is_none_or(Path::is_absolute),
        "native paths must be explicit absolute paths"
    );
    let store = match state {
        Some(state) => Store::scoped(state),
        None => Store::beside(config)?,
    };
    ensure!(
        !crate::canonical(config)?.starts_with(root),
        "machine-local config belongs outside the public source"
    );
    let application_root = matches!(action, NativeAction::Apply)
        .then(|| application_root(root, config))
        .transpose()?;
    let root = application_root.as_deref().unwrap_or(root);
    let scope = tempfile::tempdir()?;
    let bytes = output(
        chezmoi(root, scope.path(), config, destination).args(["data", "--format", "json"]),
        "read machine-local profile",
    )?
    .stdout;
    let data: Value = serde_json::from_slice(&bytes)?;
    let profile = configured_profile(&data)?;
    if matches!(action, NativeAction::Doctor) {
        let native = native_profile()?;
        ensure!(
            profile == native,
            "the configuration names the {} profile, but this host is {}; run `chezmoi init` to write the detected profile",
            profile.name(),
            native.name()
        );
    }
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
        let home = platform::home::home().context("native home is unavailable")?;
        ensure!(
            crate::canonical(destination)? == crate::canonical(&home)?,
            "setup scripts require this host's own native home destination"
        );
    }
    outside_public(root, &store.directory)?;
    fs::create_dir_all(&store.directory)?;
    let _lock = if matches!(mode, Mode::Full) {
        Some(StateLock::acquire(&store.directory)?)
    } else {
        None
    };
    if matches!(action, NativeAction::Apply) {
        let backup = backup.context("application requires an explicit fresh --backup directory")?;
        apply_live(root, scope.path(), config, destination, &store, backup)
    } else {
        run_native(root, &store, config, destination, action.name())
    }
}

fn run_native(
    root: &Path,
    store: &Store,
    config: &Path,
    destination: &Path,
    name: &str,
) -> Result<()> {
    let status = chezmoi_with(root, store, config, destination)
        .args(native_action_arguments(name))
        .status()?;
    ensure!(status.success(), "chezmoi {name} failed with {status}");
    Ok(())
}

/// Refuses before anything changes when a setup step the rendered scripts run lacks what it needs on this host.
fn preflight(root: &Path, scope: &Path, config: &Path, destination: &Path) -> Result<()> {
    let scripts = output(
        chezmoi(root, scope, config, destination).args([
            "dump",
            "--include",
            "scripts",
            "--format",
            "json",
        ]),
        "render setup scripts",
    )?;
    let rendered: Value = serde_json::from_slice(&scripts.stdout)?;
    let contents: String = rendered
        .as_object()
        .context("script dump must be a mapping")?
        .values()
        .filter_map(|script| script["contents"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let context = crate::setup::load(root, config, destination)?;
    crate::setup::preflight(
        &context,
        &mut crate::runtime::Real::new(destination)?,
        &crate::setup::scripted_steps(&contents),
    )
}

/// Applies this host's profile inside a transaction that restores the managed files and the state database when verification fails.
fn apply_live(
    root: &Path,
    scope: &Path,
    config: &Path,
    destination: &Path,
    store: &Store,
    backup: &Path,
) -> Result<()> {
    let execute = |name: &str| run_native(root, store, config, destination, name);
    outside_public(root, backup)?;
    preflight(root, scope, config, destination)?;
    let changed = uncommitted_host_edits(root, store, config, destination)?;
    ensure!(
        changed.is_empty(),
        "{}",
        host_edit_refusal(&changed, config, destination)
    );
    let state_file = store.database.as_path();
    let previous_state = match fs::read(state_file) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let listed = output(
        chezmoi(root, scope, config, destination).args([
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
            execute("apply")?;
            // Scripts can install programs that templates probe for, so render the files again on the resulting machine before verification.
            let status = chezmoi_with(root, store, config, destination)
                .args(["apply", "--exclude", "scripts"])
                .status()?;
            ensure!(
                status.success(),
                "chezmoi convergence apply failed with {status}"
            );
            Ok(())
        },
        || {
            execute("verify").map_err(|error| {
                let report = |arguments: &[&str]| {
                    chezmoi_with(root, store, config, destination)
                        .args(arguments)
                        .output()
                        .map(|output| {
                            let text = String::from_utf8_lossy(&output.stdout);
                            text.lines()
                                .take(REPORT_LINES)
                                .collect::<Vec<_>>()
                                .join("\n")
                        })
                        .unwrap_or_default()
                };
                let pending = report(&["status", "--exclude", "scripts,externals"]);
                let difference = report(&["diff", "--exclude", "scripts,externals"]);
                error.context(format!(
                    "targets still differing after apply:\n{pending}\n{difference}"
                ))
            })
        },
    );
    if let Err(error) = result {
        if let Some(bytes) = previous_state {
            crate::runtime::replace(state_file, &bytes, false)?;
        } else if state_file.exists() {
            fs::remove_file(state_file)?;
        }
        return Err(error);
    }
    println!(
        "Application verified; managed-file backup retained at {}",
        backup.display()
    );
    skill_ops::skill_install::install_catalog(root, destination, native_profile()?)?;
    crate::freshness::reinstall_lagging(destination, root)?;
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
        platform::cleanup::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_apply_from_another_checkout_preserves_configured_entry_points() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let source = scope.path().join("configured source");
        let other = scope.path().join("temporary worktree");
        let destination = scope.path().join("destination");
        let config = scope.path().join("chezmoi.toml");
        let state = scope.path().join("state");
        let backup = scope.path().join("backup");
        let targets = [
            ".local/bin/dotfiles-doctor",
            ".local/bin/dotfiles-refresh-desktop",
            ".local/bin/dotfiles-refresh-integrations",
            ".config/nushell/config.nu",
            "Library/LaunchAgents/dev.dotfiles.tools-upgrade.plist",
        ];
        let templates = [
            "dot_local/bin/dotfiles-doctor.tmpl",
            "dot_local/bin/dotfiles-refresh-desktop.tmpl",
            "dot_local/bin/dotfiles-refresh-integrations.tmpl",
            "dot_config/nushell/config.nu.tmpl",
            "Library/LaunchAgents/dev.dotfiles.tools-upgrade.plist.tmpl",
        ];
        for checkout in [&source, &other] {
            fs::create_dir_all(checkout.join("home"))?;
            fs::write(checkout.join(".chezmoiroot"), "home\n")?;
            for template in templates {
                let template = checkout.join("home").join(template);
                fs::create_dir_all(template.parent().unwrap())?;
                fs::write(
                    template,
                    "dotfiles --root {{ .chezmoi.workingTree | quote }}\n",
                )?;
            }
        }
        fs::create_dir_all(&destination)?;
        for configured in [&source, &source.join("home")] {
            fs::write(
                &config,
                format!(
                    "sourceDir = {:?}\n[data]\nprofile = {:?}\n",
                    configured.to_string_lossy().replace('\\', "/"),
                    native_profile()?.name()
                ),
            )?;
            assert_eq!(
                application_root(&source, &config)?,
                crate::canonical(&source)?
            );
            output(
                chezmoi(&source, &state, &config, &destination).args(["apply", "--force"]),
                "seed configured entry points",
            )?;
            let before = targets
                .iter()
                .map(|target| fs::read(destination.join(target)))
                .collect::<std::io::Result<Vec<_>>>()?;
            let error = operate(
                &other,
                NativeAction::Apply,
                &config,
                &destination,
                true,
                Some(&state),
                Some(&backup),
            )
            .unwrap_err();
            let message = format!("{error:#}");
            assert!(message.contains("configured chezmoi source"), "{message}");
            assert!(message.contains(&*source.to_string_lossy()), "{message}");
            assert!(!backup.exists());
            assert!(!state.join("apply.lock").exists());
            for (target, previous) in targets.iter().zip(&before) {
                let contents = fs::read(destination.join(target))?;
                assert_eq!(&contents, previous);
                let contents = String::from_utf8(contents)?;
                assert!(contents.contains(&*source.to_string_lossy().replace('\\', "/")));
                assert!(!contents.contains(&*other.to_string_lossy().replace('\\', "/")));
            }
        }
        Ok(())
    }

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

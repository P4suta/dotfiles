use crate::profile_rules::{Mode, Profile, host_edit_refused, permitted};
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
    let mut command = Tool::Chezmoi.command();
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

/// Targets that `chezmoi status` reports as changed since chezmoi last wrote them, from its first column.
pub fn externally_changed(status: &str) -> Vec<String> {
    status
        .lines()
        .filter(|line| line.len() > 3 && !line.starts_with(' '))
        .map(|line| line[3..].to_owned())
        .collect()
}

/// Targets with a `modify_` template source, from `chezmoi managed --path-style all --format json`.
/// Such a template may merge its keys into whatever the client last wrote, so expect changes made outside chezmoi there.
pub fn merged_targets(listing: &[u8]) -> Result<Vec<String>> {
    let entries: serde_json::Map<String, Value> = serde_json::from_slice(listing)?;
    entries
        .into_iter()
        .filter_map(|(target, entry)| {
            let source = match entry["sourceRelative"].as_str() {
                Some(source) => source,
                None => {
                    return Some(Err(anyhow::anyhow!(
                        "managed entry without a source: {target}"
                    )));
                }
            };
            let name = source.rsplit('/').next().unwrap_or_default();
            name.starts_with("modify_").then_some(Ok(target))
        })
        .collect()
}

/// Whether a rendering keeps every key path of the host file, so a merge overwrites only the keys its template sets.
pub fn keeps_host_keys(target: &str, current: &str, rendered: &str) -> Result<bool> {
    let toml = match Path::new(target).extension().and_then(|e| e.to_str()) {
        Some("json") => false,
        Some("toml") => true,
        _ => anyhow::bail!(
            "{target} is a modify_ target in a format whose keys keeps_host_keys in xtask/src/profiles.rs cannot read; add the format there, then run `just profiles`"
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
    let current = parse(current).with_context(|| format!("parse host {target}"))?;
    let rendered = parse(rendered).with_context(|| format!("parse rendered {target}"))?;
    Ok(kept(&current, &rendered))
}

/// A host file with one key no template sets, standing in for a key a client writes at run time.
fn with_host_key(target: &str, contents: &str) -> Result<String> {
    match Path::new(target).extension().and_then(|e| e.to_str()) {
        Some("json") => {
            let mut value: Value = serde_json::from_str(contents)?;
            value
                .as_object_mut()
                .with_context(|| format!("{target} is not a JSON object"))?
                .insert("dotfilesHostKey".into(), json!("kept"));
            Ok(value.to_string())
        }
        Some("toml") => Ok(format!(
            "dotfiles_host_key = \"kept\"
{contents}"
        )),
        _ => keeps_host_keys(target, contents, contents).map(|_| contents.to_owned()),
    }
}

/// Targets changed outside chezmoi since the last apply that the next apply would overwrite.
/// A `modify_` target passes only when its rendering keeps every key the host file has.
pub fn uncommitted_host_edits(
    root: &Path,
    state: &Path,
    config: &Path,
    destination: &Path,
) -> Result<Vec<String>> {
    let status = output(
        chezmoi(root, state, config, destination).args(["status", "--exclude", "scripts"]),
        "read target status",
    )?;
    let merged = merged_targets(
        &output(
            chezmoi(root, state, config, destination).args([
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
                chezmoi(root, state, config, destination)
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

/// The refusal for a `modify_` template whose rendering drops a key the host file has.
pub fn dropped_host_key_refusal(profile: &str, target: &str) -> String {
    format!(
        "{profile} profile renders {target} from a modify_ template that drops a key the client wrote, so application would refuse every runtime change there\nSet the template's keys over .chezmoi.stdin with mergeOverwrite, as dot_claude/modify_settings.json does, then run `just profiles`"
    )
}

/// The refusal for targets changed outside chezmoi that the next apply would overwrite.
pub fn host_edit_refusal(
    changed: &[String],
    config: &Path,
    destination: &Path,
    state: &Path,
) -> String {
    let paths = format!(
        "'{}' '{}' '{}'",
        config.display(),
        destination.display(),
        state.display()
    );
    format!(
        "{} managed files changed outside chezmoi since the last application, and nothing has been changed: {changed:?}\nRun `just diff {paths}` to see each change, carry it into the source or undo it on the host, then run `just apply {paths} <fresh backup directory>`",
        changed.len()
    )
}

/// Forgets which `run_onchange_` and `run_once_` scripts already ran, so the next apply reruns them.
pub fn forget_script_runs(
    root: &Path,
    scope: &Path,
    config: &Path,
    destination: &Path,
) -> Result<()> {
    let dump = output(
        chezmoi(root, scope, config, destination).args(["state", "dump", "--format", "json"]),
        "read chezmoi state",
    )?;
    let buckets: Value = serde_json::from_slice(&dump.stdout)?;
    for bucket in ["entryState", "scriptState"] {
        if buckets.get(bucket).is_some() {
            output(
                chezmoi(root, scope, config, destination).args([
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

pub fn native_action_arguments(name: &str) -> Vec<&str> {
    if name == "verify" {
        vec![name, "--exclude", "scripts"]
    } else {
        vec![name]
    }
}

/// Packaged tools the rendered scripts and installed entry points require that the profile's platform data omits.
pub fn unprovisioned(
    profile: Profile,
    platform: &Value,
    scripts: &str,
    files: &str,
) -> Vec<String> {
    let mut required = crate::setup::scripted_requirements(profile, scripts);
    required.extend(crate::setup::entry_requirements(files));
    required.sort();
    required.dedup();
    required
        .into_iter()
        .filter_map(|tool| tool.package(profile))
        .filter(|(pointer, entry)| {
            let list = platform.pointer(pointer);
            !(list
                .and_then(Value::as_array)
                .is_some_and(|items| items.iter().any(|item| item == entry))
                || list
                    .and_then(Value::as_object)
                    .is_some_and(|items| items.get(*entry).is_some_and(|spec| spec != "")))
        })
        .map(|(pointer, entry)| format!("{pointer}/{entry}"))
        .collect()
}

/// Script targets in the order chezmoi runs them: every `run_before_` script, then every `run_after_` script, each group by target name.
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
pub fn managed_paths(stdout: &[u8]) -> Result<Vec<String>> {
    stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| Ok(std::str::from_utf8(path)?.to_owned()))
        .collect()
}

/// Managed directories with no managed file or symlink beneath them.
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
    let source_data: Value = serde_json::from_slice(&fs::read(root.join(".chezmoidata.json"))?)?;
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
        let memory = crate::agent_memory::violations(&dump)?;
        ensure!(
            memory.is_empty(),
            "{}",
            crate::agent_memory::refusal(profile.name(), &memory)
        );
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
        let order = script_order(
            &output(
                chezmoi(root, &scope, &config, &destination)
                    .arg("--override-data")
                    .arg(&overrides)
                    .args([
                        "managed",
                        "--include",
                        "scripts",
                        "--path-style",
                        "all",
                        "--format",
                        "json",
                    ]),
                &format!("{} script order", profile.name()),
            )?
            .stdout,
        )?;
        let sequence: Vec<_> = order
            .iter()
            .flat_map(|target| {
                crate::setup::scripted_steps(dump[target]["contents"].as_str().unwrap_or_default())
            })
            .collect();
        let scripts: String = order
            .iter()
            .filter_map(|target| dump[target]["contents"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let files: String = dump
            .as_object()
            .into_iter()
            .flat_map(|targets| targets.iter())
            .filter(|(target, _)| !order.contains(*target))
            .filter_map(|(_, entry)| entry["contents"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let platform = &source_data["platforms"][profile.name()];
        let unprovisioned = unprovisioned(profile, platform, &scripts, &files);
        ensure!(
            unprovisioned.is_empty(),
            "{} profile runs setup or installs entry points that need packages its data does not install: {unprovisioned:?}",
            profile.name()
        );
        let violations = crate::setup::ordering_violations(&sequence);
        ensure!(
            violations.is_empty(),
            "{} profile runs a setup step before a step it relies on: {violations:?} in {order:?}",
            profile.name()
        );
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
        let merged = merged_targets(
            &output(
                chezmoi(root, &scope, &config, &destination)
                    .arg("--override-data")
                    .arg(&overrides)
                    .args([
                        "managed",
                        "--include",
                        "files",
                        "--path-style",
                        "all",
                        "--format",
                        "json",
                    ]),
                &format!("{} merged targets", profile.name()),
            )?
            .stdout,
        )?;
        for target in merged {
            let path = destination.join(&target);
            let host = with_host_key(
                &target,
                &crate::agent_memory::reenabled(&target, &fs::read_to_string(&path)?)?,
            )?;
            fs::write(&path, &host)?;
            let rendered = output(
                chezmoi(root, &scope, &config, &destination)
                    .arg("--override-data")
                    .arg(&overrides)
                    .arg("cat")
                    .arg(&path),
                &format!("{} render {target}", profile.name()),
            )?;
            let rendered = String::from_utf8_lossy(&rendered.stdout).into_owned();
            ensure!(
                keeps_host_keys(&target, &host, &rendered)?,
                "{}",
                dropped_host_key_refusal(profile.name(), &target)
            );
            let mut reapplied = dump.clone();
            reapplied[&target] = json!({ "contents": rendered });
            let memory: Vec<String> = crate::agent_memory::violations(&reapplied)?
                .into_iter()
                .map(|found| format!("{found} after the host file turned memory back on"))
                .collect();
            ensure!(
                memory.is_empty(),
                "{}",
                crate::agent_memory::refusal(profile.name(), &memory)
            );
        }
        output(
            Tool::Git
                .command()
                .args(["config", "--file"])
                .arg(destination.join(if profile == Profile::Windows {
                    ".config/git/config"
                } else {
                    ".gitconfig"
                }))
                .args(["--list"]),
            "parse rendered Git configuration",
        )?;
        let attributes = output(
            Tool::Git
                .command()
                .args(["config", "--file"])
                .arg(destination.join(if profile == Profile::Windows {
                    ".config/git/config"
                } else {
                    ".gitconfig"
                }))
                .args(["--get", "core.attributesFile"]),
            "read the rendered global attributes setting",
        )?;
        ensure!(
            String::from_utf8(attributes.stdout)?.trim() == "~/.config/git/attributes"
                && fs::read_to_string(destination.join(".config/git/attributes"))?
                    .lines()
                    .any(|line| line == "* text=auto eol=lf"),
            "{} does not install the global LF attributes; set core.attributesFile to ~/.config/git/attributes and render dot_config/git/attributes for this profile",
            profile.name()
        );
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

/// A setup script a machine-local configuration leaves out, with the reason that makes the omission reviewable.
#[derive(Debug, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkippedScript {
    pub script: String,
    pub reason: String,
}

/// `setup.skip` entries, which `.chezmoiignore` keeps chezmoi from applying.
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
            "setup.skip names a script target such as 10-install-tools.sh: {}",
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
            crate::canonical(destination)? == crate::canonical(Path::new(&home))?,
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
        let scripts = output(
            chezmoi(root, scope.path(), config, destination).args([
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
            &mut crate::runtime::Native::new(destination)?,
            &crate::setup::scripted_steps(&contents),
        )?;
        let changed = uncommitted_host_edits(root, state, config, destination)?;
        ensure!(
            changed.is_empty(),
            "{}",
            host_edit_refusal(&changed, config, destination, state)
        );
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
                execute("apply")?;
                // Scripts can install programs that templates probe for, so render the files again on the resulting machine before verification.
                let status = chezmoi(root, state, config, destination)
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
                        chezmoi(root, state, config, destination)
                            .args(arguments)
                            .output()
                            .map(|output| {
                                let text = String::from_utf8_lossy(&output.stdout);
                                text.lines().take(200).collect::<Vec<_>>().join("\n")
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

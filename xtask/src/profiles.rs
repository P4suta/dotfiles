//! The profile gate: every profile rendered, applied, and verified in an owned fixture, and the generated pages of the source data.

use crate::profile_rules::Profile;
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
pub use dotfiles::profiles::{
    NativeAction, SkippedScript, Store, chezmoi, chezmoi_with, configured_profile, declared_skips,
    externally_changed, host_edit_refusal, keeps_host_keys, managed_paths, merged_targets,
    native_action_arguments, native_profile, operate, output, pending_onchange_scripts,
    reset_entry_and_script_state, script_order, source_data, uncommitted_host_edits,
};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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

/// Refuses runtime credentials, private key material, and secret retrieval in managed templates anywhere in the public source.
///
/// # Errors
///
/// Returns an error when the source has no files or it can't read a file.
/// Returns an error when a file has a credential path or private key material, or a managed template retrieves a secret.
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
            if crate::has_extension(&relative, "tmpl")
                || relative.starts_with("home/.chezmoitemplates/")
            {
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

/// Refuses any source file that contains a value from the JSON denylist.
///
/// # Errors
///
/// Returns an error when it can't read the denylist, the denylist holds something other than a JSON list of strings, or the list has no entries.
/// It also fails when the denylist holds an empty value.
/// Returns an error when the source has no files, it can't read a file, or a file contains a denylisted value.
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

/// chezmoi reports `.chezmoi.homeDir`, `.chezmoi.sourceDir`, and `.chezmoi.workingTree` with forward slashes on every host, including Windows.
#[must_use]
pub fn chezmoi_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if cfg!(windows) {
        text.replace('\\', "/")
    } else {
        text.into_owned()
    }
}

/// The machine-local configuration template, relative to the source state.
pub const CONFIG_TEMPLATE: &str = ".chezmoi.toml.tmpl";

/// Answers to the prompts of the configuration template.
#[derive(Clone, Debug)]
pub struct Answers {
    pub profile: Profile,
    pub role: &'static str,
    pub name: &'static str,
    pub email: &'static str,
    pub public_key: String,
    pub doppler: &'static str,
    pub projects: String,
}

impl Answers {
    /// The anonymous answers of a gate fixture: the personal role, no signing key, and a fixture Doppler project.
    #[must_use]
    pub fn fixture(profile: Profile, projects: String) -> Self {
        Self {
            profile,
            role: "personal",
            name: "Example User",
            email: "user@example.invalid",
            public_key: String::new(),
            doppler: "fixture",
            projects,
        }
    }
}

/// The configuration that `chezmoi init` writes from the template for these answers.
///
/// # Errors
///
/// Returns an error when reading the template fails or chezmoi can't render it.
pub fn configuration(root: &Path, answers: &Answers) -> Result<String> {
    let template = fs::read_to_string(crate::source_state(root).join(CONFIG_TEMPLATE))?;
    let scope = tempfile::tempdir()?;
    let config = scope.path().join("chezmoi.toml");
    fs::write(&config, "")?;
    let mut command = Tool::chezmoi_in(root);
    command
        .arg("--config")
        .arg(&config)
        .arg("--cache")
        .arg(scope.path().join("cache"))
        .arg("--persistent-state")
        .arg(scope.path().join("state.boltdb"))
        .args(["execute-template", "--init", "--no-tty"])
        .arg("--promptChoice")
        .arg(format!("Dotfiles profile={}", answers.profile.name()))
        .arg("--promptChoice")
        .arg(format!("Machine role={}", answers.role))
        .arg("--promptString")
        .arg(format!("Git author name={}", answers.name))
        .arg("--promptString")
        .arg(format!("Git author email={}", answers.email))
        .arg("--promptString")
        .arg(format!(
            "SSH signing public key (empty disables signing)={}",
            answers.public_key
        ))
        .arg("--promptString")
        .arg(format!("Doppler project (optional)={}", answers.doppler))
        .arg("--promptString")
        .arg(format!("Doppler config (optional)={}", answers.doppler))
        .arg("--promptString")
        .arg(format!("Project checkout directory={}", answers.projects))
        .arg(template)
        .stdin(std::process::Stdio::null());
    let result = output(&mut command, "render the configuration template")?;
    Ok(String::from_utf8(result.stdout)?)
}

/// The `data` table of the configuration for these answers, rendered once per distinct answer set.
///
/// # Errors
///
/// Returns an error when rendering fails or the configuration holds no `data` table.
pub fn configuration_data(root: &Path, answers: &Answers) -> Result<Value> {
    static RENDERED: std::sync::Mutex<std::collections::BTreeMap<String, Value>> =
        std::sync::Mutex::new(std::collections::BTreeMap::new());
    let key = format!("{}\n{answers:?}", root.display());
    if let Some(data) = RENDERED
        .lock()
        .map_err(|_| anyhow::anyhow!("configuration cache poisoned"))?
        .get(&key)
    {
        return Ok(data.clone());
    }
    let table: toml::Table = toml::from_str(&configuration(root, answers)?)?;
    let data = serde_json::to_value(table.get("data").context("configuration without data")?)?;
    RENDERED
        .lock()
        .map_err(|_| anyhow::anyhow!("configuration cache poisoned"))?
        .insert(key, data.clone());
    Ok(data)
}

/// The host facts a fixture shows to templates under `.chezmoi`.
#[must_use]
pub fn fixture_host(profile: Profile, source: &str, home: &str) -> Value {
    json!({"os": profile.os(), "homeDir": home, "sourceDir": source,
        "workingTree": source, "username": "fixture", "hostname": "fixture.invalid",
        "kernel": {"osrelease": if profile == Profile::Wsl {"fixture-microsoft"} else {"fixture"}}})
}

/// The data of a gate fixture: the configuration template's answer for an anonymous host, with fixed host facts.
///
/// # Errors
///
/// Returns an error when rendering the configuration template fails.
pub fn fixture(profile: Profile, root: &Path, destination: &Path) -> Result<Value> {
    let mut data = configuration_data(
        root,
        &Answers::fixture(
            profile,
            destination.join("projects").to_string_lossy().into_owned(),
        ),
    )?;
    data["chezmoi"] = fixture_host(profile, &chezmoi_path(root), &chezmoi_path(destination));
    Ok(data)
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

/// The refusal for a `modify_` template whose rendering drops a key the host file has.
#[must_use]
pub fn dropped_host_key_refusal(profile: &str, target: &str) -> String {
    format!(
        "{profile} profile renders {target} from a modify_ template that drops a key the client wrote, so application would refuse every runtime change there\nSet the template's keys over .chezmoi.stdin with mergeOverwrite, as dot_claude/modify_settings.json does, then run `just profiles`"
    )
}

/// Packaged tools the rendered scripts and installed entry points require that the profile's platform data omits.
#[must_use]
pub fn unprovisioned(
    profile: Profile,
    platform: &Value,
    scripts: &str,
    files: &str,
) -> Vec<String> {
    let mut required = dotfiles::setup::scripted_requirements(profile, scripts);
    required.extend(dotfiles::setup::entry_requirements(files));
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

/// Managed directories with no managed file or symlink beneath them.
#[must_use]
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

/// The hooks every host runs from `core.hooksPath`.
pub const GIT_HOOKS: [&str; 7] = [
    "commit-msg",
    "post-checkout",
    "post-commit",
    "post-merge",
    "pre-commit",
    "pre-push",
    "reference-transaction",
];

/// Git settings that name a host's own executables or file watcher, the only settings in which hosts may differ.
pub const DECLARED_GIT_SETTINGS: [&str; 4] = [
    "core.sshcommand",
    "core.fsmonitor",
    "core.untrackedcache",
    "gpg.ssh.program",
];

/// The `git config --list` entries of one host without the declared per-host settings.
#[must_use]
pub fn policy(listed: &str) -> Vec<String> {
    listed
        .lines()
        .filter(|line| {
            !DECLARED_GIT_SETTINGS
                .iter()
                .any(|key| line.split_once('=').is_some_and(|(name, _)| name == *key))
        })
        .map(str::to_owned)
        .collect()
}

/// The file targets of targets.toml whose set holds `profile` but that the profile's rendering lacks.
/// A directory or a pattern renders no entry of its own, so the check skips them.
#[must_use]
pub fn absent_targets(data: &Value, profile: Profile, dump: &Value) -> Vec<String> {
    let receives = |set: &str| {
        data["profiles"][set]
            .as_array()
            .is_some_and(|members| members.iter().any(|member| member == profile.name()))
    };
    data["targets"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(target, set)| {
            !target.contains('*')
                && set.as_str().is_some_and(receives)
                && dump.get(target.as_str()).is_none()
                && !dump.as_object().is_some_and(|rendered| {
                    rendered
                        .keys()
                        .any(|key| key.starts_with(&format!("{target}/")))
                })
        })
        .map(|(target, _)| target.clone())
        .collect()
}

/// Refusals for profile sets that name an unknown profile or hold every profile, and for targets and retired entries that name no set.
#[must_use]
pub fn manifest_findings(data: &Value) -> Vec<String> {
    let mut found = Vec::new();
    let sets = data["profiles"].as_object();
    for (name, members) in sets.into_iter().flatten() {
        let members: Vec<&str> = members
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        for member in &members {
            if !Profile::ALL.iter().any(|profile| profile.name() == *member) {
                found.push(format!(
                    "profiles.toml: set {name} names the unknown profile {member}"
                ));
            }
        }
        if Profile::ALL
            .iter()
            .all(|profile| members.contains(&profile.name()))
        {
            found.push(format!(
                "profiles.toml: set {name} holds every profile, so an entry that uses it restricts nothing"
            ));
        }
    }
    for listing in ["targets", "scripts", "retired"] {
        for (target, set) in data[listing].as_object().into_iter().flatten() {
            if set
                .as_str()
                .is_none_or(|set| sets.is_none_or(|sets| !sets.contains_key(set)))
            {
                found.push(format!(
                    "{listing}.toml: {target} names no set of profiles.toml"
                ));
            }
        }
    }
    found
}

/// Where the generated reference of profile-specific targets lives, relative to the checkout.
pub const TARGETS_REFERENCE: &str = "docs/reference/targets.md";

/// The reference page that lists which profiles receive each restricted target and which retired targets each profile removes.
#[must_use]
pub fn targets_reference(data: &Value) -> String {
    let members = |set: &str| {
        data["profiles"][set]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut text = String::from(
        "# Profile targets\n\n`just gen` writes this page from `home/.chezmoidata/`.\nA target missing from the first table reaches every profile.\n\n| Target | Set | Profiles |\n| --- | --- | --- |\n",
    );
    let rows = |text: &mut String, listing: &str| {
        for (target, set) in data[listing].as_object().into_iter().flatten() {
            let set = set.as_str().unwrap_or_default();
            text.extend(["| `", target, "` | ", set, " | ", &members(set), " |\n"]);
        }
    };
    rows(&mut text, "targets");
    text.push_str(
        "\n## Setup launchers\n\n`just gen` writes each launcher from its setup step, and only the profiles of its set run it.\n\n| Target | Set | Profiles |\n| --- | --- | --- |\n",
    );
    rows(&mut text, "scripts");
    text.push_str(
        "\n## Retired targets\n\nEach profile in the set removes the target when it applies.\n\n| Target | Set | Profiles |\n| --- | --- | --- |\n",
    );
    rows(&mut text, "retired");
    text
}

/// Writes the reference of profile-specific targets, or with `check` refuses a stale one.
///
/// # Errors
///
/// Returns an error when loading the source data or writing the page fails, or with `check` when the committed page differs.
pub fn targets_page(root: &Path, check: bool) -> Result<()> {
    let path = root.join(TARGETS_REFERENCE);
    let wanted = targets_reference(&source_data(root)?);
    if check {
        ensure!(
            fs::read_to_string(&path).is_ok_and(|current| current == wanted),
            "{TARGETS_REFERENCE} differs from home/.chezmoidata; run `just gen` and commit the result"
        );
    } else {
        fs::create_dir_all(path.parent().context("reference directory")?)?;
        fs::write(&path, wanted)?;
    }
    Ok(())
}

/// Where the JSON schema of the data files lives, relative to the checkout.
pub const DATA_SCHEMA: &str = "schemas/chezmoidata.schema.json";

/// Writes the JSON schema of the data files, or with `check` refuses a stale one.
///
/// # Errors
///
/// Returns an error when writing the schema fails, or with `check` when the committed schema differs from the model.
pub fn data_schema(root: &Path, check: bool) -> Result<()> {
    let repository_schema = root.join("schemas/repository.schema.json");
    let repository_wanted = model::repository::schema();
    if check {
        ensure!(
            fs::read_to_string(&repository_schema)
                .is_ok_and(|current| current == repository_wanted),
            "repository schema differs from its model; run `just gen`"
        );
    } else {
        fs::write(repository_schema, repository_wanted)?;
    }
    let path = root.join(DATA_SCHEMA);
    let wanted = model::data::schema();
    if check {
        ensure!(
            fs::read_to_string(&path).is_ok_and(|current| current == wanted),
            "{DATA_SCHEMA} differs from the data model; run `just gen` and commit the result"
        );
    } else {
        fs::create_dir_all(path.parent().context("schema directory")?)?;
        fs::write(&path, wanted)?;
    }
    Ok(())
}

/// Renders, applies, and verifies every profile in an owned fixture after the public source check.
///
/// # Errors
///
/// Returns an error when the public source check fails, the report directory already exists, or a profile fails to render, apply, or verify.
/// Returns an error when the Git configuration of a profile differs from the others beyond the declared per-host settings.
pub fn check_profiles(root: &Path, report: Option<&Path>) -> Result<()> {
    check_public(root)?;
    if let Some(report) = report {
        fs::create_dir(report).context("profile report requires a fresh output directory")?;
    }
    let scratch = tempfile::tempdir()?;
    let mut git_policies = Vec::new();
    let source_data = source_data(root)?;
    let findings = manifest_findings(&source_data);
    ensure!(findings.is_empty(), "{}", findings.join("\n"));
    for profile in Profile::ALL {
        let fixture = Fixture::new(root, &scratch.path().join(profile.name()), profile)?;
        let dump = fixture.render(report)?;
        fixture.check_setup(&dump, &source_data["platforms"][profile.name()])?;
        fixture.apply_files()?;
        fixture.check_merged_targets(&dump)?;
        git_policies.push((profile, fixture.git_policy()?));
        println!(
            "Rendered, applied twice, and verified {} in an owned fixture",
            profile.name()
        );
    }
    let differing: Vec<_> = git_policies
        .iter()
        .filter(|(_, policy)| *policy != git_policies[0].1)
        .map(|(profile, _)| profile.name())
        .collect();
    ensure!(
        differing.is_empty(),
        "the Git configuration of {differing:?} differs from {} beyond the declared per-host settings {DECLARED_GIT_SETTINGS:?}",
        git_policies[0].0.name()
    );
    Ok(())
}

/// One profile rendered into an owned destination with fixture data.
struct Fixture<'a> {
    root: &'a Path,
    profile: Profile,
    scope: PathBuf,
    config: PathBuf,
    destination: PathBuf,
    overrides: String,
}

impl<'a> Fixture<'a> {
    fn new(root: &'a Path, scope: &Path, profile: Profile) -> Result<Self> {
        let destination = scope.join("home with ' quote");
        fs::create_dir_all(&destination)?;
        let config = scope.join("config.json");
        let data = fixture(profile, root, &destination)?;
        fs::write(&config, serde_json::to_vec(&json!({"data": data}))?)?;
        let overrides = serde_json::to_string(&data)?;
        Ok(Self {
            root,
            profile,
            scope: scope.to_path_buf(),
            config,
            destination,
            overrides,
        })
    }

    /// chezmoi on this fixture, with the fixture data overriding what the host provides.
    fn chezmoi(&self) -> Command {
        let mut command = chezmoi(self.root, &self.scope, &self.config, &self.destination);
        command.arg("--override-data").arg(&self.overrides);
        command
    }

    fn run(&self, arguments: &[&str], phase: &str) -> Result<Output> {
        output(
            self.chezmoi().args(arguments),
            &format!("{} {phase}", self.profile.name()),
        )
    }

    fn managed(&self, include: &str) -> Result<Vec<String>> {
        let listed = self.run(
            &[
                "managed",
                "--include",
                include,
                "--exclude",
                "externals",
                "--nul-path-separator",
            ],
            &format!("managed {include}"),
        )?;
        managed_paths(&listed.stdout)
    }

    /// The rendered targets, after the availability contracts, the syntax of every rendered file, and the agent memory switches pass.
    fn render(&self, report: Option<&Path>) -> Result<Value> {
        let profile = self.profile;
        let mut command = self.chezmoi();
        command
            .env_remove("SSH_AUTH_SOCK")
            .env_remove("DOTFILES_SSH_AUTH_SOCK")
            .env_remove("SSH_CONNECTION")
            .env_remove("SSH_CLIENT")
            .env_remove("SSH_TTY")
            .env("DOTFILES_ONEPASSWORD_APP", self.scope.join("absent-app"))
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
        let missing = absent_targets(&source_data(self.root)?, profile, &dump);
        ensure!(
            missing.is_empty(),
            "{} profile renders no source for these targets of targets.toml: {missing:?}",
            profile.name()
        );
        crate::quality::rendered(&dump, &self.scope.join("syntax"))?;
        let memory = crate::agent_memory::violations(&dump)?;
        ensure!(
            memory.is_empty(),
            "{}",
            crate::agent_memory::refusal(profile.name(), &memory)
        );
        fs::write(self.scope.join("render.json"), &result.stdout)?;
        if let Some(report) = report {
            fs::write(
                report.join(format!("{}.json", profile.name())),
                &result.stdout,
            )?;
        }
        Ok(dump)
    }

    /// The setup scripts run in dependency order, every entry point has its packages, and no managed directory stays empty.
    fn check_setup(&self, dump: &Value, platform: &Value) -> Result<()> {
        let profile = self.profile;
        let order = script_order(
            &self
                .run(
                    &[
                        "managed",
                        "--include",
                        "scripts",
                        "--path-style",
                        "all",
                        "--format",
                        "json",
                    ],
                    "script order",
                )?
                .stdout,
        )?;
        // The launchers map each rendered script to the step it runs, which the bootstrap launchers of `mise` and the runtime can't name through `dotfiles setup`.
        let launchers = crate::launchers::launchers()?;
        let sequence: Vec<platform::step::Step> = order
            .iter()
            .map(|target| {
                launchers
                    .iter()
                    .find(|launcher| launcher.target == *target)
                    .map(|launcher| launcher.step)
                    .with_context(|| format!("{target} names no setup launcher"))
            })
            .collect::<Result<_>>()?;
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
        let unprovisioned = unprovisioned(profile, platform, &scripts, &files);
        ensure!(
            unprovisioned.is_empty(),
            "{} profile runs setup or installs entry points that need packages its data does not install: {unprovisioned:?}",
            profile.name()
        );
        let violations = dotfiles::setup::ordering_violations(profile, &sequence);
        ensure!(
            violations.is_empty(),
            "{} profile runs its setup steps out of their declared order: {violations:?} in {order:?}",
            profile.name()
        );
        let empty = empty_directories(&self.managed("dirs")?, &self.managed("files,symlinks")?);
        ensure!(
            empty.is_empty(),
            "{} profile would create or change directories it manages nothing in: {}",
            profile.name(),
            empty.join(", ")
        );
        Ok(())
    }

    /// Applies and verifies the files twice, and checks that a retired target leaves the host.
    fn apply_files(&self) -> Result<()> {
        let preserved = crate::rehearsal::seed_history(&self.destination)?;
        let retired = self.destination.join(".config/shell/github-token.sh");
        fs::create_dir_all(retired.parent().context("fixture parent is missing")?)?;
        fs::write(&retired, "obsolete credential exporter fixture")?;
        for action in ["apply", "verify", "apply", "verify"] {
            self.run(
                &[
                    "--force",
                    action,
                    "--include",
                    "dirs,files,symlinks,remove",
                    "--exclude",
                    "externals",
                ],
                &format!("isolated files-only {action}"),
            )?;
        }
        ensure!(
            self.profile == Profile::Windows || !retired.exists(),
            "obsolete credential exporter was preserved"
        );
        for (path, contents) in preserved {
            ensure!(
                fs::read(&path)? == contents,
                "profile application changed machine-local state: {}",
                path.display()
            );
        }
        Ok(())
    }

    /// Every `modify_` target keeps the host's own keys and keeps agent memory off after the host turned it back on.
    fn check_merged_targets(&self, dump: &Value) -> Result<()> {
        let profile = self.profile;
        let merged = merged_targets(
            &self
                .run(
                    &[
                        "managed",
                        "--include",
                        "files",
                        "--path-style",
                        "all",
                        "--format",
                        "json",
                    ],
                    "merged targets",
                )?
                .stdout,
        )?;
        for target in merged {
            let path = self.destination.join(&target);
            let host = with_host_key(
                &target,
                &crate::agent_memory::reenabled(&target, &fs::read_to_string(&path)?)?,
            )?;
            fs::write(&path, &host)?;
            let rendered = output(
                self.chezmoi().arg("cat").arg(&path),
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
        Ok(())
    }

    /// The rendered Git configuration, after it installs the LF attributes and every hook.
    fn git_policy(&self) -> Result<Vec<String>> {
        let profile = self.profile;
        let destination = &self.destination;
        let git_config = destination.join(".config/git/config");
        let listed = output(
            Tool::Git
                .command()
                .args(["config", "--file"])
                .arg(&git_config)
                .args(["--list"]),
            "parse rendered Git configuration",
        )?;
        let listed = String::from_utf8(listed.stdout)?;
        let setting = |key: &str| {
            listed
                .lines()
                .filter_map(|line| line.split_once('='))
                .filter(|(name, _)| *name == key)
                .map(|(_, value)| value.to_owned())
                .next_back()
        };
        ensure!(
            setting("core.attributesfile").as_deref() == Some("~/.config/git/attributes")
                && fs::read_to_string(destination.join(".config/git/attributes"))?
                    .lines()
                    .any(|line| line == "* text=auto eol=lf"),
            "{} does not install the global LF attributes; set core.attributesFile to ~/.config/git/attributes and render dot_config/git/attributes for this profile",
            profile.name()
        );
        let missing: Vec<_> = GIT_HOOKS
            .iter()
            .filter(|hook| !destination.join(".config/git/hooks").join(hook).is_file())
            .collect();
        ensure!(
            setting("core.hookspath").as_deref() == Some("~/.config/git/hooks")
                && missing.is_empty(),
            "{} does not run every Git hook: core.hooksPath must name ~/.config/git/hooks and these hooks must render: {missing:?}",
            profile.name()
        );
        Ok(policy(&listed))
    }
}

//! Cross-platform push evidence: managed SSH access to one host per OS family, gate runs through domyjob, and Git notes that pre-push and `pr-workflow ready` require.

use crate::hosts_rules::{Evidence, Family, Key, Start, Verdict, admit, gated, pin, record, start};
use crate::runtime::Real;
use crate::tool::Tool;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use yaml_rust2::{Yaml, YamlLoader};

pub const NOTES: &str = "refs/notes/hosts";
const STATE: &str = ".local/state/hosts";
const DEFAULT_GATE: &str = "just check";
const STAGE_REPORT: &str = "DOTFILES_HOST_STAGES ";
const CODE_SHELL: &str = "/bin/sh";
const RELEASE_PINS: &str = "home/.chezmoidata/versions.toml";
const PROFILE_ROOT: &str = ".chezmoiroot";
const PROFILE_RUNTIME: &str = "crates/dotfiles/Cargo.toml";
const JOB_POLL_INTERVAL: Duration = Duration::from_secs(1);
const REQUEST_POLL_INTERVAL: Duration = Duration::from_millis(20);
const REQUEST_LIMIT: Duration = Duration::from_secs(300);
const INPUT_SCHEMA: &[u8] = b"hosts-stage-inputs-v1\0";
const REMOVE_WORKSPACE: &str = "/bin/rm";
const DISK_SPACE: &str = "/bin/df";
const BYTES_PER_KIB: u64 = 1024;
const DF_AVAILABLE_COLUMN: usize = 3;
const SHARED_INPUTS: [&str; 6] = [
    "mise.toml",
    "mise.lock",
    RELEASE_PINS,
    "crates/dotfiles/src/hosts.rs",
    "crates/dotfiles/src/host_checks.rs",
    "crates/platform/src/code_environment.rs",
];

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Code,
    Real,
}

impl Stage {
    const ALL: [Self; 2] = [Self::Code, Self::Real];

    const fn index(self) -> usize {
        match self {
            Self::Code => 0,
            Self::Real => 1,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Real => "real",
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Stages {
    code: i32,
    real: i32,
    applied: Option<crate::host_checks::Applied>,
    #[serde(default)]
    workspace: Option<String>,
}

#[derive(Debug, clap::Subcommand)]
pub enum Action {
    /// Check tailnet presence, SSH through the agent, domyjob, and the gate's toolchain on every managed host.
    Doctor {
        #[arg(long, default_value = DEFAULT_GATE)]
        gate: String,
    },
    /// Regenerate the managed SSH aliases and pinned host keys from the tailnet.
    Sync {
        /// Choose this tailnet host for its OS family.
        #[arg(long)]
        host: Vec<String>,
        /// Remote user.
        /// Defaults to the managed one, then to the user of a hub remote.
        #[arg(long)]
        user: Option<String>,
        /// Replace this host's pinned key with the one it presents now.
        #[arg(long)]
        accept: Vec<String>,
    },
    /// Run the gate for the clean head commit on every operating system family in the continuous integration matrix and record each result as a Git note.
    Check {
        #[arg(long, default_value = DEFAULT_GATE)]
        gate: String,
    },
    /// Run the code stage in the environment built from this checkout's installed pins.
    Code {
        #[arg(long, default_value = DEFAULT_GATE)]
        gate: String,
    },
    /// Verify native provisioning and network access without an SSH agent.
    Real,
    #[command(hide = true)]
    Worker {
        #[arg(long, default_value = DEFAULT_GATE)]
        gate: String,
        #[arg(long)]
        home: Option<PathBuf>,
        #[arg(long)]
        skip_code: bool,
        #[arg(long)]
        skip_real: bool,
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Refuse pushed GitHub branches that lack a passing note for an operating system family in their CI matrix.
    /// Reads pre-push input.
    Gate { remote: String, url: String },
}

/// Run one `hosts` subcommand in this user's home directory.
///
/// # Errors
///
/// Returns an error when the home directory stays unavailable, reading standard input fails, or the chosen subcommand fails.
pub fn run(action: Action) -> Result<()> {
    match action {
        Action::Doctor { gate } => doctor(&home()?, &gate),
        Action::Sync { host, user, accept } => sync(&home()?, &host, user, &accept),
        Action::Check { gate } => check(&home()?, &gate),
        Action::Code { gate } => code(&home()?, &gate),
        Action::Real => crate::host_checks::check(&std::env::current_dir()?, &home()?).candidate,
        Action::Worker {
            gate,
            home: native_home,
            skip_code,
            skip_real,
            workspace,
        } => worker(
            &native_home.map_or_else(home, Ok)?,
            &gate,
            [skip_code, skip_real],
            workspace.as_deref(),
        ),
        Action::Gate { url, .. } => {
            let mut input = Vec::new();
            std::io::Read::read_to_end(&mut std::io::stdin(), &mut input)?;
            push_gate(Path::new("."), &url, &input)
        }
    }
}

fn home() -> Result<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .context("home directory is unavailable")
}

/// The family a runner label names, such as `ubuntu-latest` or `macos-14`.
#[must_use]
pub fn label_family(label: &str) -> Option<Family> {
    let label = label.trim().to_ascii_lowercase();
    if label.starts_with("ubuntu") || label == "linux" {
        Some(Family::Linux)
    } else if label.starts_with("macos") {
        Some(Family::Mac)
    } else if label.starts_with("windows") {
        Some(Family::Windows)
    } else {
        None
    }
}

/// The family Tailscale reports for a node.
#[must_use]
pub fn os_family(os: &str) -> Option<Family> {
    match os {
        "linux" => Some(Family::Linux),
        "macOS" => Some(Family::Mac),
        "windows" => Some(Family::Windows),
        _ => None,
    }
}

fn strings(value: &Yaml) -> Vec<String> {
    match value {
        Yaml::String(text) => vec![text.clone()],
        Yaml::Array(items) => items.iter().flat_map(strings).collect(),
        _ => Vec::new(),
    }
}

fn runner_labels(job: &Yaml, name: &str) -> Result<Vec<String>> {
    let runs_on = &job["runs-on"];
    let labels = match runs_on {
        Yaml::Hash(_) => strings(&runs_on["labels"]),
        other => strings(other),
    };
    let mut resolved = Vec::new();
    for label in labels {
        let Some(expression) = label
            .trim()
            .strip_prefix("${{")
            .and_then(|rest| rest.strip_suffix("}}"))
        else {
            resolved.push(label);
            continue;
        };
        let key = expression.trim().strip_prefix("matrix.").with_context(|| {
            format!("job {name} has a runner expression the gate cannot resolve: {label}")
        })?;
        let matrix = &job["strategy"]["matrix"];
        let mut values = strings(&matrix[key]);
        if let Yaml::Array(includes) = &matrix["include"] {
            values.extend(includes.iter().flat_map(|include| strings(&include[key])));
        }
        ensure!(
            !values.is_empty(),
            "job {name} has no matrix values for {key}"
        );
        resolved.extend(values);
    }
    Ok(resolved)
}

/// The families the workflows' jobs run on, by family index.
/// Reusable-workflow calls have no runner of their own.
///
/// # Errors
///
/// Returns an error when a workflow holds invalid YAML, a runner expression resolves to nothing, or a job's runner names no known family.
pub fn required(workflows: &[(String, String)]) -> Result<[bool; 3]> {
    let mut required = [false; 3];
    for (file, text) in workflows {
        let documents = YamlLoader::load_from_str(text).with_context(|| format!("parse {file}"))?;
        let Some(Yaml::Hash(jobs)) = documents.first().map(|document| &document["jobs"]) else {
            continue;
        };
        for (name, job) in jobs {
            let name = format!("{file}:{}", name.as_str().unwrap_or("?"));
            if job["runs-on"].is_badvalue() {
                continue;
            }
            let families: Vec<_> = runner_labels(job, &name)?
                .iter()
                .filter_map(|label| label_family(label))
                .collect();
            ensure!(
                !families.is_empty(),
                "cannot tell the OS family of job {name}"
            );
            for family in families {
                required[family.index()] = true;
            }
        }
    }
    Ok(required)
}

/// One gate run, appended to the note of the commit it ran for.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub host: String,
    pub os: String,
    pub tree: String,
    pub status: i32,
    pub gate: String,
    #[serde(default)]
    pub stage: Option<Stage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied: Option<crate::host_checks::Applied>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reused: Option<Reused>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<Workspace>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reused {
    pub commit: String,
    pub line: usize,
    pub host: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    pub path: String,
    pub job: String,
    pub reused: [Option<Reused>; 2],
}

fn pending_workspaces(notes: &[(String, String)]) -> Result<BTreeMap<(String, String), Note>> {
    let mut pending = BTreeMap::new();
    for (commit, text) in notes {
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let note: Note = serde_json::from_str(line).context("malformed hosts note")?;
            if let Some(workspace) = &note.workspace {
                let key = (commit.clone(), format!("{}:{}", note.host, workspace.job));
                if crate::hosts_rules::workspace_pending(
                    pending.contains_key(&key),
                    note.stage.is_some(),
                    note.status == 0,
                ) {
                    pending.insert(key, note);
                } else {
                    pending.remove(&key);
                }
            }
        }
    }
    Ok(pending)
}

fn cleanup_arguments(family: Family, workspace: &Workspace) -> Result<Vec<String>> {
    ensure!(
        workspace
            .job
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
            && !workspace.job.is_empty(),
        "invalid workspace job identity"
    );
    let path = &workspace.path;
    ensure!(
        path.ends_with(&format!("/jobs/{}/workspace", workspace.job))
            && !path
                .split('/')
                .any(|component| matches!(component, "." | "..")),
        "cleanup requires the recorded job's workspace directory"
    );
    match family {
        Family::Linux | Family::Mac => {
            ensure!(path.starts_with('/'), "workspace path must be absolute");
            Ok(vec![
                REMOVE_WORKSPACE.into(),
                "-rf".into(),
                "--".into(),
                path.clone(),
            ])
        }
        Family::Windows => {
            ensure!(
                path.starts_with("//")
                    || path
                        .split_once(':')
                        .is_some_and(|(drive, rest)| drive.len() == 1
                            && drive.chars().all(|letter| letter.is_ascii_alphabetic())
                            && rest.starts_with('/')),
                "workspace path must be absolute"
            );
            Ok(wrap(
                family,
                &format!(
                    "$ErrorActionPreference = 'Stop'; $workspace = '{}'; if (Test-Path -LiteralPath $workspace) {{ Remove-Item -LiteralPath $workspace -Recurse -Force }}",
                    path.replace('\'', "''")
                ),
            ))
        }
    }
}

fn append_note(root: &Path, commit: &str, note: &Note) -> Result<()> {
    git(
        root,
        &[
            "notes",
            "--ref",
            NOTES,
            "append",
            "-m",
            &serde_json::to_string(note)?,
            commit,
        ],
    )?;
    Ok(())
}

fn clean_recorded_workspace(native: &Real, root: &Path, commit: &str, note: &Note) -> Result<()> {
    let workspace = note
        .workspace
        .as_ref()
        .context("workspace receipt is missing")?;
    let family = Family::ALL
        .into_iter()
        .find(|family| family.name() == note.os)
        .context("unknown workspace host family")?;
    let (status, output) = domyjob(
        native,
        &note.host,
        &["on"],
        &cleanup_arguments(family, workspace)?,
    )?;
    ensure!(
        status == Some(0),
        "{} {}: cleanup workspace: {output}",
        note.os,
        note.host
    );
    append_note(
        root,
        commit,
        &Note {
            host: note.host.clone(),
            os: note.os.clone(),
            tree: note.tree.clone(),
            status: 0,
            gate: note.gate.clone(),
            stage: None,
            applied: None,
            inputs: None,
            reused: None,
            workspace: Some(workspace.clone()),
        },
    )
}

fn disk_arguments(family: Family, home: Option<&Path>) -> Vec<String> {
    let paths = home.map_or_else(
        || match family {
            Family::Windows => ["$env:USERPROFILE".into(), "$env:TEMP".into()],
            Family::Linux | Family::Mac => ["\"$HOME\"".into(), "\"${TMPDIR:-/tmp}\"".into()],
        },
        |home| {
            [home.to_path_buf(), std::env::temp_dir()].map(|path| {
                let path = path.to_string_lossy();
                match family {
                    Family::Windows => format!("'{}'", path.replace('\'', "''")),
                    Family::Linux | Family::Mac => format!("'{}'", path.replace('\'', "'\"'\"'")),
                }
            })
        },
    );
    let [home, temporary] = paths;
    let script = match family {
        Family::Linux | Family::Mac => format!("LC_ALL=C {DISK_SPACE} -Pk {home} {temporary}"),
        Family::Windows => format!(
            "$ErrorActionPreference = 'Stop'; @({home}, {temporary}) | ForEach-Object {{ [System.IO.DriveInfo]::new([System.IO.Path]::GetPathRoot($_)).AvailableFreeSpace }}"
        ),
    };
    wrap(family, &script)
}

fn space_available(family: Family, output: &str) -> Result<u64> {
    let mut available = None;
    for line in output.lines() {
        let value = match family {
            Family::Windows => line.trim().parse::<u64>().ok(),
            Family::Linux | Family::Mac => {
                if let Some(kib) = line
                    .split_whitespace()
                    .nth(DF_AVAILABLE_COLUMN)
                    .and_then(|value| value.parse::<u64>().ok())
                {
                    Some(
                        kib.checked_mul(BYTES_PER_KIB)
                            .context("free-space byte count overflow")?,
                    )
                } else {
                    None
                }
            }
        };
        if let Some(value) = value {
            available = Some(available.map_or(value, |previous: u64| previous.min(value)));
        }
    }
    available.context("host returned no free-space measurement")
}

fn capacity(native: &Real, family: Family, target: &Target) -> Result<u64> {
    let (status, output) = match target {
        Target::Remote(alias) => domyjob(native, alias, &["on"], &disk_arguments(family, None))?,
        Target::Local(_) => {
            let arguments = disk_arguments(family, Some(&native.home));
            let mut command = platform::process::command(&arguments[0]);
            command.args(&arguments[1..]);
            let (status, output) = job_output(command, Some(REQUEST_LIMIT))?;
            (status.code(), output)
        }
    };
    ensure!(
        status == Some(0),
        "{}: free-space measurement failed: {output}",
        target.name()
    );
    space_available(family, &output).with_context(|| format!("{}: read free space", target.name()))
}

fn require_capacity(native: &Real, family: Family, target: &Target) -> Result<()> {
    let available = capacity(native, family, target)?;
    ensure!(
        crate::hosts_rules::workspace_fits(available),
        "{} has {available} bytes free; one more workspace requires {} bytes",
        target.name(),
        crate::hosts_rules::WORKSPACE_BYTES
    );
    Ok(())
}

fn recover_workspaces(native: &Real, root: &Path) -> Result<()> {
    for ((commit, reference), note) in pending_workspaces(&all_notes(root)?)? {
        let workspace = note
            .workspace
            .as_ref()
            .context("workspace receipt is missing")?;
        let family = Family::ALL
            .into_iter()
            .find(|family| family.name() == note.os)
            .context("unknown workspace host family")?;
        cleanup_arguments(family, workspace)?;
        let mut status = command(native, Tool::Domyjob);
        status.args(["status", &reference]);
        let (exit, status) = job_output(status, Some(REQUEST_LIMIT))?;
        ensure!(exit.success(), "{reference}: recover job status: {status}");
        let mut logs = command(native, Tool::Domyjob);
        logs.args(["logs", &reference]);
        let (exit, logs) = job_output(logs, Some(REQUEST_LIMIT))?;
        ensure!(exit.success(), "{reference}: recover job log: {logs}");
        let failures = record_run(
            native,
            root,
            &commit,
            &note.tree,
            &note.gate,
            &input_hashes(root, &commit, &note.gate)?,
            &GateRun {
                family,
                target: Target::Remote(note.host.clone()),
                status: outcome(&note.host, &status),
                output: format!("{reference}\n{status}\n{logs}"),
                reused: workspace.reused.clone(),
            },
        )?;
        ensure!(
            !pending_workspaces(&all_notes(root)?)?.contains_key(&(commit, reference)),
            "workspace recovery failed: {}",
            failures.join("\n")
        );
    }
    Ok(())
}

fn input_hashes(root: &Path, commit: &str, gate: &str) -> Result<[String; 2]> {
    let listing = git(root, &["ls-tree", "-r", "-z", commit])?;
    let mut hashes = [Sha256::new(), Sha256::new()];
    for hash in &mut hashes {
        hash.update(INPUT_SCHEMA);
    }
    hashes[0].update(gate.as_bytes());
    hashes[0].update([0]);
    for entry in listing
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let (_, path) = entry.split_at(
            entry
                .iter()
                .position(|byte| *byte == b'\t')
                .context("git tree entry has no path")?
                + 1,
        );
        let path = std::str::from_utf8(path)?;
        let selected = crate::hosts_rules::stage_inputs(
            path.starts_with("home/"),
            SHARED_INPUTS.contains(&path),
        );
        for (hash, selected) in hashes.iter_mut().zip(selected) {
            if selected {
                hash.update(entry);
                hash.update([0]);
            }
        }
    }
    Ok(hashes.map(|hash| crate::build_inputs::hex(&hash.finalize())))
}

fn reusable(
    notes: &[(String, String)],
    family: Family,
    stage: Stage,
    inputs: &str,
    gate: &str,
) -> Result<Option<Reused>> {
    for (commit, text) in notes {
        for (line, text) in text
            .lines()
            .enumerate()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            if text.trim().is_empty() {
                continue;
            }
            let note: Note = serde_json::from_str(text).context("malformed hosts note")?;
            let matches = [
                note.os == family.name(),
                note.stage == Some(stage),
                note.inputs.as_deref() == Some(inputs),
                stage == Stage::Real || note.gate == gate,
            ];
            let [family_matches, stage_matches, inputs_match, gate_matches] = matches;
            if crate::hosts_rules::reuse([
                note.status == 0,
                family_matches,
                stage_matches,
                inputs_match,
                gate_matches,
            ]) {
                return Ok(Some(Reused {
                    commit: commit.clone(),
                    line: line + 1,
                    host: note.host,
                }));
            }
            if matches.iter().all(|fact| *fact) {
                break;
            }
        }
    }
    Ok(None)
}

fn all_notes(root: &Path) -> Result<Vec<(String, String)>> {
    git_text(root, &["notes", "--ref", NOTES, "list"])?
        .lines()
        .map(|line| {
            let (blob, commit) = line
                .split_once(' ')
                .context("malformed git notes listing")?;
            Ok((
                commit.to_owned(),
                String::from_utf8(git(root, &["cat-file", "blob", blob])?)?,
            ))
        })
        .collect()
}

/// Judge one commit's note, one JSON record per line in append order.
///
/// # Errors
///
/// Returns an error when a line holds no valid note or names an unknown OS family.
pub fn evidence(notes: &str, tree: &str) -> Result<[Evidence; 3]> {
    let mut evidence = [[Evidence::Missing; 2]; 3];
    for line in notes.lines().filter(|line| !line.trim().is_empty()) {
        let note: Note = serde_json::from_str(line).context("malformed hosts note")?;
        let family = Family::ALL
            .into_iter()
            .find(|family| family.name() == note.os)
            .with_context(|| format!("hosts note names an unknown OS family: {}", note.os))?;
        let index = family.index();
        if let Some(stage) = note.stage {
            let previous = &mut evidence[index][stage.index()];
            *previous = record(*previous, note.tree == tree, note.status == 0);
        }
    }
    Ok(evidence.map(crate::hosts_rules::stages))
}

fn git(repository: &Path, arguments: &[&str]) -> Result<Vec<u8>> {
    let output = Tool::Git
        .command()
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .stderr(Stdio::piped())
        .output()
        .context("start git")?;
    ensure!(
        output.status.success(),
        "git {}: {}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(output.stdout)
}

fn git_text(repository: &Path, arguments: &[&str]) -> Result<String> {
    Ok(String::from_utf8(git(repository, arguments)?)?
        .trim()
        .to_owned())
}

fn workflows_at(repository: &Path, commit: &str) -> Result<Vec<(String, String)>> {
    let listing = git(
        repository,
        &[
            "ls-tree",
            "-z",
            "--name-only",
            commit,
            "--",
            ".github/workflows/",
        ],
    )?;
    let mut workflows = Vec::new();
    for path in listing
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let path = std::str::from_utf8(path)?;
        if crate::has_extension(path, "yml") || crate::has_extension(path, "yaml") {
            let text = String::from_utf8(git(repository, &["show", &format!("{commit}:{path}")])?)?;
            workflows.push((path.to_owned(), text));
        }
    }
    Ok(workflows)
}

fn note(repository: &Path, commit: &str) -> Result<String> {
    let listing = git_text(repository, &["notes", "--ref", NOTES, "list"])?;
    for line in listing.lines() {
        if let Some((blob, annotated)) = line.split_once(' ')
            && annotated == commit
        {
            return Ok(String::from_utf8(git(
                repository,
                &["cat-file", "blob", blob],
            )?)?);
        }
    }
    Ok(String::new())
}

/// The verdict on a profile commit's notes for every family its own workflows require.
pub(crate) fn admission(repository: &Path, commit: &str) -> Result<Verdict> {
    let commit = git_text(
        repository,
        &["rev-parse", "--verify", &format!("{commit}^{{commit}}")],
    )
    .with_context(|| format!("commit {commit} is not in this checkout; fetch it first"))?;
    let paths = git_text(
        repository,
        &[
            "ls-tree",
            "--name-only",
            "-r",
            &commit,
            "--",
            PROFILE_ROOT,
            PROFILE_RUNTIME,
        ],
    )?;
    if !crate::hosts_rules::profile_gate(
        paths.lines().any(|path| path == PROFILE_ROOT),
        paths.lines().any(|path| path == PROFILE_RUNTIME),
    ) {
        return Ok(admit([false; 3], [Evidence::Missing; 3]));
    }
    let tree = git_text(repository, &["rev-parse", &format!("{commit}^{{tree}}")])?;
    let required = required(&workflows_at(repository, &commit)?)?;
    Ok(admit(
        required,
        evidence(&note(repository, &commit)?, &tree)?,
    ))
}

/// Refuses a profile `commit` unless every required host family recorded a passing gate for it.
///
/// # Errors
///
/// Returns an error when a family recorded no result or a failed one, or Git fails while reading the notes.
pub fn require_admission(repository: &Path, commit: &str) -> Result<()> {
    match admission(repository, commit)? {
        Verdict::Admit => Ok(()),
        Verdict::Missing(family) => bail!(
            "no {} gate result for {commit}; run `dotfiles hosts check` from a clean checkout of it",
            family.name()
        ),
        Verdict::Failed(family) => bail!(
            "the {} gate failed for {commit}; fix it, commit, and run `dotfiles hosts check` again",
            family.name()
        ),
    }
}

fn github(url: &str) -> bool {
    url.contains("github.com:") || url.contains("github.com/")
}

/// Judge pre-push input: `<local ref> <local sha> <remote ref> <remote sha>` per line.
///
/// # Errors
///
/// Returns an error when the input holds invalid UTF-8 or has a malformed line.
/// Also returns an error when a gated commit lacks a passing note for a required family, or Git fails while judging it.
pub fn push_gate(repository: &Path, url: &str, input: &[u8]) -> Result<()> {
    for line in std::str::from_utf8(input)?.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        let [_, local, remote_ref, _] = fields[..] else {
            bail!("malformed pre-push input: {line}");
        };
        let deletion = local.bytes().all(|byte| byte == b'0');
        if gated(github(url), remote_ref.starts_with("refs/heads/"), deletion) {
            require_admission(repository, local)?;
        }
    }
    Ok(())
}

fn native(home: &Path) -> Result<Real> {
    let mut native = Real::new(home)?;
    if cfg!(windows) {
        // Only the system OpenSSH reaches the 1Password agent pipe.
        let root = std::env::var_os("SystemRoot")
            .unwrap_or_else(|| platform::home::DEFAULT_SYSTEM_ROOT.into());
        let mut paths = vec![Path::new(&root).join("System32/OpenSSH")];
        paths.extend(std::env::split_paths(&native.path));
        native.path = std::env::join_paths(paths)?;
    }
    Ok(native)
}

fn command(native: &Real, tool: Tool) -> Command {
    let mut command = native.command(tool);
    if cfg!(windows) {
        command.env_remove("SSH_AUTH_SOCK");
    }
    command
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peer {
    pub alias: String,
    pub dns: String,
    pub address: String,
    pub family: Family,
    pub online: bool,
}

#[derive(Debug)]
pub struct Tailnet {
    pub this: Option<(String, Family)>,
    pub peers: Vec<Peer>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Node {
    #[serde(rename = "DNSName")]
    dns_name: String,
    #[serde(rename = "OS")]
    os: String,
    #[serde(default, rename = "TailscaleIPs")]
    tailscale_ips: Vec<String>,
    #[serde(default)]
    online: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Status {
    #[serde(rename = "Self")]
    this: Node,
    #[serde(default)]
    peer: BTreeMap<String, Node>,
}

/// Peers of a known OS family from `tailscale status --json`, named by their `MagicDNS` label.
///
/// # Errors
///
/// Returns an error when the status fails to parse as `tailscale status --json` output.
pub fn tailnet(status: &[u8]) -> Result<Tailnet> {
    let status: Status = serde_json::from_slice(status).context("invalid tailscale status")?;
    let alias = |node: &Node| {
        node.dns_name
            .split('.')
            .next()
            .unwrap_or_default()
            .to_owned()
    };
    let mut peers: Vec<_> = status
        .peer
        .values()
        .filter_map(|node| {
            Some(Peer {
                alias: alias(node),
                dns: node.dns_name.trim_end_matches('.').to_owned(),
                address: node
                    .tailscale_ips
                    .iter()
                    .find(|ip| ip.contains('.'))?
                    .clone(),
                family: os_family(&node.os)?,
                online: node.online,
            })
        })
        .collect();
    peers.sort_by(|a, b| a.alias.cmp(&b.alias));
    Ok(Tailnet {
        this: os_family(&status.this.os).map(|family| (alias(&status.this), family)),
        peers,
    })
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Managed {
    pub aliases: Vec<String>,
    pub user: Option<String>,
}

pub fn managed(config: &str) -> Managed {
    let mut managed = Managed::default();
    for line in config.lines() {
        let mut words = line.split_whitespace();
        match words.next() {
            Some("Host") => managed.aliases.extend(words.map(str::to_owned)),
            Some("User") => managed.user = words.next().map(str::to_owned),
            _ => {}
        }
    }
    managed
}

#[must_use]
pub fn render(aliases: &[String], user: &str, known_hosts: &Path) -> String {
    format!(
        "# Generated by `dotfiles hosts sync` from the tailnet; host keys are pinned over the tailnet.\nHost {}\n  User {user}\n  UserKnownHostsFile {}\n  StrictHostKeyChecking yes\n  ForwardAgent no\n",
        aliases.join(" "),
        known_hosts.display().to_string().replace('\\', "/")
    )
}

/// One host per OS family other than this machine's: a requested one, then the managed one, then the only online one.
///
/// # Errors
///
/// Returns an error when a requested name matches no host of a known family in the tailnet.
pub fn select(tailnet: &Tailnet, previous: &[String], requested: &[String]) -> Result<Vec<Peer>> {
    for name in requested {
        ensure!(
            tailnet.peers.iter().any(|peer| &peer.alias == name),
            "{name} is not a Linux, Mac, or Windows host in the tailnet"
        );
    }
    let mut selected = Vec::new();
    for family in Family::ALL {
        if tailnet.this.as_ref().is_some_and(|(_, own)| *own == family) {
            continue;
        }
        let candidates: Vec<_> = tailnet
            .peers
            .iter()
            .filter(|peer| peer.family == family)
            .collect();
        let pick = |names: &[String]| -> Vec<&Peer> {
            candidates
                .iter()
                .copied()
                .filter(|peer| names.contains(&peer.alias))
                .collect()
        };
        let mut chosen = pick(requested);
        if chosen.is_empty() {
            chosen = pick(previous);
        }
        if chosen.is_empty() {
            chosen = candidates
                .iter()
                .copied()
                .filter(|peer| peer.online)
                .collect();
        }
        match chosen[..] {
            [] => {}
            [peer] => selected.push(peer.clone()),
            _ => bail!(
                "several {} hosts: {}; choose one with --host",
                family.name(),
                chosen
                    .iter()
                    .map(|peer| peer.alias.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
    Ok(selected)
}

/// Pinned keys by the first name of each `known_hosts` line.
#[must_use]
pub fn pinned(known_hosts: &str) -> BTreeMap<String, String> {
    known_hosts
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let names = fields.next()?;
            let kind = fields.next()?;
            let key = fields.next()?;
            kind.starts_with("ssh-").then(|| {
                (
                    names.split(',').next().unwrap_or_default().to_owned(),
                    format!("{kind} {key}"),
                )
            })
        })
        .collect()
}

/// The first key line of a `known_hosts` file written for one address, as `type key`.
#[must_use]
pub fn scanned(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let _address = fields.next()?;
        let kind = fields.next()?;
        let key = fields.next()?;
        kind.starts_with("ssh-").then(|| format!("{kind} {key}"))
    })
}

/// The user of a hub remote on one of the hosts, as in `ssh://user@host/...` or `user@host:path`.
#[must_use]
pub fn hub_user(remotes: &str, aliases: &[String]) -> Option<String> {
    remotes.split_whitespace().find_map(|url| {
        let rest = url.strip_prefix("ssh://").unwrap_or(url);
        let (user, host) = rest.split_once('@')?;
        let host = host.split([':', '/']).next()?;
        (!user.contains('/') && aliases.iter().any(|alias| alias == host)).then(|| user.to_owned())
    })
}

/// The gate as the family's login shell runs it, so a job's minimal environment still finds the profile's tools.
#[must_use]
pub fn wrap(family: Family, gate: &str) -> Vec<String> {
    let shell: &[&str] = match family {
        Family::Linux => &["sh", "-lc"],
        Family::Mac => &["zsh", "-lc"],
        Family::Windows => &["powershell.exe", "-NoProfile", "-Command"],
    };
    shell
        .iter()
        .map(|word| (*word).to_owned())
        .chain([gate.to_owned()])
        .collect()
}

/// The gate for a domyjob snapshot holding only `head.bundle`: a checkout of exactly the commit, whose project tools mise may load.
/// Trust covers only that checkout and starts before the clone, because checkout hooks already run mise there.
#[must_use]
pub fn remote_gate(family: Family, gate: &str) -> String {
    remote_gate_stages(family, gate, [false; 2])
}

fn remote_gate_stages(family: Family, gate: &str, skipped: [bool; 2]) -> String {
    const UNIX_GIT: &str = "/usr/bin/git";
    const WINDOWS_GIT: &str = "C:/Program Files/Git/cmd/git.exe";
    let quoted = match family {
        Family::Windows => format!("'{}'", gate.replace('\'', "''")),
        Family::Linux | Family::Mac => format!("'{}'", gate.replace('\'', "'\"'\"'")),
    };
    let mut worker = format!(
        "mise x -- cargo run --locked --package dotfiles --bin dotfiles -- hosts worker --gate {quoted} --workspace .."
    );
    for (skip, flag) in skipped.into_iter().zip([" --skip-code", " --skip-real"]) {
        if skip {
            worker.push_str(flag);
        }
    }
    let cache = platform::code_environment::HOST_CACHE;
    let worker = match family {
        Family::Windows => format!(
            "$env:DOTFILES_HOST_CACHE = Join-Path $nativeHome '{cache}'; $env:CARGO_TARGET_DIR = Join-Path $env:DOTFILES_HOST_CACHE worker; $env:CARGO_HOME = Join-Path $env:DOTFILES_HOST_CACHE cargo; $env:MISE_CARGO_HOME = $env:CARGO_HOME; New-Item -ItemType Directory -Force $env:CARGO_HOME | Out-Null; {worker}"
        ),
        Family::Linux | Family::Mac => format!(
            "export DOTFILES_HOST_CACHE=\"$nativeHome/{cache}\" CARGO_TARGET_DIR=\"$nativeHome/{cache}/worker\" CARGO_HOME=\"$nativeHome/{cache}/cargo\" MISE_CARGO_HOME=\"$nativeHome/{cache}/cargo\"; mkdir -p \"$CARGO_HOME\" && {worker}"
        ),
    };
    let bootstrap = match family {
        Family::Windows => format!(
            "$nativeHome = $env:USERPROFILE; $env:DOTFILES_HOST_PATH = $env:PATH; $env:DOTFILES_HOST_CARGO_HOME = if ($env:CARGO_HOME) {{ $env:CARGO_HOME }} else {{ Join-Path $nativeHome .cargo }}; $jobRoot = $PWD.Path; $env:MISE_DATA_DIR = Join-Path $env:LOCALAPPDATA mise; $env:RUSTUP_HOME = Join-Path $nativeHome .rustup; $env:HOME = Join-Path $jobRoot bootstrap; $env:USERPROFILE = $env:HOME; New-Item -ItemType Directory -Force $env:HOME | Out-Null; $env:APPDATA = Join-Path $env:HOME AppData/Roaming; $env:LOCALAPPDATA = Join-Path $env:HOME AppData/Local; $env:CARGO_HOME = Join-Path $env:HOME .cargo; $env:MISE_CARGO_HOME = $env:CARGO_HOME; New-Item -ItemType Directory -Force $env:CARGO_HOME | Out-Null; New-Item -ItemType Junction -Path (Join-Path $env:CARGO_HOME bin) -Target (Join-Path $env:DOTFILES_HOST_CARGO_HOME bin) -ErrorAction Stop | Out-Null; $env:CARGO_TARGET_DIR = Join-Path $jobRoot build; $env:MISE_CONFIG_DIR = Join-Path $env:HOME .config/mise; $env:MISE_CACHE_DIR = Join-Path $env:HOME .cache/mise; $env:MISE_STATE_DIR = Join-Path $env:HOME .local/state/mise; $env:MISE_TRUSTED_CONFIG_PATHS = Join-Path $jobRoot checkout; $env:MISE_AUTO_INSTALL = 'false'; $env:GIT_CONFIG_GLOBAL = Join-Path $env:HOME empty.toml; Set-Content -LiteralPath $env:GIT_CONFIG_GLOBAL -Value '' -NoNewline; $env:MISE_GLOBAL_CONFIG_FILE = $env:GIT_CONFIG_GLOBAL; $env:MISE_SYSTEM_CONFIG_FILE = $env:GIT_CONFIG_GLOBAL; $env:GIT_CONFIG_NOSYSTEM = '1'; & '{WINDOWS_GIT}' -c advice.detachedHead=false clone --quiet head.bundle checkout; if (-not $?) {{ exit 1 }}; Set-Location checkout; {worker} --home $nativeHome; exit $LASTEXITCODE"
        ),
        Family::Linux | Family::Mac => {
            format!(
                "nativeHome=\"$HOME\"; export DOTFILES_HOST_PATH=\"$PATH\" DOTFILES_HOST_CARGO_HOME=\"${{CARGO_HOME:-$HOME/.cargo}}\" MISE_DATA_DIR=\"$HOME/.local/share/mise\" RUSTUP_HOME=\"$HOME/.rustup\"; export HOME=\"$PWD/bootstrap\" CARGO_TARGET_DIR=\"$PWD/build\" MISE_TRUSTED_CONFIG_PATHS=\"$PWD/checkout\" MISE_AUTO_INSTALL=false; mkdir -p \"$HOME/.cargo\" && ln -s \"$DOTFILES_HOST_CARGO_HOME/bin\" \"$HOME/.cargo/bin\" && export CARGO_HOME=\"$HOME/.cargo\" MISE_CARGO_HOME=\"$HOME/.cargo\" XDG_CONFIG_HOME=\"$HOME/.config\" XDG_CACHE_HOME=\"$HOME/.cache\" XDG_DATA_HOME=\"$HOME/.local/share\" XDG_STATE_HOME=\"$HOME/.local/state\" MISE_CONFIG_DIR=\"$HOME/.config/mise\" MISE_CACHE_DIR=\"$HOME/.cache/mise\" MISE_STATE_DIR=\"$HOME/.local/state/mise\" GIT_CONFIG_GLOBAL=\"$HOME/empty.toml\" GIT_CONFIG_NOSYSTEM=1 && : > \"$GIT_CONFIG_GLOBAL\" && export MISE_GLOBAL_CONFIG_FILE=\"$GIT_CONFIG_GLOBAL\" MISE_SYSTEM_CONFIG_FILE=\"$GIT_CONFIG_GLOBAL\" && {UNIX_GIT} -c advice.detachedHead=false clone --quiet head.bundle checkout && cd checkout && {worker} --home \"$nativeHome\""
            )
        }
    };
    let github = match family {
        Family::Windows => {
            "$env:GH_CONFIG_DIR = if ($env:GH_CONFIG_DIR) { $env:GH_CONFIG_DIR } elseif ($env:XDG_CONFIG_HOME) { Join-Path $env:XDG_CONFIG_HOME gh } elseif ($env:APPDATA) { Join-Path $env:APPDATA 'GitHub CLI' } else { Join-Path $env:USERPROFILE .config/gh }; "
        }
        Family::Linux | Family::Mac => {
            "export GH_CONFIG_DIR=\"${GH_CONFIG_DIR:-${XDG_CONFIG_HOME:-$HOME/.config}/gh}\"; "
        }
    };
    format!("{github}{bootstrap}")
}

fn run_stages<C, R>(
    skipped: [bool; 2],
    concurrent: bool,
    code: C,
    real: R,
) -> (Result<()>, Option<crate::host_checks::Report>)
where
    C: FnOnce() -> Result<()> + Send,
    R: FnOnce() -> crate::host_checks::Report,
{
    let code = || if skipped[0] { Ok(()) } else { code() };
    let real = || (!skipped[1]).then(real);
    if concurrent && skipped == [false; 2] {
        std::thread::scope(|scope| {
            let code = scope.spawn(code);
            let real = real();
            (
                code.join()
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("code stage panicked"))),
                real,
            )
        })
    } else {
        (code(), real())
    }
}

fn concurrent_capacity(home: &Path) -> Result<bool> {
    let cores = std::thread::available_parallelism()?.get();
    let family = Family::ALL
        .into_iter()
        .find(|family| family.name() == std::env::consts::OS)
        .context("native host family")?;
    let available = capacity(&native(home)?, family, &Target::Local("worker".into()))?;
    Ok(crate::hosts_rules::concurrent_stages(cores, available))
}

fn worker(home: &Path, gate: &str, skipped: [bool; 2], workspace: Option<&Path>) -> Result<()> {
    let workspace = workspace
        .map(|path| -> Result<_> {
            let path = platform::paths::canonical(path)?;
            ensure!(
                path.join("checkout") == platform::paths::canonical(&std::env::current_dir()?)?,
                "workspace must own this checkout"
            );
            Ok(path.to_string_lossy().replace('\\', "/"))
        })
        .transpose()?;
    let concurrent = skipped == [false; 2]
        && concurrent_capacity(home).unwrap_or_else(|error| {
            eprintln!("stages run sequentially: measure capacity: {error:#}");
            false
        });
    let root = std::env::current_dir()?;
    let (code, real) = run_stages(
        skipped,
        concurrent,
        || code(home, gate),
        || crate::host_checks::check(&root, home),
    );
    let code = match code {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("code: {error:#}");
            1
        }
    };
    let (candidate, applied) = match real {
        Some(crate::host_checks::Report { candidate, applied }) => (candidate, Some(applied)),
        None => (Ok(()), None),
    };
    let real = match candidate {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("real: {error:#}");
            1
        }
    };
    println!(
        "{STAGE_REPORT}{}",
        serde_json::to_string(&Stages {
            code,
            real,
            applied,
            workspace
        })?
    );
    ensure!(
        code == 0 && real == 0,
        "host stages failed: code={code}, real={real}"
    );
    Ok(())
}

fn code(home: &Path, gate: &str) -> Result<()> {
    let environment = code_environment(&std::env::current_dir()?, home)?;
    let argv = code_shell(gate);
    let status = environment
        .command(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .status()?;
    ensure!(status.success(), "code stage failed with {status}");
    Ok(())
}

pub(crate) fn code_environment(
    root: &Path,
    home: &Path,
) -> Result<platform::code_environment::CodeEnvironment> {
    let mut installed = platform::code_environment::Installations::discover(home)?;
    let releases = model::data::parse(RELEASE_PINS, &fs::read_to_string(root.join(RELEASE_PINS))?)
        .map_err(anyhow::Error::msg)?;
    let pin = releases
        .mise
        .and_then(|release| release.version)
        .context("mise has no release pin")?;
    let version = platform::process::command(&installed.mise)
        .arg("--version")
        .output()?;
    ensure!(version.status.success(), "query the installed mise release");
    if std::str::from_utf8(&version.stdout)?
        .split_whitespace()
        .next()
        == Some(pin.as_str())
    {
        return platform::code_environment::CodeEnvironment::build(root, &installed);
    }
    let scope = tempfile::tempdir()?;
    let config = scope.path().join("chezmoi.toml");
    fs::write(
        &config,
        format!(
            "[data]\nprofile = {:?}\n",
            crate::profiles::native_profile()?.name()
        ),
    )?;
    let bootstrap = crate::host_checks::Scratch::new(&config)?;
    installed.mise = bootstrap
        .renderer(root, &installed)
        .and_then(|renderer| bootstrap.provision_mise(root, &renderer))
        .with_context(|| format!("resolve mise release pin {pin}"))?;
    platform::code_environment::CodeEnvironment::build(root, &installed)
}

fn code_shell(gate: &str) -> Vec<String> {
    let shell: &[&str] = if cfg!(windows) {
        &["pwsh", "-NoLogo", "-NoProfile", "-Command"]
    } else {
        &[CODE_SHELL, "-c"]
    };
    shell
        .iter()
        .map(|word| (*word).to_owned())
        .chain([gate.to_owned()])
        .collect()
}

/// The exit status domyjob reports for a finished job.
/// Returns `None` when the job didn't finish or failed to launch.
#[must_use]
pub fn outcome(alias: &str, output: &str) -> Option<i32> {
    let prefix = format!("{alias}:");
    let line = output
        .lines()
        .find(|line| line.starts_with(&prefix) && line.contains(" finished "))?;
    let result = line.split_once(" finished ")?.1.trim();
    if result == "succeeded" {
        return Some(0);
    }
    result
        .strip_prefix("failed (")?
        .strip_suffix(')')?
        .parse()
        .ok()
}

fn tailnet_status(native: &Real) -> Result<Tailnet> {
    let output = command(native, Tool::Tailscale)
        .args(["status", "--json"])
        .stderr(Stdio::inherit())
        .output()
        .context("start tailscale; install it and sign in to the tailnet")?;
    ensure!(
        output.status.success(),
        "tailscale is not connected; run `tailscale up`"
    );
    tailnet(&output.stdout)
}

fn paths(home: &Path) -> (PathBuf, PathBuf) {
    let state = home.join(STATE);
    (state.join("ssh_config"), state.join("known_hosts"))
}

fn read_or_empty(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error).with_context(|| format!("read {}", path.display())),
    }
}

fn fingerprint(native: &Real, key: &str) -> Result<String> {
    let file = tempfile::NamedTempFile::new()?;
    fs::write(file.path(), format!("{key}\n"))?;
    let output = command(native, Tool::SshKeygen)
        .arg("-lf")
        .arg(file.path())
        .output()?;
    ensure!(
        output.status.success(),
        "ssh-keygen cannot fingerprint the scanned key"
    );
    Ok(String::from_utf8(output.stdout)?
        .split_whitespace()
        .nth(1)
        .context("ssh-keygen printed no fingerprint")?
        .to_owned())
}

/// The ed25519 key the host presents to the SSH client that later connects, which `ssh-keyscan` can't always negotiate.
/// The scan offers no identity, so at most a host that admits anyone runs `true`.
fn scan(native: &Real, user: &str, address: &str) -> Result<Option<String>> {
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("known_hosts");
    let known = file.display().to_string().replace('\\', "/");
    command(native, Tool::Ssh)
        .args([
            "-F",
            "none",
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=10",
        ])
        .args([
            "-o",
            "StrictHostKeyChecking=accept-new",
            "-o",
            "HostKeyAlgorithms=ssh-ed25519",
        ])
        .args([
            "-o",
            "PreferredAuthentications=publickey",
            "-o",
            "IdentitiesOnly=yes",
        ])
        .args(["-o", "IdentityAgent=none", "-o", "IdentityFile=none"])
        .arg("-o")
        .arg(format!("UserKnownHostsFile={known}"))
        .arg("-o")
        .arg(format!("GlobalKnownHostsFile={known}"))
        .args(["-l", user, address, "true"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("start ssh")?;
    Ok(scanned(&read_or_empty(&file)?))
}

pub(crate) fn sync(
    home: &Path,
    requested: &[String],
    user: Option<String>,
    accept: &[String],
) -> Result<()> {
    let native = native(home)?;
    let (config, known) = paths(home);
    let previous = managed(&read_or_empty(&config)?);
    let tailnet = tailnet_status(&native)?;
    let selected = select(&tailnet, &previous.aliases, requested)?;
    ensure!(
        !selected.is_empty(),
        "the tailnet has no other Linux, Mac, or Windows host"
    );
    let aliases: Vec<_> = selected.iter().map(|peer| peer.alias.clone()).collect();
    let remotes = git(Path::new("."), &["remote", "-v"]).unwrap_or_default();
    let user = user
        .or(previous.user)
        .or_else(|| hub_user(&String::from_utf8_lossy(&remotes), &aliases))
        .context("no hub remote names the remote user; pass --user")?;
    let pinned = pinned(&read_or_empty(&known)?);
    let mut lines = String::new();
    for peer in &selected {
        let key = scan(&native, &user, &peer.address)?.with_context(|| {
            format!(
                "{} presented no ed25519 host key over the tailnet; check its SSH server",
                peer.alias
            )
        })?;
        let state = match pinned.get(&peer.alias) {
            None => Key::New,
            Some(current) if *current == key => Key::Same,
            Some(_) => Key::Changed,
        };
        ensure!(
            pin(state, accept.contains(&peer.alias)),
            "{} presents a different host key {}; if expected, run `dotfiles hosts sync --accept {}`",
            peer.alias,
            fingerprint(&native, &key)?,
            peer.alias
        );
        writeln!(lines, "{},{},{} {key}", peer.alias, peer.dns, peer.address)?;
    }
    crate::runtime::replace(&known, lines.as_bytes(), false)?;
    crate::runtime::replace(&config, render(&aliases, &user, &known).as_bytes(), false)?;
    for peer in &selected {
        println!("{} {}: pinned", peer.family.name(), peer.alias);
    }
    Ok(())
}

/// Where each family's gate runs.
#[derive(Clone, Debug)]
enum Target {
    Local(String),
    Remote(String),
}

impl Target {
    fn name(&self) -> &str {
        match self {
            Self::Local(name) | Self::Remote(name) => name,
        }
    }
}

fn targets(home: &Path, tailnet: &Tailnet) -> Result<[Option<Target>; 3]> {
    let managed = managed(&read_or_empty(&paths(home).0)?);
    Ok(Family::ALL.map(|family| match &tailnet.this {
        Some((name, own)) if *own == family => Some(Target::Local(name.clone())),
        _ => managed
            .aliases
            .iter()
            .find(|alias| {
                tailnet
                    .peers
                    .iter()
                    .any(|peer| &peer.alias == *alias && peer.family == family)
            })
            .map(|alias| Target::Remote(alias.clone())),
    }))
}

fn domyjob(
    native: &Real,
    alias: &str,
    arguments: &[&str],
    program: &[String],
) -> Result<(Option<i32>, String)> {
    let mut submission = command(native, Tool::Domyjob);
    submission
        .args(arguments)
        .arg(alias)
        .arg("--")
        .args(program);
    remote_job(native, alias, submission)
}

/// Collects output from a file when the child exits.
fn job_output(
    mut request: Command,
    limit: Option<Duration>,
) -> Result<(std::process::ExitStatus, String)> {
    let log = tempfile::NamedTempFile::new()?;
    let mut child = request
        .stdin(Stdio::null())
        .stdout(log.as_file().try_clone()?)
        .stderr(log.as_file().try_clone()?)
        .spawn()
        .context("start host check process")?;
    let Some(limit) = limit else {
        let status = child.wait()?;
        return Ok((
            status,
            String::from_utf8_lossy(&fs::read(log.path())?).into_owned(),
        ));
    };
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok((
                status,
                String::from_utf8_lossy(&fs::read(log.path())?).into_owned(),
            ));
        }
        if started.elapsed() >= limit {
            platform::cleanup::report(child.kill(), "stop a stalled domyjob request");
            platform::cleanup::report(child.wait(), "reap a stalled domyjob request");
            bail!(
                "domyjob request did not exit within {limit:?}; inspect the remote job before rerunning"
            );
        }
        std::thread::sleep(REQUEST_POLL_INTERVAL);
    }
}

fn remote_job(native: &Real, alias: &str, submission: Command) -> Result<(Option<i32>, String)> {
    let (status, mut text) = job_output(submission, Some(REQUEST_LIMIT))?;
    ensure!(
        status.success(),
        "{alias}: submit remote job: {text}; inspect `domyjob ls {alias}` before rerunning"
    );
    let prefix = format!("{alias}:");
    let reference = text.lines()
        .find_map(|line| line.strip_prefix(&prefix).and_then(|rest| rest.split_whitespace().next()))
        .filter(|job| !job.is_empty())
        .map(|job| format!("{prefix}{job}"))
        .with_context(|| format!("{alias}: submission returned no job identity: {text}; inspect `domyjob ls {alias}` before rerunning"))?;
    println!("{reference}: submitted");
    loop {
        let mut request = command(native, Tool::Domyjob);
        request.args(["status", &reference]);
        let (status, current) = job_output(request, Some(REQUEST_LIMIT))
            .with_context(|| format!("inspect {reference} with `domyjob status {reference}`"))?;
        ensure!(status.success(), "{reference}: read job status: {current}");
        let state_prefix = format!("{reference} ");
        let state = current
            .lines()
            .find_map(|line| line.strip_prefix(&state_prefix))
            .with_context(|| format!("{reference}: status returned no matching job: {current}"))?;
        if state.starts_with("finished ") {
            let result = outcome(alias, &current);
            text.push_str(&current);
            let mut request = command(native, Tool::Domyjob);
            request.args(["logs", &reference]);
            let (status, logs) = job_output(request, Some(REQUEST_LIMIT))
                .with_context(|| format!("read {reference} with `domyjob logs {reference}`"))?;
            ensure!(status.success(), "{reference}: read job logs: {logs}");
            text.push_str(&logs);
            return Ok((result, text));
        }
        ensure!(
            matches!(state, "accepted" | "queued" | "starting" | "running"),
            "{reference}: unknown job state: {state}"
        );
        std::thread::sleep(JOB_POLL_INTERVAL);
    }
}

/// `Ok` when the host can run the gate, or the next action that would make it able to.
fn probe(
    native: &Real,
    tailnet: &Tailnet,
    family: Family,
    target: &Target,
    program: &str,
) -> Result<Result<(), String>> {
    let Target::Remote(alias) = target else {
        return Ok(if on_path(native, program) {
            require_capacity(native, family, target).map_err(|error| format!("{error:#}"))
        } else {
            Err(format!(
                "`{program}` is not on this machine's PATH; apply its dotfiles profile"
            ))
        });
    };
    let Some(peer) = tailnet.peers.iter().find(|peer| &peer.alias == alias) else {
        return Ok(Err(format!(
            "{alias} left the tailnet; run `dotfiles hosts sync`"
        )));
    };
    if !peer.online {
        return Ok(Err(format!(
            "{alias} is offline on the tailnet; wake it or start Tailscale on it"
        )));
    }
    let doctor = command(native, Tool::Domyjob)
        .args(["doctor", alias])
        .stdin(Stdio::null())
        .output()
        .context("start domyjob")?;
    if !doctor.status.success() {
        return Ok(Err(format!(
            "{alias} refused SSH or domyjob; run `ssh -o BatchMode=yes {alias} true`, approve it in 1Password, then `domyjob doctor {alias}`"
        )));
    }
    let (status, _) = domyjob(
        native,
        alias,
        &["on"],
        &wrap(family, &format!("{program} --version")),
    )?;
    Ok(if status == Some(0) {
        require_capacity(native, family, target).map_err(|error| format!("{error:#}"))
    } else {
        Err(format!(
            "{alias} has no `{program}` on its login PATH; apply its dotfiles profile"
        ))
    })
}

fn on_path(native: &Real, program: &str) -> bool {
    std::env::split_paths(&native.path).any(|directory| {
        directory.join(program).is_file()
            || (cfg!(windows)
                && ["exe", "cmd", "bat"]
                    .iter()
                    .any(|extension| directory.join(format!("{program}.{extension}")).is_file()))
    })
}

fn program(gate: &str) -> Result<&str> {
    gate.split_whitespace()
        .next()
        .context("the gate command is empty")
}

pub(crate) fn doctor(home: &Path, gate: &str) -> Result<()> {
    let native = native(home)?;
    let tailnet = tailnet_status(&native)?;
    let program = program(gate)?;
    let mut failures = 0;
    for (family, target) in Family::ALL.into_iter().zip(targets(home, &tailnet)?) {
        let Some(target) = target else {
            println!(
                "{}: no managed host; run `dotfiles hosts sync`",
                family.name()
            );
            failures += 1;
            continue;
        };
        match probe(&native, &tailnet, family, &target, program)? {
            Ok(()) => println!("{} {}: ready", family.name(), target.name()),
            Err(action) => {
                println!("{} {}: {action}", family.name(), target.name());
                failures += 1;
            }
        }
    }
    ensure!(failures == 0, "{failures} OS family host(s) are not ready");
    Ok(())
}

fn ready_hosts(
    native: &Real,
    home: &Path,
    gate: &str,
    clean: bool,
    pending: [bool; 3],
) -> Result<[Option<Target>; 3]> {
    let mut ready = [true; 3];
    let mut actions: [Option<String>; 3] = Default::default();
    let mut hosts: [Option<Target>; 3] = Default::default();
    if clean && pending.iter().any(|pending| *pending) {
        let tailnet = tailnet_status(native)?;
        hosts = targets(home, &tailnet)?;
        let program = program(gate)?;
        for family in Family::ALL
            .into_iter()
            .filter(|family| pending[family.index()])
        {
            let index = family.index();
            let verdict = match &hosts[index] {
                None => Err("no managed host; run `dotfiles hosts sync`".to_owned()),
                Some(target) => probe(native, &tailnet, family, target, program)?,
            };
            if let Err(action) = verdict {
                ready[index] = false;
                actions[index] = Some(action);
            }
        }
    }
    match start(clean, pending, ready) {
        Start::Dirty => bail!(
            "the tree has uncommitted or untracked files; notes bind to a commit, so commit first"
        ),
        Start::Unready(_) => bail!(
            "{}",
            Family::ALL
                .into_iter()
                .filter_map(|family| actions[family.index()]
                    .as_ref()
                    .map(|action| format!("{}: {action}", family.name())))
                .collect::<Vec<_>>()
                .join("\n")
        ),
        Start::Run => Ok(hosts),
    }
}

fn reuse_plan(
    root: &Path,
    required: [bool; 3],
    inputs: &[String; 2],
    gate: &str,
) -> Result<[[Option<Reused>; 2]; 3]> {
    let notes = all_notes(root)?;
    let mut reused: [[Option<Reused>; 2]; 3] = Default::default();
    for family in Family::ALL
        .into_iter()
        .filter(|family| required[family.index()])
    {
        for stage in Stage::ALL {
            reused[family.index()][stage.index()] =
                reusable(&notes, family, stage, &inputs[stage.index()], gate)?;
        }
    }
    Ok(reused)
}

pub(crate) fn check(home: &Path, gate: &str) -> Result<()> {
    let root = PathBuf::from(git_text(Path::new("."), &["rev-parse", "--show-toplevel"])?);
    let clean = git(&root, &["status", "--porcelain", "--untracked-files=all"])?.is_empty();
    let commit = git_text(&root, &["rev-parse", "HEAD"])?;
    let tree = git_text(&root, &["rev-parse", "HEAD^{tree}"])?;
    let required = required(&workflows_at(&root, &commit)?)?;
    if required == [false; 3] {
        println!("The CI matrix requires no OS family; nothing to check");
        return Ok(());
    }
    let native = native(home)?;
    if clean {
        recover_workspaces(&native, &root)?;
    }
    let inputs = input_hashes(&root, &commit, gate)?;
    let reused = reuse_plan(&root, required, &inputs, gate)?;
    let pending = Family::ALL.map(|family| {
        required[family.index()] && reused[family.index()].iter().any(Option::is_none)
    });
    let hosts = ready_hosts(&native, home, gate, clean, pending)?;
    let snapshot = tempfile::tempdir()?;
    if pending.iter().any(|pending| *pending) {
        let bundle = snapshot.path().join("head.bundle");
        git(
            &root,
            &["bundle", "create", &bundle.to_string_lossy(), "HEAD"],
        )?;
    }
    let runs: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = Family::ALL
            .into_iter()
            .filter(|family| required[family.index()])
            .map(|family| {
                let (native, root, snapshot) = (&native, &root, snapshot.path());
                let reused = reused[family.index()].clone();
                let target = hosts[family.index()].clone();
                let thread = scope.spawn(move || {
                    if reused.iter().all(Option::is_some) {
                        let host = reused[0]
                            .as_ref()
                            .context("reused code record")?
                            .host
                            .clone();
                        return Ok(GateRun {
                            family,
                            target: Target::Local(host),
                            status: Some(0),
                            output: format!(
                                "{STAGE_REPORT}{}",
                                serde_json::to_string(&Stages {
                                    code: 0,
                                    real: 0,
                                    applied: None,
                                    workspace: None
                                })?
                            ),
                            reused,
                        });
                    }
                    let target = target.context("managed host is unavailable")?;
                    let skipped = reused.each_ref().map(Option::is_some);
                    run_gate(native, root, snapshot, family, &target, gate, skipped).map(
                        |(status, output)| GateRun {
                            family,
                            target,
                            status,
                            output,
                            reused,
                        },
                    )
                });
                (family, thread)
            })
            .collect();
        handles
            .into_iter()
            .map(|(family, handle)| {
                handle.join().unwrap_or_else(|_| {
                    Err(anyhow::anyhow!(
                        "the {} gate thread panicked",
                        family.name()
                    ))
                })
            })
            .collect()
    });
    let mut failed = Vec::new();
    for run in runs {
        match run.and_then(|run| record_run(&native, &root, &commit, &tree, gate, &inputs, &run)) {
            Ok(failures) => failed.extend(failures),
            Err(error) => failed.push(format!("host gate: {error:#}")),
        }
    }
    ensure!(failed.is_empty(), "{}", failed.join("\n"));
    Ok(())
}

/// One family's gate as it ran on its host: the exit status when the job finished, and its combined output.
struct GateRun {
    family: Family,
    target: Target,
    status: Option<i32>,
    output: String,
    reused: [Option<Reused>; 2],
}

/// Appends the note of one finished gate to the commit, returning the failure to report when the gate didn't pass.
fn reported(run: &GateRun) -> Result<(Stages, Option<Workspace>)> {
    let GateRun {
        family,
        target,
        status,
        output,
        reused,
    } = run;
    let Some(status) = *status else {
        eprintln!("{output}");
        bail!(
            "{} {}: the job did not finish; inspect it with `domyjob ls {}` before rerunning",
            family.name(),
            target.name(),
            target.name()
        );
    };
    let reports: Vec<_> = output
        .lines()
        .filter_map(|line| line.strip_prefix(STAGE_REPORT))
        .collect();
    let [report] = reports.as_slice() else {
        eprintln!("{output}");
        bail!(
            "{} {}: the job returned {status} without exactly one complete stage report",
            family.name(),
            target.name()
        );
    };
    let report: Stages = serde_json::from_str(report).context("malformed host stage report")?;
    let workspace = if let (Target::Remote(alias), Some(path)) = (target, &report.workspace) {
        let prefix = format!("{alias}:");
        let job = output
            .lines()
            .find_map(|line| {
                line.strip_prefix(&prefix)
                    .and_then(|text| text.split_whitespace().next())
            })
            .context("workspace job identity is missing")?;
        let workspace = Workspace {
            path: path.clone(),
            job: job.to_owned(),
            reused: reused.clone(),
        };
        cleanup_arguments(*family, &workspace)?;
        Some(workspace)
    } else {
        None
    };
    let statuses = [report.code, report.real];
    ensure!(
        (status == 0) == statuses.iter().all(|status| *status == 0),
        "the job status disagrees with its stage report"
    );
    Ok((report, workspace))
}

fn record_run(
    native: &Real,
    root: &Path,
    commit: &str,
    tree: &str,
    gate: &str,
    inputs: &[String; 2],
    run: &GateRun,
) -> Result<Vec<String>> {
    let (report, workspace) = reported(run)?;
    let GateRun {
        family,
        target,
        output,
        reused,
        ..
    } = run;
    let statuses = [report.code, report.real];
    let mut failures = Vec::new();
    let mut recorded = true;
    let mut receipt = None;
    if reused[1].is_none()
        && let Some(applied) = &report.applied
    {
        applied.report(&format!("{} {} applied", family.name(), target.name()));
    }
    for (stage, status) in Stage::ALL.into_iter().zip(statuses) {
        let reuse = &reused[stage.index()];
        ensure!(
            reuse.is_none() || status == 0,
            "a reused stage reported failure"
        );
        let note = Note {
            host: target.name().to_owned(),
            os: family.name().to_owned(),
            tree: tree.to_owned(),
            status,
            gate: gate.to_owned(),
            stage: Some(stage),
            applied: match stage {
                Stage::Code => None,
                Stage::Real => report.applied.clone(),
            },
            inputs: Some(inputs[stage.index()].clone()),
            reused: reuse.clone(),
            workspace: workspace.clone(),
        };
        if let Err(error) = append_note(root, commit, &note) {
            recorded = false;
            failures.push(format!(
                "{} {} {}: record note: {error:#}",
                family.name(),
                target.name(),
                stage.name()
            ));
        }
        receipt = Some(note);
        if let Some(reuse) = reuse {
            println!(
                "{} {} {}: reused {}:{}",
                family.name(),
                target.name(),
                stage.name(),
                reuse.commit,
                reuse.line
            );
        } else if status == 0 {
            println!(
                "{} {} {}: passed",
                family.name(),
                target.name(),
                stage.name()
            );
        } else {
            failures.push(format!(
                "{} {} {}: failed with {status}",
                family.name(),
                target.name(),
                stage.name()
            ));
        }
    }
    if recorded
        && workspace.is_some()
        && let Some(receipt) = receipt
    {
        let cleanup = clean_recorded_workspace(native, root, commit, &receipt);
        if cleanup.is_err() {
            failures.push(format!(
                "{} {}: workspace cleanup is pending",
                family.name(),
                target.name()
            ));
        }
        platform::cleanup::report(cleanup, "remove the recorded host workspace");
    }
    if !failures.is_empty() {
        eprintln!("{output}");
    }
    Ok(failures)
}

/// One family's gate on its host, with its exit status when the job finished and its combined output.
fn run_gate(
    native: &Real,
    root: &Path,
    snapshot: &Path,
    family: Family,
    target: &Target,
    gate: &str,
    skipped: [bool; 2],
) -> Result<(Option<i32>, String)> {
    match target {
        Target::Remote(alias) => {
            let mut submission = command(native, Tool::Domyjob);
            submission
                .current_dir(snapshot)
                .args(["run", alias, "--"])
                .args(wrap(family, &remote_gate_stages(family, gate, skipped)));
            remote_job(native, alias, submission)
        }
        Target::Local(_) => {
            let mut request = crate::host_checks::unattended(std::env::current_exe()?);
            request
                .args(["hosts", "worker", "--gate", gate])
                .current_dir(root);
            for (skip, flag) in skipped.into_iter().zip(["--skip-code", "--skip-real"]) {
                if skip {
                    request.arg(flag);
                }
            }
            let (status, text) = job_output(request, None).context("run the local gate")?;
            Ok((status.code(), text))
        }
    }
}

#[cfg(test)]
mod job_tests {
    use super::*;

    const STAGE_START_LIMIT: Duration = Duration::from_secs(5);

    fn failed_real() -> crate::host_checks::Report {
        crate::host_checks::Report {
            candidate: Err(anyhow::anyhow!("real failure")),
            applied: crate::host_checks::Applied {
                drifted_targets: Vec::new(),
                verification_error: Some("observed drift".into()),
            },
        }
    }

    #[test]
    fn concurrent_stages_start_together_and_preserve_both_failures() -> Result<()> {
        let (code_started, await_code) = std::sync::mpsc::channel();
        let (real_started, await_real) = std::sync::mpsc::channel();
        let (code, real) = run_stages(
            [false; 2],
            true,
            move || {
                code_started.send(())?;
                await_real.recv_timeout(STAGE_START_LIMIT)?;
                bail!("code failure")
            },
            move || {
                real_started.send(()).expect("code stage is present");
                await_code
                    .recv_timeout(STAGE_START_LIMIT)
                    .expect("code stage starts before real finishes");
                failed_real()
            },
        );
        assert_eq!(code.unwrap_err().to_string(), "code failure");
        let real = real.context("real stage result")?;
        assert_eq!(real.candidate.unwrap_err().to_string(), "real failure");
        assert_eq!(
            real.applied.verification_error.as_deref(),
            Some("observed drift")
        );
        Ok(())
    }

    #[test]
    fn sequential_stages_finish_code_before_real_and_skip_reused_stages() {
        for skipped in [[false; 2], [true, false], [false, true], [true; 2]] {
            let (events, received) = std::sync::mpsc::channel();
            let code_events = events.clone();
            let (code, real) = run_stages(
                skipped,
                false,
                move || {
                    code_events.send("code").expect("event receiver");
                    bail!("code failure")
                },
                move || {
                    events.send("real").expect("event receiver");
                    failed_real()
                },
            );
            assert_eq!(code.is_ok(), skipped[0]);
            assert_eq!(real.is_none(), skipped[1]);
            let expected: Vec<_> = skipped
                .into_iter()
                .zip(["code", "real"])
                .filter_map(|(skip, name)| (!skip).then_some(name))
                .collect();
            assert_eq!(received.into_iter().collect::<Vec<_>>(), expected);
        }
    }

    #[test]
    fn workspace_receipts_keep_passed_failed_and_interrupted_jobs_pending() -> Result<()> {
        let record = |stage: Option<&str>, status| {
            serde_json::json!({
            "host":"box","os":"linux","tree":"tree","gate":"just check","status":status,
            "stage":stage,"workspace":{"path":"/state/jobs/gate/workspace","job":"gate","reused":[null,null]}
        }).to_string()
        };
        let stages = format!("{}\n{}", record(Some("code"), 0), record(Some("real"), 9));
        let notes = [("commit".into(), stages.clone())];
        assert_eq!(pending_workspaces(&notes)?.len(), 1);
        let interrupted = [("commit".into(), record(Some("code"), 0))];
        assert_eq!(pending_workspaces(&interrupted)?.len(), 1);
        let cleaned = [("commit".into(), format!("{stages}\n{}", record(None, 0)))];
        assert!(pending_workspaces(&cleaned)?.is_empty());
        let failed_cleanup = [("commit".into(), format!("{stages}\n{}", record(None, 1)))];
        assert_eq!(pending_workspaces(&failed_cleanup)?.len(), 1);
        Ok(())
    }

    #[test]
    fn cleanup_addresses_one_workspace_and_preserves_its_job() -> Result<()> {
        let workspace = Workspace {
            path: "/state/jobs/gate/workspace".into(),
            job: "gate".into(),
            reused: [None, None],
        };
        let arguments = cleanup_arguments(Family::Linux, &workspace)?;
        assert_eq!(
            arguments,
            ["/bin/rm", "-rf", "--", "/state/jobs/gate/workspace"]
        );
        assert!(
            cleanup_arguments(
                Family::Linux,
                &Workspace {
                    path: "/state/jobs/gate".into(),
                    ..workspace
                }
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn capacity_uses_the_least_free_volume_and_rejects_missing_or_overflowing_data() -> Result<()> {
        assert_eq!(
            space_available(
                Family::Linux,
                "Filesystem 1024-blocks Used Available Capacity Mounted\n/dev/home 100 20 80 20% /home\n/dev/tmp 100 90 10 90% /tmp"
            )?,
            10 * BYTES_PER_KIB
        );
        assert_eq!(
            space_available(Family::Windows, "host:job finished succeeded\n80000\n10000")?,
            10000
        );
        assert!(space_available(Family::Mac, "no disk data").is_err());
        assert!(
            space_available(Family::Linux, &format!("/dev/home 0 0 {} 0% /", u64::MAX)).is_err()
        );
        Ok(())
    }

    #[test]
    fn native_free_space_is_measured_for_home_and_scratch_storage() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let family = Family::ALL
            .into_iter()
            .find(|family| family.name() == std::env::consts::OS)
            .context("native family")?;
        capacity(
            &native(scope.path())?,
            family,
            &Target::Local("fixture".into()),
        )?;
        Ok(())
    }

    fn tree(root: &Path) -> Result<String> {
        git(root, &["add", "."])?;
        git_text(root, &["write-tree"])
    }

    #[test]
    fn stage_hashes_follow_their_inputs_and_the_code_gate() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let root = scope.path();
        git(root, &["init", "--quiet"])?;
        fs::create_dir_all(root.join("home"))?;
        fs::create_dir_all(root.join("crates/example/src"))?;
        fs::write(root.join("home/dot_profile"), "first")?;
        fs::write(root.join("crates/example/src/lib.rs"), "first")?;
        fs::write(root.join("mise.toml"), "first")?;
        let first = input_hashes(root, &tree(root)?, DEFAULT_GATE)?;
        assert_eq!(first, input_hashes(root, &tree(root)?, DEFAULT_GATE)?);
        fs::write(root.join("home/dot_profile"), "second")?;
        let profile = input_hashes(root, &tree(root)?, DEFAULT_GATE)?;
        assert_eq!(first[0], profile[0]);
        assert_ne!(first[1], profile[1]);
        fs::write(root.join("crates/example/src/lib.rs"), "second")?;
        let code = input_hashes(root, &tree(root)?, DEFAULT_GATE)?;
        assert_ne!(profile[0], code[0]);
        assert_eq!(profile[1], code[1]);
        let gate = input_hashes(root, &tree(root)?, "just proofs")?;
        assert_ne!(code[0], gate[0]);
        assert_eq!(code[1], gate[1]);
        fs::write(root.join("mise.toml"), "second")?;
        let pins = input_hashes(root, &tree(root)?, DEFAULT_GATE)?;
        assert_ne!(code[0], pins[0]);
        assert_ne!(code[1], pins[1]);
        fs::remove_file(root.join("home/dot_profile"))?;
        assert_ne!(pins[1], input_hashes(root, &tree(root)?, DEFAULT_GATE)?[1]);
        Ok(())
    }

    #[test]
    fn reuse_requires_a_passing_matching_family_stage_hash_and_gate() -> Result<()> {
        let note = |os: &str, stage: &str, status: i32, hash: Option<&str>, gate: &str| {
            serde_json::json!({"host":"box","os":os,"stage":stage,"status":status,
                "tree":"old-tree","gate":gate,"inputs":hash})
            .to_string()
        };
        for record in [
            note("macos", "code", 0, Some("hash"), DEFAULT_GATE),
            note("linux", "real", 0, Some("hash"), DEFAULT_GATE),
            note("linux", "code", 1, Some("hash"), DEFAULT_GATE),
            note("linux", "code", 0, Some("other"), DEFAULT_GATE),
            note("linux", "code", 0, None, DEFAULT_GATE),
            note("linux", "code", 0, Some("hash"), "just proofs"),
        ] {
            assert!(
                reusable(
                    &[("old".into(), record)],
                    Family::Linux,
                    Stage::Code,
                    "hash",
                    DEFAULT_GATE
                )?
                .is_none()
            );
        }
        let notes = [(
            "old".into(),
            note("linux", "code", 0, Some("hash"), DEFAULT_GATE),
        )];
        let reused = reusable(&notes, Family::Linux, Stage::Code, "hash", DEFAULT_GATE)?.unwrap();
        assert_eq!(reused.commit, "old");
        assert_eq!(reused.line, 1);
        let failed = [(
            "old".into(),
            format!(
                "{}\n{}",
                notes[0].1,
                note("linux", "code", 1, Some("hash"), DEFAULT_GATE)
            ),
        )];
        assert!(reusable(&failed, Family::Linux, Stage::Code, "hash", DEFAULT_GATE)?.is_none());
        Ok(())
    }

    #[test]
    fn partial_reuse_skips_only_its_worker_stage() {
        for family in Family::ALL {
            let code = remote_gate_stages(family, DEFAULT_GATE, [true, false]);
            assert!(code.contains(" --skip-code"));
            assert!(!code.contains(" --skip-real"));
            let real = remote_gate_stages(family, DEFAULT_GATE, [false, true]);
            assert!(real.contains(" --skip-real"));
            assert!(!real.contains(" --skip-code"));
        }
    }

    #[test]
    fn a_stalled_request_is_stopped_and_reports_its_deadline() {
        let mut request =
            platform::process::command(if cfg!(windows) { "pwsh" } else { CODE_SHELL });
        if cfg!(windows) {
            request.args([
                "-NoLogo",
                "-NoProfile",
                "-Command",
                "Start-Sleep -Seconds 30",
            ]);
        } else {
            request.args(["-c", "sleep 30"]);
        }
        let error = job_output(request, Some(Duration::ZERO))
            .expect_err("the request must reach its deadline");
        assert!(
            error.to_string().contains("did not exit within"),
            "{error:#}"
        );
    }
}

use crate::hook_rules::{self, Configured, Gate};
use crate::refusal::Refusal;
use crate::runtime::{Real, Runner, args};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum Hook {
    PreCommit,
    CommitMsg,
    PrePush,
    PostCommit,
    PostCheckout,
    PostMerge,
    ReferenceTransaction,
}

impl Hook {
    /// The hook the plan of `hook_rules` names.
    #[must_use]
    pub const fn rule(self) -> hook_rules::Hook {
        match self {
            Self::PreCommit => hook_rules::Hook::PreCommit,
            Self::CommitMsg => hook_rules::Hook::CommitMsg,
            Self::PrePush => hook_rules::Hook::PrePush,
            Self::PostCommit => hook_rules::Hook::PostCommit,
            Self::PostCheckout => hook_rules::Hook::PostCheckout,
            Self::PostMerge => hook_rules::Hook::PostMerge,
            Self::ReferenceTransaction => hook_rules::Hook::ReferenceTransaction,
        }
    }

    fn name(self) -> &'static str {
        hook_rules::name(self.rule())
    }
}

fn piped(command: &mut Command, input: &[u8]) -> Result<()> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;
    let written = child
        .stdin
        .take()
        .context("child input is unavailable")?
        .write_all(input);
    let status = child.wait()?;
    ensure!(status.success(), "hook gate failed with {status}");
    match written {
        Err(error) if error.kind() != std::io::ErrorKind::BrokenPipe => Err(error.into()),
        _ => Ok(()),
    }
}

/// Copies `from` to `to` as it arrives, and returns every byte it read.
/// A closed `to` stops the copy, and the bytes still reach the caller.
fn relay(
    mut from: impl Read + Send + 'static,
    mut to: impl Write + Send + 'static,
) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut seen = Vec::new();
        let mut chunk = [0; 8192];
        let mut relaying = true;
        while let Ok(count) = from.read(&mut chunk) {
            if count == 0 {
                break;
            }
            relaying = relaying
                && to
                    .write_all(&chunk[..count])
                    .and_then(|()| to.flush())
                    .is_ok();
            seen.extend_from_slice(&chunk[..count]);
        }
        seen
    })
}

/// What Lefthook prints before the name of each job that failed, and before its own name in the banner of a run.
const LEFTHOOK_FAILED: &str = "🥊";

/// How the banner of a Lefthook run continues after [`LEFTHOOK_FAILED`].
const LEFTHOOK_BANNER: &str = "lefthook ";

/// The lines at the end of a failed gate's output that its refusal quotes.
const QUOTED_LINES: usize = 5;

/// `text` without the terminal color and cursor sequences that start with `ESC [`.
fn plain(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' && characters.clone().next() == Some('[') {
            characters.find(|next| ('@'..='~').contains(next) && *next != '[');
        } else {
            plain.push(character);
        }
    }
    plain
}

/// The refusal for a gate that failed without its own record last.
/// It names each Lefthook job that failed, and either the refusal that job printed or the last lines of the output.
fn failure(output: &str, rerun: &str, status: &str) -> Refusal {
    let output = plain(output);
    let jobs: Vec<&str> = output
        .lines()
        .filter_map(|line| line.trim().strip_prefix(LEFTHOOK_FAILED))
        .map(str::trim)
        .filter(|job| !job.starts_with(LEFTHOOK_BANNER))
        .map(|job| {
            job.rsplit_once(" (")
                .filter(|(_, timing)| timing.ends_with(')'))
                .map_or(job, |(name, _)| name)
        })
        .collect();
    let cause = if jobs.is_empty() {
        "a hook gate failed without a structured refusal; its output above names the cause"
            .to_owned()
    } else {
        let names: Vec<String> = jobs.iter().map(|job| format!("`{job}`")).collect();
        let noun = if jobs.len() == 1 { "job" } else { "jobs" };
        format!(
            "the Lefthook {noun} {} failed; the evidence quotes how it ended",
            names.join(", ")
        )
    };
    let nested = Refusal::find(&output);
    let next = nested
        .as_ref()
        .and_then(|nested| nested.next.clone())
        .unwrap_or_else(|| rerun.to_owned());
    let mut refusal = Refusal::new("hook.gate", cause, next).evidence(format!("{rerun}: {status}"));
    if let Some(nested) = nested {
        let reason = nested.cause.lines().next().unwrap_or_default();
        return refusal.evidence(format!("{} refused: {reason}", nested.rule));
    }
    let tail: Vec<&str> = output
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty() && !line.trim().starts_with(LEFTHOOK_FAILED))
        .collect();
    for line in &tail[tail.len().saturating_sub(QUOTED_LINES)..] {
        refusal = refusal.evidence(format!("output: {line}"));
    }
    refusal
}

/// Runs one hook gate with `input` on its standard input, and relays its output and errors as they arrive.
/// A gate that refuses with its own record keeps that record last.
/// Any other failure becomes a `hook.gate` refusal that names `rerun`, each failed Lefthook job, and how the output ended.
fn gate(command: &mut Command, input: &[u8], rerun: &str) -> Result<()> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    // Lefthook prints each job's output and its summary on standard output.
    let output = relay(
        child.stdout.take().context("gate output is unavailable")?,
        std::io::stdout(),
    );
    let errors = relay(
        child.stderr.take().context("gate errors are unavailable")?,
        std::io::stderr(),
    );
    let written = child
        .stdin
        .take()
        .context("child input is unavailable")?
        .write_all(input);
    let status = child.wait()?;
    let output = output.join().unwrap_or_default();
    let errors = errors.join().unwrap_or_default();
    if !status.success() {
        let errors = String::from_utf8_lossy(&errors);
        let last = errors.lines().rev().find(|line| !line.trim().is_empty());
        if last.is_some_and(|line| Refusal::find(line).is_some()) {
            return Err(crate::Relayed.into());
        }
        let mut seen = String::from_utf8_lossy(&output).into_owned();
        seen.push_str(&errors);
        return Err(failure(&seen, rerun, &status.to_string()).into());
    }
    match written {
        Err(error) if error.kind() != std::io::ErrorKind::BrokenPipe => Err(error.into()),
        _ => Ok(()),
    }
}

/// How this host installs lefthook outside a full setup run.
fn install_lefthook() -> &'static str {
    if cfg!(windows) {
        "scoop install lefthook"
    } else if cfg!(target_os = "macos") {
        "brew install lefthook"
    } else {
        "nix profile install nixpkgs#lefthook"
    }
}

fn lefthook_missing(configuration: &str, hook: Hook) -> Refusal {
    Refusal::new(
        "hook.lefthook",
        "lefthook runs the configured hook gates and is not installed",
        install_lefthook(),
    )
    .evidence(format!("{configuration} configures {}", hook.name()))
}

pub(crate) fn configured_hooks(bytes: &[u8]) -> Result<Vec<String>> {
    let text = std::str::from_utf8(bytes)?;
    let documents = yaml_rust2::YamlLoader::load_from_str(text)?;
    ensure!(
        documents.len() == 1,
        "merged hook configuration requires one document"
    );
    let mapping = documents[0]
        .as_hash()
        .context("merged hook configuration requires a mapping")?;
    mapping
        .keys()
        .map(|key| {
            key.as_str()
                .context("hook configuration keys require strings")
                .map(str::to_owned)
        })
        .collect()
}

/// The Lefthook configurations a repository can carry at its root.
const LEFTHOOK_CONFIGURATIONS: [&str; 8] = [
    "lefthook.yml",
    "lefthook.yaml",
    "lefthook.toml",
    "lefthook.json",
    ".lefthook.yml",
    ".lefthook.yaml",
    ".lefthook.toml",
    ".lefthook.json",
];

/// The project configurations mise loads from a repository root.
const MISE_CONFIGURATIONS: [&str; 9] = [
    "mise.toml",
    "mise.local.toml",
    ".mise.toml",
    ".mise.local.toml",
    "mise/config.toml",
    ".mise/config.toml",
    ".config/mise.toml",
    ".config/mise/config.toml",
    ".tool-versions",
];

/// The root of the repository whose own Lefthook configuration the hook runs, with its mise configuration trusted.
/// `post-commit` and `reference-transaction` run no repository configuration.
fn lefthook_repository(native: &mut Real, hook: Hook) -> Result<Option<PathBuf>> {
    if matches!(hook, Hook::PostCommit | Hook::ReferenceTransaction) {
        return Ok(None);
    }
    let reply = native.run(Tool::Git, &args(&["rev-parse", "--show-toplevel"]))?;
    if !reply.success {
        return Ok(None);
    }
    let root = PathBuf::from(String::from_utf8(reply.bytes)?.trim());
    Ok(LEFTHOOK_CONFIGURATIONS
        .iter()
        .any(|name| root.join(name).is_file())
        .then_some(root))
}

/// mise's paranoid mode loads a repository's configuration only after a trust that records its current contents.
/// Until then, a mise shim there starts no tool, not even Lefthook.
/// The hook runs that repository's own Lefthook commands anyway, so trusting the configuration beside them grants them nothing more.
fn trust_repository(native: &mut Real, root: &Path) -> Result<()> {
    if !native.available(Tool::Mise) {
        return Ok(());
    }
    for name in MISE_CONFIGURATIONS {
        let configuration = root.join(name);
        if configuration.is_file() {
            let mut arguments = args(&["trust", "--quiet"]);
            arguments.push(configuration.clone().into());
            ensure!(
                native.run(Tool::Mise, &arguments)?.success,
                "mise could not trust {}",
                configuration.display()
            );
        }
    }
    Ok(())
}

fn delegate(
    native: &mut Real,
    hook: Hook,
    arguments: &[OsString],
    input: &[u8],
    global: Option<&Path>,
) -> Result<()> {
    let configuration = global.map_or_else(
        || "the repository's lefthook configuration".to_owned(),
        |config| config.display().to_string(),
    );
    if !native.available(Tool::Lefthook) {
        return Err(lefthook_missing(&configuration, hook).into());
    }
    let mut dump = native.hook_command(Tool::Lefthook);
    dump.arg("dump");
    if let Some(config) = global {
        dump.env("LEFTHOOK_CONFIG", config);
    }
    let output = dump.output()?;
    if !output.status.success() {
        let refusal = Refusal::new(
            "hook.lefthook",
            "the lefthook configuration could not be loaded; correct it and commit again",
            "lefthook dump",
        )
        .evidence(format!("{configuration}: {}", output.status));
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        return Err(if stderr.is_empty() {
            refusal
        } else {
            refusal.evidence(stderr.to_owned())
        }
        .into());
    }
    let hooks = configured_hooks(&output.stdout)?;
    if !hooks.iter().any(|name| name == hook.name()) {
        return Ok(());
    }
    let mut command = native.hook_command(Tool::Lefthook);
    if let Some(config) = global {
        command.env("LEFTHOOK_CONFIG", config);
    }
    command
        .args(["run", "--no-auto-install", hook.name()])
        .args(arguments);
    gate(
        &mut command,
        input,
        &crate::refusal::command(&["lefthook", "run", hook.name()]),
    )
}

/// Runs the source-matched signing worker without depending on the policy companion.
#[must_use]
pub fn post_commit_worker(producer: &str) -> i32 {
    if producer != platform::BUILD_HASH {
        Refusal::new(
            "tool.stale",
            "the post-commit worker does not match the invoking runtime",
            "chezmoi apply",
        )
        .evidence("the worker producer differs from its compiled source hash")
        .emit();
        return 1;
    }
    dotguard::postcommit::run()
}

fn dotguard_command(native: &Real, hook: Hook, arguments: &[OsString]) -> Result<Command> {
    let mut command = if matches!(hook, Hook::PostCommit) {
        let mut worker = native.hook_command_installed(&std::env::current_exe()?);
        worker.args(["post-commit-worker", platform::BUILD_HASH]);
        worker
    } else {
        let mut gate = native.hook_command(Tool::Dotguard);
        gate.arg(hook.name()).args(arguments);
        gate
    };
    command.stdin(Stdio::piped());
    Ok(command)
}

/// Runs one global Git hook: the health gate, the push pause, and the repository's own hooks.
///
/// # Errors
///
/// Returns an error when it can't find the home directory, the host fails its health gate, a push pause stops pushing, or a gate the hook runs refuses.
pub fn run(hook: Hook, arguments: &[OsString]) -> Result<()> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .context("native home is unavailable")?;
    let native = Real::new(Path::new(&home))?;
    let mut run = Run {
        hook,
        arguments,
        repository: None,
        working_copy: None,
        input: Vec::new(),
        staged: None,
        native,
    };
    let configured = run.configured()?;
    for gate in hook_rules::plan(hook.rule(), configured).gates() {
        if let Err(error) = run.gate(*gate) {
            if !hook_rules::notification(*gate) {
                return Err(error);
            }
            eprintln!("Warning: {error:#}");
        }
    }
    Ok(())
}

/// One run of a hook: what it found before planning, and what its gates read and pass along.
struct Run<'a> {
    hook: Hook,
    arguments: &'a [OsString],
    /// The repository whose own Lefthook configuration runs.
    repository: Option<PathBuf>,
    /// The checkout of this repository a commit or merge ran in.
    working_copy: Option<PathBuf>,
    /// The standard input, read once and passed unchanged to every gate that takes it.
    input: Vec<u8>,
    /// The staged tree before the first gate that could change it, or `None` without a tree.
    staged: Option<String>,
    native: Real,
}

impl Run<'_> {
    const GLOBAL_LEFTHOOK: &str = ".config/lefthook/global.yml";
    const PUSH_PAUSED: &str = ".config/git/push-paused";
    const STORAGE_SCOUT: &str = ".cargo/bin/storage-scout";
    const STORAGE_POLICY: &str = ".config/storage-scout/auto.toml";

    /// What the hook finds without running a gate.
    fn configured(&mut self) -> Result<Configured> {
        let home = self.native.home.clone();
        self.repository = lefthook_repository(&mut self.native, self.hook)?;
        if matches!(self.hook, Hook::PostCommit | Hook::PostMerge) {
            self.working_copy = std::env::current_dir()
                .ok()
                .and_then(|directory| crate::freshness::working_copy(&directory));
        }
        Ok(Configured {
            global: home.join(Self::GLOBAL_LEFTHOOK).is_file(),
            repository: self.repository.is_some(),
            url: self.arguments.get(1).is_some(),
            committed: self
                .arguments
                .first()
                .is_some_and(|argument| argument == "committed"),
            storage: home.join(Self::STORAGE_SCOUT).is_file()
                && home.join(Self::STORAGE_POLICY).is_file(),
            working_copy: self.working_copy.is_some(),
        })
    }

    /// The input a gate receives: the hook's input when it takes one, and nothing otherwise.
    fn input(&self, gate: Gate) -> Vec<u8> {
        if hook_rules::takes_input(gate) {
            self.input.clone()
        } else {
            Vec::new()
        }
    }

    fn gate(&mut self, gate: Gate) -> Result<()> {
        let hook = self.hook;
        let arguments = self.arguments;
        let input = self.input(gate);
        match gate {
            Gate::PushHold => {
                let marker = self.native.home.join(Self::PUSH_PAUSED);
                if marker.exists() {
                    return Err(push_paused(&marker).into());
                }
            }
            Gate::Health => crate::health::gate(&self.native.home, hook.name())?,
            Gate::ReadInput => {
                std::io::stdin().read_to_end(&mut self.input)?;
            }
            Gate::StagedTree => self.staged = staged_tree(&self.native),
            Gate::Dotguard => {
                let mut command = dotguard_command(&self.native, hook, arguments)?;
                self::gate(
                    &mut command,
                    &input,
                    &crate::refusal::command(&["dotguard", hook.name()]),
                )?;
            }
            Gate::PushHosts => {
                let url = arguments
                    .get(1)
                    .context("the pre-push hook needs the remote URL")?
                    .to_string_lossy();
                crate::hosts::push_gate(Path::new("."), &url, &input)?;
                crate::ready_push::gate(&url, &input)?;
            }
            Gate::Verification => verification(hook, arguments, &input)?,
            Gate::CommitProse => {
                let message = arguments
                    .first()
                    .context("the commit-msg hook needs the message file")?;
                prose::commit_gate(
                    &std::env::current_dir()?,
                    Path::new(message),
                    prose::Bundle::installed,
                )?;
            }
            Gate::GlobalLefthook => {
                let global = self.native.home.join(Self::GLOBAL_LEFTHOOK);
                delegate(&mut self.native, hook, arguments, &input, Some(&global))?;
            }
            Gate::RepositoryLefthook => {
                if let Some(root) = &self.repository {
                    trust_repository(&mut self.native, root)?;
                }
                delegate(&mut self.native, hook, arguments, &input, None)?;
            }
            Gate::StagedTreeCheck => {
                preserve_staged_tree(self.staged.as_deref(), staged_tree(&self.native).as_deref())?;
            }
            Gate::Renovate => self::gate(
                self.native
                    .hook_command(Tool::Dotguard)
                    .arg("renovate-gate")
                    .args(arguments),
                &input,
                "dotguard renovate run",
            )?,
            Gate::Reinstall => {
                if let Some(root) = &self.working_copy {
                    crate::freshness::reinstall_lagging(&self.native.home, root)
                        .context("reinstalling tools built from changed sources failed")?;
                }
            }
            Gate::Storage => {
                let mut command = self
                    .native
                    .command_installed(&self.native.home.join(Self::STORAGE_SCOUT));
                command
                    .args(["auto", "--config"])
                    .arg(self.native.home.join(Self::STORAGE_POLICY))
                    .args(["--execute", "--detach", "--event", hook.name(), "--"])
                    .args(arguments)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                piped(&mut command, &input).context("storage maintenance notification failed")?;
            }
        }
        Ok(())
    }
}

/// The verification gate of `pre-commit` and `pre-push`, which refuses a change that adds code of a shape nothing verifies.
fn verification(hook: Hook, arguments: &[OsString], input: &[u8]) -> Result<()> {
    if !matches!(hook, Hook::PreCommit | Hook::PrePush) {
        return Ok(());
    }
    let Some(root) = crate::assure::repository() else {
        return Ok(());
    };
    let catalog = crate::assure::installed()?;
    if matches!(hook, Hook::PrePush) {
        let remote = arguments
            .first()
            .context("the pre-push hook needs the remote name")?;
        crate::assure::push(&root, &catalog, &remote.to_string_lossy(), input)
    } else {
        crate::assure::commit(&root, &catalog)
    }
}

/// The machine-local push pause, which only an explicit owner instruction lifts, so the refusal names no bypass.
#[must_use]
pub fn push_paused(marker: &Path) -> Refusal {
    Refusal::new(
        "push.paused",
        "pushes are paused on this machine; keep the commits local until the owner explicitly resumes pushing",
        "git status --short --branch",
    )
    .evidence(format!("marker present: {}", marker.display()))
}

/// Git commits the index that the pre-commit hook leaves, so a gate that changed the staged tree refuses the commit.
pub(crate) fn preserve_staged_tree(before: Option<&str>, after: Option<&str>) -> Result<()> {
    if before == after {
        return Ok(());
    }
    let tree = |tree: Option<&str>| tree.unwrap_or("none").to_owned();
    Err(Refusal::new(
        "commit.staged-tree",
        "a pre-commit gate changed the staged changes; the commit was refused and the working tree still holds your edits",
        "git status --short",
    )
    .evidence(format!("staged tree before the gates: {}", tree(before)))
    .evidence(format!("staged tree after the gates: {}", tree(after)))
    .into())
}

fn staged_tree(native: &Real) -> Option<String> {
    let output = native
        .hook_command(Tool::Git)
        .arg("write-tree")
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Runs Git through the dotguard wrapper and returns its exit status.
///
/// # Errors
///
/// Returns an error when the wrapper can't start.
pub fn git(arguments: &[OsString]) -> Result<std::process::ExitStatus> {
    Tool::Dotguard
        .hook_command()
        .arg("git")
        .args(arguments)
        .status()
        .context("start the dotguard Git wrapper")
}

/// Prints the signing and hook state of every repository under `root`, and with `fix` removes stale local hooks that match a known generated copy.
///
/// # Errors
///
/// Returns an error when it can't read a directory, Git can't report a repository's configuration, or it can't remove a stale hook.
pub fn audit(root: &Path, fix: bool) -> Result<()> {
    fn walk(path: &Path, repos: &mut Vec<std::path::PathBuf>) -> Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir()
                || matches!(
                    entry.file_name().to_str(),
                    Some("target" | "node_modules" | ".cache" | ".cargo" | ".rustup" | ".local")
                )
            {
                continue;
            }
            if entry.file_name() == ".git" {
                if entry.path().join("objects").is_dir() {
                    repos.push(path.to_path_buf());
                }
            } else {
                walk(&entry.path(), repos)?;
            }
        }
        Ok(())
    }
    let mut repos = Vec::new();
    walk(root, &mut repos)?;
    println!("repo\tunsigned\thooksPath\tstale_hooks\tlocal_gpgsign");
    for repo in repos {
        let capture = |arguments: &[&str]| -> Result<String> {
            let reply = Tool::Git
                .command()
                .arg("-C")
                .arg(&repo)
                .args(arguments)
                .output()?;
            ensure!(reply.status.success(), "Git signing inventory failed");
            Ok(String::from_utf8(reply.stdout)?.trim().to_owned())
        };
        let unsigned = capture(&["log", "--pretty=%G?", "--no-merges"])?
            .lines()
            .filter(|line| *line != "G")
            .count();
        let hooks =
            capture(&["config", "--get", "core.hooksPath"]).unwrap_or_else(|_| "(unset)".into());
        let local = capture(&["config", "--local", "--get", "commit.gpgsign"]).unwrap_or_default();
        let mut stale = 0;
        for (name, expected) in [
            (
                "post-commit",
                "a25de0cf425cfd79af1f2c635abb5adf97822053235cc9f84b1dfdc35178f5d4",
            ),
            (
                "pre-push",
                "19be9760267012a443a1e12224349c732d2435d3481aefb0fcd4777e34eb5d01",
            ),
        ] {
            let path = repo.join(".git/hooks").join(name);
            if path.is_file() {
                stale += 1;
                if fix {
                    use sha2::{Digest, Sha256};
                    let digest = crate::build_inputs::hex(&Sha256::digest(fs::read(&path)?));
                    if digest == expected {
                        fs::remove_file(path)?;
                    }
                }
            }
        }
        println!("{}\t{unsigned}\t{hooks}\t{stale}\t{local}", repo.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_gate_that_changes_the_staged_tree_refuses_the_commit() {
        assert!(preserve_staged_tree(Some("staged"), Some("staged")).is_ok());
        assert!(preserve_staged_tree(None, None).is_ok());
        for (before, after) in [
            (Some("staged"), Some("emptied")),
            (Some("staged"), None),
            (None, Some("staged")),
        ] {
            let error = preserve_staged_tree(before, after).unwrap_err();
            let refusal = error
                .downcast_ref::<Refusal>()
                .expect("a structured refusal");
            assert!(refusal.is_complete(), "{refusal:?}");
            assert_eq!(refusal.rule, "commit.staged-tree");
            assert_eq!(refusal.bypass, None);
        }
    }

    /// Built here because a host's fixed tool directories can hold lefthook, which would then run the dispatcher.
    #[test]
    fn a_missing_lefthook_names_its_installation_and_the_configuration() {
        let refusal = lefthook_missing("lefthook.yml", Hook::PrePush);
        assert!(refusal.is_complete(), "{refusal:?}");
        assert_eq!(refusal.rule, "hook.lefthook");
        assert_eq!(
            refusal.next.as_deref().unwrap_or_default(),
            install_lefthook()
        );
        assert_eq!(refusal.evidence, ["lefthook.yml configures pre-push"]);
        assert_eq!(refusal.bypass, None);
    }

    #[test]
    fn a_paused_push_names_the_marker_and_no_waiver() {
        let refusal = push_paused(Path::new("/home/me/.config/git/push-paused"));
        assert!(refusal.is_complete(), "{refusal:?}");
        assert_eq!(refusal.rule, "push.paused");
        assert_eq!(refusal.bypass, None);
    }

    /// Each dispatcher hook matches one rule hook and one rendered hook file, and the global configuration names only those.
    #[test]
    fn the_hooks_match_the_rendered_hook_files_and_the_global_configuration() {
        use clap::ValueEnum;
        use std::collections::BTreeSet;
        // The top-level keys Lefthook reads as settings rather than as hooks.
        const SETTINGS: [&str; 6] = ["output", "colors", "min_version", "no_tty", "rc", "remotes"];
        let accepted: BTreeSet<&str> = Hook::value_variants()
            .iter()
            .map(|hook| hook.name())
            .collect();
        let ruled: BTreeSet<&str> = hook_rules::HOOKS
            .iter()
            .map(|hook| hook_rules::name(*hook))
            .collect();
        assert_eq!(accepted, ruled);
        assert_eq!(accepted.len(), hook_rules::HOOKS.len());
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../home/dot_config");
        let rendered: BTreeSet<String> = fs::read_dir(source.join("git/hooks"))
            .unwrap()
            .map(|entry| {
                let name = entry.unwrap().file_name().to_string_lossy().into_owned();
                name.strip_prefix("executable_")
                    .and_then(|name| name.strip_suffix(".tmpl"))
                    .unwrap_or_else(|| panic!("{name} is not a rendered hook"))
                    .to_owned()
            })
            .collect();
        assert_eq!(
            rendered,
            accepted.iter().map(|name| (*name).to_owned()).collect()
        );
        let global =
            configured_hooks(&fs::read(source.join("lefthook/global.yml")).unwrap()).unwrap();
        let hooks: Vec<&String> = global
            .iter()
            .filter(|name| !SETTINGS.contains(&name.as_str()))
            .collect();
        assert_ne!(hooks, Vec::<&String>::new());
        assert!(
            hooks.iter().all(|name| accepted.contains(name.as_str())),
            "{global:?}"
        );
    }

    #[test]
    fn a_nested_hook_name_does_not_enable_an_unconfigured_global_hook() {
        assert_eq!(
            configured_hooks(b"pre-commit:\n  commands:\n    pre-push: {}\n").unwrap(),
            ["pre-commit"]
        );
        assert!(configured_hooks(b"- pre-push\n").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_gate_that_ignores_its_input_is_judged_by_its_exit_status() {
        // More than a pipe buffer, so the write can only end in a broken pipe once the child exits.
        let input = vec![b'x'; 1 << 20];
        piped(&mut crate::tool::external("true"), &input).unwrap();
        assert!(piped(&mut crate::tool::external("false"), &input).is_err());
    }
}

use crate::refusal::Refusal;
use crate::runtime::{Native, Runner, args};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
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
    fn name(self) -> &'static str {
        match self {
            Self::PreCommit => "pre-commit",
            Self::CommitMsg => "commit-msg",
            Self::PrePush => "pre-push",
            Self::PostCommit => "post-commit",
            Self::PostCheckout => "post-checkout",
            Self::PostMerge => "post-merge",
            Self::ReferenceTransaction => "reference-transaction",
        }
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

/// Runs one hook gate with `input` on its standard input and relays its standard error line by line.
/// A gate that refused with its own record keeps that record last; any other failure becomes a `hook.gate` refusal that names `rerun`.
fn gate(command: &mut Command, input: &[u8], rerun: &str) -> Result<()> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stderr = child.stderr.take().context("gate errors are unavailable")?;
    let relay = std::thread::spawn(move || {
        let mut seen = Vec::new();
        let mut chunk = [0; 8192];
        while let Ok(count) = stderr.read(&mut chunk) {
            if count == 0 {
                break;
            }
            let _ = std::io::stderr().write_all(&chunk[..count]);
            seen.extend_from_slice(&chunk[..count]);
        }
        seen
    });
    let written = child
        .stdin
        .take()
        .context("child input is unavailable")?
        .write_all(input);
    let status = child.wait()?;
    let output = relay.join().unwrap_or_default();
    if !status.success() {
        let output = String::from_utf8_lossy(&output);
        let last = output.lines().rev().find(|line| !line.trim().is_empty());
        if last.is_some_and(|line| Refusal::find(line).is_some()) {
            return Err(crate::Relayed.into());
        }
        return Err(Refusal::new(
            "hook.gate",
            "a hook gate failed without a structured refusal; its output above names the cause",
            rerun,
        )
        .evidence(format!("{rerun}: {status}"))
        .into());
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

pub fn configured_hooks(bytes: &[u8]) -> Result<Vec<String>> {
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

fn delegate(
    native: &mut Native,
    hook: Hook,
    arguments: &[OsString],
    input: &[u8],
    global: Option<&Path>,
) -> Result<()> {
    if global.is_none() {
        let reply = native.run(Tool::Git, &args(&["rev-parse", "--show-toplevel"]))?;
        if !reply.success {
            return Ok(());
        }
        let root = String::from_utf8(reply.bytes)?.trim().to_owned();
        let configured = [
            "lefthook.yml",
            "lefthook.yaml",
            "lefthook.toml",
            "lefthook.json",
            ".lefthook.yml",
            ".lefthook.yaml",
            ".lefthook.toml",
            ".lefthook.json",
        ]
        .iter()
        .any(|name| Path::new(&root).join(name).is_file());
        if !configured {
            return Ok(());
        }
    }
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

pub fn run(hook: Hook, arguments: &[OsString]) -> Result<()> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .context("native home is unavailable")?;
    let mut native = Native::new(Path::new(&home))?;
    if matches!(hook, Hook::PrePush) {
        let marker = native.home.join(".config/git/push-paused");
        if marker.exists() {
            return Err(push_paused(&marker).into());
        }
    }
    let mut input = Vec::new();
    if matches!(hook, Hook::PrePush | Hook::ReferenceTransaction) {
        std::io::stdin().read_to_end(&mut input)?;
    }
    let staged = matches!(hook, Hook::PreCommit).then(|| staged_tree(&native));
    // Every host runs the same dotguard gates, so a policy holds identically on the Mac, Linux, and Windows.
    if matches!(
        hook,
        Hook::PreCommit | Hook::CommitMsg | Hook::PrePush | Hook::PostCommit
    ) {
        let mut command = native.hook_command(Tool::Dotguard);
        command.arg(hook.name()).args(arguments);
        gate(
            &mut command,
            &input,
            &crate::refusal::command(&["dotguard", hook.name()]),
        )?;
    }
    if matches!(hook, Hook::PrePush)
        && let Some(url) = arguments.get(1)
    {
        crate::hosts::push_gate(Path::new("."), &url.to_string_lossy(), &input)?;
        crate::ready_push::gate(&url.to_string_lossy(), &input)?;
    }
    if matches!(hook, Hook::PreCommit) {
        let global = native.home.join(".config/lefthook/global.yml");
        if global.is_file() {
            delegate(&mut native, hook, arguments, &input, Some(&global))?;
        }
    }
    if !matches!(hook, Hook::PostCommit | Hook::ReferenceTransaction) {
        delegate(&mut native, hook, arguments, &input, None)?;
    }
    if let Some(before) = staged {
        preserve_staged_tree(before.as_deref(), staged_tree(&native).as_deref())?;
    }
    if matches!(hook, Hook::PrePush) {
        gate(
            native
                .hook_command(Tool::Dotguard)
                .arg("renovate-gate")
                .args(arguments),
            &input,
            "dotguard renovate run",
        )?;
    }
    if matches!(hook, Hook::PostMerge)
        && let Some(root) = std::env::current_dir()
            .ok()
            .and_then(|directory| crate::freshness::working_copy(&directory))
        && let Err(error) = crate::freshness::reinstall_lagging(&native.home, &root)
    {
        eprintln!("Warning: reinstalling tools built from changed sources failed: {error:#}");
    }
    if matches!(hook, Hook::PostCheckout | Hook::PostMerge)
        || (matches!(hook, Hook::ReferenceTransaction)
            && arguments
                .first()
                .is_some_and(|argument| argument == "committed"))
    {
        let executable = native.home.join(".cargo/bin/storage-scout");
        let policy = native.home.join(".config/storage-scout/auto.toml");
        if executable.is_file() && policy.is_file() {
            let mut command = native.command_installed(&executable);
            command
                .args(["auto", "--config"])
                .arg(policy)
                .args(["--execute", "--detach", "--event", hook.name(), "--"])
                .args(arguments)
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if let Err(error) = piped(&mut command, &input) {
                eprintln!("Warning: storage maintenance notification failed: {error:#}");
            }
        }
    }
    Ok(())
}

/// The machine-local push hold; only an explicit owner instruction lifts it, so the refusal names no waiver.
pub fn push_paused(marker: &Path) -> Refusal {
    Refusal::new(
        "push.paused",
        "pushes are paused on this machine; keep the commits local until the owner explicitly resumes pushing",
        "git status --short --branch",
    )
    .evidence(format!("marker present: {}", marker.display()))
}

/// Git records the index as it stands after the pre-commit hook, so a gate that changed what is staged refuses the commit.
pub fn preserve_staged_tree(before: Option<&str>, after: Option<&str>) -> Result<()> {
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

fn staged_tree(native: &Native) -> Option<String> {
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

pub fn git(arguments: &[OsString]) -> Result<std::process::ExitStatus> {
    Tool::Dotguard
        .hook_command()
        .arg("git")
        .args(arguments)
        .status()
        .context("start the dotguard Git wrapper")
}

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
                    let digest: String = Sha256::digest(fs::read(&path)?)
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect();
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
            assert_eq!(refusal.waiver, None);
        }
    }

    /// Built here because the fixed tool directories of a host can hold lefthook, so the dispatcher cannot be run without it.
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
        assert_eq!(refusal.waiver, None);
    }

    #[test]
    fn a_paused_push_names_the_marker_and_no_waiver() {
        let refusal = push_paused(Path::new("/home/me/.config/git/push-paused"));
        assert!(refusal.is_complete(), "{refusal:?}");
        assert_eq!(refusal.rule, "push.paused");
        assert_eq!(refusal.waiver, None);
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

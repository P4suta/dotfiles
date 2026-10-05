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
    ensure!(
        native.available(Tool::Lefthook),
        "lefthook is required for configured hook gates"
    );
    let mut dump = native.hook_command(Tool::Lefthook);
    dump.arg("dump");
    if let Some(config) = global {
        dump.env("LEFTHOOK_CONFIG", config);
    }
    let output = dump.output()?;
    ensure!(
        output.status.success(),
        "lefthook configuration could not be loaded"
    );
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
    piped(&mut command, input)
}

pub fn run(hook: Hook, arguments: &[OsString]) -> Result<()> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .context("native home is unavailable")?;
    let mut native = Native::new(Path::new(&home))?;
    if matches!(hook, Hook::PrePush) {
        ensure!(
            !native.home.join(".config/git/push-paused").exists(),
            "push is paused on this machine; explicit resumption is required"
        );
    }
    let mut input = Vec::new();
    if matches!(hook, Hook::PrePush | Hook::ReferenceTransaction) {
        std::io::stdin().read_to_end(&mut input)?;
    }
    let staged = matches!(hook, Hook::PreCommit).then(|| staged_tree(&native));
    // Every host runs the same dotguard gates.
    if matches!(
        hook,
        Hook::PreCommit | Hook::CommitMsg | Hook::PrePush | Hook::PostCommit
    ) {
        let mut command = native.hook_command(Tool::Dotguard);
        command.arg(hook.name()).args(arguments);
        piped(&mut command, &input)?;
    }
    if matches!(hook, Hook::CommitMsg) {
        let message = arguments
            .first()
            .context("the commit-msg hook needs the message file")?;
        crate::prose::commit_gate(
            &std::env::current_dir()?,
            Path::new(message),
            crate::prose::Bundle::installed,
        )?;
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
        piped(
            native
                .hook_command(Tool::Dotguard)
                .arg("renovate-gate")
                .args(arguments),
            &input,
        )?;
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

/// Git commits the index that the pre-commit hook leaves, so a gate that changed the staged tree refuses the commit.
pub fn preserve_staged_tree(before: Option<&str>, after: Option<&str>) -> Result<()> {
    ensure!(
        before == after,
        "a pre-commit gate changed the staged changes; the commit was refused and the working tree still holds your edits"
    );
    Ok(())
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
        assert!(preserve_staged_tree(Some("staged"), Some("emptied")).is_err());
        assert!(preserve_staged_tree(Some("staged"), None).is_err());
        assert!(preserve_staged_tree(None, Some("staged")).is_err());
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

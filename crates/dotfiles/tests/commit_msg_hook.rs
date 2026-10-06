//! The global commit-msg hook checks the message of every repository that takes the personal writing standard.

use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;

#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

const ATTRIBUTED: &str = "As requested, the parser keeps every edit.";
const COMPLIANT: &str = "The parser keeps every edit.";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    let mut stack = vec![from.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            let target = to.join(path.strip_prefix(from)?);
            if path.is_dir() {
                stack.push(path);
            } else {
                fs::create_dir_all(target.parent().expect("parent"))?;
                fs::copy(&path, &target)?;
            }
        }
    }
    Ok(())
}

fn git(directory: &Path, arguments: &[&str]) -> Result<()> {
    let status = fixture_git::command("git", &directory.join(".fixture-gitconfig"))
        .current_dir(directory)
        .args(["-c", "core.hooksPath=", "-c", "commit.gpgsign=false"])
        .args(arguments)
        .stdout(Stdio::null())
        .status()?;
    assert!(status.success(), "git {arguments:?}");
    Ok(())
}

/// A repository holding the pinned prose configuration and its exemptions.
fn repository() -> Result<tempfile::TempDir> {
    let directory = tempfile::tempdir()?;
    copy_tree(
        &root().join("home/dot_config/prose"),
        &directory.path().join("home/dot_config/prose"),
    )?;
    fs::create_dir_all(directory.path().join("policy"))?;
    fs::write(
        directory.path().join("policy/prose.toml"),
        "[[exempt]]\nscope = \"documents\"\npaths = [\"home/dot_config/prose/styles/\"]\nreason = \"Vendored Vale packages keep their upstream text.\"\n\n[[exempt]]\nscope = \"comments\"\npaths = [\"home/dot_config/prose/\"]\nreason = \"The pinned configuration is data.\"\n",
    )?;
    git(directory.path(), &["init", "--quiet"])?;
    git(directory.path(), &["add", "--all"])?;
    Ok(directory)
}

/// A home whose `dotguard` succeeds, in which each commit runs the global commit-msg hook in a fresh repository.
struct CommitMsgHook {
    home: tempfile::TempDir,
}

impl CommitMsgHook {
    fn new() -> Result<Self> {
        let tools = tempfile::tempdir()?;
        let succeed = tools.path().join("succeed.rs");
        fs::write(&succeed, "fn main() {}\n")?;
        let home = tempfile::tempdir()?;
        let bin = home.path().join(".local/bin");
        fs::create_dir_all(&bin)?;
        let status = platform::process::command("rustc")
            .arg(&succeed)
            .arg("-o")
            .arg(bin.join(format!("dotguard{}", std::env::consts::EXE_SUFFIX)))
            .status()?;
        assert!(status.success());
        Ok(Self { home })
    }

    /// Whether the hook accepts `text` in a repository with these remotes and `prose.standard`, and what it printed.
    fn commit(
        &self,
        remotes: &[(&str, &str)],
        declared: Option<&str>,
        text: &str,
    ) -> Result<(bool, String)> {
        let directory = repository()?;
        for (name, url) in remotes {
            git(directory.path(), &["remote", "add", name, url])?;
        }
        if let Some(declared) = declared {
            git(directory.path(), &["config", "prose.standard", declared])?;
        }
        let message = directory.path().join(".git/COMMIT_EDITMSG");
        fs::write(&message, text)?;
        let output = platform::process::command(env!("CARGO_BIN_EXE_dotfiles-xtask"))
            .current_dir(directory.path())
            .args(["hook", "commit-msg", "--"])
            .arg(&message)
            .env(
                if cfg!(windows) { "USERPROFILE" } else { "HOME" },
                self.home.path(),
            )
            .env("PROSE_CONFIG", root().join("home/dot_config/prose"))
            .env("PROSE_RUNTIME", self.home.path().join("runtime"))
            .stdin(Stdio::null())
            .output()?;
        Ok((
            output.status.success(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ))
    }

    fn origin(&self, origin: &str, upstream: Option<&str>, text: &str) -> Result<(bool, String)> {
        let mut remotes = vec![("origin", origin)];
        remotes.extend(upstream.map(|upstream| ("upstream", upstream)));
        self.commit(&remotes, None, text)
    }
}

const HUB: &str = "hub:repositories/project.git";

fn attributed() -> String {
    format!("fix(parser): keep edits\n\n{ATTRIBUTED}\n")
}

#[test]
fn an_unresolved_destination_passes_and_names_the_setting_that_decides_it() -> Result<()> {
    let hook = CommitMsgHook::new()?;
    let (accepted, stderr) = hook.origin(HUB, None, &attributed())?;
    assert!(accepted, "{stderr}");
    assert!(
        stderr.contains(HUB) && stderr.contains("git config prose.standard true"),
        "an unresolved destination is reported with the setting that decides it: {stderr}"
    );
    let personal_remote = ("github", "git@github.com:P4suta/project.git");
    let (accepted, stderr) = hook.commit(&[personal_remote], None, &attributed())?;
    assert!(
        !accepted && stderr.contains("Dotfiles.Attribution"),
        "a personal GitHub remote names the destination of a repository without origin: {stderr}"
    );
    let (accepted, stderr) = hook.commit(&[], None, &attributed())?;
    assert!(accepted, "{stderr}");
    assert!(
        stderr.contains("no origin remote") && stderr.contains("git config prose.standard true"),
        "a repository without remotes reports the setting that decides it: {stderr}"
    );
    Ok(())
}

#[test]
fn the_declared_setting_decides_whether_the_standard_applies() -> Result<()> {
    let hook = CommitMsgHook::new()?;
    let (accepted, stderr) = hook.commit(&[], Some("true"), &attributed())?;
    assert!(
        !accepted && stderr.contains("Dotfiles.Attribution"),
        "{stderr}"
    );
    let origin = [("origin", HUB)];
    let (accepted, stderr) = hook.commit(&origin, Some("true"), &attributed())?;
    assert!(
        !accepted && stderr.contains("Dotfiles.Attribution"),
        "{stderr}"
    );
    let (accepted, stderr) = hook.commit(&origin, Some("false"), &attributed())?;
    assert!(accepted && stderr.is_empty(), "{stderr}");
    let (accepted, stderr) = hook.commit(&origin, Some("maybe"), &attributed())?;
    assert!(
        !accepted && stderr.contains("prose.standard"),
        "an invalid setting is refused: {stderr}"
    );
    Ok(())
}

#[test]
fn the_global_commit_msg_hook_checks_personal_repositories() -> Result<()> {
    let hook = CommitMsgHook::new()?;
    let commit =
        |origin: &str, upstream: Option<&str>, text: &str| hook.origin(origin, upstream, text);
    let attributed = attributed();
    let (accepted, stderr) = commit("git@github.com:P4suta/project.git", None, &attributed)?;
    assert!(!accepted);
    assert!(
        stderr.contains("Dotfiles.Attribution")
            && stderr.contains(ATTRIBUTED)
            && stderr.contains("commit again"),
        "{stderr}"
    );
    let compliant = format!("fix(parser): keep edits\n\n{COMPLIANT}\n");
    let (accepted, stderr) = commit("git@github.com:P4suta/project.git", None, &compliant)?;
    assert!(accepted, "{stderr}");
    for generated in [
        "Revert \"feat(prose): enforce one writing standard\"\n\nThis reverts commit 39f8a76.\n",
        "fixup! feat(prose): enforce one writing standard\n",
        "squash! feat(prose): enforce one writing standard\n",
    ] {
        let (accepted, stderr) = commit("git@github.com:P4suta/project.git", None, generated)?;
        assert!(accepted, "Git generated {generated:?}: {stderr}");
    }
    let (accepted, stderr) = commit("https://github.com/other/project.git", None, &attributed)?;
    assert!(
        accepted,
        "an external destination keeps its own rules: {stderr}"
    );
    let (accepted, stderr) = commit(
        "git@github.com:P4suta/project.git",
        Some("https://github.com/other/project.git"),
        &attributed,
    )?;
    assert!(accepted, "a fork keeps its upstream rules: {stderr}");
    Ok(())
}

#![allow(
    clippy::disallowed_methods,
    reason = "integration tests spawn the binaries and real tools they verify"
)]

use anyhow::{Result, ensure};
use dotfiles_xtask::line_endings;
use serde_json::json;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const CRLF: &[u8] = b"first\r\nsecond\r\n";
const LF: &[u8] = b"first\nsecond\n";

fn source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .to_path_buf()
}

/// A Git environment that reads only the given global configuration.
struct Isolated {
    _scope: tempfile::TempDir,
    global: PathBuf,
}

impl Isolated {
    fn new(attributes: bool) -> Result<Self> {
        let scope = tempfile::tempdir()?;
        let global = scope.path().join("gitconfig");
        let mut text = String::from("[init]\n\tdefaultBranch = main\n");
        if attributes {
            let file = source().join("dot_config/git/attributes");
            text.push_str(&format!(
                "[core]\n\tattributesFile = {}\n",
                file.to_str().expect("UTF-8 source path").replace('\\', "/")
            ));
        }
        fs::write(&global, text)?;
        Ok(Self {
            _scope: scope,
            global,
        })
    }

    fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = Command::new(program);
        command
            .env("GIT_CONFIG_GLOBAL", &self.global)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE");
        command
    }

    fn git(&self, directory: &Path, arguments: &[&str]) -> Result<Output> {
        let output = self
            .command("git")
            .arg("-C")
            .arg(directory)
            .args(arguments)
            .output()?;
        ensure!(
            output.status.success(),
            "git {arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(output)
    }

    fn repository(&self) -> Result<tempfile::TempDir> {
        let directory = tempfile::tempdir()?;
        self.git(directory.path(), &["init", "--quiet"])?;
        Ok(directory)
    }

    fn hook(&self, input: &serde_json::Value) -> Result<Output> {
        let mut child = self
            .command(env!("CARGO_BIN_EXE_dotfiles-xtask"))
            .args(["line-endings", "hook"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        child
            .stdin
            .take()
            .expect("hook input")
            .write_all(input.to_string().as_bytes())?;
        let output = child.wait_with_output()?;
        ensure!(
            output.status.success(),
            "line-ending hook failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(output)
    }
}

#[test]
fn global_attributes_stage_lf_in_a_repository_without_its_own() -> Result<()> {
    for (attributes, expected) in [(true, LF), (false, CRLF)] {
        let git = Isolated::new(attributes)?;
        let repository = git.repository()?;
        fs::write(repository.path().join("notes.txt"), CRLF)?;
        git.git(repository.path(), &["add", "notes.txt"])?;
        let staged = git.git(repository.path(), &["cat-file", "blob", ":notes.txt"])?;
        assert_eq!(staged.stdout, expected, "global attributes: {attributes}");
    }
    Ok(())
}

#[test]
fn hook_converts_written_text_and_keeps_declared_line_endings() -> Result<()> {
    let git = Isolated::new(true)?;
    let repository = git.repository()?;
    let root = repository.path();
    fs::write(
        root.join(".gitattributes"),
        "*.bat eol=crlf\nraw.txt -text\n*.dat binary\n",
    )?;
    fs::write(
        root.join(".gitignore"),
        "ignored.txt\nunnamed-ignored.log\n",
    )?;
    fs::write(root.join("changed.txt"), LF)?;
    git.git(root, &["add", "changed.txt"])?;
    let mut nul = CRLF.to_vec();
    nul.push(0);
    for (name, bytes) in [
        ("written.txt", CRLF),
        ("changed.txt", CRLF),
        ("ignored.txt", CRLF),
        ("unnamed-ignored.log", CRLF),
        ("keep.bat", CRLF),
        ("raw.txt", CRLF),
        ("blob.dat", CRLF),
        ("lone.txt", b"a\rb\r\n"),
        ("nul.txt", &nul),
    ] {
        fs::write(root.join(name), bytes)?;
    }
    let outside = tempfile::tempdir()?;
    fs::write(outside.path().join("loose.txt"), CRLF)?;

    let output = git.hook(&json!({
        "hook_event_name": "PostToolUse",
        "cwd": root,
        "tool_name": "Write",
        "tool_input": {"file_path": "ignored.txt"},
    }))?;
    assert!(String::from_utf8(output.stdout)?.contains("written.txt"));
    for name in ["written.txt", "changed.txt", "ignored.txt"] {
        assert_eq!(fs::read(root.join(name))?, LF, "{name}");
    }
    for name in ["unnamed-ignored.log", "keep.bat", "raw.txt", "blob.dat"] {
        assert_eq!(fs::read(root.join(name))?, CRLF, "{name}");
    }
    assert_eq!(fs::read(root.join("lone.txt"))?, b"a\rb\r\n");
    assert_eq!(fs::read(root.join("nul.txt"))?, nul);

    git.hook(&json!({
        "cwd": outside.path(),
        "tool_input": {"file_path": "loose.txt"},
    }))?;
    assert_eq!(fs::read(outside.path().join("loose.txt"))?, CRLF);
    Ok(())
}

#[test]
fn repository_check_refuses_only_undeclared_tracked_crlf() -> Result<()> {
    let git = Isolated::new(false)?;
    let repository = git.repository()?;
    let root = repository.path();
    fs::write(
        root.join(".gitattributes"),
        "* text=auto eol=lf\n*.bat eol=crlf\n",
    )?;
    fs::write(root.join("keep.bat"), CRLF)?;
    fs::write(root.join("clean.txt"), LF)?;
    git.git(root, &["add", "."])?;
    fs::write(root.join("clean.txt"), CRLF)?;
    fs::write(root.join("untracked.txt"), CRLF)?;
    assert_eq!(line_endings::offending(root)?, [PathBuf::from("clean.txt")]);
    let error = line_endings::check(root).unwrap_err().to_string();
    assert!(error.contains("clean.txt") && error.contains("line-endings normalize"));
    line_endings::normalize(root, &[PathBuf::from("clean.txt")])?;
    assert!(line_endings::offending(root)?.is_empty());
    line_endings::check(root)?;
    assert_eq!(fs::read(root.join("keep.bat"))?, CRLF);
    Ok(())
}

#[test]
fn hook_registration_is_stable_beside_the_skill_observer() -> Result<()> {
    let original = json!({"model":"owner-choice","hooks":{"PostToolUse":[{"matcher":"Write","hooks":[{"type":"command","command":"existing-check"}]}]}});
    let mut value = line_endings::merge_hooks(original.clone())?;
    assert_eq!(value["model"], "owner-choice");
    assert_eq!(
        value["hooks"]["PostToolUse"][0],
        original["hooks"]["PostToolUse"][0]
    );
    assert_eq!(
        value["hooks"]["PostToolUse"][1]["hooks"][0]["command"],
        line_endings::HOOK_COMMAND
    );
    for client in ["claude", "codex"] {
        value = dotfiles_xtask::skill_install::merge_hooks(value, client)?;
        assert_eq!(line_endings::merge_hooks(value.clone())?, value);
        assert_eq!(
            dotfiles_xtask::skill_install::merge_hooks(value.clone(), client)?,
            value
        );
    }
    let mut stale = json!({"hooks":{"PostToolUse":[{"matcher":"Edit","hooks":[{"type":"command","command":line_endings::HOOK_COMMAND}]}]}});
    stale = line_endings::merge_hooks(stale)?;
    assert_eq!(
        stale["hooks"]["PostToolUse"].as_array().map(Vec::len),
        Some(1)
    );
    assert!(stale["hooks"]["PostToolUse"][0].get("matcher").is_none());
    assert!(line_endings::merge_hooks(json!({"hooks":false})).is_err());
    Ok(())
}

#[test]
fn opencode_adapter_normalizes_a_written_file() -> Result<()> {
    let git = Isolated::new(true)?;
    let repository = git.repository()?;
    let root = repository.path();
    fs::write(root.join("written.txt"), CRLF)?;
    let runtime = Command::new("bun")
        .args(["-e", "console.log(process.execPath)"])
        .output()?;
    ensure!(runtime.status.success(), "bun is unavailable");
    let runtime = String::from_utf8(runtime.stdout)?;
    let output = git
        .command(runtime.trim())
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/line-endings-adapter.ts"))
        .arg(root)
        .arg(env!("CARGO_BIN_EXE_dotfiles-xtask"))
        .output()?;
    ensure!(
        output.status.success(),
        "OpenCode adapter failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(root.join("written.txt"))?, LF);
    Ok(())
}

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

    /// Stages `bytes` at `path` without conversion, as a repository that committed them before normalizing would hold them.
    fn index_raw(&self, directory: &Path, path: &str, bytes: &[u8]) -> Result<()> {
        let scope = tempfile::tempdir()?;
        let file = scope.path().join("blob");
        fs::write(&file, bytes)?;
        let file = file.to_str().expect("UTF-8 temporary path");
        let object = self.git(directory, &["hash-object", "-w", "--no-filters", file])?;
        let object = String::from_utf8(object.stdout)?;
        let entry = format!("100644,{},{path}", object.trim());
        self.git(directory, &["update-index", "--add", "--cacheinfo", &entry])?;
        Ok(())
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
    git.index_raw(root, "legacy.txt", CRLF)?;
    fs::write(root.join("legacy.txt"), CRLF)?;
    assert_eq!(
        line_endings::offending(root)?,
        [PathBuf::from("clean.txt"), PathBuf::from("legacy.txt")]
    );
    let error = line_endings::check(root).unwrap_err().to_string();
    assert!(error.contains("clean.txt") && error.contains("line-endings normalize"));
    line_endings::normalize(
        root,
        &[PathBuf::from("clean.txt"), PathBuf::from("legacy.txt")],
    )?;
    assert_eq!(fs::read(root.join("legacy.txt"))?, LF);
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

/// Content and attribute cases where the hook must leave exactly the bytes that `git add` stages.
fn git_detection_cases() -> Vec<(&'static str, Vec<u8>)> {
    let mut sparse = vec![b'a'; 128];
    sparse.extend_from_slice(b"\x01\r\n");
    let mut dense = vec![b'a'; 127];
    dense.extend_from_slice(b"\x01\r\n");
    vec![
        (
            "control-heavy.txt",
            b"\x01\x02\x03\x04\x05\r\nab\r\n".to_vec(),
        ),
        ("sparse-control.txt", sparse),
        ("dense-control.txt", dense),
        ("trailing-eof.txt", b"ab\r\n\x1a".to_vec()),
        ("leading-eof.txt", b"\x1aab\r\n".to_vec()),
        ("printable-controls.txt", b"\x1b\t\x08\x0cx\r\n".to_vec()),
        ("delete.txt", b"a\x7f\r\n".to_vec()),
        ("lone-before-pair.txt", b"a\r\r\nb\r\n".to_vec()),
        ("forced-lone.txt", b"a\rb\r\n".to_vec()),
        ("forced-nul.txt", b"a\0b\r\n".to_vec()),
        ("forced-control.txt", b"\x01\x02\x03\r\n".to_vec()),
        ("eol-only-lone.txt", b"a\rb\r\n".to_vec()),
        ("eol-only-nul.txt", b"a\0b\r\n".to_vec()),
    ]
}

#[test]
fn hook_leaves_the_bytes_git_stages() -> Result<()> {
    let git = Isolated::new(true)?;
    let repository = git.repository()?;
    let root = repository.path();
    fs::write(
        root.join(".gitattributes"),
        "forced-* text\neol-only-* !text eol=lf\n",
    )?;
    let originals = tempfile::tempdir()?;
    let cases = git_detection_cases();
    let mut staged = Vec::new();
    for (name, bytes) in &cases {
        let original = originals.path().join(name);
        fs::write(&original, bytes)?;
        fs::write(root.join(name), bytes)?;
        let path = format!("--path={name}");
        let original = original.to_str().expect("UTF-8 temporary path");
        let object = git.git(root, &["hash-object", "-w", &path, original])?;
        let object = String::from_utf8(object.stdout)?;
        staged.push(git.git(root, &["cat-file", "blob", object.trim()])?.stdout);
    }
    git.hook(&json!({"cwd": root, "tool_input": {}}))?;
    for ((name, _), staged) in cases.iter().zip(&staged) {
        assert_eq!(&fs::read(root.join(name))?, staged, "{name}");
    }
    let unchanged = [
        "control-heavy.txt",
        "dense-control.txt",
        "leading-eof.txt",
        "delete.txt",
        "lone-before-pair.txt",
    ];
    for (name, bytes) in &cases {
        assert_eq!(
            unchanged.contains(name),
            &fs::read(root.join(name))? == bytes,
            "{name}"
        );
    }
    Ok(())
}

/// Index blobs and edited bytes where Git's `text=auto` decision depends on the CRLF the index already holds.
fn git_index_cases() -> Vec<(&'static str, &'static [u8], &'static [u8])> {
    let edited: &[u8] = b"a\r\nb\r\nc\r\n";
    vec![
        ("index-crlf.txt", b"a\r\nb\r\n", edited),
        ("index-mixed.txt", b"a\r\nb\n", edited),
        ("index-lf.txt", b"a\nb\n", edited),
        ("index-lone.txt", b"a\rb\r\n", edited),
        ("index-nul.txt", b"a\0b\r\n", edited),
        ("forced-index.txt", b"a\r\nb\r\n", edited),
        ("eol-only-index.txt", b"a\r\nb\r\n", edited),
    ]
}

#[test]
fn hook_keeps_crlf_that_git_keeps_from_the_index() -> Result<()> {
    let git = Isolated::new(true)?;
    let repository = git.repository()?;
    let root = repository.path();
    fs::write(
        root.join(".gitattributes"),
        "forced-* text\neol-only-* !text eol=lf\n",
    )?;
    let cases = git_index_cases();
    let mut staged = Vec::new();
    for (name, indexed, edited) in &cases {
        git.index_raw(root, name, indexed)?;
        fs::write(root.join(name), edited)?;
        git.git(root, &["add", name])?;
        let blob = format!(":{name}");
        staged.push(git.git(root, &["cat-file", "blob", &blob])?.stdout);
        git.index_raw(root, name, indexed)?;
    }
    git.hook(&json!({"cwd": root, "tool_input": {"file_path": "index-crlf.txt"}}))?;
    for ((name, _, edited), staged) in cases.iter().zip(&staged) {
        let written = fs::read(root.join(name))?;
        assert_eq!(&written, staged, "{name}");
        assert_eq!(
            ["index-crlf.txt", "index-mixed.txt"].contains(name),
            written == *edited,
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn hook_keeps_read_only_files_and_files_changed_since_they_were_read() -> Result<()> {
    let git = Isolated::new(true)?;
    let repository = git.repository()?;
    let root = repository.path();
    let locked = root.join("locked.txt");
    fs::write(&locked, CRLF)?;
    let writable = fs::metadata(&locked)?.permissions();
    let mut permissions = writable.clone();
    permissions.set_readonly(true);
    fs::set_permissions(&locked, permissions)?;
    let output = git.hook(&json!({"cwd": root, "tool_input": {"file_path": "locked.txt"}}))?;
    assert_eq!(fs::read(&locked)?, CRLF);
    assert!(!String::from_utf8(output.stdout)?.contains("locked.txt"));
    fs::set_permissions(&locked, writable)?;

    let raced = root.join("raced.txt");
    fs::write(&raced, CRLF)?;
    assert!(!line_endings::replace(&raced, b"other\r\n", LF)?);
    assert_eq!(fs::read(&raced)?, CRLF);
    assert!(line_endings::replace(&raced, CRLF, LF)?);
    assert_eq!(fs::read(&raced)?, LF);
    Ok(())
}

#[cfg(unix)]
#[test]
fn hook_keeps_permissions_and_skips_symbolic_links() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let git = Isolated::new(true)?;
    let repository = git.repository()?;
    let root = repository.path();
    let script = root.join("script.sh");
    fs::write(&script, CRLF)?;
    fs::set_permissions(&script, fs::Permissions::from_mode(0o750))?;
    let outside = tempfile::tempdir()?;
    let target = outside.path().join("target.txt");
    fs::write(&target, CRLF)?;
    std::os::unix::fs::symlink(&target, root.join("link.txt"))?;
    git.hook(&json!({"cwd": root, "tool_input": {"file_path": "link.txt"}}))?;
    assert_eq!(fs::read(&script)?, LF);
    assert_eq!(fs::metadata(&script)?.permissions().mode() & 0o777, 0o750);
    assert!(
        fs::symlink_metadata(root.join("link.txt"))?
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(&target)?, CRLF);
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn hook_normalizes_a_path_that_is_not_utf8() -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let git = Isolated::new(true)?;
    let repository = git.repository()?;
    let root = repository.path();
    let name = std::ffi::OsStr::from_bytes(b"latin-\xe9.txt");
    fs::write(root.join(name), CRLF)?;
    git.hook(&json!({"cwd": root, "tool_input": {}}))?;
    assert_eq!(fs::read(root.join(name))?, LF);
    Ok(())
}

#[test]
fn hook_warns_and_succeeds_when_git_cannot_inspect_the_directory() -> Result<()> {
    let git = Isolated::new(true)?;
    let scope = tempfile::tempdir()?;
    let missing = scope.path().join("removed");
    let output = git.hook(&json!({"cwd": missing, "tool_input": {"file_path": "notes.txt"}}))?;
    let diagnostics = String::from_utf8(output.stderr)?;
    assert!(
        diagnostics.contains("skipped line-ending normalization") && diagnostics.contains("git -C"),
        "{diagnostics}"
    );
    Ok(())
}

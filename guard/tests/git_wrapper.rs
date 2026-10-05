//! The installed `git` entry point against the real git of the host it runs on.
//! Windows installs a copy of dotguard named `git.exe`, so the test runs that form everywhere.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

/// A scratch directory holding the copied wrapper, an isolated home, and a repository, removed when dropped.
struct Wrapper {
    scope: PathBuf,
    git: PathBuf,
}

impl Drop for Wrapper {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.scope);
    }
}

impl Wrapper {
    fn new(name: &str) -> Self {
        let scope =
            std::env::temp_dir().join(format!("dotguard-wrapper-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scope);
        std::fs::create_dir_all(scope.join("repository")).unwrap();
        let git = scope.join(format!("git{}", std::env::consts::EXE_SUFFIX));
        std::fs::copy(env!("CARGO_BIN_EXE_dotguard"), &git).unwrap();
        let wrapper = Self { scope, git };
        assert!(wrapper.run(&["init", "-q"], None).status.success());
        wrapper
    }

    fn home(&self) -> PathBuf {
        self.scope.join("home")
    }

    /// Runs the wrapper with an isolated home and Git configuration, so its audit records stay out of the developer's.
    /// Waivers set in the calling shell, such as the one a force push sets for its hooks, are removed so they cannot decide a refusal.
    fn run(&self, arguments: &[&str], input: Option<&[u8]>) -> Output {
        self.run_with(arguments, input, &[])
    }

    fn run_with(&self, arguments: &[&str], input: Option<&[u8]>, waivers: &[&str]) -> Output {
        let mut command = Command::new(&self.git);
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("ALLOW_") {
                command.env_remove(name);
            }
        }
        for waiver in waivers {
            command.env(waiver, "1");
        }
        let mut child = command
            .args(arguments)
            .current_dir(self.scope.join("repository"))
            .env("HOME", self.home())
            .env("USERPROFILE", self.home())
            .env("GIT_CONFIG_GLOBAL", self.scope.join("gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        if let Some(bytes) = input {
            std::io::Write::write_all(&mut stdin, bytes).unwrap();
        }
        drop(stdin);
        child.wait_with_output().unwrap()
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn a_copy_named_git_refuses_skipped_hooks_and_records_the_refusal() {
    let wrapper = Wrapper::new("no-verify");
    for arguments in [
        &["commit", "--allow-empty", "-m", "x", "--no-verify"][..],
        &["push", "--no-verify", "origin", "main"][..],
    ] {
        let refused = wrapper.run(arguments, None);
        assert_eq!(refused.status.code(), Some(1), "{}", text(&refused.stderr));
        assert!(
            text(&refused.stderr).contains("Signing and hook checks cannot be bypassed."),
            "{}",
            text(&refused.stderr)
        );
    }
    let log = std::fs::read_to_string(wrapper.home().join(".local/state/git-bypass.log")).unwrap();
    assert_eq!(
        log.lines().filter(|line| line.contains("REJECT")).count(),
        2
    );
}

#[test]
fn a_copy_named_git_refuses_destructive_commands_unless_waived() {
    let wrapper = Wrapper::new("force");
    let refused = wrapper.run(&["reset", "--hard"], None);
    assert_eq!(refused.status.code(), Some(1), "{}", text(&refused.stderr));
    assert!(text(&refused.stderr).contains("ALLOW_FORCE=1"));
    let waived = wrapper.run_with(&["reset", "--hard"], None, &["ALLOW_FORCE"]);
    assert!(
        text(&waived.stderr).contains("ALLOW_FORCE=1 — allowing"),
        "{}",
        text(&waived.stderr)
    );
    let log = std::fs::read_to_string(wrapper.home().join(".local/state/git-bypass.log")).unwrap();
    assert!(log.lines().any(|line| line.contains("BYPASS")), "{log}");
}

#[test]
fn a_copy_named_git_passes_arguments_streams_and_status_through() {
    let wrapper = Wrapper::new("passthrough");
    let version = wrapper.run(&["--version"], None);
    assert!(version.status.success());
    assert!(text(&version.stdout).starts_with("git version"));
    let hashed = wrapper.run(&["hash-object", "--stdin"], Some(b"hello\n"));
    assert_eq!(
        text(&hashed.stdout).trim(),
        "ce013625030ba8dba906f756967f9e9ca394464a"
    );
    let missing = wrapper.run(
        &["rev-parse", "--verify", "--quiet", "refs/heads/absent"],
        None,
    );
    assert_eq!(missing.status.code(), Some(1));
    let unknown = wrapper.run(&["no-such-command"], None);
    assert_eq!(unknown.status.code(), Some(1));
}

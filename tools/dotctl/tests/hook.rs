//! The signing hooks, driven against throwaway repos.
//!
//! Every test runs with `GIT_CONFIG_GLOBAL` and `GIT_CONFIG_SYSTEM` pointed away from the real ones, so no signing key is configured and no 1Password dialog can appear: the interesting path is the one where signing *fails*,
//! and that is the path that must not leave a mess behind.
//!
//! The repository location is isolated the same way: git exports `GIT_DIR` and its siblings to a hook it runs, and a worktree's is absolute, so they are cleared rather than inherited.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// A git repo in a temp dir, isolated from the user's git configuration.
struct Repo {
    dir: tempfile::TempDir,
}

impl Repo {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let repo = Self { dir };
        // An empty init.templateDir keeps the real post-commit hook out of this repo; otherwise the test's own commit would trigger it.
        repo.git(&["-c", "init.templateDir=", "init", "--initial-branch=main"]);
        repo.git(&["config", "user.name", "Test"]);
        repo.git(&["config", "user.email", "test@example.invalid"]);
        repo
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn empty_config(&self) -> PathBuf {
        let path = self.path().join("isolated.gitconfig");
        if !path.exists() {
            std::fs::write(&path, "").expect("config written");
        }
        path
    }

    fn command(&self, program: &str) -> Command {
        let mut cmd = Command::new(program);
        cmd.current_dir(self.path())
            .env("GIT_CONFIG_GLOBAL", self.empty_config())
            .env("GIT_CONFIG_SYSTEM", self.empty_config());
        // git exports these to a hook it runs, and a worktree's GIT_DIR is an absolute path.
        // Inherited, they aim every git call below at the repository the hook came from rather than at this temp dir, so a `cargo test` under `lefthook run pre-push` rewrites the real HEAD.
        for var in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_COMMON_DIR",
            "GIT_PREFIX",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        ] {
            cmd.env_remove(var);
        }
        cmd
    }

    fn git(&self, args: &[&str]) -> Output {
        self.command("git").args(args).output().expect("git runs")
    }

    fn git_ok(&self, args: &[&str]) -> bool {
        self.git(args).status.success()
    }

    fn stdout(&self, args: &[&str]) -> String {
        String::from_utf8_lossy(&self.git(args).stdout)
            .trim()
            .to_owned()
    }

    /// Commits a file with signing explicitly off, which is exactly the state the post-commit hook exists to catch.
    fn commit_unsigned(&self, name: &str) {
        std::fs::write(self.path().join(name), "content\n").expect("file written");
        self.git(&["add", name]);
        self.git(&[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--no-verify",
            "-m",
            "unsigned",
        ]);
    }

    fn hook(&self, args: &[&str], stdin: Option<&str>) -> Output {
        let mut cmd = self.command(env!("CARGO_BIN_EXE_dotctl"));
        cmd.arg("hook").args(args);
        match stdin {
            Some(text) => {
                use std::io::Write;
                cmd.stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                let mut child = cmd.spawn().expect("dotctl runs");
                child
                    .stdin
                    .as_mut()
                    .expect("stdin")
                    .write_all(text.as_bytes())
                    .expect("stdin written");
                child.wait_with_output().expect("dotctl finishes")
            }
            None => cmd.output().expect("dotctl runs"),
        }
    }
}

/// With no signing key anywhere, the re-sign cannot succeed, and the commit must not be left on HEAD.
/// This one is the root commit, so the ref goes.
#[test]
fn post_commit_rolls_back_when_signing_is_impossible() {
    let repo = Repo::new();
    repo.commit_unsigned("a.txt");
    assert!(repo.git_ok(&["rev-parse", "HEAD"]), "commit exists");

    let out = repo.hook(&["post-commit"], None);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert_eq!(out.status.code(), Some(1), "stderr:\n{stderr}");
    assert!(
        !repo.git_ok(&["rev-parse", "HEAD"]),
        "root commit should have been dropped, stderr:\n{stderr}"
    );
    assert_eq!(
        repo.stdout(&["diff", "--cached", "--name-only"]),
        "a.txt",
        "the user's changes must survive as staged"
    );
}

/// The second commit has a parent, so the rollback steps back to it rather than deleting the ref.
#[test]
fn post_commit_rolls_back_to_the_parent_commit() {
    let repo = Repo::new();
    repo.commit_unsigned("a.txt");
    let root = repo.stdout(&["rev-parse", "HEAD"]);
    repo.commit_unsigned("b.txt");

    let out = repo.hook(&["post-commit"], None);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert_eq!(out.status.code(), Some(1), "stderr:\n{stderr}");
    assert_eq!(repo.stdout(&["rev-parse", "HEAD"]), root);
    assert_eq!(repo.stdout(&["diff", "--cached", "--name-only"]), "b.txt");
}

/// The recursion guard: the hook run by its own `--amend` must do nothing.
#[test]
fn post_commit_is_a_noop_while_holding_the_lock() {
    let repo = Repo::new();
    repo.commit_unsigned("a.txt");
    let head = repo.stdout(&["rev-parse", "HEAD"]);

    let out = repo
        .command(env!("CARGO_BIN_EXE_dotctl"))
        .args(["hook", "post-commit"])
        .env("GIT_HOOK_FORCE_SIGN_LOCK", "1")
        .output()
        .expect("dotctl runs");

    assert!(out.status.success());
    assert_eq!(repo.stdout(&["rev-parse", "HEAD"]), head, "HEAD untouched");
}

/// Mid-rebase, mid-merge and friends: amending would corrupt the operation,
/// so the hook stands down and leaves the commit alone.
#[test]
fn post_commit_stands_down_mid_operation() {
    let repo = Repo::new();
    repo.commit_unsigned("a.txt");
    let head = repo.stdout(&["rev-parse", "HEAD"]);
    std::fs::write(repo.path().join(".git").join("MERGE_HEAD"), &head).expect("marker written");

    let out = repo.hook(&["post-commit"], None);

    assert!(out.status.success());
    assert_eq!(repo.stdout(&["rev-parse", "HEAD"]), head, "HEAD untouched");
}

#[test]
fn pre_push_refuses_an_unsigned_commit() {
    let repo = Repo::new();
    repo.commit_unsigned("a.txt");
    let head = repo.stdout(&["rev-parse", "HEAD"]);
    let zero = "0".repeat(head.len());
    let line = format!("refs/heads/main {head} refs/heads/main {zero}\n");

    let out = repo.hook(&["pre-push", "origin"], Some(&line));
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert_eq!(out.status.code(), Some(1), "stderr:\n{stderr}");
    assert!(
        stderr.contains("unsigned commit cannot be pushed"),
        "{stderr}"
    );
    assert!(
        stderr.contains(&head),
        "the offending sha is named: {stderr}"
    );
}

/// Deleting a remote branch pushes an all-zero local sha; there is nothing to verify, and the hook must not invent a reason to refuse.
#[test]
fn pre_push_allows_a_branch_deletion() {
    let repo = Repo::new();
    repo.commit_unsigned("a.txt");
    let head = repo.stdout(&["rev-parse", "HEAD"]);
    let zero = "0".repeat(head.len());
    let line = format!("(delete) {zero} refs/heads/gone {head}\n");

    let out = repo.hook(&["pre-push", "origin"], Some(&line));

    assert!(
        out.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// No refs on stdin means no work, not a failure.
#[test]
fn pre_push_accepts_an_empty_range() {
    let repo = Repo::new();
    repo.commit_unsigned("a.txt");
    let out = repo.hook(&["pre-push", "origin"], Some(""));
    assert!(out.status.success());
}

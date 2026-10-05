//! The pre-push and post-commit gates against real repositories: each refusal reaches standard error as one complete record.

use dotguard::refusal::Refusal;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

#[path = "support/fixture_git.rs"]
mod fixture_git;

const ZERO: &str = "0000000000000000000000000000000000000000";

struct Repository {
    scope: PathBuf,
}

impl Drop for Repository {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.scope);
    }
}

impl Repository {
    fn new(name: &str) -> Self {
        let scope =
            std::env::temp_dir().join(format!("dotguard-gates-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scope);
        std::fs::create_dir_all(scope.join("home")).unwrap();
        std::fs::write(
            scope.join("gitconfig"),
            "[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n",
        )
        .unwrap();
        let repository = Self { scope };
        repository.git(&["init", "-q", "-b", "main"]);
        repository
    }

    fn path(&self) -> PathBuf {
        self.scope.join("home")
    }

    fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = fixture_git::command(program, &self.scope.join("gitconfig"));
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("ALLOW_") {
                command.env_remove(name);
            }
        }
        command
            .current_dir(self.path())
            .env("HOME", self.path())
            .env("USERPROFILE", self.path());
        command
    }

    fn git(&self, arguments: &[&str]) -> String {
        let output = self.command("git").args(arguments).output().unwrap();
        assert!(
            output.status.success(),
            "git {arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    /// Records an unsigned commit with plumbing, so no hook runs before the gate under test.
    /// Signing is configured only for the gate, so the fixture commits stay unsigned.
    fn commit(&self, file: &str, parents: &[&str]) -> String {
        std::fs::write(self.path().join(file), file).unwrap();
        self.git(&["add", file]);
        let tree = self.git(&["write-tree"]);
        let mut arguments = vec!["commit-tree", &tree, "-m", file];
        for parent in parents {
            arguments.extend(["-p", parent]);
        }
        self.git(&arguments)
    }

    /// Runs the gate with `config` added to the repository's configuration through the environment.
    fn dotguard(&self, arguments: &[&str], input: &str, config: &[(&str, &str)]) -> Output {
        let mut command = self.command(env!("CARGO_BIN_EXE_dotguard"));
        command.env("GIT_CONFIG_COUNT", config.len().to_string());
        for (index, (key, value)) in config.iter().enumerate() {
            command
                .env(format!("GIT_CONFIG_KEY_{index}"), key)
                .env(format!("GIT_CONFIG_VALUE_{index}"), value);
        }
        let mut child = command
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
}

/// The record a refused gate printed last on standard error.
fn refusal(output: &Output) -> Refusal {
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let refusal = Refusal::find(&stderr).expect("a structured refusal");
    assert!(refusal.is_complete(), "{refusal:?}");
    assert!(
        stderr.trim_end().ends_with(&refusal.line()),
        "the record must be the last line: {stderr}"
    );
    refusal
}

/// Signing required, as the pre-push gate reads it.
const SIGNED: [(&str, &str); 1] = [("commit.gpgsign", "true")];

/// A repository whose `origin` already carries `base`.
fn published(name: &str) -> (Repository, String) {
    let repository = Repository::new(name);
    let base = repository.commit("base", &[]);
    repository.git(&["update-ref", "refs/remotes/origin/main", &base]);
    (repository, base)
}

#[test]
fn a_rewrite_is_refused_with_the_integration_step_and_a_waiver_for_exactly_that_update() {
    let (repository, base) = published("rewrite");
    let published = repository.commit("published", &[&base]);
    repository.git(&["update-ref", "refs/remotes/origin/main", &published]);
    let local = repository.commit("local", &[&base]);
    let refused = refusal(&repository.dotguard(
        &["pre-push", "origin"],
        &format!("refs/heads/main {local} refs/heads/main {published}\n"),
        &SIGNED,
    ));
    assert_eq!(refused.rule, "push.history");
    assert_eq!(
        refused.next.as_deref().unwrap_or_default(),
        "git pull --rebase origin main"
    );
    assert_eq!(
        refused.waiver.as_deref(),
        Some("ALLOW_FORCE=1 git push --force-with-lease origin refs/heads/main:refs/heads/main")
    );
}

#[test]
fn a_deletion_is_refused_with_the_forge_step_and_a_waiver_that_repeats_it() {
    let (repository, base) = published("delete");
    let refused = refusal(&repository.dotguard(
        &["pre-push", "origin"],
        &format!("(delete) {ZERO} refs/heads/topic {base}\n"),
        &SIGNED,
    ));
    assert_eq!(refused.rule, "push.history");
    assert_eq!(
        refused.next.as_deref().unwrap_or_default(),
        "gh api -X DELETE 'repos/{owner}/{repo}/git/refs/heads/topic'"
    );
    assert_eq!(
        refused.waiver.as_deref(),
        Some("ALLOW_FORCE=1 git push origin :refs/heads/topic")
    );
}

#[test]
fn one_linear_unsigned_branch_is_rebased_onto_the_parent_of_its_oldest_unsigned_commit() {
    let (repository, base) = published("linear");
    let first = repository.commit("first", &[&base]);
    let second = repository.commit("second", &[&first]);
    repository.git(&["update-ref", "refs/heads/topic", &second]);
    let refused = refusal(&repository.dotguard(
        &["pre-push", "origin"],
        &format!("refs/heads/topic {second} refs/heads/topic {ZERO}\n"),
        &SIGNED,
    ));
    assert_eq!(refused.rule, "push.signature");
    assert_eq!(
        refused.next.as_deref().unwrap_or_default(),
        format!("git rebase --exec 'git commit --amend --no-edit -S' {base} topic")
    );
    assert_eq!(refused.evidence.len(), 2);
}

/// An amend moves only the checked-out branch, so a tag at the checked-out commit is listed rather than amended.
#[test]
fn an_unsigned_tag_at_the_checked_out_commit_is_listed_instead_of_amended() {
    let (repository, base) = published("tag");
    let tip = repository.commit("tip", &[&base]);
    repository.git(&["update-ref", "HEAD", &tip]);
    repository.git(&["tag", "v1", &tip]);
    let refused = refusal(&repository.dotguard(
        &["pre-push", "origin"],
        &format!("refs/tags/v1 {tip} refs/tags/v1 {ZERO}\n"),
        &SIGNED,
    ));
    assert_eq!(refused.rule, "push.signature");
    assert_eq!(
        refused.next.as_deref().unwrap_or_default(),
        format!("git log '--format=%h %G? %s' {tip} --not --remotes=origin")
    );
    let refused = refusal(&repository.dotguard(
        &["pre-push", "origin"],
        &format!("refs/heads/main {tip} refs/heads/main {base}\n"),
        &SIGNED,
    ));
    assert_eq!(
        refused.next.as_deref().unwrap_or_default(),
        "git commit --amend -S --no-edit"
    );
}

#[test]
fn several_refs_or_a_merge_are_listed_instead_of_rebased() {
    let (repository, base) = published("several");
    let left = repository.commit("left", &[&base]);
    let right = repository.commit("right", &[&base]);
    let refused = refusal(&repository.dotguard(
        &["pre-push", "origin"],
        &format!(
            "refs/heads/left {left} refs/heads/left {ZERO}\nrefs/heads/right {right} refs/heads/right {ZERO}\n"
        ),
        &SIGNED,
    ));
    assert_eq!(refused.rule, "push.signature");
    assert_eq!(
        refused.next.as_deref().unwrap_or_default(),
        format!("git log '--format=%h %G? %s' {left} --not --remotes=origin")
    );
    assert_eq!(refused.evidence.len(), 2);

    let merge = repository.commit("merge", &[&left, &right]);
    let refused = refusal(&repository.dotguard(
        &["pre-push", "origin"],
        &format!("refs/heads/main {merge} refs/heads/main {base}\n"),
        &SIGNED,
    ));
    assert_eq!(
        refused.next.as_deref().unwrap_or_default(),
        format!("git log '--format=%h %G? %s' {merge} --not --remotes=origin")
    );
    assert_eq!(refused.evidence.len(), 3);
}

#[test]
fn a_failed_signature_rolls_the_commit_back_and_names_the_commit_to_repeat() {
    let repository = Repository::new("signing");
    let base = repository.commit("base", &[]);
    let unsigned = repository.commit("unsigned", &[&base]);
    repository.git(&["update-ref", "HEAD", &unsigned]);
    let missing = repository.path().join("missing");
    let missing = missing.to_str().unwrap();
    let refused = refusal(&repository.dotguard(
        &["post-commit"],
        "",
        &[
            ("commit.gpgsign", "true"),
            ("gpg.format", "ssh"),
            ("user.signingkey", missing),
            ("gpg.ssh.program", missing),
        ],
    ));
    assert_eq!(refused.rule, "commit.signature");
    assert_eq!(
        refused.next.as_deref().unwrap_or_default(),
        format!("git commit --reuse-message={unsigned}")
    );
    assert_eq!(refused.waiver, None);
    assert_eq!(repository.git(&["rev-parse", "HEAD"]), base);
}

//! The post-commit gate against real repositories: an ordinary commit that records no change is rolled back.

use dotguard::refusal::Refusal;
use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "support/fixture_git.rs"]
mod fixture_git;

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
            std::env::temp_dir().join(format!("dotguard-empty-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scope);
        std::fs::create_dir_all(scope.join("home")).unwrap();
        std::fs::write(
            scope.join("gitconfig"),
            "[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n",
        )
        .unwrap();
        let repository = Self { scope };
        repository.git(&["init", "-q", "-b", "main"]);
        std::fs::write(repository.path().join("a.txt"), "a\n").unwrap();
        repository.git(&["add", "a.txt"]);
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

    /// Records a commit with plumbing, so no hook runs before the gate under test.
    fn commit(&self, tree: &str, parent: Option<&str>) -> String {
        let mut arguments = vec!["commit-tree", tree, "-m", "commit"];
        if let Some(parent) = parent {
            arguments.extend(["-p", parent]);
        }
        let commit = self.git(&arguments);
        self.git(&["update-ref", "HEAD", &commit]);
        commit
    }

    fn post_commit(&self, waivers: &[&str]) -> bool {
        self.post_commit_output(waivers).status.success()
    }

    fn post_commit_output(&self, waivers: &[&str]) -> std::process::Output {
        let mut command = self.command(env!("CARGO_BIN_EXE_dotguard"));
        command.arg("post-commit");
        for waiver in waivers {
            command.env(waiver, "1");
        }
        command.output().unwrap()
    }
}

fn head(repository: &Repository) -> String {
    repository.git(&["rev-parse", "HEAD"])
}

#[test]
fn an_empty_commit_is_rolled_back_and_its_files_stay_in_the_working_tree() {
    let repository = Repository::new("refused");
    let tree = repository.git(&["write-tree"]);
    let base = repository.commit(&tree, None);
    repository.commit(&tree, Some(&base));
    let refused = repository.post_commit_output(&[]);
    assert_eq!(refused.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&refused.stderr);
    let refusal = Refusal::parse(stderr.trim_end().lines().last().unwrap_or_default())
        .unwrap_or_else(|| panic!("the record must be the last line: {stderr}"));
    assert!(refusal.is_complete(), "{refusal:?}");
    assert_eq!(refusal.rule, "commit.empty");
    assert_eq!(head(&repository), base);
    assert!(Path::new(&repository.path().join("a.txt")).is_file());
}

#[test]
fn a_waived_empty_commit_a_change_and_the_root_commit_are_kept() {
    let repository = Repository::new("kept");
    let tree = repository.git(&["write-tree"]);
    let base = repository.commit(&tree, None);
    assert!(repository.post_commit(&[]));
    assert_eq!(head(&repository), base);

    let empty = repository.commit(&tree, Some(&base));
    assert!(repository.post_commit(&["ALLOW_EMPTY"]));
    assert_eq!(head(&repository), empty);

    std::fs::write(repository.path().join("a.txt"), "b\n").unwrap();
    repository.git(&["add", "a.txt"]);
    let changed = repository.git(&["write-tree"]);
    let change = repository.commit(&changed, Some(&empty));
    assert!(repository.post_commit(&[]));
    assert_eq!(head(&repository), change);
}

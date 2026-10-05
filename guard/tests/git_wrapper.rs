//! Tests the installed `git` wrapper with the real git of the host.
//! Windows installs a copy of dotguard named `git.exe`, so the test runs that form everywhere.

use dotguard::refusal::Refusal;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::sync::OnceLock;

#[path = "support/fixture_git.rs"]
mod fixture_git;
#[path = "support/wrapper.rs"]
mod wrapper;

/// The wrapper every test runs, which the binary places once before it starts its first process.
/// Every spawn here follows `Wrapper::inheriting`, which waits for it.
fn wrapper_binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| wrapper::place(&wrapper::preparation("git-wrapper")))
}

/// A scratch directory holding an isolated home and a repository, removed when dropped.
struct Wrapper {
    scope: PathBuf,
    git: &'static Path,
    /// The variables a child starts from as a caller exported them, or `None` for this process's own.
    environment: Option<Vec<(OsString, OsString)>>,
}

impl Drop for Wrapper {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.scope);
    }
}

impl Wrapper {
    fn new(name: &str) -> Self {
        Self::starting(name, None)
    }

    /// A wrapper whose children start from `environment` as a caller would export it.
    fn inheriting(name: &str, environment: impl IntoIterator<Item = (OsString, OsString)>) -> Self {
        Self::starting(name, Some(environment.into_iter().collect()))
    }

    fn starting(name: &str, environment: Option<Vec<(OsString, OsString)>>) -> Self {
        let git = wrapper_binary();
        let scope =
            std::env::temp_dir().join(format!("dotguard-wrapper-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scope);
        std::fs::create_dir_all(scope.join("repository")).unwrap();
        let wrapper = Self {
            scope,
            git,
            environment,
        };
        assert!(wrapper.run(&["init", "-q"], None).status.success());
        wrapper
    }

    fn home(&self) -> PathBuf {
        self.scope.join("home")
    }

    /// Runs the wrapper with an isolated home and Git configuration, so its audit records stay out of the developer's.
    /// The wrapper drops waivers from the calling shell, such as the one a force push sets for its hooks, so they never decide a refusal.
    fn run(&self, arguments: &[&str], input: Option<&[u8]>) -> Output {
        self.run_with(arguments, input, &[])
    }

    fn run_with(&self, arguments: &[&str], input: Option<&[u8]>, waivers: &[&str]) -> Output {
        let set: Vec<(&str, &str)> = waivers.iter().map(|waiver| (*waiver, "1")).collect();
        self.run_env(arguments, input, &set)
    }

    /// Runs the wrapper with `variables` set.
    /// The run drops a stack lease token from the calling shell, as it drops a waiver.
    fn run_env(
        &self,
        arguments: &[&str],
        input: Option<&[u8]>,
        variables: &[(&str, &str)],
    ) -> Output {
        let global = self.scope.join("gitconfig");
        let mut command = match &self.environment {
            Some(environment) => fixture_git::inheriting(self.git, &global, environment.clone()),
            None => fixture_git::command(self.git, &global),
        };
        for (name, _) in std::env::vars_os() {
            let text = name.to_string_lossy();
            if text.starts_with("ALLOW_") || text.starts_with("DOTGUARD_") {
                command.env_remove(name);
            }
        }
        for (name, value) in variables {
            command.env(name, value);
        }
        let mut child = command
            .args(arguments)
            .current_dir(self.scope.join("repository"))
            .env("HOME", self.home())
            .env("USERPROFILE", self.home())
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

/// The complete record a refused command printed as the last line of its standard error.
fn last_record(output: &Output) -> Refusal {
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    let refusal = Refusal::parse(stderr.trim_end().lines().last().unwrap_or_default())
        .unwrap_or_else(|| panic!("the record must be the last line: {stderr}"));
    assert!(refusal.is_complete(), "{refusal:?}");
    refusal
}

#[test]
fn a_copy_named_git_refuses_skipped_hooks_and_records_the_refusal() {
    let wrapper = Wrapper::new("no-verify");
    for arguments in [
        &["commit", "--allow-empty", "-m", "x", "--no-verify"][..],
        &["push", "--no-verify", "origin", "main"][..],
    ] {
        let refused = wrapper.run(arguments, None);
        let refusal = last_record(&refused);
        assert_eq!(refusal.rule, "git.no-verify");
        assert_eq!(refusal.waiver, None);
        let next = refusal.next.unwrap_or_default();
        assert!(
            next.starts_with("git ") && !next.contains("--no-verify"),
            "{next}"
        );
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
    let refusal = last_record(&wrapper.run(&["reset", "--hard"], None));
    assert_eq!(refusal.rule, "git.force");
    assert_eq!(
        refusal.waiver.as_deref(),
        Some("ALLOW_FORCE=1 git reset --hard")
    );
    let waived = wrapper.run_with(&["reset", "--hard"], None, &["ALLOW_FORCE"]);
    assert!(
        text(&waived.stderr).contains("ALLOW_FORCE=1 — allowing"),
        "{}",
        text(&waived.stderr)
    );
    let log = std::fs::read_to_string(wrapper.home().join(".local/state/git-bypass.log")).unwrap();
    assert!(log.lines().any(|line| line.contains("BYPASS")), "{log}");
}

/// Git reads `--rep` as `--repo` and the next word as its value, so the refusal names only the branch Git deletes.
#[test]
fn an_abbreviated_option_value_is_not_a_deleted_branch() {
    let wrapper = Wrapper::new("push-abbreviation");
    std::fs::write(
        wrapper.scope.join("gitconfig"),
        "[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n",
    )
    .unwrap();
    let succeeds = |arguments: &[&str]| {
        let output = wrapper.run(arguments, None);
        assert!(
            output.status.success(),
            "git {arguments:?}: {}",
            text(&output.stderr)
        );
        text(&output.stdout).trim().to_owned()
    };
    succeeds(&["init", "-q", "--bare", "../remote.git"]);
    let tree = succeeds(&["write-tree"]);
    let commit = succeeds(&["commit-tree", &tree, "-m", "c"]);
    for branch in ["a", "origin"] {
        succeeds(&[
            "push",
            "-q",
            "../remote.git",
            &format!("{commit}:refs/heads/{branch}"),
        ]);
    }
    succeeds(&["remote", "add", "origin", "../remote.git"]);
    let deletion = ["push", "-q", "--delete", "--rep", "x", "origin", "a"];
    assert_eq!(
        last_record(&wrapper.run(&deletion, None))
            .next
            .as_deref()
            .unwrap_or_default(),
        "gh api -X DELETE 'repos/{owner}/{repo}/git/refs/heads/a'"
    );
    let waived = wrapper.run_with(&deletion, None, &["ALLOW_FORCE"]);
    assert!(waived.status.success(), "{}", text(&waived.stderr));
    let remote = |branch: &str| {
        wrapper
            .run(
                &[
                    "--git-dir=../remote.git",
                    "rev-parse",
                    "-q",
                    "--verify",
                    &format!("refs/heads/{branch}"),
                ],
                None,
            )
            .status
            .success()
    };
    assert!(!remote("a"), "Git deleted the named branch");
    assert!(remote("origin"), "Git read origin as the repository");
}

/// The wrapper lets `push --repo=<remote> :<ref>` through because Git reads `:<ref>` as the repository, not as a deletion.
#[test]
fn git_reads_the_first_push_positional_as_the_repository() {
    let wrapper = Wrapper::new("push-repository");
    std::fs::write(
        wrapper.scope.join("gitconfig"),
        "[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n",
    )
    .unwrap();
    let succeeds = |arguments: &[&str]| {
        let output = wrapper.run(arguments, None);
        assert!(
            output.status.success(),
            "git {arguments:?}: {}",
            text(&output.stderr)
        );
        text(&output.stdout).trim().to_owned()
    };
    succeeds(&["init", "-q", "--bare", "../remote.git"]);
    let tree = succeeds(&["write-tree"]);
    let commit = succeeds(&["commit-tree", &tree, "-m", "c"]);
    succeeds(&[
        "push",
        "-q",
        "../remote.git",
        &format!("{commit}:refs/heads/c"),
    ]);
    succeeds(&["remote", "add", "origin", "../remote.git"]);
    for arguments in [
        &["push", "--repo=origin", ":c"][..],
        &["push", "--repo", "origin", ":c"][..],
    ] {
        let pushed = wrapper.run(arguments, None);
        assert!(
            Refusal::find(&text(&pushed.stderr)).is_none(),
            "{}",
            text(&pushed.stderr)
        );
        assert!(!pushed.status.success(), "git {arguments:?} succeeded");
        succeeds(&[
            "--git-dir=../remote.git",
            "rev-parse",
            "--verify",
            "refs/heads/c",
        ]);
    }
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

#[test]
fn a_callers_git_repository_variables_cannot_redirect_the_wrapper() {
    let outer = Wrapper::new("outer-repository");
    let outer_git = outer.scope.join("repository/.git");
    let config_before = std::fs::read(outer_git.join("config")).unwrap();
    let redirection = [
        ("GIT_DIR", outer_git.clone()),
        ("GIT_WORK_TREE", outer.scope.join("repository")),
        ("GIT_INDEX_FILE", outer_git.join("index")),
    ]
    .map(|(name, value)| (OsString::from(name), value.into_os_string()));
    let inner = Wrapper::inheriting("inner-repository", std::env::vars_os().chain(redirection));
    assert!(
        inner.scope.join("repository/.git").is_dir(),
        "the scratch repository was not initialized"
    );
    assert_eq!(
        std::fs::read(outer_git.join("config")).unwrap(),
        config_before
    );
}

/// Git rejects a push with an option it doesn't accept, so a refused one suggests no deletion built from its other words.
#[test]
fn a_refused_push_with_an_option_git_rejects_suggests_nothing() {
    let wrapper = Wrapper::new("push-unread");
    std::fs::write(
        wrapper.scope.join("gitconfig"),
        "[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n",
    )
    .unwrap();
    let succeeds = |arguments: &[&str]| {
        let output = wrapper.run(arguments, None);
        assert!(
            output.status.success(),
            "git {arguments:?}: {}",
            text(&output.stderr)
        );
        text(&output.stdout).trim().to_owned()
    };
    succeeds(&["init", "-q", "--bare", "../remote.git"]);
    let tree = succeeds(&["write-tree"]);
    let commit = succeeds(&["commit-tree", &tree, "-m", "c"]);
    for branch in ["a", "origin", "x"] {
        succeeds(&[
            "push",
            "-q",
            "../remote.git",
            &format!("{commit}:refs/heads/{branch}"),
        ]);
    }
    succeeds(&["remote", "add", "origin", "../remote.git"]);
    for option in ["--re", "--bogus", "--d"] {
        let deletion = ["push", "--delete", option, "x", "origin", "a"];
        let refused = last_record(&wrapper.run(&deletion, None));
        assert_eq!(refused.rule, "git.force");
        assert_eq!(refused.next, None, "git {deletion:?}");
        assert!(
            refused
                .cause
                .contains(&format!("Git does not accept `{option}`")),
            "{}",
            refused.cause
        );
        let waived = wrapper.run_with(&deletion, None, &["ALLOW_FORCE"]);
        assert!(!waived.status.success(), "Git ran git {deletion:?}");
    }
    for branch in ["a", "origin", "x"] {
        succeeds(&[
            "--git-dir=../remote.git",
            "rev-parse",
            "-q",
            "--verify",
            &format!("refs/heads/{branch}"),
        ]);
    }
}

/// A rebase of a published commit runs only through the stack tooling's lease or a recorded waiver, and the refusal names the stack command.
#[test]
fn a_rebase_of_a_published_branch_runs_only_through_the_stack_lease() {
    let wrapper = Wrapper::new("published-rebase");
    std::fs::write(
        wrapper.scope.join("gitconfig"),
        "[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n",
    )
    .unwrap();
    let succeeds = |arguments: &[&str]| {
        let output = wrapper.run(arguments, None);
        assert!(
            output.status.success(),
            "git {arguments:?}: {}",
            text(&output.stderr)
        );
        text(&output.stdout).trim().to_owned()
    };
    succeeds(&["init", "-q", "--bare", "../remote.git"]);
    let tree = succeeds(&["write-tree"]);
    let base = succeeds(&["commit-tree", &tree, "-m", "base"]);
    let published = succeeds(&["commit-tree", &tree, "-p", &base, "-m", "published"]);
    let moved = succeeds(&["commit-tree", &tree, "-p", &base, "-m", "moved"]);
    succeeds(&["update-ref", "refs/heads/main", &moved]);
    succeeds(&["update-ref", "refs/heads/topic", &published]);
    succeeds(&["update-ref", "refs/heads/local", &published]);
    succeeds(&[
        "remote",
        "add",
        "origin",
        "https://github.com/P4suta/dotfiles.git",
    ]);
    succeeds(&["update-ref", "refs/remotes/origin/topic", &published]);
    succeeds(&["symbolic-ref", "HEAD", "refs/heads/topic"]);
    succeeds(&["reset", "-q", "--keep", "topic"]);

    let refused = last_record(&wrapper.run(&["rebase", "main"], None));
    assert_eq!(refused.rule, "git.force");
    assert_eq!(
        refused.next.as_deref(),
        Some("pr-workflow stack sync --repo P4suta/dotfiles")
    );
    assert_eq!(
        refused.waiver.as_deref(),
        Some("ALLOW_FORCE=1 git rebase main")
    );
    let unleased = wrapper.run_env(&["rebase", "main"], None, &[("DOTGUARD_STACK", "token")]);
    assert_eq!(last_record(&unleased).rule, "git.force");
    assert!(
        Refusal::find(&text(
            &wrapper.run(&["rebase", "main", "local"], None).stderr
        ))
        .is_none(),
        "an unpublished branch rebases freely"
    );
    succeeds(&["checkout", "-q", "topic"]);

    std::fs::write(
        wrapper.scope.join("repository/.git/dotguard-stack.lease"),
        "token",
    )
    .unwrap();
    let leased = wrapper.run_env(&["rebase", "main"], None, &[("DOTGUARD_STACK", "token")]);
    assert!(leased.status.success(), "{}", text(&leased.stderr));
    assert!(Refusal::find(&text(&leased.stderr)).is_none());
    let log = std::fs::read_to_string(wrapper.home().join(".local/state/git-bypass.log")).unwrap();
    assert!(
        log.lines()
            .any(|line| line.contains("\tSTACK\t") && line.contains("git rebase main")),
        "{log}"
    );
}

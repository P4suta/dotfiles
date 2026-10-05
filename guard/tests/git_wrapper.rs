//! The installed `git` entry point against the real git of the host it runs on.
//! Windows installs a copy of dotguard named `git.exe`, so the test runs that form everywhere.

use dotguard::refusal::Refusal;
use std::path::PathBuf;
use std::process::{Output, Stdio};

#[path = "support/fixture_git.rs"]
mod fixture_git;
#[path = "support/wrapper.rs"]
mod wrapper;

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
        wrapper::install(&git);
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
        let mut command = fixture_git::command(&self.git, &self.scope.join("gitconfig"));
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
    // SAFETY: std serializes its own environment access and nothing in this binary reads the environment outside std; concurrent tests remove these variables from their children too.
    let saved: Vec<_> = ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"]
        .iter()
        .map(|name| (*name, std::env::var_os(name)))
        .collect();
    unsafe {
        std::env::set_var("GIT_DIR", &outer_git);
        std::env::set_var("GIT_WORK_TREE", outer.scope.join("repository"));
        std::env::set_var("GIT_INDEX_FILE", outer_git.join("index"));
    }
    let inner = Wrapper::new("inner-repository");
    let initialized = inner.scope.join("repository/.git").is_dir();
    for (name, value) in saved {
        // SAFETY: restores the values captured above.
        unsafe {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
    assert!(initialized, "the scratch repository was not initialized");
    assert_eq!(
        std::fs::read(outer_git.join("config")).unwrap(),
        config_before
    );
}

/// Git rejects a push with an option it does not accept, so a refused one suggests no deletion built from its other words.
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

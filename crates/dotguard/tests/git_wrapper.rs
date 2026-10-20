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
        if std::env::var_os("SKILL_OPS_INCIDENT_EVIDENCE_ROOT")
            .is_none_or(|root| !self.scope.starts_with(root))
        {
            platform::cleanup::remove_dir_all(&self.scope);
        }
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
        let temporary = std::env::var_os("SKILL_OPS_INCIDENT_EVIDENCE_ROOT")
            .map_or_else(std::env::temp_dir, PathBuf::from);
        let scope = temporary.join(format!("dotguard-wrapper-{}-{name}", std::process::id()));
        platform::cleanup::remove_dir_all(&scope);
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
    /// The wrapper drops bypasses from the calling shell, such as the one a force push sets for its hooks, so they never decide a refusal.
    fn run(&self, arguments: &[&str], input: Option<&[u8]>) -> Output {
        self.run_with(arguments, input, &[])
    }

    fn run_with(&self, arguments: &[&str], input: Option<&[u8]>, bypasses: &[&str]) -> Output {
        let set: Vec<(&str, &str)> = bypasses.iter().map(|bypass| (*bypass, "1")).collect();
        self.run_env(arguments, input, &set)
    }

    /// Runs the wrapper with `variables` set.
    /// The run drops a stack lease token from the calling shell, as it drops a bypass.
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
        command.env("DOTFILES_EXPECTED_FAILURE", "counterexample");
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

#[test]
fn a_failed_real_push_records_its_rule_status_and_invocation() {
    let wrapper = Wrapper::new("failed-push-observation");
    let opt_in = wrapper.home().join(platform::incidents::EXPERIMENTAL);
    std::fs::create_dir_all(opt_in.parent().unwrap()).unwrap();
    std::fs::write(opt_in, rules::incident_rules::EXPERIMENTAL_OPT_IN).unwrap();
    let state = wrapper.scope.join("state");
    let output = wrapper.run_env(
        &["push", "origin", "main"],
        None,
        &[
            ("SKILL_OPS_STATE", state.to_str().unwrap()),
            ("DOTFILES_EXPECTED_FAILURE", ""),
        ],
    );
    assert!(!output.status.success());
    let records: Vec<_> = std::fs::read_dir(state.join("incidents"))
        .unwrap()
        .map(|entry| {
            serde_json::from_slice::<serde_json::Value>(
                &std::fs::read(entry.unwrap().path()).unwrap(),
            )
            .unwrap()
        })
        .collect();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0]["rule"], "push.failed");
    assert_eq!(records[0]["source"], "failed-delivery");
    assert!(
        records[0]["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item
                .as_str()
                .is_some_and(|text| text.contains("git push origin main"))),
        "{records:?}"
    );
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
        assert_eq!(refusal.bypass, None);
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
    std::fs::write(wrapper.scope.join("repository/tracked"), "work").unwrap();
    assert!(wrapper.run(&["add", "tracked"], None).status.success());
    let refusal = last_record(&wrapper.run(&["reset", "--hard"], None));
    assert_eq!(refusal.rule, "git.force");
    assert_eq!(
        refusal.bypass.as_deref(),
        Some("ALLOW_FORCE=1 git reset --hard")
    );
    let bypassed = wrapper.run_with(&["reset", "--hard"], None, &["ALLOW_FORCE"]);
    assert!(
        text(&bypassed.stderr).contains("ALLOW_FORCE=1 — allowing"),
        "{}",
        text(&bypassed.stderr)
    );
    let log = std::fs::read_to_string(wrapper.home().join(".local/state/git-bypass.log")).unwrap();
    assert!(log.lines().any(|line| line.contains("BYPASS")), "{log}");
}

/// Lefthook saves the unstaged hunks of partly staged files, then hides them with `checkout --force -- <paths>`.
#[test]
fn lefthooks_forced_checkout_passes_only_when_its_patch_holds_the_discarded_hunks() {
    let wrapper = Wrapper::new("lefthook-partial");
    let repository = wrapper.scope.join("repository");
    for name in ["staged", "other"] {
        std::fs::write(repository.join(name), "staged\n").unwrap();
        assert!(wrapper.run(&["add", name], None).status.success());
        std::fs::write(repository.join(name), "staged\nunstaged\n").unwrap();
    }
    let hide = ["checkout", "--force", "--", "staged", "other"];
    assert_eq!(last_record(&wrapper.run(&hide, None)).rule, "git.force");
    let diff = |name: &str| {
        let output = wrapper.run(
            &[
                "diff",
                "--binary",
                "--unified=0",
                "--no-color",
                "--no-ext-diff",
                "--src-prefix=a/",
                "--dst-prefix=b/",
                "--patch",
                "--submodule=short",
                "--",
                name,
            ],
            None,
        );
        assert!(output.status.success(), "{}", text(&output.stderr));
        output.stdout
    };
    let patch = repository.join(".git/info/lefthook-unstaged.patch");
    std::fs::write(&patch, diff("staged")).unwrap();
    assert_eq!(last_record(&wrapper.run(&hide, None)).rule, "git.force");
    assert!(
        std::fs::read_to_string(repository.join("other"))
            .unwrap()
            .contains("unstaged")
    );
    std::fs::write(&patch, [diff("staged"), diff("other")].concat()).unwrap();
    let hidden = wrapper.run(&hide, None);
    assert!(hidden.status.success(), "{}", text(&hidden.stderr));
    for name in ["staged", "other"] {
        assert_eq!(
            std::fs::read_to_string(repository.join(name)).unwrap(),
            "staged\n"
        );
    }
    std::fs::write(repository.join("staged"), "staged\nagain\n").unwrap();
    assert_eq!(
        last_record(&wrapper.run(&["checkout", "--force", "--", "staged"], None)).rule,
        "git.force",
        "a hunk the patch never saw stays protected"
    );
}

/// A clone made with `--no-checkout` has no index, so its status lists every file as a staged deletion that a forced checkout restores.
#[test]
fn a_forced_checkout_that_only_restores_deleted_files_passes() {
    let wrapper = Wrapper::new("force-restore");
    std::fs::write(
        wrapper.scope.join("gitconfig"),
        "[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n",
    )
    .unwrap();
    let repository = wrapper.scope.join("repository");
    let succeeds = |arguments: &[&str]| {
        let output = wrapper.run(arguments, None);
        assert!(
            output.status.success(),
            "git {arguments:?}: {}",
            text(&output.stderr)
        );
        text(&output.stdout).trim().to_owned()
    };
    std::fs::write(repository.join("kept"), "kept\n").unwrap();
    succeeds(&["add", "kept"]);
    let tree = succeeds(&["write-tree"]);
    let commit = succeeds(&["commit-tree", &tree, "-m", "c"]);
    succeeds(&["update-ref", "HEAD", &commit]);
    let checkout = ["checkout", "--force", "--detach", commit.as_str()];

    std::fs::remove_file(repository.join(".git/index")).unwrap();
    std::fs::remove_file(repository.join("kept")).unwrap();
    succeeds(&checkout);
    assert_eq!(
        std::fs::read_to_string(repository.join("kept")).unwrap(),
        "kept\n"
    );

    succeeds(&["rm", "--quiet", "--cached", "kept"]);
    std::fs::write(repository.join("kept"), "mine\n").unwrap();
    assert_eq!(
        last_record(&wrapper.run(&checkout, None)).rule,
        "git.force",
        "a staged deletion whose file stays in the worktree keeps that copy protected"
    );
    assert_eq!(
        std::fs::read_to_string(repository.join("kept")).unwrap(),
        "mine\n"
    );
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
    let bypassed = wrapper.run_with(&deletion, None, &["ALLOW_FORCE"]);
    assert!(bypassed.status.success(), "{}", text(&bypassed.stderr));
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
fn paired_git_commands_preserve_results_when_notifications_fail() {
    let environment = std::env::vars_os().filter(|(name, _)| {
        ![
            "CI",
            "SSH_CONNECTION",
            "SSH_TTY",
            "DOTFILES_HOST_CACHE",
            "WSL_DISTRO_NAME",
            platform::speech::INTERNAL,
        ]
        .iter()
        .any(|excluded| name == excluded)
    });
    let wrapper = Wrapper::inheriting("paired-notifications", environment);
    let config = wrapper.home().join(platform::speech::CONFIG);
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("frequency"), "all\n").unwrap();
    std::fs::write(config.join(platform::speech::MATERIALS), "owned fixture").unwrap();
    let variables = [
        ("GIT_AUTHOR_NAME", "Fixture"),
        ("GIT_AUTHOR_EMAIL", "fixture@example.invalid"),
        ("GIT_COMMITTER_NAME", "Fixture"),
        ("GIT_COMMITTER_EMAIL", "fixture@example.invalid"),
        ("DISPLAY", "notification-fixture"),
        ("SESSIONNAME", "Console"),
    ];
    let committed = wrapper.run_env(
        &["commit", "--allow-empty", "-F", "-"],
        Some(b"paired commit\n"),
        &variables,
    );
    assert!(committed.status.success(), "{committed:?}");
    assert!(text(&committed.stdout).contains("paired commit"));
    let warning = "warning: could not queue the spoken notification: notification operation failed";
    assert_eq!(
        text(&committed.stderr)
            .lines()
            .filter(|line| *line == warning)
            .count(),
        2
    );
    assert_eq!(
        text(&wrapper.run(&["rev-list", "--count", "HEAD"], None).stdout).trim(),
        "1"
    );
    let remote = wrapper.scope.join("remote.git");
    let remote = remote.to_str().unwrap();
    assert!(
        wrapper
            .run(&["init", "--bare", "--quiet", remote], None)
            .status
            .success()
    );
    let pushed = wrapper.run_env(&["push", remote, "HEAD:refs/heads/main"], None, &variables);
    assert!(pushed.status.success(), "{pushed:?}");
    assert_eq!(pushed.stdout, Vec::<u8>::new());
    let error = text(&pushed.stderr);
    assert!(error.contains("[new branch]"));
    assert!(!error.contains("Everything up-to-date"));
    assert_eq!(error.lines().filter(|line| *line == warning).count(), 2);
    assert_eq!(
        text(
            &wrapper
                .run(
                    &[
                        "--git-dir",
                        remote,
                        "rev-list",
                        "--count",
                        "refs/heads/main"
                    ],
                    None
                )
                .stdout
        )
        .trim(),
        "1"
    );
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
        let bypassed = wrapper.run_with(&deletion, None, &["ALLOW_FORCE"]);
        assert!(!bypassed.status.success(), "Git ran git {deletion:?}");
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

/// A rebase of a published commit runs only through the stack tooling's lease or a recorded bypass, and the refusal names the stack command.
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
        refused.bypass.as_deref(),
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

/// A test fixture or probe under the temporary directory holds no owner work, so only a push keeps its refusal there.
#[test]
fn a_throwaway_repository_admits_overrides_but_not_a_push() {
    let wrapper = Wrapper::new("throwaway");
    std::fs::write(
        wrapper.scope.join("gitconfig"),
        "[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n",
    )
    .unwrap();
    let unguarded = [("DOTGUARD_STRICT_TEMPORARY", "0")];
    for arguments in [
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "-q",
            "-m",
            "x",
        ][..],
        &["commit", "--allow-empty", "-q", "--no-verify", "-m", "y"][..],
        &["reset", "--hard", "-q", "HEAD~1"][..],
    ] {
        let output = wrapper.run_env(arguments, None, &unguarded);
        assert!(
            output.status.success(),
            "{arguments:?}: {}",
            text(&output.stderr)
        );
    }
    let push = wrapper.run_env(&["push", "--force", "origin", "main"], None, &unguarded);
    assert_eq!(last_record(&push).rule, "git.force");
    let guarded = wrapper.run(
        &["commit", "--allow-empty", "-q", "--no-verify", "-m", "z"],
        None,
    );
    assert_eq!(last_record(&guarded).rule, "git.no-verify");
}

/// `pr-workflow prune-merged` deletes the branches it verified under the stack lease, and the guard records the deletion as tooling.
#[test]
fn a_forced_branch_deletion_passes_only_under_the_stack_lease() {
    let wrapper = Wrapper::new("prune-lease");
    std::fs::write(
        wrapper.scope.join("gitconfig"),
        "[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n",
    )
    .unwrap();
    let succeeds = |arguments: &[&str]| {
        let output = wrapper.run(arguments, None);
        assert!(
            output.status.success(),
            "{arguments:?}: {}",
            text(&output.stderr)
        );
    };
    succeeds(&["commit", "--allow-empty", "-q", "-m", "x"]);
    succeeds(&["branch", "merged"]);
    let token = [("DOTGUARD_STACK", "token")];
    assert_eq!(
        last_record(&wrapper.run_env(&["branch", "-D", "merged"], None, &token)).rule,
        "git.force"
    );
    std::fs::write(
        wrapper.scope.join("repository/.git/dotguard-stack.lease"),
        "token",
    )
    .unwrap();
    let leased = wrapper.run_env(&["branch", "-D", "merged"], None, &token);
    assert!(leased.status.success(), "{}", text(&leased.stderr));
    let log = std::fs::read_to_string(wrapper.home().join(".local/state/git-bypass.log")).unwrap();
    assert!(
        log.lines()
            .any(|line| line.contains("\tSTACK\t") && line.contains("git branch -D merged")),
        "{log}"
    );
}

/// Codex deletes its own turn refs, and a ref no porcelain command manages loses nothing the owner keeps.
#[test]
fn deleting_a_tool_ref_passes_and_deleting_a_branch_ref_does_not() {
    let wrapper = Wrapper::new("tool-ref");
    std::fs::write(
        wrapper.scope.join("gitconfig"),
        "[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n",
    )
    .unwrap();
    let succeeds = |arguments: &[&str]| {
        let output = wrapper.run(arguments, None);
        assert!(
            output.status.success(),
            "{arguments:?}: {}",
            text(&output.stderr)
        );
    };
    succeeds(&["commit", "--allow-empty", "-q", "-m", "x"]);
    succeeds(&["update-ref", "refs/codex/turn-diffs/1", "HEAD"]);
    succeeds(&["update-ref", "-d", "refs/codex/turn-diffs/1"]);
    let refused = wrapper.run(&["update-ref", "-d", "refs/heads/main"], None);
    assert_eq!(last_record(&refused).rule, "git.force");
}

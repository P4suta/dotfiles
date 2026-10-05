#![allow(
    clippy::disallowed_methods,
    reason = "integration tests spawn the binaries and real tools they verify"
)]

use anyhow::{Context, Result, ensure};
use dotfiles_xtask::next_action::{
    Environment, HostStatus, Observed, PullRequest, Recommendation, classify_checks,
    decision_backlog, github_repository, host_status, inspect, perform, recommend,
};
use dotfiles_xtask::next_action_rules::{
    Checks, Head, Host, Pr, State, Step, Upstream, decide, executable,
};
use dotfiles_xtask::skill_ops::catalog;
use dotfiles_xtask::tool::Tool;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;

/// A clean, pushed draft PR with passing checks; each case changes the fields that define its state.
fn published() -> State {
    State {
        host: Host::Ready,
        head: Head::Feature,
        dirty: false,
        undecided: false,
        incomplete: false,
        behind_base: false,
        ahead_base: true,
        upstream: Upstream::Current,
        push_paused: false,
        pr: Pr::Draft,
        checks: Checks::Passing,
        workflows: true,
        review_paused: false,
    }
}

#[test]
fn every_state_maps_to_its_single_next_step() {
    let base = published();
    let cases = [
        (
            State {
                host: Host::TempUnset,
                head: Head::Detached,
                ..base
            },
            Step::SetTemp,
        ),
        (
            State {
                host: Host::VolumeMissing,
                dirty: true,
                ..base
            },
            Step::MountVolume,
        ),
        (
            State {
                host: Host::TempMissing,
                head: Head::Detached,
                ..base
            },
            Step::CreateTemp,
        ),
        (
            State {
                head: Head::Detached,
                ..base
            },
            Step::AttachBranch,
        ),
        (
            State {
                head: Head::Default,
                dirty: true,
                ..base
            },
            Step::BranchFromDefault,
        ),
        (
            State {
                head: Head::Default,
                behind_base: true,
                ..base
            },
            Step::BranchFromDefault,
        ),
        (
            State {
                head: Head::Default,
                ahead_base: false,
                behind_base: true,
                ..base
            },
            Step::FastForward,
        ),
        (
            State {
                head: Head::Default,
                ahead_base: false,
                ..base
            },
            Step::StartBranch,
        ),
        (
            State {
                undecided: true,
                incomplete: true,
                dirty: true,
                ..base
            },
            Step::DraftDecisions,
        ),
        (
            State {
                incomplete: true,
                dirty: true,
                ..base
            },
            Step::CompleteDecisions,
        ),
        (
            State {
                dirty: true,
                pr: Pr::Merged,
                ..base
            },
            Step::Commit,
        ),
        (
            State {
                pr: Pr::Merged,
                behind_base: true,
                ..base
            },
            Step::Finished,
        ),
        (
            State {
                pr: Pr::Closed,
                ..base
            },
            Step::Reopen,
        ),
        (
            State {
                upstream: Upstream::Behind,
                behind_base: true,
                ..base
            },
            Step::SyncUpstream,
        ),
        (
            State {
                behind_base: true,
                upstream: Upstream::Diverged,
                ..base
            },
            Step::Rebase,
        ),
        (
            State {
                ahead_base: false,
                pr: Pr::None,
                ..base
            },
            Step::StartWork,
        ),
        (
            State {
                upstream: Upstream::Diverged,
                push_paused: true,
                review_paused: true,
                ..base
            },
            Step::AwaitPushResume,
        ),
        (
            State {
                upstream: Upstream::Ahead,
                ..base
            },
            Step::Push,
        ),
        (
            State {
                upstream: Upstream::Absent,
                pr: Pr::Unknown,
                ..base
            },
            Step::Push,
        ),
        (
            State {
                upstream: Upstream::Ahead,
                review_paused: true,
                ..base
            },
            Step::Push,
        ),
        (
            State {
                upstream: Upstream::Ahead,
                review_paused: true,
                pr: Pr::Ready,
                ..base
            },
            Step::AwaitReviewResume,
        ),
        (
            State {
                upstream: Upstream::Diverged,
                review_paused: true,
                pr: Pr::Unknown,
                ..base
            },
            Step::InspectGithub,
        ),
        (
            State {
                upstream: Upstream::Diverged,
                ..base
            },
            Step::ForcePush,
        ),
        (
            State {
                pr: Pr::Unknown,
                ..base
            },
            Step::InspectGithub,
        ),
        (
            State {
                pr: Pr::None,
                ..base
            },
            Step::CreatePr,
        ),
        (
            State {
                checks: Checks::Failing,
                review_paused: true,
                ..base
            },
            Step::FixChecks,
        ),
        (
            State {
                checks: Checks::Pending,
                ..base
            },
            Step::AwaitChecks,
        ),
        (
            State {
                checks: Checks::None,
                ..base
            },
            Step::AwaitChecks,
        ),
        (
            State {
                checks: Checks::None,
                workflows: false,
                ..base
            },
            Step::MarkReady,
        ),
        (
            State {
                checks: Checks::Pending,
                workflows: false,
                ..base
            },
            Step::AwaitChecks,
        ),
        (
            State {
                review_paused: true,
                ..base
            },
            Step::AwaitReviewResume,
        ),
        (base, Step::MarkReady),
        (
            State {
                pr: Pr::Ready,
                ..base
            },
            Step::AwaitReview,
        ),
    ];
    for (state, expected) in cases {
        assert_eq!(decide(state), expected, "{state:?}");
    }
}

#[test]
fn only_local_reversible_steps_are_executable() {
    for step in [
        Step::CreateTemp,
        Step::FastForward,
        Step::SyncUpstream,
        Step::DraftDecisions,
    ] {
        assert!(executable(step), "{step:?}");
    }
    for step in [
        Step::SetTemp,
        Step::MountVolume,
        Step::Commit,
        Step::Rebase,
        Step::Push,
        Step::ForcePush,
        Step::CreatePr,
        Step::MarkReady,
        Step::Reopen,
        Step::AwaitChecks,
    ] {
        assert!(!executable(step), "{step:?}");
    }
}

#[test]
fn failing_checks_outrank_pending_and_an_empty_rollup_reports_none() {
    let success = json!({"status": "COMPLETED", "conclusion": "SUCCESS"});
    let skipped = json!({"status": "COMPLETED", "conclusion": "SKIPPED"});
    let running = json!({"status": "IN_PROGRESS", "conclusion": ""});
    let failed = json!({"status": "COMPLETED", "conclusion": "FAILURE"});
    let context_pending = json!({"state": "PENDING"});
    let context_error = json!({"state": "ERROR"});
    assert_eq!(classify_checks(&[]), Checks::None);
    assert_eq!(
        classify_checks(&[success.clone(), skipped.clone()]),
        Checks::Passing
    );
    assert_eq!(
        classify_checks(&[success.clone(), running]),
        Checks::Pending
    );
    assert_eq!(
        classify_checks(&[context_pending.clone(), failed]),
        Checks::Failing
    );
    assert_eq!(classify_checks(&[context_pending]), Checks::Pending);
    assert_eq!(classify_checks(&[success, context_error]), Checks::Failing);
}

#[test]
fn github_remotes_resolve_to_owner_and_repository() {
    for url in [
        "git@github.com:P4suta/dotfiles.git",
        "https://github.com/P4suta/dotfiles",
        "https://github.com/P4suta/dotfiles.git\n",
        "ssh://git@github.com/P4suta/dotfiles.git",
    ] {
        assert_eq!(github_repository(url).as_deref(), Some("P4suta/dotfiles"));
    }
    for url in [
        "https://example.com/P4suta/dotfiles",
        "https://github.com/P4suta",
        "https://github.com/P4suta/dotfiles/tree/main",
    ] {
        assert_eq!(github_repository(url), None, "{url}");
    }
}

#[test]
fn a_missing_temporary_directory_is_reported_and_created() -> Result<()> {
    let scope = tempfile::tempdir()?;
    assert_eq!(host_status(Some(scope.path()), None), HostStatus::Ready);
    assert_eq!(host_status(None, Some(scope.path())), HostStatus::TempUnset);
    let temp = scope.path().join("tmp");
    assert_eq!(
        host_status(Some(&temp), Some(scope.path())),
        HostStatus::TempMissing { path: temp.clone() }
    );
    let fixture = Fixture::new()?;
    fs::remove_dir(fixture.scope.path().join("tmp"))?;
    let recommendation = recommend(fixture.next()?);
    assert_eq!(
        recommendation.command,
        Some(format!(
            "mkdir \"{}\"",
            fixture.scope.path().join("tmp").display()
        ))
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn an_unmounted_volume_is_reported_before_a_missing_directory() {
    let letter = (b'E'..=b'Z')
        .map(char::from)
        .find(|letter| !Path::new(&format!("{letter}:\\")).is_dir())
        .expect("an unused drive letter");
    let volume = PathBuf::from(format!("{letter}:\\"));
    let scope = tempfile::tempdir().unwrap();
    assert_eq!(
        host_status(Some(scope.path()), Some(&volume.join("cargo"))),
        HostStatus::VolumeMissing {
            path: volume.clone()
        }
    );
    assert_eq!(
        host_status(Some(&volume.join("tmp")), None),
        HostStatus::VolumeMissing { path: volume }
    );
}

#[test]
fn skills_without_a_current_or_complete_decision_are_listed() -> Result<()> {
    let root = tempfile::tempdir()?;
    let tree = root.path().join("dot_agents/skills");
    for name in ["alpha", "beta", "gamma"] {
        fs::create_dir_all(tree.join(name))?;
        fs::write(
            tree.join(name).join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Check {name}.\n---\n# Contract\nCheck it.\n"),
        )?;
    }
    let revisions = catalog(&tree)?;
    let decisions = root.path().join("docs/skills/decisions");
    fs::create_dir_all(&decisions)?;
    for (name, reason) in [("beta", ""), ("gamma", "Keep it.")] {
        fs::write(
            decisions.join(format!("{name}.json")),
            serde_json::to_string(&json!({
                "revision": revisions.skills[name].revision,
                "outcome": "keep",
                "reason": reason,
                "evidence": [format!("dot_agents/skills/{name}/SKILL.md")],
                "revisit": null,
            }))?,
        )?;
    }
    assert_eq!(
        decision_backlog(root.path())?,
        (vec!["alpha".to_owned()], vec!["beta".to_owned()])
    );
    fs::write(
        tree.join("gamma/SKILL.md"),
        "---\nname: gamma\ndescription: Check gamma.\n---\n# Contract\nChanged.\n",
    )?;
    assert_eq!(
        decision_backlog(root.path())?.0,
        ["alpha".to_owned(), "gamma".to_owned()]
    );
    Ok(())
}

struct Fixture {
    scope: tempfile::TempDir,
    clone: PathBuf,
    config: PathBuf,
}

impl Fixture {
    fn new() -> Result<Self> {
        let scope = tempfile::tempdir()?;
        let config = scope.path().join("gitconfig");
        fs::write(
            &config,
            "[user]\n\tname = Fixture\n\temail = fixture@example.com\n[commit]\n\tgpgsign = false\n[init]\n\tdefaultBranch = main\n",
        )?;
        fs::create_dir(scope.path().join("hooks"))?;
        let fixture = Self {
            clone: scope.path().join("clone"),
            config,
            scope,
        };
        fixture.git(fixture.scope.path(), &["init", "--bare", "origin.git"])?;
        fixture.git(fixture.scope.path(), &["clone", "origin.git", "clone"])?;
        let hooks = fixture.scope.path().join("hooks");
        fixture.git(
            &fixture.clone,
            &["config", "core.hooksPath", hooks.to_str().unwrap()],
        )?;
        fixture.commit("first")?;
        fixture.git(&fixture.clone, &["push", "--quiet", "-u", "origin", "main"])?;
        fs::create_dir(fixture.scope.path().join("home"))?;
        fs::create_dir(fixture.scope.path().join("tmp"))?;
        Ok(fixture)
    }

    fn git(&self, directory: &Path, arguments: &[&str]) -> Result<()> {
        let output = Tool::Git
            .command()
            .current_dir(directory)
            .env("GIT_CONFIG_GLOBAL", &self.config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args(arguments)
            .output()?;
        ensure!(
            output.status.success(),
            "git {arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }

    fn commit(&self, name: &str) -> Result<()> {
        fs::write(self.clone.join(name), name)?;
        self.git(&self.clone, &["add", name])?;
        self.git(&self.clone, &["commit", "--quiet", "-m", name])
    }

    /// Advances origin/main from a second clone and fetches it into the fixture clone.
    fn advance_origin(&self) -> Result<()> {
        self.git(
            self.scope.path(),
            &["clone", "--quiet", "origin.git", "other"],
        )?;
        let other = self.scope.path().join("other");
        fs::write(other.join("upstream"), "upstream")?;
        self.git(&other, &["add", "upstream"])?;
        self.git(&other, &["commit", "--quiet", "-m", "upstream"])?;
        self.git(&other, &["push", "--quiet", "origin", "main"])?;
        self.git(&self.clone, &["fetch", "--quiet", "origin"])
    }

    fn environment(&self) -> Environment {
        Environment {
            home: self.scope.path().join("home"),
            temp: Some(self.scope.path().join("tmp")),
            cargo_target: None,
            offline: true,
        }
    }

    fn next(&self) -> Result<Observed> {
        inspect(&self.clone, &self.environment())
    }

    fn step(&self) -> Result<Step> {
        Ok(recommend(self.next()?).action)
    }
}

#[test]
fn the_default_branch_is_fast_forwarded_and_then_branched_from() -> Result<()> {
    let fixture = Fixture::new()?;
    assert_eq!(fixture.step()?, Step::StartBranch);
    fixture.advance_origin()?;
    let recommendation = recommend(fixture.next()?);
    assert_eq!(recommendation.action, Step::FastForward);
    assert!(recommendation.executable);
    assert_eq!(recommendation.state.behind_base, 1);
    perform(&fixture.clone, &recommendation)?;
    assert_eq!(fixture.step()?, Step::StartBranch);
    fixture.commit("local")?;
    assert_eq!(fixture.step()?, Step::BranchFromDefault);
    Ok(())
}

#[test]
fn a_feature_branch_moves_from_work_to_commit_to_push() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.git(&fixture.clone, &["switch", "--quiet", "-c", "feat/topic"])?;
    assert_eq!(fixture.step()?, Step::StartWork);
    fs::write(fixture.clone.join("draft"), "draft")?;
    let recommendation = recommend(fixture.next()?);
    assert_eq!(recommendation.action, Step::Commit);
    assert_eq!(recommendation.state.changed_files, 1);
    let refusal = perform(&fixture.clone, &recommendation)
        .unwrap_err()
        .to_string();
    assert!(refusal.contains("`commit` is not a local, reversible step"));
    assert!(refusal.contains("next action: Review the 1 uncommitted changes"));
    fs::remove_file(fixture.clone.join("draft"))?;
    fixture.commit("change")?;
    let recommendation = recommend(fixture.next()?);
    assert_eq!(recommendation.action, Step::Push);
    assert_eq!(recommendation.state.unpushed, 1);
    assert_eq!(
        recommendation.command.as_deref(),
        Some("git push -u origin feat/topic")
    );
    fs::create_dir_all(fixture.scope.path().join("home/.config/git"))?;
    fs::write(
        fixture.scope.path().join("home/.config/git/push-paused"),
        "",
    )?;
    assert_eq!(fixture.step()?, Step::AwaitPushResume);
    fs::remove_file(fixture.scope.path().join("home/.config/git/push-paused"))?;
    fixture.git(
        &fixture.clone,
        &["push", "--quiet", "-u", "origin", "feat/topic"],
    )?;
    let observed = fixture.next()?;
    assert_eq!(observed.upstream.as_deref(), Some("origin/feat/topic"));
    assert_eq!(observed.unpushed, 0);
    assert_eq!(recommend(observed).action, Step::InspectGithub);
    fixture.advance_origin()?;
    assert_eq!(fixture.step()?, Step::Rebase);
    Ok(())
}

#[test]
fn a_detached_head_and_a_missing_temporary_directory_are_reported() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.git(&fixture.clone, &["switch", "--quiet", "--detach"])?;
    assert_eq!(fixture.step()?, Step::AttachBranch);
    fs::remove_dir(fixture.scope.path().join("tmp"))?;
    let recommendation = recommend(fixture.next()?);
    assert_eq!(recommendation.action, Step::CreateTemp);
    perform(&fixture.clone, &recommendation)?;
    assert!(fixture.scope.path().join("tmp").is_dir());
    assert_eq!(fixture.step()?, Step::AttachBranch);
    Ok(())
}

fn on_github(pr: Option<(&str, bool, Checks)>, review_paused: bool) -> Observed {
    Observed {
        host: HostStatus::Ready,
        branch: Some("feat/topic".into()),
        changed_files: 0,
        base: "origin/main".into(),
        behind_base: 0,
        ahead_base: 2,
        upstream: Some("origin/feat/topic".into()),
        unpushed: 0,
        unpulled: 0,
        undecided_skills: vec![],
        incomplete_decisions: vec![],
        push_paused: false,
        review_paused,
        repository: Some("P4suta/dotfiles".into()),
        workflows: true,
        github_inspected: true,
        pull_request: pr.map(|(state, draft, checks)| PullRequest {
            number: 7,
            url: "https://github.com/P4suta/dotfiles/pull/7".into(),
            state: state.into(),
            draft,
            base: "main".into(),
            checks,
        }),
    }
}

#[test]
fn github_states_name_the_pull_request_and_its_command() -> Result<()> {
    let cases = [
        (
            on_github(None, false),
            Step::CreatePr,
            Some(
                "pr-workflow create --repo P4suta/dotfiles --issue <issue> --title <title> --body-file <file> --head feat/topic",
            ),
        ),
        (
            on_github(Some(("OPEN", true, Checks::Failing)), false),
            Step::FixChecks,
            Some("gh pr checks 7 --repo P4suta/dotfiles"),
        ),
        (
            on_github(Some(("OPEN", true, Checks::Pending)), false),
            Step::AwaitChecks,
            Some("gh pr checks 7 --repo P4suta/dotfiles --watch"),
        ),
        (
            on_github(Some(("OPEN", true, Checks::Passing)), false),
            Step::MarkReady,
            Some("pr-workflow ready --repo P4suta/dotfiles --issue <issue> --pr 7"),
        ),
        (
            on_github(Some(("OPEN", true, Checks::Passing)), true),
            Step::AwaitReviewResume,
            None,
        ),
        (
            on_github(Some(("OPEN", false, Checks::Passing)), false),
            Step::AwaitReview,
            Some("gh pr view 7 --repo P4suta/dotfiles --comments"),
        ),
        (
            on_github(Some(("MERGED", false, Checks::Passing)), false),
            Step::Finished,
            None,
        ),
        (
            Observed {
                unpushed: 1,
                ..on_github(Some(("OPEN", false, Checks::Passing)), true)
            },
            Step::AwaitReviewResume,
            None,
        ),
        (
            on_github(Some(("CLOSED", false, Checks::Failing)), false),
            Step::Reopen,
            Some("gh pr reopen 7 --repo P4suta/dotfiles"),
        ),
    ];
    for (observed, step, command) in cases {
        let recommendation = recommend(observed);
        assert_eq!(recommendation.action, step);
        assert_eq!(recommendation.command.as_deref(), command, "{step:?}");
        assert!(!recommendation.executable);
        let line = serde_json::to_value(&recommendation)?;
        assert_eq!(line["action"], serde_json::to_value(step)?);
        assert_eq!(line["state"]["host"]["status"], "ready");
    }
    Ok(())
}

#[test]
fn the_report_is_one_json_line_with_a_kebab_case_action() -> Result<()> {
    let mut observed = on_github(None, false);
    observed.undecided_skills = vec!["adr".into(), "just".into()];
    let recommendation = recommend(observed);
    let line = serde_json::to_string(&recommendation)?;
    assert!(!line.contains('\n'));
    let value: serde_json::Value = serde_json::from_str(&line)?;
    assert_eq!(value["action"], "draft-decisions");
    assert_eq!(value["executable"], true);
    assert_eq!(value["command"], "just decide adr");
    assert_eq!(value["state"]["pull_request"], serde_json::Value::Null);
    Ok(())
}

impl Fixture {
    fn home(&self) -> PathBuf {
        self.scope.path().join("home")
    }

    /// Pushes a commit to `branch` from a second clone, so the fixture clone's upstream gains a commit it lacks.
    fn advance_upstream(&self, branch: &str) -> Result<()> {
        let other = self.scope.path().join("upstream-writer");
        if !other.is_dir() {
            self.git(
                self.scope.path(),
                &["clone", "--quiet", "origin.git", "upstream-writer"],
            )?;
        }
        self.git(&other, &["fetch", "--quiet", "origin"])?;
        self.git(
            &other,
            &[
                "switch",
                "--quiet",
                "-C",
                branch,
                &format!("origin/{branch}"),
            ],
        )?;
        fs::write(other.join("elsewhere"), "elsewhere")?;
        self.git(&other, &["add", "elsewhere"])?;
        self.git(&other, &["commit", "--quiet", "-m", "elsewhere"])?;
        self.git(&other, &["push", "--quiet", "origin", branch])?;
        self.git(&self.clone, &["fetch", "--quiet", "origin"])
    }

    /// The CLI, run in the fixture clone with the fixture's home, temporary directory, Git configuration, and `gh`.
    fn cli(&self, arguments: &[&str]) -> Result<Command> {
        let mut paths = vec![gh_directory().path().to_path_buf()];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").context("PATH")?,
        ));
        let mut command = Command::new(env!("CARGO_BIN_EXE_dotfiles-xtask"));
        command
            .current_dir(&self.clone)
            .env("PATH", std::env::join_paths(paths)?)
            .env("HOME", self.home())
            .env("USERPROFILE", self.home())
            .env("TEMP", self.scope.path().join("tmp"))
            .env_remove("CARGO_TARGET_DIR")
            .env("GIT_CONFIG_GLOBAL", &self.config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GH_FIXTURE_LOG", self.scope.path().join("gh.log"))
            .args(["--root", "."])
            .arg("next")
            .args(arguments);
        Ok(command)
    }
}

fn gh_directory() -> &'static tempfile::TempDir {
    static DIRECTORY: OnceLock<tempfile::TempDir> = OnceLock::new();
    DIRECTORY.get_or_init(|| {
        let directory = tempfile::tempdir().unwrap();
        let result = Command::new("rustc")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/gh.rs"))
            .arg("-o")
            .arg(
                directory
                    .path()
                    .join(format!("gh{}", std::env::consts::EXE_SUFFIX)),
            )
            .status()
            .unwrap();
        assert!(result.success());
        directory
    })
}

/// The single JSON line on stdout, after checking that nothing else is printed there.
fn reported(output: &Output) -> Result<Value> {
    let stdout = String::from_utf8(output.stdout.clone())?;
    ensure!(
        stdout.lines().count() == 1,
        "expected one JSON line on stdout, got {stdout:?}; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_str(&stdout)?)
}

#[test]
fn a_rebased_pushed_branch_is_force_pushed_and_upstream_commits_are_pulled_first() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.git(&fixture.clone, &["switch", "--quiet", "-c", "feat/topic"])?;
    fixture.commit("change")?;
    fixture.git(
        &fixture.clone,
        &["push", "--quiet", "-u", "origin", "feat/topic"],
    )?;
    fixture.advance_upstream("feat/topic")?;
    let recommendation = recommend(fixture.next()?);
    assert_eq!(recommendation.action, Step::SyncUpstream);
    assert_eq!(
        (recommendation.state.unpulled, recommendation.state.unpushed),
        (1, 0)
    );
    assert_eq!(
        recommendation.command.as_deref(),
        Some("git merge --ff-only @{upstream}")
    );
    assert_eq!(perform(&fixture.clone, &recommendation)?, None);
    assert_eq!(fixture.step()?, Step::InspectGithub);

    fixture.advance_origin()?;
    let recommendation = recommend(fixture.next()?);
    assert_eq!(recommendation.action, Step::Rebase);
    assert_eq!(
        recommendation.command.as_deref(),
        Some("git rebase origin/main")
    );
    fixture.git(&fixture.clone, &["rebase", "--quiet", "origin/main"])?;
    let recommendation = recommend(fixture.next()?);
    assert_eq!(recommendation.action, Step::ForcePush);
    assert_eq!(
        (recommendation.state.unpushed, recommendation.state.unpulled),
        (3, 2)
    );
    assert_eq!(
        recommendation.command.as_deref(),
        Some("git push --force-with-lease origin feat/topic")
    );
    assert!(recommendation.summary.contains("git range-diff"));
    // The origin takes the rebased branch by fetching it, so the fixture does not depend on a host guard against force-pushes.
    fixture.git(
        &fixture.scope.path().join("origin.git"),
        &[
            "fetch",
            "--quiet",
            "../clone",
            "+refs/heads/feat/topic:refs/heads/feat/topic",
        ],
    )?;
    fixture.git(&fixture.clone, &["fetch", "--quiet", "origin"])?;
    assert_eq!(fixture.step()?, Step::InspectGithub);
    Ok(())
}

#[test]
fn the_coderabbit_pause_file_holds_a_push_whose_pr_is_unknown() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.git(&fixture.clone, &["switch", "--quiet", "-c", "feat/topic"])?;
    fixture.commit("change")?;
    assert!(!fixture.next()?.review_paused);
    assert_eq!(fixture.step()?, Step::Push);
    let pause = fixture.home().join(".local/state/coderabbit-guard/paused");
    fs::create_dir_all(pause.parent().context("pause directory")?)?;
    fs::write(&pause, "")?;
    let observed = fixture.next()?;
    assert!(observed.review_paused);
    assert_eq!(recommend(observed).action, Step::InspectGithub);
    Ok(())
}

#[test]
fn executing_draft_decisions_writes_the_draft_and_leaves_its_reason_to_complete() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.git(&fixture.clone, &["switch", "--quiet", "-c", "feat/topic"])?;
    let skill = fixture.clone.join("dot_agents/skills/alpha");
    fs::create_dir_all(&skill)?;
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: alpha\ndescription: Check alpha.\n---\n# Contract\nCheck it.\n",
    )?;
    let policy = fixture.clone.join("dot_config/skill-ops/policy.json");
    fs::create_dir_all(policy.parent().context("policy directory")?)?;
    fs::write(
        &policy,
        json!({
            "schema": 1, "review_every": 3, "minimum_pair_sessions": 2, "pair_percent": 80,
            "max_entry_words": 100, "requirements": [], "bundles": {},
        })
        .to_string(),
    )?;
    let recommendation = recommend(fixture.next()?);
    assert_eq!(recommendation.action, Step::DraftDecisions);
    let output = perform(&fixture.clone, &recommendation)?.context("decide prints its context")?;
    assert!(
        output.contains("Drafted docs/skills/decisions/alpha.json"),
        "{output}"
    );
    assert!(
        fixture
            .clone
            .join("docs/skills/decisions/alpha.json")
            .is_file()
    );
    let recommendation = recommend(fixture.next()?);
    assert_eq!(recommendation.action, Step::CompleteDecisions);
    assert_eq!(recommendation.state.incomplete_decisions, ["alpha"]);
    assert_eq!(
        recommendation.summary,
        "Write the reason, evidence, and any revisit condition in docs/skills/decisions/alpha.json, then run `just check`"
    );
    assert_eq!(recommendation.command, None);
    Ok(())
}

#[test]
fn execute_refuses_host_steps_it_cannot_complete() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let volume = scope.path().join("unmounted");
    let mut observed = on_github(None, false);
    observed.host = HostStatus::VolumeMissing {
        path: volume.clone(),
    };
    let recommendation = recommend(observed);
    assert_eq!(recommendation.action, Step::MountVolume);
    let refusal = perform(scope.path(), &recommendation)
        .unwrap_err()
        .to_string();
    assert!(
        refusal.starts_with("`mount-volume` is not a local, reversible step"),
        "{refusal}"
    );
    assert!(refusal.contains("then rerun `just next`"), "{refusal}");
    // A temporary directory is created only where the inspection found it missing on a mounted volume.
    let mismatched = Recommendation {
        action: Step::CreateTemp,
        ..recommendation
    };
    let refusal = perform(scope.path(), &mismatched).unwrap_err().to_string();
    assert!(
        refusal.starts_with("`create-temp` is not a local, reversible step"),
        "{refusal}"
    );
    assert!(!volume.exists());
    Ok(())
}

#[test]
fn a_merged_pull_request_names_its_branch_for_retirement() {
    let recommendation = recommend(on_github(Some(("MERGED", false, Checks::Passing)), false));
    assert_eq!(
        recommendation.summary,
        "PR #7 is merged; retire feat/topic and its worktree"
    );
}

#[test]
fn execute_runs_the_step_and_then_reports_the_one_after_it() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.advance_origin()?;
    let output = fixture.cli(&["--offline", "--execute"])?.output()?;
    let stderr = String::from_utf8(output.stderr.clone())?;
    assert!(output.status.success(), "{stderr}");
    assert_eq!(reported(&output)?["action"], "start-branch");
    assert_eq!(
        stderr.lines().collect::<Vec<_>>(),
        [
            "done: Fast-forward main by 1 commits to origin/main",
            "next: main matches origin/main; branch from it for the next issue",
        ]
    );

    fixture.git(&fixture.clone, &["switch", "--quiet", "-c", "feat/topic"])?;
    fixture.commit("change")?;
    let output = fixture.cli(&["--offline"])?.output()?;
    assert!(output.status.success());
    let line = reported(&output)?;
    assert_eq!(line["action"], "push");
    assert_eq!(line["executable"], false);
    let output = fixture.cli(&["--offline", "--execute"])?.output()?;
    let stderr = String::from_utf8(output.stderr.clone())?;
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        stderr.contains("`push` is not a local, reversible step")
            && stderr.contains("(`git push -u origin feat/topic`)"),
        "{stderr}"
    );
    Ok(())
}

#[test]
fn online_inspection_reads_the_pull_request_and_compares_with_its_base() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.git(&fixture.clone, &["switch", "--quiet", "-c", "feat/base"])?;
    fixture.commit("base")?;
    fixture.git(
        &fixture.clone,
        &["push", "--quiet", "-u", "origin", "feat/base"],
    )?;
    fixture.git(&fixture.clone, &["switch", "--quiet", "-c", "feat/topic"])?;
    fixture.commit("change")?;
    fixture.git(
        &fixture.clone,
        &["push", "--quiet", "-u", "origin", "feat/topic"],
    )?;
    let listed = json!([{
        "number": 7, "url": "https://github.com/owner/project/pull/7", "state": "OPEN",
        "isDraft": true, "baseRefName": "feat/base",
        "statusCheckRollup": [{"status": "COMPLETED", "conclusion": "SUCCESS"}],
    }]);
    let output = fixture
        .cli(&[])?
        .env("GH_FIXTURE_LIST", listed.to_string())
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let line = reported(&output)?;
    assert_eq!(line["action"], "mark-ready");
    assert_eq!(line["state"]["base"], "origin/feat/base");
    assert_eq!(line["state"]["ahead_base"], 1);
    assert_eq!(
        line["state"]["pull_request"],
        json!({
            "number": 7, "url": "https://github.com/owner/project/pull/7", "state": "OPEN",
            "draft": true, "base": "feat/base", "checks": "passing",
        })
    );
    let log = fs::read_to_string(fixture.scope.path().join("gh.log"))?;
    assert!(
        log.contains("list\n--head\nfeat/topic\n--state\nall"),
        "{log}"
    );

    let pause = fixture.home().join(".local/state/coderabbit-guard/paused");
    fs::create_dir_all(pause.parent().context("pause directory")?)?;
    fs::write(&pause, "")?;
    let output = fixture
        .cli(&[])?
        .env("GH_FIXTURE_LIST", listed.to_string())
        .output()?;
    assert_eq!(reported(&output)?["action"], "await-review-resume");

    let output = fixture
        .cli(&[])?
        .env(
            "GH_FIXTURE_LIST",
            json!([{"number": 7, "state": "OPEN"}]).to_string(),
        )
        .output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)?.contains("PR without a draft state"));
    Ok(())
}

#[cfg(windows)]
#[test]
fn an_unset_temp_is_reported_before_every_other_step() -> Result<()> {
    let fixture = Fixture::new()?;
    let output = fixture.cli(&["--offline"])?.env_remove("TEMP").output()?;
    assert!(output.status.success());
    let line = reported(&output)?;
    assert_eq!(line["action"], "set-temp");
    assert_eq!(line["state"]["host"]["status"], "temp-unset");
    Ok(())
}

use anyhow::{Context, Result, ensure};
use std::fs;
use std::path::Path;
use std::process::Command;
use xtask::quality::{push_hook_findings, workflow_findings};

#[path = "../../crates/dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

fn repository(path: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(path)).unwrap()
}

const POLICY: &str = "[ruleset.main]\nrequired_status_checks = [\"required\"]\n";
const WORKFLOW: &str = r#"name: ci
on:
  pull_request:
  merge_group:
permissions:
  contents: read
  issues: read
concurrency:
  group: ci-${{ github.event.pull_request.number || github.ref }}
  cancel-in-progress: ${{ github.event_name == 'pull_request' }}
jobs:
  check:
    name: required (${{ matrix.os }})
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    timeout-minutes: 5
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1
        with:
          persist-credentials: false
      - name: Report the checked-out source
        shell: bash
        run: git log -1 --format='commit=%H%ntree=%T'
      - name: Report the native admission boundary
        shell: bash
        run: |
          printf '%s\n' 'Native checks follow the committed publication policy.' 'CI reports the checked-out source and does not rerun or attest native checks.'
  dependencies:
    name: dependencies
    runs-on: ubuntu-latest
    timeout-minutes: 20
    steps:
      - name: Leave the owner's changes to the local gate
        if: github.event_name != 'pull_request' || github.event.pull_request.user.login == github.repository_owner
        shell: bash
        run: |
          printf '%s\n' 'The local pre-push gate checks the owner'"'"'s changes, so this job checks only pull requests from others, such as Renovate updates.'
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1
        if: github.event_name == 'pull_request' && github.event.pull_request.user.login != github.repository_owner
        with:
          persist-credentials: false
      - uses: jdx/mise-action@fc42bcc7ddbf51124c30f95a877a0c5168a3bfde
        if: github.event_name == 'pull_request' && github.event.pull_request.user.login != github.repository_owner
        with:
          version: v2026.10.19
          sha256: 3162760f40b6b5f706c60641e4ef5ee7322bc6c54f72fc26a7111b337aa18d9d
          install_args: rust just
          cache: false
      - name: Check tool locks for every host OS
        if: github.event_name == 'pull_request' && github.event.pull_request.user.login != github.repository_owner
        shell: bash
        run: just tool-locks
      - name: Compile the workspace and check the toolchain pins
        if: github.event_name == 'pull_request' && github.event.pull_request.user.login != github.repository_owner
        shell: bash
        run: just dependency-gate
  required:
    name: required
    needs: [check, dependencies]
    if: always()
    runs-on: ubuntu-latest
    timeout-minutes: 5
    steps:
      - name: Refuse a missing, skipped, failed, or cancelled job
        shell: bash
        env:
          RESULTS: ${{ join(needs.*.result, ' ') }}
        run: |
          [[ "$RESULTS" =~ ^success( success)*$ ]]
"#;

#[test]
fn pre_push_uses_the_canonical_candidate_publication_gate() {
    let hook = repository("lefthook.yml");
    assert_eq!(push_hook_findings(&hook).unwrap(), Vec::<String>::new());
    assert_eq!(dotfiles::hosts::DEFAULT_GATE, "just push-check");
    for candidate in [
        hook.replace("hosts gate {1} {2}", "check"),
        hook.replace("use_stdin: true", "use_stdin: false"),
        format!("{hook}\n  scripts:\n    duplicate:\n      runner: bash\n      run: just check\n"),
    ] {
        assert_ne!(
            push_hook_findings(&candidate).unwrap(),
            Vec::<String>::new()
        );
    }
}

#[test]
fn lightweight_ci_reports_source_and_keeps_all_native_families_available() {
    for workflow in [repository(".github/workflows/ci.yml"), WORKFLOW.into()] {
        assert_eq!(
            workflow_findings(&workflow, POLICY).unwrap(),
            Vec::<String>::new()
        );
        assert_eq!(
            dotfiles::hosts::required(&[("ci.yml".into(), workflow)]).unwrap(),
            [true; 3]
        );
    }
}

#[test]
fn lightweight_ci_refuses_missing_reports_heavy_work_and_suppressed_failures() {
    for (from, to) in [
        ("  merge_group:\n", ""),
        ("  pull_request:\n", ""),
        ("contents: read", "contents: write"),
        ("issues: read", "issues: write"),
        ("  issues: read\n", "  issues: read\n  id-token: write\n"),
        (
            "git log -1 --format='commit=%H%ntree=%T'",
            "git log -1 --format=%H",
        ),
        (
            "git log -1 --format='commit=%H%ntree=%T'",
            "mise x -- just check",
        ),
        (
            "git log -1 --format='commit=%H%ntree=%T'",
            "git log -1 --format='commit=%H%ntree=%T' || true",
        ),
        (
            "does not rerun or attest native checks",
            "verifies native checks",
        ),
        ("persist-credentials: false", "persist-credentials: true"),
        (
            "checkout@3d3c42e5aac5ba805825da76410c181273ba90b1",
            "checkout@main",
        ),
        ("fail-fast: false", "fail-fast: true"),
        (
            "ubuntu-latest, macos-latest, windows-latest",
            "ubuntu-latest, macos-latest",
        ),
        (
            "ubuntu-latest, macos-latest, windows-latest",
            "ubuntu-latest, macos-latest, macos-latest",
        ),
        ("runs-on: ${{ matrix.os }}", "runs-on: ubuntu-latest"),
        (
            "os: [ubuntu-latest, macos-latest, windows-latest]",
            "os: [ubuntu-latest, macos-latest, windows-latest]\n        exclude: [{os: windows-latest}]",
        ),
        ("needs: [check, dependencies]", "needs: []"),
        ("needs: [check, dependencies]", "needs: [check]"),
        (
            "&& github.event.pull_request.user.login != github.repository_owner\n        with:\n          persist",
            "\n        with:\n          persist",
        ),
        (
            "jdx/mise-action@fc42bcc7ddbf51124c30f95a877a0c5168a3bfde",
            "jdx/mise-action@v5",
        ),
        (
            "          sha256: 3162760f40b6b5f706c60641e4ef5ee7322bc6c54f72fc26a7111b337aa18d9d\n",
            "",
        ),
        ("cache: false", "cache: true"),
        (
            "run: just dependency-gate",
            "run: cargo run --locked --package xtask -- check",
        ),
        ("if: always()", "if: success()"),
        ("( success)*$ ]]", "( success)*$ ]] || true"),
        ("[[ \"$RESULTS\" =~ ^success( success)*$ ]]", "true"),
        (
            "    timeout-minutes: 5",
            "    continue-on-error: true\n    timeout-minutes: 5",
        ),
        (
            "        shell: bash",
            "        continue-on-error: true\n        shell: bash",
        ),
        (
            "      - name: Report the checked-out source",
            "      - run: mise install\n      - name: Report the checked-out source",
        ),
        (
            "      - name: Report the checked-out source",
            "      - uses: actions/cache@55cc8345863c7cc4c66a329aec7e433d2d1c52a9\n      - name: Report the checked-out source",
        ),
        (
            "      - name: Report the checked-out source",
            "      - name: Report the checked-out source\n        if: false",
        ),
        ("    name: required\n", "    name: other\n"),
        (
            "  required:\n",
            "  extra:\n    runs-on: ubuntu-latest\n    steps:\n      - run: just proofs\n  required:\n",
        ),
    ] {
        let changed = WORKFLOW.replacen(from, to, 1);
        assert_ne!(changed, WORKFLOW, "missing fixture input: {from}");
        assert_ne!(
            workflow_findings(&changed, POLICY).unwrap(),
            Vec::<String>::new(),
            "{from} -> {to}"
        );
    }
}

#[test]
fn lightweight_ci_refuses_lock_bypasses_and_budget_or_order_changes() {
    for changed in [
        WORKFLOW.replace("run: just tool-locks", "run: true"),
        WORKFLOW.replace(
            "      - name: Check tool locks for every host OS\n        if: github.event_name == 'pull_request' && github.event.pull_request.user.login != github.repository_owner\n",
            "      - name: Check tool locks for every host OS\n",
        ),
        WORKFLOW.replacen("    timeout-minutes: 5", "    timeout-minutes: 20", 1),
        WORKFLOW
            .replace("run: just dependency-gate", "run: just tool-locks")
            .replacen("run: just tool-locks", "run: just dependency-gate", 1),
    ] {
        assert_ne!(changed, WORKFLOW);
        assert_ne!(
            workflow_findings(&changed, POLICY).unwrap(),
            Vec::<String>::new()
        );
    }
}

fn workflow_bash(global: &Path) -> Result<Command> {
    if !cfg!(windows) {
        return Ok(platform::tool::Tool::Bash.command());
    }
    let git = fixture_git::command("git", global)
        .arg("--exec-path")
        .output()?;
    ensure!(git.status.success(), "git --exec-path failed");
    let directory = String::from_utf8(git.stdout)?;
    let shell = xtask::quality::git_for_windows_shell(Path::new(directory.trim()))
        .context("unexpected Git for Windows layout")?
        .with_file_name("bash.exe");
    ensure!(shell.is_file(), "native Git Bash is missing");
    Ok(platform::process::command(shell))
}

#[test]
fn final_status_accepts_only_a_successful_matrix() -> Result<()> {
    use yaml_rust2::YamlLoader;
    let workflow = repository(".github/workflows/ci.yml");
    let documents = YamlLoader::load_from_str(&workflow).unwrap();
    let guard = documents[0]["jobs"]["required"]["steps"][0]["run"]
        .as_str()
        .unwrap();
    assert_eq!(guard.trim(), "[[ \"$RESULTS\" =~ ^success( success)*$ ]]");
    let fixture = tempfile::tempdir()?;
    for (result, passes) in [
        ("success", true),
        ("success success", true),
        ("failure", false),
        ("cancelled", false),
        ("skipped", false),
        ("", false),
        ("success failure", false),
        ("skipped success", false),
    ] {
        let status = workflow_bash(&fixture.path().join("gitconfig"))?
            .env("RESULTS", result)
            .args(["-e", "-c", guard])
            .status()
            .unwrap();
        assert_eq!(status.success(), passes, "{result}");
    }
    Ok(())
}

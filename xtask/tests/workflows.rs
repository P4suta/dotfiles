use anyhow::{Context, Result, bail, ensure};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use xtask::quality::{push_hook_findings, workflow_findings};
use yaml_rust2::YamlLoader;

#[path = "../../crates/dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

fn repository(path: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(path)).unwrap()
}

const POLICY: &str = "[ruleset.main]\nrequired_status_checks = [\"required\"]\n";
const MISE_PINS: &str = "home/.chezmoidata/versions.toml";
const CONTROL_VERSION: &str = "0.0.1";
const PRODUCER_SYSTEM_PATH: &str = "/usr/bin:/bin";

const WORKFLOW: &str = "on:
  pull_request:
  merge_group:
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - run: mise x -- just check
  proofs:
    runs-on: ubuntu-latest
    steps:
      - run: mise x -- just proofs-ci
  required:
    name: required
    needs: [check, proofs]
    if: always()
    runs-on: ubuntu-latest
    steps:
      - env:
          RESULTS: ${{ join(needs.*.result, ' ') }}
        run: |
          for result in $RESULTS; do
            case \"$result\" in
              success) ;;
              *) exit 1 ;;
            esac
          done
";

#[test]
fn pre_push_cannot_diverge_from_the_verified_hosts_gate() {
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
fn the_ci_workflow_provides_what_the_ruleset_requires() {
    assert_eq!(
        workflow_findings(
            &repository(".github/workflows/ci.yml"),
            &repository("policy/github.toml")
        )
        .unwrap(),
        Vec::<String>::new()
    );
    assert_eq!(
        workflow_findings(WORKFLOW, POLICY).unwrap(),
        Vec::<String>::new()
    );
}

#[test]
fn ci_uses_the_explicit_proof_entry_for_the_paused_mutation_campaign() -> Result<()> {
    for text in [repository(".github/workflows/ci.yml"), WORKFLOW.to_owned()] {
        let workflow = YamlLoader::load_from_str(&text)?;
        let [workflow] = workflow.as_slice() else {
            bail!("CI must contain one workflow document")
        };
        let steps = workflow["jobs"]["proofs"]["steps"]
            .as_vec()
            .context("the proofs job has no steps")?;
        assert!(
            steps.iter().any(|step| matches!(
                step["run"].as_str(),
                Some(xtask::quality::CI_PROOFS_ENTRY | xtask::quality::CI_PHASE_ENTRY)
            )),
            "the proofs job must call its explicit CI entry"
        );
    }
    Ok(())
}

#[test]
fn a_hosted_partition_cannot_omit_duplicate_or_skip_required_work() {
    let current = repository(".github/workflows/ci.yml");
    let (prefix, proofs) = current.split_once("\n  proofs:\n").unwrap();
    let (proofs, suffix) = proofs.split_once("\n  minimum-git:\n").unwrap();
    for (from, to, expected) in [
        (
            "phase: [kani, runtime, metrics-models]",
            "phase: [kani, runtime]",
            "every known partition exactly once",
        ),
        (
            "phase: [kani, runtime, metrics-models]",
            "phase: [kani, runtime, runtime]",
            "every known partition exactly once",
        ),
        (
            "phase: [kani, runtime, metrics-models]",
            "phase: [kani, runtime, metrics-models, unknown]",
            "every known partition exactly once",
        ),
        (
            "fail-fast: false",
            "fail-fast: true",
            "collect every partition",
        ),
        ("max-parallel: 2", "max-parallel: 3", "at most two jobs"),
        (
            "PHASE: ${{ matrix.phase }}",
            "PHASE: kani",
            "execute its selected mandatory partition",
        ),
        (
            xtask::quality::CI_PHASE_ENTRY,
            "mise x -- just proofs-ci --phase kani",
            "execute its selected mandatory partition",
        ),
        (
            "phase: [kani, runtime, metrics-models]",
            "phase: [kani, runtime, metrics-models]\n        exclude: [{phase: runtime}]",
            "without include or exclude overrides",
        ),
    ] {
        assert_eq!(proofs.matches(from).count(), 1, "fixture subject: {from}");
        let changed = format!(
            "{prefix}\n  proofs:\n{}\n  minimum-git:\n{suffix}",
            proofs.replacen(from, to, 1)
        );
        let findings = workflow_findings(&changed, POLICY).unwrap();
        assert!(
            findings.iter().any(|finding| finding.contains(expected)),
            "missing {expected}: {findings:?}"
        );
    }
}

#[test]
fn a_workflow_the_ruleset_cannot_rely_on_is_refused() {
    for (from, to, expected) in [
        ("  merge_group:\n", "", "does not run on merge_group"),
        (
            "[check, proofs]",
            "[check]",
            "does not wait for the proofs job",
        ),
        (
            "if: always()",
            "if: success()",
            "must run with `if: always()`",
        ),
        (
            "success) ;;",
            "success | skipped) ;;",
            "must refuse every result other than success",
        ),
        (
            "    runs-on: ubuntu-latest",
            "    continue-on-error: true\n    runs-on: ubuntu-latest",
            "suppresses failure",
        ),
        (
            "just proofs-ci",
            "cargo run -- proofs-ci",
            "calls `cargo run`",
        ),
        (
            "  proofs:\n    runs-on: ubuntu-latest",
            "  proofs:\n    runs-on: macos-latest",
            "the proofs job must run on Linux",
        ),
        (
            "name: required\n",
            "name: aggregate\n",
            "which no job of ci.yml reports",
        ),
    ] {
        let workflow = WORKFLOW.replacen(from, to, 1);
        let found = workflow_findings(&workflow, POLICY).unwrap();
        assert!(
            found.iter().any(|line| line.contains(expected)),
            "{expected} missing from {found:?}"
        );
    }
}

fn pinned_mise(text: &str) -> Result<String> {
    model::data::parse(MISE_PINS, text)
        .map_err(anyhow::Error::msg)?
        .mise
        .and_then(|release| release.version)
        .context("the source data must declare mise.version")
}

fn producer_bash(global: &Path) -> Result<Command> {
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

fn produced_mise(script: &str, pins: &str) -> Result<String> {
    let fixture = tempfile::tempdir()?;
    let source = fixture.path().join(MISE_PINS);
    fs::create_dir_all(source.parent().context("pin source parent is missing")?)?;
    fs::write(source, pins)?;
    let mut command = producer_bash(&fixture.path().join("gitconfig"))?;
    let path = if cfg!(windows) {
        std::env::join_paths([Path::new(command.get_program())
            .parent()
            .context("native Bash directory is missing")?])?
    } else {
        PRODUCER_SYSTEM_PATH.into()
    };
    command
        .env_clear()
        .env("PATH", path)
        .env("HOME", fixture.path())
        .env("USERPROFILE", fixture.path())
        .env("TMPDIR", fixture.path())
        .env("TEMP", fixture.path())
        .env("TMP", fixture.path())
        .env("GITHUB_OUTPUT", "mise output")
        .current_dir(fixture.path())
        .args([
            "--noprofile",
            "--norc",
            "-e",
            "-o",
            "pipefail",
            "-c",
            script,
        ])
        .stdin(Stdio::null());
    if cfg!(windows) {
        command.env(
            "SystemRoot",
            std::env::var_os("SystemRoot").context("Windows system root is missing")?,
        );
    }
    let output = command.output()?;
    ensure!(
        output.status.success(),
        "CI mise producer failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::read_to_string(fixture.path().join("mise output")).context("read CI producer output")
}

#[test]
fn every_ci_mise_producer_executes_against_the_typed_release_pin() -> Result<()> {
    // Native Bash executes the CI source.
    // GitHub expression evaluation and WSL installation remain separate boundaries.
    let workflow = YamlLoader::load_from_str(&repository(".github/workflows/ci.yml"))?;
    let [workflow] = workflow.as_slice() else {
        bail!("CI must contain one workflow document")
    };
    let mut producers = BTreeMap::new();
    for (name, job) in workflow["jobs"].as_hash().context("CI jobs are missing")? {
        for step in job["steps"].as_vec().context("CI job steps are missing")? {
            if step["id"].as_str() != Some("mise") {
                continue;
            }
            ensure!(
                step["shell"].as_str() == Some("bash"),
                "mise producer requires bash"
            );
            let name = name.as_str().context("CI job name must be a string")?;
            let script = step["run"]
                .as_str()
                .context("mise producer has no run block")?;
            ensure!(
                producers.insert(name, script).is_none(),
                "duplicate mise producer"
            );
        }
    }
    assert_eq!(
        producers.keys().copied().collect::<Vec<_>>(),
        ["check", "classify", "minimum-git", "proofs", "rehearsal"]
    );
    let pins = repository(MISE_PINS);
    ensure!(
        pinned_mise(&pins)? != CONTROL_VERSION,
        "control must change the actual pin"
    );
    let mut changed: toml::Table = toml::from_str(&pins)?;
    changed
        .get_mut("mise")
        .and_then(toml::Value::as_table_mut)
        .context("mise source table is missing")?
        .insert(
            "version".into(),
            toml::Value::String(CONTROL_VERSION.into()),
        );
    let changed = toml::to_string(&changed)?;
    for (job, script) in producers {
        for source in [&pins, &changed] {
            let expected = format!("version={}\n", pinned_mise(source)?);
            assert_eq!(produced_mise(script, source)?, expected, "{job}");
        }
    }
    Ok(())
}

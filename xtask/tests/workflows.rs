use std::fs;
use std::path::Path;
use xtask::quality::{push_hook_findings, workflow_findings};

fn repository(path: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(path)).unwrap()
}

const POLICY: &str = "[ruleset.main]\nrequired_status_checks = [\"required\"]\n";

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
      - run: mise x -- just proofs
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
        ("just proofs", "cargo run -- proofs", "calls `cargo run`"),
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

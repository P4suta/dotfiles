use anyhow::{Context, Result, bail, ensure};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use xtask::checks::Checks;

#[path = "../../crates/dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

use fixture_git::fixture_process;

#[path = "support/incident_fixture.rs"]
mod incident_fixture;

const COMMENT_CASES: [(&str, usize, &str, &str); 3] = [
    (
        "xtask/src/quality.rs",
        75,
        "Returns an error when the hook configuration cannot be parsed.",
        "Returns an error when parsing the hook configuration fails.",
    ),
    (
        "xtask/src/quality.rs",
        246,
        "Returns every workspace whose lockfile is missing or needs an update.",
        "Reports every workspace with a missing or stale lockfile.",
    ),
    (
        "crates/dotfiles/src/hosts.rs",
        558,
        "Returns an error when the committed tree cannot be read or parsed.",
        "Returns an error when reading or parsing the committed tree fails.",
    ),
];

const GATE_COMMENT_PATH: &str = "crates/rules/src/hosts_rules.rs";
const GATE_COMMENT_CASES: [(&str, &str); 3] = [
    (
        "Allocate the next execution generation, refusing an exhausted counter.",
        "Assign the next execution generation, refusing an exhausted counter.",
    ),
    (
        "Authorize only a candidate receipt matching the latest actual success.",
        "Accept a candidate receipt only at the highest successful execution generation.",
    ),
    (
        "A latest failure refuses the candidate, and missing evidence stays missing.",
        "A failure at that generation refuses the candidate, and missing evidence stays missing.",
    ),
];

fn write_comment_fixture(root: &Path, corrected: bool) -> Result<()> {
    let mut files = BTreeMap::<&str, Vec<String>>::new();
    for (path, line, bad, good) in COMMENT_CASES {
        let lines = files.entry(path).or_default();
        lines.resize(line - 1, String::new());
        lines.push(format!("/// {}", if corrected { good } else { bad }));
        lines.push(format!("pub fn fixture_{line}() {{}}"));
    }
    for (path, lines) in files {
        let target = root.join(path);
        fs::create_dir_all(target.parent().context("fixture file has a parent")?)?;
        fs::write(target, format!("{}\n", lines.join("\n")))?;
    }
    Ok(())
}

fn write_gate_comment_fixture(root: &Path, corrected: bool) -> Result<()> {
    write_comment_fixture(root, corrected)?;
    let mut lines = Vec::new();
    for (index, (bad, good)) in GATE_COMMENT_CASES.into_iter().enumerate() {
        lines.push(format!("/// {}", if corrected { good } else { bad }));
        lines.push(format!("pub fn fixture_{index}() {{}}"));
    }
    let target = root.join(GATE_COMMENT_PATH);
    fs::create_dir_all(target.parent().context("fixture file has a parent")?)?;
    fs::write(target, format!("{}\n", lines.join("\n")))?;
    Ok(())
}

fn copy_prose_bundle(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let destination = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_prose_bundle(&entry.path(), &destination)?;
        } else {
            fs::copy(entry.path(), destination)?;
        }
    }
    Ok(())
}

fn prose_gate(root: &Path) -> Result<std::process::Output> {
    Ok(
        fixture_git::command(env!("CARGO_BIN_EXE_xtask"), &root.join("global.gitconfig"))
            .arg("--root")
            .arg(root)
            .args(["check", "--gates", "prose"])
            .env("DOTFILES_EXPECTED_FAILURE", "")
            .output()?,
    )
}

fn assert_gate_comment_findings(stderr: &str) {
    assert!(
        stderr.contains("prose.comment-style: the Comments prose check failed"),
        "{stderr}"
    );
    for (path, line, rule) in [
        (GATE_COMMENT_PATH, 1, "write-good.TooWordy"),
        (GATE_COMMENT_PATH, 3, "write-good.TooWordy"),
        (GATE_COMMENT_PATH, 3, "Google.Timeless"),
        (GATE_COMMENT_PATH, 5, "Google.Timeless"),
        (
            COMMENT_CASES[0].0,
            COMMENT_CASES[0].1,
            "Microsoft.Contractions",
        ),
        (COMMENT_CASES[1].0, COMMENT_CASES[1].1, "write-good.E-Prime"),
        (
            COMMENT_CASES[2].0,
            COMMENT_CASES[2].1,
            "Microsoft.Contractions",
        ),
    ] {
        assert!(
            stderr
                .lines()
                .any(|entry| entry.starts_with(&format!("{path}:{line}:")) && entry.contains(rule)),
            "{stderr}"
        );
    }
}

fn assert_gate_comment_incident(root: &Path) -> Result<()> {
    let recorded = skill_ops::incidents(&root.join("state"))?;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].rule, "tool.failure");
    assert!(recorded[0].evidence.iter().any(|evidence| {
        evidence.contains("prose.comment-style")
            && evidence.contains("write-good.TooWordy")
            && evidence.contains("Google.Timeless")
    }));
    Ok(())
}

#[test]
fn the_prose_gate_checks_original_comments_and_accepts_their_corrections() -> Result<()> {
    let scope = incident_fixture::scope()?;
    let root = scope.path();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("xtask has a repository parent")?;
    copy_prose_bundle(
        &source.join(prose::SOURCE_CONFIG),
        &root.join(prose::SOURCE_CONFIG),
    )?;
    write_gate_comment_fixture(root, false)?;
    fs::create_dir(root.join("policy"))?;
    fs::write(root.join("policy/prose.toml"), "exempt = []\n")?;
    fs::write(root.join(".ocomment.toml"), "version = 1\noverrides = []\n")?;
    let global = root.join("global.gitconfig");
    fs::write(&global, "")?;
    for arguments in [
        vec!["init", "--quiet"],
        vec![
            "add",
            "xtask/src",
            "crates/rules/src",
            "crates/dotfiles/src",
            "policy",
            ".ocomment.toml",
        ],
    ] {
        let output = fixture_git::command("git", &global)
            .current_dir(root)
            .args(arguments)
            .output()?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let refused = prose_gate(root)?;
    assert!(!refused.status.success());
    let stdout = String::from_utf8(refused.stdout)?;
    let stderr = String::from_utf8(refused.stderr)?;
    assert!(stdout.contains("Checking prose.comment-style"), "{stdout}");
    assert_gate_comment_findings(&stderr);
    assert_gate_comment_incident(root)?;
    write_gate_comment_fixture(root, true)?;
    let corrected = prose_gate(root)?;
    assert!(
        !corrected.status.success(),
        "unrelated fixture stages still fail"
    );
    let stdout = String::from_utf8(corrected.stdout)?;
    let stderr = String::from_utf8(corrected.stderr)?;
    assert!(
        stdout.contains("Validated repository Comments prose against policy/prose.toml"),
        "{stdout}"
    );
    let (_, summary) = stderr
        .rsplit_once("selected stages failed:")
        .context("unrelated fixture failures")?;
    assert!(summary.contains("prose.adr:"), "{stderr}");
    assert!(!summary.contains("prose.comment-style:"), "{stderr}");
    incident_fixture::preserve(scope);
    Ok(())
}

#[test]
fn original_comment_style_failures_are_reported_and_corrected_comments_pass() -> Result<()> {
    if incident_fixture::run(
        "original_comment_style_failures_are_reported_and_corrected_comments_pass",
    )? {
        return Ok(());
    }
    let scope = tempfile::tempdir()?;
    let root = scope.path();
    write_comment_fixture(root, false)?;
    fs::create_dir(root.join("policy"))?;
    fs::write(root.join("policy/prose.toml"), "exempt = []\n")?;
    let global = root.join("global.gitconfig");
    fs::write(&global, "")?;
    for arguments in [vec!["init", "--quiet"], vec!["add", "."]] {
        let output = fixture_git::command("git", &global)
            .current_dir(root)
            .args(arguments)
            .output()?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("xtask has a repository parent")?;
    let bundle = prose::Bundle::source(source)?;
    let findings = prose::findings(root, prose::Scope::Comments, &bundle)?;
    let actual: BTreeSet<_> = findings
        .iter()
        .map(|finding| (finding.path.as_str(), finding.line, finding.rule.as_str()))
        .collect();
    let mut expected = BTreeSet::new();
    for (path, line, bad, _) in COMMENT_CASES {
        let rules: &[&str] = if bad.contains("cannot") {
            &[
                "Google.Contractions",
                "Microsoft.Contractions",
                "Google.Passive",
                "Microsoft.Passive",
                "write-good.E-Prime",
                "write-good.Passive",
            ]
        } else {
            &["write-good.E-Prime"]
        };
        for rule in rules {
            expected.insert((path, line, *rule));
        }
    }
    assert_eq!(actual, expected, "{findings:?}");
    assert_eq!(findings.len(), expected.len());
    let mut checks = Checks::default();
    checks.run("prose.comments", || {
        prose::repository(root, prose::Scope::Comments, &bundle)
    });
    let failure = checks
        .finish()
        .expect_err("the original comments violate the pinned style rules");
    assert!(
        failure
            .to_string()
            .contains("prose.comments: the Comments prose check failed")
    );
    xtask::report(&failure);
    let state =
        std::env::var_os("SKILL_OPS_STATE").context("the child fixture owns its incident state")?;
    let recorded = skill_ops::incidents(Path::new(&state))?;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].rule, "tool.failure");
    assert!(
        recorded[0]
            .evidence
            .iter()
            .any(|evidence| evidence.contains("Microsoft.Contractions")
                && evidence.contains("write-good.E-Prime"))
    );
    write_comment_fixture(root, true)?;
    assert_eq!(prose::findings(root, prose::Scope::Comments, &bundle)?, []);
    let mut checks = Checks::default();
    checks.run("prose.comments", || {
        prose::repository(root, prose::Scope::Comments, &bundle)
    });
    checks.finish()?;
    assert_eq!(skill_ops::incidents(Path::new(&state))?, recorded);
    Ok(())
}

#[test]
fn a_failure_cannot_skip_later_operations_or_hide_their_failures() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let marker = scope.path().join("later-stage");
    let mut checks = Checks::default();
    checks.run("first", || -> Result<()> { bail!("first fixture defect") });
    checks.run("middle", || Ok(fs::write(&marker, "executed")?));
    checks.run("last", || -> Result<()> { bail!("last fixture defect") });
    assert_eq!(fs::read_to_string(marker)?, "executed");
    assert_eq!(
        checks.finish().unwrap_err().to_string(),
        "2 selected stages failed:\nfirst: first fixture defect\nlast: last fixture defect"
    );
    Ok(())
}

#[test]
fn empty_and_successful_selections_succeed() -> Result<()> {
    Checks::default().finish()?;
    let mut checks = Checks::default();
    checks.run("successful", || Ok(()));
    checks.finish()
}

#[test]
fn the_check_command_reports_multiple_selected_gate_failures_in_one_run() -> Result<()> {
    let scope = match std::env::var_os(skill_ops::INCIDENT_EVIDENCE_ROOT) {
        Some(directory) => tempfile::tempdir_in(directory)?,
        None => tempfile::tempdir()?,
    };
    let output = fixture_process::command(env!("CARGO_BIN_EXE_xtask"), scope.path())
        .arg("--root")
        .arg(scope.path())
        .args(["check", "--gates", "prose,skills"])
        .env("DOTFILES_EXPECTED_FAILURE", "")
        .output()?;
    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    let stages: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("Checking "))
        .collect();
    assert_eq!(
        stages,
        [
            "Checking prose.lint",
            "Checking prose.adr",
            "Checking prose.editorconfig",
            "Checking prose.comments",
            "Checking prose.comment-style",
            "Checking prose.instructions",
            "Checking prose.line-endings",
            "Checking prose.recipes",
            "Checking prose.secrets",
            "Checking prose.documents",
            "Checking prose.repository",
            "Checking skills.catalog",
            "Checking skills.inventory",
            "Checking skills.validation",
            "Checking skills.adapters",
        ]
    );
    let stderr = String::from_utf8(output.stderr)?;
    eprint!("{stderr}");
    let (_, summary) = stderr
        .rsplit_once("selected stages failed:")
        .expect("the final failure summary");
    for failure in [
        "prose.lint:",
        "prose.adr:",
        "skills.inventory:",
        "skills.adapters:",
    ] {
        assert!(summary.contains(failure), "{stderr}");
    }
    if std::env::var_os(skill_ops::INCIDENT_EVIDENCE_ROOT).is_some() {
        let _ = scope.keep();
    }
    Ok(())
}

use anyhow::{Context, Result, bail, ensure};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use xtask::checks::Checks;

#[path = "../../crates/platform/tests/support/fixture_process.rs"]
mod fixture_process;

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
    for arguments in [vec!["init", "--quiet"], vec!["add", "."]] {
        let output = platform::tool::Tool::Git
            .command()
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

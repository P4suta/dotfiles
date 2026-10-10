use anyhow::{Result, bail};
use std::fs;
use xtask::checks::Checks;

#[path = "../../crates/platform/tests/support/fixture_process.rs"]
mod fixture_process;

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
    let scope = tempfile::tempdir()?;
    let output = fixture_process::command(env!("CARGO_BIN_EXE_xtask"), scope.path())
        .arg("--root")
        .arg(scope.path())
        .args(["check", "--gates", "prose,skills"])
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
    Ok(())
}

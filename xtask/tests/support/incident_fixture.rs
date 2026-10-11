use anyhow::{Result, ensure};

const GATE_FIXTURE: &str = "DOTFILES_GATE_FIXTURE";

pub fn scope() -> Result<tempfile::TempDir> {
    match std::env::var_os(skill_ops::INCIDENT_EVIDENCE_ROOT) {
        Some(directory) => Ok(tempfile::tempdir_in(directory)?),
        None => Ok(tempfile::tempdir()?),
    }
}

pub fn preserve(scope: tempfile::TempDir) {
    if std::env::var_os(skill_ops::INCIDENT_EVIDENCE_ROOT).is_some() {
        let _ = scope.keep();
    }
}

pub fn run(test: &str) -> Result<bool> {
    if std::env::var(GATE_FIXTURE).is_ok_and(|value| value == test) {
        return Ok(false);
    }
    let scope = scope()?;
    let output = platform::process::command(std::env::current_exe()?)
        .args(["--exact", test, "--nocapture"])
        .env(GATE_FIXTURE, test)
        .env("SKILL_OPS_STATE", scope.path().join("state"))
        .env("DOTFILES_EXPECTED_FAILURE", "")
        .output()?;
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    ensure!(output.status.success(), "owned gate fixture failed");
    preserve(scope);
    Ok(true)
}

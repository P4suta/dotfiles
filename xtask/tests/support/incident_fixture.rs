use anyhow::{Result, ensure};
use std::path::Path;
use std::process::Command;

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

pub fn enable(command: &mut Command, scope: &Path) -> Result<()> {
    let home = scope.join("home");
    let marker = home.join(platform::incidents::EXPERIMENTAL);
    std::fs::create_dir_all(marker.parent().unwrap())?;
    std::fs::write(marker, skill_ops::incident_rules::EXPERIMENTAL_OPT_IN)?;
    if let Some(original) = platform::home::home() {
        for (name, directory) in [("CARGO_HOME", ".cargo"), ("RUSTUP_HOME", ".rustup")] {
            command.env(
                name,
                std::env::var_os(name).unwrap_or_else(|| original.join(directory).into()),
            );
        }
    }
    command.env("HOME", &home).env("USERPROFILE", &home);
    Ok(())
}

pub fn run(test: &str) -> Result<bool> {
    if std::env::var(GATE_FIXTURE).is_ok_and(|value| value == test) {
        return Ok(false);
    }
    let scope = scope()?;
    let mut command = platform::process::command(std::env::current_exe()?);
    enable(&mut command, scope.path())?;
    let output = command
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

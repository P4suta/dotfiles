use anyhow::Result;
use platform::freshness::{BUILD_IDENTITY_ARGUMENT, Embedded};

#[path = "../../platform/tests/support/fixture_process.rs"]
mod fixture_process;

#[test]
fn compiled_identity_does_not_open_maintenance_state() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let absent = scope.path().join("absent-home");
    let output = fixture_process::command(env!("CARGO_BIN_EXE_skill-ops"), scope.path())
        .arg(BUILD_IDENTITY_ARGUMENT)
        .env("HOME", &absent)
        .env("USERPROFILE", &absent)
        .env("SKILL_OPS_TREE", absent.join("skills"))
        .env("SKILL_OPS_POLICY", absent.join("policy.json"))
        .env("SKILL_OPS_STATE", absent.join("state"))
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let identity: Embedded = serde_json::from_slice(&output.stdout)?;
    assert_eq!(identity, platform::freshness::embedded());
    assert!(!absent.exists());
    Ok(())
}

use anyhow::Result;
use platform::freshness::{BUILD_IDENTITY_ARGUMENT, Embedded};

#[path = "../../platform/tests/support/fixture_process.rs"]
mod fixture_process;

#[test]
fn compiled_identity_does_not_open_review_configuration_or_pools() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let absent = scope.path().join("absent-home");
    let output = fixture_process::command(env!("CARGO_BIN_EXE_coderabbit"), scope.path())
        .arg(BUILD_IDENTITY_ARGUMENT)
        .env("HOME", &absent)
        .env("USERPROFILE", &absent)
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

#[path = "../../crates/platform/tests/support/fixture_process.rs"]
mod fixture_process;

#[test]
fn native_sync_plan_refuses_write_mode_before_authentication() {
    let scope = tempfile::tempdir().expect("fixture directory");
    let output = fixture_process::command(env!("CARGO_BIN_EXE_xtask"), scope.path())
        .args(["doppler-sync", "--phase", "plan", "--apply", "--manifest"])
        .arg(scope.path().join("absent.toml"))
        .arg("--config-dir")
        .arg(scope.path())
        .arg("--scope")
        .arg(scope.path())
        .output()
        .expect("start sync command");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("read-only phases refuse --apply"),
        "{stderr}"
    );
}

#[test]
fn native_sync_prepare_requires_write_mode_before_authentication() {
    let scope = tempfile::tempdir().expect("fixture directory");
    let output = fixture_process::command(env!("CARGO_BIN_EXE_xtask"), scope.path())
        .args(["doppler-sync", "--phase", "prepare", "--manifest"])
        .arg(scope.path().join("absent.toml"))
        .arg("--config-dir")
        .arg(scope.path())
        .arg("--scope")
        .arg(scope.path())
        .output()
        .expect("start sync command");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("prepare requires --apply"), "{stderr}");
}

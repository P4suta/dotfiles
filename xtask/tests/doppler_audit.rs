#[path = "../../crates/platform/tests/support/fixture_process.rs"]
mod fixture_process;

#[test]
fn delivery_audit_refuses_apply_before_authentication() {
    let scope = tempfile::tempdir().expect("fixture directory");
    let output = fixture_process::command(env!("CARGO_BIN_EXE_xtask"), scope.path())
        .args([
            "doppler-organize",
            "--workplace",
            "fixture",
            "--phase",
            "audit",
            "--apply",
            "--config-dir",
        ])
        .arg(scope.path().join("absent-credentials"))
        .arg("--scope")
        .arg(scope.path())
        .arg("--checkouts")
        .arg(scope.path())
        .arg("--report")
        .arg(scope.path().join("absent-report.json"))
        .output()
        .expect("start audit command");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("read-only phases refuse it"), "{stderr}");
}

#[test]
fn unused_sync_retirement_requires_an_exact_target_before_authentication() {
    let scope = tempfile::tempdir().expect("fixture directory");
    let output = fixture_process::command(env!("CARGO_BIN_EXE_xtask"), scope.path())
        .args([
            "doppler-organize",
            "--workplace",
            "fixture",
            "--phase",
            "retire-unused-sync",
            "--config-dir",
        ])
        .arg(scope.path().join("absent-credentials"))
        .arg("--scope")
        .arg(scope.path())
        .arg("--checkouts")
        .arg(scope.path())
        .arg("--report")
        .arg(scope.path().join("absent-report.json"))
        .output()
        .expect("start retirement command");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("one consumer and sync are required"),
        "{stderr}"
    );
}

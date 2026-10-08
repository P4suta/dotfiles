use std::process::Command;

#[path = "../../crates/platform/tests/support/fixture_process.rs"]
mod fixture_process;

/// The path environment variables xtask refuses while they still hold a variable reference.
fn xtask(scope: &std::path::Path, environment: &str, value: &str) -> Command {
    let mut command = fixture_process::command(env!("CARGO_BIN_EXE_xtask"), scope);
    command.arg("--help").env(environment, value);
    command
}

#[test]
fn an_unexpanded_target_directory_is_named_and_refused() {
    let scope = tempfile::tempdir().expect("a fixture directory");
    for (name, value, reference) in [
        ("CARGO_TARGET_DIR", "D:\\cargo$k", "$k"),
        ("CARGO_HOME", "D:/cargo/${name}", "${name}"),
        ("RUSTUP_HOME", "D:/tools/%NAME%", "%NAME%"),
    ] {
        let output = xtask(scope.path(), name, value)
            .output()
            .expect("start the xtask binary");
        assert!(!output.status.success(), "{name}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(name), "{name}: {stderr}");
        assert!(stderr.contains(reference), "{name}: {stderr}");
        assert!(stderr.contains("Next action"), "{name}: {stderr}");
    }
    let status = xtask(scope.path(), "CARGO_TARGET_DIR", "D:/cargo/lane")
        .status()
        .expect("start the xtask binary");
    assert!(status.success());
}

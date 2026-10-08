use std::process::Command;

/// The path environment variables xtask refuses while they still hold a variable reference.
fn xtask(environment: &str, value: &str) -> Command {
    let mut command = platform::process::command(env!("CARGO_BIN_EXE_xtask"));
    command.arg("--help").env(environment, value);
    command
}

#[test]
fn an_unexpanded_target_directory_is_named_and_refused() {
    for (name, value, reference) in [
        ("CARGO_TARGET_DIR", "D:\\cargo$k", "$k"),
        ("CARGO_HOME", "D:/cargo/${name}", "${name}"),
        ("RUSTUP_HOME", "D:/tools/%NAME%", "%NAME%"),
    ] {
        let output = xtask(name, value).output().expect("start the xtask binary");
        assert!(!output.status.success(), "{name}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(name), "{name}: {stderr}");
        assert!(stderr.contains(reference), "{name}: {stderr}");
        assert!(stderr.contains("Next action"), "{name}: {stderr}");
    }
    let status = xtask("CARGO_TARGET_DIR", "D:/cargo/lane")
        .status()
        .expect("start the xtask binary");
    assert!(status.success());
}

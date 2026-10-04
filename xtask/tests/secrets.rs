#![allow(
    clippy::disallowed_methods,
    reason = "integration tests spawn the binaries and real tools they verify"
)]

use anyhow::{Result, ensure};
use std::fs;
use std::path::Path;
use std::process::Command;

#[test]
fn scoped_doppler_execution_refuses_invalid_values_and_preserves_consumer_exit_status() -> Result<()>
{
    let scope = tempfile::tempdir()?;
    let fake = scope
        .path()
        .join(format!("doppler{}", std::env::consts::EXE_SUFFIX));
    let status = Command::new("rustc")
        .args(["--edition", "2024"])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/secret-process.rs"))
        .arg("-o")
        .arg(&fake)
        .status()?;
    ensure!(status.success(), "fake Doppler compilation failed");
    let path = std::env::join_paths(std::iter::once(scope.path().to_path_buf()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))?;
    for (index, value) in ["fixture-only", "", "${fixture.API_KEY}"]
        .iter()
        .enumerate()
    {
        let consumed = scope.path().join(format!("consumer-{index}"));
        let doppler = scope.path().join(format!("doppler-{index}"));
        let output = Command::new(env!("CARGO_BIN_EXE_dotfiles-xtask"))
            .current_dir(scope.path())
            .args([
                "run-secrets",
                "--project",
                "fixture",
                "--config",
                "fixture",
                "--names",
                "API_KEY",
                "--",
            ])
            .arg(&fake)
            .arg("--consumer")
            .env("PATH", &path)
            .env("FAKE_SECRET", value)
            .env("FAKE_DOPPLER_LOG", &doppler)
            .env("FAKE_CONSUMER_LOG", &consumed)
            .output()?;
        assert_eq!(output.status.code(), Some(if index == 0 { 37 } else { 1 }));
        assert_eq!(consumed.exists(), index == 0);
        assert_eq!(fs::read(doppler)?, b"scope and no-fallback verified");
        assert!(!String::from_utf8_lossy(&output.stderr).contains(value) || value.is_empty());
    }
    assert!(!scope.path().join(".doppler").exists());
    Ok(())
}

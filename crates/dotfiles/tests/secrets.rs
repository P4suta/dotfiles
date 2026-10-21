use anyhow::{Result, ensure};
use std::fs;
use std::path::Path;

#[path = "../../platform/tests/support/fixture_process.rs"]
mod fixture_process;

#[test]
fn scoped_doppler_execution_refuses_invalid_values_and_preserves_consumer_exit_status() -> Result<()>
{
    let scope = tempfile::tempdir()?;
    let home = scope.path().join("configured home");
    let checkout = scope.path().join("checkout");
    fs::create_dir(&home)?;
    fs::create_dir(&checkout)?;
    let fake = scope
        .path()
        .join(format!("doppler{}", std::env::consts::EXE_SUFFIX));
    let status = platform::process::command("rustc")
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
        let output = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), scope.path())
            .current_dir(&checkout)
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
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("PATH", &path)
            .env("FAKE_SECRET", value)
            .env("FAKE_DOPPLER_LOG", &doppler)
            .env("FAKE_CONSUMER_LOG", &consumed)
            .output()?;
        assert_eq!(
            output.status.code(),
            Some(if index == 0 { 37 } else { 1 }),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(consumed.exists(), index == 0);
        assert_eq!(fs::read(doppler)?, b"scope and no-fallback verified");
        assert!(!String::from_utf8_lossy(&output.stderr).contains(value) || value.is_empty());
    }
    for home in [None, Some(""), Some("relative-home")] {
        let doppler = scope.path().join("invalid-home-doppler");
        let consumed = scope.path().join("invalid-home-consumer");
        let mut command = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), scope.path());
        command
            .current_dir(&checkout)
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
            .env("FAKE_SECRET", "fixture-only")
            .env("FAKE_DOPPLER_LOG", &doppler)
            .env("FAKE_CONSUMER_LOG", &consumed);
        for name in ["HOME", "USERPROFILE"] {
            if let Some(home) = home {
                command.env(name, home);
            } else {
                command.env_remove(name);
            }
        }
        let output = command.output()?;
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("absolute home directory"));
        assert!(!doppler.exists());
        assert!(!consumed.exists());
    }
    assert!(!scope.path().join(".doppler").exists());
    Ok(())
}

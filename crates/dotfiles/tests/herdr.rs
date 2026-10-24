#![cfg(unix)]

use anyhow::Result;
use std::process::Stdio;

const DOTFILES: &str = env!("CARGO_BIN_EXE_dotfiles");

#[test]
fn cleanup_refuses_noninteractive_use_before_inspecting_or_signalling_a_process() -> Result<()> {
    let home = tempfile::tempdir()?;
    let output = platform::process::command(DOTFILES)
        .args(["herdr", "cleanup", "--pid", &std::process::id().to_string()])
        .env("HOME", home.path())
        .stdin(Stdio::null())
        .output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)?.contains("owner's interactive confirmation"));
    Ok(())
}

#[test]
fn session_health_is_advisory_on_a_host_without_the_managed_ssh_configuration() -> Result<()> {
    let home = tempfile::tempdir()?;
    let output = platform::process::command(DOTFILES)
        .args(["herdr", "health", "--session"])
        .env("HOME", home.path())
        .output()?;
    assert!(output.status.success());
    assert_eq!(output.stdout, b"");
    assert_eq!(output.stderr, b"");
    Ok(())
}

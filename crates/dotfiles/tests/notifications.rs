//! Optional notifications preserve host results and report failures without private inputs.

use anyhow::{Context, Result, ensure};
use std::path::Path;
use std::process::{Command, Stdio};

#[path = "../../platform/tests/support/fixture_process.rs"]
mod fixture_process;

fn runtime(home: &Path) -> Command {
    let mut command = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), home);
    command
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("CI", "notification-fixture");
    command
}

fn desktop_runtime(home: &Path) -> Command {
    let mut command = runtime(home);
    for name in [
        "CI",
        "SSH_CONNECTION",
        "SSH_TTY",
        "DOTFILES_HOST_CACHE",
        "WSL_DISTRO_NAME",
        platform::speech::INTERNAL,
    ] {
        command.env_remove(name);
    }
    command
        .env("DISPLAY", "notification-fixture")
        .env("SESSIONNAME", "Console");
    command
}

#[test]
fn disabled_missing_or_malformed_optional_callbacks_never_create_audio_state() -> Result<()> {
    let home = tempfile::tempdir()?;
    for args in [
        vec![
            "notifications",
            "emit",
            "commit-completed",
            "--id",
            "fixture",
        ],
        vec!["notifications", "agent", "codex", "not-json"],
        vec!["notifications", "agent", "codex", "{\"type\":\"unknown\"}"],
        vec![
            "notifications",
            "agent",
            "codex",
            "{\"type\":\"agent-turn-complete\",\"thread-id\":\"fixture\",\"turn-id\":\"turn\"}",
        ],
    ] {
        let output = runtime(home.path()).args(args).output()?;
        assert!(output.status.success());
        assert_eq!(output.stdout, Vec::<u8>::new());
        assert_eq!(output.stderr, Vec::<u8>::new());
    }
    assert!(!home.path().join(platform::speech::STATE).exists());
    assert!(!home.path().join(".local/state/skill-ops").exists());
    Ok(())
}

#[test]
fn enabled_malformed_or_unprepared_callbacks_remain_quiet() -> Result<()> {
    let home = tempfile::tempdir()?;
    let config = home.path().join(platform::speech::CONFIG);
    std::fs::create_dir_all(&config)?;
    std::fs::write(config.join("frequency"), "all\n")?;
    let missing_transcript = serde_json::json!({
        "session_id": "private session",
        "hook_event_name": "Notification",
        "notification_type": "permission_prompt",
        "transcript_path": home.path().join("absent-transcript"),
    })
    .to_string();
    let private_scalar = serde_json::to_string("private forwarding scalar and path")?;
    for args in [
        vec![
            "notifications",
            "emit",
            "commit-completed",
            "--id",
            "fixture",
        ],
        vec!["notifications", "cancel", "--id", ""],
        vec!["notifications", "agent", "codex", "not-json"],
        vec![
            "notifications",
            "agent",
            "codex",
            "{\"type\":\"agent-turn-complete\"}",
        ],
        vec!["notifications", "agent", "claude", "{}"],
        vec!["notifications", "agent", "claude", &missing_transcript],
        vec![
            "notifications",
            "agent",
            "codex",
            "--forward-notify",
            "not-json",
            "not-json",
        ],
        vec![
            "notifications",
            "agent",
            "codex",
            "--forward-notify",
            &private_scalar,
            "private callback payload",
        ],
    ] {
        let output = desktop_runtime(home.path()).args(args).output()?;
        assert!(output.status.success());
        assert_eq!(output.stdout, Vec::<u8>::new());
        assert_eq!(output.stderr, Vec::<u8>::new());
    }
    assert!(!home.path().join(platform::speech::STATE).exists());
    Ok(())
}

#[test]
fn optional_operation_failures_warn_without_changing_status_or_exposing_input() -> Result<()> {
    let home = tempfile::tempdir()?;
    let private_input = "private notification payload and identity";
    let forward = serde_json::to_string(&[home.path().join("absent-callback")])?;
    let forwarded = runtime(home.path())
        .args([
            "notifications",
            "agent",
            "codex",
            "--forward-notify",
            &forward,
            private_input,
        ])
        .output()?;
    let state = home.path().join(platform::speech::STATE);
    std::fs::create_dir_all(state.parent().context("fixture state has no parent")?)?;
    std::fs::write(&state, "owned obstruction")?;
    let cancelled = runtime(home.path())
        .args(["notifications", "cancel", "--id", private_input])
        .output()?;
    let launch_home = tempfile::tempdir()?;
    let config = launch_home.path().join(platform::speech::CONFIG);
    std::fs::create_dir_all(&config)?;
    std::fs::write(config.join("frequency"), "all\n")?;
    std::fs::write(
        config.join(platform::speech::MATERIALS),
        "fixture marker; no worker exists",
    )?;
    let launched = desktop_runtime(launch_home.path())
        .args([
            "notifications",
            "emit",
            "commit-completed",
            "--id",
            private_input,
        ])
        .output()?;
    for (output, operation) in [
        (forwarded, "forward the existing notification callback"),
        (cancelled, "cancel the spoken notification"),
        (launched, "queue the spoken notification"),
    ] {
        assert!(output.status.success());
        assert_eq!(output.stdout, Vec::<u8>::new());
        let warning = String::from_utf8(output.stderr)?;
        assert_eq!(
            warning,
            format!("warning: could not {operation}: notification operation failed\n")
        );
        assert!(!warning.contains(private_input));
        assert!(!warning.contains(home.path().to_string_lossy().as_ref()));
        assert!(!warning.contains(launch_home.path().to_string_lossy().as_ref()));
    }
    Ok(())
}

#[test]
fn status_is_read_only_and_frequency_override_survives_managed_default_changes() -> Result<()> {
    let home = tempfile::tempdir()?;
    let output = runtime(home.path())
        .args(["notifications", "status"])
        .output()?;
    assert!(output.status.success());
    let actual: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(actual["playback_attempted_by_status"], false);
    assert_eq!(actual["audible_confirmation"], false);
    assert_eq!(actual["materials_valid"], false);
    assert!(!home.path().join(platform::speech::STATE).exists());
    let output = runtime(home.path())
        .args(["notifications", "frequency", "off"])
        .output()?;
    assert!(output.status.success());
    let config = home.path().join(platform::speech::CONFIG);
    std::fs::create_dir_all(&config)?;
    std::fs::write(config.join("frequency"), "all\n")?;
    assert_eq!(
        platform::speech::frequency(home.path())?,
        rules::notification_rules::Frequency::Off
    );
    Ok(())
}

#[test]
fn unknown_presets_are_refused_without_using_input_as_a_command() -> Result<()> {
    let home = tempfile::tempdir()?;
    let output = runtime(home.path())
        .args(["notifications", "test", "echo secret > written"])
        .output()?;
    assert!(!output.status.success());
    assert!(!home.path().join("written").exists());
    assert!(!home.path().join(platform::speech::STATE).exists());
    Ok(())
}

#[test]
fn opencode_notifications_refuse_false_completion_and_cached_callbacks_after_disable() -> Result<()>
{
    let home = tempfile::tempdir()?;
    let config = home.path().join(platform::speech::CONFIG);
    std::fs::create_dir_all(&config)?;
    std::fs::write(config.join("frequency"), "all\n")?;
    std::fs::write(
        config.join(platform::speech::MATERIALS),
        "fixture marker; no native player is called",
    )?;
    let runtime_path = platform::process::command("bun")
        .args(["-e", "console.log(process.execPath)"])
        .output()?;
    ensure!(
        runtime_path.status.success(),
        "the pinned Bun fixture runtime is unavailable"
    );
    let runtime_path = String::from_utf8(runtime_path.stdout)?;
    let output = fixture_process::command(runtime_path.trim(), home.path())
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env_remove("CI")
        .env_remove("SSH_CONNECTION")
        .env_remove("SSH_TTY")
        .env_remove("DOTFILES_HOST_CACHE")
        .env_remove("WSL_DISTRO_NAME")
        .env_remove(platform::speech::INTERNAL)
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/notifications-adapter.ts"))
        .arg(home.path())
        .stdin(Stdio::null())
        .output()?;
    ensure!(
        output.status.success(),
        "notification adapter fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

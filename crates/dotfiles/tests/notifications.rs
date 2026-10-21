//! Optional notifications preserve host results and report failures without private inputs.

use anyhow::{Context, Result, ensure};
use std::path::Path;
use std::process::{Command, Stdio};

#[path = "../../platform/tests/support/fixture_process.rs"]
mod fixture_process;

const GIT_DISPATCH_FIXTURE: &str = "DOTFILES_GIT_NOTIFICATION_DISPATCH_FIXTURE";
const GIT_OPERATION: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

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

fn prepared_git_home() -> Result<tempfile::TempDir> {
    let home = tempfile::tempdir()?;
    let config = home.path().join(platform::speech::CONFIG);
    std::fs::create_dir_all(&config)?;
    std::fs::write(config.join("frequency"), "all\n")?;
    std::fs::write(
        config.join(platform::speech::MATERIALS),
        "fixture marker; no worker exists",
    )?;
    Ok(home)
}

#[test]
fn git_notifications_refuse_stale_producers_and_non_git_events() -> Result<()> {
    let home = prepared_git_home()?;
    for (producer, event) in [
        ("different-source", "0"),
        (platform::BUILD_HASH, "6"),
        (platform::BUILD_HASH, "7"),
        (platform::BUILD_HASH, "255"),
    ] {
        let output = desktop_runtime(home.path())
            .args([
                "notifications",
                "git",
                "--producer",
                producer,
                "--event",
                event,
                "--operation",
                "fixture",
            ])
            .output()?;
        assert!(output.status.success(), "{output:?}");
        assert!(output.stdout.is_empty() && output.stderr.is_empty());
        assert!(!home.path().join(platform::speech::STATE).exists());
    }
    Ok(())
}

#[test]
fn git_notifications_keep_owner_event_ids_and_nonfatal_worker_failures() -> Result<()> {
    let home = prepared_git_home()?;
    let operation = "0123456789abcdef".repeat(4);
    let mut expected = String::new();
    for event in 0..=5 {
        let output = desktop_runtime(home.path())
            .args([
                "notifications",
                "git",
                "--producer",
                platform::BUILD_HASH,
                "--event",
                &event.to_string(),
                "--operation",
                &operation,
            ])
            .output()?;
        assert!(output.status.success());
        assert_eq!(output.stdout, Vec::<u8>::new());
        assert_eq!(
            String::from_utf8(output.stderr)?,
            "warning: could not start the spoken notification worker: notification operation failed\n"
        );
        expected.push_str(&platform::speech::identity(
            &format!("git-{event}"),
            &operation,
        ));
        expected.push('\n');
    }
    let state = home.path().join(platform::speech::STATE);
    assert_eq!(std::fs::read_to_string(state.join("ledger"))?, expected);
    let queued = std::fs::read_to_string(state.join("queue"))?;
    assert_eq!(queued.lines().count(), 6);
    for (event, line) in queued.lines().enumerate() {
        assert!(line.starts_with(&format!("{event}\t")));
    }
    Ok(())
}

#[test]
fn git_notification_queue_failures_keep_status_and_hide_private_input() -> Result<()> {
    let home = prepared_git_home()?;
    let state = home.path().join(platform::speech::STATE);
    std::fs::create_dir_all(&state)?;
    let queue = state.join("queue");
    std::fs::write(&queue, "owned obstruction")?;
    assert_eq!(
        platform::speech::frequency(home.path())?,
        rules::notification_rules::Frequency::All
    );
    let private_input = "private operation and path";
    let output = desktop_runtime(home.path())
        .args([
            "notifications",
            "git",
            "--producer",
            platform::BUILD_HASH,
            "--event",
            "0",
            "--operation",
            private_input,
        ])
        .output()?;
    assert!(output.status.success());
    assert_eq!(output.stdout, Vec::<u8>::new());
    assert_eq!(
        String::from_utf8(output.stderr)?,
        "warning: could not queue the spoken notification: notification operation failed\n"
    );
    assert_eq!(std::fs::read_to_string(queue)?, "owned obstruction");
    Ok(())
}

#[test]
fn git_notifications_dispatch_the_matching_runtime_from_the_parent() -> Result<()> {
    if std::env::var_os(GIT_DISPATCH_FIXTURE).is_some() {
        platform::speech::announce(
            rules::notification_rules::Event::CommitStarted,
            GIT_OPERATION,
        );
        return Ok(());
    }
    let home = prepared_git_home()?;
    let bin = home.path().join(".local/bin");
    std::fs::create_dir_all(&bin)?;
    std::fs::copy(
        env!("CARGO_BIN_EXE_dotfiles"),
        bin.join(format!("dotfiles{}", std::env::consts::EXE_SUFFIX)),
    )?;
    let mut command = fixture_process::command(std::env::current_exe()?, home.path());
    command
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("DISPLAY", "notification-fixture")
        .env("SESSIONNAME", "Console")
        .env(GIT_DISPATCH_FIXTURE, "1");
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
    let output = command
        .args([
            "--exact",
            "git_notifications_dispatch_the_matching_runtime_from_the_parent",
            "--nocapture",
        ])
        .output()?;
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stderr, Vec::<u8>::new());
    assert_eq!(
        std::fs::read_to_string(home.path().join(platform::speech::STATE).join("ledger"))?,
        format!("{}\n", platform::speech::identity("git-0", GIT_OPERATION))
    );
    Ok(())
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
fn codex_forwarding_cuts_cycles_before_launch_and_preserves_a_distinct_payload() -> Result<()> {
    const MARKER_ENV: &str = "DOTFILES_NOTIFY_FIXTURE_MARKER";
    let home = tempfile::tempdir()?;
    let marker = home.path().join("forwarded");
    let client = home
        .path()
        .join(format!("third-notifier{}", std::env::consts::EXE_SUFFIX));
    let compiled = platform::process::command("rustc")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/notify-client.rs"))
        .arg("-o")
        .arg(&client)
        .output()?;
    ensure!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let dotfiles = home
        .path()
        .join(format!("dotfiles{}", std::env::consts::EXE_SUFFIX));
    let app = home.path().join("SkyComputerUseClient");
    std::fs::copy(&client, &dotfiles)?;
    std::fs::copy(&client, &app)?;
    let own = vec![
        dotfiles.to_string_lossy().into_owned(),
        "notifications".into(),
        "agent".into(),
        "codex".into(),
    ];
    let app_cycle = vec![
        app.to_string_lossy().into_owned(),
        "turn-ended".into(),
        "--previous-notify".into(),
        serde_json::to_string(&own)?,
    ];
    let indirect = vec![
        client.to_string_lossy().into_owned(),
        "--forward-notify".into(),
        serde_json::to_string(&app_cycle)?,
    ];
    let mut app_with_payload_separator = app_cycle.clone();
    app_with_payload_separator.push("--".into());
    let payload = "opaque private payload with spaces and \"quotes\"";
    for callback in [own, app_cycle, indirect, app_with_payload_separator] {
        let output = runtime(home.path())
            .env(MARKER_ENV, &marker)
            .args([
                "notifications",
                "agent",
                "codex",
                "--forward-notify",
                &serde_json::to_string(&callback)?,
                payload,
            ])
            .output()?;
        assert!(output.status.success(), "{output:?}");
        assert!(
            output.stdout.is_empty() && output.stderr.is_empty(),
            "{output:?}"
        );
        assert!(
            !marker.exists(),
            "a callback with a return edge was launched"
        );
    }
    let distinct =
        serde_json::to_string(&[client.to_string_lossy().as_ref(), "argument with spaces"])?;
    let output = runtime(home.path())
        .env(MARKER_ENV, &marker)
        .args([
            "notifications",
            "agent",
            "codex",
            "--forward-notify",
            &distinct,
            payload,
        ])
        .output()?;
    assert!(output.status.success(), "{output:?}");
    assert!(
        output.stdout.is_empty() && output.stderr.is_empty(),
        "{output:?}"
    );
    assert_eq!(std::fs::read_to_string(marker)?, format!("{payload}\n"));
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

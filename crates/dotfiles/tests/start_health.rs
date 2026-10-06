use anyhow::Result;
use dotfiles::health::{PROBE_VARIABLE, gate_with, incidents};
use dotfiles::start_health_rules::STALL_MILLIS;
use dotfiles::tool::external;
use std::time::{Duration, Instant};

/// A probe that outlives the bound, as process creation does on a host with a hung Defender engine.
const STALLED: &str = if cfg!(windows) {
    "ping -n 30 127.0.0.1"
} else {
    "sleep 30"
};

fn trivial() -> std::process::Command {
    if cfg!(windows) {
        let mut command = external("cmd");
        command.args(["/d", "/c", "exit 0"]);
        command
    } else {
        external("true")
    }
}

/// A stalled start may also wait out the two event queries.
fn bound() -> Duration {
    Duration::from_millis(STALL_MILLIS * 3 + 5_000)
}

#[test]
fn a_hook_refuses_a_stalled_start_within_the_bound_and_names_the_restart() -> Result<()> {
    let home = tempfile::tempdir()?;
    let started = Instant::now();
    let output = external(env!("CARGO_BIN_EXE_dotfiles"))
        .args(["hook", "pre-commit"])
        .env(PROBE_VARIABLE, STALLED)
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .output()?;
    let elapsed = started.elapsed();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(elapsed < bound(), "the refusal took {elapsed:?}");
    assert!(
        stderr.contains("\"rule\":\"host.start-stalled\"")
            && stderr.contains("\"next\":\"Restart-Computer\""),
        "{stderr}"
    );
    let log = std::fs::read_to_string(incidents(home.path()))?;
    assert_eq!(log.lines().count(), 1, "{log}");
    assert!(log.contains("\"context\":\"pre-commit\""), "{log}");
    Ok(())
}

#[test]
fn a_session_start_hands_the_agent_the_refusal() -> Result<()> {
    let home = tempfile::tempdir()?;
    let output = external(env!("CARGO_BIN_EXE_dotfiles"))
        .args(["health", "--session"])
        .env(PROBE_VARIABLE, STALLED)
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.contains("\"rule\":\"host.start-stalled\"") && stdout.contains("Restart-Computer"),
        "{stdout}"
    );
    assert!(
        std::fs::read_to_string(incidents(home.path()))?.contains("\"context\":\"session-start\"")
    );
    Ok(())
}

#[test]
fn a_prompt_start_passes_and_records_nothing() -> Result<()> {
    let home = tempfile::tempdir()?;
    gate_with(home.path(), Some(trivial()), "test")?;
    gate_with(home.path(), None, "test")?;
    assert!(!incidents(home.path()).exists());
    Ok(())
}

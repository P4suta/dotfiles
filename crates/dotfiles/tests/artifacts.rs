use anyhow::{Context, Result};
use dotfiles::artifacts::{Digest, Inputs, Plan, Request, Target};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Output;

#[path = "../../platform/tests/support/fixture_process.rs"]
mod fixture_process;

const TARGET: &str = "windows-x64";
const TOOL: &str = "example";
const BYTES: &[u8] = b"abc";
const SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const PROJECT: &str = "[tools]\nexample = \"1.2.3\"\n";
const CARGO: &str = "version = 4\n";

fn lock() -> String {
    format!(
        "lockfile_version = 3\n[[tools.example]]\nversion = \"1.2.3\"\nbackend = \"aqua:owner/example\"\nspecifiers = [\"1.2.3\"]\n[tools.example.\"platforms.windows-x64\"]\nurl = \"https://example.com/example-1.2.3.zip\"\nchecksum = \"sha256:{SHA256}\"\n"
    )
}

fn source(root: &Path) -> Result<()> {
    fs::create_dir_all(root.join("home/.chezmoidata"))?;
    fs::write(root.join("mise.toml"), PROJECT)?;
    fs::write(root.join("mise.lock"), lock())?;
    fs::write(root.join("Cargo.lock"), CARGO)?;
    fs::write(root.join("home/.chezmoidata/versions.toml"), "")?;
    Ok(())
}

fn invoke(root: &Path, action: &str, extra: &[&std::ffi::OsStr]) -> Result<Output> {
    let home = root.join("owned-home");
    fs::create_dir_all(&home)?;
    let mut command = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), root);
    command
        .args(["--root"])
        .arg(root)
        .args(["artifacts", action, "--target", TARGET, "--tool", TOOL])
        .args(extra)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("SKILL_OPS_STATE", home.join(".local/state/skill-ops"))
        .env("XDG_STATE_HOME", home.join(".local/state"));
    command.output().context("run the acquisition fixture CLI")
}

#[test]
fn the_cli_reports_complete_scoped_material_and_preserves_existing_reports() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    source(fixture.path())?;
    let first = invoke(fixture.path(), "plan", &[])?;
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let report: Value = serde_json::from_slice(&first.stdout)?;
    assert_eq!(
        report["artifacts"]
            .as_array()
            .context("artifact report")?
            .len(),
        1
    );
    assert_eq!(report["artifacts"][0]["expected"].as_str(), Some(SHA256));
    assert_eq!(
        report["scope"].as_str(),
        Some("project-pins-and-workspace-cargo")
    );
    let file = fixture.path().join("plan.json");
    fs::write(&file, b"preserved report")?;
    let refused = invoke(
        fixture.path(),
        "plan",
        &[std::ffi::OsStr::new("--output"), file.as_os_str()],
    )?;
    assert!(!refused.status.success());
    assert_eq!(fs::read(file)?, b"preserved report");
    Ok(())
}

#[test]
fn the_cli_preserves_unresolved_diagnostics_and_refuses_acquisition_before_fetch() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    source(fixture.path())?;
    fs::write(
        fixture.path().join("mise.toml"),
        "[tools]\nexample = \"stable\"\n",
    )?;
    let refused = invoke(fixture.path(), "plan", &[])?;
    assert!(!refused.status.success());
    let report: Value = serde_json::from_slice(&refused.stdout)?;
    assert_eq!(
        report["diagnostics"][0]["reason"].as_str(),
        Some("unpinned-version")
    );
    let cache = fixture.path().join("cache");
    let refused = invoke(
        fixture.path(),
        "acquire",
        &[std::ffi::OsStr::new("--cache"), cache.as_os_str()],
    )?;
    assert!(!refused.status.success());
    assert!(
        !cache.exists(),
        "unresolved metadata must refuse before cache mutation or transport"
    );
    Ok(())
}

#[test]
fn the_cli_grants_only_acquisition_credit_and_refuses_poisoned_material() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    source(fixture.path())?;
    let lock = lock();
    let plan = Plan::parse(
        Inputs {
            project: PROJECT,
            mise_lock: &lock,
            cargo_lock: CARGO,
            versions: "",
        },
        Request {
            targets: vec![Target::WindowsX64],
            tools: vec![TOOL.into()],
        },
    )?;
    let artifact = serde_json::to_value(plan.artifacts().first().context("selected material")?)?;
    let provenance = artifact["provenance"]
        .as_str()
        .context("pinned source provenance")?;
    let cache = fixture.path().join("cache");
    fs::create_dir(&cache)?;
    fs::write(cache.join(format!("{SHA256}.blob")), BYTES)?;
    let legacy = cache.join(format!("{SHA256}-{provenance}.json"));
    let legacy_bytes = serde_json::to_vec(
        &serde_json::json!({"schema":1,"expected":SHA256,"provenance":provenance}),
    )?;
    fs::write(&legacy, &legacy_bytes)?;
    assert_eq!(
        Digest::of(BYTES).hex(),
        SHA256,
        "the standard known-answer digest must match"
    );
    let acquired = invoke(
        fixture.path(),
        "acquire",
        &[std::ffi::OsStr::new("--cache"), cache.as_os_str()],
    )?;
    assert!(
        acquired.status.success(),
        "{}",
        String::from_utf8_lossy(&acquired.stderr)
    );
    let acquisition: Value = serde_json::from_slice(&acquired.stdout)?;
    assert_eq!(acquisition["scope"].as_str(), Some("acquisition-only"));
    assert_eq!(
        acquisition["plan"].as_str(),
        Some(plan.digest().hex().as_str())
    );
    assert_eq!(fs::read(cache.join(format!("{SHA256}.blob")))?, BYTES);
    assert_eq!(fs::read(&legacy)?, legacy_bytes);
    assert_eq!(fs::read_dir(&cache)?.count(), 3);
    let verified = invoke(
        fixture.path(),
        "verify",
        &[std::ffi::OsStr::new("--cache"), cache.as_os_str()],
    )?;
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );
    let report: Value = serde_json::from_slice(&verified.stdout)?;
    assert_eq!(report["scope"].as_str(), Some("acquisition-only"));
    assert_eq!(report["plan"].as_str(), Some(plan.digest().hex().as_str()));
    fs::write(cache.join(format!("{SHA256}.blob")), b"poisoned")?;
    let refused = invoke(
        fixture.path(),
        "verify",
        &[std::ffi::OsStr::new("--cache"), cache.as_os_str()],
    )?;
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("SHA-256 mismatch"));
    Ok(())
}

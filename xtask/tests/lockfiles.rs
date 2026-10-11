use anyhow::{Result, ensure};
use std::fs;
use std::path::Path;
use xtask::quality::{locked_workspaces, workspace_manifests};

#[path = "../../crates/dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

fn cargo(root: &Path, manifest: &str) -> Result<()> {
    let output = platform::process::command("cargo")
        .current_dir(root)
        .args([
            "generate-lockfile",
            "--offline",
            "--manifest-path",
            manifest,
        ])
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn every_tracked_workspace_requires_a_current_lockfile() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let root = scope.path();
    let global = root.join("gitconfig");
    fs::write(&global, "")?;
    fs::create_dir_all(root.join("local/src"))?;
    fs::create_dir_all(root.join("satellite/src"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = ['local']\nresolver = '3'\n",
    )?;
    let local = "[package]\nname = 'local'\nversion = '0.0.0'\nedition = '2024'\n";
    fs::write(root.join("local/Cargo.toml"), local)?;
    fs::write(root.join("local/src/lib.rs"), "")?;
    fs::write(
        root.join("satellite/Cargo.toml"),
        "[package]\nname = 'satellite'\nversion = '0.0.0'\nedition = '2024'\n[dependencies]\nlocal = { path = '../local' }\n[workspace]\n",
    )?;
    fs::write(root.join("satellite/src/lib.rs"), "")?;
    for arguments in [
        &["init", "--quiet"][..],
        &["add", "Cargo.toml", "local", "satellite"],
    ] {
        let status = fixture_git::command("git", &global)
            .current_dir(root)
            .args(arguments)
            .status()?;
        ensure!(status.success(), "prepare tracked manifests");
    }
    assert_eq!(
        workspace_manifests(root)?,
        ["Cargo.toml", "satellite/Cargo.toml"]
    );
    let missing = locked_workspaces(root).unwrap_err().to_string();
    assert!(
        missing.contains("Cargo.toml:") && missing.contains("satellite/Cargo.toml:"),
        "{missing}"
    );
    cargo(root, "Cargo.toml")?;
    cargo(root, "satellite/Cargo.toml")?;
    locked_workspaces(root)?;
    fs::write(
        root.join("local/Cargo.toml"),
        local.replace("0.0.0", "0.0.1"),
    )?;
    let stale = locked_workspaces(root).unwrap_err().to_string();
    assert!(
        stale.contains("Cargo.toml:") && stale.contains("satellite/Cargo.toml:"),
        "{stale}"
    );
    assert!(stale.contains("--locked"), "{stale}");
    cargo(root, "Cargo.toml")?;
    cargo(root, "satellite/Cargo.toml")?;
    locked_workspaces(root)?;
    Ok(())
}

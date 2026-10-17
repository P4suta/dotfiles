use anyhow::{Result, ensure};
use std::fs;
use std::path::Path;
use xtask::quality::{locked_workspaces, with_locked_workspaces, workspace_manifests};

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

fn fixture(independent: &str) -> Result<tempfile::TempDir> {
    let scope = tempfile::tempdir()?;
    let root = scope.path();
    fs::create_dir_all(root.join("local/src"))?;
    fs::create_dir_all(root.join(independent).join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = ['local']\nresolver = '3'\n",
    )?;
    let local = "[package]\nname = 'local'\nversion = '0.0.0'\nedition = '2024'\n";
    fs::write(root.join("local/Cargo.toml"), local)?;
    fs::write(root.join("local/src/lib.rs"), "")?;
    fs::write(
        root.join(independent).join("Cargo.toml"),
        format!(
            "[package]\nname = '{independent}'\nversion = '0.0.0'\nedition = '2024'\n[dependencies]\nlocal = {{ path = '../local' }}\n[workspace]\n"
        ),
    )?;
    fs::write(root.join(independent).join("src/lib.rs"), "")?;
    Ok(scope)
}

fn track(root: &Path, independent: &str) -> Result<()> {
    let global = root.join("gitconfig");
    fs::write(&global, "")?;
    for arguments in [
        &["init", "--quiet"][..],
        &["add", "Cargo.toml", "local", independent],
    ] {
        let status = fixture_git::command("git", &global)
            .current_dir(root)
            .args(arguments)
            .status()?;
        ensure!(status.success(), "prepare tracked manifests");
    }
    Ok(())
}

#[test]
fn every_tracked_workspace_requires_a_current_lockfile() -> Result<()> {
    let scope = fixture("satellite")?;
    let root = scope.path();
    track(root, "satellite")?;
    let local = fs::read_to_string(root.join("local/Cargo.toml"))?;
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
    let mut heavy_calls = 0;
    let barrier = with_locked_workspaces(root, || {
        heavy_calls += 1;
        Ok(())
    })
    .expect_err("every stale lock is refused before heavy verification");
    assert_eq!(heavy_calls, 0);
    let barrier = format!("{barrier:#}");
    assert!(
        barrier.contains("Cargo.toml:") && barrier.contains("satellite/Cargo.toml:"),
        "{barrier}"
    );
    cargo(root, "Cargo.toml")?;
    cargo(root, "satellite/Cargo.toml")?;
    locked_workspaces(root)?;
    Ok(())
}

#[test]
fn a_stale_independent_path_lock_stops_before_the_heavy_callback() -> Result<()> {
    let scope = fixture("benchmarks")?;
    let root = scope.path();
    assert!(!root.join(".git").exists());
    cargo(root, "Cargo.toml")?;
    cargo(root, "benchmarks/Cargo.toml")?;
    let manifest = root.join("benchmarks/Cargo.toml");
    let original = fs::read_to_string(&manifest)?;
    fs::write(&manifest, original.replace("0.0.0", "0.0.1"))?;
    let mut heavy_calls = 0;
    let error = with_locked_workspaces(root, || {
        heavy_calls += 1;
        Ok(())
    })
    .expect_err("an independent workspace lock must match its path dependency graph");
    assert_eq!(heavy_calls, 0);
    let refusal = format!("{error:#}");
    assert!(refusal.contains("benchmarks/Cargo.toml:"), "{refusal}");
    assert!(refusal.contains("--locked"), "{refusal}");
    assert!(refusal.contains("1 selected stages failed"), "{refusal}");
    cargo(root, "benchmarks/Cargo.toml")?;
    with_locked_workspaces(root, || {
        heavy_calls += 1;
        Ok(())
    })?;
    assert_eq!(heavy_calls, 1);
    Ok(())
}

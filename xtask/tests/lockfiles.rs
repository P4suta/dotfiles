use anyhow::{Result, ensure};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output};
use xtask::quality::{
    locked_workspaces, with_locked_workspaces, with_locked_workspaces_using, workspace_manifests,
};

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

fn cargo_output(code: i32, stderr: &str) -> Output {
    #[cfg(unix)]
    let status = {
        use std::os::unix::process::ExitStatusExt as _;
        ExitStatus::from_raw(code << 8)
    };
    #[cfg(windows)]
    let status = {
        use std::os::windows::process::ExitStatusExt as _;
        ExitStatus::from_raw(code.cast_unsigned())
    };
    Output {
        status,
        stdout: Vec::new(),
        stderr: stderr.as_bytes().to_vec(),
    }
}

fn cargo_invocation(root: &Path, command: &Command) -> Result<(String, PathBuf)> {
    assert_eq!(command.get_current_dir(), Some(root));
    let arguments: Vec<_> = command.get_args().collect();
    assert!(arguments.iter().any(|argument| *argument == "--locked"));
    assert!(!arguments.iter().any(|argument| {
        matches!(
            argument.to_str(),
            Some("--target" | "--no-deps" | "--frozen")
        )
    }));
    let operation = arguments[0].to_string_lossy().into_owned();
    assert_eq!(
        arguments.iter().any(|argument| *argument == "--offline"),
        operation == "metadata"
    );
    assert_eq!(arguments[arguments.len() - 2], "--manifest-path");
    let manifest = Path::new(arguments[arguments.len() - 1])
        .strip_prefix(root)?
        .to_owned();
    Ok((operation, manifest))
}

#[test]
fn every_workspace_is_fetched_before_any_offline_resolution_or_heavy_verification() -> Result<()> {
    let scope = fixture("satellite")?;
    let root = scope.path();
    for manifest in ["Cargo.toml", "satellite/Cargo.toml"] {
        cargo(root, manifest)?;
    }
    let mut fetched = BTreeSet::new();
    let mut calls = Vec::new();
    let mut heavy_calls = 0;
    with_locked_workspaces_using(
        root,
        |command| {
            let (operation, manifest) = cargo_invocation(root, command)?;
            let output = if operation == "fetch" {
                fetched.insert(manifest.clone());
                cargo_output(0, "")
            } else {
                assert_eq!(operation, "metadata");
                if fetched.len() == 2 && fetched.contains(&manifest) {
                    cargo_output(0, "")
                } else {
                    cargo_output(1, "offline Cargo material is unavailable")
                }
            };
            calls.push((operation, manifest));
            Ok(output)
        },
        || {
            heavy_calls += 1;
            Ok(())
        },
    )?;
    assert_eq!(heavy_calls, 1);
    assert_eq!(
        calls,
        [
            ("fetch".into(), "Cargo.toml".into()),
            ("fetch".into(), "satellite/Cargo.toml".into()),
            ("metadata".into(), "Cargo.toml".into()),
            ("metadata".into(), "satellite/Cargo.toml".into()),
        ]
    );
    Ok(())
}

#[test]
fn every_failed_fetch_is_reported_before_metadata_or_heavy_verification() -> Result<()> {
    let scope = fixture("satellite")?;
    let root = scope.path();
    for manifest in ["Cargo.toml", "satellite/Cargo.toml"] {
        cargo(root, manifest)?;
    }
    let mut calls = Vec::new();
    let mut heavy_calls = 0;
    let error = with_locked_workspaces_using(
        root,
        |command| {
            let (operation, manifest) = cargo_invocation(root, command)?;
            assert_eq!(operation, "fetch");
            calls.push(manifest.clone());
            if manifest == Path::new("Cargo.toml") {
                Err(std::io::Error::from(std::io::ErrorKind::NotFound).into())
            } else {
                Ok(cargo_output(1, "fixture dependency fetch failed"))
            }
        },
        || {
            heavy_calls += 1;
            Ok(())
        },
    )
    .expect_err("all fetch failures must precede offline resolution and verification");
    assert_eq!(heavy_calls, 0);
    assert_eq!(
        calls,
        [PathBuf::from("Cargo.toml"), "satellite/Cargo.toml".into()]
    );
    let refusal = format!("{error:#}");
    assert!(refusal.contains("Cargo.toml:"), "{refusal}");
    assert!(refusal.contains("satellite/Cargo.toml:"), "{refusal}");
    assert!(refusal.contains("2 selected stages failed"), "{refusal}");
    assert!(
        refusal.contains("fixture dependency fetch failed"),
        "{refusal}"
    );
    Ok(())
}

#[test]
fn changed_inputs_during_fetch_are_refused_before_metadata_or_heavy_verification() -> Result<()> {
    for changed in ["Cargo.toml", "Cargo.lock", ".cargo/config.toml"] {
        let scope = fixture("satellite")?;
        let root = scope.path();
        for manifest in ["Cargo.toml", "satellite/Cargo.toml"] {
            cargo(root, manifest)?;
        }
        fs::create_dir(root.join(".cargo"))?;
        fs::write(root.join(".cargo/config.toml"), "[net]\nretry = 0\n")?;
        let changed = root.join(changed);
        let mut calls = 0;
        let mut heavy_calls = 0;
        let error = with_locked_workspaces_using(
            root,
            |command| {
                let (operation, _) = cargo_invocation(root, command)?;
                assert_eq!(operation, "fetch");
                if calls == 0 {
                    let text = fs::read_to_string(&changed)?;
                    fs::write(&changed, format!("{text}\n# changed during fetch\n"))?;
                }
                calls += 1;
                Ok(cargo_output(0, ""))
            },
            || {
                heavy_calls += 1;
                Ok(())
            },
        )
        .expect_err("successful fetches cannot authorize changed source inputs");
        assert_eq!(calls, 2);
        assert_eq!(heavy_calls, 0);
        assert!(
            error.to_string().contains("Cargo workload inputs changed"),
            "{error:#}"
        );
    }
    Ok(())
}

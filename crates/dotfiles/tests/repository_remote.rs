#![cfg(unix)]

use anyhow::{Result, ensure};
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::Output;

#[path = "../../platform/tests/support/fixture_process.rs"]
mod fixture_process;

fn repository_command(root: &Path, bin: &Path, action: &str, remote: &str) -> Result<Output> {
    let path = std::env::join_paths(std::iter::once(bin.to_path_buf()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))?;
    Ok(
        fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), root)
            .arg("--root")
            .arg(root)
            .args(["repository", action, "--remote", remote])
            .env("PATH", path)
            .output()?,
    )
}

#[test]
fn repository_remotes_resolve_to_github_identity_before_description_operations() -> Result<()> {
    let project = tempfile::tempdir()?;
    let tools = tempfile::tempdir()?;
    let root = project.path();
    platform::signed_git::git(root, &["init", "--quiet"], None)?;
    fs::write(
        root.join("repository.toml"),
        concat!(
            "name = \"fixture\"\nsummary = \"A fixture repository\"\n",
            "install = \"fixture\"\nusage = \"fixture --help\"\nlicense = \"unlicensed\"\n",
        ),
    )?;
    dotfiles::repository::write(root)?;
    let gh = tools.path().join("gh");
    fs::write(
        &gh,
        concat!(
            "#!/bin/sh\n",
            "if [ \"$3\" != Owner/Fixture ]; then\n",
            "  printf 'expected Owner/Fixture, got %s\\n' \"$3\" >&2\n  exit 19\nfi\n",
            "case \"$1 $2\" in\n",
            "  'repo view') printf 'A fixture repository\\n' ;;\n",
            "  'repo edit') exit 0 ;;\n",
            "  *) exit 20 ;;\nesac\n",
        ),
    )?;
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o700))?;
    platform::signed_git::git(
        root,
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:Owner/Fixture.git",
        ],
        None,
    )?;
    for url in [
        "git@github.com:Owner/Fixture.git",
        "https://github.com/Owner/Fixture.git",
        "ssh://git@github.com/Owner/Fixture.git",
    ] {
        platform::signed_git::git(root, &["remote", "set-url", "origin", url], None)?;
        for remote in ["origin", "Owner/Fixture"] {
            for action in ["check", "generate"] {
                let output = repository_command(root, tools.path(), action, remote)?;
                ensure!(
                    output.status.success(),
                    "{action} {remote}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
    for remote in ["missing", "owner/repo/extra", "-invalid"] {
        let output = repository_command(root, tools.path(), "check", remote)?;
        assert!(!output.status.success());
    }
    platform::signed_git::git(
        root,
        &[
            "remote",
            "set-url",
            "origin",
            "https://example.com/Owner/Fixture.git",
        ],
        None,
    )?;
    let output = repository_command(root, tools.path(), "check", "origin")?;
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("expected Owner/Fixture"));
    Ok(())
}

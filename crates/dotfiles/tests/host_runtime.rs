use anyhow::{Context, Result, ensure};
use dotfiles::host_checks::Scratch;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn candidate_git_runs_the_rendered_hook_with_the_candidate_runtime() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let scope = tempfile::tempdir()?;
    let config = scope.path().join("chezmoi.toml");
    fs::write(
        &config,
        format!(
            "[data]\nprofile = {:?}\n",
            dotfiles::profiles::native_profile()?.name()
        ),
    )?;
    let scratch = Scratch::new(&config)?;
    let environment = scratch.command("git");
    let home = PathBuf::from(
        environment
            .get_envs()
            .find_map(|(name, value)| (name == "HOME").then_some(value).flatten())
            .context("candidate home")?,
    );
    let mut render = dotfiles::profiles::chezmoi(&root, scope.path(), &config, &home);
    for (name, value) in environment.get_envs() {
        if let Some(value) = value {
            render.env(name, value);
        } else {
            render.env_remove(name);
        }
    }
    let rendered = render
        .args(["execute-template", "--file"])
        .arg(root.join("home/dot_config/git/hooks/executable_reference-transaction.tmpl"))
        .output()?;
    ensure!(
        rendered.status.success(),
        "{}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    let hooks = home.join(".config/git/hooks");
    fs::create_dir_all(&hooks)?;
    let hook = hooks.join("reference-transaction");
    fs::write(&hook, rendered.stdout)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        const EXECUTABLE_MODE: u32 = 0o755;
        fs::set_permissions(&hook, fs::Permissions::from_mode(EXECUTABLE_MODE))?;
    }
    fs::write(
        home.join(".config/git/config"),
        "[core]\nhooksPath = ~/.config/git/hooks\n[commit]\ngpgsign = false\n",
    )?;
    let repository = scope.path().join("repository");
    fs::create_dir(&repository)?;
    let run = || {
        scratch
            .command(platform::tool::Tool::Git.program())
            .current_dir(&repository)
            .args(["init", "--bare", "--quiet"])
            .output()
    };
    assert!(!run()?.status.success());
    let binary = Path::new(env!("CARGO_BIN_EXE_dotfiles"));
    scratch.provision_runtime(binary)?;
    let installed = home
        .join(".local/bin")
        .join(format!("dotfiles{}", std::env::consts::EXE_SUFFIX));
    assert_eq!(fs::read(&installed)?, fs::read(binary)?);
    let output = run()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!scope.path().join(".local/bin").exists());
    Ok(())
}

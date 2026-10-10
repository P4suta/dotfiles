use anyhow::{Context, Result, ensure};
use dotfiles::host_checks::Scratch;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

const REFERENCE: &str = "refs/tags/fixture";

fn candidate_git(environment: &Command, repository: &Path) -> Result<Command> {
    let global = environment
        .get_envs()
        .find_map(|(name, value)| (name == "GIT_CONFIG_GLOBAL").then_some(value).flatten())
        .context("candidate Git configuration")?;
    let mut command = fixture_git::command("git", Path::new(global));
    for (name, value) in environment.get_envs() {
        if let Some(value) = value {
            command.env(name, value);
        } else {
            command.env_remove(name);
        }
    }
    command.current_dir(repository);
    Ok(command)
}

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
    fs::write(home.join(".config/git/config"), "")?;
    let repository = scope.path().join("repository");
    fs::create_dir(&repository)?;
    ensure!(
        candidate_git(&environment, &repository)?
            .args(["init", "--bare", "--quiet"])
            .status()?
            .success(),
        "initialize the fixture repository"
    );
    fs::write(
        home.join(".config/git/config"),
        "[core]\nhooksPath = ~/.config/git/hooks\n[commit]\ngpgsign = false\n",
    )?;
    let object = candidate_git(&environment, &repository)?
        .args(["hash-object", "-w"])
        .arg(&hook)
        .output()?;
    ensure!(object.status.success(), "write the fixture object");
    let object = String::from_utf8(object.stdout)?;
    let run = || {
        candidate_git(&environment, &repository)?
            .args(["update-ref", REFERENCE, object.trim()])
            .output()
            .map_err(anyhow::Error::from)
    };
    let missing = run()?;
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("dotfiles"));
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

use std::path::{Path, PathBuf};

use anyhow::{Result, ensure};

use super::scoop;
use crate::windows::{env, proc};

/// The gallery modules an interactive shell may do without, whose install failure stays a warning.
const PS_MODULES: &[&str] = &["PSFzf"];

const PSGALLERY_HELPER: &str = include_str!("../../../assets/psgallery.ps1");

pub fn run(scoop_apps: &[String]) -> Result<i32> {
    let token = github_token();
    let token = token.as_deref();

    install_mise(|| mise(token))?;
    let absent = scoop::absent(scoop_apps);
    if !absent.is_empty() {
        eprintln!("scoop has not installed {}", absent.join(", "));
    }

    install_ps_modules()?;
    install_script_analyzer()?;

    println!(">>> git signing config sanity check");
    report(&signing_problems(&SigningConfig::read()));
    Ok(0)
}

fn install_mise(mut command: impl FnMut() -> std::process::Command) -> Result<()> {
    // Installing converges to the reviewed pins, and version changes arrive only as pull requests.
    println!(">>> mise install");
    let mut install = command();
    install.args(["install", "-y"]);
    let status = platform::process::mise_install(install)?.status()?;
    ensure!(status.success(), "mise install failed with {status}");

    proc::run(command().arg("reshim"))?;
    Ok(())
}

fn github_token() -> Option<String> {
    if let Some(existing) = std::env::var_os("GITHUB_TOKEN") {
        return Some(existing.to_string_lossy().into_owned());
    }
    let out = proc::capture(env::command("gh").args(["auth", "token"])).ok()?;
    let token = out.stdout_text().trim().to_owned();
    (out.ok() && !token.is_empty()).then_some(token)
}

fn mise(token: Option<&str>) -> std::process::Command {
    let mut cmd = env::command("mise");
    if let Some(token) = token {
        cmd.env("GITHUB_TOKEN", token);
    }
    cmd
}

/// Saves the pinned `PSScriptAnalyzer` into the user's PowerShell modules and refuses a release whose assembly differs from the pin.
fn install_script_analyzer() -> Result<()> {
    let documents = proc::capture(env::command("pwsh").args([
        "-NoLogo",
        "-NoProfile",
        "-Command",
        "[Environment]::GetFolderPath('MyDocuments')",
    ]))?;
    anyhow::ensure!(documents.ok(), "pwsh can't name the Documents folder");
    let modules = PathBuf::from(documents.stdout_text().trim()).join("PowerShell/Modules");
    println!(
        ">>> Save-Module PSScriptAnalyzer -RequiredVersion {}",
        platform::script_analyzer::VERSION
    );
    platform::script_analyzer::save(&modules)?;
    platform::script_analyzer::verified(&modules)?;
    Ok(())
}

fn install_ps_modules() -> Result<()> {
    let helper = proc::write_temp(PSGALLERY_HELPER, ".ps1")?;
    let mut cmd = env::command("pwsh");
    cmd.args(["-NoLogo", "-NoProfile", "-File"])
        .arg(helper.as_os_str())
        .arg("-Modules")
        .arg(PS_MODULES.join(","));
    proc::run(&mut cmd)
}

#[derive(Debug, Default)]
struct SigningConfig {
    gpgsign: Option<String>,
    signingkey: Option<String>,
    format: Option<String>,
    program: Option<String>,
    allowed_signers: Option<String>,
    program_resolves: bool,
    signers_exist: bool,
}

impl SigningConfig {
    fn read() -> Self {
        let mut config = Self {
            gpgsign: git_config("commit.gpgsign"),
            signingkey: git_config("user.signingkey"),
            format: git_config("gpg.format"),
            program: git_config("gpg.ssh.program"),
            allowed_signers: git_config("gpg.ssh.allowedSignersFile"),
            ..Self::default()
        };
        config.program_resolves = config.program.as_deref().is_some_and(resolves);
        config.signers_exist = config
            .allowed_signers
            .as_deref()
            .map(|path| expand_home(path, env::home_dir().as_deref()))
            .is_some_and(|path| path.is_file());
        config
    }
}

fn signing_problems(config: &SigningConfig) -> Vec<String> {
    let mut problems = Vec::new();

    if config.gpgsign.as_deref() != Some("true") {
        problems.push(
            "commit.gpgsign is not true (~/.config/git/config may not be deployed; \
             re-run chezmoi apply)"
                .to_owned(),
        );
    }
    match config.format.as_deref() {
        Some("ssh") => {}
        other => problems.push(format!(
            "gpg.format is not 'ssh' (current value: {})",
            other.unwrap_or("unset")
        )),
    }
    if config.signingkey.is_none() {
        problems.push("user.signingkey is not set".to_owned());
    }
    match config.program.as_deref() {
        None => problems.push(
            "gpg.ssh.program is not set (~/.config/git/config may not be deployed; \
             re-run chezmoi apply)"
                .to_owned(),
        ),
        Some(program) if program.contains("op-ssh-sign") => problems.push(format!(
            "gpg.ssh.program points at op-ssh-sign: {program} (leftover 1Password Git \
             signing integration in ~/.gitconfig; turn that integration off in the app, \
             delete the signing lines from ~/.gitconfig, then re-run chezmoi apply)"
        )),
        Some(program) if !config.program_resolves => problems.push(format!(
            "gpg.ssh.program path cannot be resolved: {program}"
        )),
        Some(_) => {}
    }
    match config.allowed_signers.as_deref() {
        None => problems.push(
            "gpg.ssh.allowedSignersFile is not set (verify-commit then always reports \
             No signature); chezmoi apply should deploy it"
                .to_owned(),
        ),
        Some(path) if !config.signers_exist => problems.push(format!(
            "gpg.ssh.allowedSignersFile points at a missing path: {path}"
        )),
        Some(_) => {}
    }
    problems
}

fn report(problems: &[String]) {
    if problems.is_empty() {
        println!("    OK - commit signing and verification should work");
        return;
    }
    eprintln!("git signing config sanity check failed:");
    for problem in problems {
        eprintln!("  - {problem}");
    }
    eprintln!(
        "Fix: in the 1Password app, Settings > Developer, turn 'Use the SSH agent' ON \
         (the 'Sign Git commits' integration is not needed). Delete any op-ssh-sign \
         leftovers from ~/.gitconfig, then re-run chezmoi apply"
    );
}

fn git_config(key: &str) -> Option<String> {
    let out = proc::capture(env::command("git").args(["config", "--get", key])).ok()?;
    if !out.ok() {
        return None;
    }
    let value = out.stdout_text().trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn resolves(program: &str) -> bool {
    if Path::new(program).is_file() {
        return true;
    }
    std::env::split_paths(&env::augmented_path())
        .any(|dir| dir.join(program).is_file() || dir.join(format!("{program}.exe")).is_file())
}

fn expand_home(path: &str, home: Option<&Path>) -> PathBuf {
    match (path.strip_prefix('~'), home) {
        (Some(rest), Some(home)) => home.join(rest.trim_start_matches(['/', '\\'])),
        _ => PathBuf::from(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context as _;

    #[test]
    fn applying_paranoid_settings_preserves_a_tracked_project_pin() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let config = scope.path().join("chezmoi.toml");
        std::fs::write(&config, "[data]\nprofile = 'windows'\n")?;
        let scratch = crate::host_checks::Scratch::new(&config)?;
        let environment = scratch.command("mise");
        let variable = |name: &str| -> Result<PathBuf> {
            environment
                .get_envs()
                .find_map(|(key, value)| (key == name).then_some(value).flatten())
                .map(PathBuf::from)
                .ok_or_else(|| anyhow::anyhow!("missing {name}"))
        };
        let global = variable("MISE_GLOBAL_CONFIG_FILE")?;
        std::fs::create_dir_all(global.parent().context("global config directory")?)?;
        std::fs::write(&global, "[settings]\nparanoid = false\n")?;
        let project = scope.path().join("project");
        std::fs::create_dir(&project)?;
        let pin = project.join("mise.toml");
        std::fs::write(&pin, "[tools]\nrg = '15.2.0'\n")?;
        let installation = variable("MISE_DATA_DIR")?.join("installs/rg/15.2.0");
        std::fs::create_dir_all(&installation)?;
        std::fs::write(installation.join("project-tool"), "installed project pin\n")?;
        std::fs::write(
            installation
                .parent()
                .context("backend directory")?
                .join(".mise.backend.toml"),
            "short = 'rg'\nfull = 'aqua:BurntSushi/ripgrep'\n",
        )?;
        for arguments in [
            vec!["trust", pin.to_str().context("project pin path")?],
            vec!["ls", "--json"],
        ] {
            let output = scratch
                .command("mise")
                .current_dir(&project)
                .args(arguments)
                .output()?;
            ensure!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let state = variable("MISE_STATE_DIR")?;
        assert!(
            std::fs::read_dir(state.join("tracked-configs"))?
                .next()
                .is_some()
        );
        std::fs::write(&global, "[settings]\nparanoid = true\n")?;
        let preview = scratch
            .command("mise")
            .args(["prune", "--dry-run"])
            .output()?;
        ensure!(
            preview.status.success(),
            "{}",
            String::from_utf8_lossy(&preview.stderr)
        );
        assert!(String::from_utf8_lossy(&preview.stderr).contains("rg@15.2.0"));
        install_mise(|| scratch.command("mise"))?;
        assert_eq!(
            std::fs::read_to_string(installation.join("project-tool"))?,
            "installed project pin\n"
        );
        assert!(pin.is_file());
        Ok(())
    }

    fn healthy() -> SigningConfig {
        SigningConfig {
            gpgsign: Some("true".to_owned()),
            signingkey: Some("key::ssh-ed25519 AAAA".to_owned()),
            format: Some("ssh".to_owned()),
            program: Some("C:/Windows/System32/OpenSSH/ssh-keygen.exe".to_owned()),
            allowed_signers: Some("~/.config/git/allowed_signers".to_owned()),
            program_resolves: true,
            signers_exist: true,
        }
    }

    #[test]
    fn a_healthy_config_reports_nothing() {
        assert_eq!(signing_problems(&healthy()), Vec::<String>::new());
    }

    #[test]
    fn signing_left_off_is_reported() {
        let config = SigningConfig {
            gpgsign: Some("false".to_owned()),
            ..healthy()
        };
        assert_eq!(signing_problems(&config).len(), 1);
    }

    #[test]
    fn an_op_ssh_sign_leftover_is_named_as_such() {
        let config = SigningConfig {
            program: Some("C:/Program Files/1Password/app/8/op-ssh-sign.exe".to_owned()),
            ..healthy()
        };
        let problems = signing_problems(&config);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("op-ssh-sign"), "{problems:?}");
    }

    #[test]
    fn a_missing_allowed_signers_file_is_reported() {
        let config = SigningConfig {
            signers_exist: false,
            ..healthy()
        };
        let problems = signing_problems(&config);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("missing path"), "{problems:?}");
    }

    #[test]
    fn an_empty_config_reports_every_problem() {
        let problems = signing_problems(&SigningConfig::default());
        assert_eq!(problems.len(), 5);
    }

    #[test]
    fn home_expansion_handles_both_separators() {
        let home = PathBuf::from("C:/home");
        assert_eq!(expand_home("~/a/b", Some(&home)), home.join("a/b"));
        assert_eq!(expand_home(r"~\a", Some(&home)), home.join("a"));
        assert_eq!(
            expand_home("/abs/path", Some(&home)),
            PathBuf::from("/abs/path")
        );
        assert_eq!(expand_home("~/a", None), PathBuf::from("~/a"));
    }
}

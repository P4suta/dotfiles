use std::path::{Path, PathBuf};

use anyhow::Result;

use super::scoop;
use crate::{env, proc};

const PS_MODULES: &[&str] = &["PSFzf", "PSScriptAnalyzer"];

const PSGALLERY_HELPER: &str = include_str!("../../assets/psgallery.ps1");

pub fn run(scoop_apps: &[String]) -> Result<i32> {
    let token = github_token();

    println!(">>> mise install");
    proc::status(mise(&token).args(["install", "-y"]))?;

    println!(">>> mise upgrade --bump");
    proc::status(mise(&token).args(["upgrade", "--bump", "-y"]))?;

    let absent = scoop::absent(scoop_apps);
    if absent.is_empty() {
        println!(">>> mise prune");
        proc::status(mise(&token).args(["prune", "-y"]))?;
        proc::status(mise(&token).arg("reshim"))?;
    } else {
        eprintln!(
            "mise prune waits for scoop to install {}",
            absent.join(", ")
        );
    }

    install_ps_modules()?;

    println!(">>> git signing config sanity check");
    report(&signing_problems(&SigningConfig::read()));
    Ok(0)
}

fn github_token() -> Option<String> {
    if let Some(existing) = std::env::var_os("GITHUB_TOKEN") {
        return Some(existing.to_string_lossy().into_owned());
    }
    let out = proc::capture(env::command("gh").args(["auth", "token"])).ok()?;
    let token = out.stdout_text().trim().to_owned();
    (out.ok() && !token.is_empty()).then_some(token)
}

fn mise(token: &Option<String>) -> std::process::Command {
    let mut cmd = env::command("mise");
    if let Some(token) = token {
        cmd.env("GITHUB_TOKEN", token);
    }
    cmd
}

fn install_ps_modules() -> Result<i32> {
    let helper = proc::write_temp(PSGALLERY_HELPER, ".ps1")?;
    let mut cmd = env::command("pwsh");
    cmd.args(["-NoLogo", "-NoProfile", "-File"])
        .arg(helper.as_os_str())
        .arg("-Modules")
        .arg(PS_MODULES.join(","));
    proc::status(&mut cmd)
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
        assert!(signing_problems(&healthy()).is_empty());
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

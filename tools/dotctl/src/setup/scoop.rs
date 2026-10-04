use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use super::{download, winget};
use crate::{env, proc};

#[derive(Debug)]
pub struct Options {
    pub installer_commit: String,
    pub installer_sha256: String,
    pub buckets: Vec<String>,
    pub apps: Vec<String>,
    pub winget_duplicates: Vec<String>,
}

pub fn run(options: &Options) -> Result<i32> {
    let root = env::scoop_root();
    if !script(&root).is_file() {
        install(options)?;
    }

    for bucket in &options.buckets {
        if !root.join("buckets").join(bucket).is_dir() {
            println!(">>> scoop bucket add {bucket}");
            scoop(&root, &["bucket", "add", bucket])?;
        }
    }

    // Public Scoop repositories must update without the interactive 1Password SSH agent.
    // Scoop commands ignore the machine-wide HTTPS-to-SSH rewrite, and these repositories use HTTPS remotes.
    configure_public_scoop_https(&root.join("apps").join("scoop").join("current"))?;
    for bucket in &options.buckets {
        configure_public_scoop_https(&root.join("buckets").join(bucket))?;
    }

    println!(">>> scoop update");
    scoop(&root, &["update"])?;

    let (present, missing): (Vec<&str>, Vec<&str>) = options
        .apps
        .iter()
        .map(String::as_str)
        .partition(|app| installed(&root, app));
    if !missing.is_empty() {
        println!(">>> scoop install {}", missing.join(" "));
        scoop(&root, &[&["install"], missing.as_slice()].concat())?;
    }
    if !present.is_empty() {
        println!(">>> scoop update {}", present.join(" "));
        scoop(&root, &[&["update"], present.as_slice()].concat())?;
    }

    let absent = absent(&options.apps);
    if !absent.is_empty() {
        eprintln!(
            "scoop left {} uninstalled; keeping the winget copies",
            absent.join(", ")
        );
        return Ok(1);
    }

    let duplicates: Vec<&String> = options
        .winget_duplicates
        .iter()
        .filter(|id| winget::installed(id))
        .collect();
    for id in &duplicates {
        println!(">>> winget uninstall {id}");
        winget::uninstall(id)?;
    }
    if !duplicates.is_empty() {
        let apps: Vec<&str> = options.apps.iter().map(String::as_str).collect();
        println!(">>> scoop reset {}", apps.join(" "));
        scoop(&root, &[&["reset"], apps.as_slice()].concat())?;
    }
    Ok(0)
}

pub fn absent(apps: &[String]) -> Vec<String> {
    let root = env::scoop_root();
    apps.iter()
        .filter(|app| !installed(&root, app))
        .cloned()
        .collect()
}

pub fn current_exe(name: &str) -> PathBuf {
    let exe = env::scoop_root()
        .join("apps")
        .join(name)
        .join("current")
        .join(format!("{name}.exe"));
    if exe.is_file() {
        exe
    } else {
        PathBuf::from(name)
    }
}

fn installed(root: &Path, app: &str) -> bool {
    root.join("apps").join(app).join("current").is_dir()
}

fn configure_public_scoop_https(repo: &Path) -> Result<()> {
    let out = proc::capture(env::command("git").arg("-C").arg(repo).args([
        "config",
        "--local",
        "--get",
        "remote.origin.url",
    ]))?;
    if !out.ok() {
        bail!("reading the Scoop remote for {} failed", repo.display());
    }
    let url = out.stdout_text();
    let Some(suffix) = url.trim().strip_prefix("git@github.com:ScoopInstaller/") else {
        return Ok(());
    };
    let https = format!("https://github.com/ScoopInstaller/{suffix}");
    let code = proc::status(env::command("git").arg("-C").arg(repo).args([
        "config",
        "--local",
        "remote.origin.url",
        &https,
    ]))?;
    if code != 0 {
        bail!(
            "setting HTTPS access for {} exited with {code}",
            repo.display()
        );
    }
    Ok(())
}

fn script(root: &Path) -> PathBuf {
    root.join("apps")
        .join("scoop")
        .join("current")
        .join("bin")
        .join("scoop.ps1")
}

fn install(options: &Options) -> Result<()> {
    let url = format!(
        "https://raw.githubusercontent.com/ScoopInstaller/Install/{}/install.ps1",
        options.installer_commit
    );
    println!(">>> installing scoop from {url}");
    let installer = tempfile::Builder::new()
        .prefix("scoop-install-")
        .suffix(".ps1")
        .tempfile()?
        .into_temp_path();
    download::download_verified(&url, &options.installer_sha256, &installer)?;
    let code = proc::status(pwsh().arg("-File").arg(&*installer))?;
    if code != 0 {
        bail!("the scoop installer exited with {code}");
    }
    Ok(())
}

fn scoop(root: &Path, args: &[&str]) -> Result<()> {
    let code = proc::status(pwsh().arg("-File").arg(script(root)).args(args))?;
    if code != 0 {
        bail!("scoop {} exited with {code}", args.join(" "));
    }
    Ok(())
}

fn pwsh() -> std::process::Command {
    let mut cmd = env::command("pwsh");
    cmd.env("GIT_CONFIG_GLOBAL", "NUL");
    cmd.args(["-NoLogo", "-NoProfile", "-ExecutionPolicy", "Bypass"]);
    cmd
}

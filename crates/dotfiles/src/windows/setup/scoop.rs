use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::{download, winget};
use crate::windows::{env, proc};

/// Scoop's GitHub organization, and the raw files of its installer repository.
const SCOOP_ORGANIZATION: &str = "https://github.com/ScoopInstaller";
const SCOOP_INSTALLER: &str = "https://raw.githubusercontent.com/ScoopInstaller/Install";

#[derive(Debug)]
pub struct Options {
    pub installer_commit: String,
    pub installer_sha256: String,
    pub buckets: Vec<String>,
    pub apps: Vec<String>,
    pub winget_duplicates: Vec<String>,
}

/// Win32's `ERROR_UNTRUSTED_MOUNT_POINT`, which an elevated process gets for a junction that a sandboxed process created.
const UNTRUSTED_MOUNT_POINT: i32 = 448;

pub fn run(options: &Options) -> Result<i32> {
    let root = env::scoop_root();
    if !script(&root).is_file() {
        install(options)?;
    }
    repair_untrusted_links(&root);

    for bucket in &options.buckets {
        if !root.join("buckets").join(bucket).is_dir() {
            println!(">>> scoop bucket add {bucket}");
            scoop(&root, &["bucket", "add", bucket])?;
        }
    }

    // Public Scoop repositories must update without the interactive 1Password SSH agent.
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

/// Creates again each app's `current` junction that this process can't follow, with the same target.
/// A sandboxed agent that runs `scoop update` leaves junctions that ordinary sessions follow and elevated ones, such as domyjob jobs over OpenSSH, refuse.
/// A failed repair leaves its junction in place and prints a warning.
fn repair_untrusted_links(root: &Path) {
    let Ok(apps) = std::fs::read_dir(root.join("apps")) else {
        return;
    };
    for app in apps.flatten() {
        let link = app.path().join("current");
        let untrusted = std::fs::read_dir(&link)
            .err()
            .is_some_and(|error| error.raw_os_error() == Some(UNTRUSTED_MOUNT_POINT));
        if untrusted && let Err(error) = recreate(&app.path(), &link) {
            eprintln!(
                "Warning: the Scoop link {} stays untrusted: {error:#}",
                link.display()
            );
        }
    }
}

/// Creates `link` again with its own target, which must be a version folder of `app`.
fn recreate(app: &Path, link: &Path) -> Result<()> {
    let target = junction::get_target(link)
        .with_context(|| format!("reading the target of {}", link.display()))?;
    let version =
        crate::canonical(&target).with_context(|| format!("resolving {}", target.display()))?;
    if version.parent() != Some(crate::canonical(app)?.as_path()) {
        bail!(
            "{} points outside its app, at {}",
            link.display(),
            target.display()
        );
    }
    // A read-only junction refuses removal, and `/L` changes the link instead of its target.
    let cleared = proc::capture(env::command("attrib").arg("-R").arg(link).arg("/L"))?;
    if !cleared.ok() {
        bail!(
            "clearing the read-only attribute of {} failed",
            link.display()
        );
    }
    junction::delete(link)?;
    std::fs::remove_dir(link)?;
    junction::create(&target, link)
        .with_context(|| format!("linking {} -> {}", link.display(), target.display()))?;
    println!(
        ">>> recreated the Scoop link {} -> {}",
        link.display(),
        target.display()
    );
    Ok(())
}

fn installed(root: &Path, app: &str) -> bool {
    root.join("apps").join(app).join("current").is_dir()
}

fn configure_public_scoop_https(repo: &Path) -> Result<()> {
    // A Scoop installed or bucket added from an archive has no Git repository to configure.
    if !repo.join(".git").exists() {
        return Ok(());
    }
    let out = proc::capture(env::command("git").arg("-C").arg(repo).args([
        "config",
        "--local",
        "--get",
        "remote.origin.url",
    ]))?;
    // `git config --get` exits 1 for an unset key, which means no origin to rewrite.
    if out.code == 1 {
        return Ok(());
    }
    if !out.ok() {
        bail!("reading the Scoop remote for {} failed", repo.display());
    }
    let url = out.stdout_text();
    let Some(suffix) = url.trim().strip_prefix("git@github.com:ScoopInstaller/") else {
        return Ok(());
    };
    let https = format!("{SCOOP_ORGANIZATION}/{suffix}");
    let code = proc::status(env::command("git").arg("-C").arg(repo).args([
        "config",
        "--local",
        "remote.origin.url",
        &https,
    ]))?
    .code();
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
    let url = format!("{SCOOP_INSTALLER}/{}/install.ps1", options.installer_commit);
    println!(">>> installing scoop from {url}");
    let installer = tempfile::Builder::new()
        .prefix("scoop-install-")
        .suffix(".ps1")
        .tempfile()?
        .into_temp_path();
    download::download_verified(&url, &options.installer_sha256, &installer)?;
    let code = proc::status(pwsh().arg("-File").arg(&*installer))?.code();
    if code != 0 {
        bail!("the scoop installer exited with {code}");
    }
    Ok(())
}

fn scoop(root: &Path, args: &[&str]) -> Result<()> {
    let code = proc::status(pwsh().arg("-File").arg(script(root)).args(args))?.code();
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A Scoop root with one app whose `current` junction points at `target`.
    fn app(root: &Path, target: &Path) -> (PathBuf, PathBuf) {
        let app = root.join("apps").join("tool");
        std::fs::create_dir_all(target).unwrap();
        std::fs::write(target.join("tool.exe"), b"").unwrap();
        std::fs::create_dir_all(&app).unwrap();
        let link = app.join("current");
        junction::create(target, &link).unwrap();
        (app, link)
    }

    fn resolved(link: &Path) -> PathBuf {
        crate::canonical(&junction::get_target(link).unwrap()).unwrap()
    }

    #[test]
    fn a_readable_link_keeps_its_target() {
        let root = tempfile::tempdir().unwrap();
        let version = root.path().join("apps").join("tool").join("1.0.0");
        let (_, link) = app(root.path(), &version);
        repair_untrusted_links(root.path());
        assert_eq!(resolved(&link), crate::canonical(&version).unwrap());
        assert!(link.join("tool.exe").is_file());
    }

    #[test]
    fn recreating_a_read_only_link_keeps_its_target_and_clears_the_attribute() {
        let root = tempfile::tempdir().unwrap();
        let version = root.path().join("apps").join("tool").join("1.0.0");
        let (app, link) = app(root.path(), &version);
        let marked = proc::capture(env::command("attrib").arg("+R").arg(&link).arg("/L")).unwrap();
        assert!(marked.ok());
        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .permissions()
                .readonly()
        );
        recreate(&app, &link).unwrap();
        assert_eq!(resolved(&link), crate::canonical(&version).unwrap());
        assert!(link.join("tool.exe").is_file());
        assert!(
            !std::fs::symlink_metadata(&link)
                .unwrap()
                .permissions()
                .readonly()
        );
    }

    #[test]
    fn a_link_outside_its_app_is_refused_and_kept() {
        let root = tempfile::tempdir().unwrap();
        let elsewhere = root.path().join("elsewhere");
        let (app, link) = app(root.path(), &elsewhere);
        assert!(recreate(&app, &link).is_err());
        assert_eq!(resolved(&link), crate::canonical(&elsewhere).unwrap());
    }
}

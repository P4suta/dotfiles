//! Installs ocomment, the prose gate's binary.
//!
//! `OComment` has no crates.io release, so this clones its repository and builds the release binary.
//! The prose gate's config names this checkout.
//! Every apply runs this step, and it rebuilds only when the checkout's revision differs from the recorded revision of the installed binary.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use super::ocomment_rules::{Action, action};
use crate::windows::{env, proc};

#[derive(Debug)]
pub struct Options {
    pub repo_url: String,
    pub destination: String,
}

/// Reports a failure as a warning, so a stale gate never rolls back the rest of the apply.
pub fn run(options: &Options) {
    println!(">>> ocomment install");
    if let Err(err) = build(options) {
        eprintln!("ocomment install failed: {err:#}");
    }
}

fn head(checkout: &Path) -> Result<Option<String>> {
    if !checkout.join("rust").join("Cargo.toml").is_file() {
        return Ok(None);
    }
    let mut revision = env::command("git");
    revision.arg("-C").arg(checkout).args(["rev-parse", "HEAD"]);
    Ok(Some(
        proc::capture_ok(&mut revision)
            .context("reading the OComment revision")?
            .trim()
            .to_owned(),
    ))
}

fn install(built: &Path, dest: &Path) -> Result<()> {
    platform::aside::install(built, dest)
}

fn build_command(checkout: &Path, target: &Path) -> Command {
    let mut build = env::command("cargo");
    build
        .args(["build", "--release", "--locked", "-p", "ocomment"])
        .arg("--manifest-path")
        .arg(checkout.join("rust/Cargo.toml"))
        .arg("--target-dir")
        .arg(target);
    build
}

fn build(options: &Options) -> Result<()> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .context("no home directory to install under")?;
    let checkout = home.join(&options.destination);
    let dest = home.join(".local").join("bin").join("ocomment.exe");
    let stamp = home.join(".local/state/dotfiles/ocomment.revision");
    let recorded = std::fs::read_to_string(&stamp).ok();
    let current = head(&checkout)?;
    match action(
        current.as_deref().map(str::as_bytes),
        dest.is_file(),
        recorded.as_deref().map(|text| text.trim().as_bytes()),
    ) {
        Action::Current => {
            println!("ocomment is current at {}", current.unwrap_or_default());
            return Ok(());
        }
        Action::Clone => {
            let mut clone = env::command("git");
            clone.args(["clone", &options.repo_url]);
            clone.arg(&checkout);
            proc::run(&mut clone).context("cloning OComment")?;
        }
        Action::Build => {}
    }
    let revision = head(&checkout)?.context("the OComment checkout has no Rust workspace")?;

    // `--locked` builds the committed lockfile, the same one the tests used.
    let target = checkout.join("rust/target");
    proc::run(&mut build_command(&checkout, &target)).context("building ocomment")?;

    let binary = target.join("release/ocomment.exe");
    install(&binary, &dest)?;

    let mut version = env::command(&dest.display().to_string());
    version.arg("--version");
    proc::run(&mut version).context("running the installed ocomment")?;
    std::fs::create_dir_all(stamp.parent().context("no state parent")?)?;
    std::fs::write(&stamp, format!("{revision}\n")).context("recording the ocomment revision")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installation_uses_its_build_output_despite_an_inherited_cargo_target() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let checkout = fixture.path().join("checkout");
        let target = checkout.join("rust/target");
        let shared = fixture.path().join("shared-target");
        std::fs::create_dir_all(checkout.join("rust/src"))?;
        std::fs::write(
            checkout.join("rust/Cargo.toml"),
            "[package]\nname = \"ocomment\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )?;
        std::fs::write(
            checkout.join("rust/Cargo.lock"),
            "version = 4\n[[package]]\nname = \"ocomment\"\nversion = \"0.1.0\"\n",
        )?;
        std::fs::write(checkout.join("rust/src/main.rs"), "fn main() {}\n")?;
        let mut command = build_command(&checkout, &target);
        command.env("CARGO_TARGET_DIR", &shared);
        proc::run(&mut command)?;
        let binary = target.join("release/ocomment.exe");
        let destination = fixture.path().join("installed/ocomment.exe");
        install(&binary, &destination)?;
        assert_eq!(std::fs::read(destination)?, std::fs::read(binary)?);
        assert!(!shared.exists());
        Ok(())
    }
}

//! Installs ocomment, the prose gate's binary.
//!
//! `OComment` has no crates.io release, so this clones its repository and builds the release binary.
//! The prose gate's config names this checkout.
//! Every apply runs this step, and it rebuilds only when the checkout's revision differs from the recorded revision of the installed binary.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::aside_rules::{aside_name, is_aside};
use super::ocomment_rules::{Action, action};
use crate::{env, proc};

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

/// Windows refuses to overwrite a running `ocomment.exe` but lets a process rename it, so the current binary moves aside and a later installation deletes it.
fn install(built: &Path, dest: &Path) -> Result<()> {
    let bin = dest.parent().context("no .local/bin parent")?;
    std::fs::create_dir_all(bin)?;
    let name = dest
        .file_name()
        .context("no binary name")?
        .to_string_lossy()
        .into_owned();
    for entry in std::fs::read_dir(bin)?.flatten() {
        if is_aside(name.as_bytes(), entry.file_name().as_encoded_bytes()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let staged = tempfile::NamedTempFile::new_in(bin)?;
    std::fs::copy(built, staged.path()).context("staging ocomment.exe")?;
    let (_, staged) = staged.keep()?;
    let aside = bin.join(aside_name(&name, std::process::id()));
    let moved = dest.exists();
    if moved {
        std::fs::rename(dest, &aside).context("moving the installed ocomment.exe aside")?;
    }
    if let Err(error) = std::fs::rename(&staged, dest) {
        if moved {
            let _ = std::fs::rename(&aside, dest);
        }
        return Err(error).context("installing ocomment.exe");
    }
    Ok(())
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
    let manifest = checkout.join("rust").join("Cargo.toml");
    let mut build = env::command("cargo");
    build
        .args(["build", "--release", "--locked", "-p", "ocomment"])
        .arg("--manifest-path")
        .arg(&manifest);
    proc::run(&mut build).context("building ocomment")?;

    let binary = checkout
        .join("rust")
        .join("target")
        .join("release")
        .join("ocomment.exe");
    install(&binary, &dest)?;

    let mut version = env::command(&dest.display().to_string());
    version.arg("--version");
    proc::run(&mut version).context("running the installed ocomment")?;
    std::fs::create_dir_all(stamp.parent().context("no state parent")?)?;
    std::fs::write(&stamp, format!("{revision}\n")).context("recording the ocomment revision")?;
    Ok(())
}

//! Installs ocomment, the prose gate's binary.
//!
//! OComment has no crates.io release, so this clones its repository and builds the release binary.
//! The prose gate's config names this checkout.
//! After `git pull` there, a chezmoi apply rebuilds the binary.

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::{env, proc};

#[derive(Debug)]
pub struct Options {
    pub repo_url: String,
    pub destination: String,
}

/// Reports a failure as a warning, so a stale gate never rolls back the rest of the apply.
pub fn run(options: &Options) -> Result<i32> {
    println!(">>> ocomment install");
    if let Err(err) = install(options) {
        eprintln!("ocomment install failed: {err:#}");
    }
    Ok(0)
}

fn install(options: &Options) -> Result<()> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .context("no home directory to install under")?;
    let checkout = home.join(&options.destination);

    if !checkout.join("rust").join("Cargo.toml").is_file() {
        let mut clone = env::command("git");
        clone.args(["clone", &options.repo_url]);
        clone.arg(&checkout);
        proc::run(&mut clone).context("cloning OComment")?;
    }

    // `--locked` builds the committed lockfile, the same one the tests used.
    let manifest = checkout.join("rust").join("Cargo.toml");
    let mut build = env::command("cargo");
    build
        .args(["build", "--release", "--locked", "-p", "ocomment"])
        .arg("--manifest-path")
        .arg(&manifest);
    proc::run(&mut build).context("building ocomment")?;

    let built = checkout
        .join("rust")
        .join("target")
        .join("release")
        .join("ocomment.exe");
    let dest = home.join(".local").join("bin").join("ocomment.exe");
    std::fs::create_dir_all(dest.parent().context("no .local/bin parent")?)?;
    std::fs::copy(&built, &dest).context("installing ocomment.exe")?;

    let mut version = env::command(&dest.display().to_string());
    version.arg("--version");
    proc::run(&mut version).context("running the installed ocomment")?;
    Ok(())
}

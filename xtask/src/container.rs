//! The target semantics and native identity of proof containers.

use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use std::ffi::OsString;
use std::path::Path;
use std::process::Stdio;

pub(crate) const LINUX_AMD64: &str = "linux/amd64";
const PLATFORM_FORMAT: &str = "{{.Os}}/{{.Architecture}}";
const ASSET_TEST: &str = "proof_images_reject_corrupt_verifier_assets_before_use";

pub(crate) fn verify_image_assets(root: &Path) -> Result<()> {
    let output = Tool::Cargo
        .command()
        .current_dir(root)
        .args([
            "test",
            "--locked",
            "--package",
            "xtask",
            "--test",
            "container_proofs",
            "--",
            "--ignored",
            "--exact",
            ASSET_TEST,
            "--nocapture",
        ])
        .output()
        .context("check the model images' corrupted verifier assets")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    print!("{stdout}");
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    ensure!(
        output.status.success()
            && stdout
                .lines()
                .any(|line| line.starts_with("test result: ok. 1 passed; 0 failed;")),
        "the model image corruption regression did not pass"
    );
    Ok(())
}

pub(crate) fn amd64_image_present(image: &str) -> Result<bool> {
    let output = Tool::Docker
        .command()
        .args(["image", "inspect", "--format", PLATFORM_FORMAT, image])
        .stderr(Stdio::null())
        .output()?;
    Ok(output.status.success() && String::from_utf8(output.stdout)?.trim() == LINUX_AMD64)
}

pub(crate) fn run_arguments() -> Vec<OsString> {
    let mut arguments: Vec<OsString> = ["run", "--rm", "--network", "none"]
        .map(OsString::from)
        .into();
    arguments.extend(owner_arguments());
    arguments
}

#[cfg(unix)]
fn owner_arguments() -> [OsString; 2] {
    let user = rustix::process::geteuid();
    let group = rustix::process::getegid();
    ["--user".into(), format!("{user}:{group}").into()]
}

#[cfg(not(unix))]
fn owner_arguments() -> [OsString; 0] {
    []
}

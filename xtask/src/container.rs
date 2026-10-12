//! The target semantics and native identity of proof containers.

use crate::tool::Tool;
use anyhow::Result;
use std::ffi::OsString;
use std::process::Stdio;

pub(crate) const LINUX_AMD64: &str = "linux/amd64";
const PLATFORM_FORMAT: &str = "{{.Os}}/{{.Architecture}}";

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

//! The native identity used by proof containers that write host output.

use std::ffi::OsString;

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

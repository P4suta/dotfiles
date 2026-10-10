//! A keyring login and mise shims contending for an install lock.

use std::fs::{self, File};
use std::path::PathBuf;
use std::time::Duration;

#[path = "../../../crates/platform/src/github.rs"]
mod github;
#[path = "../../../crates/platform/src/paths.rs"]
pub mod paths;
#[path = "../../../crates/platform/src/process.rs"]
mod process;

const POLL_INTERVAL: Duration = Duration::from_millis(10);

fn fixture_path(name: &str) -> PathBuf {
    std::env::var_os(name).unwrap().into()
}

fn install() {
    let lock = File::create(fixture_path("GH_FIXTURE_LOCK")).unwrap();
    lock.lock().unwrap();
    fs::write(fixture_path("GH_FIXTURE_HELD"), "install holds the lock").unwrap();
    let mut child =
        process::isolated_command(fixture_path("GH_FIXTURE_REAL_MISE"), std::env::vars_os())
            .args(["token", "github", "--raw", "github.com"])
            .spawn()
            .unwrap();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if fixture_path("GH_FIXTURE_RELEASE").exists() {
            drop(lock);
            break child.wait().unwrap();
        }
        std::thread::sleep(POLL_INTERVAL);
    };
    std::process::exit(status.code().unwrap());
}

fn reenter(arguments: &[String]) {
    assert_eq!(&arguments[..3], ["x", "--", "gh"]);
    fs::write(
        fixture_path("GH_FIXTURE_REENTERED"),
        "mise x waits for install",
    )
    .unwrap();
    let lock = File::options()
        .read(true)
        .write(true)
        .open(fixture_path("GH_FIXTURE_LOCK"))
        .unwrap();
    lock.lock().unwrap();
    let status = process::command(fixture_path("GH_FIXTURE_INSTALLED_GH"))
        .args(&arguments[3..])
        .status()
        .unwrap();
    std::process::exit(status.code().unwrap());
}

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let executable = std::env::current_exe().unwrap();
    if executable.file_stem().unwrap() == "dotfiles" {
        assert_eq!(arguments, ["github-token"]);
        let status = github::command(&std::env::var_os("MISE_CREDENTIAL_HOST").unwrap())
            .unwrap()
            .status()
            .unwrap();
        std::process::exit(status.code().unwrap());
    }
    if executable.file_stem().unwrap() == "mise" {
        if arguments == ["install", "-y"] {
            install();
        } else {
            reenter(&arguments);
        }
    }
    if executable.parent().unwrap().file_name().unwrap() == "shims" {
        let status = process::command(fixture_path("GH_FIXTURE_MISE"))
            .args(["x", "--", "gh"])
            .args(&arguments)
            .status()
            .unwrap();
        std::process::exit(status.code().unwrap());
    }
    assert_eq!(
        arguments,
        [
            "auth",
            "token",
            "--hostname",
            &std::env::var("MISE_CREDENTIAL_HOST").unwrap(),
        ]
    );
    assert_eq!(std::env::var("MISE_CREDENTIAL_PROVIDER").unwrap(), "github");
    std::fs::write(
        std::env::var_os("GH_FIXTURE_LOG").unwrap(),
        arguments.join(" "),
    )
    .unwrap();
    if std::env::var("GH_FIXTURE_LOGIN").unwrap() == "yes" {
        println!("fixture-keyring-token");
    } else {
        std::process::exit(1);
    }
}

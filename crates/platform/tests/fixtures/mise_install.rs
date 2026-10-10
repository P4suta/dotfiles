//! A managed tool runs while its installer holds an operating system lock.

use std::fs::{self, File};
use std::path::PathBuf;
use std::time::Duration;

#[path = "../../src/home.rs"]
pub mod home;
#[path = "../../src/paths.rs"]
pub mod paths;
#[path = "../../src/process.rs"]
pub mod process;
#[path = "../../../rules/src/process_rules.rs"]
pub mod process_rules;

const POLL_INTERVAL: Duration = Duration::from_millis(10);

fn state(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os("BACKEND_FIXTURE_STATE").unwrap()).join(name)
}

fn install() {
    let lock = File::create(state("install.lock")).unwrap();
    lock.lock().unwrap();
    fs::write(state("held"), "install holds its lock").unwrap();
    let mut child = process::command("backend").spawn().unwrap();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if state("release").exists() {
            drop(lock);
            break child.wait().unwrap();
        }
        std::thread::sleep(POLL_INTERVAL);
    };
    std::process::exit(status.code().unwrap());
}

fn main() {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let executable = std::env::current_exe().unwrap();
    let name = executable.file_stem().unwrap().to_str().unwrap();
    let tool = std::env::var("BACKEND_FIXTURE_TOOL").unwrap();
    let status = match name {
        "mise" if arguments == ["install", "-y"] => {
            install();
            unreachable!()
        }
        "mise" => {
            assert_eq!(arguments, ["x", "--", &tool, "--version"]);
            fs::write(state("reentered"), "shim waits for the install lock").unwrap();
            let lock = File::options()
                .read(true)
                .write(true)
                .open(state("install.lock"))
                .unwrap();
            lock.lock().unwrap();
            process::command(std::env::var_os("BACKEND_FIXTURE_NATIVE").unwrap())
                .arg("--version")
                .status()
                .unwrap()
        }
        "backend" => process::command(&tool).arg("--version").status().unwrap(),
        _ if executable.parent().unwrap().file_name().unwrap() == "shims" => {
            process::command(std::env::var_os("BACKEND_FIXTURE_MISE").unwrap())
                .args(["x", "--", &tool])
                .args(&arguments)
                .status()
                .unwrap()
        }
        _ => {
            assert_eq!(name, tool);
            assert_eq!(arguments, ["--version"]);
            fs::write(state("native"), &tool).unwrap();
            println!("fixture-native-{tool}");
            return;
        }
    };
    std::process::exit(status.code().unwrap());
}

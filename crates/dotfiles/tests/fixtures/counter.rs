//! A stand-in for dotguard, lefthook, and Git that appends each invocation to `COUNTER_LOG`.
//! As Git it forwards to the binary `COUNTER_GIT` names, and as lefthook it configures every hook.

use std::io::Write;
use std::process::Command;

/// Every hook the managed hooks directory installs, which the stand-in lefthook configures.
const HOOKS: &str = "pre-commit: {}\ncommit-msg: {}\npre-push: {}\npost-commit: {}\npost-checkout: {}\npost-merge: {}\nreference-transaction: {}\n";

fn main() {
    let name = std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .unwrap_or_default();
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if let Some(log) = std::env::var_os("COUNTER_LOG") {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log)
            .unwrap();
        writeln!(file, "{name} {}", arguments.join(" ")).unwrap();
    }
    match name.as_str() {
        "git" => {
            let status = Command::new(std::env::var_os("COUNTER_GIT").unwrap())
                .args(&arguments)
                .status()
                .unwrap();
            std::process::exit(status.code().unwrap_or(1));
        }
        "lefthook" if arguments.first().map(String::as_str) == Some("dump") => {
            print!("{HOOKS}");
        }
        _ => {}
    }
}

//! A native boundary for project and global Actionlint selection.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

#[path = "../../../platform/src/home.rs"]
pub mod home;
#[path = "../../../platform/src/paths.rs"]
pub mod paths;
#[path = "../../../platform/src/process.rs"]
pub mod process;
#[path = "../../../rules/src/process_rules.rs"]
pub mod process_rules;

fn record(value: &str) {
    writeln!(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(std::env::var_os("ACTIONLINT_FIXTURE_LOG").unwrap())
            .unwrap(),
        "{value}"
    )
    .unwrap();
}

fn main() {
    let executable = std::env::current_exe().unwrap();
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if executable.file_stem().unwrap() == "mise" {
        assert_eq!(std::env::var("MISE_AUTO_INSTALL").unwrap(), "false");
        assert_eq!(
            paths::canonical(&std::env::current_dir().unwrap()).unwrap(),
            paths::canonical(&PathBuf::from(
                std::env::var_os("ACTIONLINT_FIXTURE_CWD").unwrap()
            ))
            .unwrap()
        );
        if arguments == ["ls", "--local", "--json"] {
            record("metadata");
            print!(
                "{}",
                fs::read_to_string(std::env::var_os("ACTIONLINT_FIXTURE_METADATA").unwrap())
                    .unwrap()
            );
        } else {
            assert_eq!(
                arguments,
                [
                    "which",
                    "--tool",
                    "github:kjanat/actionlint@1.17.0",
                    "actionlint"
                ]
            );
            record("which");
            if std::env::var("ACTIONLINT_FIXTURE_WHICH").as_deref() == Ok("fail") {
                std::process::exit(1);
            }
            println!(
                "{}",
                std::env::var("ACTIONLINT_FIXTURE_EXECUTABLE").unwrap()
            );
        }
        return;
    }
    let project = paths::canonical(&executable).unwrap()
        == paths::canonical(&PathBuf::from(
            std::env::var_os("ACTIONLINT_FIXTURE_PROJECT").unwrap(),
        ))
        .unwrap();
    record(if project { "project" } else { "global" });
    if !project && std::env::var("ACTIONLINT_FIXTURE_GLOBAL").as_deref() != Ok("allow") {
        eprintln!("a declared project Actionlint must not fall back to PATH");
        std::process::exit(1);
    }
    let root = PathBuf::from(std::env::var_os("ACTIONLINT_FIXTURE_ROOT").unwrap());
    assert_eq!(
        paths::canonical(&std::env::current_dir().unwrap()).unwrap(),
        paths::canonical(&root).unwrap()
    );
    assert_eq!(arguments[0], "-oneline");
    assert_eq!(arguments[1], "-config-file");
    assert_eq!(
        paths::canonical(&PathBuf::from(&arguments[2])).unwrap(),
        paths::canonical(&root.join(".github/actionlint.yaml")).unwrap()
    );
    let status = process::command(std::env::var_os("ACTIONLINT_FIXTURE_NATIVE").unwrap())
        .args(arguments)
        .stdin(std::process::Stdio::inherit())
        .status()
        .unwrap();
    std::process::exit(status.code().unwrap_or(1));
}

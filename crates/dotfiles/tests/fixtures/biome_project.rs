//! A native process boundary that records local and global Biome selection.

use std::fs;
use std::path::PathBuf;

#[path = "../../../platform/src/home.rs"]
pub mod home;
#[path = "../../../platform/src/paths.rs"]
pub mod paths;
#[path = "../../../platform/src/process.rs"]
pub mod process;
#[path = "../../../rules/src/process_rules.rs"]
pub mod process_rules;

fn main() {
    let executable = std::env::current_exe().unwrap();
    let local = executable.file_stem().unwrap() == "mise";
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let log = std::env::var_os("BIOME_FIXTURE_LOG").unwrap();
    fs::write(&log, if local { "local" } else { "global" }).unwrap();
    let arguments = if local {
        assert_eq!(&arguments[..4], ["x", "--no-deps", "--", "node"]);
        assert_eq!(std::env::var("MISE_AUTO_INSTALL").unwrap(), "false");
        let launcher = PathBuf::from(&arguments[4]);
        assert!(launcher.is_absolute());
        assert_eq!(fs::read_to_string(&launcher).unwrap(), "fixture launcher\n");
        &arguments[5..]
    } else {
        if std::env::var("BIOME_FIXTURE_GLOBAL").as_deref() != Ok("allow") {
            eprintln!("the global Biome must not handle a declared local dependency");
            std::process::exit(1);
        }
        &arguments[..]
    };
    let status = process::command(std::env::var_os("BIOME_FIXTURE_NATIVE").unwrap())
        .args(arguments)
        .status()
        .unwrap();
    std::process::exit(status.code().unwrap_or(1));
}

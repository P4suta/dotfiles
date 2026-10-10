//! A GHCup installer whose metadata listing can't bootstrap it.

use std::fs;
use std::path::PathBuf;

fn main() {
    let executable = std::env::current_exe().unwrap();
    let version = std::env::var("HOST_GHC_VERSION").unwrap();
    match executable.file_name().unwrap().to_str().unwrap() {
        "list-all" => {
            fs::write(std::env::var_os("HOST_GHC_LISTING").unwrap(), "listing").unwrap();
            eprintln!("GHCup bootstrap belongs to installation, not version listing");
            std::process::exit(1);
        }
        "install" => {
            assert_eq!(std::env::var("ASDF_INSTALL_VERSION").unwrap(), version);
            let bin = PathBuf::from(std::env::var_os("ASDF_INSTALL_PATH").unwrap()).join("bin");
            fs::create_dir_all(&bin).unwrap();
            fs::copy(&executable, bin.join("ghc")).unwrap();
        }
        "ghc" => println!("{version}"),
        other => panic!("unexpected installer program: {other}"),
    }
}

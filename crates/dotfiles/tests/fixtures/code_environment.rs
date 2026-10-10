//! Fixtures for pinned tools, older tools, unrelated apps, and mise resolution.

use std::path::Path;
use std::process::Command;

fn main() {
    let executable = std::env::current_exe().unwrap();
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    match executable.file_stem().unwrap().to_str().unwrap() {
        "mise" => {
            if arguments == ["--version"] {
                println!("mise 2026.10.0");
                return;
            }
            let file = match arguments[0].as_str() {
                "ls" => "tools.json",
                "bin-paths" => "paths",
                _ => panic!("unexpected mise command: {arguments:?}"),
            };
            print!("{}", std::fs::read_to_string(file).unwrap());
        }
        "unrelated-application" => panic!("the host application reached the code gate"),
        "python" => {
            assert_ne!(executable.parent().unwrap().file_name().unwrap(), "old");
            for name in [
                "GH_TOKEN",
                "GITHUB_TOKEN",
                "SSH_AUTH_SOCK",
                "UNRELATED_SETTING",
            ] {
                assert!(std::env::var_os(name).is_none(), "inherited {name}");
            }
            let home = std::env::var_os("HOME").unwrap();
            assert!(!Path::new(&home).join(".gitconfig").exists());
            for name in ["GIT_CONFIG_GLOBAL", "GIT_CONFIG_SYSTEM"] {
                assert!(
                    std::fs::read(std::env::var_os(name).unwrap())
                        .unwrap()
                        .is_empty()
                );
            }
            let temporary = std::env::temp_dir();
            let canonical = std::fs::canonicalize(&temporary).unwrap();
            assert_eq!(
                Path::new(canonical.to_str().unwrap().trim_start_matches(r"\\?\")),
                temporary.as_path()
            );
            assert!(Command::new("unrelated-application").output().is_err());
            if arguments.is_empty() {
                println!("pinned parent");
                let child = Command::new("python").arg("--child").output().unwrap();
                assert!(
                    child.status.success(),
                    "{}",
                    String::from_utf8_lossy(&child.stderr)
                );
                print!("{}", String::from_utf8(child.stdout).unwrap());
            } else {
                println!("pinned child");
                let git = Command::new("git")
                    .args(["remote", "get-url", "origin"])
                    .output()
                    .unwrap();
                assert!(git.status.success());
                print!("{}", String::from_utf8(git.stdout).unwrap());
            }
        }
        name => panic!("unexpected fixture tool: {name}"),
    }
}

//! Fixtures for pinned tools, older tools, unrelated apps, and mise resolution.

use std::path::Path;
use std::process::Command;

fn main() {
    let executable = std::env::current_exe().unwrap();
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    match executable.file_stem().unwrap().to_str().unwrap() {
        "mise" => {
            if arguments == ["--version"] {
                println!("2026.10.0 fixture-platform (2026-10-01)");
                return;
            }
            let data = std::path::PathBuf::from(std::env::var_os("MISE_DATA_DIR").unwrap());
            let installation = data.join("installs/python/3.14.8");
            if arguments[0] == "install" {
                assert_eq!(arguments, ["install", "--yes", "python@3.14.8"]);
                assert!(data.starts_with(std::env::var_os("HOME").unwrap()));
                std::fs::create_dir_all(&installation).unwrap();
                std::fs::copy(
                    &executable,
                    installation.join(format!("python{}", std::env::consts::EXE_SUFFIX)),
                )
                .unwrap();
                std::fs::write("installed-at", data.to_string_lossy().as_bytes()).unwrap();
                return;
            }
            let file = match arguments[0].as_str() {
                "ls" => "tools.json",
                "bin-paths" => "paths",
                _ => panic!("unexpected mise command: {arguments:?}"),
            };
            let mut contents = std::fs::read_to_string(file).unwrap();
            if Path::new("missing").exists() && installation.is_dir() {
                let paths = std::fs::read_to_string("paths").unwrap();
                let original = paths.lines().next().unwrap();
                contents = if file == "tools.json" {
                    contents
                        .replace("\"installed\":false", "\"installed\":true")
                        .replace(
                            &format!("{original:?}"),
                            &format!("{:?}", installation.to_str().unwrap()),
                        )
                } else {
                    contents.replace(original, installation.to_str().unwrap())
                };
            }
            print!("{contents}");
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
            if cfg!(windows) {
                for name in ["ProgramData", "ALLUSERSPROFILE"] {
                    assert!(Path::new(&std::env::var_os(name).unwrap()).starts_with(&home));
                }
            }
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

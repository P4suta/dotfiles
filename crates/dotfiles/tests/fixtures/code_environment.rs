//! Fixtures for pinned tools, older tools, unrelated apps, and mise resolution.

use std::path::Path;
use std::process::Command;

const MISSING_MATERIAL_EXIT: i32 = 77;
const MATERIAL_SOURCE: &str =
    "https://static.crates.io/crates/host-fixture-material/host-fixture-material-1.2.3.crate";

fn main() {
    let control = Path::new(env!("DOTFILES_TEST_CODE_ROOT"));
    let executable = std::fs::canonicalize(std::env::current_exe().unwrap()).unwrap();
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let invoked = std::path::PathBuf::from(std::env::args_os().next().unwrap());
    match invoked.file_stem().unwrap().to_str().unwrap() {
        "curl" => {
            if arguments == ["--disable", "--version"] {
                println!("curl 8.4.0 fixture");
                return;
            }
            assert_eq!(arguments.first().map(String::as_str), Some("--disable"));
            assert!(
                arguments
                    .iter()
                    .any(|argument| argument == "--max-filesize")
            );
            let source = arguments
                .windows(2)
                .find_map(|pair| (pair[0] == "--url").then_some(pair[1].as_str()))
                .unwrap();
            assert_eq!(source, MATERIAL_SOURCE);
            std::fs::write(
                std::env::var_os("HOSTS_FIXTURE_MATERIAL_FETCH").unwrap(),
                "refused\n",
            )
            .unwrap();
            std::process::exit(MISSING_MATERIAL_EXIT);
        }
        "mise" => {
            if arguments == ["--version"] {
                let version = std::fs::read_to_string(executable.with_extension("version"))
                    .unwrap_or_else(|_| "2026.10.0".into());
                println!("{version} fixture-platform (2026-10-01)");
                return;
            }
            if arguments == ["env", "--json"] {
                assert!(std::env::var_os("__MISE_DIFF").is_some());
                let scope = std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap();
                for name in [
                    "MISE_CONFIG_FILE",
                    "MISE_GLOBAL_CONFIG_FILE",
                    "MISE_SYSTEM_CONFIG_FILE",
                ] {
                    let config = std::path::PathBuf::from(std::env::var_os(name).unwrap());
                    assert!(std::fs::canonicalize(&config).unwrap().starts_with(&scope));
                    assert!(std::fs::read(config).unwrap().is_empty());
                }
                print!(
                    "{}",
                    std::fs::read_to_string(control.join("native-path.json")).unwrap()
                );
                return;
            }
            let data = std::path::PathBuf::from(std::env::var_os("MISE_DATA_DIR").unwrap());
            let installation = data.join("installs/python/3.14.8");
            if arguments[0] == "install" {
                if let Some(pin) = arguments[2].strip_prefix("chezmoi@") {
                    assert_eq!(arguments[..2], ["install", "--yes"]);
                    assert!(data.starts_with(std::env::var_os("HOME").unwrap()));
                    assert!(
                        std::env::vars_os()
                            .all(|(name, _)| { !name.to_string_lossy().starts_with("__MISE_") })
                    );
                    assert!(
                        std::fs::read(std::env::var_os("MISE_GLOBAL_CONFIG_FILE").unwrap())
                            .unwrap()
                            .is_empty()
                    );
                    let source =
                        std::fs::read_to_string(executable.with_extension("renderer")).unwrap();
                    let directory = data.join("installs/chezmoi").join(pin);
                    std::fs::create_dir_all(&directory).unwrap();
                    std::fs::copy(
                        source,
                        directory.join(format!("chezmoi{}", std::env::consts::EXE_SUFFIX)),
                    )
                    .unwrap();
                    return;
                }
                assert_eq!(arguments, ["install", "--yes", "python@3.14.8"]);
                assert!(data.starts_with(std::env::var_os("HOME").unwrap()));
                std::fs::create_dir_all(&installation).unwrap();
                std::fs::write(
                    installation.parent().unwrap().join(".mise.backend.toml"),
                    "short = 'python'\nfull = 'core:python'\n",
                )
                .unwrap();
                std::fs::copy(
                    &executable,
                    installation.join(format!("python{}", std::env::consts::EXE_SUFFIX)),
                )
                .unwrap();
                std::fs::write(
                    control.join("installed-at"),
                    data.to_string_lossy().as_bytes(),
                )
                .unwrap();
                return;
            }
            let file = match arguments[0].as_str() {
                "ls" => "tools.json",
                "bin-paths" => "paths",
                _ => panic!("unexpected mise command: {arguments:?}"),
            };
            let configuration =
                std::path::PathBuf::from(std::env::var_os("MISE_CONFIG_FILE").unwrap());
            assert_eq!(
                std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap(),
                std::fs::canonicalize(configuration.parent().unwrap()).unwrap()
            );
            let mut contents = std::fs::read_to_string(control.join(file)).unwrap();
            if file == "tools.json" {
                contents = contents.replace(
                    &format!("{:?}", control.join("mise.toml").to_str().unwrap()),
                    &format!("{:?}", configuration.to_str().unwrap()),
                );
            }
            if file == "tools.json"
                && control.join("relocate").exists()
                && data.starts_with(std::env::var_os("HOME").unwrap())
                && !control.join("installed-at").exists()
            {
                contents = contents
                    .replace("\"installed\":true", "\"installed\":false")
                    .replace("\"active\":true", "\"active\":false");
            }
            if (control.join("missing").exists() || control.join("relocate").exists())
                && control.join("installed-at").exists()
                && installation.is_dir()
            {
                let paths = std::fs::read_to_string(control.join("paths")).unwrap();
                let original = paths.lines().next().unwrap();
                contents = if file == "tools.json" {
                    contents
                        .replace("\"installed\":false", "\"installed\":true")
                        .replace("\"active\":false", "\"active\":true")
                        .replace(
                            &format!("{original:?}"),
                            &format!("{:?}", installation.to_str().unwrap()),
                        )
                } else {
                    contents.replace(original, installation.to_str().unwrap())
                };
            }
            if installation.is_dir() {
                let paths = std::fs::read_to_string(control.join("paths")).unwrap();
                let original = paths.lines().next().unwrap();
                contents = if file == "tools.json" {
                    contents.replace(
                        &format!("{original:?}"),
                        &format!("{:?}", installation.to_str().unwrap()),
                    )
                } else {
                    contents.replace(original, installation.to_str().unwrap())
                };
            }
            print!("{contents}");
        }
        name @ ("rustc" | "rustdoc") => match arguments.as_slice() {
            [version] if version == "--version" => {
                let toolchain = if executable.file_stem().unwrap() == "rustup" {
                    std::env::var("RUSTUP_TOOLCHAIN").unwrap()
                } else {
                    "1.99.0".into()
                };
                println!("{name} {toolchain} fixture");
            }
            [print, sysroot] if print == "--print" && sysroot == "sysroot" => {
                println!(
                    "{}",
                    std::fs::read_to_string(control.join("rust-sysroot")).unwrap()
                );
            }
            _ => panic!("unexpected rustc command: {arguments:?}"),
        },
        "rustup" => {
            if arguments == ["--version"] {
                println!("captured rustup");
            } else {
                assert_eq!(arguments[0], "run");
                let status = Command::new(&arguments[2])
                    .env("RUSTUP_TOOLCHAIN", &arguments[1])
                    .args(&arguments[3..])
                    .status()
                    .unwrap();
                std::process::exit(status.code().unwrap());
            }
        }
        "cargo" => {
            assert_eq!(executable.file_stem().unwrap(), "rustup");
            if arguments == ["--version"] {
                println!(
                    "cargo {} fixture",
                    std::env::var("RUSTUP_TOOLCHAIN").unwrap()
                );
            } else {
                let program = match arguments.as_slice() {
                    [argument] if argument == "--rustc-version" => "rustc",
                    [argument] if argument == "--rustdoc-version" => "rustdoc",
                    _ => panic!("unexpected cargo command: {arguments:?}"),
                };
                let status = Command::new(program).arg("--version").status().unwrap();
                std::process::exit(status.code().unwrap());
            }
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
            let data = std::path::PathBuf::from(std::env::var_os("MISE_DATA_DIR").unwrap());
            assert!(data.starts_with(&home));
            assert_eq!(
                std::fs::read_to_string(data.join("installs/python/.mise.backend.toml")).unwrap(),
                "short = 'python'\nfull = 'core:python'\n"
            );
            std::fs::write(data.join("gate-state"), "scratch write").unwrap();
            let github = std::path::PathBuf::from(std::env::var_os("GH_CONFIG_DIR").unwrap());
            assert!(github.starts_with(&home));
            assert_eq!(
                std::fs::read_to_string(github.join("hosts.yml")).unwrap(),
                "github.com: {}\n"
            );
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

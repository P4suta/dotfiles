//! A Docker image store with a native platform and an observed build.

use std::fs;

fn main() {
    let executable = std::env::current_exe().unwrap();
    let platform = executable.with_extension("platform");
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    match arguments.first().map(String::as_str) {
        Some("image") => {
            assert_eq!(arguments.get(1).map(String::as_str), Some("inspect"));
            assert!(arguments.iter().any(|argument| argument == "--format"));
            let Ok(platform) = fs::read_to_string(platform) else {
                std::process::exit(1);
            };
            println!("{platform}");
        }
        Some("build") => {
            assert!(
                arguments
                    .windows(2)
                    .any(|pair| pair == ["--platform", "linux/amd64"])
            );
            fs::write(platform, "linux/amd64").unwrap();
            fs::write(executable.with_extension("built"), "linux/amd64").unwrap();
        }
        other => panic!("unexpected Docker command: {other:?}"),
    }
}

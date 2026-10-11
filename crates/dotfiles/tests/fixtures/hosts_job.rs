//! Completed jobs whose waiting client retains its SSH session until the fixture releases it.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

fn workspace(target: &str) -> PathBuf {
    PathBuf::from(std::env::var_os("HOSTS_JOB_STATE").unwrap())
        .join(target.split(':').next().unwrap())
        .join("jobs/gate/workspace")
}

fn report(target: &str) {
    let stages = std::env::var("HOSTS_JOB_STAGES").unwrap();
    let alias = target.split(':').next().unwrap();
    let mut path = workspace(target).to_string_lossy().replace('\\', "/");
    if alias == "pc" && !cfg!(windows) {
        path = format!("C:{path}");
    }
    if alias != "pc" && cfg!(windows) {
        path = format!("/{path}");
    }
    println!(
        "DOTFILES_HOST_STAGES {},\"workspace\":{path:?}}}",
        &stages[..stages.len() - 1]
    );
}

fn clean(target: &str) {
    let expected = workspace(target);
    let arguments: Vec<String> = std::env::args().collect();
    assert!(arguments.iter().any(|argument| {
        argument
            .replace('\\', "/")
            .contains(&expected.to_string_lossy().replace('\\', "/"))
    }));
    let root = PathBuf::from(std::env::var_os("HOSTS_JOB_ROOT").unwrap());
    let notes = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["notes", "--ref", "refs/notes/hosts", "show", "HEAD"])
        .output()
        .unwrap();
    assert!(notes.status.success());
    let notes = String::from_utf8(notes.stdout).unwrap();
    for stage in ["code", "real"] {
        assert!(
            notes
                .lines()
                .any(|line| line.contains(&format!("\"host\":\"{target}\""))
                    && line.contains(&format!("\"stage\":\"{stage}\"")))
        );
    }
    if expected.exists() {
        std::fs::remove_dir_all(expected).unwrap();
    }
}

fn main() {
    let program = std::env::current_exe().unwrap();
    if program.file_stem().unwrap() == "tailscale" {
        println!("{}", std::env::var("HOSTS_JOB_TAILNET").unwrap());
        return;
    }
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments[0] == "held-stdout" {
        let release = PathBuf::from(std::env::var_os("HOSTS_JOB_RELEASE").unwrap());
        while !release.exists() {
            std::thread::sleep(Duration::from_millis(10));
        }
        return;
    }
    let target = &arguments[1];
    let outcome = if std::env::var("HOSTS_JOB_FAILED").unwrap() == "yes" {
        "failed (1)"
    } else {
        "succeeded"
    };
    match arguments[0].as_str() {
        "doctor" => println!("{target}: ready"),
        "on" if arguments.iter().any(|argument| argument == "--wait") => {
            println!("{target}:probe finished succeeded")
        }
        "on" if arguments.iter().any(|argument| {
            argument.contains("df -Pk") || argument.contains("AvailableFreeSpace")
        }) =>
        {
            println!("{target}:space")
        }
        "on" if arguments
            .iter()
            .any(|argument| argument.contains("workspace")) =>
        {
            clean(target);
            println!("{target}:cleanup");
        }
        "on" => println!("{target}:probe"),
        "run" if arguments.iter().any(|argument| argument == "--wait") => {
            report(target);
            println!("{target}:gate finished {outcome}");
            std::io::stdout().flush().unwrap();
            let release = PathBuf::from(std::env::var_os("HOSTS_JOB_RELEASE").unwrap());
            while !release.exists() {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        "run" => {
            let workspace = workspace(target);
            std::fs::create_dir_all(&workspace).unwrap();
            std::fs::write(workspace.join("build-output"), "built").unwrap();
            std::fs::write(
                workspace.parent().unwrap().join("output.log"),
                "retained log",
            )
            .unwrap();
            println!("{target}:gate");
        }
        "status" if target.ends_with(":probe") => println!("{target} finished succeeded"),
        "status" if target.ends_with(":space") => println!("{target} finished succeeded"),
        "status" if target.ends_with(":cleanup") => println!("{target} finished succeeded"),
        "status" => {
            let state = PathBuf::from(std::env::var_os("HOSTS_JOB_STATE").unwrap())
                .join(target.replace(':', "-"));
            if state.exists() {
                let _holder = std::process::Command::new(&program)
                    .arg("held-stdout")
                    .stdin(std::process::Stdio::null())
                    .spawn()
                    .unwrap();
                println!("{target} finished {outcome}");
            } else {
                std::fs::write(state, "running").unwrap();
                println!("{target} running");
            }
        }
        "logs" if target.ends_with(":probe") => {}
        "logs" if target.ends_with(":space") => {
            if target.starts_with("pc:") {
                println!("100000000000");
            } else {
                println!("/dev/fixture 0 0 100000000 0% /fixture");
            }
        }
        "logs" if target.ends_with(":cleanup") => {}
        "logs" => report(target),
        other => panic!("unexpected request {other}"),
    }
}

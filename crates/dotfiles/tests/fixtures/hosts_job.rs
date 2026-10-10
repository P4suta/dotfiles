//! Completed jobs whose waiting client retains its SSH session until the fixture releases it.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

fn report() {
    println!(
        "DOTFILES_HOST_STAGES {}",
        std::env::var("HOSTS_JOB_STAGES").unwrap()
    );
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
        "on" => println!("{target}:probe"),
        "run" if arguments.iter().any(|argument| argument == "--wait") => {
            report();
            println!("{target}:gate finished {outcome}");
            std::io::stdout().flush().unwrap();
            let release = PathBuf::from(std::env::var_os("HOSTS_JOB_RELEASE").unwrap());
            while !release.exists() {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        "run" => println!("{target}:gate"),
        "status" if target.ends_with(":probe") => println!("{target} finished succeeded"),
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
        "logs" => report(),
        other => panic!("unexpected request {other}"),
    }
}

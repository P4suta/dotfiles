//! Git transport helpers for an unattended native fetch fixture.

use std::fs;
use std::io::{BufRead, Write};
use std::path::Path;

fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn main() {
    assert!(std::env::var_os("SSH_AUTH_SOCK").is_none());
    assert!(std::env::var_os("SSH_AGENT_PID").is_none());
    let executable = std::env::current_exe().unwrap();
    if executable.file_stem().unwrap() == "ssh" {
        fs::write(
            std::env::var("HOST_FETCH_LOG").unwrap(),
            "ssh without an agent",
        )
        .unwrap();
        std::process::exit(1);
    }
    fs::write(
        std::env::var("HOST_FETCH_LOG").unwrap(),
        "https without credentials",
    )
    .unwrap();
    let head = std::env::var("HOST_FETCH_HEAD").unwrap();
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        if line == "capabilities" {
            print!("fetch\noption\n\n");
        } else if line.starts_with("option ") {
            println!("ok");
        } else if line == "list" {
            print!("{head} HEAD\n{head} refs/heads/main\n\n");
        } else if line.starts_with("fetch ") {
            let directory = std::env::var("GIT_DIR").unwrap();
            copy(
                Path::new(&std::env::var("HOST_FETCH_OBJECTS").unwrap()),
                &Path::new(&directory).join("objects"),
            );
        } else if line.is_empty() {
            println!();
        } else {
            panic!("unexpected helper command: {line}");
        }
        std::io::stdout().flush().unwrap();
    }
}

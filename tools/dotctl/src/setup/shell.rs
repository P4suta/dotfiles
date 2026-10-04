use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

use super::scoop;
use crate::{env, proc};

const MISE_SESSION: &[&str] = &[
    "__MISE_DIFF",
    "__MISE_SESSION",
    "__MISE_ORIG_PATH",
    "MISE_SHELL",
];

pub fn run() -> Result<i32> {
    let dir = env::home_dir()
        .context("no home directory")?
        .join(".cache")
        .join("pwsh");
    std::fs::create_dir_all(&dir)?;

    let mut starship = scoop_command("starship");
    starship.args(["init", "powershell", "--print-full-init"]);
    let mut zoxide = scoop_command("zoxide");
    zoxide.args(["init", "powershell"]);
    let mut mise = env::command("mise");
    mise.args(["activate", "pwsh"])
        .env("PATH", without_mise(&env::augmented_path()));
    for name in MISE_SESSION {
        mise.env_remove(name);
    }

    let mut code = 0;
    for (name, cmd) in [
        ("starship", &mut starship),
        ("zoxide", &mut zoxide),
        ("mise", &mut mise),
    ] {
        if let Err(err) = cache(&dir.join(format!("{name}.ps1")), cmd) {
            eprintln!("{name}: {err:#}");
            code = 1;
        }
    }
    Ok(code)
}

fn scoop_command(name: &str) -> Command {
    let exe = scoop::current_exe(name);
    let mut cmd = Command::new(&exe);
    if let Some(dir) = exe.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        let mut path = dir.as_os_str().to_owned();
        path.push(";");
        path.push(env::augmented_path());
        cmd.env("PATH", path);
    }
    cmd
}

fn without_mise(path: &OsStr) -> OsString {
    std::env::join_paths(std::env::split_paths(path).filter(|dir| {
        let dir = dir.to_string_lossy().to_lowercase().replace('/', r"\");
        !dir.contains(r"\mise\shims") && !dir.contains(r"\mise\installs")
    }))
    .unwrap_or_default()
}

fn cache(path: &Path, cmd: &mut Command) -> Result<()> {
    let script = proc::capture_ok(cmd)?;
    std::fs::write(path, script)?;
    println!(">>> cached {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mise_entries_leave_the_path() {
        let path = OsString::from(
            r"C:\a;C:\Users\x\AppData\Local\mise\shims;C:\Users\x\AppData\Local\mise\installs\rg\1;C:\b",
        );
        assert_eq!(without_mise(&path), OsString::from(r"C:\a;C:\b"));
    }
}

//! Keeps `storage-scout watch` running from logon.
//!
//! storage-scout removes build waste the moment it appears: caches whose owner let go or whose work has landed, files rustc never reads again, and duplicate bytes.
//! It has no schedule, because the watcher reacts to filesystem events and released locks; a logon task that restarts it is all it needs.
//! `conhost --headless` keeps a console window from opening, and the watcher's lines go to the dotfiles log.
//! Stopping the task ends only conhost, so a restart also stops the watcher it started, which then rereads the policy.

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::{env, proc};

const TASK: &str = "storage-scout";

pub fn run() -> Result<i32> {
    let home = env::home_dir().context("no home directory")?;
    let exe = home.join(".cargo").join("bin").join("storage-scout.exe");
    let policy = home.join(".config").join("storage-scout").join("auto.toml");
    if !exe.is_file() {
        println!(">>> storage-scout is not installed; skipping its watcher");
        return Ok(0);
    }
    if !policy.is_file() {
        println!(">>> no storage-scout policy; skipping its watcher");
        return Ok(0);
    }
    let logs = home
        .join(".local")
        .join("state")
        .join("dotfiles")
        .join("log");
    std::fs::create_dir_all(&logs)?;
    let script = register(&exe, &policy, &logs.join("storage-scout.log"));
    let mut pwsh = env::command("pwsh");
    pwsh.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"])
        .arg(script);
    let code = proc::status(&mut pwsh)?.code();
    if code != 0 {
        bail!("registering the {TASK} task exited with {code}");
    }
    println!(">>> {TASK} watches from logon");
    Ok(0)
}

/// A PowerShell single-quoted literal: only a single quote needs escaping, by doubling it.
fn literal(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

fn register(exe: &Path, policy: &Path, log: &Path) -> String {
    let command = format!(
        "\"{}\" watch --config \"{}\" >> \"{}\" 2>&1",
        exe.display(),
        policy.display(),
        log.display()
    );
    let argument = format!("--headless cmd.exe /d /c \"{command}\"");
    [
        "$ErrorActionPreference = 'Stop'".to_owned(),
        format!(
            "$action = New-ScheduledTaskAction -Execute 'conhost.exe' -Argument {}",
            literal(&argument)
        ),
        "$trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME".to_owned(),
        "$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1) -MultipleInstances IgnoreNew -Priority 7".to_owned(),
        format!(
            "Register-ScheduledTask -TaskName {task} -Description 'storage-scout watch: remove build waste the moment it appears' -Action $action -Trigger $trigger -Settings $settings -Force | Out-Null",
            task = literal(TASK)
        ),
        format!("Stop-ScheduledTask -TaskName {}", literal(TASK)),
        "Get-Process -Name 'storage-scout' -ErrorAction SilentlyContinue | Stop-Process -Force".to_owned(),
        format!("Start-ScheduledTask -TaskName {}", literal(TASK)),
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quote_in_a_literal_is_doubled() {
        assert_eq!(literal("C:\\a"), "'C:\\a'");
        assert_eq!(literal("it's"), "'it''s'");
    }

    #[test]
    fn the_task_runs_the_watcher_headless_and_restarts_it() {
        let script = register(
            Path::new("C:\\home\\.cargo\\bin\\storage-scout.exe"),
            Path::new("C:\\home\\.config\\storage-scout\\auto.toml"),
            Path::new("C:\\home\\log\\storage-scout.log"),
        );
        assert!(script.contains(
            "-Argument '--headless cmd.exe /d /c \"\"C:\\home\\.cargo\\bin\\storage-scout.exe\" watch --config \"C:\\home\\.config\\storage-scout\\auto.toml\" >> \"C:\\home\\log\\storage-scout.log\" 2>&1\"'"
        ));
        assert!(script.contains("-AtLogOn -User $env:USERNAME"));
        assert!(script.contains("-RestartCount 999"));
        assert!(script.contains("Get-Process -Name 'storage-scout'"));
        assert!(script.ends_with("Start-ScheduledTask -TaskName 'storage-scout'"));
    }
}

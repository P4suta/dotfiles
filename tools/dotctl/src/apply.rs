use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::{env, proc};

const TARGETS: &[&str] = &[
    ".gitconfig",
    ".claude/settings.json",
    ".config/git/config",
    ".config/git/allowed_signers",
    ".config/starship.toml",
    ".config/wezterm/wezterm.lua",
    "Documents/PowerShell/Microsoft.PowerShell_profile.ps1",
];

const APPDATA_TARGETS: &[&str] = &["herdr/config.toml"];

#[derive(Debug)]
struct Backup {
    source: PathBuf,
    copy: PathBuf,
}

pub fn run(yes: bool, force: bool) -> Result<i32> {
    let root = backup_root()?;
    let backups = take_backups(&root)?;
    println!(
        ">>> backed up {} file(s) to {}",
        backups.len(),
        root.display()
    );

    println!();
    println!(">>> chezmoi diff");
    let mut diff = env::command("chezmoi");
    diff.arg("diff");
    proc::status(&mut diff)?;

    if !yes && !confirm()? {
        println!("aborted");
        return Ok(1);
    }

    println!();
    let mut apply = env::command("chezmoi");
    apply.arg("apply");
    if force {
        apply.arg("--force");
    }
    println!(">>> chezmoi apply{}", if force { " --force" } else { "" });
    let code = proc::status(&mut apply)?;

    if code != 0 {
        eprintln!(">>> chezmoi apply failed (exit {code}) - restoring backups");
        for backup in &backups {
            match std::fs::copy(&backup.copy, &backup.source) {
                Ok(_) => println!("    restored {}", backup.source.display()),
                Err(err) => eprintln!("    FAILED to restore {}: {err}", backup.source.display()),
            }
        }
        eprintln!(
            ">>> rollback complete. backup retained at {}",
            root.display()
        );
        return Ok(code);
    }

    println!();
    println!(">>> chezmoi verify");
    let mut verify = env::command("chezmoi");
    verify.arg("verify");
    proc::status(&mut verify)?;

    println!();
    println!(
        ">>> done. backup retained at {} (delete it once you are satisfied)",
        root.display()
    );
    Ok(0)
}

fn backup_root() -> Result<PathBuf> {
    let stamp = jiff::Zoned::now().strftime("%Y%m%d-%H%M%S").to_string();
    let root = std::env::temp_dir().join(format!("dotfiles-win-backup-{stamp}"));
    std::fs::create_dir_all(&root).with_context(|| format!("creating {}", root.display()))?;
    Ok(root)
}

fn take_backups(root: &Path) -> Result<Vec<Backup>> {
    let mut backups = Vec::new();
    for (base, relatives) in [(env::home_dir(), TARGETS), (appdata_dir(), APPDATA_TARGETS)] {
        let Some(base) = base else { continue };
        for relative in relatives {
            let source = base.join(relative);
            if !source.is_file() {
                continue;
            }
            let copy = root.join(relative);
            if let Some(parent) = copy.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&source, &copy)
                .with_context(|| format!("backing up {}", source.display()))?;
            backups.push(Backup { source, copy });
        }
    }
    Ok(backups)
}

fn appdata_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| env::home_dir().map(|home| home.join(".config")))
}

fn confirm() -> Result<bool> {
    println!();
    print!("apply? [y/N] ");
    std::io::stdout().flush()?;

    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer)? == 0 {
        return Ok(false);
    }
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

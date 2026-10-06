//! Writes `administrators_authorized_keys` and locks it down.
//!
//! Windows' stock `sshd_config` ends with `Match Group administrators`, which makes sshd read this file instead of `~/.ssh/authorized_keys` for every administrator account.
//! StrictModes refuses the file unless only SYSTEM and the Administrators group hold permissions on it.

use std::path::Path;

use anyhow::{Context, Result};

use crate::setup::sshd::config;
use crate::{env, proc};

/// `NT AUTHORITY\SYSTEM` and `BUILTIN\Administrators`.
/// SIDs rather than names, which a renamed or localized group would break.
const SYSTEM_SID: &str = "*S-1-5-18";
const ADMINISTRATORS_SID: &str = "*S-1-5-32-544";

pub fn authorize(ssh_dir: &Path, key: &str, sshd_config: &str) -> Result<()> {
    let path = ssh_dir.join("administrators_authorized_keys");

    if !config::has_admin_match_block(sshd_config) {
        eprintln!(
            "sshd_config has no `Match Group administrators` block, so this file may be ignored:"
        );
        eprintln!("  {}", path.display());
    }

    // Only a missing file starts empty, so an unreadable file never loses its other keys.
    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(error).with_context(|| format!("reading {}", path.display()));
        }
    };
    let wanted = merge(&existing, key);
    if wanted == existing {
        println!("    authorized key already present");
    } else {
        std::fs::write(&path, wanted.as_bytes())
            .with_context(|| format!("writing {}", path.display()))?;
        println!("    authorized key -> {}", path.display());
    }

    restrict(&path)
}

/// Keeps every other key and appends this one.
///
/// Compares only type and base64, because the comment changes on each 1Password rename.
fn merge(existing: &str, key: &str) -> String {
    let body = material(key);
    let mut lines: Vec<&str> = existing
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .filter(|line| material(line) != body)
        .collect();
    lines.push(key);
    format!("{}\r\n", lines.join("\r\n"))
}

/// The `<type> <base64>` half of an authorized_keys line, without the trailing comment.
fn material(line: &str) -> String {
    line.split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ")
}

/// `/inheritance:r` drops inherited ACEs, and `/grant:r` replaces instead of adding.
/// The owner needs a second icacls call, because it rejects `/setowner` together with `/grant`, with exit 87.
fn restrict(path: &Path) -> Result<()> {
    let mut acl = env::command("icacls.exe");
    acl.arg(path)
        .arg("/inheritance:r")
        .arg("/grant:r")
        .arg(format!("{SYSTEM_SID}:F"))
        .arg("/grant:r")
        .arg(format!("{ADMINISTRATORS_SID}:F"));
    proc::capture_ok(&mut acl)
        .with_context(|| format!("restricting the ACL on {}", path.display()))?;

    let mut owner = env::command("icacls.exe");
    owner.arg(path).arg("/setowner").arg(ADMINISTRATORS_SID);
    proc::capture_ok(&mut owner)
        .with_context(|| format!("setting the owner of {}", path.display()))?;

    println!("    ACL: SYSTEM and Administrators only, owned by Administrators");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIExample";

    #[test]
    fn an_empty_file_gets_the_key() {
        assert_eq!(merge("", KEY), format!("{KEY}\r\n"));
    }

    #[test]
    fn a_second_pass_changes_nothing() {
        let once = merge("", KEY);
        assert_eq!(merge(&once, KEY), once);
    }

    /// A different comment still names the same key.
    #[test]
    fn a_renamed_key_does_not_duplicate() {
        let existing = format!("{KEY} old-name\r\n");
        assert_eq!(
            merge(&existing, &format!("{KEY} new-name")),
            format!("{KEY} new-name\r\n")
        );
    }

    #[test]
    fn other_keys_survive() {
        let other = "ssh-rsa AAAAB3Nz someone-else";
        let merged = merge(&format!("{other}\r\n"), KEY);
        assert_eq!(merged, format!("{other}\r\n{KEY}\r\n"));
    }

    #[test]
    fn blank_lines_are_dropped() {
        let merged = merge("\r\n\r\n", KEY);
        assert_eq!(merged, format!("{KEY}\r\n"));
    }
}

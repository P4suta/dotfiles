use crate::runtime::{Runner, args, checked, replace};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const ARCHIVE: &str = "WezTerm-windows-nightly.zip";

fn checksum(text: &str) -> Result<String> {
    let parts: Vec<_> = text.split_whitespace().collect();
    ensure!(
        parts.len() == 2
            && parts[1] == ARCHIVE
            && parts[0].len() == 64
            && parts[0].bytes().all(|byte| byte.is_ascii_hexdigit()),
        "unexpected WezTerm checksum format"
    );
    Ok(parts[0].to_ascii_lowercase())
}

fn archive_paths(text: &str) -> Result<()> {
    ensure!(!text.trim().is_empty(), "empty WezTerm archive");
    for line in text.lines() {
        let path = line.trim_end_matches('/');
        ensure!(
            !path.contains('\\') && !path.contains(':') && !path.is_empty(),
            "unsafe archive entry"
        );
        crate::transaction::relative(Path::new(path))?;
    }
    Ok(())
}

pub fn wezterm(home: &Path, runner: &mut impl Runner) -> Result<()> {
    ensure!(
        cfg!(windows),
        "WezTerm installation requires native Windows"
    );
    let local = PathBuf::from(std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA is missing")?);
    let roaming = PathBuf::from(std::env::var_os("APPDATA").context("APPDATA is missing")?);
    let install = local.join("Programs/WezTerm-nightly");
    let shim = home.join(".local/bin/wezterm.cmd");
    let shortcut = roaming.join("Microsoft/Windows/Start Menu/Programs/WezTerm Nightly.lnk");
    if install.join("wezterm.exe").is_file() && shim.is_file() && shortcut.is_file() {
        return Ok(());
    }
    let scope = tempfile::tempdir()?;
    let zip = scope.path().join(ARCHIVE);
    let hashes = scope.path().join("checksum");
    for (name, path) in [
        (format!("{ARCHIVE}.sha256"), &hashes),
        (ARCHIVE.to_owned(), &zip),
    ] {
        checked(
            runner,
            Tool::CurlExe,
            &[
                "--proto".into(),
                "=https".into(),
                "--tlsv1.2".into(),
                "-fsSL".into(),
                "--max-time".into(),
                "300".into(),
                "-o".into(),
                path.into(),
                format!("https://github.com/wezterm/wezterm/releases/download/nightly/{name}")
                    .into(),
            ],
        )?;
    }
    let expected = checksum(&fs::read_to_string(hashes)?)?;
    let actual: String = Sha256::digest(fs::read(&zip)?)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    ensure!(actual == expected, "WezTerm SHA-256 verification failed");
    archive_paths(&String::from_utf8(checked(
        runner,
        Tool::TarExe,
        &["-tf".into(), zip.clone().into()],
    )?)?)?;
    let extract = scope.path().join("extract");
    fs::create_dir(&extract)?;
    checked(
        runner,
        Tool::TarExe,
        &[
            "-xf".into(),
            zip.into(),
            "-C".into(),
            extract.clone().into(),
        ],
    )?;
    let roots: Vec<_> = fs::read_dir(&extract)?.collect::<std::io::Result<_>>()?;
    ensure!(
        roots.len() == 1
            && roots[0].file_type()?.is_dir()
            && roots[0].path().join("wezterm.exe").is_file()
            && roots[0].path().join("wezterm-gui.exe").is_file(),
        "unexpected WezTerm archive layout"
    );
    fs::create_dir_all(install.parent().context("installation parent missing")?)?;
    let backup = tempfile::tempdir_in(install.parent().context("installation parent missing")?)?;
    let previous = backup.path().join("previous");
    let old_shim = fs::read(&shim).ok();
    let old_shortcut = fs::read(&shortcut).ok();
    let had_previous = install.exists();
    if had_previous {
        fs::rename(&install, &previous)?;
    }
    let result = (|| -> Result<()> {
        fs::rename(roots[0].path(), &install)?;
        replace(&install.join(".sha256"), expected.as_bytes(), false)?;
        replace(
            &shim,
            b"@echo off\r\n\"%LOCALAPPDATA%\\Programs\\WezTerm-nightly\\wezterm.exe\" %*\r\n",
            false,
        )?;
        fs::create_dir_all(shortcut.parent().context("shortcut parent missing")?)?;
        let script = scope.path().join("shortcut.ps1");
        fs::write(
            &script,
            "param([string]$Shortcut, [string]$Directory)\n$ErrorActionPreference = 'Stop'\n$link = (New-Object -ComObject WScript.Shell).CreateShortcut($Shortcut)\n$link.TargetPath = Join-Path $Directory 'wezterm-gui.exe'\n$link.WorkingDirectory = $Directory\n$link.IconLocation = $link.TargetPath + ',0'\n$link.Save()\n",
        )?;
        checked(
            runner,
            Tool::Pwsh,
            &[
                "-NoProfile".into(),
                "-File".into(),
                script.into(),
                shortcut.clone().into(),
                install.clone().into(),
            ],
        )?;
        ensure!(
            runner
                .run_installed(&install.join("wezterm.exe"), &args(&["--version"]))?
                .success,
            "installed WezTerm does not run"
        );
        Ok(())
    })();
    if let Err(error) = result {
        if install.exists() {
            fs::remove_dir_all(&install)?;
        }
        if had_previous {
            fs::rename(previous, &install)?;
        }
        for (path, before) in [(&shim, old_shim), (&shortcut, old_shortcut)] {
            if let Some(bytes) = before {
                replace(path, &bytes, false)?;
            } else if path.exists() {
                fs::remove_file(path)?;
            }
        }
        return Err(error).context("WezTerm installation failed; previous installation restored");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn moving_assets_require_the_exact_filename_and_safe_archive_paths() {
        assert!(checksum(&format!("{}  {ARCHIVE}\n", "ab".repeat(32))).is_ok());
        for text in ["bad hash", &format!("{}  other.zip", "ab".repeat(32))] {
            assert!(checksum(text).is_err());
        }
        assert!(archive_paths("wezterm/\nwezterm/wezterm.exe\n").is_ok());
        for text in ["../escape", "/escape", "C:/escape", "root\\escape", ""] {
            assert!(archive_paths(text).is_err());
        }
    }
}

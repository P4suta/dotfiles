//! The pinned `PSScriptAnalyzer` release that the Windows setup installs and every PowerShell gate loads.
//! A module whose analyzer assembly differs from the recorded digest fails, so a stray or newer installation never stands in for the pin.

use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// The `PSScriptAnalyzer` release every gate and the Windows setup load.
pub const VERSION: &str = "1.25.0";

/// The SHA-256 of the analyzer assembly that release ships.
pub const ASSEMBLY_SHA256: &str =
    "145ba1459672da808757b2bc6c6494926a13d92353ccd6a0354a0d5d8fe14a5f";

/// The analyzer assembly inside the module directory, whose digest identifies the release.
pub const ASSEMBLY: &str = "Microsoft.Windows.PowerShell.ScriptAnalyzer.dll";

/// The module name PowerShell imports.
pub const MODULE: &str = "PSScriptAnalyzer";

/// The directory that `Save-Module -Path base` writes the pinned release into.
#[must_use]
pub fn module_directory(base: &Path) -> PathBuf {
    base.join(MODULE).join(VERSION)
}

/// Saves an absent pinned release under `base` with `Save-Module -RequiredVersion`.
///
/// # Errors
///
/// Returns an error when pwsh can't start or `Save-Module` fails.
pub fn save(base: &Path) -> Result<()> {
    if module_directory(base).join(ASSEMBLY).is_file() {
        return Ok(());
    }
    std::fs::create_dir_all(base)?;
    let output = Tool::Pwsh
        .command()
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$ErrorActionPreference = 'Stop'; Save-Module -Name $env:DOTFILES_MODULE -RequiredVersion $env:DOTFILES_MODULE_VERSION -Path $env:DOTFILES_MODULE_BASE -Repository PSGallery -Force",
        ])
        .env("DOTFILES_MODULE", MODULE)
        .env("DOTFILES_MODULE_VERSION", VERSION)
        .env("DOTFILES_MODULE_BASE", base)
        .stdin(std::process::Stdio::null())
        .output()
        .context("start pwsh to save PSScriptAnalyzer")?;
    ensure!(
        output.status.success(),
        "Save-Module PSScriptAnalyzer {VERSION} failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

/// The module manifest of the pinned release under `base`, after its assembly matches the recorded digest.
///
/// # Errors
///
/// Returns an error for a missing release or an assembly digest mismatch.
pub fn verified(base: &Path) -> Result<PathBuf> {
    let directory = module_directory(base);
    let assembly = directory.join(ASSEMBLY);
    let bytes = std::fs::read(&assembly).with_context(|| {
        format!(
            "PSScriptAnalyzer {VERSION} is missing at {}",
            directory.display()
        )
    })?;
    let digest = crate::build_inputs::hex(&Sha256::digest(&bytes));
    ensure!(
        digest == ASSEMBLY_SHA256,
        "{} has SHA-256 {digest}, not the {ASSEMBLY_SHA256} of PSScriptAnalyzer {VERSION}; delete {} and save the pinned release again",
        assembly.display(),
        directory.display()
    );
    Ok(directory.join(format!("{MODULE}.psd1")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_or_mismatched_release_fails_instead_of_warning() {
        let base = tempfile::tempdir().unwrap();
        let missing = verified(base.path()).unwrap_err();
        assert!(format!("{missing:#}").contains("missing"), "{missing:#}");
        let directory = module_directory(base.path());
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join(ASSEMBLY), b"another release").unwrap();
        let mismatched = verified(base.path()).unwrap_err();
        assert!(
            format!("{mismatched:#}").contains(ASSEMBLY_SHA256),
            "{mismatched:#}"
        );
    }
}

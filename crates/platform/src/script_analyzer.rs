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
    "c377532a1d8d8975e598891602b5c3ed39ab719f589d054eb889d11eb6ccaea3";

/// The analyzer assembly inside the module directory, whose digest identifies the release.
pub const ASSEMBLY: &str = "PSv7/Microsoft.Windows.PowerShell.ScriptAnalyzer.dll";

/// The module name PowerShell imports.
pub const MODULE: &str = "PSScriptAnalyzer";

/// Every entry point and dependency that PowerShell 7 loads from the release.
const RELEASE_FILES: [(&str, &str); 8] = [
    (ASSEMBLY, ASSEMBLY_SHA256),
    (
        "PSScriptAnalyzer.psd1",
        "2b219f688bcdd67101040f845e530b22907d39bbf49089b4b5b2afaba7996791",
    ),
    (
        "PSScriptAnalyzer.psm1",
        "b69773fc9080cfb958dc13c95e42a625553e9bca5dcbedc27231e5c183a4e869",
    ),
    (
        "ScriptAnalyzer.types.ps1xml",
        "34246566553fe97c96a9ffea8aeefee353c1e07e41a72d3bfe9fdf826cc71d2a",
    ),
    (
        "ScriptAnalyzer.format.ps1xml",
        "d647e6c1cdfe4052d2382f2d76b899ec596ec71d23d724553b93f74827d49044",
    ),
    (
        "PSv7/Microsoft.PowerShell.CrossCompatibility.dll",
        "d92cb413e18927bb64d9bac221191fc69cf1659f55c72c27ddf1fe19ef774caf",
    ),
    (
        "PSv7/Microsoft.Windows.PowerShell.ScriptAnalyzer.BuiltinRules.dll",
        "c8aa9890743a6d9c47ccfee5ec33caaee58c30591454d3ccf379720025a1533c",
    ),
    (
        "PSv7/Pluralize.NET.dll",
        "eeecdf663a3e854f67bbbb872dc45577e785a581c1ca86ea2ae1d8215b8505d6",
    ),
];

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
    let cache = crate::process::PowerShellCache::new()?;
    let mut command = Tool::Pwsh.command();
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$ErrorActionPreference = 'Stop'; Import-Module PowerShellGet -ErrorAction Stop; Save-Module -Name $env:DOTFILES_MODULE -RequiredVersion $env:DOTFILES_MODULE_VERSION -Path $env:DOTFILES_MODULE_BASE -Repository PSGallery -Force -Verbose",
        ])
        .env("DOTFILES_MODULE", MODULE)
        .env("DOTFILES_MODULE_VERSION", VERSION)
        .env("DOTFILES_MODULE_BASE", base)
        .stdin(std::process::Stdio::null());
    let output = cache
        .apply(&mut command)
        .output()
        .context("start pwsh to save PSScriptAnalyzer")?;
    ensure!(
        output.status.success(),
        "Save-Module PSScriptAnalyzer {VERSION} failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout).trim(),
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
    for (name, expected) in RELEASE_FILES {
        let file = directory.join(name);
        let bytes = std::fs::read(&file).with_context(|| {
            format!(
                "PSScriptAnalyzer {VERSION} is missing at {}",
                directory.display()
            )
        })?;
        let digest = crate::build_inputs::hex(&Sha256::digest(&bytes));
        ensure!(
            digest == expected,
            "{} has SHA-256 {digest}, not the {expected} of PSScriptAnalyzer {VERSION}; delete {} and save the pinned release again",
            file.display(),
            directory.display()
        );
    }
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
        std::fs::create_dir_all(directory.join("PSv7")).unwrap();
        std::fs::write(directory.join(ASSEMBLY), b"another release").unwrap();
        let mismatched = verified(base.path()).unwrap_err();
        assert!(
            format!("{mismatched:#}").contains(ASSEMBLY_SHA256),
            "{mismatched:#}"
        );
    }
}
